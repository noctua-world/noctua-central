//! Getting a line to the central log store, and never being the reason
//! something else stopped.
//!
//! `NOCTUA.md §18.5`: the store is self-hosted, on the same box, and takes
//! structured JSON lines over HTTP. So this speaks to one address, over plain
//! HTTP/1.1, with no TLS, no redirects and no authentication — the simplest
//! client that exists, written here rather than taken as a dependency on the
//! path every log line travels (`§2.3`).
//!
//! # Three rules, and each is about what happens when something is wrong
//!
//! **A full queue drops.** The channel is bounded and the send never blocks. A
//! log store that fell behind must not be able to slow the platform down, and a
//! queue that grows without bound is a memory leak with a delay fuse.
//!
//! **An unreachable store falls back to stderr.** The line still exists
//! somewhere a person can read it. `§18.5`'s own reason for self-hosting is that
//! there is nowhere else for the data to go; that argument only holds if the
//! failure path also keeps it here.
//!
//! **Drops are counted and reported.** A shipper that silently discarded would
//! be the `§19.3` failure in the one component whose job is telling you what
//! happened. The count is written to stderr with the batch that noticed it. It does **not**
//! reach the store as a `log.dropped` event: that event is declared in the
//! registry and constructed nowhere, and saying it is sent would describe a
//! wire that does not exist. Whoever adds the wire deletes this sentence.

use std::io::{Read as _, Write as _};
use std::net::{TcpStream, ToSocketAddrs as _};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{
    Receiver, RecvTimeoutError, SyncSender, TryRecvError, TrySendError, sync_channel,
};

/// How long to wait for the store to accept a connection.
///
/// The timeouts set after `connect` do not apply to `connect` itself, and a
/// connect to an address that drops packets does not return until the OS gives
/// up on its SYN retries — minutes, on the one thread draining the queue.
const CONNECT_TIMEOUT: core::time::Duration = core::time::Duration::from_secs(2);

/// How much of the store's answer is read before giving up on finding a newline.
///
/// A status line is under a hundred bytes. A read timeout does not bound this on
/// its own: a server answering steadily with no newline never trips one, because
/// every read returns data.
const STATUS_LIMIT: u64 = 8 * 1024;

/// How long one `post` may take, from connect to answer.
///
/// **A per-read timeout is not a bound on the whole exchange.** It bounds each
/// read, and a store trickling one byte every few seconds never trips it:
/// measured, a server writing a byte every 200 ms held `post` for 13 seconds
/// over forty bytes, and the worst case at `STATUS_LIMIT` is hours — on the one
/// thread draining the queue, with every line dropped meanwhile and the stderr
/// fallback that is `§18.5`'s whole argument never running.
///
/// **Ten seconds and not two, because the loss is not set here.** Shortening it
/// looks as though it cuts the loss — a hung store means `BATCH / DEADLINE`
/// lines a second — and it does not, because the loss is set by `QUEUE`, which
/// a busy service fills in milliseconds. Measured against one fixture, a store
/// that accepts and never answers, over ten seconds:
///
/// ```text
/// DEADLINE = 10s   the store read 116 line(s), 19 884 of 20 000 dropped
/// DEADLINE =  2s   the store read 360 line(s), 19 640 of 20 000 dropped
/// ```
///
/// Delivery triples; loss falls by 1.2 points. What the short deadline costs is
/// paid by stores that *work*: one answering anywhere between two and ten
/// seconds goes from `Taken` to `Silent`, so every batch it really did take is
/// written to stderr as well — a hundred per cent duplication for any store
/// slower than two seconds — and a refusal whose status line arrives in two
/// pieces more than two seconds apart becomes `Unreadable`.
///
/// So this is ten, and what bounds what a process waits for at exit is
/// [`FLUSH`].
const DEADLINE: core::time::Duration = core::time::Duration::from_secs(10);

/// How many lines may be waiting before new ones are dropped.
///
/// One thousand is a second or two of a busy service, which is long enough to
/// ride out a restart of the store and short enough that the memory is
/// bounded at something a person can name.
pub(crate) const QUEUE: usize = 1_000;

/// How many lines are sent in one request.
///
/// **A paragraph argues for it, and
/// `every_timing_bound_is_the_number_its_paragraph_argues_for` holds it to the
/// paragraph.** Without that pin, `BATCH = 8` leaves `cargo test -q -p nc-log`
/// green and `check passed` — eight times the requests, eight times the
/// connections and eight times the `DEADLINE` exposures for one full queue,
/// with `a_shutdown_stays_inside_its_own_budget` still inside its bound because
/// the bound is a wall-clock one. (`BATCH = 1000` fails even without it: one
/// batch then swallows the whole queue and the duplication ceiling below stops
/// meaning anything.)
///
/// So the argument, and it is [`QUEUE`] and [`DEADLINE`] together: a full
/// queue is `QUEUE / BATCH` requests, and at sixty-four that is sixteen —
/// few enough that a flush bounded at `FLUSH` gets through a useful part of
/// them, and many enough that one uncertain batch in flight at a shutdown is
/// a small fraction of the queue rather than most of it. That fraction is
/// what `lines_the_flush_runs_out_of_time_for_are_counted_and_kept` allows
/// for as `total + BATCH`, so the constant growing loosens that ceiling and
/// has to be a decision rather than an edit.
pub(crate) const BATCH: usize = 64;

/// Where the shipper posts, when nothing says otherwise.
///
/// `pub(crate)`, like the rest of this module: as `pub` it would hand a
/// consumer the store's address to write to directly.
#[expect(
    dead_code,
    reason = "the default the app stage will pass to `Log::open`"
)]
pub(crate) const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:9428/insert/jsonline";

/// A handle the log sends lines through.
///
/// **Not a public door.** With `Shipper::start` and `Shipper::send` `pub` —
/// and `send` takes a raw `String` — three lines from a consumer crate put
/// arbitrary run-time text into the central log store past every wall, and an
/// embedded newline forges a second record, defeating the
/// property `a_line_is_one_line` exists to guarantee. `DEFAULT_ENDPOINT`
/// is `pub(crate)` for the same reason.
///
/// The compile-fail fixtures in `crates/nc-log/fixtures`, a
/// sealed `Event`, `#[non_exhaustive]` on every `Field` variant and a
/// const-evaluation door all guard `Log::record`. This is the gate beside them.
///
/// **How many fixtures is read off a run rather than written here.** `check`
/// prints the number
/// in `ok N fixture(s) refuse to compile, for the stated reason`,
/// and `xtask/src/counts.rs` holds a stated count in a *document* to the tree
/// and excludes Rust sources by design — so a number written in this sentence
/// would drift with nothing to say so, and a reader counting the wall from here
/// would find doors it does not mention.
///
/// **Not `Clone`.** A second handle would mean a second `Drop`, and the first
/// one to run would close the channel while the other still held a sender — so
/// the flush would wait on a queue nothing was draining. One shipper, one
/// lifetime.
#[derive(Debug)]
pub(crate) struct Shipper {
    /// `None` only while [`Shipper::drop`] is closing it.
    ///
    /// The sender has to be dropped *before* the thread is joined: `recv` ends
    /// when the last sender is gone, and joining first would wait for a loop
    /// that is still waiting for a line.
    lines: Option<SyncSender<String>>,
    dropped: Arc<AtomicU64>,
    /// How many lines have been accepted into the queue.
    ///
    /// With [`Self::handled`], this is what makes a line abandoned at exit
    /// *countable*. `dropped()` counts a full queue and nothing else, so
    /// without the pair a flush that runs out of time reports no number at all
    /// — a message saying "lines queued at exit were not sent" leaves a reader
    /// to guess whether that was one line or a thousand.
    queued: Arc<AtomicU64>,
    /// How many lines the thread has finished with, sent or written to stderr.
    handled: Arc<AtomicU64>,
    /// Set when [`FLUSH`] runs out, asking the thread to stop using the network
    /// and write what is left where a person can read it.
    abandoning: Arc<AtomicBool>,
    /// Answers once the shipper's thread has drained the queue and stopped.
    finished: Receiver<()>,
    /// Set before the channel is closed, so the thread learns it at once.
    shutting_down: Arc<AtomicBool>,
}

/// How long [`Shipper::drop`] waits for the queue to drain.
///
/// **A process that returns from `main` keeps what is still queued because of
/// this wait.** With the thread detached and its handle discarded, the last
/// thing a program says goes with it — measured against a healthy store on
/// loopback: one `journal.broken` line recorded and returned from, **nothing
/// delivered**, and `dropped()` still zero because it is not a
/// queue-drop. `ship`'s three stated rules all hold and none of them covers
/// this.
///
/// Longer than one `DEADLINE` so a store that is merely slow still gets its
/// batch, and short enough that a store that is *gone* does not hold a shutdown
/// open: `post` is itself bounded by `DEADLINE`, so this is a small number of
/// attempts and not an unbounded wait.
///
/// **And it is what a process pays to exit**, which is why it is twelve and not
/// four. Four is not longer than one `DEADLINE`, which the sentence above
/// requires — so at four `FLUSH` silently becomes the deadline for the last
/// batch of every CLI run and every restart. Measured, a store answering after
/// five seconds: `drop 4058 ms`, the line written to stderr *and* taken by the
/// store, `duplicated 1`. That is the duplication `DEADLINE`'s own doc
/// condemns, produced by the constant meant to bound shutdown.
///
/// Twelve is longer than one `DEADLINE` with room for the answer to arrive, and
/// the one stderr writer thread is what bounds the part that is really
/// unbounded: the write to a pipe nobody is draining, which can hold exit past
/// thirty seconds while `FLUSH` and `GRACE` look like they are in charge.
///
/// **One shipper's whole shutdown is `FLUSH + GRACE` of network and one `GRACE`
/// of stderr — eighteen seconds — and it is one budget, not one per wait.** Per
/// *shipper*: `Log::open` is public and `Drop` budgets the shipper it is
/// dropping, so a process holding two pays it twice — measured, 24.14 s for two
/// against a mute store. Nothing here can bound a process it does not know the
/// shape of; what this number is, and all it is, is the cost of one. Five
/// independently bounded waits in sequence make an exit a doc bounds at fifteen
/// seconds measure **24.0 s** with an undrained stderr and 18.1 s with a hung
/// store, so the network waits share one deadline and each is clamped to what
/// is left of it; the stderr flush is a phase of its own, because clamping it
/// to the remainder makes it zero exactly when there is something to flush — 80
/// of 1000 lines in neither place, measured. Eighteen is the number, it is
/// stated here, and `every_timing_bound_is_the_number_its_paragraph_argues_for`
/// holds it — `a_shutdown_stays_inside_its_own_budget` cannot, because it
/// builds its bound out of these same constants and every value satisfies it.
const FLUSH: core::time::Duration = core::time::Duration::from_secs(12);

/// How long the shipper is given to write the rest to stderr once [`FLUSH`] runs out.
///
/// **The second phase, and the reason there is one.** With a healthy store
/// taking about a second per batch, a full queue at exit loses 47 of 1000 lines
/// without it: `FLUSH` bounds the flush at twelve seconds, and lines past that
/// are dropped with `dropped()` still reading zero and the message carrying no
/// number. Any store slower than `FLUSH / (QUEUE / BATCH)` — 0.768s a batch —
/// reaches it.
///
/// So a timeout is not the end: it sets a flag the thread reads, and the thread
/// writes what is left to stderr, which is where this crate's other failure
/// path already writes. Short, because it is not doing any network work — it is
/// a loop over strings and a locked writer.
const GRACE: core::time::Duration = core::time::Duration::from_secs(3);

/// How many posts in a row may fail during a shutdown before the rest go to stderr.
///
/// One is too few: it cannot tell a store that has gone from a store that
/// hiccuped, and a `bool` latch here sends 236 of 300 lines to stderr against a
/// store that answers every subsequent request. Two bounds a really-gone store
/// at two `DEADLINE`s rather than one per remaining batch.
const GIVE_UP_AFTER: usize = 2;

/// The whole of a shutdown's network budget, in one place a test can name.
///
/// **The constants are asserted, and so is the call
/// site.** `every_timing_bound_is_the_number_its_paragraph_argues_for`
/// pins `FLUSH`, `GRACE` and their sum, and with only those pinned a deadline
/// written out of the two at `Drop`'s call site would be read by nothing.
/// Measured that way, `Instant::now() + FLUSH + GRACE * 10` — a budget of 42 s
/// against the 15 s `FLUSH`'s own paragraph states: `cargo test -q -p nc-log`
/// green and `cargo run --quiet -p xtask -- check` at `check passed`.
/// `a_shutdown_stays_inside_its_own_budget` cannot see the widening, because
/// in its fixture the second wait returns as soon as the abandon flag reaches
/// the drain thread and the bound is never spent. `Log::open` is public, so
/// the cost is per shipper.
///
/// This is the shape this file records one module down for `CONNECT_TIMEOUT`: a
/// constant asserted and a call site not.
const fn the_network_budget_of_a_shutdown() -> core::time::Duration {
    FLUSH.saturating_add(GRACE)
}

/// How an interim answer says so to the loop that reads past it.
///
/// A prefix on the `Unreadable` text rather than a variant of its own: it never
/// escapes `answer`, so a fifth `Delivery` for it would be a case every match in
/// this file has to carry and no caller can ever see.
const INTERIM: &str = "the store answered the interim ";

/// How long an answer is waited for once the shutdown has given up on the store.
///
/// `DEADLINE` is ten seconds and `GRACE` is three, so an exchange that began
/// before the flush ran out could still be waiting when the grace period ended —
/// and the lines the grace period exists to write went nowhere. Short, because
/// by this point the process is leaving and the batch in flight has already been
/// written to the socket: what is being waited for is only the answer.
const ABANDONED_ANSWER: core::time::Duration = core::time::Duration::from_millis(200);

/// How long one read may block before the shutdown flag is looked at again.
///
/// The budget is recomputed *between* reads, so a read that blocks for the whole
/// of it hides the flag for the whole of it — which is what a store that sends
/// nothing does. Short enough that `ABANDONED_ANSWER` means what it says, long
/// enough that an ordinary exchange costs a handful of extra syscalls.
const POLL: core::time::Duration = core::time::Duration::from_millis(100);

/// How long one write to the store may take.
///
/// A store that accepted the connection and then stopped reading would
/// otherwise hold this thread forever, and the thread is the only one draining
/// the queue. Named rather than written twice inline, because it is one of the
/// three bounds a shutdown shortens.
const WRITE_TIMEOUT: core::time::Duration = core::time::Duration::from_secs(5);

impl Shipper {
    /// Starts a shipper posting to `endpoint`.
    ///
    /// # Errors
    ///
    /// Returns the endpoint when it is not an `http://host:port/path` this can
    /// speak to. There is deliberately no TLS: the store is on the same machine
    /// (`§18.5`), and an endpoint that needed TLS would be one this is not the
    /// right client for.
    pub(crate) fn start(endpoint: &str) -> Result<Self, String> {
        Self::to(Target::parse(endpoint)?)
    }

    /// The same shipper, aimed at a target that did not come from an endpoint.
    ///
    /// **For the two tests that need a host `parse` refuses.** `Target::parse`
    /// speaks to this machine only (`§18.5`), and the connect bound is about a
    /// host that *black-holes* packets — which loopback cannot be: a closed
    /// loopback port refuses instantly, so the drain keeps up and nothing is
    /// measured. TEST-NET-1 is what those tests need and what production must
    /// not have, so the difference is a constructor rather than a hole in the
    /// rule.
    fn to(target: Target) -> Result<Self, String> {
        let (lines, waiting) = sync_channel::<String>(QUEUE);
        let (done, finished) = sync_channel::<()>(1);
        let dropped = Arc::new(AtomicU64::new(0));
        let counter = Arc::clone(&dropped);
        let shutting_down = Arc::new(AtomicBool::new(false));
        let closing = Arc::clone(&shutting_down);
        let queued = Arc::new(AtomicU64::new(0));
        let handled = Arc::new(AtomicU64::new(0));
        let handled_by_thread = Arc::clone(&handled);
        let abandoning = Arc::new(AtomicBool::new(false));
        let abandon_flag = Arc::clone(&abandoning);
        std::thread::Builder::new()
            .name("nc-log-shipper".to_owned())
            .spawn(move || {
                let handled = handled_by_thread;
                let abandoning = abandon_flag;
                let mut batch = Vec::with_capacity(BATCH);
                // Set by `drop` before it closes the channel. The receiver
                // cannot answer this on its own: `try_recv` reports
                // `Disconnected` only once the *buffer* is also empty, so with a
                // thousand lines queued the thread would not learn it was
                // shutting down until it had tried the network forty-six more
                // times.
                // **How many posts in a row have failed during a shutdown.**
                // Retrying a store that is not there costs one `DEADLINE` per
                // batch, and a shutdown holding that open for every batch left
                // in the queue is a process that will not exit — measured at
                // fifteen seconds for one test's three thousand queued lines.
                // Within a running program the store may come back, so this is
                // only ever incremented while closing.
                //
                // **Consecutive, and not a latch.** A `bool` set by the first
                // failure cannot tell "the store is gone" from "the store
                // hiccuped once": measured, 300 lines queued with only the
                // *first* exchange failing, and the store receives 64 — the
                // other 236 go to stderr and are never retried, against a store
                // that answers every request after that one. A count resets on
                // success, so one bad exchange costs one retry and a store that
                // is really gone still costs at most `GIVE_UP_AFTER` deadlines.
                let mut failures = 0_usize;
                while let Ok(line) = waiting.recv() {
                    batch.push(line);
                    // Drain whatever else is already waiting, so a burst becomes
                    // one request rather than sixty-four.
                    while batch.len() < BATCH {
                        match waiting.try_recv() {
                            Ok(next) => batch.push(next),
                            // Empty or gone: either way there is nothing more
                            // to add to this batch. `drop` sets its own flag, so
                            // nothing here has to tell the two apart.
                            Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
                        }
                    }
                    // Either the store has failed `GIVE_UP_AFTER` times in a
                    // row during this shutdown, or `drop`'s wait ran out and it
                    // asked for whatever is left to go somewhere a person can
                    // read it. Both mean: stop trying the network, keep the
                    // lines.
                    if failures >= GIVE_UP_AFTER || abandoning.load(Ordering::Relaxed) {
                        for line in &batch {
                            to_stderr_line(format!("nc-log: {line}\n"));
                        }
                        handled.fetch_add(batch.len() as u64, Ordering::Relaxed);
                        batch.clear();
                        continue;
                    }
                    let outcome = target.post(&batch, &abandoning);
                    if let Some(why) = outcome.why() {
                        // **Only a store that never received the batch counts
                        // against it.** A refusal and an unreadable answer both
                        // mean the store is there, and concluding "gone" from
                        // either sends 236 lines to stderr during a shutdown
                        // while the store keeps answering.
                        if outcome.store_answered() {
                            failures = 0;
                        } else if closing.load(Ordering::Relaxed) {
                            failures += 1;
                        }
                        // The lines still exist somewhere a person can read
                        // them, which is the whole point of keeping the data on
                        // this machine — unless the store may already hold them,
                        // in which case writing them here duplicates them.
                        if outcome.keep_on_stderr() {
                            for line in &batch {
                                to_stderr_line(format!("nc-log: {line}\n"));
                            }
                            // **The note is inside the `if`.** It says *"so
                            // they are here too"*, which is true only of the
                            // lines the branch above just wrote.
                            // `keep_on_stderr()` is `true` for every outcome
                            // `why()` answers `Some` for — the two are the same
                            // predicate today, which
                            // `every_delivery_keeps_its_lines_exactly_when_it_has_a_reason`
                            // states — so the placement moves nothing now and
                            // keeps the sentence true the day a fourth verdict
                            // makes them differ.
                            //
                            // **Bounded, like the lines above it.** A bare
                            // `to_stderr` here is an unbounded `write_all`
                            // under the process-wide stderr lock, on the one
                            // thread that drains the queue. A pipe nobody is
                            // reading blocks it for ever, and every line after
                            // it is then accepted into a queue nothing empties:
                            // measured against an undrained stderr, 830 of 3000
                            // accepted lines reach neither the store nor
                            // stderr, and `dropped()` counts only the 1616
                            // refused at `try_send`. The lines go through
                            // `to_stderr_line`, and so do the two notes beside
                            // them, because one blocked note is the same
                            // outage.
                            //
                            // **"may already hold", because it may.** A store that
                            // answers slower than `DEADLINE` receives the batch and
                            // is reported as not having taken it — measured, a store
                            // answering at 11 s: all 64 lines in the store *and* all
                            // 64 on stderr, with this sentence above them. The
                            // duplication is the deliberate trade (`Delivery`'s own
                            // doc: a duplicate is recoverable and a loss is not);
                            // what would not be is telling a reader the store
                            // did not take lines it did.
                            to_stderr_line(format!(
                                "nc-log: the store did not confirm {} line(s), so they are \
                                 here too and may be duplicates: {why}\n",
                                batch.len()
                            ));
                        }
                    } else {
                        failures = 0;
                    }
                    handled.fetch_add(batch.len() as u64, Ordering::Relaxed);
                    if let Some(said) = what_the_queue_refused(counter.swap(0, Ordering::Relaxed)) {
                        to_stderr_line(said);
                    }
                    batch.clear();
                }
                // The queue is drained and the last sender is gone. Saying so is
                // what lets `drop` wait for exactly this rather than for a
                // guess.
                let _ = done.send(());
            })
            .map_err(|error| format!("the shipper's thread would not start: {error}"))?;
        Ok(Self {
            lines: Some(lines),
            dropped,
            queued,
            handled,
            abandoning,
            finished,
            shutting_down,
        })
    }
}

/// What a shutdown says about the lines it never got to.
///
/// **The one sentence in this crate that reports an outright loss**, and at its
/// own call site no fixture reaches it: measured without a test that reads the
/// source, the whole `match self.finished.recv_timeout(left()) { … }` replaced
/// by `let _ = self.finished.recv_timeout(left());`
/// leaves `cargo test -q -p nc-log` green and `check passed`. Those lines are
/// absent from `LOST` as well — `Log::lost()` is documented as "how many lines
/// reached neither the store nor stderr", and `flush_stderr` charges its own
/// leftovers to `LOST` for exactly that reason one queue down.
///
/// **Why no fixture reaches it, written down rather than implied.** The second
/// wait ends when the drain thread stops, and the abandon flag exists to make
/// the drain thread stop: once it is set, the loop takes the give-up branch
/// before any post, writes what is left through the bounded handover and ends
/// within milliseconds. Reaching this sentence means the flag did not do that,
/// which is the state it is written for and not a state a test can build
/// without breaking the flag. So the sentence is a function driven by a test,
/// and `the_last_two_sentences_of_a_shutdown_are_still_said` holds the two
/// call sites.
fn the_lines_still_queued_at_the_end(remaining: u64) -> String {
    format!("nc-log: {remaining} line(s) were still queued after {GRACE:?} and are lost\n")
}

/// What a queue that refused lines says, or nothing when it refused none.
///
/// **One sentence for two arms, so a weakening of it fails where it is
/// held.** The drain thread says this after every batch and `Drop` says it for
/// the drops that outlive the last one. No fixture reaches `Drop`'s arm, so a
/// guard there that is always false — `if lost > 0 && self.lines.is_some()`,
/// with `self.lines` already taken — changes no behaviour a test can see,
/// which is why the call site `Drop` makes is held by its source, in
/// `the_last_two_sentences_of_a_shutdown_are_still_said`. The drain thread's
/// arm is genuinely held (`counter.swap(0, …) * 0` there is RED), so writing
/// the sentence once means a weakening of it fails at that arm.
fn what_the_queue_refused(lost: u64) -> Option<String> {
    (lost > 0).then(|| format!("nc-log: {lost} line(s) were dropped because the queue was full\n"))
}

impl Drop for Shipper {
    // Drains the queue before the process goes.
    //
    // **Without it, the lines lost are the ones worth keeping.** `Log::record`
    // is fire-and-forget into a bounded channel drained by a separate thread;
    // with no `Drop`, no `flush` and the `JoinHandle` discarded, a program that
    // recorded one `journal.broken` line and returned from `main` delivers
    // **nothing**, to a healthy store on loopback, every time — and `dropped()`
    // stays zero, because a line the process never got round to sending is not
    // a queue-drop.
    //
    // Dropping the sender is what ends the thread's `recv` loop; joining first
    // would wait for a loop still waiting for a line. The wait is bounded so a
    // store that is *gone* cannot hold a shutdown open, and a wait that runs out
    // says how it ended — on stderr, which is where this crate's other failure
    // path already writes.
    fn drop(&mut self) {
        // **One budget for the whole shutdown, not one per wait.** Five
        // independently bounded waits in sequence — `FLUSH`, then four `GRACE`s
        // — make the exit a doc bounds at `FLUSH + GRACE` measure **24.0 s**
        // against a stated 15, and 18.1 s in the ordinary
        // full-queue-and-hung-store case. Every wait below asks how much of the
        // budget is left, so the number in the doc is the number a process
        // pays.
        let deadline = std::time::Instant::now() + the_network_budget_of_a_shutdown();
        let left = move || deadline.saturating_duration_since(std::time::Instant::now());
        // **The flag before the channel.** `try_recv` reports `Disconnected`
        // only once the buffer is also empty, so a thousand queued lines would
        // otherwise be forty-six more attempts at a store that is not there —
        // one `CONNECT_TIMEOUT` each, and the whole flush capped by `FLUSH`
        // rather than finishing.
        self.shutting_down.store(true, Ordering::Relaxed);
        drop(self.lines.take());
        match self.finished.recv_timeout(FLUSH.min(left())) {
            // Finished, or gone without answering — which it only is if it
            // panicked, and there is nothing left to wait for either way.
            Ok(()) | Err(RecvTimeoutError::Disconnected) => {}
            Err(RecvTimeoutError::Timeout) => {
                // **A timeout is not the end.** Measured against a healthy
                // store taking about a second a batch, a full queue at exit
                // loses 47 of 1000 lines when it is: `FLUSH` bounds the flush
                // at twelve seconds and the rest go nowhere, with `dropped()`
                // reading zero and the message carrying no number.
                //
                // So the wait ending asks the thread to stop using the network
                // and write what is left to stderr, which is where this crate's
                // other failure path already writes, and gives it `GRACE` to do
                // it. Then it says how many lines that was, or how many are
                // still unaccounted for — a message a reader can act on rather
                // than one they have to guess at.
                let remaining = self.still_waiting();
                self.abandoning.store(true, Ordering::Relaxed);
                to_stderr_line(format!(
                    "nc-log: the store did not answer for {remaining} line(s) within \
                     {FLUSH:?}; writing them here instead, and any the store took after \
                     all are duplicates rather than losses\n"
                ));
                match self.finished.recv_timeout(left()) {
                    Ok(()) | Err(RecvTimeoutError::Disconnected) => {}
                    Err(RecvTimeoutError::Timeout) => {
                        to_stderr_line(the_lines_still_queued_at_the_end(self.still_waiting()));
                    }
                }
            }
        }
        if let Some(said) = what_the_queue_refused(self.dropped.swap(0, Ordering::Relaxed)) {
            to_stderr_line(said);
        }
        // **And one wait for stderr, at the end, inside the same budget.** The
        // drain thread never waits on it; this is where a process pays for the
        // lines still queued, and it is bounded like everything else here.
        // **Its own phase, not the remainder of the network's.** Clamped to
        // what is left of the budget above, this waits zero whenever the
        // network has used it all — and the lines the network phase has just
        // handed to stderr are still in the queue when the process ends:
        // measured,
        // `41 reached the store and 879 reached stderr, of 1000 — 80 line(s) reached neither`.
        // So the whole shutdown is `FLUSH + GRACE` of network and one `GRACE`
        // of stderr, which is the number `FLUSH`'s doc states and a test
        // asserts.
        flush_stderr(GRACE);
    }
}

impl Shipper {
    /// Queues one line, which must already be one line.
    ///
    /// **Debug-asserted, because the type system cannot say it.** The only
    /// caller is `Log::line`, which builds a canonical JSON document —
    /// and `nc-canon` escapes every C0 character, so what arrives here holds no
    /// newline. A public `send` would let a caller hand it anything, and an
    /// embedded newline forges a second record in the store: `post` writes the
    /// line and then a `\n`, so two lines in mean two records out. The door is
    /// closed; this is what says so to whoever opens it next.
    ///
    /// Never blocks, and never fails.
    ///
    /// A full queue drops the line and counts it. Returning an error here would
    /// put a `?` on every logging call site, and a logging call that a caller
    /// has to handle is a logging call somebody removes.
    pub(crate) fn send(&self, line: String) {
        debug_assert!(
            !line.contains(['\n', '\r']),
            "a line holding a line break forges a second record in the store"
        );
        // `Some` for the whole of a shipper's usable life: `drop` is the only
        // thing that takes it, and nothing can send afterwards.
        let Some(lines) = self.lines.as_ref() else {
            return;
        };
        match lines.try_send(line) {
            Ok(()) => {
                self.queued.fetch_add(1, Ordering::Relaxed);
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// How many accepted lines the thread has not finished with.
    ///
    /// Two relaxed loads and no snapshot, so this is approximate while lines are
    /// moving. It is read at exit, after the sender is closed and the thread has
    /// either finished or been given up on, where nothing is moving.
    fn still_waiting(&self) -> u64 {
        self.queued
            .load(Ordering::Relaxed)
            .saturating_sub(self.handled.load(Ordering::Relaxed))
    }

    /// How many lines have been dropped and not yet reported.
    ///
    /// **Both causes, and this is where that is written down.** `send` charges
    /// this counter for `TrySendError::Full` *and*
    /// for `TrySendError::Disconnected` — a queue that is full and a drain
    /// thread that is gone are one number. `Log::dropped` in `lib.rs` says so
    /// and cites this doc as its authority, and a fact a later reader relies on
    /// rather than re-derives has to be where the citation sends them.
    #[must_use]
    pub(crate) fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// How many lines the fallback could not write, since this process began.
    ///
    /// **`dropped()` is not the only number a caller can read**, and it counts
    /// one thing: lines refused at the queue. A line the store did not take and
    /// stderr would not write is a different loss, and counted in a `static`
    /// nothing outside this file could reach it would be invisible — measured,
    /// 235 of 300 lines in neither place with `dropped()` reading `0`.
    ///
    /// Takes no `self`: the counter is process-wide, because the writer thread
    /// is. Two shippers in one process share stderr and share this number, and
    /// pretending otherwise would give a caller a per-shipper figure that is
    /// not one.
    #[must_use]
    pub(crate) fn lost() -> u64 {
        LOST.load(Ordering::Relaxed)
    }

    /// Charges `many` lines to the lost counter.
    ///
    /// **A door, because the two accessors are in two files.** `Log::lost()`'s
    /// other driver is an inequality in a child whose stderr is full, where a
    /// wrong number arrives as a swallowed panic. Its sibling `Log::dropped()`
    /// has an equality against the shipper; this is what lets `lost()` have one
    /// too, in the parent, where a failure can be read.
    #[cfg(test)]
    #[doc(hidden)]
    pub(crate) fn charge_lost_for_tests(many: u64) {
        LOST.fetch_add(many, Ordering::Relaxed);
    }

    /// Takes back what `charge_lost_for_tests` charged.
    #[cfg(test)]
    #[doc(hidden)]
    pub(crate) fn discharge_lost_for_tests(many: u64) {
        LOST.fetch_sub(many, Ordering::Relaxed);
    }
}

/// Writes to stderr, and never panics doing it.
///
/// **`eprintln!` panics when the write fails**, and the fallback path is exactly
/// where that happens: stderr closed, redirected to a full disk, or a pipe whose
/// reader has gone. A panic there kills the one thread draining the queue, so
/// every subsequent `send` fills it and drops silently — the store being down
/// would have turned logging off permanently, which is the opposite of what the
/// fallback is for. `§18.5`'s argument for self-hosting only holds if the
/// failure path keeps the data here.
///
/// The write is a single `write_all` under one lock, so a batch's lines cannot
/// interleave with another thread's.
fn to_stderr(text: &str) -> bool {
    #[expect(
        clippy::disallowed_methods,
        reason = "the stderr fallback: a log store that is down must not stop the platform"
    )]
    let mut error = std::io::stderr().lock();
    // **Whether it landed, counted.** `let _ = write_all(…)` and an
    // unconditional `WRITTEN += 1` would count a stderr that *refuses* every
    // write — a full disk, a closed descriptor — as having taken every line:
    // measured with stderr on `/dev/full`, 5000 lines in, 66 to the store,
    // every stderr write `ENOSPC`, and **4934 accounted for nowhere**, with the
    // process exiting 0. The panic this function exists to avoid is avoided
    // either way; what this adds is that a refused write is counted as lost.
    let wrote = error.write_all(text.as_bytes()).is_ok();
    let _ = error.flush();
    wrote
}

/// The same, bounded — for `Drop`, and for the lines themselves.
///
/// **A blocking write is not a bounded wait.** `FLUSH` and `GRACE` bound what
/// the shipper waits for on the network; the fallback is a `write_all` under
/// the process-wide stderr lock, and a pipe nobody is draining blocks it for
/// ever. Measured with that write unbounded: a full queue, a hung store and an
/// undrained stderr pipe hold process exit past **thirty seconds** — the same
/// run takes 4.06 s with stderr on a file. A `Drop` written to bound shutdown
/// is then what makes it unbounded, and journald backpressure or a stopped
/// terminal is enough to produce it.
///
/// The write happens on a thread this does not wait for past `GRACE`. If the
/// pipe is blocked the thread stays blocked and the process still exits —
/// `exit` does not wait for it — and the lines it was holding are lost, which is
/// the same trade the queue already makes and is better than not exiting.
///
/// **And the log lines go through here too.** With only `Drop`'s own three
/// messages bounded, the fallback that exists to keep a line when the store is
/// unreachable blocks on the first full pipe: measured against an undrained
/// stderr, 810 of 1000 accepted lines reach neither the store nor stderr,
/// with `dropped()` reading zero — and in the overflow case 2810 lost against
/// 1999 counted, so 811 go with no number anywhere. 64 031 bytes is one pipe
/// buffer; journald backpressure, a stopped terminal or an unread `docker logs`
/// is enough.
///
/// **And a blocked pipe never blocks the drain thread.** The two simpler
/// designs are both wrong. A `write_all` on the drain thread stops it dead: an
/// undrained pipe holds it in `futex_do_wait` while every later line fills a
/// queue nothing empties —
/// measured, `on_stderr(554) + reported_drops(1616) = 2170 of 3000`, with 830
/// lines named by no number at all. A thread per line with a timeout leaks one
/// thread per line and loses each one silently, and its "stderr is blocked"
/// latch can be set *after* the write it was watching succeeded, which leaves
/// the latch set with nothing left to clear it — a permanent silence produced
/// by the fallback that exists to prevent one.
///
/// So there is one writer thread and a bounded queue. Enqueuing never blocks;
/// a full queue is counted rather than waited on; and when the pipe drains the
/// writer catches up by itself, with no latch to get stuck. `flush_stderr` is
/// how `Drop` waits for it, once, inside the shutdown's own budget.
///
/// **As deep as the line queue, plus room for the notes.** At 512 a shutdown
/// that has to write a full `QUEUE` out cannot:
/// measured, `62 reached the store and 820 reached stderr, of 1000 — 118 line(s) reached neither`.
/// The fallback exists precisely for the case where every queued line has to go
/// to stderr at once, so it holds one of those plus the handful of notes that
/// accompany it.
const STDERR_QUEUE: usize = QUEUE + 128;

/// How many lines have been handed to the writer. With [`WRITTEN`], the
/// difference is how many are still queued — two monotonic counters rather than
/// one gauge, because a gauge has to be incremented before the send and
/// decremented after the write, and those two race: measured, an outstanding
/// count above the queue's own capacity.
static SENT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// How many have been written. With the two below it, every line handed to the
/// fallback is in exactly one of three places — written, queued, or counted as
/// lost — and *none of the three is silence*, where an unbounded write leaves
/// 830 of 3000 lines.
static WRITTEN: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// How many lines were lost because the stderr queue was full, and not yet said
/// out loud. Swapped to zero by the write that reports it.
static UNREPORTED: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// The same, never reset — so the three counters add up to what was handed over.
/// Read from outside through [`Shipper::lost`], because a number nothing can
/// reach is not a number an operator has: measured, 235 of 300 lines in neither
/// the store nor stderr with the only reachable counter reading `0`.
/// The reportable gauge cannot do that job: it is consumed by the line that
/// reports it, and a total that goes down is not a total.
static LOST: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Lines a grace period gave up on and charged to `LOST`, still in the channel.
///
/// **The gauge needs a third number, and inflating `WRITTEN` is not
/// it.** Moving `flush_stderr`'s backlog out of `SENT - WRITTEN` by adding it
/// to `WRITTEN` counts those lines twice, because the writer thread is still
/// alive and adds to `WRITTEN` again for each of them when it dequeues
/// them. The overshoot is permanent, so `still_queued()` saturates at zero for
/// that many lines for the rest of the process:
///
/// ```text
/// queued when grace began  39   LOST charged  39   gauge after charge  1
/// after stderr came back:  SENT 42  WRITTEN 81  written - sent = 39
/// second shutdown: 40 handed over with stderr blocked
///   still_queued() 0   LOST charged 0   flush_stderr waited 681ns of 3s
/// ```
///
/// That second line is verbatim the outcome `flush_stderr` exists to prevent —
/// lines "in neither place and named by no number" — and it is reachable
/// whenever a process drops more than one `Shipper`, which `Log::open` being
/// public makes ordinary and which this crate's own test binary does dozens of
/// times per run.
///
/// So the charge is recorded here instead. `WRITTEN` goes on meaning *dequeued*,
/// which is what its own paragraph says it means, and the writer decrements this
/// as it drains — restoring the gauge exactly rather than leaving it short.
static CHARGED: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// The one thread that writes to stderr, started on first use.
static WRITER: std::sync::OnceLock<std::sync::mpsc::SyncSender<String>> =
    std::sync::OnceLock::new();

/// Hands one line to the stderr writer, without waiting for it.
fn to_stderr_line(text: String) {
    let writer = WRITER.get_or_init(|| {
        let (sending, waiting) = std::sync::mpsc::sync_channel::<String>(STDERR_QUEUE);
        let started = std::thread::Builder::new()
            .name("nc-log-stderr".to_owned())
            .spawn(move || {
                for line in waiting {
                    // **`WRITTEN` either way, because the line left the queue
                    // either way.** `still_queued()` is `SENT - WRITTEN`, so
                    // counting only a landed write leaves a refused one in the
                    // gauge: it reads 10 with an empty queue and an idle
                    // writer, `flush_stderr` then spins its whole 3 s grace on
                    // every later shutdown, and charges the same lines to
                    // `LOST` a second time — ten lines handed over, twenty
                    // counted lost. What `WRITTEN` counts is *dequeued*;
                    // whether it landed is `LOST`.
                    WRITTEN.fetch_add(1, Ordering::Relaxed);
                    // **And whether a grace period already gave up on it.** If
                    // so it is already in `LOST`, and this is where the gauge is
                    // handed back the line — see `CHARGED`.
                    #[allow(
                        deprecated,
                        reason = "Rust 1.99 renames `fetch_update` to `try_update`, which does not \
                                  exist at this workspace's 1.96 floor; rename it when the floor moves"
                    )]
                    let charged = CHARGED
                        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |left| {
                            (left > 0).then(|| left - 1)
                        })
                        .is_ok();
                    let landed = to_stderr(&line);
                    match (charged, landed) {
                        // Charged as lost, and then it landed after all: the
                        // process was still alive and stderr came back. `LOST`
                        // is "reached neither the store nor stderr", so this one
                        // is not lost and the charge is taken back.
                        (true, true) => {
                            LOST.fetch_sub(1, Ordering::Relaxed);
                        }
                        // Charged and really lost, counted once at the charge;
                        // or written and never charged, which is the ordinary
                        // path. Neither moves a number here.
                        (true, false) | (false, true) => {}
                        // Nowhere left to say so: this *is* the place that says
                        // things. The number is what a test and a monitor read.
                        (false, false) => {
                            LOST.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            });
        if started.is_err() {
            // No thread, so nothing will ever drain: the sender is kept anyway
            // and every send fills the queue and is counted, which is the same
            // answer as a blocked pipe and is at least a number.
        }
        sending
    });
    let (text, carried) = the_line_to_write(text);
    if writer.try_send(text).is_ok() {
        SENT.fetch_add(1, Ordering::Relaxed);
    } else {
        // The number this line was carrying goes back, or reporting it would be
        // what loses it; the line itself is counted once, in both places.
        UNREPORTED.fetch_add(carried, Ordering::Relaxed);
        LOST.fetch_add(1, Ordering::Relaxed);
    }
}

/// The line to write next, with the gap since the last one taken up.
///
/// **The `swap` needs a test of its own.** `UNREPORTED` is written where a
/// handover is refused and read here, and the tests for the pair
/// call `the_backlog_after_a_refusal`, `with_the_backlog_notice`
/// and `the_line_and_what_it_carries` *directly* — never this function, so the
/// static between the write and the read is held by none of them. Measured
/// without one, `UNREPORTED.swap(0, …)` replaced by `0`: `cargo test -p nc-log`
/// green and `cargo xtask check` at `check passed`.
///
/// What that costs is the whole of "counted as lost" reaching a person: with the
/// read severed the notice is handed `0` for the life of the process and
/// `nc-log: N line(s) were lost while stderr was not draining` never appears
/// again, while `LOST` goes on rising where only a monitor looks. This module's
/// header states the invariant as *"every line handed to the fallback is in
/// exactly one of three places — written, queued, or counted as lost — and none
/// of the three is silence"*.
///
/// The swap is here, one call from the two expressions it feeds, for the same
/// reason those two were folded into one function: a seam between two tested
/// halves is a seam nothing tests. What is left at the call site is a single
/// call, and `the_handover_is_the_one_that_does_not_wait` holds its shape.
fn the_line_to_write(text: String) -> (String, u64) {
    // Said before the line it belongs to, so a reader meets the gap before the
    // first line that followed it rather than after.
    let lost = UNREPORTED.swap(0, Ordering::Relaxed);
    the_line_and_what_it_carries(text, lost)
}

/// The line to write, and the backlog to carry if writing it is refused.
///
/// One function because it is one invariant: **the number a refusal leaves
/// behind is the number the next notice reports.** Split into two expressions
/// the call site combines, each is testable and the combination is not — and
/// the defect the tests below name, dropping the carried number, is plantable
/// again one line over.
fn the_line_and_what_it_carries(text: String, lost: u64) -> (String, u64) {
    (
        with_the_backlog_notice(text, lost),
        the_backlog_after_a_refusal(lost),
    )
}

/// One line, with the gap before it named when there was one.
///
/// **Both halves of this are held, because nothing else would hold
/// them.** `UNREPORTED` and the sentence it produces are the operator's only
/// account of lines the stderr queue refused, and `§18.5`'s argument for
/// self-hosting the log rests on this module's own claim that every line is
/// "written, queued, or counted as lost — and none of the three is
/// silence". The sentence appears once, and with nothing looking for it, it can
/// be deleted outright with `check passed`. Its sibling one screen away — the
/// queue-full report — *is* held, through a child process that reads its own
/// stderr.
fn with_the_backlog_notice(text: String, lost: u64) -> String {
    if lost == 0 {
        return text;
    }
    format!("nc-log: {lost} line(s) were lost while stderr was not draining\n{text}")
}

/// What a refused line leaves behind for the next one that gets through.
///
/// **The line itself, plus what it was carrying.** Dropping the carried number
/// makes the notice say `1 line(s)` for ever while the real backlog grows —
/// permanent under-reporting, with every test in the workspace green. Named
/// beside the sentence above because the two are one invariant: the number a
/// refusal leaves behind is the number the next notice reports.
fn the_backlog_after_a_refusal(carried: u64) -> u64 {
    carried + 1
}

/// How long one read of the answer may block, given what is left of the budget.
///
/// `POLL` is the ceiling, so an abandoned exchange notices the flag within one
/// slice rather than at the deadline. **One millisecond is the floor, and the
/// floor is the whole point**: when `Drop` sets the abandon flag the budget
/// drops from `DEADLINE` to `ABANDONED_ANSWER` while `started.elapsed()` is
/// already seconds, so `left` saturates to zero —
/// and `set_read_timeout(Some(ZERO))` is `InvalidInput`. Without the floor
/// every abandoned exchange dies with this client's own socket API as the
/// operator-facing sentence, verbatim on stderr:
///
/// ```text
/// nc-log: the store did not take 64 line(s): could not bound the answer:
///         cannot set a 0 duration timeout
/// ```
///
/// and returns *without attempting the read*, discarding an answer one syscall
/// away. A separate function because the arm that reports the refusal is
/// unreachable once this is right — the socket has to break
/// for `set_read_timeout` to fail otherwise — so what a test can hold is the
/// arithmetic, which is where the defect lives.
fn answer_slice(left: core::time::Duration) -> core::time::Duration {
    left.min(POLL).max(core::time::Duration::from_millis(1))
}

/// Waits up to `grace` for the stderr writer to catch up.
///
/// Only `Drop` calls it, and only once: the drain thread must never wait on
/// stderr, which is the whole reason the writer is a thread of its own.
fn flush_stderr(grace: core::time::Duration) {
    let until = std::time::Instant::now() + grace;
    while still_queued() > 0 && std::time::Instant::now() < until {
        std::thread::sleep(core::time::Duration::from_millis(5));
    }
    // **And what it gives up on is counted.** With `STDERR_QUEUE` at
    // `QUEUE + 128`, `try_send` never refuses a line during a shutdown — so
    // without this `LOST` stays zero while the writer sits blocked on a pipe
    // nobody is reading, and the accounting this module's own header promises
    // turns a *counted* loss into an uncounted one. Measured: exit 15.1 s, 65
    // lines delivered, 48 on stderr, **887 of 1000 in neither place and named
    // by no number**.
    //
    // A line still queued when the process ends is a line nobody will read, so
    // it belongs in `LOST` — and the last attempt to say so goes out before the
    // count is folded in, because if stderr has come back this is the one
    // message that matters.
    let left = still_queued();
    if left > 0 {
        to_stderr_line(format!(
            "nc-log: {left} line(s) were still waiting for stderr when this process \
             ended, and are lost\n"
        ));
        LOST.fetch_add(left, Ordering::Relaxed);
        // **And out of the gauge, or the same backlog is charged twice.**
        // `still_queued()` is `SENT - WRITTEN`, and moving only `LOST` means a
        // second shutdown counts every one of those lines again, and a stderr
        // that comes back afterwards writes them all. Measured at the seam, 900
        // lines handed over with the writer thread blocked on a stderr lock:
        //
        // ```text
        // queued when the first grace began    899
        // LOST after one flush_stderr          899
        // LOST after two                      1799   <- the same backlog, twice
        // WRITTEN once stderr came back        902   <- and they were not lost
        // ```
        //
        // Both halves of the module header's rule broken at once — "every line
        // handed to the fallback is in exactly one of three places: written,
        // queued, or counted as lost" — and `Log::lost` is documented as "how
        // many lines reached neither the store nor stderr", which those 900 did.
        // `FLUSH`'s own doc contemplates a process holding two of them; `Log::open`
        // is public, so it is not hypothetical. What `WRITTEN` counts is
        // *dequeued from the gauge*, not *landed*, which is the same reasoning
        // the refused-write path already carries.
        CHARGED.fetch_add(left, Ordering::Relaxed);
        // Nothing waits for that write: stderr is by definition not draining if
        // this branch was reached, and the process is leaving.
    }
}

/// How many lines are handed over and not yet written.
fn still_queued() -> u64 {
    SENT.load(Ordering::Relaxed)
        .saturating_sub(WRITTEN.load(Ordering::Relaxed))
        .saturating_sub(CHARGED.load(Ordering::Relaxed))
}

/// What one attempt to post a batch did to it.
///
/// **More than two answers, and this is why.** As a `Result<(), String>` with
/// every `Err` batch written to stderr, `Err` covers both "the store never saw
/// this" and "the store read the whole body and then said something this could
/// not parse". Measured against a server that reads the body and closes without
/// a status line:
///
/// ```text
/// one line sent; the store received 1 of it, and stderr holds it too
/// ```
///
/// `ship`'s own three rules say an *unreachable* store falls back to stderr,
/// and a store that was reached and took the data is not unreachable — a
/// duplicate in an audit-adjacent log is not a smaller failure than a loss,
/// because a reader counting events counts it twice.
#[derive(Debug)]
enum Delivery {
    /// The store answered with a 2xx. The batch is its problem now.
    ///
    /// **The status line is the whole of what this reads.** A `200 OK` whose
    /// body says it rejected every line is `Taken`, and the batch is discarded.
    /// That is what HTTP means by 2xx, and it is what this variant asserts —
    /// *the store answered that it took them*, which is one step short of *the
    /// store has them* and is as far as a client ignorant of every store's
    /// answer format can go.
    Taken,
    /// The store is not answering: no address, no connection, or it closed
    /// without a word. Stderr is the fallback `§18.5` asks for.
    ///
    /// **It does not mean the body never arrived.** Measured against a peer
    /// that reads all 64 lines, sends an interim answer and then
    /// closes: `Unreachable`, with `store_read = 64 of 64`. What a client can
    /// see from here is that no status came back, and that is what this variant
    /// says. The consequence for the caller is the same either way — the lines
    /// go to stderr, and may be duplicates — but a shutdown that concludes *the
    /// store has gone* from this is concluding it from the wrong evidence,
    /// which is why [`Delivery::store_answered`] exists and is what that
    /// decision reads.
    Unreachable(String),
    /// The store answered, and its answer was a refusal. It did not keep the
    /// batch, so stderr is right here too.
    Refused(String),
    /// The body went out and **no status came back**. Nothing says the store
    /// took it and nothing says it did not, so the lines are kept: a loss is not
    /// recoverable and a duplicate is. It accepted the connection, so it has not
    /// gone — which is a different question, and [`Self::store_answered`] is
    /// where that one is answered.
    ///
    /// **Not "not one byte": two arms that reach here carry bytes.** A run of
    /// more interim answers than `INTERIM_LIMIT` is bytes — measured,
    /// five `100 Continue` and then a `204`, so the store has read the batch
    /// and said so, and this variant carries it
    /// with `store_answered() == true`. What is true of every arm is that no
    /// *status* arrived within the budget, which is the thing a client can act
    /// on and the reason the lines are kept.
    Silent(String),
    /// The store **answered**, and the answer was not one this can read — no
    /// status line, an interim `1xx`, or something that is not HTTP. It read the
    /// request to reply at all, so it may well hold the batch: writing the lines
    /// out as well would duplicate every one it took.
    ///
    /// **Kept apart from [`Self::Silent`]**, because one variant for both costs
    /// a defect in each direction. Treated as "unknown, do not re-emit", a
    /// store that accepts a connection and never reads loses 116 lines of
    /// 300. Treated as "unknown, re-emit", a store that answered `500`, garbage
    /// or `100 Continue` duplicates **300 of 300**. The question that separates
    /// them is whether anything came back.
    Unreadable(String),
}

impl Delivery {
    /// Why it was not taken, when it was not.
    fn why(&self) -> Option<&str> {
        match self {
            Self::Taken => None,
            Self::Unreachable(why)
            | Self::Refused(why)
            | Self::Silent(why)
            | Self::Unreadable(why) => Some(why),
        }
    }

    /// Whether the lines should also be written to stderr.
    ///
    /// **Unless the store said it has them.** One question, one answer:
    /// the `read == 0` branch of [`Target::answer`] resolves the unknown by
    /// saying *a duplicate is recoverable and a loss is not, and `§18.5`'s
    /// argument for keeping the data on this machine only holds if the failure
    /// path keeps it here*, and a variant resolving the same unknown the other
    /// way, on the grounds that re-emitting would duplicate, is a second answer
    /// that disagrees with it. Measured with that second answer, against a
    /// store that accepts a connection and never calls `read`:
    ///
    /// ```text
    /// 300 line(s): delivered=0  on stderr=184  BOTH=0  NEITHER=116
    /// full-queue drops=0
    /// ```
    ///
    /// 116 lines in no store, on no stream, and counted by nothing — against a
    /// peer that provably held none of them. A second fixture, a store that
    /// reads part of the request and stops, loses 81 of 300. The module header
    /// says this shipper must never lose a line silently.
    ///
    /// So there is one rule and it is the crate's own: the lines go to stderr
    /// unless the store answered that it took them. What that costs is a
    /// duplicate whenever a store ingested a batch and its answer did not
    /// arrive — measured at 114 lines against a store that reads the body and
    /// never replies. That is the trade `§18.5` asks for, made once.
    ///
    /// **It is not the same question as [`Self::store_answered`]**, and
    /// conflating them makes this look like a choice between losing lines and
    /// blaming a healthy store. A silent store keeps its lines *and* is not
    /// concluded to have gone.
    const fn keep_on_stderr(&self) -> bool {
        // Kept when nothing came back at all, when what came back was a refusal
        // or something unreadable, or when the store was never there: a loss is
        // not recoverable and a duplicate is. Not kept when the store said it
        // has them.
        //
        // **An unreadable answer is kept too.** "It read the request to reply
        // at all, so re-emitting would duplicate whatever it took" is
        // falsified: measured against a store that never calls `read` and
        // answers `ERROR: no server available`, which is what a load balancer
        // with no backend says, sixty-four lines reach the store *and* stderr
        // not at all when it is not kept, in two milliseconds, with `dropped()`
        // still zero. A silent, total, uncounted loss needing no timeout.
        //
        // The duplication this costs was measured too — 300 of 300 against a
        // store that answered `500` — and a duplicate is recoverable where a
        // loss is not, which is the rule this crate's own header states. What
        // the message says is that these may be duplicates.
        matches!(
            self,
            Self::Unreachable(_) | Self::Refused(_) | Self::Silent(_) | Self::Unreadable(_)
        )
    }

    /// Whether the store took the batch.
    ///
    /// For tests: the thread acts on [`Self::why`], [`Self::keep_on_stderr`] and
    /// [`Self::store_answered`], each of which is a different question from this
    /// one — a refusal and an unreadable answer are both "not taken" and are not
    /// the same thing to do next.
    #[cfg(test)]
    const fn taken(&self) -> bool {
        matches!(self, Self::Taken)
    }

    /// Why it was not taken, or a panic naming what happened instead.
    ///
    /// For tests: the shape `Result::expect_err` gave them, kept so that a test
    /// asserting on the message says what it is asserting about.
    #[cfg(test)]
    fn expect_refusal(&self, why: &str) -> &str {
        self.why()
            .unwrap_or_else(|| panic!("{why}, and the store took it"))
    }

    /// Whether this attempt says anything about the store still being there.
    ///
    /// [`Self::Refused`] and [`Self::Unreadable`] both mean the store answered,
    /// so a shutdown must not conclude from either that it has gone.
    const fn store_answered(&self) -> bool {
        !matches!(self, Self::Unreachable(_))
    }
}

/// The longest route this client will write into a request line.
///
/// **Named here rather than inside `parse`, because a bound no test can say
/// out loud is a bound no test pins.** Both ends of it are exercised beside
/// the character rule: `LONGEST_PATH - 1` characters of route are accepted
/// and `LONGEST_PATH` refused, so a widening is a failure rather than a
/// silence.
///
/// **What a route this long still admits, disclosed rather than implied.**
/// The character rule takes letters, digits and `/-._~;=:`, so
/// `http://127.0.0.1:9428/insert/jsonline/12345678901` is a route by every
/// rule here — eleven digits, which is the width of the CPF this repository
/// uses as its own example of a personal number. An operator who builds an
/// endpoint with `format!` can still put that much data in a request line,
/// and the store's access log keeps it. The endpoint is operator
/// configuration rather than user input, which is what makes this a
/// residual and not a hole; `value.rs` discloses its twelve-character
/// residual by name for the same reason; this paragraph is this one's.
const LONGEST_PATH: usize = 120;

/// How much of a store's own answer this quotes back, in characters.
///
/// **`STATUS_LIMIT` bytes of somebody else's HTML is not a diagnostic.** The
/// read is bounded at 8 KiB, and every one of those bytes goes to stderr — the
/// same bounded queue the real log lines fall back to — once per failed batch.
///
/// Written as a bare `120` at each site that quotes, the bound would be one no
/// test can name, and a path that forgot to write it, such as the interim
/// answer at `Delivery::Unreadable`, would be bounded by none. A bound no test
/// can say out loud is a bound no test pins — the sentence `LONGEST_PATH`
/// above is given for the same reason.
const QUOTED_ANSWER: usize = 120;

/// A host as it is written in an authority: an IPv6 literal is bracketed.
///
/// **`Target::parse` strips the brackets on purpose** — `host` is `::1`,
/// not `[::1]`, and its own test says so — so the header writer has to put them
/// back. `Host: ::1:9428` is not `uri-host [ ":" port ]`, and a store that
/// validates the header refuses every batch, which this client reads as a
/// refusal and duplicates the whole stream onto stderr, once per batch, for
/// ever. The parser and the writer each have a bracketed-host test.
fn as_an_authority(host: &str) -> String {
    if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_owned()
    }
}

/// As much of `status` as this quotes back, trimmed and bounded.
fn as_much_as_is_a_diagnostic(status: impl AsRef<str>) -> String {
    // **Escaped, as `Target::parse` escapes the endpoint it refuses.** Trimmed
    // and bounded and nothing else, the store's own answer goes onto stderr
    // with its control characters raw — a `\r` rewriting the line a reader is
    // looking at, an `ESC [ 2 J` clearing their screen. `Target::parse` states
    // the reason at its own call site: *"a crate whose premise is that a value
    // cannot end the line it is written on must not let its own endpoint do
    // it"*, and a store's answer is no more this crate's than an endpoint is.
    // One rule, two paths.
    // **Bounded on what a reader sees, which is after the escaping.** Taking
    // `QUOTED_ANSWER` of the *store's* characters and then expanding each
    // unsafe one to six sends a hostile answer to stderr at 720 characters,
    // while the paragraph on `QUOTED_ANSWER` argues that `STATUS_LIMIT` bytes
    // of somebody else's HTML is not a diagnostic. A test that keeps this bound
    // with `"y".repeat(…)`, where no escaping happens, cannot fail.
    let answer = status.as_ref().trim();
    let mut shown = String::with_capacity(QUOTED_ANSWER);
    for letter in answer.chars() {
        let escaped = nc_canon::shown(letter);
        if shown.chars().count() + escaped.chars().count() > QUOTED_ANSWER {
            break;
        }
        shown.push_str(&escaped);
    }
    shown
}

/// Where the lines go.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Target {
    host: String,
    port: u16,
    path: String,
}

impl Target {
    /// Reads an `http://host:port/path` endpoint.
    fn parse(endpoint: &str) -> Result<Self, String> {
        /// The punctuation a route is made of, beyond letters and digits.
        ///
        /// `;` and `=` are here because this repository's own endpoint may carry
        /// a matrix parameter — `/insert/jsonline;v=1` is accepted by a test
        /// beside this one, and the paragraph there explains why. `@` is **not**
        /// here, and it is the character the measured attack used: an address is
        /// data wearing a route's clothes.
        const ROUTE: &str = "/-._~;=:";

        // **No control character anywhere in it.** A CRLF in the path injects
        // header lines into the request this writes — `…/insert` followed by a
        // carriage return and a header, accepted, arrives at the store as three
        // lines. The endpoint is operator configuration rather than user input,
        // which is why this is a small hole; but a crate whose premise is that
        // a value cannot end the line it is written on must not let its own
        // endpoint do it.
        // **Escaped once, for every refusal below.** The guard reads
        // `char::is_control`, while `nc_canon::is_unsafe_in_a_message`, which
        // the sibling escapes by, also names `"` and `\` as what would end a
        // field or a line — so escaping on the control-character path alone
        // lets those two reach the other refusals raw. Measured with
        // `http://exa"mple.invalid:9428/insert`, which is in the list the test
        // beside this one walks.
        //
        // **Escaped and not bounded, which is where it parts from the sibling.**
        // `as_much_as_is_a_diagnostic` bounds because a store's answer is remote
        // data of arbitrary size; an endpoint is the string this caller just
        // passed, and two of the refusals below are *about* its length — a
        // refusal that says a path is 121 characters and then shows 120 of them
        // is a refusal a reader cannot act on. The length rules in this same
        // function are the bound.
        let visible: String = endpoint.chars().map(nc_canon::shown).collect();
        if let Some(bad) = endpoint.chars().find(|character| character.is_control()) {
            return Err(format!(
                "`{visible}` is not an endpoint: `{}` in it would write extra header \
                 lines into every request this sends",
                nc_canon::shown(bad)
            ));
        }
        let rest = endpoint.strip_prefix("http://").ok_or_else(|| {
            format!(
                "`{visible}` is not an http:// endpoint. The store is on this machine \
                 (NOCTUA.md §18.5), so this client speaks plain HTTP and nothing else"
            )
        })?;
        // **And no query string, which is a channel into the store.** The
        // endpoint is the one `&str` on this crate's public surface: `Origin`'s
        // two fields are `Literal` precisely because they appear on every line,
        // and the parameter beside them would otherwise take arbitrary text.
        // VictoriaLogs reads query parameters on `/insert/jsonline`, so a caller
        // that builds its endpoint with `format!` writes whatever it likes into
        // every record — measured, from an ordinary consumer crate with no
        // `format!` at a logging call, no `leak`, no `unsafe`:
        //
        // ```text
        // …/insert/jsonline?extra_fields=smuggled={user}
        // -> {"_msg":"prune.within_limit","smuggled":"ana.silva@example.com", …}
        // ```
        //
        // An email address, verbatim, as a field of the log record — past the
        // type wall, past the source gate, and unbounded, where the residual
        // channel this crate does disclose is twelve Crockford characters. The
        // same door takes `ignore_fields`, `_msg_field` and `debug=1`, which
        // rewrite or discard every record: a central log switched off while
        // `check` is green.
        //
        // A query string is refused rather than stripped. Stripping would make
        // the endpoint a caller wrote and the endpoint this posts to different
        // things, and the next reader of the two would have to work out which.
        if let Some(at) = rest.find(['?', '#']) {
            return Err(format!(
                "`{visible}` carries `{}`, and this client refuses a query string. The \
                 store reads parameters there — `extra_fields` writes them onto every \
                 record, `ignore_fields` and `_msg_field` rewrite them — so an endpoint \
                 built with `format!` is a way to put caller text into the log that the \
                 type wall exists to prevent",
                // **Escaped, like every other name in this function.** `"` and
                // `\` are `is_unsafe_in_a_message` without being
                // `char::is_control`, so they pass the guard at the top, and a
                // slice of the caller's own text interpolated raw in these two
                // arms would arrive here unescaped. The list beside this
                // function has a case that reaches each site.
                rest[at..].chars().map(nc_canon::shown).collect::<String>()
            ));
        }
        let (authority, path) = rest.split_once('/').map_or((rest, ""), |(a, p)| (a, p));
        // **A bracketed authority is one host, not two.** Split on the first
        // colon, `http://[::1]:9428/insert` is unparseable, and the refusal
        // names `:1]:9428` — a fragment the parser invented — rather than
        // saying the client does not read this shape. `§18.5` puts the store on
        // this machine, and `[::1]` is the ordinary way to write that.
        let (host, port) = match authority.strip_prefix('[') {
            Some(rest) => {
                let (host, after) = rest.split_once(']').ok_or_else(|| {
                    format!(
                        "`{visible}` opens a bracketed host and never closes it, so there \
                         is no address in it"
                    )
                })?;
                let port = after.strip_prefix(':').ok_or_else(|| {
                    format!("`{visible}` names no port, and this client does not assume one")
                })?;
                (host, port)
            }
            None => authority.split_once(':').ok_or_else(|| {
                format!("`{visible}` names no port, and this client does not assume one")
            })?,
        };
        let port: u16 = port.parse().map_err(|_| {
            format!(
                "`{}` is not a port number, so `{visible}` names nothing this \
                 client can connect to",
                port.chars().map(nc_canon::shown).collect::<String>()
            )
        })?;
        if host.is_empty() {
            return Err(format!(
                "`{visible}` names no host. The store runs on this machine, so the \
                 endpoint looks like `http://127.0.0.1:9428/insert/jsonline`"
            ));
        }
        // **And the host is this machine, which `§18.5` asserts and this
        // enforces.** That section's argument for a self-hosted store is that
        // "the cheapest way to guarantee no user data leaves this machine is
        // for there to be nowhere else for it to go" — and the endpoint is the
        // one `&str` left on this crate's public surface. A caller building it
        // with `format!` could redirect the entire log stream to a host of its
        // choosing: not the twelve-character channel the wall's residual
        // discloses, but every line of every event.
        //
        // So the endpoint is held here, whoever calls: the kernel is one caller
        // outside this crate, and every caller after it inherits the rule rather
        // than a review of how it built its `&str`. The compose file binds the
        // store to loopback for the same reason; this is the client half of the
        // same sentence.
        //
        // `localhost` is accepted because `cargo xtask dev` may print it and a
        // developer may type it; what it resolves to is the machine's own
        // resolver's business, and refusing it would refuse the ordinary case
        // to close nothing a `127.0.0.1` spelling does not already close.
        // Every spelling of this machine, decided by **parsing the address**
        // rather than by matching its text. `starts_with("127.")` accepts
        // `127.0.0.1.example.invalid`, which is a name somebody else owns — the
        // whole point of the rule, defeated by four characters.
        // `IpAddr::is_loopback` knows `127.0.0.0/8`, `::1` and the IPv4-mapped
        // form a dual-stack resolver produces.
        //
        // `localhost` by name, because `cargo xtask dev` may print it and a
        // developer may type it; what it resolves to is that machine's
        // resolver's business, and refusing it would refuse the ordinary case
        // to close nothing the numeric spellings do not already close.
        // The mapped form is unwrapped first: `Ipv6Addr::is_loopback` is true
        // of `::1` alone, and `::ffff:127.0.0.1` is how a dual-stack resolver
        // writes this machine's IPv4 address.
        let here = host.eq_ignore_ascii_case("localhost")
            || host.parse::<std::net::IpAddr>().is_ok_and(|address| {
                let address = match address {
                    std::net::IpAddr::V6(inner) => {
                        inner.to_ipv4_mapped().map_or(address, std::net::IpAddr::V4)
                    }
                    other @ std::net::IpAddr::V4(_) => other,
                };
                address.is_loopback()
            });
        if !here {
            return Err(format!(
                "`{visible}` names `{}`, and this client speaks to the store on \
                 this machine only (NOCTUA.md §18.5). The whole argument for a \
                 self-hosted log is that there is nowhere else for the lines to go, \
                 and an endpoint a caller builds is the one place that could send \
                 them somewhere else",
                host.chars().map(nc_canon::shown).collect::<String>()
            ));
        }
        // **And the path is bounded.** The refusal above takes a query string
        // apart at length — a caller can put anything after `?` and it reaches
        // the store in the request line — and a path left to `format!` is the
        // same channel. Measured against a real listener, with an endpoint
        // built by `format!`:
        //
        // ```text
        // POST /insert/jsonline/ana.silva@example.com HTTP/1.1
        // ```
        //
        // Unbounded length, any character that is not a C0 control, in every
        // request the shipper makes, and into the store's own access log.
        // `Log::open(` is not a logging call, so nothing looks at the `format!`
        // that built it either.
        //
        // A path is a route, and a route is a short thing made of the characters
        // a route is made of. Anything else is a caller putting data where a
        // route goes.

        if path.len() > LONGEST_PATH {
            return Err(format!(
                "`{visible}`'s path is {} characters, and a route is not where data \
                 goes — `{}` is the longest this accepts. The store's own access log \
                 keeps the request line",
                path.len(),
                LONGEST_PATH
            ));
        }
        if let Some(stray) = path.chars().find(|character| {
            !matches!(character, 'a'..='z' | 'A'..='Z' | '0'..='9') && !ROUTE.contains(*character)
        }) {
            return Err(format!(
                "`{visible}`'s path holds `{}`, and a route is made of letters, \
                 digits and `{ROUTE}`. Anything else is a caller putting \
                 data where a route goes, and the store keeps the request line",
                nc_canon::shown(stray)
            ));
        }
        Ok(Self {
            host: host.to_owned(),
            port,
            path: format!("/{path}"),
        })
    }

    /// Posts a batch as one request, and says what became of it.
    ///
    /// `abandoning` is set by [`Shipper::drop`] when its wait runs out, and
    /// this reads it **while waiting for an answer**. Without that, the thread
    /// can be inside one exchange for a whole `DEADLINE` — ten seconds —
    /// while `GRACE` is three, so the phase whose entire job is to write the
    /// rest of the queue where a person can read it never runs. Measured
    /// against a store answering each batch after five seconds, with a full
    /// queue at exit:
    ///
    /// ```text
    /// 228 reached the store, 0 reached stderr, of 1000
    /// nc-log: the store did not take 836 line(s) within 15s; writing them here instead
    /// nc-log: 836 line(s) were still queued after 3s and are lost
    /// ```
    ///
    /// 772 lines lost by the phase that exists to keep them. A test stalling
    /// 1100ms a batch covers the band 0.768s–3s and nothing outside it.
    fn post(&self, lines: &[String], abandoning: &AtomicBool) -> Delivery {
        match self.exchange(lines, abandoning) {
            Ok(outcome) => outcome,
            Err(unreachable) => Delivery::Unreachable(unreachable),
        }
    }

    /// The request line and headers this shipper sends, for a body of `body`.
    ///
    /// **Its own function so the `Host:` header can be driven rather than
    /// pinned.** A text pin on the hand-over from `as_an_authority` is wrong
    /// either way — as a line with twelve spaces of indentation it refuses an
    /// ordinary hoist, and as a call anywhere it is paid for by a discarded
    /// call. Built inside a function that opens a socket, the header could only
    /// be pinned as text, and the one fixture that reads a real header speaks
    /// to the loopback — where a bracket rule cannot fire.
    ///
    /// Everything above the blank line and nothing below it: the body is the
    /// caller's, and what this is about is the authority.
    fn request(&self, body: &str) -> String {
        format!(
            "POST {} HTTP/1.1\r\n\
             Host: {}:{}\r\n\
             Content-Type: application/stream+json\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\
             \r\n",
            self.path,
            as_an_authority(&self.host),
            self.port,
            body.len()
        )
    }

    /// The request itself.
    ///
    /// Every `?` here is a batch the store never received, which is what makes
    /// the mapping in [`Self::post`] correct rather than a convenience: this
    /// returns `Err` only before **any** of the body is written.
    ///
    /// **The body's own write is not one of them.** A store that reads a prefix
    /// and then stops reading — ingest stalled, not gone — makes `write_all`
    /// fail part way through, and classified `Unreachable` the whole batch
    /// would be re-emitted to stderr, duplicating every line the store had
    /// already taken, and counted against `GIVE_UP_AFTER` so that two of them
    /// divert the rest of the queue untried. Measured against a listener that
    /// reads 2 MB of a 60 000-line batch and stops, with that classification:
    ///
    /// ```text
    /// outcome=Unreachable("could not write to the store: Resource temporarily unavailable")
    /// keep_on_stderr=true  store_answered=false
    /// lines_the_store_read=4876 of 60000
    /// ```
    ///
    /// 4876 lines in the store and 60 000 on stderr. Here it is `Silent`,
    /// and `Silent` is kept.
    ///
    /// **The half that matters is the store not being concluded gone**, which
    /// is what stops two of these diverting the rest of the queue untried. The
    /// other half is the duplication, and on the trade it is the right way
    /// round: a duplicated line is a line somebody can read twice, and a
    /// dropped one is gone. `keep_on_stderr`'s own doc states the rule this
    /// follows — the lines go to stderr unless the store answered that it took
    /// them — and a peer that stopped reading answered nothing.
    ///
    /// **And the numbers above cannot happen as shipped.** `BATCH` is 64, so a
    /// body is about ten kilobytes and fits in a socket buffer: `write_all`
    /// does not fail part way through, and this branch is unreachable at the
    /// configured batch size. It is kept because `bounds` shortens the write
    /// during a shutdown, and because a larger `BATCH` would make it live
    /// again — the measurement is what it would cost, not what it costs today.
    fn exchange(&self, lines: &[String], abandoning: &AtomicBool) -> Result<Delivery, String> {
        let mut body = String::new();
        for line in lines {
            body.push_str(line);
            body.push('\n');
        }
        let request = self.request(&body);
        // **The connect is bounded too.** The timeouts below are set *after* it
        // returns, so they do not apply to it — and a connect to an address
        // that silently drops packets does not return until the OS gives up on
        // its SYN retries, which is minutes. The caller never blocks, because
        // the queue drops; the single thread draining that queue does, and with
        // it the stderr fallback that is `§18.5`'s whole reason for
        // self-hosting.
        let started = std::time::Instant::now();
        // **And the shutdown's own deadline bounds all three, not just the
        // read.** With `abandoning` read inside `answer` alone, a thread that
        // enters here just before the flush runs out could spend
        // `CONNECT_TIMEOUT` plus the write timeout — seven seconds — against a
        // `GRACE` of three, and everything behind it in the queue would be
        // reported lost while this thread is still inside one exchange. Not
        // reachable from one shipper against an ordinary store, because a
        // batch's body fits in the socket buffer and the backlog never fills;
        // latent rather than measured, and bounded here because the cost of
        // being wrong about it is the whole tail of the queue.
        let (connect, write) = bounds(abandoning.load(Ordering::Relaxed));
        let address = (self.host.as_str(), self.port)
            .to_socket_addrs()
            .map_err(|error| format!("could not resolve the store: {error}"))?
            .next()
            .ok_or_else(|| format!("`{}` resolved to no address", self.host))?;
        let mut stream = TcpStream::connect_timeout(&address, connect)
            .map_err(|error| format!("could not reach the store: {error}"))?;
        // Bounded, because a store that accepted the connection and then stopped
        // reading would otherwise hold this thread forever — and the thread is
        // the only one draining the queue.
        stream
            .set_write_timeout(Some(write))
            .and_then(|()| stream.set_read_timeout(Some(write)))
            .map_err(|error| format!("could not bound the request: {error}"))?;
        stream
            .write_all(request.as_bytes())
            .map_err(|error| format!("could not write to the store: {error}"))?;
        // From here the store has the headers, so anything it then accepts of
        // the body it may keep. A failure is unknown rather than undelivered.
        if let Err(error) = stream.write_all(body.as_bytes()) {
            // **Closed is not stopped**, the same distinction the answer makes:
            // a peer that closed the connection took none of this batch and the
            // lines are kept, while a peer that stopped *reading* may hold part
            // of it and re-emitting would duplicate what it took.
            return if peer_closed(&error) {
                Err(format!("could not write to the store: {error}"))
            } else {
                Ok(Delivery::Silent(format!(
                    "the store stopped reading part way through the batch: {error}, and \
                     answered nothing"
                )))
            };
        }

        Ok(Self::answer(&stream, started, abandoning))
    }
}

/// How long the connect and the write may take, given whether this is a shutdown.
///
/// **A function, because the timing cannot be tested from outside.** A refused
/// connection returns instantly whatever the bound is, and an address that
/// black-holes packets is not something a test can rely on having. What can be
/// asserted is the choice, so the choice is what this returns.
///
/// **How narrow the window is, said rather than implied.** The drain loop checks
/// `abandoning` and writes the batch to stderr *before* it calls `post`, so an
/// exchange can only observe the flag already set in the microseconds between
/// that check and this call. What actually carries a shutdown is
/// [`Target::answer`], which re-reads the flag every `POLL` and is reached on
/// every batch. These two bounds cover the race and nothing wider, and a test
/// that reaches them has to construct a state the loop does not produce — which
/// is a shape this file condemns elsewhere, so it is written down here rather
/// than left for a reader to discover.
const fn bounds(abandoning: bool) -> (core::time::Duration, core::time::Duration) {
    if abandoning {
        (ABANDONED_ANSWER, ABANDONED_ANSWER)
    } else {
        (CONNECT_TIMEOUT, WRITE_TIMEOUT)
    }
}

/// Whether this error means the store closed, as opposed to stopping.
///
/// The same question on a read and on a write, and the answer decides the same
/// thing both times: a peer that closed is not holding the batch, so the lines
/// are kept; a peer that merely stopped may be ingesting them, so re-emitting
/// would duplicate every one.
fn peer_closed(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::UnexpectedEof
            | std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::NotConnected
    )
}

/// What a failed read of the status line means for the batch already written.
///
/// **Closed is not the same as silent**, and `status.is_empty()` cannot tell
/// them apart.
///
/// A peer that *closed* gives `ECONNRESET` or an unexpected end of file:
/// nothing is holding the batch, so it is `Unreachable` and the lines are kept
/// — which is what a store being restarted looks like.
///
/// A peer that is merely *silent* past the deadline times out
/// (`WouldBlock`/`TimedOut`). It read the body and may well be ingesting it —
/// measured, a store answering one second late: `lines_the_store_received = 1`,
/// and with the batch re-emitted as `Unreachable` every line is duplicated and
/// the shutdown concludes the store has gone. So it is not `Unreachable`: it
/// gets the verdict a *trickling* store gets, which is the same unknown fate.
///
/// **A function, because inside the read loop nothing can reach it.** The only
/// way to a non-continuable read error with a silent store is the very last
/// slice of a ten-second deadline, so inside the loop the branch would have no
/// test: deleting the whole distinction there leaves `check passed`, while a
/// probe against a listener that accepts and says nothing shows the difference
/// is real — `Unreachable`, every line of the batch duplicated to stderr, and a
/// shutdown concluding the store has gone. A kind a list names and nothing
/// exercises is a needle, which is the `§19.3` shape this crate's own gate
/// condemns. Here all five are reachable in a unit test, and each one is.
fn verdict(error: &std::io::Error, nothing_read_yet: bool) -> Delivery {
    match (nothing_read_yet, peer_closed(error)) {
        // Closed with nothing said: it is not holding the batch.
        (true, true) => Delivery::Unreachable(format!("the store did not answer: {error}")),
        // Alive and silent: unknown, and a loss is the worse half of unknown.
        (true, false) => Delivery::Silent(format!("the store did not answer: {error}")),
        // Part of an answer arrived, so it read the request.
        (false, _) => Delivery::Unreadable(format!("the store stopped answering: {error}")),
    }
}

impl Target {
    /// Reads the store's status line, bounded twice over.
    ///
    /// Split out because `exchange` would otherwise cross a hundred lines, and
    /// because these are two different jobs: everything above is "did the body
    /// go out", and every `?` there means it did not. From here down the body
    /// **has** gone out, which is what makes every outcome below a `Delivery`
    /// rather than an `Err`.
    fn answer(
        stream: &TcpStream,
        started: std::time::Instant,
        abandoning: &AtomicBool,
    ) -> Delivery {
        // **An interim answer is followed, not reported.** RFC 9110 lets a
        // server send a `1xx` at any time and requires a client to read past
        // it; the real status follows. Reading one status line and stopping
        // puts `HTTP/1.1 100 Continue` into `Unreadable`, and where
        // `Unreadable` is not re-emitted that is a batch in neither the store
        // nor stderr, sixty-four lines at a time, with only a count reaching
        // anybody. `Unreadable` is kept, which closes the loss but not the
        // verdict: `100 Continue` is still not a store refusing a batch.
        // Following it costs no second loop: it is the same loop called again,
        // with a bound and a test.
        //
        // Four is generous: a store sends one `100 Continue`, a proxy may add a
        // `103 Early Hints`. Past that the peer is not answering.
        const INTERIM_LIMIT: usize = 4;
        let mut last = None;
        // **Inclusive, because every iteration spends itself on an interim.**
        // Written exclusive, at most three interims can precede a status the
        // client reads while the constant says four — and the fourth-then-`204`
        // case comes back `Silent`, so a store that *took* the batch has all 64
        // lines written to stderr as well. Measured: `BOTH = 64 of 64`.
        for _ in 0..=INTERIM_LIMIT {
            match Self::one_answer(stream, started, abandoning) {
                Delivery::Unreadable(why) if why.starts_with(INTERIM) => last = Some(why),
                settled => return settled,
            }
        }
        // **`Silent`, because a store that never sent a status never said it
        // has them.** A run of interim answers is the store saying, in as many
        // words, that it has not answered yet, so `Unreadable` — an answer that
        // could not be parsed — is the wrong verdict for it. Measured where
        // `Unreadable` is not re-emitted: five `100 Continue` and then a `500`
        // lose sixty-four lines to no store and no stderr. Both halves matter
        // and only one of them is the loss: `Unreadable` is kept, so the lines
        // survive either way, and this arm is what makes the *verdict* say the
        // store went quiet rather than that it answered something unparseable.
        // **What it says is that this client stopped reading.** Carrying the
        // *last interim* — "the store answered the interim
        // `HTTP/1.1 100 Continue`" — would give an operator a sentence about
        // the peer and never say that the client had given up after
        // `INTERIM_LIMIT`. An `unwrap_or_else` here would be unreachable
        // besides: `last` is `Some` on every path that reaches here, and its
        // message would say four when five arrived.
        Delivery::Silent(format!(
            "the store sent more than {INTERIM_LIMIT} interim answer(s) and no status, so \
             this client stopped reading{}",
            last.map_or_else(String::new, |why| format!(" — the last was {why}"))
        ))
    }

    /// One status line, which may be an interim `1xx`.
    fn one_answer(
        stream: &TcpStream,
        started: std::time::Instant,
        abandoning: &AtomicBool,
    ) -> Delivery {
        // **Bounded, and the read timeout does not bound it.** A server that
        // answers steadily with no newline never trips a timeout — every read
        // returns data — so without the bound this grows without limit: a 384
        // MiB answer takes the process from 3.4 MiB resident to 223 MiB, and an
        // 8 MiB answer produces 8 MiB of stderr from the line below. A status
        // line is under a hundred bytes; anything past `STATUS_LIMIT` is not
        // one.
        // **The budget is re-applied before every read, not once before the
        // first.** Set once, it does not bound the exchange: `set_read_timeout`
        // bounds *each* read, so a store writing one byte every 100 ms and
        // never a newline holds `post` for 25 seconds against a 10-second
        // deadline, measured — and the worst case is `STATUS_LIMIT` reads of
        // `DEADLINE` each, about 22 hours, on the one thread draining the
        // queue, with every line dropped meanwhile and the stderr fallback that
        // is §18.5's whole argument never running.
        let mut answer = stream.take(STATUS_LIMIT);
        let mut read_bytes = Vec::new();
        let mut byte = [0_u8; 1];
        loop {
            // **And the shutdown's own deadline, once it has one.** The batch
            // is already written, so its fate is unknown either way; what this
            // buys is that everything *after* it reaches stderr instead of
            // being lost to a wait nobody is left to hear the end of.
            let budget = if abandoning.load(Ordering::Relaxed) {
                ABANDONED_ANSWER
            } else {
                DEADLINE
            };
            let left = budget.saturating_sub(started.elapsed());
            // **Each read is capped at `POLL`, so the flag is observable.** The
            // budget above is recomputed between reads, and a store that sends
            // *nothing* leaves this thread inside a single `read` whose timeout
            // is already set to `DEADLINE` — so uncapped, `abandoning` flipping
            // at `FLUSH` is never seen, `GRACE` ends while the thread is still
            // waiting, and the queue behind it goes nowhere.
            //
            // Measured without the cap, against a store answering each batch
            // after five seconds with a full queue at exit:
            //
            // ```text
            // took=18.0s store=232 stderr=0 accounted=232 of 1000
            // nc-log: the store did not take 832 line(s) within 15s; writing them here instead
            // nc-log: 832 line(s) were still queued after 3s and are lost
            // ```
            //
            // 768 lost, and "writing them here instead" followed by nothing
            // written. A timeout is not an answer, so waking to look at a flag
            // costs one syscall per `POLL` and changes no verdict.
            // **Never zero, which std refuses.** When `Drop` sets the abandon
            // flag the budget drops from `DEADLINE` to `ABANDONED_ANSWER` while
            // `started.elapsed()` is already seconds, so `left` saturates to
            // zero and `set_read_timeout(Some(ZERO))` is `InvalidInput`.
            // Without the floor every abandoned exchange dies with this
            // client's own socket API as the operator-facing sentence —
            // verbatim on stderr,
            // `nc-log: the store did not take 64 line(s): could not bound the
            // answer: cannot set a 0 duration timeout` — and returns *without
            // attempting the read*, discarding an answer one syscall away and
            // forcing the verdict to `Silent` even where a partial read_bytes line
            // has arrived. One millisecond is a bound; zero is an error.
            let slice = answer_slice(left);
            if let Err(error) = stream.set_read_timeout(Some(slice)) {
                return Delivery::Silent(format!("could not bound the answer: {error}"));
            }
            match answer.read(&mut byte) {
                // A slice that expired is not a verdict: the loop goes round,
                // recomputes the budget against the flag, and stops when the
                // budget really is spent.
                //
                // **Whatever has arrived so far.** Gated on
                // `read_bytes.is_empty()`, the retry would apply only *before
                // the first byte* — after one byte of the read_bytes line, any
                // gap longer than one `POLL` would fall through to the verdict
                // below and `DEADLINE` would never apply to the rest of the
                // line. Two measured consequences, both of them the shipper
                // failing at the one thing it promises:
                //
                // ```text
                // "HTTP" … 400ms … "/1.1 500 Nope"   -> 60 recorded, 0 in the
                //                                       store, 0 on stderr
                // a valid 200 written one byte per 200ms
                //                                    -> the store took all 60
                //                                       and it was reported as
                //                                       a failure, in 207ms
                //                                       against a 10s budget
                // ```
                //
                // The first is the worse one where `Unreadable` is not kept: a
                // peer that starts answering and is *refusing* is read as
                // `Unreadable`, on the reasoning that a peer which answered may
                // hold the batch. It does not, and the lines exist nowhere. Any
                // store, proxy or loaded ingest path that emits its read_bytes
                // line in two segments trips this.
                //
                // **What this closes is the threshold, not the class.** The gap
                // that loses those lines goes from one `POLL` — 100 ms, which
                // any loaded ingest path produces — to the whole `DEADLINE`.
                // Measured: a `500` split by an eleven-second pause is still
                // `Unreadable` — kept on stderr, so the lines survive, but with
                // the store's verdict still unknown. Closing the class itself
                // needs the shipper to know whether the *body* was taken, which
                // one read_bytes line cannot say; what it has instead is a
                // bound beyond which nothing is worth waiting for.
                //
                // The bound has a price of its own, paid by the one thread that
                // drains the queue: a peer that sends one byte and stops holds
                // it for the full budget, where a retry gated on the first byte
                // reaches the same losing verdict in 100 ms. At `BATCH` lines a
                // batch that is a slow drain, and the queue drops behind it —
                // which is the trade `QUEUE`'s doc already argues for, made
                // here explicit.
                //
                // `slice < left` is what still ends the loop when the real
                // budget is gone, which is the clause that does the work.
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) && slice < left =>
                {
                    // Nothing to do: the loop goes round, recomputes the budget
                    // against the flag, and stops when the budget really is
                    // spent. Written as an arm rather than a `continue` because
                    // this is the *only* outcome that is not a verdict.
                }
                // **And the budget being spent, which is where it is spent.**
                // Checked as `if left.is_zero()` at the top of the loop, it is
                // never reached: the loop never goes round again with a spent
                // budget, because the retry arm requires `slice < left`, and
                // when `left <= POLL` they are equal. So a timeout message
                // there — written to tell a reader that the *thread draining
                // the queue* waited — is a needle nothing reaches, and every
                // expiry is reported as the raw errno instead:
                //
                // ```text
                //   the store stopped answering: Resource temporarily unavailable (os error 11)
                // ```
                //
                // That is the shape `verdict`'s own doc condemns four functions
                // up: a needle nothing exercises.
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    // **Not `Unreachable`, from here down.** The body is
                    // written: the store may well have taken every line, and
                    // writing them to stderr too would duplicate them.
                    let why = format!(
                        "the store took longer than {budget:?} to answer, and this \
                         thread drains the queue"
                    );
                    return if read_bytes.is_empty() {
                        Delivery::Silent(why)
                    } else {
                        Delivery::Unreadable(why)
                    };
                }
                Ok(0) => break,
                Ok(_) => {
                    read_bytes.push(byte[0]);
                    if byte[0] == b'\n' {
                        // **A blank line is not a read_bytes line.** An interim
                        // answer is followed by one, so reading past a
                        // `100 Continue` lands on `\r\n`, and without skipping
                        // it reports "the store answered ``, which is not an
                        // HTTP read_bytes line". A read_bytes line is never
                        // empty; skipping empties is right whether or not an
                        // interim came first, and the `STATUS_LIMIT` on the
                        // reader still bounds it.
                        if read_bytes.iter().all(u8::is_ascii_whitespace) {
                            read_bytes.clear();
                            continue;
                        }
                        break;
                    }
                }
                Err(error) => return verdict(&error, read_bytes.is_empty()),
            }
        }
        let read = read_bytes.len();
        let status = String::from_utf8_lossy(&read_bytes).into_owned();
        // **How much it took to find no newline** — reported, so that a store
        // answering with something that is not a status line is distinguishable
        // from one answering with a refusal. It is also what makes the bound
        // above testable: without it this number is unbounded.
        // **Nothing at all is not an unreadable answer, it is no answer.** The
        // peer accepted the connection and closed it without a word — a store
        // restarting, a proxy with no backend, a connection limit. Classified
        // `Unreadable` where that is not re-emitted, measured, a child
        // recording 20 lines against such a listener:
        //
        // ```text
        // 2 connection(s) made, 0 line(s) on stderr, dropped() = 0
        // ```
        //
        // **All twenty gone**, to the store and to stderr both — which is
        // exactly what this module's header says cannot happen.
        //
        // Whether the peer read the body before closing is not knowable from
        // here, so this is a choice and not a deduction: a duplicate is
        // recoverable and a loss is not, and `§18.5`'s argument for keeping the
        // data on this machine only holds if the failure path keeps it here.
        if read == 0 {
            return Delivery::Unreachable(
                "the store closed the connection without answering at all".to_owned(),
            );
        }
        if !status.ends_with('\n') {
            return Delivery::Unreadable(format!(
                "the store answered {read} byte(s) with no status line, and this reads \
                 at most {STATUS_LIMIT}"
            ));
        }
        // `HTTP/1.1 204 No Content` — the code is the second word, and anything
        // outside the 2xx range is a refusal this must not treat as success.
        // **The version, then a three-digit code in the 2xx range.**
        // `code.starts_with('2')` alone accepts `HTTP/1.1 2 whatever` and
        // `garbage 200 ok`, so a store answering something that is not HTTP
        // would have its batches discarded as though they had been taken.
        let mut words = status.split_whitespace();
        let version = words.next().unwrap_or("");
        let code = words.next().unwrap_or("");
        // **An interim answer is not the answer.** RFC 9110 lets a server send
        // a `1xx` at any time and requires a client to read past it; the real
        // status follows. Read as one line and stopped at,
        // `HTTP/1.1 100 Continue` is classified `Refused` — "it did not keep
        // the batch", which is false — and the lines are written to the store
        // *and* to stderr, the duplication `Delivery`'s own doc calls no
        // smaller a failure than a loss.
        //
        // Reported rather than followed, because following it means another
        // bounded read loop for a case no store this ships against produces, and
        // an untested second loop in the one thread that drains the queue is the
        // more expensive mistake. What matters is that it is not counted as a
        // refusal: the body went out, and whether the store kept it is unknown.
        if version.starts_with("HTTP/") && code.starts_with('1') && code.len() == 3 {
            // Marked so `answer` above can tell "read past this" from every
            // other unreadable answer, which are all final.
            return Delivery::Unreadable(format!(
                "{INTERIM}`{}`",
                as_much_as_is_a_diagnostic(status)
            ));
        }
        let accepted = version.starts_with("HTTP/")
            && code.len() == 3
            && code.chars().all(|digit| digit.is_ascii_digit())
            && code.starts_with('2');
        if accepted {
            // **The status line, and this client reads nothing after it.** A
            // store answering `200 OK` with a body saying it rejected every
            // line is `Taken` here, and the batch is discarded with nothing on
            // stderr. That is HTTP's own semantics — a 2xx means the request
            // succeeded — and it is what `Delivery`'s taxonomy claims to
            // distinguish, so it is written down rather than guessed at:
            // `Taken` means *the store answered 2xx*, not *the store has the
            // lines*. Reading a body to find out otherwise would mean parsing
            // one store's answer format in a client that is deliberately
            // ignorant of every store's, and `§18.5` picked a store that
            // answers 204 with no body at all.
            Delivery::Taken
        } else if version.starts_with("HTTP/") {
            // A real HTTP answer outside 2xx is a refusal: the store said no,
            // so it is not holding the batch and stderr is where it belongs.
            Delivery::Refused(format!(
                "the store answered `{}`",
                as_much_as_is_a_diagnostic(status)
            ))
        } else {
            // Something that is not HTTP at all, after the body went out. What
            // the store did with it is unknown.
            Delivery::Unreadable(format!(
                "the store answered `{}`, which is not an HTTP status line",
                as_much_as_is_a_diagnostic(status)
            ))
        }
    }
}

/// How many hashes a raw string's opener at `at` has, or `None` for anything
/// else.
///
/// `r"…"` and `r#"…"#`: nothing opens a comment inside one until the matching
/// quote and hashes.
#[cfg(test)]
fn raw_string_at(source: &[char], at: usize) -> Option<usize> {
    if source.get(at) != Some(&'r') {
        return None;
    }
    let mut hashes = 0_usize;
    while source.get(at + 1 + hashes) == Some(&'#') {
        hashes += 1;
    }
    (source.get(at + 1 + hashes) == Some(&'"')).then_some(hashes)
}

/// The width of a Rust character literal at `at`, or `None` for a lifetime.
///
/// The longest literal Rust has is `'\u{10FFFF}'`, at eleven characters. A `'`
/// that closes none of them is a lifetime and opens nothing.
#[cfg(test)]
fn char_literal_at(source: &[char], at: usize) -> Option<usize> {
    let escaped = source.get(at + 1) == Some(&'\\');
    let limit = if escaped { 11 } else { 3 };
    let mut offset = if escaped { 3 } else { 2 };
    while offset <= limit {
        match source.get(at + offset) {
            Some('\'') => return Some(offset + 1),
            Some('\n') | None => return None,
            Some(_) => offset += 1,
        }
    }
    None
}

/// This file's code, with the test modules and every comment gone.
///
/// **A comment pays for a call.** Three pins in this module count needles over
/// the raw file — the two above and the authority needle below — so a
/// twelve-space comment naming the call satisfies the count while the call
/// itself is gone. Measured on the refusal arm. Comments are where the next
/// reader is told what a rule holds; they are not where a rule may find its
/// evidence.
///
/// **And `//` is one spelling of a comment.** A reader that strips `//` to the
/// end of the line and nothing else lets a `/* … */` pay for the call just as
/// well — the same defect, one spelling along. Measured with the `Refused` arm
/// put back to an unbounded trim under a different binding name and the needle
/// written in a block comment: `cargo test -p nc-log` green and `check passed`.
///
/// So this is a state machine over the whole file rather than a scan of each
/// line: a line comment runs to its newline, a block comment nests, a raw
/// string ends at its own hash count, and neither opens inside a string.
#[cfg(test)]
fn the_code_this_crate_ships(source: &str) -> String {
    let ships = source
        .split_once("\n#[cfg(test)]")
        .map_or(source, |(above, _)| above);
    let text: Vec<char> = ships.chars().collect();
    let mut out = String::with_capacity(ships.len());
    let mut at = 0_usize;
    // Blanked rather than deleted, so a line number here is a line number in the
    // file and an indented needle still matches its own indentation.
    let blank = |out: &mut String, here: char| out.push(if here == '\n' { '\n' } else { ' ' });
    while at < text.len() {
        let here = text[at];
        let next = text.get(at + 1).copied();
        if here == '/' && next == Some('/') {
            while at < text.len() && text[at] != '\n' {
                blank(&mut out, text[at]);
                at += 1;
            }
            continue;
        }
        if here == '/' && next == Some('*') {
            let mut depth = 0_usize;
            while at < text.len() {
                if text[at] == '/' && text.get(at + 1) == Some(&'*') {
                    depth += 1;
                    blank(&mut out, text[at]);
                    blank(&mut out, text[at + 1]);
                    at += 2;
                    continue;
                }
                if text[at] == '*' && text.get(at + 1) == Some(&'/') {
                    depth -= 1;
                    blank(&mut out, text[at]);
                    blank(&mut out, text[at + 1]);
                    at += 2;
                    if depth == 0 {
                        break;
                    }
                    continue;
                }
                blank(&mut out, text[at]);
                at += 1;
            }
            continue;
        }
        // A raw string: `r`, hashes, a quote — and it ends at a quote followed
        // by the same number of hashes. Nothing opens a comment inside one.
        if let Some(hashes) = raw_string_at(&text, at) {
            for _ in 0..=(1 + hashes) {
                out.push(text[at]);
                at += 1;
            }
            while at < text.len() {
                let closes = text[at] == '"'
                    && (0..hashes).all(|step| text.get(at + 1 + step) == Some(&'#'));
                out.push(text[at]);
                at += 1;
                if closes {
                    for _ in 0..hashes {
                        out.push(text[at]);
                        at += 1;
                    }
                    break;
                }
            }
            continue;
        }
        if here == '\''
            && let Some(past) = char_literal_at(&text, at)
        {
            for _ in 0..past {
                out.push(text[at]);
                at += 1;
            }
            continue;
        }
        if here == '"' {
            out.push(here);
            at += 1;
            while at < text.len() {
                if text[at] == '\\' {
                    out.push(text[at]);
                    if let Some(after) = text.get(at + 1) {
                        out.push(*after);
                    }
                    at += 2;
                    continue;
                }
                let done = text[at] == '"';
                out.push(text[at]);
                at += 1;
                if done {
                    break;
                }
            }
            continue;
        }
        out.push(here);
        at += 1;
    }
    out
}

/// Every method `code` calls on the binding called `named`, in order.
///
/// The name is matched as a word, so a longer identifier ending in it is
/// not this binding.
#[cfg(test)]
fn the_methods_called_on(code: &str, named: &str) -> Vec<String> {
    let needle = format!("{named}.");
    code.match_indices(&needle)
        .filter(|(at, _)| {
            !code[..*at]
                .chars()
                .next_back()
                .is_some_and(|before| before.is_alphanumeric() || before == '_')
        })
        .map(|(at, _)| {
            code[at + needle.len()..]
                .chars()
                .take_while(|written| written.is_alphanumeric() || *written == '_')
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    /// The request this shipper sends to an IPv6 store brackets its host.
    ///
    /// **The hand-over, driven rather than pinned.** `as_an_authority` has its
    /// own corpus and `Target::parse` its own bracket test; what sits between
    /// them, held by a needle over this file alone, would be paid for by a call
    /// whose result is discarded. The one fixture that reads a real `Host:`
    /// header speaks to the loopback, where the bracket rule cannot fire. This
    /// builds the header for `[::1]` and reads it.
    ///
    /// `Host: ::1:9428` is not an authority — RFC 3986 reads the last colon as
    /// the port separator — and a store that validates the header refuses every
    /// batch for ever.
    #[test]
    fn a_request_to_an_ipv6_store_brackets_its_host() {
        for (endpoint, header) in [
            ("http://[::1]:9428/insert", "Host: [::1]:9428"),
            // Only endpoints `Target::parse` accepts: this client speaks to
            // the store on this machine (`NOCTUA.md §18.5`), so the loopback in
            // both families is the whole corpus there is — and `::1` is the one
            // the parser treats specially.
            ("http://127.0.0.1:9428/insert", "Host: 127.0.0.1:9428"),
            ("http://localhost:9428/insert", "Host: localhost:9428"),
        ] {
            let target = super::Target::parse(endpoint).expect("the endpoint parses");
            let request = target.request("{}\n");
            assert!(
                request.contains(header),
                "`{endpoint}` is sent as `{}`, and the header a store reads is `{header}`",
                request
                    .lines()
                    .find(|line| line.starts_with("Host:"))
                    .unwrap_or_default()
            );
        }
    }

    /// A host is written into `Host:` the way an authority spells it.
    ///
    /// **The parser strips an IPv6 literal's brackets on purpose**, so a header
    /// writer interpolating the parsed host directly writes it bare. Every case
    /// that reads the request uses `127.0.0.1`, so the one spelling the parser
    /// treats specially is the one this test writes.
    #[test]
    fn an_ipv6_host_is_bracketed_in_the_header() {
        assert_eq!(super::as_an_authority("127.0.0.1"), "127.0.0.1");
        assert_eq!(super::as_an_authority("localhost"), "localhost");
        assert_eq!(super::as_an_authority("::1"), "[::1]");
        assert_eq!(
            super::as_an_authority("2001:db8::1"),
            "[2001:db8::1]",
            "an address with more than one colon is one host, not a host and a port"
        );
    }

    /// `lost()` answers the counter, and not the counter minus something.
    ///
    /// **Not only `after > before`.** That driver is an expectation that moves
    /// with the thing it measures: `Shipper::lost()` wrapped in
    /// a `saturating_sub(1)` leaves it green, because one fewer than eight is
    /// still more than zero. It cannot be given an equality — it runs in a
    /// child whose stderr is deliberately full, so a wrong number arrives as
    /// *"the child ran no test"* with the panic swallowed.
    ///
    /// So the arithmetic is asked here, where a failure can be read: the
    /// accessor against the counter, before and after a rise, so a constant
    /// subtracted from either end shows up.
    ///
    /// The counter is process-wide and other tests move it, so what is compared
    /// is the accessor and the counter *at the same instant* rather than two
    /// numbers taken apart.
    #[test]
    fn the_accessor_answers_the_counter() {
        use std::sync::atomic::Ordering;
        assert_eq!(
            super::Shipper::lost(),
            super::LOST.load(Ordering::Relaxed),
            "the accessor and the counter disagree"
        );
        let before = super::LOST.load(Ordering::Relaxed);
        super::LOST.fetch_add(7, Ordering::Relaxed);
        assert_eq!(
            super::Shipper::lost(),
            before + 7,
            "seven lines were charged and the accessor moved by {}",
            super::Shipper::lost() - before
        );
        super::LOST.fetch_sub(7, Ordering::Relaxed);
    }

    /// A store's own answer is quoted back bounded, on every path that quotes
    /// it.
    ///
    /// **One named bound on one helper, on every path.** As a bare number
    /// written twice, both copies can be removed with the crate green, and a
    /// third path — the interim answer, put into `Delivery::Unreadable` and
    /// written out verbatim by `keep_on_stderr` — would have none. The read
    /// itself is bounded at `STATUS_LIMIT`, 8 KiB, so what is at stake is how
    /// much of somebody else's error page goes to stderr — through the same
    /// bounded queue the real log lines fall back to — once per failed batch.
    ///
    /// Both ends, because a bound that only refuses is a bound that could be
    /// zero: one character under is kept whole, one over is cut.
    #[test]
    fn a_quoted_answer_is_bounded_at_the_number_it_names() {
        let under = "x".repeat(super::QUOTED_ANSWER - 1);
        assert_eq!(
            super::as_much_as_is_a_diagnostic(&under),
            under,
            "an answer inside the bound was cut"
        );
        let over = "y".repeat(super::QUOTED_ANSWER + 40);
        let quoted = super::as_much_as_is_a_diagnostic(&over);
        assert_eq!(
            quoted.chars().count(),
            super::QUOTED_ANSWER,
            "an answer past the bound was quoted at {} character(s)",
            quoted.chars().count()
        );
        // **And an answer that escapes, which is where the bound can be wrong
        // by six.** `"y".repeat(…)` escapes to itself, so the assertion above
        // is baselined where the escaping cannot move it: with the bound taken
        // on the store's characters rather than on the reader's, this answer
        // reaches stderr at 720.
        let hostile = "\r".repeat(super::QUOTED_ANSWER);
        let quoted = super::as_much_as_is_a_diagnostic(&hostile);
        assert!(
            quoted.chars().count() <= super::QUOTED_ANSWER,
            "an answer of control characters was quoted at {} character(s), and the \
             bound is on what a reader sees",
            quoted.chars().count()
        );
        assert!(
            !quoted.contains('\r'),
            "a control character reached the quote raw: {quoted:?}"
        );
        // And it is the same number the path bound uses, which is the reason
        // both are named rather than written out.
        assert_eq!(super::QUOTED_ANSWER, super::LONGEST_PATH);
        // Trimmed, because a store's answer arrives with its line ending on it.
        assert_eq!(
            super::as_much_as_is_a_diagnostic("  503 Busy \r\n"),
            "503 Busy"
        );

        // **And every path that quotes an answer goes through it.** The doc
        // above says "on every path that quotes it", and a test of the helper
        // alone does not hold that: putting an unbounded trim back into the
        // refusal arm leaves the whole crate green. Three quoting paths — the
        // interim answer, the refusal, and the answer that is not HTTP at all —
        // and this assertion assembles its needle because the test lives in the
        // file it reads.
        let here = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ship.rs"))
            .expect("this file is readable");
        let shipped = super::the_code_this_crate_ships(&here);
        // **And no arm trims the answer itself.** A count of the helper's calls
        // over the file is paid for by a discarded call beside a live arm put
        // back to an unbounded trim; and a needle naming the local `status`
        // turns red on a rename — a refactor that changes nothing. Too weak and
        // too strong in one line.
        //
        // **And `trim()` is one spelling of a trim.** Zero occurrences of
        // `status.trim()` over the *raw* file is wrong twice over:
        // `status.trim_end()` is the same unbounded quote under another name,
        // and a `//` comment naming the bound pays for a live arm that has
        // stopped calling it. Measured — the refusal arm rewritten as
        // `status.trim_end()` beside a comment naming the bounded call —
        // `check passed`, and the only line that moved in the transcript was a
        // source-line count.
        //
        // So what is read is every method this file calls **on** the answer,
        // over comment-stripped code, compared as a list. A trim by any
        // spelling is a row that arrived; so is anything else new. The reader
        // below asks the answer three things, and the bound is the one place
        // `trim` is among them.
        //
        // **And the answer, not a buffer that shares its name.** A census whose
        // rows are mostly about something else is one an ordinary rename turns
        // red and a real trim hides inside: with the `Vec<u8>` the socket is
        // read into also called `status`, six of nine rows are `push`, `clear`,
        // `iter`, `len` and two `is_empty`s on the buffer, which the answer
        // then shadows. The buffer is `read_bytes`.
        let mut asked = super::the_methods_called_on(&shipped, "status");
        asked.sort();
        assert_eq!(
            asked,
            ["as_ref", "ends_with", "split_whitespace"],
            "this file asks a store's answer something it is not written down as asking. A \
             trim outside the bound — under any spelling — is what an unbounded quote looks \
             like: up to `STATUS_LIMIT` bytes of somebody else's error page on stderr, once \
             per failed batch"
        );
        // **The three arms, whole — not a count and not a name.** Both of the
        // rules above key on the binding called `status`, and a binding is not
        // the answer: a second one over the same bytes — `read_bytes` is still
        // in scope — carries an unbounded quote with the census and the count
        // both green, and the count itself is paid by a call whose result is
        // discarded.
        //
        // What the three arms have in common is their whole text, so that is
        // what is compared: an arm rewritten to quote anything else, under any
        // name, is a line that does not match.
        let bounded = format!("{}(status)", "as_much_as_is_a_diagnostic");
        assert_eq!(
            shipped.matches(&bounded).count(),
            3,
            "{} arm(s) quote a store's answer through the bound, and there are three",
            shipped.matches(&bounded).count()
        );
        for arm in [
            format!(
                "            return Delivery::Unreadable(format!(\n                \"{{INTERIM}}`{{}}`\",\n                {bounded}\n            ));"
            ),
            format!(
                "            Delivery::Refused(format!(\n                \"the store answered `{{}}`\",\n                {bounded}\n            ))"
            ),
            format!(
                "            Delivery::Unreadable(format!(\n                \"the store answered `{{}}`, which is not an HTTP status line\",\n                {bounded}\n            ))"
            ),
        ] {
            assert_eq!(
                shipped.matches(&arm).count(),
                1,
                "an arm that quotes a store's answer is not the one written down. Read it \
                 whole: a second binding over the same bytes carries an unbounded quote \
                 with every count above still green, and so does a call whose result is \
                 thrown away"
            );
        }
    }

    use super::{QUEUE, Shipper, Target};

    #[test]
    fn an_endpoint_is_read_into_its_three_parts() {
        // **A bracketed IPv6 authority is one host.** Splitting on the first
        // colon makes `http://[::1]:9428/insert` unparseable and names
        // `:1]:9428` in the refusal — a fragment the parser invents. `§18.5`
        // puts the store on this machine, and `[::1]` is how that is written.
        for (endpoint, host, port) in [
            ("http://[::1]:9428/insert", "::1", 9428_u16),
            (
                "http://[::ffff:127.0.0.1]:9428/insert",
                "::ffff:127.0.0.1",
                9428,
            ),
        ] {
            let target = Target::parse(endpoint)
                .unwrap_or_else(|why| panic!("`{endpoint}` was refused: {why}"));
            assert_eq!(target.host, host);
            assert_eq!(target.port, port);
        }
        // And a bracket that never closes is refused for what it is.
        let why = Target::parse("http://[::1:9428/insert").expect_err("no closing bracket");
        assert!(why.contains("never closes it"), "{why}");

        assert_eq!(
            Target::parse("http://127.0.0.1:9428/insert/jsonline"),
            Ok(Target {
                host: "127.0.0.1".to_owned(),
                port: 9428,
                path: "/insert/jsonline".to_owned(),
            })
        );
    }

    /// A control character is refused **for what it does**, not merely refused.
    ///
    /// **The list above asserts only that a refusal names the endpoint.** Which
    /// rule refused it is decided by nothing there, and since every refusal
    /// escapes the endpoint the same way, one rule can stand in for another
    /// without the list noticing — measured without this test,
    /// `if let Some(bad) = None::<char>` leaves the whole suite green. Every
    /// control character is *also* outside `ROUTE`, so the route rule catches
    /// the same endpoint and names it back identically; what is lost is the
    /// sentence that tells a reader their endpoint would write header lines into
    /// every request, which is the only part a person can act on.
    #[test]
    fn a_control_character_is_refused_for_the_header_lines_it_would_write() {
        for bad in [
            "http://127.0.0.1:9428/insert\r\nX-Evil: yes",
            "http://127.0.0.1:9428/insert\nX-Evil: yes",
            "http://127.0.0.1:9428/insert\u{7f}",
            "http://127.0.0.1:9428/\tinsert",
        ] {
            let error = Target::parse(bad).expect_err("should be refused");
            assert!(
                error.contains("would write extra header"),
                "`{bad:?}` is refused, and not for the reason a reader can act on: {error}"
            );
        }
    }

    #[test]
    fn an_endpoint_this_client_cannot_speak_to_is_refused_with_the_reason() {
        for bad in [
            "https://example.invalid:443/x",
            "127.0.0.1:9428/x",
            "http://127.0.0.1/x",
            "http://:9428/x",
            "http://127.0.0.1:notaport/x",
            // A CRLF in the path writes extra header lines into every request.
            "http://127.0.0.1:9428/insert\r\nX-Evil: yes",
            "http://127.0.0.1:9428/insert\nX-Evil: yes",
            // **And a query string, which the store reads.** This is the one
            // `&str` on the crate's public surface, and VictoriaLogs takes
            // `extra_fields` on `/insert/jsonline` — so a caller that builds its
            // endpoint with `format!` writes arbitrary text onto every record,
            // past the type wall and past the source gate. Measured with an
            // email address, which arrived verbatim as a field of the record.
            "http://127.0.0.1:9428/insert/jsonline?extra_fields=who=ana@example.com",
            "http://127.0.0.1:9428/insert/jsonline?debug=1",
            "http://127.0.0.1:9428/insert/jsonline?_msg_field=nothing",
            "http://127.0.0.1:9428/insert/jsonline#fragment",
            "http://127.0.0.1:9428?extra_fields=x=y",
            // **One case per site that would interpolate raw.** The two
            // `is_unsafe_in_a_message` characters reach the query-string arm
            // and the route-character arm without passing the control guard,
            // and every other row with a quote in it is caught by the *host*
            // arm first — so without these the assertion below is real and
            // never asked the question.
            "http://127.0.0.1:9428/insert?f=a\"b",
            "http://127.0.0.1:9428/insert/a\"b",
            // **And a host that is not this machine.** `§18.5`'s argument for a
            // self-hosted store is that there is nowhere else for the lines to
            // go, and the endpoint is the one `&str` left on this crate's
            // public surface — a caller building it with `format!` could send
            // every line of every event to a host of its choosing. Not the
            // twelve-character residual the wall discloses: all of it.
            "http://example.invalid:9428/insert/jsonline",
            "http://10.0.0.5:9428/insert/jsonline",
            "http://[2001:db8::1]:9428/insert/jsonline",
            "http://127.0.0.1.example.invalid:9428/insert/jsonline",
            // **And a path carrying data**, which the query-string refusal
            // above does not cover. Measured against a real listener:
            // `POST /insert/jsonline/ana.silva@example.com HTTP/1.1`, in every
            // request the shipper makes, kept by the store's own access log,
            // with the `?` taken apart at length and the `/` beside it left to
            // `format!`.
            "http://127.0.0.1:9428/insert/jsonline/ana.silva@example.com",
            "http://127.0.0.1:9428/insert/jsonline/who is ana",
            "http://127.0.0.1:9428/insert/jsonline/%41%42",
            // The two `is_unsafe_in_a_message` characters the control-character
            // guard does not name, which would otherwise reach every refusal
            // but one raw.
            "http://exa\"mple.invalid:9428/insert",
            "http://exa\\mple.invalid:9428/insert",
        ]
        .into_iter()
        .map(str::to_owned)
        // **And one character past the bound.** Every case above is refused by
        // another rule — the path ones by the *character* rule, for an `@`, a
        // space, a `%` — and none is longer than the bound, so without this row
        // the comparison that reads `LONGEST_PATH` refuses nothing: the
        // constant's own pin holds the number, not its use. The rule this file
        // states is that a route is a *short* thing, and its own comment quotes
        // a request line built by `format!` from an email address.
        //
        // **Written out, not derived from the constant.** `LONGEST_PATH + 1`
        // moves with whatever the constant becomes, which is an expectation
        // that shrinks with the thing it measures: a plant of the constant
        // passes under it. The number the paragraph argues for is asserted in
        // `the_longest_route_is_the_number_its_paragraph_argues_for`.
        .chain(core::iter::once(format!(
            "http://127.0.0.1:9428/{}",
            "a".repeat(121)
        ))) {
            let bad = bad.as_str();
            let error = Target::parse(bad).expect_err("should be refused");
            // **What it names, not how long it is.** `len() > 30` over all five
            // cases is the length heuristic `gates.rs` condemns elsewhere, and
            // swapping any two messages passes it. A refusal has to name the
            // endpoint it refused, or a reader has to guess which one of five
            // it was about.
            // An endpoint holding a control character is named back escaped,
            // for the reason `Target::parse` gives; anything else verbatim.
            let visible: String = bad.chars().map(nc_canon::shown).collect();
            assert!(
                error.contains(&visible) || error.contains(visible.trim_start_matches("http://")),
                "`{bad:?}` was refused without naming it: {error}"
            );
            // **And nothing in it can end a line or forge a field.** Escaping
            // the endpoint once for every refusal leaves two names beside it —
            // the host and the port — to be escaped too, or one message escapes
            // on the left and interpolates on the right, and the assertion
            // above, which only asks whether the endpoint is named, cannot see
            // it. `nc-ids`' `lib.rs` asks exactly this of all three of its
            // refusal renderings: one rule, two crates.
            assert!(
                !error.chars().any(nc_canon::is_unsafe_in_a_message),
                "`{bad:?}`'s refusal carries a character that ends a line or forges a \
                 field: {error:?}"
            );
        }
    }

    #[test]
    fn the_endpoint_this_repository_uses_is_still_accepted() {
        // The other half of the refusals above: the shape `cargo xtask dev`
        // prints and `docker-compose.yml` publishes has to go on working, or
        // the rule is "no endpoint at all".
        for good in [
            "http://127.0.0.1:9428/insert/jsonline",
            "http://[::1]:9428/insert/jsonline",
            "http://localhost:9428/insert/jsonline",
            // Every other spelling of this machine, so the rule is "this
            // machine" and not "one address".
            "http://127.0.0.53:9428/insert/jsonline",
            "http://[::ffff:127.0.0.1]:9428/insert/jsonline",
            // A path may hold anything else, including the characters that
            // *look* like a query and are not one.
            "http://127.0.0.1:9428/insert/jsonline;v=1",
            // A route made of route characters, at the length a route has.
            "http://127.0.0.1:9428/insert/json_lines-v2.0~beta:1",
        ]
        .into_iter()
        .map(str::to_owned)
        // **And the last route the bound admits**, so the bound is a bound
        // and not a ceiling nobody has reached: one character more is in the
        // refusal list above, and a repair that closed either end alone
        // would be the identity on the other. The `/` is added back after the
        // measurement, so a route the bound admits reaches the request line one
        // character longer than the number here. Written out for the reason the
        // refusal beside it is.
        .chain(core::iter::once(format!(
            "http://127.0.0.1:9428/{}",
            "a".repeat(120)
        ))) {
            let good = good.as_str();
            Target::parse(good).unwrap_or_else(|error| panic!("`{good}`: {error}"));
        }
    }

    /// The longest route is the number its own paragraph argues for.
    ///
    /// The two length cases beside this one are written out rather than
    /// derived, so they pin the *behaviour* at 120 and 121 characters; this
    /// pins the constant to the argument. Measured with the cases derived from
    /// the constant instead, `LONGEST_PATH = 100_000` and `LONGEST_PATH = 119`
    /// are both green — the expectation moves with the thing it measures, which
    /// is the shape this repository names.
    #[test]
    fn the_longest_route_is_the_number_its_paragraph_argues_for() {
        assert_eq!(
            super::LONGEST_PATH,
            120,
            "`LONGEST_PATH` is not what its paragraph argues for: a route is a short \
             thing made of route characters, and this repository's own longest route — \
             `/insert/json_lines-v2.0~beta:1` — is thirty-one characters, so the bound \
             leaves room for a route four times that and not for a payload"
        );
    }

    /// The shutdown budget is the number `FLUSH`'s paragraph states, and `Drop`
    /// spends that one.
    ///
    /// **Both halves, because either alone is satisfied by the other's
    /// absence.** With the constants pinned and the expression that adds them
    /// not, `Instant::now() + FLUSH + GRACE * 10`
    /// leaves `cargo test -q -p nc-log` green and `check passed`, measured, so
    /// a shutdown documented at fifteen seconds could cost forty-two per
    /// shipper with every timing assertion in this file still true. The number
    /// is written out rather than added up here, for the
    /// reason `the_longest_route_is_the_number_its_paragraph_argues_for` gives.
    #[test]
    fn the_network_budget_of_a_shutdown_is_the_number_its_paragraph_argues_for() {
        assert_eq!(
            super::the_network_budget_of_a_shutdown(),
            core::time::Duration::from_secs(15),
            "the network half of a shutdown is not the fifteen seconds `FLUSH`'s own \
             paragraph states, and `Log::open` is public, so every shipper in the \
             process pays the difference"
        );
        let dropping = the_body_of("fn drop(&mut self) {");
        assert!(
            dropping.contains("Instant::now() + the_network_budget_of_a_shutdown()"),
            "`drop` writes its own deadline out of the constants again, so the budget a \
             test can read is not the budget a process spends: {dropping}"
        );
    }

    /// The two sentences a shutdown writes name the number they are about.
    #[test]
    fn the_two_sentences_a_shutdown_writes_name_their_numbers() {
        let still = super::the_lines_still_queued_at_the_end(41);
        assert!(
            still.contains("41 line(s)") && still.contains("lost"),
            "the last sentence of a shutdown does not say how many it lost: {still}"
        );
        assert!(
            still.ends_with('\n'),
            "a line the fallback writes has to end its own line: {still:?}"
        );
        assert_eq!(
            super::what_the_queue_refused(0),
            None,
            "a queue that refused nothing still says so, which teaches a reader to read \
             past this sentence"
        );
        let refused = super::what_the_queue_refused(7).expect("seven is more than none");
        assert!(
            refused.contains("7 line(s)") && refused.contains("queue was full"),
            "the queue-full sentence does not say how many or why: {refused}"
        );
    }

    /// `Drop` still says both of the last two things a shutdown can say.
    ///
    /// **Source rather than behaviour, and the reason is written down.** The
    /// second wait ends when the drain thread stops, and the abandon flag
    /// exists to make the drain thread stop — so no fixture can reach that arm
    /// without breaking the flag it is written for. Measured without this test,
    /// the whole `match self.finished.recv_timeout(left())` replaced
    /// by `let _ = self.finished.recv_timeout(left());`: green
    /// and `check passed`. `Drop`'s queue-full arm is the same shape from the
    /// other side — `if lost > 0 && self.lines.is_some()`, with `self.lines`
    /// already taken, is green as well.
    #[test]
    fn the_last_two_sentences_of_a_shutdown_are_still_said() {
        let dropping = the_body_of("fn drop(&mut self) {");
        // **The whole arm, not the call inside it.** A call stays on its own
        // line whichever arm it sits in, so with the call alone pinned the two
        // arms of this `match` could be swapped with every count in the run
        // byte-identical. Measured: the ordinary abandon path then reports a
        // loss that did not happen, *"0 line(s) were still queued after 3s and
        // are lost"*, while the path that genuinely runs out of grace — the
        // only one that loses lines, and the only reason the sentence exists —
        // says nothing, and `Log::lost()` never hears about them either.
        //
        // The sibling below pins its whole `if let … { … }` block for the same
        // reason, which is why that one cannot be moved.
        assert!(
            dropping.contains(
                "Err(RecvTimeoutError::Timeout) => {\n                        to_stderr_line(the_lines_still_queued_at_the_end(self.still_waiting()));\n                    }",
            ),
            "the sentence about lines still queued is no longer on the arm that runs out \
             of grace — on the ordinary arm it reports a loss that did not happen, and \
             the arm that does lose lines then says nothing and `LOST` never hears about \
             them either: {dropping}"
        );
        assert!(
            dropping.contains(
                "if let Some(said) = what_the_queue_refused(self.dropped.swap(0, Ordering::Relaxed)) {\n            to_stderr_line(said);\n        }",
            ),
            "`Drop` no longer reports the drops that outlived the last batch, or reports \
             them without taking them out of the counter, or guards the report with \
             something else — `self.lines` is taken four lines above, so a guard reading \
             it is a report that never happens: {dropping}"
        );
        // And the counter the first of those two sentences is built from: the
        // give-up branch is where a shutdown accounts for the lines it diverts,
        // and `handled` is what turns them from queued into handled.
        // The loop is in `to`, which `start` is one line of.
        let shipping = the_body_of("fn to(target: Target)");
        assert_eq!(
            shipping
                .matches("handled.fetch_add(batch.len() as u64, Ordering::Relaxed);")
                .count(),
            2,
            "the shipping loop counts what it handed on in one place and not two: the \
             give-up branch is where a shutdown accounts for the lines it diverts to \
             stderr, and without it `still_waiting` names lines that did reach a reader"
        );
    }

    /// A verdict keeps its lines exactly when it has a reason to give.
    ///
    /// **The call site's guard is a constant, and this is what says so out
    /// loud.** `if outcome.keep_on_stderr()` sits
    /// inside `if let Some(why) = outcome.why()`, and the two answer for the
    /// same four verdicts —
    /// measured, `if outcome.keep_on_stderr() || !batch.is_empty()` (the batch
    /// always holds at least the line that entered the loop)
    /// leaves `cargo test -q -p nc-log` green. Removing the call outright is
    /// caught only by `dead_code`, which is the compiler noticing a function
    /// has no caller rather than a test noticing behaviour.
    ///
    /// `a_store_that_answered_does_not_have_its_batch_written_out_as_well`
    /// records what a collapse to `!Taken` costs. `keep_on_stderr` is `!Taken`
    /// by decision — the `Unreadable` row keeps its lines deliberately — and
    /// what this refuses is the collapse happening *quietly*: the day a fifth
    /// verdict makes the two predicates differ, this fails and names the call
    /// site that has to be read again, rather than the note beside it starting
    /// to lie.
    #[test]
    fn every_delivery_keeps_its_lines_exactly_when_it_has_a_reason() {
        let why = "the store said no".to_owned();
        for outcome in [
            super::Delivery::Taken,
            super::Delivery::Unreachable(why.clone()),
            super::Delivery::Refused(why.clone()),
            super::Delivery::Silent(why.clone()),
            super::Delivery::Unreadable(why),
        ] {
            assert_eq!(
                outcome.why().is_some(),
                outcome.keep_on_stderr(),
                "`{outcome:?}` answers one of `why` and `keep_on_stderr` and not the \
                 other, so the note the drain thread writes beside the lines — \"so they \
                 are here too\" — is said where no line was written, or the lines are \
                 written with no note. The call site guards one with the other"
            );
        }
    }

    /// The part of this file a source assertion is about.
    ///
    /// Named once: three tests read a function's text, and each of them getting
    /// the split wrong in its own way is three chances to read the whole file
    /// and call it the body of one function.
    fn the_body_of(signature: &str) -> &'static str {
        // Everything before the first test module, so a signature this test
        // passes as a literal is not itself one of the matches counted below.
        let source = include_str!("ship.rs")
            .split("\n#[cfg(test)]\n")
            .next()
            .expect("this file has a test module");
        assert_eq!(
            source.matches(signature).count(),
            1,
            "`{signature}` names more than one function of this file, so a test reading \
             its body is reading whichever came first"
        );
        source
            .split(signature)
            .nth(1)
            .unwrap_or_else(|| panic!("`{signature}` is a function of this file"))
            // A function of an `impl` block ends at the first close-brace back
            // at four spaces; everything inside it is deeper than that.
            .split("\n    }\n")
            .next()
            .expect("a function body ends where its brace closes")
    }

    /// A read slice is never zero and never longer than one `POLL`.
    ///
    /// **Zero is what `std` refuses**, and the arm that reports the refusal
    /// turns this client's own socket API into the operator's sentence: measured
    /// on stderr, `nc-log: the store did not take 64 line(s): could not bound
    /// the answer: cannot set a 0 duration timeout`, returned without attempting
    /// the read at all. `left` saturates to zero on every abandoned exchange —
    /// `Drop` sets the flag, the budget falls from `DEADLINE` to
    /// `ABANDONED_ANSWER`, and `started.elapsed()` is already past it — so this
    /// is the ordinary path at shutdown and not an edge case.
    ///
    /// The ceiling matters for the same reason from the other side: a slice
    /// longer than `POLL` is an abandoned exchange that does not look at the
    /// flag until it expires.
    #[test]
    fn a_read_slice_is_bounded_at_both_ends() {
        use core::time::Duration;

        for left in [
            Duration::ZERO,
            Duration::from_nanos(1),
            Duration::from_millis(1),
            super::POLL,
            Duration::from_secs(12),
            Duration::MAX,
        ] {
            let slice = super::answer_slice(left);
            assert!(
                !slice.is_zero(),
                "a slice of zero is `InvalidInput`, and the answer is never read: {left:?}"
            );
            assert!(
                slice <= super::POLL,
                "a slice past `POLL` is an abandoned exchange that keeps waiting: {left:?}"
            );
        }
        // And inside the bounds it is the budget, not a constant: a function
        // that always answered `POLL` satisfies both assertions above.
        assert_eq!(
            super::answer_slice(Duration::from_millis(40)),
            Duration::from_millis(40)
        );
    }
    /// A drop is counted **and said**, on the process's real stderr.
    ///
    /// **A counter with a test is not a sentence with
    /// one.** `a_full_queue_drops_and_counts_rather_than_blocking`
    /// reads `shipper.dropped()` and then leaks the shipper, so the arm that
    /// turns that number into a line an operator can see never runs in it, and
    /// without this test it runs nowhere in the suite: deleting
    /// the `to_stderr_line` call inside it leaves everything else green, and a
    /// dropped line is then a number nobody is looking at — the process that
    /// lost the lines is the only one that knows.
    ///
    /// A child process, for the reason the `/dev/full` pair gives: this needs
    /// the harness's real stderr, and a test cannot redirect it without
    /// disturbing every other test in the binary. Here it is a pipe the parent
    /// reads.
    ///
    /// What it observes is the drain thread's report — the one that fires while
    /// the process runs. `Drop`'s arm writes the same sentence for the drops
    /// that outlive the last batch, and no observation can tell the two apart
    /// from outside: this holds the drain thread's arm,
    /// and `the_last_two_sentences_of_a_shutdown_are_still_said` holds the call
    /// site `Drop` makes.
    #[test]
    fn a_dropped_line_is_reported_and_not_only_counted() {
        #[expect(
            clippy::disallowed_methods,
            reason = "a tool this gate drives; its output is read, not printed"
        )]
        let child = std::process::Command::new(std::env::current_exe().expect("this binary"))
            // Filter first, `--exact` after, and the module's real path: each
            // of those three, got wrong, makes a child test vacuous, and the
            // assertion on "1 passed" below is what catches a fourth.
            .args([
                "ship::tests::the_child_that_fills_its_queue",
                "--exact",
                // Without this libtest swallows stderr into its capture buffer
                // and the pipe below stays empty however the arm behaves.
                "--nocapture",
            ])
            .env("NC_LOG_DROP_CHILD", "1")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("the child starts");
        let said = child.wait_with_output().expect("the child ends");
        let told = String::from_utf8_lossy(&said.stdout);
        let complained = String::from_utf8_lossy(&said.stderr);
        assert!(
            told.contains("1 passed"),
            "the child ran no test, so this proves nothing about the report: {told}"
        );
        assert!(
            complained.contains("line(s) were dropped because the queue was full"),
            "the queue filled, lines were dropped, and the process said nothing an \
             operator could read. What reached stderr was: {complained}"
        );
    }

    #[test]
    fn the_child_that_fills_its_queue() {
        // Only the child runs the body; the parent's copy returns at once, so
        // this costs an ordinary run nothing.
        if std::env::var_os("NC_LOG_DROP_CHILD").is_none() {
            return;
        }
        // The same arrangement as `a_full_queue_drops_and_counts_rather_than_blocking`,
        // and for the same reason: a socket this process owns, accepted and
        // never read, so the writes block and the queue behind them fills
        // without the network being part of the question.
        let deaf = std::net::TcpListener::bind("127.0.0.1:0").expect("a port of our own");
        let port = deaf.local_addr().expect("it is bound").port();
        let (accepted, taken) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = accepted.send(deaf.accept().map(|(stream, _)| stream));
        });
        let shipper = Shipper::to(Target {
            host: "127.0.0.1".to_owned(),
            port,
            path: "/insert".to_owned(),
        })
        .expect("the thread starts");
        let padding = "x".repeat(4 * 1024);
        for index in 0..QUEUE * 3 {
            shipper.send(format!("{{\"n\":{index},\"pad\":\"{padding}\"}}"));
            if shipper.dropped() > 0 {
                break;
            }
        }
        assert!(
            shipper.dropped() > 0,
            "the queue never filled, so this proves nothing about the report"
        );
        // **Closed, not held.** Held open the drain thread stays blocked in its
        // write, `Drop` pays `FLUSH + GRACE` — fifteen seconds — and the arm
        // under test is reached at the end of that. Closed, the blocked write
        // fails at once, the drain thread finishes the batch it is holding, and
        // the report follows immediately.
        drop(
            taken
                .recv_timeout(core::time::Duration::from_secs(5))
                .expect("the shipper connected"),
        );
        drop(shipper);
        // The writer thread is a thread: the sentence is queued, not written,
        // when `drop` returns. `flush_stderr` is what the process waits on, and
        // it is bounded.
        super::flush_stderr(core::time::Duration::from_secs(3));
    }

    #[test]
    fn a_full_queue_drops_and_counts_rather_than_blocking() {
        // The property the whole component rests on: a log store that fell
        // behind must not be able to stop the platform.
        //
        // **A listener that accepts and never reads**, which is a property of
        // this process rather than of the network the machine is on.
        //
        // **Not TEST-NET-1.** Aimed at `192.0.2.1:9428` — guaranteed unroutable
        // — the drain thread would park in `connect` for `CONNECT_TIMEOUT` and
        // the queue would fill. That is a fact about the host's routing table:
        // with a default route the connect hangs and the test passes; with
        // none, `connect` fails instantly, the drain keeps up, and it fails.
        // Measured, same tree and same binary, three runs each:
        //
        // ```text
        // with a default route:   3/3 ok.     4.01s
        // no route at all:        3/3 FAILED. 0.01s
        //   the queue never filled, so this proves nothing about dropping
        // ```
        //
        // `127.0.0.1:1` fails the same way, because there the connect is
        // *refused* instantly; TEST-NET-1 narrows the set of hosts the test
        // fails on rather than removing it. A second machine with no default
        // route, a REJECT rule or an ICMP-unreachable path is inside that set,
        // and so is a CI runner with egress filtered — and this is a `nc-log`
        // unit test, so it runs under `--offline` too.
        //
        // A socket this test owns removes the network from the question
        // entirely: it accepts the connection and never reads a byte, so the
        // kernel's receive buffer fills, the shipper's write blocks, and the
        // queue behind it fills for a reason this process arranged. Built rather
        // than parsed because `Target::parse` speaks to this machine only
        // (`§18.5`).
        let deaf = std::net::TcpListener::bind("127.0.0.1:0").expect("a port of our own");
        let port = deaf.local_addr().expect("it is bound").port();
        // Accepted here and handed back, so the *main* thread owns it for the
        // whole test. A listener that closes the connection makes every write
        // fail rather than block, and a failing write goes to the stderr
        // fallback — which is correct behaviour and floods the run.
        let (accepted, taken) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = accepted.send(deaf.accept().map(|(stream, _)| stream));
            // The listener itself is dropped here; the accepted connection is
            // not, and it is the one that matters.
        });
        let shipper = Shipper::to(Target {
            host: "127.0.0.1".to_owned(),
            port,
            path: "/insert".to_owned(),
        })
        .expect("the thread starts");
        // Padded, because the socket buffers this has to overflow are megabytes
        // and `QUEUE * 3` short lines are a few tens of kilobytes.
        let padding = "x".repeat(4 * 1024);
        // Stops at the first drop rather than sending a fixed number: what is
        // being shown is that the queue *has a bottom*, and every send past the
        // one that proves it is time a unit test does not need to spend.
        let mut sent = 0_usize;
        for index in 0..QUEUE * 3 {
            shipper.send(format!("{{\"n\":{index},\"pad\":\"{padding}\"}}"));
            sent += 1;
            if shipper.dropped() > 0 {
                break;
            }
        }
        assert!(
            shipper.dropped() > 0,
            "the queue never filled after {sent} line(s), so this proves nothing about \
             dropping"
        );
        // Taken only now: the shipper connects when it first has something to
        // send, so asking for the connection before the loop asks before there
        // is one. It has been sitting in the channel — which is what kept it
        // open, and open-but-unread is what makes the writes block.
        drop(
            taken
                .recv_timeout(core::time::Duration::from_secs(5))
                .expect("the shipper connected"),
        );
        // **Leaked rather than dropped.** `Drop` waits `FLUSH` and then `GRACE`
        // for a store that is never going to read — fifteen seconds of a unit
        // test's time to shut down a thread the process is about to exit
        // anyway. What is under test is the counter, and it has been read.
        std::mem::forget(shipper);
    }
}

#[cfg(test)]
mod hostile_store_tests {
    use super::{
        BATCH, CONNECT_TIMEOUT, DEADLINE, FLUSH, GIVE_UP_AFTER, GRACE, QUEUE, STATUS_LIMIT,
        Shipper, Target,
    };

    /// Held by every test that moves `SENT`, `WRITTEN` or `LOST`.
    ///
    /// **Those three are process-wide and the harness runs tests in
    /// parallel.** `a_backlog_is_charged_to_lost_once_and_not_once_per_shutdown`
    /// reads two of them as absolutes across a window in
    /// which `every_line_the_fallback_takes_is_written_queued_or_counted` is
    /// handing over four thousand lines on another thread — measured
    /// unserialised, 2 failures in 25 runs of the test binary and 2 of 7
    /// full `cargo xtask check` runs, one of them on an unmodified tree. A gate
    /// that fails 8% of the time for no reason is a gate people learn to
    /// re-run, which is the same as not having it.
    ///
    /// Serialising them costs a few seconds and buys a verdict that means what it
    /// says. Poisoning is ignored deliberately: a panic in one of these tests has
    /// already failed the run, and turning it into a second failure in an
    /// unrelated test would send a reader to the wrong place.
    static COUNTERS: std::sync::Mutex<()> = std::sync::Mutex::new(());
    use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::io::{Read as _, Write as _};
    use std::net::TcpListener;

    #[test]
    fn the_last_line_a_process_writes_reaches_the_store() {
        // **A program that records one line and returns delivers nothing
        // without `Drop`'s flush.** With the shipper's thread detached and its
        // handle discarded, the queue goes with the process — measured against
        // a healthy store on loopback, one `journal.broken` line and **zero**
        // delivered, with `dropped()` still zero because it is not a
        // queue-drop. The lines most likely to be lost are the ones a process
        // emits just before it stops, which is the whole reason a log store
        // exists.
        //
        // **In a child process, because a scope ending is not a process
        // ending.** Written as `{ let shipper = …; }`, this passes with the
        // flush deleted: the thread outlives the scope and has all the time it
        // needs. Only a real exit tests a real exit.
        let (port, seen) = answering("HTTP/1.1 204 No Content\r\n\r\n");
        #[expect(
            clippy::disallowed_methods,
            reason = "a tool this gate drives; its output is read, not printed"
        )]
        let child = std::process::Command::new(std::env::current_exe().expect("this binary"))
            .args([
                "ship::hostile_store_tests::the_child_that_records_one_line_and_returns",
                "--exact",
                "--nocapture",
            ])
            .env("NC_LOG_EXIT_CHILD", port.to_string())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("the child starts");
        let said = child.wait_with_output().expect("the child ends");
        let told = String::from_utf8_lossy(&said.stdout);
        // The same guard the stderr child carries, for the same reason: a filter
        // that matches nothing runs zero tests and exits 0.
        assert!(
            told.contains("1 passed"),
            "the child ran no test, so this proves nothing: {told}"
        );

        let request = seen
            .recv_timeout(core::time::Duration::from_secs(20))
            .expect("the last line the child wrote did not reach the store");
        assert!(request.contains("journal.broken"), "{request}");
        assert!(request.starts_with("POST /insert HTTP/1.1"), "{request}");
    }

    #[test]
    fn the_child_that_records_one_line_and_returns() {
        // Only the child runs the body; the parent's copy returns at once, so
        // this costs nothing in an ordinary run.
        let Some(port) = std::env::var_os("NC_LOG_EXIT_CHILD") else {
            return;
        };
        let port = port.to_string_lossy().into_owned();
        let shipper =
            Shipper::start(&format!("http://127.0.0.1:{port}/insert")).expect("a valid endpoint");
        shipper.send("{\"_msg\":\"journal.broken\"}".to_owned());
        // And nothing else: this returns, the test harness finishes, and the
        // process ends. Whether the line arrives is entirely `Drop`'s doing.
    }

    /// How many lines one request carries, read whole.
    ///
    /// The headers name the body's length, so the end of the body is where the
    /// request stops growing: read until the peer stops writing, which it does
    /// after the body because the shipper sends `Connection: close`.
    ///
    /// **One copy, because two stores read the same wire.** The second store
    /// below needs exactly what `counting_store` reads — a second transcription
    /// of a hand-rolled HTTP reader is two fixtures that can disagree about
    /// what the shipper sent.
    fn the_lines_in_one_request(stream: &mut std::net::TcpStream) -> Option<usize> {
        let mut request = Vec::new();
        let mut chunk = [0_u8; 4096];
        loop {
            match stream.read(&mut chunk) {
                Ok(0) | Err(_) => break None,
                Ok(read) => {
                    request.extend_from_slice(&chunk[..read]);
                    let text = String::from_utf8_lossy(&request);
                    if let Some((head, body)) = text.split_once("\r\n\r\n")
                        && let Some(declared) = head
                            .lines()
                            .find_map(|line| line.strip_prefix("Content-Length: "))
                        && let Ok(declared) = declared.trim().parse::<usize>()
                        && body.len() >= declared
                    {
                        break Some(body.lines().filter(|l| !l.is_empty()).count());
                    }
                }
            }
        }
    }

    /// A store that serves many connections, counting the lines each body holds.
    ///
    /// `answering` accepts once. Everything about a shutdown is about the
    /// *second* batch and the ones after it, so a one-shot server cannot see any
    /// of it — and a shipper connecting a second time to a listener nobody is
    /// accepting on simply hangs.
    ///
    /// `stall` is how long each answer is held back, and `fail_nth` closes that
    /// connection (1-based; 0 for none) after reading the body and before
    /// answering — the shape that is neither a delivery nor a loss.
    ///
    /// **Which connection matters, and the first one is the wrong one.** The
    /// defect is a latch set by a failure *during a shutdown*, and the first
    /// batch is posted while the child is still running: with `fail_first`,
    /// this test passes with the latch put back. `stall` is what makes the
    /// choice deterministic — the child queues everything and exits while the
    /// first answer is still being held back, so every connection after the
    /// first is a shutdown connection.
    fn counting_store(
        stall: core::time::Duration,
        fail_nth: usize,
    ) -> (u16, std::sync::Arc<AtomicU64>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("bound").port();
        let received = std::sync::Arc::new(AtomicU64::new(0));
        let counted = std::sync::Arc::clone(&received);
        std::thread::spawn(move || {
            let mut connection = 0_usize;
            while let Ok((mut stream, _)) = listener.accept() {
                connection += 1;
                let length = the_lines_in_one_request(&mut stream);
                counted.fetch_add(length.unwrap_or(0) as u64, Ordering::Relaxed);
                if connection == fail_nth {
                    // Read the body, then go without a word. The store may have
                    // kept it; the client cannot tell.
                    continue;
                }
                std::thread::sleep(stall);
                let _ = stream.write_all(b"HTTP/1.1 204 No Content\r\n\r\n");
            }
        });
        (port, received)
    }

    /// A store that is slow until its client gives up on an answer, and quick
    /// from then on.
    ///
    /// **The one shape that tells the abandon flag from the give-up
    /// count.** Every other store in this file answers slower
    /// than `ABANDONED_ANSWER` once a shutdown has shortened its bounds, so a
    /// shipper that ignored the flag entirely would fail `GIVE_UP_AFTER`
    /// exchanges inside four hundred milliseconds and divert the queue anyway —
    /// measured, the flag deleted from the give-up test and the whole crate
    /// green. A store that answers *inside* the shortened bound resets the
    /// counter on every batch, so the count never reaches its limit and the
    /// flag is the only thing left that can take the queue off the network.
    ///
    /// **Switched by what the client does, never by a clock.** Each answer is
    /// held back `stall`, inside `DEADLINE`, so every exchange succeeds and a
    /// full queue takes far longer than `FLUSH` to drain; the first exchange
    /// whose client hangs up before its answer is the flush giving up, which is
    /// the one moment a client stops waiting inside `DEADLINE`, and every
    /// answer after it is at once. Switched a fixed time after its first
    /// connection instead, the flush it has to outlast runs on the child's
    /// clock from the child's own shutdown — so a child slow to fill its queue
    /// under load meets a store already quick, finishes its flush and never
    /// gives up, which is red on an untouched tree whenever the scheduler says
    /// so.
    fn a_store_that_is_slow_until_its_client_gives_up(
        stall: core::time::Duration,
    ) -> (u16, std::sync::Arc<AtomicU64>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("bound").port();
        let received = std::sync::Arc::new(AtomicU64::new(0));
        let counted = std::sync::Arc::clone(&received);
        std::thread::spawn(move || {
            let mut slow = true;
            while let Ok((mut stream, _)) = listener.accept() {
                let length = the_lines_in_one_request(&mut stream);
                counted.fetch_add(length.unwrap_or(0) as u64, Ordering::Relaxed);
                if slow && hung_up_within(&mut stream, stall) {
                    slow = false;
                }
                let _ = stream.write_all(b"HTTP/1.1 204 No Content\r\n\r\n");
            }
        });
        (port, received)
    }

    /// Holds `stream` for `stall`, and answers whether its client hung up on it
    /// meanwhile.
    fn hung_up_within(stream: &mut std::net::TcpStream, stall: core::time::Duration) -> bool {
        let started = std::time::Instant::now();
        let _ = stream.set_read_timeout(Some(core::time::Duration::from_millis(20)));
        let mut byte = [0_u8; 1];
        while started.elapsed() < stall {
            match stream.read(&mut byte) {
                Ok(0) => return true,
                Ok(_) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => return true,
            }
        }
        false
    }

    /// A store that answers one connection and is then not there at all.
    ///
    /// **The one state `shutting_down` decides anything in.** `failures` grows
    /// only while that flag is set, so a store that was never reachable has an
    /// empty queue by the time the shutdown starts, and a store that stays
    /// reachable never fails at all: what the flag is for is a queue that is
    /// still full when the store goes. This holds the first answer back long
    /// enough for the child to fill its queue and exit, answers it, and then
    /// drops the listener — so every connection the shutdown makes is refused.
    fn a_store_that_answers_once_and_then_goes(stall: core::time::Duration) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("bound").port();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                // One batch of short lines fits in the socket buffers, so the
                // client is not blocked writing a body nobody reads.
                std::thread::sleep(stall);
                let _ = stream.write_all(b"HTTP/1.1 204 No Content\r\n\r\n");
            }
            drop(listener);
        });
        port
    }

    /// A store that goes during a shutdown is given up on after `GIVE_UP_AFTER`.
    ///
    /// **The flag that says a shutdown is happening needs a test of its
    /// own.** Measured without one, `self.shutting_down.store(true, …)`
    /// replaced by `store(false, …)`: `cargo test -q -p nc-log` green
    /// and `check passed`. `failures += 1` is gated on that flag, so the whole
    /// "give up on a store that is really gone" mechanism stops existing and a
    /// shutdown retries the network for every remaining batch instead of
    /// diverting it. `one_hiccup_is_not_a_store_that_has_gone` tests only that
    /// it does not give up too *early*, which a shipper that never gives up
    /// satisfies more easily,
    /// and `every_timing_bound_is_the_number_its_paragraph_argues_for`
    /// pins `GIVE_UP_AFTER == 2` and not the wire.
    ///
    /// Counted by the note, because the note is written once per batch that
    /// was *offered* to the store and not once per batch diverted: the
    /// give-up branch writes its lines out with no note at all.
    #[test]
    fn a_store_that_goes_during_a_shutdown_is_given_up_on() {
        let port = a_store_that_answers_once_and_then_goes(core::time::Duration::from_secs(1));
        let stderr = child(
            "ship::hostile_store_tests::the_child_that_queues_a_full_bounded_queue",
            port,
        );
        let offered = stderr
            .lines()
            .filter(|line| line.contains("the store did not confirm"))
            .count();
        // The floor: a run whose store never went, or that never reached the
        // shutdown, offers nothing and would satisfy the ceiling by being idle.
        assert!(
            offered > 0,
            "the shutdown offered the store nothing at all, so the ceiling below is \
             a statement about an empty run: {stderr}"
        );
        assert!(
            offered <= super::GIVE_UP_AFTER,
            "a store that had gone was offered {offered} batch(es) during one \
             shutdown and `GIVE_UP_AFTER` is {}, so the shutdown is retrying the \
             network for a store that is not there instead of writing the queue \
             out: {stderr}",
            super::GIVE_UP_AFTER
        );
    }

    /// Runs one of the child tests below and returns what it printed on stderr.
    fn child(name: &str, port: u16) -> String {
        #[expect(
            clippy::disallowed_methods,
            reason = "a tool this gate drives; its output is read, not printed"
        )]
        let said = std::process::Command::new(std::env::current_exe().expect("this binary"))
            .args([name, "--exact", "--nocapture"])
            .env("NC_LOG_EXIT_CHILD", port.to_string())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .expect("the child runs");
        let told = String::from_utf8_lossy(&said.stdout);
        // A filter that matches nothing runs zero tests and exits 0, which would
        // make every assertion below a statement about an empty run.
        assert!(
            told.contains("1 passed"),
            "the child ran no test, so this proves nothing: {told}"
        );
        String::from_utf8_lossy(&said.stderr).into_owned()
    }

    /// A shutdown costs what its own doc says it costs.
    ///
    /// **Five independently bounded waits in sequence cost more than the sum a
    /// doc quotes.** `Drop` waiting `FLUSH`, then `GRACE` four separate times,
    /// makes an exit documented at fifteen seconds measure **24.0 s** against
    /// an undrained stderr and 18.1 s against a hung store — paid on every CLI
    /// run and every container restart whose store is not answering. The
    /// network waits share one deadline and the stderr flush is one phase after
    /// it.
    #[test]
    fn a_shutdown_stays_inside_its_own_budget() {
        // A store that accepts connections and never answers, so every phase of
        // the shutdown runs to its bound rather than finishing early.
        let (port, _received) = counting_store(core::time::Duration::from_secs(120), 0);
        let started = std::time::Instant::now();
        let stderr = child(
            "ship::hostile_store_tests::the_child_that_queues_a_full_bounded_queue",
            port,
        );
        let took = started.elapsed();
        // **And the number that shutdown reports, which this reads.**
        // `still_waiting` is `queued - handled`, and with nothing reading it
        // both halves are free: measured, `self.queued.fetch_add(1, …)`
        // replaced by `fetch_add(0, …)` leaves `cargo test -q -p nc-log` green
        // and `check passed` — the operator-facing line then reads `0` while a
        // thousand lines are being written out — and
        // `.saturating_sub(self.handled.load(…))` replaced by
        // `.saturating_sub(0)` is green too, which reports the whole queue as
        // waiting when a batch of it is already in the store. `queued`'s own
        // doc says the pair exists so a line abandoned at exit is *countable*,
        // rather than a message that leaves a reader to guess whether it was
        // one line or a thousand.
        //
        // Both ends, because either alone is the other's plant: this store
        // holds every answer back past `DEADLINE`, so exactly one batch is
        // handled inside `FLUSH` and the number is neither zero nor the whole
        // queue.
        let Some((number, _)) = stderr
            .lines()
            .find_map(|line| line.split_once("did not answer for "))
            .and_then(|(_, rest)| rest.split_once(" line(s)"))
        else {
            panic!("the shutdown said nothing about what it was waiting for: {stderr}")
        };
        let remaining: u64 = number.parse().expect("a count is a number");
        let queued = u64::try_from(QUEUE).expect("a small number");
        assert!(
            remaining > 0,
            "the shutdown reported {remaining} line(s) waiting while a full queue was \
             being written out, so the number a reader acts on is not the number of \
             lines: {stderr}"
        );
        assert!(
            remaining < queued,
            "the shutdown reported all {remaining} of {queued} line(s) as waiting, and \
             at least one batch had already been handed to the store, so what was \
             handled is not being taken off the count"
        );
        // `super::FLUSH + super::GRACE` of network and one `super::GRACE` of stderr, plus what the
        // child spends starting up and queueing — generous, because what this
        // catches is a bound that is not a bound, and 24 s against a stated 15
        // is what that looks like.
        let bound = super::FLUSH + super::GRACE + super::GRACE + core::time::Duration::from_secs(6);
        assert!(
            took < bound,
            "a shutdown against a store that never answers took {took:?}, and the \
             documented bound is {:?} + {:?} twice",
            super::FLUSH,
            super::GRACE
        );
        // The floor: a run that never reached the shutdown at all would satisfy
        // the line above by being fast.
        assert!(
            took > super::FLUSH,
            "the child exited in {took:?}, before the flush could have run"
        );
    }

    #[test]
    fn one_failed_exchange_at_exit_does_not_divert_the_rest_of_the_queue() {
        // **A `bool` latch cannot tell "the store is gone" from "the store
        // hiccuped once".** Set by the *first* failed post during a shutdown,
        // it sends every batch after it to stderr without being tried.
        // Measured with such a latch: 300 lines queued with only the first
        // exchange failing, and the store receives 64 — the other 236 are never
        // offered to a store that answers every subsequent request.
        //
        // The failed batch is genuinely uncertain: the server reads its body and
        // closes without answering, so `Silent` is the right verdict and
        // those lines are neither re-sent nor duplicated to stderr. What this
        // asserts is that the rest were *offered*, which is the whole property.
        // Answers held back 300ms each, and the *second* connection is the one
        // that fails: the child queues all 300 lines and exits while the first
        // answer is still outstanding, so every connection after it is one the
        // shutdown makes.
        let (port, received) = counting_store(core::time::Duration::from_millis(300), 2);
        let stderr = child(
            "ship::hostile_store_tests::the_child_that_queues_three_hundred_lines",
            port,
        );
        // Every line reached the store's socket: the 64 of the failed exchange
        // (whose body it read) and the 236 in the batches before and after it.
        let seen = received.load(Ordering::Relaxed);
        assert_eq!(
            seen, 300,
            "the store was offered {seen} of 300 line(s); stderr said: {stderr}"
        );
    }

    #[test]
    fn the_child_that_queues_three_hundred_lines() {
        let Some(port) = std::env::var_os("NC_LOG_EXIT_CHILD") else {
            return;
        };
        let port = port.to_string_lossy().into_owned();
        let shipper =
            Shipper::start(&format!("http://127.0.0.1:{port}/insert")).expect("a valid endpoint");
        for index in 0..300 {
            shipper.send(format!("{{\"_msg\":\"n\",\"i\":{index}}}"));
        }
    }

    #[test]
    fn a_store_that_answers_nothing_at_all_keeps_its_lines() {
        // **Twenty lines to nowhere, without this.** A peer that accepts a
        // connection and closes it without a word — a store restarting, a proxy
        // with no backend, a connection limit — reads `Ok(0)` on the first
        // read; classified `Unreadable` and not re-emitted, measured, a child
        // recording 20 lines and returning loses them all:
        //
        // ```text
        // 2 connection(s) made, 0 line(s) on stderr, dropped() = 0
        // ```
        //
        // to the store and to stderr both, which is precisely what this
        // module's header says cannot happen.
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("bound").port();
        std::thread::spawn(move || {
            while let Ok((stream, _)) = listener.accept() {
                // Not a word, and not even a read.
                drop(stream);
            }
        });
        let stderr = child(
            "ship::hostile_store_tests::the_child_that_queues_three_hundred_lines",
            port,
        );
        let kept = stderr
            .lines()
            .filter(|line| line.contains("\"i\":"))
            .count();
        assert_eq!(
            kept,
            300,
            "a store that said nothing lost {} of 300 line(s). Every line it would not \
             take has to be somewhere a person can read it, which is §18.5's whole \
             argument for keeping the data on this machine: {stderr}",
            300 - kept
        );
    }

    /// Every read error the answer can end on, and what each means for the batch.
    ///
    /// **Without this, deleting the distinction would leave the rest of the
    /// suite green.** The only route to a non-continuable read error with a
    /// *silent* store is the last slice of a ten-second deadline, so nothing
    /// else in the suite reaches this arm: with the whole classification
    /// replaced by `return if status.is_empty()`, `check passed`. A probe against a
    /// listener that accepts and says nothing shows the difference is real:
    ///
    /// ```text
    /// classified: took=10.0s Silent       keep_on_stderr=true  store_answered=true
    /// reverted:   took=10.0s Unreachable  keep_on_stderr=true  store_answered=false
    /// ```
    ///
    /// — every line of the batch duplicated to stderr, and a shutdown
    /// concluding the store has gone.
    ///
    /// Without this the closed kinds would be needles nothing exercises, which
    /// is the `§19.3` shape `MINTED_FROM_TEXT`'s own doc condemns: shrinking
    /// the list to `ConnectionReset` alone leaves the rest of nc-log
    /// green. Each is named here, and each fails the assertion if it leaves the
    /// list.
    #[test]
    fn every_way_the_answer_can_fail_is_classified_and_none_is_a_needle_nothing_reaches() {
        use std::io::ErrorKind;

        // A peer that closed: nothing is holding the batch, so the lines are
        // kept and the store did not answer.
        let closed = [
            ErrorKind::ConnectionReset,
            ErrorKind::ConnectionAborted,
            ErrorKind::UnexpectedEof,
            ErrorKind::BrokenPipe,
            ErrorKind::NotConnected,
        ];
        for kind in closed {
            let outcome = super::verdict(&std::io::Error::from(kind), true);
            assert!(
                outcome.keep_on_stderr() && !outcome.store_answered(),
                "{kind:?} with nothing read is a store that has gone: {outcome:?}"
            );
        }

        // A peer that is merely silent: it read the body and may be ingesting
        // it, so re-emitting would duplicate every line.
        for kind in [ErrorKind::WouldBlock, ErrorKind::TimedOut, ErrorKind::Other] {
            let outcome = super::verdict(&std::io::Error::from(kind), true);
            assert!(
                outcome.keep_on_stderr() && outcome.store_answered(),
                "{kind:?} with nothing read is a silent store: its lines are kept, and \
                 it has not gone: {outcome:?}"
            );
        }

        // **And once a byte of the status has arrived, the peer is there** —
        // whatever the socket then says. This half is what `status.is_empty()`
        // buys and it is asserted separately, because deleting the other half
        // leaves it passing.
        for kind in closed {
            let outcome = super::verdict(&std::io::Error::from(kind), false);
            // **A byte of an answer means it read the request — and the lines
            // are kept anyway.** "The store may hold them and re-emitting
            // duplicates whatever it took" is falsified by a peer that answers
            // *without* reading: measured against one that never calls `read`,
            // sixty-four lines reach neither the store nor stderr when they are
            // not kept, in two milliseconds, with `dropped()` still zero. A
            // duplicate is recoverable and a loss is not, which is the rule
            // this crate's header states.
            assert!(
                outcome.keep_on_stderr() && outcome.store_answered(),
                "{kind:?} after a partial status lost the batch: {outcome:?}"
            );
        }
    }

    /// A store that answered does not get its batch written out as well.
    ///
    /// **Five tests guard "never lose a line", and this one asks about
    /// duplicating.** With both halves of an unknown answer folded into one
    /// variant, `keep_on_stderr` is `!Taken` — and at its only call site,
    /// inside `if let Some(why) = outcome.why()`, `why()` is `None` exactly
    /// for `Taken`, so the four-way taxonomy decides nothing about what is
    /// kept. Measured against fake stores counting the lines each request body
    /// really delivered, 300 lines each:
    ///
    /// ```text
    /// answers 500              store=300 stderr=300   *** 300 duplicated ***
    /// answers garbage          store=300 stderr=300   *** 300 duplicated ***
    /// answers 100 Continue     store=300 stderr=300   *** 300 duplicated ***
    /// ```
    ///
    /// A store that answers has read the request. Whether it kept the batch is
    /// unknown, and that unknown is not the same as the one where nothing came
    /// back at all — which is the distinction `Silent` and `Unreadable` carry.
    #[test]
    fn a_store_that_answered_does_not_have_its_batch_written_out_as_well() {
        // `(what the store writes back, whether the lines are kept, whether the
        // store answered at all)`.
        let answers: [(&str, bool, bool); 5] = [
            // It took them.
            ("HTTP/1.1 204 No Content\r\n\r\n", false, true),
            // It said no: it is not holding them, so they are kept.
            ("HTTP/1.1 500 Internal Server Error\r\n\r\n", true, true),
            // **An interim is read past**, and this fixture then closes — so
            // the store said "I have not answered yet" and went. It never
            // answered, and the lines are kept rather than discarded on the
            // reasoning that it might hold them; a `100 Continue` is the one
            // answer that explicitly says it is not the answer.
            ("HTTP/1.1 100 Continue\r\n\r\n", true, false),
            // Unreadable: the store answered something this cannot parse. It
            // read the request to answer at all, so re-emitting may duplicate
            // whatever it took, and whether it kept the batch is unknown — so
            // the lines are kept.
            ("nonsense that is not HTTP\r\n", true, true),
            ("HTTP/1.1 204 No Content", true, true),
        ];
        for (answer, kept, answered) in answers {
            let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
            let port = listener.local_addr().expect("bound").port();
            let reply = answer.to_owned();
            std::thread::spawn(move || {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let mut sink = [0_u8; 8192];
                let _ = stream.read(&mut sink);
                let _ = stream.write_all(reply.as_bytes());
                std::thread::sleep(core::time::Duration::from_millis(50));
            });
            let target = Target::parse(&format!("http://127.0.0.1:{port}/insert"))
                .expect("a valid endpoint");
            let outcome = target.post(&["{\"i\":1}".to_owned()], &AtomicBool::new(false));
            assert_eq!(
                outcome.keep_on_stderr(),
                kept,
                "`{}` was handled wrongly: {outcome:?}",
                answer.escape_debug()
            );
            assert_eq!(
                outcome.store_answered(),
                answered,
                "`{}` was concluded wrongly about whether the store answered: {outcome:?}",
                answer.escape_debug()
            );
        }
    }

    /// A store that accepts and never reads loses nothing.
    ///
    /// **This is the scenario a one-variant classification gets wrong**, and
    /// the fixture differs
    /// from `a_store_that_answers_nothing_at_all_keeps_its_lines` by one line:
    /// that one drops the accepted stream, this one holds it. The difference is
    /// not something a store operator chooses, and with one variant it decides
    /// whether the lines survive:
    ///
    /// ```text
    /// 300 line(s): delivered=0  on stderr=184  BOTH=0  NEITHER=116
    /// full-queue drops=0
    /// ```
    ///
    /// 116 lines against a peer that provably held none of them — it never
    /// called `read` — with nothing counting them. `keep_on_stderr`'s doc
    /// carries the rule that closes it.
    #[test]
    fn a_store_that_accepts_and_never_reads_loses_nothing() {
        let deaf = TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = deaf.local_addr().expect("bound").port();
        std::thread::spawn(move || {
            // Held, never read from: the socket stays open and the peer is
            // silent, which is a store whose ingest has stalled.
            let held: Vec<_> = deaf.incoming().take(4).flatten().collect();
            std::thread::sleep(core::time::Duration::from_secs(30));
            drop(held);
        });
        let target =
            Target::parse(&format!("http://127.0.0.1:{port}/insert")).expect("a valid endpoint");

        // Both states the drain thread can be in, because the two take
        // different branches out of `answer` and either could lose the batch.
        for abandoning in [false, true] {
            let outcome = target.post(
                &["{\"i\":1}".to_owned(), "{\"i\":2}".to_owned()],
                &AtomicBool::new(abandoning),
            );
            assert!(
                outcome.keep_on_stderr(),
                "a store that never read the batch lost it (abandoning={abandoning}): \
                 {outcome:?}"
            );
            assert!(
                outcome.store_answered(),
                "a store that accepted the connection was concluded to have gone \
                 (abandoning={abandoning}): {outcome:?}"
            );
        }
    }

    /// A shutdown bounds the whole exchange, not only the answer.
    ///
    /// **`abandoning` read inside `answer` alone is not enough.** A thread that
    /// enters `exchange` just before the flush runs out could
    /// spend `CONNECT_TIMEOUT` plus the write timeout — seven seconds — against
    /// a `GRACE` of three, and everything behind it in the queue would be
    /// reported lost while it is still inside one exchange.
    ///
    /// Driven against an address that accepts nothing, which is what a store
    /// that has gone looks like to `connect`.
    #[test]
    fn a_shutdown_shortens_the_connect_and_not_only_the_read() {
        // A port nothing is listening on: the listener is bound and dropped, so
        // the address is real and refuses.
        let closed = TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = closed.local_addr().expect("bound").port();
        drop(closed);
        let target =
            Target::parse(&format!("http://127.0.0.1:{port}/insert")).expect("a valid endpoint");

        // The choice itself, which is the half a test can hold: a refused
        // connection returns instantly whatever the bound is.
        assert_eq!(
            super::bounds(true),
            (super::ABANDONED_ANSWER, super::ABANDONED_ANSWER),
            "a shutdown does not shorten the connect and the write"
        );
        assert_eq!(
            super::bounds(false),
            (CONNECT_TIMEOUT, super::WRITE_TIMEOUT)
        );
        // And the shortened pair really is shorter — both of them, and by
        // enough that the whole exchange fits inside `GRACE` rather than
        // overrunning it by four seconds.
        let (connect, write) = super::bounds(true);
        assert!(connect < CONNECT_TIMEOUT && write < super::WRITE_TIMEOUT);
        assert!(
            connect + write + super::ABANDONED_ANSWER < super::GRACE,
            "an abandoned exchange can still outlast the grace period"
        );

        // And the verdict is unchanged by the bound: a refused connection is a
        // store that is not there, so the lines are kept.
        let outcome = target.post(&["{\"a\":1}".to_owned()], &AtomicBool::new(true));
        assert!(outcome.keep_on_stderr(), "{outcome:?}");

        // **And the flag reaches it**, which the choice above cannot show: with
        // `bounds` hard-wired to `false` every assertion so far still passes,
        // which is a function tested and wired to a constant — the `§19.3` shape
        // this crate's own gate is about.
        //
        // A store that accepts and never reads is what makes the *write* block:
        // the body below is far larger than any socket buffer, so `write_all`
        // waits for a reader that never comes. Abandoning bounds that at
        // `ABANDONED_ANSWER`; not abandoning bounds it at `WRITE_TIMEOUT`, and
        // the two are a factor of twenty-five apart.
        //
        // **This batch is 625 times what a `Shipper` can form**, and the flag is
        // set before the exchange begins — neither is a state the drain loop
        // produces, because `BATCH` is 64 lines of about a hundred bytes and the
        // loop diverts to stderr once the flag is set. What is under test is the
        // bound, which is correct and cheap and covers a real race; `bounds`'
        // own doc says how narrow that race is rather than leaving this fixture
        // to imply otherwise.
        let deaf = TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = deaf.local_addr().expect("bound").port();
        std::thread::spawn(move || {
            // Held, not dropped: a dropped stream closes and the write fails
            // fast for the wrong reason.
            let held: Vec<_> = deaf.incoming().take(1).flatten().collect();
            std::thread::sleep(core::time::Duration::from_secs(20));
            drop(held);
        });
        let target =
            Target::parse(&format!("http://127.0.0.1:{port}/insert")).expect("a valid endpoint");
        let huge: Vec<String> = (0..40_000)
            .map(|index| format!("{{\"i\":{index},\"pad\":\"{}\"}}", "x".repeat(200)))
            .collect();
        let began = std::time::Instant::now();
        let outcome = target.post(&huge, &AtomicBool::new(true));
        let took = began.elapsed();
        assert!(
            took < super::WRITE_TIMEOUT / 2,
            "a shutdown waited {took:?} on a write, so the flag never reached the bound"
        );
        // **Kept.** A store that stopped reading part way through may hold an
        // unknown prefix, so writing the batch out duplicates whatever it took
        // — measured at 4876 lines in the store and 60 000 on stderr. The other
        // way round loses them: 81 of 300 against a store that reads part of the
        // request and stops. `keep_on_stderr`'s doc carries the rule and the
        // arithmetic; here it is asserted where the shutdown makes it reachable.
        assert!(
            outcome.keep_on_stderr(),
            "a batch the store may not have taken was lost: {outcome:?}"
        );
        assert!(
            outcome.store_answered(),
            "a store that stopped reading was concluded to have gone: {outcome:?}"
        );

        // **And a store that *closed* is still a store that has gone**, whose
        // lines are kept. The two are told apart by the error kind, the same way
        // the answer tells them apart — so this is the other half, and without
        // it the distinction could be replaced by "a write failure is always
        // unknown" and every line of a batch to a dead store would be lost.
        let gone = TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = gone.local_addr().expect("bound").port();
        std::thread::spawn(move || {
            // Accept and close at once, without reading: the write then meets a
            // closed peer rather than a full buffer.
            for stream in gone.incoming().take(1).flatten() {
                drop(stream);
            }
        });
        let target =
            Target::parse(&format!("http://127.0.0.1:{port}/insert")).expect("a valid endpoint");
        let outcome = target.post(&huge, &AtomicBool::new(false));
        assert!(
            outcome.keep_on_stderr(),
            "a batch to a store that closed was not kept: {outcome:?}"
        );
        assert!(!outcome.store_answered(), "{outcome:?}");
    }

    /// A status line that begins and stops is a store that is there.
    ///
    /// The half `verdict`'s `nothing_read_yet` buys, end to end and in
    /// milliseconds — the socket ends the read, not the deadline. Which branch
    /// the kernel delivers it on (`Ok(0)` or `ECONNRESET`) is not this crate's
    /// to choose, and the point is that both mean the same thing here: a peer
    /// that started answering read the body, so re-emitting the batch would
    /// duplicate every line of it.
    #[test]
    fn a_status_that_began_to_arrive_is_not_a_store_that_has_gone() {
        let partial = TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = partial.local_addr().expect("bound").port();
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = partial.accept() else {
                return;
            };
            let mut sink = [0_u8; 4096];
            let _ = stream.read(&mut sink);
            // No newline: a status line that began and stopped.
            let _ = stream.write_all(b"HTTP/1.1 200 OK");
            drop(stream);
        });
        let target =
            Target::parse(&format!("http://127.0.0.1:{port}/insert")).expect("a valid endpoint");
        let outcome = target.post(&["{\"a\":1}".to_owned()], &AtomicBool::new(false));
        assert!(
            outcome.store_answered(),
            "a status that began to arrive is a store that is there: {outcome:?}"
        );
    }

    #[test]
    fn a_store_that_says_nothing_is_unreachable_however_the_socket_says_so() {
        // **Two shapes, one meaning, and they arrive on different branches.** A
        // peer that reads the body and then closes gives a clean end of stream
        // (`Ok(0)`); a peer that closes with the body still unread gives
        // `ECONNRESET`. Which one a real store produces is not this crate's to
        // choose — measured, one fixture produced both across runs — so both
        // have to classify the same way.
        //
        // Asserted here rather than only end to end, because the socket decides
        // which branch fires and a test that cannot say which one it exercised
        // proves nothing about the other. The two fixtures below are named for
        // what the *server* does, not for what the socket reports: which of
        // `Ok(0)` and `ECONNRESET` each produces is the kernel's business, and
        // that is precisely why both branches have to agree.
        for (about, reads_first) in [
            ("closed with the body unread", false),
            ("read some of the body, then closed", true),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
            let port = listener.local_addr().expect("bound").port();
            std::thread::spawn(move || {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                if reads_first {
                    let mut sink = [0_u8; 4096];
                    let _ = stream.read(&mut sink);
                }
                drop(stream);
            });
            let target = Target::parse(&format!("http://127.0.0.1:{port}/insert"))
                .expect("a valid endpoint");
            let outcome = target.post(&["{\"a\":1}".to_owned()], &AtomicBool::new(false));
            assert!(
                outcome.keep_on_stderr(),
                "{about}: a store that answered nothing lost the batch: {outcome:?}"
            );
            assert!(
                !outcome.store_answered(),
                "{about}: a store that answered nothing counts as being there: {outcome:?}"
            );
        }

        // **And a store that is merely silent is *not* unreachable.** It read
        // the body and may be ingesting it, so re-emitting duplicates every
        // line — measured against a store answering one second past the
        // deadline: `lines_the_store_received = 1`, and stderr held a copy of
        // the same line. That is the same unknown fate a *trickling* store has,
        // and it gets the same handling.
        //
        // Driven through the shutdown budget so it costs a fraction of a
        // `DEADLINE` rather than a whole one, which is the other property this
        // asserts.
        let silent = TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = silent.local_addr().expect("bound").port();
        std::thread::spawn(move || {
            let held: Vec<_> = silent.incoming().take(1).flatten().collect();
            std::thread::sleep(core::time::Duration::from_secs(5));
            drop(held);
        });
        let target =
            Target::parse(&format!("http://127.0.0.1:{port}/insert")).expect("a valid endpoint");
        // **The flag flips *during* the read, which is the only state the
        // shipper is ever in.** Set before the exchange begins —
        // `AtomicBool::new(true)` — this would certify a state the drain thread
        // cannot reach: the loop refuses to post at all once the flag is set,
        // so the shortened budget only ever applies to an exchange that is
        // *already running*. Written that way, this test passes a shipper that
        // loses 768 of 1000 lines on its own headline fixture.
        let abandoning = std::sync::Arc::new(AtomicBool::new(false));
        let flip = std::sync::Arc::clone(&abandoning);
        std::thread::spawn(move || {
            std::thread::sleep(core::time::Duration::from_millis(300));
            flip.store(true, Ordering::Relaxed);
        });
        let started = std::time::Instant::now();
        let outcome = target.post(&["{}".to_owned()], &abandoning);
        let took = started.elapsed();
        // **Kept, and the store not blamed.** These are two questions and one
        // answer for both is wrong: not keeping the lines is how a silent store
        // costs 116 of 300 of them. A silent store keeps its lines *and* has
        // not gone.
        assert!(
            outcome.keep_on_stderr(),
            "a silent store's batch was lost rather than written out: {outcome:?}"
        );
        assert!(
            outcome.store_answered(),
            "a silent store was concluded to have gone: {outcome:?}"
        );
        assert!(
            outcome.store_answered(),
            "a store that is merely slow was read as gone, which diverts the whole \
             queue during a shutdown: {outcome:?}"
        );
        // **And it gave up in the shutdown's time, not the store's.**
        // `abandoning` is read while waiting, because otherwise the thread sits
        // in one exchange for a whole `DEADLINE` — ten seconds — while `GRACE`
        // is three, and the phase that exists to write the rest of the queue
        // where a person can read it never runs. Measured against a store
        // answering each batch after five seconds: 772 of 1000 lines lost, and
        // nothing written by the phase whose whole job is to write them.
        //
        // Generous against a loaded machine, and still far below `DEADLINE`.
        assert!(
            took < DEADLINE / 4,
            "a shutdown waited {took:?} for one answer, against a {DEADLINE:?} deadline"
        );

        // And the distinction this must not erase: a store that *did* answer,
        // unreadably, is a store that is there, though its batch is kept.
        let (port, _seen) = answering("nonsense with no newline");
        let target =
            Target::parse(&format!("http://127.0.0.1:{port}/insert")).expect("a valid endpoint");
        let outcome = target.post(&["{}".to_owned()], &AtomicBool::new(false));
        assert!(
            outcome.store_answered(),
            "an unreadable answer is still an answer, so the store is there: {outcome:?}"
        );
    }

    #[test]
    fn one_hiccup_is_not_a_store_that_has_gone() {
        // **Without a store that says nothing, `GIVE_UP_AFTER` is dead code.**
        // `failures` is incremented only when the store did *not* answer, and
        // an unreadable answer counts as the store being there — so with that
        // as the only shutdown failure a test produces, nothing reaches the
        // counter, and setting it to `1`, a boolean latch, leaves the whole
        // suite green.
        //
        // A store that says nothing is `Unreachable`, which is what makes the
        // counter reachable: this fails at `GIVE_UP_AFTER = 1` and passes at 2,
        // which is the difference the constant exists to express.
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("bound").port();
        let received = std::sync::Arc::new(AtomicU64::new(0));
        let counted = std::sync::Arc::clone(&received);
        std::thread::spawn(move || {
            let mut connection = 0_usize;
            while let Ok((mut stream, _)) = listener.accept() {
                connection += 1;
                // The second connection is the shutdown's first, for the reason
                // `counting_store` gives: the child has exited by then.
                if connection == 2 {
                    drop(stream);
                    continue;
                }
                let mut request = Vec::new();
                let mut chunk = [0_u8; 4096];
                let length = loop {
                    match stream.read(&mut chunk) {
                        Ok(0) | Err(_) => break None,
                        Ok(read) => {
                            request.extend_from_slice(&chunk[..read]);
                            let text = String::from_utf8_lossy(&request);
                            if let Some((head, body)) = text.split_once("\r\n\r\n")
                                && let Some(declared) = head
                                    .lines()
                                    .find_map(|line| line.strip_prefix("Content-Length: "))
                                && let Ok(declared) = declared.trim().parse::<usize>()
                                && body.len() >= declared
                            {
                                break Some(body.lines().filter(|l| !l.is_empty()).count());
                            }
                        }
                    }
                };
                counted.fetch_add(length.unwrap_or(0) as u64, Ordering::Relaxed);
                std::thread::sleep(core::time::Duration::from_millis(300));
                let _ = stream.write_all(b"HTTP/1.1 204 No Content\r\n\r\n");
            }
        });
        let stderr = child(
            "ship::hostile_store_tests::the_child_that_queues_three_hundred_lines",
            port,
        );
        // One hiccup costs one batch to stderr; the rest are still offered to a
        // store that answers every request after it.
        let seen = received.load(Ordering::Relaxed);
        assert!(
            seen >= 236,
            "one hiccup diverted the queue: the store was offered {seen} of 300; {stderr}"
        );
    }

    /// A run of interim answers past the bound keeps the lines.
    ///
    /// **The bound must not turn "not answered yet" into "may hold
    /// them".** Five `100 Continue` and then a `500` — a store or proxy chain
    /// that emits one more interim than `INTERIM_LIMIT` — is `Unreadable` by
    /// that reading, and where that is not re-emitted sixty-four lines are in
    /// no store and on no stderr. A store that has sent nothing but interims
    /// has said it has not answered; that is `Silent`. Both are kept on stderr,
    /// so this test asserts the verdict.
    #[test]
    fn a_store_that_only_ever_says_wait_keeps_its_lines() {
        let many: &'static str = "HTTP/1.1 100 Continue\r\n\r\n\
             HTTP/1.1 100 Continue\r\n\r\n\
             HTTP/1.1 100 Continue\r\n\r\n\
             HTTP/1.1 100 Continue\r\n\r\n\
             HTTP/1.1 100 Continue\r\n\r\n\
             HTTP/1.1 500 Nope\r\n\r\n";
        let (port, _seen) = answering(many);
        let target =
            Target::parse(&format!("http://127.0.0.1:{port}/insert")).expect("a valid endpoint");
        let outcome = target.post(&["{}".to_owned()], &AtomicBool::new(false));
        assert!(
            outcome.keep_on_stderr(),
            "a store that only ever said wait lost the batch: {outcome:?}"
        );
        assert!(
            outcome.why().is_some_and(|why| why.contains("interim")),
            "the verdict does not say what happened: {outcome:?}"
        );
    }

    #[test]
    fn an_interim_answer_is_read_past_and_the_lines_are_not_lost() {
        // RFC 9110 lets a server send a `1xx` at any time and requires a client
        // to read past it. Read as one status line and stopped at,
        // `HTTP/1.1 100 Continue` is a `Refused` — "it did not keep the batch",
        // false — or an `Unreadable`, on the reasoning that the store may hold
        // the lines. A store that has said `100 Continue` has said the
        // opposite: it has not finished answering, and where `Unreadable` is
        // not written to stderr, sixty-four lines at a time reach neither the
        // store nor stderr, and only a count reaches anybody.
        //
        // So it is read past. This fixture sends the interim and then closes,
        // which is a store that never answered — so the verdict is that the
        // store went, and the lines are kept where a person can read them.
        let (port, _seen) = answering("HTTP/1.1 100 Continue\r\n\r\n");
        let target =
            Target::parse(&format!("http://127.0.0.1:{port}/insert")).expect("a valid endpoint");
        let outcome = target.post(&["{}".to_owned()], &AtomicBool::new(false));
        assert!(
            outcome.keep_on_stderr(),
            "an interim answer and no status lost the batch: {outcome:?}"
        );

        // And when the real status does follow, that is the answer.
        let (port, _seen) = answering("HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 204 No Content\r\n");
        let target =
            Target::parse(&format!("http://127.0.0.1:{port}/insert")).expect("a valid endpoint");
        let outcome = target.post(&["{}".to_owned()], &AtomicBool::new(false));
        assert!(
            outcome.taken(),
            "the status after an interim was not read as the answer: {outcome:?}"
        );

        // **At the bound exactly, which is what the loop's inclusive range is
        // for.** One interim, one-then-`204`, five-then-`500` and six are none
        // of them *four* — the permitted number — followed by a real status, so
        // an exclusive range reads the same on all of them. A store sending
        // `100 Continue` while three proxies each add a `103 Early Hints` is
        // four, and with the range written exclusive that store *takes* the
        // batch and this client answers `Silent`: the lines it accepted are
        // duplicated onto stderr, which this file's own doc calls not a smaller
        // failure than a loss. Measured without this case: `0..INTERIM_LIMIT`
        // leaves `check passed`.
        let at_the_bound: &'static str = "HTTP/1.1 100 Continue\r\n\r\n\
             HTTP/1.1 103 Early Hints\r\n\r\n\
             HTTP/1.1 103 Early Hints\r\n\r\n\
             HTTP/1.1 103 Early Hints\r\n\r\n\
             HTTP/1.1 204 No Content\r\n";
        let (port, _seen) = answering(at_the_bound);
        let target =
            Target::parse(&format!("http://127.0.0.1:{port}/insert")).expect("a valid endpoint");
        let outcome = target.post(&["{}".to_owned()], &AtomicBool::new(false));
        assert!(
            outcome.taken(),
            "the store took the batch after the permitted number of interim answers and \
             this client reported otherwise, so the lines it accepted go to stderr too: \
             {outcome:?}"
        );

        // A store that sends nothing but interims is not answering, and says so
        // rather than waiting for ever.
        let many: &'static str = "HTTP/1.1 100 Continue\r\n\r\n\
             HTTP/1.1 100 Continue\r\n\r\n\
             HTTP/1.1 100 Continue\r\n\r\n\
             HTTP/1.1 100 Continue\r\n\r\n\
             HTTP/1.1 100 Continue\r\n\r\n\
             HTTP/1.1 100 Continue\r\n\r\n";
        let (port, _seen) = answering(many);
        let target =
            Target::parse(&format!("http://127.0.0.1:{port}/insert")).expect("a valid endpoint");
        let outcome = target.post(&["{}".to_owned()], &AtomicBool::new(false));
        assert!(
            outcome.why().is_some_and(|why| why.contains("interim")),
            "a store sending nothing but interims was not named as such: {outcome:?}"
        );
    }

    #[test]
    fn lines_the_flush_runs_out_of_time_for_are_counted_and_kept() {
        // **The tail of a full queue, lost silently without `GRACE`.** With
        // `Drop` bounding the flush at `FLUSH` and doing nothing else, against
        // a healthy store taking about a second a batch, 47 of 1000 lines go
        // nowhere: `dropped()` reads zero — a line the process never got round
        // to sending is not a queue-drop — and the message carries no number.
        // Any store slower than `FLUSH / (QUEUE / BATCH)`, 0.768s a batch,
        // reaches it.
        //
        // A timeout asks the thread to stop using the network and write what is
        // left where a person can read it, and says how many that was. This is
        // the slowest test in the crate for the reason the last one runs in a
        // child process: only a real exit tests a real exit, and only a real
        // `FLUSH` tests a real `FLUSH`.
        // **Slower than the whole grace period, which is the case that
        // matters.** At 1100ms a batch the exchange the flag interrupts
        // finishes well inside `GRACE`, so the queue behind it reaches stderr
        // and this test would pass with the `POLL` cap doing nothing: measured
        // at five seconds a batch without the cap, 232 of 1000 delivered,
        // **zero** on stderr, and "writing them here instead" followed by
        // nothing written. The thread is inside one `read` whose timeout is
        // already set to `DEADLINE`, so the flag is invisible for as long as
        // the store stays silent.
        // **Longer than `FLUSH + GRACE`, or the fixture decides the verdict.**
        // At five seconds a batch this passes 2 of 4 runs with the `POLL` cap
        // deleted: the store's answer sometimes arrives inside `GRACE`, the
        // thread comes free on its own, and the tail reaches stderr without the
        // flag ever having been seen. A store that will not answer until long
        // after the process must be gone leaves exactly one way for the tail to
        // be written, which is the one under test.
        let (port, received) = counting_store(core::time::Duration::from_secs(30), 0);
        let stderr = child(
            "ship::hostile_store_tests::the_child_that_queues_a_full_bounded_queue",
            port,
        );
        assert!(
            stderr.contains("writing them here instead"),
            "the flush ran out and said nothing about it: {stderr}"
        );
        // The number, which is the half a bare message lacks: a message saying
        // "lines queued at exit were not sent" leaves a reader guessing whether
        // that was one line or a thousand.
        let counted: u64 = stderr
            .lines()
            .find_map(|line| {
                line.strip_prefix("nc-log: the store did not confirm ")?
                    .split_whitespace()
                    .next()?
                    .parse()
                    .ok()
            })
            .expect("the message carries a number");
        assert!(counted > 0, "{stderr}");

        // And they are on stderr rather than gone: every line the store did not
        // take is written out, so the two together account for the queue.
        let kept = stderr
            .lines()
            .filter(|line| line.contains("\"i\":"))
            .count() as u64;
        let seen = received.load(Ordering::Relaxed);
        assert!(
            kept > 0,
            "the flush counted {counted} line(s) and wrote none of them: {stderr}"
        );
        let total = u64::try_from(QUEUE).expect("a small number");
        let batch = u64::try_from(BATCH).expect("a small number");
        // **Nothing is lost.** Every line is delivered or written out, and this
        // is the assertion the whole classification exists for: a batch that
        // reaches neither is gone, and `dropped()` does not count it because the
        // process did try to send it.
        assert!(
            seen + kept >= total,
            "{seen} reached the store and {kept} reached stderr, of {total} — {} line(s) reached neither",
            total.saturating_sub(seen + kept)
        );
        // **And at most one batch is written twice.** The batch in flight when
        // the flag flipped may have been read by the store and is written out
        // anyway, because its fate is unknown and a loss is not recoverable
        // (`keep_on_stderr`'s doc carries the arithmetic). What this refuses is
        // that becoming a habit: more than one batch of overlap means the
        // classification is duplicating batches whose fate is known.
        assert!(
            seen + kept <= total + batch,
            "{seen} reached the store and {kept} reached stderr, of {total} — {} line(s) went to both, which is more than the one batch in flight",
            (seen + kept).saturating_sub(total)
        );
    }

    /// Every line the flush says it is writing out is written out.
    ///
    /// **The sentence the shutdown prints is a promise, and this holds
    /// it.** `nc-log: the store did not answer for N line(s) within 12s; writing them here instead`
    /// is what a person reads when a process exits with a queue behind it, and
    /// the only thing that makes it true is the abandon flag being asked at the
    /// top of the loop. Measured without this
    /// test, `|| abandoning.load(Ordering::Relaxed)` deleted from that
    /// condition: the whole of `nc-log` green, and `check passed`.
    ///
    /// `lines_the_flush_runs_out_of_time_for_are_counted_and_kept` cannot see it
    /// and neither can any other test here, for one reason: their stores are all
    /// slower than `ABANDONED_ANSWER`, so a shipper ignoring the flag fails
    /// `GIVE_UP_AFTER` exchanges in four hundred milliseconds and diverts the
    /// queue by the other arm. The store beside this one answers inside that
    /// bound instead, which resets the counter on every batch and leaves the
    /// flag carrying the whole decision.
    ///
    /// What it asserts is the promise rather than a count of connections: the
    /// number the message names is the number that reaches stderr. A shipper
    /// that puts them back on the network delivers them, which is not a loss and
    /// is not what the reader was told.
    #[test]
    fn the_lines_the_flush_gives_up_on_do_not_go_back_to_the_network() {
        // Slower than a batch's share of `FLUSH` so the flush cannot finish,
        // inside `DEADLINE` so no exchange fails before it gives up, and quick
        // from the exchange the give-up abandons on, so every exchange after
        // that succeeds well inside `ABANDONED_ANSWER`.
        //
        // **No window, because machine load would decide it.** A store that
        // turns quick a fixed ten seconds after its first connection meets a
        // child slow to fill its queue that begins its flush later than that,
        // finishes it against the quick store and never gives up — red on an
        // untouched tree whenever the scheduler says so.
        //
        // What the store answers to is the client giving up, which is the event
        // this test is about.
        let (port, received) =
            a_store_that_is_slow_until_its_client_gives_up(core::time::Duration::from_secs(4));
        let stderr = child(
            "ship::hostile_store_tests::the_child_that_queues_a_full_bounded_queue",
            port,
        );
        let counted: u64 = stderr
            .lines()
            .find_map(|line| {
                line.strip_prefix("nc-log: the store did not answer for ")?
                    .split_whitespace()
                    .next()?
                    .parse()
                    .ok()
            })
            .expect("the flush gave up and said how many lines it was writing out");
        assert!(
            counted > 0,
            "the flush gave up with nothing left, so this test proves nothing: {stderr}"
        );
        let kept = stderr
            .lines()
            .filter(|line| line.contains("\"i\":"))
            .count() as u64;
        let seen = received.load(Ordering::Relaxed);
        // **All but the one batch in flight**, which is the same allowance
        // `lines_the_flush_runs_out_of_time_for_are_counted_and_kept` makes at
        // the other end: the number is read the moment the flag is set, and the
        // batch already on the wire is counted as still waiting until the
        // exchange it is in returns. Measured here, exactly `BATCH`: 872 named,
        // 808 on stderr, 192 in the store. What this refuses is the *rest* of
        // them going back to the network, which is what deleting the flag does.
        let batch = u64::try_from(BATCH).expect("a small number");
        assert!(
            kept + batch >= counted,
            "the shutdown said it was writing {counted} line(s) here instead and {kept} \
             reached stderr, so {} of them went back to the network — more than the one \
             batch in flight. The store took {seen} in all, and a reader told to look \
             here would not find them",
            counted.saturating_sub(kept + batch)
        );
    }

    #[test]
    fn the_child_that_queues_a_full_bounded_queue() {
        let Some(port) = std::env::var_os("NC_LOG_EXIT_CHILD") else {
            return;
        };
        let port = port.to_string_lossy().into_owned();
        let shipper =
            Shipper::start(&format!("http://127.0.0.1:{port}/insert")).expect("a valid endpoint");
        // Exactly the queue, sent as fast as the channel accepts them, so
        // nothing is dropped for fullness and every line is one the flush is
        // responsible for.
        for index in 0..QUEUE {
            shipper.send(format!("{{\"_msg\":\"n\",\"i\":{index}}}"));
        }
        assert_eq!(
            shipper.dropped(),
            0,
            "the fixture must not drop for fullness"
        );
    }

    #[test]
    fn an_answer_with_no_newline_is_bounded_rather_than_read_to_the_end() {
        // Measured without this bound: a 384 MiB answer takes the process from
        // 3.4 MiB resident to 223 MiB, because a read timeout does not fire
        // while every read returns data. The message built from it is as large.
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("bound").port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("a connection");
            let mut sink = [0_u8; 4096];
            let _ = stream.read(&mut sink);
            // No newline, ever. Enough to dwarf a status line many times over.
            for _ in 0..64 {
                if stream.write_all(&b"x".repeat(64 * 1024)).is_err() {
                    break;
                }
            }
        });

        let target =
            Target::parse(&format!("http://127.0.0.1:{port}/insert")).expect("a valid endpoint");
        let outcome = target.post(&["{}".to_owned()], &AtomicBool::new(false));
        let error = outcome.expect_refusal("an answer with no status line is not an answer");
        // **And the store answered.** The body went out and something came
        // back; whether the store kept the batch is unknown, but it has not
        // gone.
        assert!(
            outcome.store_answered(),
            "an unreadable answer is still an answer: {outcome:?}"
        );
        // The number the message carries is the number the read stopped at, and
        // without the bound it is however much the server chose to send.
        assert!(
            // Not `|| error.contains("no status line")`: that phrase is a
            // constant part of this error path, so the disjunction could never
            // fail and the number it is meant to pin would go unchecked.
            // **And written out, not built from the constant under test.**
            // `format!("{STATUS_LIMIT} byte(s)")` holds whatever the constant
            // happens to say — the shape this file condemns by name in
            // `a_shutdown_stays_inside_its_own_budget`'s own doc, and the shape
            // that lets `STATUS_LIMIT` go from 8 KiB to 64 KiB with
            // `check passed`. The pin lives in
            // `every_timing_bound_is_the_number_its_paragraph_argues_for`; this
            // says the message carries that number and not another.
            error.contains("8192 byte(s)"),
            "{error}"
        );
        let read: u64 = error
            .split_whitespace()
            .find_map(|word| word.parse().ok())
            .expect("the message names how much it read");
        assert!(
            read <= STATUS_LIMIT,
            "it read {read} bytes looking for a newline, and the limit is {STATUS_LIMIT}"
        );
        let _ = server.join();
    }

    /// A server that answers `status`, having first read the request.
    ///
    /// Returns the port and the request it received, so a test can assert on
    /// both — the **bytes this writes** as well as the verdict.
    fn answering(status: impl Into<String>) -> (u16, std::sync::mpsc::Receiver<String>) {
        let status = status.into();
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("bound").port();
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("a connection");
            // Read until the body has arrived: the headers and the body are two
            // writes, so one `read` sees only the first and the assertion about
            // what this ships would be about the headers alone.
            let mut request = Vec::new();
            let mut chunk = [0_u8; 512];
            while !request.windows(2).any(|pair| pair == b"}\n") {
                match stream.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => request.extend_from_slice(&chunk[..read]),
                }
            }
            let _ = sender.send(String::from_utf8_lossy(&request).into_owned());
            let _ = stream.write_all(status.as_bytes());
        });
        (port, receiver)
    }

    #[test]
    fn a_refusal_is_a_refusal_and_a_success_is_a_success() {
        // **`post` is handed a status line here.** Without that, replacing the
        // 2xx test with a tautology — so every answer counts as success and
        // every batch is discarded — leaves the whole suite green. A mistyped
        // path answering 404 would swallow the platform's logs in silence, in
        // the component whose module doc calls that the §19.3 failure.
        for (status, ok) in [
            ("HTTP/1.1 204 No Content\r\n\r\n", true),
            ("HTTP/1.1 200 OK\r\n\r\n", true),
            ("HTTP/1.1 404 Not Found\r\n\r\n", false),
            ("HTTP/1.1 500 Internal Server Error\r\n\r\n", false),
        ] {
            let (port, seen) = answering(status);
            let target = Target::parse(&format!("http://127.0.0.1:{port}/insert/jsonline"))
                .expect("a valid endpoint");
            let answer = target.post(&["{\"a\":1}".to_owned()], &AtomicBool::new(false));
            assert_eq!(answer.taken(), ok, "{status:?} → {answer:?}");

            // And the request itself: the path, the content type the store
            // needs, and the body on its own line.
            let request = seen.recv().expect("the server read something");
            assert!(
                request.starts_with("POST /insert/jsonline HTTP/1.1"),
                "{request}"
            );
            assert!(
                request.contains("Content-Type: application/stream+json"),
                "{request}"
            );
            assert!(request.ends_with("{\"a\":1}\n"), "{request}");
            // **And the header the one-request-per-connection model rests on**:
            // measured without this, deleting `Connection: close\r\n` from the
            // request leaves `cargo test -q -p nc-log` green and
            // `check passed`. `counting_store`'s own comment reads the end of a
            // body off it — *"read until the peer stops writing, which it does
            // after the body because the shipper sends `Connection: close`"* —
            // so a store doing the same would wait for a request that never
            // ends, and this client keeps no pool to reuse a connection it did
            // not close.
            assert!(request.contains("Connection: close"), "{request}");
            // **And the header that says where the body stops.** Six facts are
            // stated, and the two addressed to the server are the two a reader
            // of the body cannot see: `Content-Length` short by one or two,
            // unread here, leaves the whole crate green and `check passed` —
            // and on a running store the effect is one of two the client cannot
            // tell apart: the malformed last record dropped under a 2xx this
            // code reads as `Taken`, or every batch refused and duplicated onto
            // stderr. The two helpers that parse a request cannot catch it: one
            // loops while `body.len() >= declared`, so a short declaration just
            // ends the loop early, and the other judges with `contains` over
            // whatever arrived.
            let body = "{\"a\":1}\n";
            assert!(
                request.contains(&format!("Content-Length: {}", body.len())),
                "the length the request declares is not the length of the body it sends: \
                 {request}"
            );
            // **And the host, which is the other fact the server reads.**
            // Pointed at `nowhere:0`, unread here, the whole gate is green.
            assert!(
                request.contains(&format!("Host: 127.0.0.1:{port}")),
                "the request names a host that is not the one it was sent to: {request}"
            );
            // **And it is an authority, which for an IPv6 literal means
            // brackets.** `Target::parse` strips them on purpose, so the writer
            // has to put them back: `Host: ::1:9428` is not
            // `uri-host [ ":" port ]`, and a store that validates the header
            // refuses every batch for ever.
            let said = request
                .lines()
                .find_map(|line| line.strip_prefix("Host: "))
                .expect("the request states a host");
            let (host, _) = said.rsplit_once(':').expect("and a port with it");
            assert!(
                host.matches(':').count() == 0 || (host.starts_with('[') && host.ends_with(']')),
                "`Host: {said}` is not an authority: an IPv6 literal is bracketed"
            );
            // **And the writer goes through the rule.** The clause above can
            // never fire here: this fixture's host is the loopback, so the left
            // disjunct answers every time.
            // `an_ipv6_host_is_bracketed_in_the_header` holds the rule and
            // `Target::parse` has its own bracket test; what neither holds is
            // the hand-over between them, and stripping the brackets at this
            // call site leaves both green.
            let here = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ship.rs"))
                .expect("this file is readable");
            // Comment-stripped, and the test modules off: a twelve-space
            // comment naming this call pays for it otherwise, which is the
            // mechanism measured on the quoting arms one screen up.
            let shipped = super::the_code_this_crate_ships(&here);
            // **The call, not the line it is written on.** A needle carrying
            // twelve spaces of indentation and a trailing comma makes hoisting
            // the call into a `let` one line above — a refactor that changes
            // nothing — fail with a message saying the host written into the
            // request is not the one the authority rule returns. That is the
            // too-strong half.
            // **Driven as well as pinned.**
            // `a_request_to_an_ipv6_store_brackets_its_host` builds the header
            // for `[::1]` and reads it, which a header built inside a function
            // that opens a socket would not allow. The needle stays as a second
            // reader — a call anywhere, so an ordinary hoist is not refused.
            let writes = format!("{}(&self.host)", "as_an_authority");
            assert_eq!(
                shipped.matches(&writes).count(),
                1,
                "the host written into the request is not the one the authority rule \
                 returns, so an IPv6 store is sent a malformed `Host:` and refuses every \
                 batch"
            );
        }
    }

    /// The table in `xtask/canaries/status-lines.tsv`, answered here.
    ///
    /// **One question, one function per crate, one table.** `served`
    /// in `xtask/src/store.rs` decides whether the log store is up and this
    /// decides whether a batch was taken — the same test of the same first line
    /// — and with separate copies only one requires the three characters of the
    /// code to be digits, so `HTTP/1.1 2ab Invented` is a refusal here
    /// and `the log store answers /health` there, on the same answer from the
    /// same store.
    ///
    /// The copies are one function per crate, and this is the table both
    /// read. Adding a case to the file adds it to both suites, which is the
    /// point: a shared sentence in two comments is what lets them drift.
    #[test]
    fn the_shared_status_line_table_is_answered_the_same_way() {
        const ROWS: usize = 23;
        let table = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../xtask/canaries/status-lines.tsv"
        ))
        .expect("the shared status-line table is readable");
        let mut checked = 0_usize;
        for row in table.lines() {
            if row.starts_with('#') || row.trim().is_empty() {
                continue;
            }
            let mut column = row.split('\t');
            let line = column.next().expect("a status line");
            if line == "line" {
                continue;
            }
            let terminator = column.next().expect("a terminator column");
            let served = column.next().expect("a `served` column");
            let taken = column.next().expect("a `taken` column");
            let _ = served;
            let expected = match taken {
                "taken" => true,
                "not-taken" => false,
                other => panic!("`{other}` is not a verdict this table can carry"),
            };
            let written = line.replace("\\r", "\r");
            let answer = match terminator {
                "crlf" => format!("{written}\r\n\r\n"),
                "none" => written,
                other => panic!("`{other}` is not a terminator this table can carry"),
            };
            let (port, _seen) = answering(answer);
            let target = Target::parse(&format!("http://127.0.0.1:{port}/insert/jsonline"))
                .expect("a valid endpoint");
            let answer = target.post(&["{\"a\":1}".to_owned()], &AtomicBool::new(false));
            assert_eq!(
                answer.taken(),
                expected,
                "`{line}` ({terminator}) is `{taken}` in the shared table and this said \
                 {answer:?}"
            );
            checked += 1;
        }
        // **The row count, not a floor with room under it.** `checked >= 15`
        // over twenty rows leaves five to spare, and one of the five is the
        // only row that decides `code.chars().all(is_ascii_digit)` — so
        // deleting `HTTP/1.1 2ab Invented` and then that clause restores
        // exactly the drift this table exists to end, with both suites green. A
        // number that has to move when the file moves is what makes a shared
        // table a mechanism rather than a habit.
        assert_eq!(
            checked, ROWS,
            "the shared table answered {checked} case(s) and holds {ROWS}. Adding or \
             removing a row is a change to what both suites are held to, so the number \
             moves here in the same edit"
        );
    }

    #[test]
    fn a_store_that_never_answers_the_connect_gives_up_within_the_bound() {
        // **The call site, and not only the constant.** With only the number
        // read, replacing `connect_timeout` with `connect` leaves the suite
        // green — a test that reads the number does not watch it apply. Without
        // it the OS decides, and its SYN retries run for minutes on the one
        // thread draining the queue.
        //
        // `192.0.2.1` is TEST-NET-1 (RFC 5737): guaranteed not routable, so the
        // connect can only end by timing out.
        // Built rather than parsed, for the reason above.
        let target = Target {
            host: "192.0.2.1".to_owned(),
            port: 9428,
            path: "/insert".to_owned(),
        };
        let started = std::time::Instant::now();
        let outcome = target.post(&["{}".to_owned()], &AtomicBool::new(false));
        let error = outcome.expect_refusal("nothing is there to answer");
        // A store that was never reached *is* re-emitted, which is §18.5's
        // whole argument for keeping the data on this machine.
        assert!(
            outcome.keep_on_stderr(),
            "an unreachable store lost the batch: {outcome:?}"
        );
        let took = started.elapsed();
        assert!(
            took < CONNECT_TIMEOUT * 3,
            "the connect took {took:?}, and the bound is {CONNECT_TIMEOUT:?} — the OS \
             decided, not this code"
        );
        assert!(error.contains("could not reach the store"), "{error}");
    }

    #[test]
    fn only_a_real_http_success_counts_as_one() {
        // `code.starts_with('2')` accepts `HTTP/1.1 2 whatever` and
        // `garbage 200 ok`, so a store answering something that is not HTTP — a
        // proxy's error page, a TLS handshake read as text — would have its
        // batches discarded as though they had been taken.
        for (answer, taken) in [
            ("HTTP/1.1 200 OK\r\n", true),
            ("HTTP/1.1 204 No Content\r\n", true),
            ("HTTP/1.0 201 Created\r\n", true),
            ("HTTP/1.1 2 whatever\r\n", false),
            ("garbage 200 ok\r\n", false),
            ("HTTP/1.1 404 Not Found\r\n", false),
            ("HTTP/1.1 500 Internal Server Error\r\n", false),
            ("HTTP/1.1 20 short\r\n", false),
            ("HTTP/1.1 2000 long\r\n", false),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
            let port = listener.local_addr().expect("bound").port();
            let reply = answer.to_owned();
            let serving = std::thread::spawn(move || {
                if let Ok((mut stream, _)) = listener.accept() {
                    // **Read the request before answering, and shut the write
                    // side down rather than dropping.** Closing a socket with
                    // unread data in its receive buffer sends RST on Linux,
                    // which destroys the reply that was already written — so
                    // dropping makes this test fail about one run in eight, on
                    // a correct client. Measured: 2 failures in 17 runs, and
                    // `cargo xtask check` fails with it.
                    let mut request = [0_u8; 4096];
                    let _ = stream.read(&mut request);
                    let _ = stream.write_all(reply.as_bytes());
                    let _ = stream.flush();
                    let _ = stream.shutdown(std::net::Shutdown::Write);
                    // Let the client read before this end goes away.
                    let mut drain = [0_u8; 64];
                    let _ = stream.read(&mut drain);
                }
            });
            let target = Target::parse(&format!("http://127.0.0.1:{port}/insert"))
                .expect("a valid endpoint");
            let answered = target.post(&["{}".to_owned()], &AtomicBool::new(false));
            let _ = serving.join();
            assert_eq!(
                answered.taken(),
                taken,
                "`{}` was read as {}",
                answer.trim(),
                if answered.taken() {
                    "success"
                } else {
                    "a refusal"
                }
            );
        }
    }

    #[test]
    fn a_stderr_that_refuses_every_write_does_not_kill_the_drain() {
        // **`eprintln!` panics when the write fails**, and the fallback path is
        // exactly where that happens: stderr closed, redirected to a full disk,
        // or a pipe whose reader has gone. A panic there kills the one thread
        // draining the queue, so every subsequent `send` fills it and drops
        // silently — the store being down would turn logging off permanently,
        // which is the opposite of what the fallback is for.
        //
        // Measured with stderr on `/dev/full`: with `eprint!` the drain thread
        // dies on the first fallback write; with `to_stderr` it does not.
        // Without a test, restoring `eprint!` leaves the suite green.
        //
        // **Pointed at the failing writer.** A test that builds a broken pipe,
        // asserts *the pipe* refuses a write, and then calls `to_stderr` —
        // which writes to the harness's own working stderr — never points the
        // subject at anything that fails, so restoring `eprint!` leaves it
        // green: a title, not an assertion, which is `§19.5` exactly.
        //
        // A child process is what it takes: a test cannot redirect the
        // harness's own stderr without disturbing every other test in the
        // binary. `/dev/full` refuses every write with `ENOSPC`, and `2>&-` does
        // not work — the first socket reuses the closed descriptor.
        #[expect(
            clippy::disallowed_methods,
            reason = "a tool this gate drives; its output is read, not printed"
        )]
        let child = std::process::Command::new(std::env::current_exe().expect("this binary"))
            // The filter first, then `--exact`: libtest takes the filter as a
            // positional argument, and with the order reversed nothing matches,
            // zero tests run, and the child exits 0 — so this would pass with
            // the defect restored.
            .args([
                // The module's real path, which is not `tests`: this file has
                // two test modules. With a filter that matches nothing the
                // child runs zero tests and exits 0, so the assertion below
                // would pass with the defect restored — the same vacuity, one
                // layer out.
                "ship::hostile_store_tests::the_child_that_writes_to_a_full_stderr",
                "--exact",
                // **And `--nocapture`.** libtest replaces stderr with a capture
                // buffer, so without this the write never reaches the failing
                // descriptor at all: with `eprint!` restored the child writes
                // into the buffer, exits 0, and this passes.
                "--nocapture",
            ])
            .env("NC_LOG_STDERR_CHILD", "1")
            .stderr(
                #[expect(
                    clippy::disallowed_methods,
                    reason = "the descriptor under test is a full device, which is a handle \
                              and not a terminal"
                )]
                std::fs::OpenOptions::new()
                    .write(true)
                    .open("/dev/full")
                    .expect("/dev/full"),
            )
            // **Captured, not discarded.** Asserting only that the child exited
            // 0 is not enough — a child whose filter matches nothing runs zero
            // tests and exits 0 too, and the argument order, the module's name
            // and `--nocapture` are each a way to get there. Reading what the
            // child said is what stops a fourth: a module rename is a failure
            // here rather than a silent pass.
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("the child starts");
        let said = child.wait_with_output().expect("the child ends");
        let told = String::from_utf8_lossy(&said.stdout);
        assert!(
            told.contains("1 passed"),
            "the child ran no test, so this proves nothing about stderr: {told}"
        );
        assert!(
            said.status.success(),
            "writing to a stderr that refuses every write killed the process: {}",
            said.status
        );
    }

    #[test]
    fn the_child_that_writes_to_a_full_stderr() {
        struct Refused;
        impl crate::sealed::Sealed for Refused {}
        impl crate::Event for Refused {
            fn name(&self) -> &'static str {
                "probe.refused"
            }
            fn level(&self) -> crate::Level {
                crate::Level::Error
            }
            fn fields(&self) -> Vec<(&'static str, crate::Field)> {
                vec![("count", crate::Field::Unsigned(1))]
            }
        }

        // Only the child runs the body; the parent's copy returns at once, so
        // this costs nothing in an ordinary run.
        if std::env::var_os("NC_LOG_STDERR_CHILD").is_none() {
            return;
        }
        // With `eprint!` this panics and the process exits non-zero, which is
        // what the drain thread dying looks like from outside.
        for _ in 0..8 {
            // The return value says whether it landed, and on `/dev/full` it
            // does not — which is the point: the write is refused and the
            // process still lives.
            assert!(
                !super::to_stderr("nc-log: a line nobody can read\n"),
                "a stderr that refuses every write reported success"
            );
        }

        // **And through the writer thread, which is where the counting lives.**
        // Everything above calls `to_stderr` directly, so the arm that charges
        // a refused write to `LOST` — the point of counting a refused write at
        // all — runs nowhere else in the suite. Measured without this:
        // `LOST.fetch_add(0, …)` in that arm green, and so is
        // `LOST.fetch_add(1_000_000, …)`, which is what proves it never
        // executes.
        // `every_line_the_fallback_takes_is_written_queued_or_counted` cannot
        // see it either: it runs against the harness's working stderr, so
        // `SENT` alone satisfies `SENT + LOST`.
        //
        // Here the store is unreachable *and* stderr refuses, which is the only
        // state the `(false, false)` arm is reachable from. Read through
        // `Log::lost()` rather than the static, because that accessor is the
        // crate's public answer and this is a caller of it — its own doc is "a
        // number an operator cannot reach is not a number they have".
        let log = crate::Log::open(
            crate::Origin {
                service: crate::literal!("nc-log-stderr-child"),
                // **Deliberately not this tree's version.** The release gate
                // walks every file for the version it is at, so a fixture that
                // names the real one reads as a file the release would leave
                // behind. A fixture's version is arbitrary; this one says so.
                version: crate::literal!("9.9.9"),
            },
            // Port 1 on loopback: nothing listens, and the connection is refused
            // rather than hanging, so this costs no wall clock.
            "http://127.0.0.1:1/insert/jsonline",
        )
        .expect("the endpoint parses");
        let before = log.lost();
        for _ in 0..8 {
            log.record(&Refused);
        }
        // The writer thread charges asynchronously, so this waits for the number
        // rather than reading it once. Bounded: the failure is the whole budget
        // elapsing with the count unmoved.
        let mut after = before;
        for _ in 0..100 {
            after = log.lost();
            if after > before {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(
            after > before,
            "the store refused every line and stderr refused every fallback, and \
             `lost()` stayed at {before}: a line that reached neither is accounted \
             for nowhere, which is the outage this arm exists to count"
        );
        // **The number this cannot hold is held next door.** `after > before`
        // is an expectation that moves with the thing it measures —
        // `Shipper::lost()` wrapped in a `saturating_sub(1)` leaves it green —
        // and an equality here cannot say so either, because this body runs in a
        // child whose stderr is deliberately full: its panic message is
        // swallowed, so a wrong number arrives as "the child ran no test".
        // `the_accessor_answers_the_counter` reads the accessor against the
        // counter itself, in the parent, where a failure can be read.
    }

    /// A stderr that **blocks** is a pause with a number, not a silent outage.
    ///
    /// **`/dev/full` is the easy stderr failure**: it fails fast
    /// with `ENOSPC`. What a real deployment produces is a pipe nobody is
    /// draining — journald backpressure, an unread `docker logs`, a stopped
    /// terminal — and that *blocks*. Measured against one, with a hung store,
    /// 3000 lines and the write on the drain thread:
    ///
    /// ```text
    /// on_stderr(554) + reported_drops(1616) = 2170 of 3000
    /// ```
    ///
    /// 830 lines in no store, on no stderr, named by no number, and
    /// `/proc` showing `nc-log-stderr` in `anon_pipe_write` with the drain
    /// thread in `futex_do_wait` — the lock, not the pipe. Every one of those
    /// lines had also spawned a thread that would never return.
    ///
    /// Driven at the seam, because a child cannot demonstrate it: with stderr
    /// blocked, libtest's own harness blocks writing its result line, so the
    /// child never exits and the test measures the harness. Measured that way
    /// once — 626 s and a kill.
    ///
    /// What the seam can say is the property at stake: handing a line to the
    /// fallback never waits, and every line is in exactly one of three places
    /// afterwards.
    #[test]
    fn every_line_the_fallback_takes_is_written_queued_or_counted() {
        const LINES: u64 = 4_000;
        // The other side of the same lock: this hands over four thousand lines
        // and its sibling reads the counters as absolutes.
        let _counters = COUNTERS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before =
            super::SENT.load(super::Ordering::Relaxed) + super::LOST.load(super::Ordering::Relaxed);
        let started = std::time::Instant::now();
        for at in 0..LINES {
            super::to_stderr_line(format!("nc-log: accounting line {at}\n"));
        }
        let took = started.elapsed();

        // **Handing a line over does not wait for it.** This is the property an
        // unbounded `write_all` on the drain thread does not have: measured
        // against a pipe nobody was reading, the thread sat in `futex_do_wait`
        // and 830 of 3000 accepted lines reached neither the store nor stderr,
        // with nothing counting them. Four thousand lines against a working
        // stderr is far more than a real burst, and the bound is generous
        // because a loaded machine is not the subject.
        assert!(
            took < core::time::Duration::from_secs(10),
            "{LINES} lines took {took:?} to hand over, so the caller is waiting on stderr"
        );

        // The queue's depth is not asserted here: `still_queued()` reads two
        // counters that every other test in this binary also moves, so under
        // the full suite it measures the suite. What bounds it is the type —
        // `try_send` on a `sync_channel(STDERR_QUEUE)` cannot exceed capacity —
        // and what this test is for is the two properties that are not
        // structural.

        super::flush_stderr(core::time::Duration::from_secs(5));
        let after =
            super::SENT.load(super::Ordering::Relaxed) + super::LOST.load(super::Ordering::Relaxed);
        assert!(
            after >= before + LINES,
            "{} line(s) went with no number anywhere: {before} before, {after} after",
            before + LINES - after
        );
    }

    /// A shutdown charges its backlog to `LOST` once, not once per shutdown.
    ///
    /// **A charge to `LOST` that leaves the gauge alone is counted again.** The
    /// gauge `flush_stderr` reads is `SENT - WRITTEN`, so moving only `LOST`
    /// leaves it unchanged by the charge and the next shutdown counts the same
    /// lines again. Measured with the writer thread blocked on a stderr lock:
    /// 899 queued, `LOST` 899 after one flush and 1799 after two. `FLUSH`'s own
    /// doc contemplates a process holding two of these, and `Log::open` is
    /// public.
    ///
    /// **Driven through a real blocked stderr, because the arithmetic is the
    /// hiding place.** A test that adds 64 to `SENT` and asserts about the
    /// gauge — no lines, no channel, no writer — on the grounds that "the
    /// property is arithmetic" cannot see the defect: the charge and the drain
    /// are two halves of one invariant, and a test that fabricates one half
    /// cannot see the other. What it misses: `flush_stderr` taking its backlog
    /// out of the gauge by adding it to `WRITTEN`, while the writer thread —
    /// still alive — adds to `WRITTEN` again for each of those same lines when
    /// it dequeues them. The overshoot is permanent, so every later shutdown in
    /// the process reads `still_queued() == 0`, skips its whole grace period
    /// and charges nothing:
    ///
    /// ```text
    /// second shutdown: 40 handed over with stderr blocked
    ///   still_queued() 0   LOST charged 0   flush_stderr waited 681ns of 3s
    /// ```
    ///
    /// So this holds the process-wide stderr lock, which is what a blocked
    /// stderr *is*, and hands real lines to the real channel. The counters are
    /// process-wide and every test in this binary moves them, so it reads deltas
    /// and never absolutes.
    #[test]
    fn a_backlog_is_charged_to_lost_once_and_not_once_per_shutdown() {
        const LINES: u64 = 64;
        let _counters = COUNTERS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // **The writer is blocked here, not absent.** It dequeues one line and
        // stops inside `to_stderr` waiting for this guard, which is exactly the
        // state a stderr nobody is draining leaves behind.
        #[expect(
            clippy::disallowed_methods,
            reason = "the stderr fallback: a log store that is down must not stop the platform"
        )]
        let held = std::io::stderr().lock();
        for index in 0..LINES {
            super::to_stderr_line(format!("nc-log: probe {index}\n"));
        }
        let queued = super::still_queued();
        assert!(
            queued >= LINES - 2,
            "only {queued} of {LINES} line(s) are queued, so stderr is not blocked and \
             this test is measuring nothing"
        );

        let lost_before = super::LOST.load(super::Ordering::Relaxed);
        super::flush_stderr(core::time::Duration::from_millis(1));
        let after_one = super::LOST.load(super::Ordering::Relaxed);
        // Within two: the writer dequeues one line and blocks *inside*
        // `to_stderr` holding it, so the gauge can move by one between the
        // reading above and the charge. The property is that the whole backlog
        // was charged, not that no line was in flight.
        assert!(
            after_one >= lost_before + queued.saturating_sub(2),
            "the backlog was not charged at all: {lost_before} then {after_one}, with \
             {queued} queued"
        );

        // **The same backlog, twice — with stderr still held.** `still_queued`
        // subtracts `CHARGED` precisely so a second shutdown does not charge a
        // backlog the first already charged, and a test of `CHARGED` that
        // drains the queue between its two calls decides nothing about the
        // subtraction. `Log::open` is public, so a process holding two shippers
        // is not hypothetical, and `Log::lost` is the one number an operator
        // has. Measured with the term removed and the lock still held:
        // `63 line(s) were still waiting for stderr` and then `64 line(s)`.
        super::flush_stderr(core::time::Duration::from_millis(1));
        let after_two = super::LOST.load(super::Ordering::Relaxed);
        assert!(
            after_two <= after_one + 2,
            "a second shutdown charged the same backlog again — {after_one} then \
             {after_two}, with {queued} queued and stderr still blocked, so the number \
             an operator reads doubles"
        );

        // **stderr comes back and the writer drains what it was holding.**
        // Those lines reached stderr after all, and `Log::lost` is documented
        // as "how many lines reached neither the store nor stderr" — so the
        // charge is taken back as each one lands. A charge left standing, with
        // the same lines also counted in `WRITTEN`, is in neither place and
        // named by no number, which is the state `flush_stderr` exists to
        // prevent.
        drop(held);
        for _ in 0..300 {
            if super::still_queued() == 0 && super::CHARGED.load(super::Ordering::Relaxed) == 0 {
                break;
            }
            std::thread::sleep(core::time::Duration::from_millis(10));
        }
        let settled = super::LOST.load(super::Ordering::Relaxed);
        assert!(
            settled <= lost_before + 2,
            "{} line(s) are still counted lost after stderr took them: {lost_before} \
             before, {after_one} charged, {settled} now",
            settled.saturating_sub(lost_before)
        );

        // The second shutdown, over a drained queue: nothing is waiting, so
        // nothing new is lost.
        super::flush_stderr(core::time::Duration::from_millis(1));
        assert_eq!(
            super::LOST.load(super::Ordering::Relaxed),
            settled,
            "a shutdown with an empty queue charged something to LOST"
        );

        // **And a new backlog reads as a backlog, which is the half no
        // arithmetic can show.** With the charge folded into `WRITTEN` the
        // overshoot is permanent — `still_queued()` saturates at zero for that
        // many lines for the rest of the process, so every later shutdown skips
        // its whole grace period and charges nothing. Measured: 40 lines handed
        // over with stderr blocked, `still_queued() == 0`, `flush_stderr`
        // waited 681 ns of its 3 seconds.
        #[expect(
            clippy::disallowed_methods,
            reason = "the stderr fallback: a log store that is down must not stop the platform"
        )]
        let held = std::io::stderr().lock();
        for index in 0..LINES {
            super::to_stderr_line(format!("nc-log: second probe {index}\n"));
        }
        let again = super::still_queued();
        drop(held);
        assert!(
            again >= LINES - 2,
            "a second backlog of {LINES} read as {again} queued, so the gauge never \
             recovered from the first charge — every later shutdown skips its whole \
             grace period and charges nothing"
        );
    }

    /// The line handed over takes up the gap the last refusal left.
    ///
    /// **The `swap` between them is held here.** `UNREPORTED` is written where
    /// a handover is refused and read where the next line is built, and the
    /// test below drives the three expressions around it directly — never the
    /// function that reads the static. Measured without this, the read replaced
    /// by `0`: this crate green and `cargo xtask check` at `check passed`,
    /// with `nc-log: N line(s) were lost while stderr was not draining` never
    /// appearing again for the life of the process.
    ///
    /// Under `COUNTERS`, because `UNREPORTED` is process-wide and this test both
    /// writes and reads it.
    #[test]
    fn the_line_handed_over_takes_up_the_gap_left_behind() {
        let _held = super::hostile_store_tests::COUNTERS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = super::UNREPORTED.swap(3, Ordering::Relaxed);
        let (said, carried) = super::the_line_to_write("nc-log: a line\n".to_owned());
        assert!(
            said.starts_with("nc-log: 3 line(s) were lost while stderr was not draining\n"),
            "the gap the last refusal left was not taken up: {said}"
        );
        assert_eq!(
            carried, 4,
            "a refusal of this line would carry a number the next notice cannot report"
        );
        // And it was *taken*: read twice, the same gap would be reported twice
        // and the count would never come down.
        let (again, _) = super::the_line_to_write("nc-log: another\n".to_owned());
        assert_eq!(
            again, "nc-log: another\n",
            "the gap was reported and not cleared, so every later line repeats it: {again}"
        );
        super::UNREPORTED.store(before, Ordering::Relaxed);
    }

    /// The handover is the one that does not wait.
    ///
    /// **"Enqueuing never blocks", measured where waiting is
    /// possible.** `every_line_the_fallback_takes_is_written_queued_or_counted`
    /// hands over 4 000 lines and asserts it took under ten seconds — against
    /// the harness's *working* stderr, where the writer drains and the queue
    /// never fills. So with only that test, restoring the blocking `send`
    /// leaves this crate green and `cargo xtask check` at `check passed`.
    ///
    /// Measured with stderr blocked and `STDERR_QUEUE + 200` lines handed over
    /// from a worker thread:
    ///
    /// ```text
    /// try_send  ->  Ok(291.179µs)
    /// send      ->  Err(Timeout)   at a five-second ceiling
    /// ```
    ///
    /// **This is a source assertion and not that measurement**, said plainly:
    /// reproducing it needs the test binary's own stderr blocked for the length
    /// of the flood, which is a process-wide condition the rest of this suite
    /// writes through. What it holds is the one word that decides —
    /// `to_stderr_line` is called *from the drain thread*, five times in its
    /// `recv` loop and three more in `Drop`, so a handover that waits puts that
    /// thread back inside the stderr write. This file records what that looked
    /// like: `on_stderr(554) + reported_drops(1616) = 2170 of 3000`, with 830
    /// lines named by no number and the thread in `futex_do_wait`.
    #[test]
    fn the_handover_is_the_one_that_does_not_wait() {
        let source = include_str!("ship.rs")
            .split("mod hostile_store_tests")
            .next()
            .expect("this file has a module of hostile-store tests");
        let handing = source
            .split("fn to_stderr_line")
            .nth(1)
            .expect("the handover is a function of this file")
            .split("\nfn ")
            .next()
            .expect("a function ends where the next one begins");
        assert!(
            handing.contains("writer.try_send(text)"),
            "the handover no longer refuses rather than waits. `to_stderr_line` runs on the \
             drain thread, so a `send` that waits puts that thread back inside the stderr \
             write — measured at 2170 of 3000 lines accounted for, 830 named by no number, \
             and the thread in `futex_do_wait`"
        );
        assert!(
            !handing.contains("writer.send("),
            "the handover waits for room in the queue: {handing}"
        );
        // And the call site of the read above, which is the other seam this pair
        // of tests holds: the line and its gap are decided in one call.
        assert!(
            handing.contains("the_line_to_write(text)"),
            "the backlog is read somewhere other than where the line is built, which is \
             the seam `the_line_handed_over_takes_up_the_gap_left_behind` drives"
        );
    }

    /// The gap a refused line leaves is carried, and the next line names it.
    ///
    /// **Neither half of this is decided by anything else.** `UNREPORTED`
    /// appears at three places in the workspace — its declaration, the swap and
    /// the add — and outside these tests is read by none; the sentence appears
    /// once. Measured without them: deleting the notice outright, and dropping
    /// the carried number from the add, each leave `check passed`. The second
    /// is the worse one: the notice then says `1 line(s)` for ever while the
    /// real backlog grows.
    ///
    /// This module's own header states the invariant as "every line handed to
    /// the fallback is in exactly one of three places — written, queued, or
    /// counted as lost — and none of the three is silence", and the sentence
    /// below is the whole of "counted as lost" reaching a person.
    #[test]
    fn a_refused_line_carries_its_gap_and_the_next_one_names_it() {
        // Three refusals in a row, each carrying what the one before left.
        let mut backlog = 0_u64;
        for _ in 0..3 {
            backlog = super::the_backlog_after_a_refusal(backlog);
        }
        assert_eq!(
            backlog, 3,
            "a refused line dropped the gap it was carrying, so the notice \
             under-reports for the rest of the process"
        );

        let said = super::with_the_backlog_notice("nc-log: a line\n".to_owned(), backlog);
        assert!(
            said.starts_with("nc-log: 3 line(s) were lost while stderr was not draining\n"),
            "the gap was not named, or not named before the line it belongs to: {said}"
        );
        assert!(
            said.ends_with("nc-log: a line\n"),
            "the line the notice was put in front of is gone: {said}"
        );

        // And no gap is no notice: a prefix on every line would be noise a
        // reader learns to skip.
        assert_eq!(
            super::with_the_backlog_notice("nc-log: a line\n".to_owned(), 0),
            "nc-log: a line\n"
        );

        // **And the two together, which is what the call site does.** Held
        // apart, each expression is driven and the wiring between them is not:
        // without this, `the_backlog_after_a_refusal(0)` at the call site leaves
        // this crate's tests and the whole gate green. One call, one backlog,
        // both answers.
        let (said, carried) = super::the_line_and_what_it_carries("nc-log: a line\n".to_owned(), 3);
        assert!(
            said.starts_with("nc-log: 3 line(s) were lost while stderr was not draining\n"),
            "the line does not name the gap it was handed: {said}"
        );
        assert_eq!(
            carried, 4,
            "a refusal of that line would carry a number the next notice cannot report"
        );
    }

    #[test]
    fn a_store_that_trickles_forever_is_held_to_the_deadline() {
        // **`DEADLINE` bounds the whole exchange, not each read.** Its own doc
        // claims "the whole exchange is bounded and not each read of it", and a
        // read timeout set **once**, before the first read, would not do that —
        // `set_read_timeout` bounds each read, so a store writing one byte
        // every 100 ms and never a newline would hold `post` for 25 seconds
        // against a 10-second deadline. The worst case would be `STATUS_LIMIT`
        // reads of `DEADLINE` each: about 22 hours on the one thread draining
        // the queue, with every line dropped meanwhile.
        //
        // `every_timing_bound_is_the_number_its_paragraph_argues_for` pins the
        // constant at ten seconds, and a read timeout set once passes that pin;
        // this is what holds the whole exchange to it.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("bound").port();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let told = std::sync::Arc::clone(&stop);
        let trickling = std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            // Never a newline, and never idle long enough to trip a per-read
            // timeout — which is exactly why a per-read timeout is not a bound.
            while !told.load(std::sync::atomic::Ordering::Relaxed) {
                if stream.write_all(b"H").is_err() || stream.flush().is_err() {
                    break;
                }
                std::thread::sleep(core::time::Duration::from_millis(100));
            }
        });

        let target =
            Target::parse(&format!("http://127.0.0.1:{port}/insert")).expect("a valid endpoint");
        let started = std::time::Instant::now();
        let answer = target.post(&["{}".to_owned()], &AtomicBool::new(false));
        let took = started.elapsed();
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = trickling.join();

        assert!(
            !answer.taken(),
            "a store that never answers reported success"
        );
        // **An absolute bound, not one written in terms of the constant it
        // guards.** `took < DEADLINE * 2` is satisfied for every value of
        // `DEADLINE` — so an assertion meant to stop the constant being raised
        // unnoticed could not notice it being raised. Measured: `DEADLINE` set
        // to 100 000 seconds, that assertion green in 14.7 s.
        //
        // Twenty seconds, so a loaded machine does not make this flaky while
        // still failing outright on the 25 seconds a per-read timeout takes —
        // and failing on any `DEADLINE` a person would have to be told about.
        assert!(
            took < core::time::Duration::from_secs(20),
            "post took {took:?}, and the exchange is supposed to be bounded by \
             DEADLINE ({DEADLINE:?}) — the deadline bounds each read rather than \
             the exchange, or DEADLINE itself has been raised"
        );
    }

    #[test]
    fn a_budget_that_runs_out_says_so_rather_than_naming_an_errno() {
        // **The timeout message is a needle unless something reaches it.** In
        // an `if left.is_zero()` at the top of the loop it is never reached —
        // the loop never goes round again with a spent budget, because the
        // retry arm requires `slice < left`, and when `left <= POLL` they are
        // equal — and every expiry is reported as the raw errno:
        //
        // ```text
        //   the store stopped answering: Resource temporarily unavailable (os error 11)
        // ```
        //
        // A reader meets an operating-system message where the sentence
        // explains that the thread draining the queue waited ten seconds. That
        // is the shape `verdict`'s own doc condemns: a kind nothing exercises.
        //
        // A store that sends one byte and then nothing, which is the cheapest
        // way to reach it.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("bound").port();
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let told = std::sync::Arc::clone(&stop);
        let holding = std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut sink = [0_u8; 4096];
            let _ = stream.read(&mut sink);
            let _ = stream.write_all(b"H");
            let _ = stream.flush();
            while !told.load(Ordering::Relaxed) {
                std::thread::sleep(core::time::Duration::from_millis(20));
            }
        });

        let target =
            Target::parse(&format!("http://127.0.0.1:{port}/insert")).expect("a valid endpoint");
        // Abandoning, so the budget is `ABANDONED_ANSWER` and this test takes
        // that rather than the ten-second `DEADLINE` — the branch is the same
        // one and the message names whichever budget applied.
        let answer = target.post(&["{}".to_owned()], &AtomicBool::new(true));
        stop.store(true, Ordering::Relaxed);
        let _ = holding.join();

        let said = format!("{answer:?}");
        assert!(
            said.contains("took longer than"),
            "a spent budget was reported as something else: {said}"
        );
        assert!(
            said.contains("drains the queue"),
            "the message does not say why the wait matters: {said}"
        );
        assert!(
            !answer.taken(),
            "a store that never finished answering reported success"
        );
    }

    #[test]
    fn a_store_that_pauses_inside_its_status_line_is_still_read() {
        // **The deadline applies after the first byte too.** A retry arm gated
        // on `status.is_empty()` lets a gap longer than one `POLL` anywhere
        // *inside* the status line fall through to a verdict — `Unreadable`,
        // and where that is not kept, the lines with it. Two measured
        // consequences:
        //
        // ```text
        // "HTTP" … 400ms … "/1.1 500 Nope"  -> 60 recorded, 0 in the store,
        //                                      0 on stderr, dropped() = 0
        // a valid 200 one byte per 200ms    -> the store took all 60 lines and
        //                                      it was reported as a failure,
        //                                      in 207ms against a 10s budget
        // ```
        //
        // The first is lines that exist nowhere, which is the one thing this
        // module promises cannot happen. Both halves are here: a refusal
        // arriving in two segments has to be read as a refusal, and an
        // acceptance arriving in two segments has to be read as an acceptance.
        for (head, tail, taken) in [
            ("HTTP", "/1.1 500 Nope\r\n\r\n", false),
            ("HTTP/1.1 2", "00 OK\r\n\r\n", true),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
            let port = listener.local_addr().expect("bound").port();
            let answering = std::thread::spawn(move || {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let mut sink = [0_u8; 4096];
                let _ = stream.read(&mut sink);
                if stream.write_all(head.as_bytes()).is_err() {
                    return;
                }
                let _ = stream.flush();
                // Four `POLL` slices, which a guard on the first byte treats as
                // the end of the exchange.
                std::thread::sleep(core::time::Duration::from_millis(400));
                let _ = stream.write_all(tail.as_bytes());
                let _ = stream.flush();
            });

            let target = Target::parse(&format!("http://127.0.0.1:{port}/insert"))
                .expect("a valid endpoint");
            let answer = target.post(&["{}".to_owned()], &AtomicBool::new(false));
            let _ = answering.join();

            assert_eq!(
                answer.taken(),
                taken,
                "a status line that arrived in two segments was read as {answer:?}"
            );
            if !taken {
                assert!(
                    answer.keep_on_stderr(),
                    "a refusal that arrived in two segments was read as {answer:?}, and \
                     those lines are then in no store and on no stderr"
                );
            }
        }
    }

    #[test]
    fn the_two_bounds_are_small_enough_to_be_bounds() {
        // A floor on the constants themselves: a `STATUS_LIMIT` of a gigabyte or
        // a `CONNECT_TIMEOUT` of an hour would satisfy every assertion above and
        // bound nothing.
        const { assert!(STATUS_LIMIT <= 64 * 1024) };
        const { assert!(CONNECT_TIMEOUT.as_secs() <= 5) };
    }

    /// Every timing bound in this file is the number its paragraph argues for.
    ///
    /// **Pinned here, because a bound built *from the constants under test*
    /// pins nothing:**
    ///
    /// ```rust,ignore
    /// let bound = FLUSH + GRACE + GRACE + Duration::from_secs(6);
    /// ```
    ///
    /// — which every value satisfies. Measured with only that bound, each
    /// alone, `cargo xtask check` green in all three cases: `GRACE` 3 s → 30 s,
    /// making the documented eighteen-second shutdown seventy-two per
    /// shipper; `DEADLINE` 10 s → 2 s, the state whose own paragraph records it
    /// tripling delivery and costing 1.2 points of loss; and `FLUSH` 12 s → 4
    /// s, the state that paragraph calls out by name — *"at four `FLUSH`
    /// silently becomes the deadline for the last batch of every CLI run and
    /// every restart"*.
    ///
    /// The relation is written down as well as the numbers, because it is the
    /// relation the paragraph argues and a pair of values can satisfy each bound
    /// separately while breaking it.
    #[test]
    fn every_timing_bound_is_the_number_its_paragraph_argues_for() {
        // **`QUEUE` is pinned here.** Three paragraphs in this file do
        // arithmetic on it — `QUEUE / BATCH` requests to drain a full queue,
        // `FLUSH / (QUEUE / BATCH)` as the store speed that reaches the flush
        // bound, `QUEUE + 128` as the stderr queue's own size — and none of
        // them holds it. Measured without this, `QUEUE = 20_000`:
        // `cargo test -p nc-log` green and `check passed`, with `BATCH`'s own
        // pin still green while the sentence beside it — *"a full queue is
        // `QUEUE / BATCH` requests, and at sixty-four that is sixteen"* — has
        // become 313.
        //
        // Downwards it *is* held elsewhere: `QUEUE = 8` fails seven
        // hostile-store tests. What is loose is upwards, because the tests that
        // would notice derive their totals from `QUEUE` itself — an expectation
        // that grows with the thing it measures.
        assert_eq!(
            QUEUE, 1_000,
            "long enough to ride out a restart of the store and short enough that the \
             memory is bounded at something a person can name, and the number three \
             paragraphs in this file do arithmetic on"
        );
        assert_eq!(
            DEADLINE.as_secs(),
            10,
            "a store slower than this is duplicated rather than delivered: at two \
             seconds every batch to a slower store went to stderr *and* landed"
        );
        assert_eq!(
            FLUSH.as_secs(),
            12,
            "longer than one `DEADLINE`, so the last batch of a run gets its whole \
             exchange rather than being cut short by the shutdown budget"
        );
        assert_eq!(
            GRACE.as_secs(),
            3,
            "the stderr drain's share of the shutdown, and the second half of the \
             eighteen seconds a shipper's whole exit is bounded at"
        );
        assert_eq!(GIVE_UP_AFTER, 2);
        assert_eq!(
            BATCH, 64,
            "a full queue is `QUEUE / BATCH` requests, and this is the number that \
             makes it sixteen: smaller multiplies the connections and the `DEADLINE` \
             exposures for one queue, larger widens the one-batch duplication ceiling \
             `lines_the_flush_runs_out_of_time_for_are_counted_and_kept` allows"
        );
        // **And the three no other test pins to a number.** Outside this test,
        // `super::WRITE_TIMEOUT` is asserted only against measured times, so
        // five seconds could become five hours with every other test in this
        // file green while the one thread draining the queue hangs on a store
        // that accepted the connection and then stopped reading, which is
        // verbatim the state its own paragraph exists to prevent. Loosening it
        // that far also loosens the comparison that bounds
        // `super::ABANDONED_ANSWER` from above.
        assert_eq!(
            super::WRITE_TIMEOUT.as_secs(),
            5,
            "the bound on the one thread draining the queue, against a store that \
             accepted the connection and stopped reading"
        );
        assert_eq!(
            super::ABANDONED_ANSWER.as_millis(),
            200,
            "what a leaving process waits for an answer already written to the socket"
        );
        assert_eq!(
            super::POLL.as_millis(),
            100,
            "how long a read may hide the shutdown flag"
        );
        // The relations these three paragraphs argue, each of which a pair of
        // values can break while satisfying every bound separately.
        const {
            assert!(
                super::CONNECT_TIMEOUT.as_millis() + super::WRITE_TIMEOUT.as_millis()
                    < super::DEADLINE.as_millis()
            );
        }
        const { assert!(super::POLL.as_millis() <= super::ABANDONED_ANSWER.as_millis()) };
        // **And the last two, which this test's own name promises.**
        // `CONNECT_TIMEOUT` is otherwise pinned only by
        // `the_two_bounds_are_small_enough_to_be_bounds`, which allows anything
        // up to 5 s, and `STATUS_LIMIT` only by the same test allowing anything
        // up to 64 KiB — so 2 s → 4 s and 8 KiB → 64 KiB together leave
        // `check passed`. That test's own doc says its purpose is that "a
        // `STATUS_LIMIT` of a gigabyte or a `CONNECT_TIMEOUT` of an hour would
        // satisfy every assertion above and bound nothing"; a factor of eight
        // is not a gigabyte, and it is not a pin either.
        assert_eq!(
            super::CONNECT_TIMEOUT.as_secs(),
            2,
            "a connect to an address that drops packets holds the one thread draining \
             the queue until the OS gives up on its SYN retries"
        );
        assert_eq!(
            super::STATUS_LIMIT,
            8 * 1024,
            "a status line is under a hundred bytes, and a store answering steadily \
             with no newline trips no read timeout"
        );
        const { assert!(super::ABANDONED_ANSWER.as_millis() < super::WRITE_TIMEOUT.as_millis()) };
        // The relation, which is what the paragraph actually argues.
        const { assert!(FLUSH.as_secs() > super::DEADLINE.as_secs()) };
        assert_eq!(
            (FLUSH + GRACE + GRACE).as_secs(),
            18,
            "`FLUSH`'s own doc states eighteen as the platform-visible number, and \
             nothing held it"
        );
    }

    /// Handing a line over does not wait, with stderr genuinely blocked.
    ///
    /// **What a source assertion cannot measure.** `the_handover_is_the_one_that_does_not_wait`
    /// reads this file's own source for the word `try_send`, because
    /// `every_line_the_fallback_takes_is_written_queued_or_counted` measures the
    /// handover against the harness's *working* stderr — where the writer
    /// drains, the queue never fills, and a blocking `send` never has to wait.
    /// Measured with stderr blocked and `STDERR_QUEUE + 200` lines handed over
    /// from a worker thread:
    ///
    /// ```text
    /// try_send  ->  Ok(291.179µs)
    /// send      ->  Err(Timeout)   at a five-second ceiling
    /// ```
    ///
    /// A child process is what makes that a test rather than a paragraph, for
    /// the reason the `/dev/full` pair gives: a test cannot block the harness's
    /// own stderr without blocking every other test in the binary. Here the
    /// child's stderr is a pipe **the parent never reads**, so the pipe buffer
    /// fills, the writer thread blocks inside `write_all`, the queue behind it
    /// fills, and the handover has to decide. The child says which on its
    /// **stdout**, which the parent does drain.
    ///
    /// `wait_with_output` is deliberately not used: it drains stderr, and a
    /// drained stderr is the state that measures nothing.
    ///
    /// Two floors, because "it returned" is also true of a `send` that never had
    /// to wait: the child reports how many lines the full queue **refused**, and
    /// what the next line's backlog notice says. Both are zero unless stderr was
    /// really blocked.
    #[test]
    fn the_handover_does_not_wait_on_a_stderr_nobody_is_draining() {
        #[expect(
            clippy::disallowed_methods,
            reason = "a tool this gate drives; its output is read, not printed"
        )]
        let mut running = std::process::Command::new(std::env::current_exe().expect("this binary"))
            // Filter first, `--exact` after, the module's real path, and
            // `--nocapture`: each of those four, got wrong, makes a child test
            // vacuous, and the "1 passed" assertion below is what catches a
            // fifth.
            .args([
                "ship::hostile_store_tests::the_child_that_floods_a_blocked_stderr",
                "--exact",
                "--nocapture",
            ])
            .env("NC_LOG_BLOCKED_STDERR_CHILD", "1")
            .stdout(std::process::Stdio::piped())
            // Piped and never read. That is the whole fixture.
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("the child starts");
        let mut out = running.stdout.take().expect("stdout is piped");
        let (said, heard) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut told = String::new();
            let _ = out.read_to_string(&mut told);
            let _ = said.send(told);
        });
        // Bounded, because a child whose handover really waits would otherwise
        // hold this test for as long as the pipe stays full.
        let told = heard
            .recv_timeout(core::time::Duration::from_secs(120))
            .unwrap_or_default();
        let _ = running.kill();
        let _ = running.wait();

        assert!(
            told.contains("1 passed"),
            "the child ran no test, so this proves nothing about the handover: {told}"
        );
        // **Found in the line, not at the start of it.** `strip_prefix` assumes
        // the child's `--nocapture` output begins a line, which is true only
        // when the child's harness runs at concurrency two or more: at one,
        // libtest writes `test <name> ... ` with no newline and the child's
        // first line lands on the end of it. So with `strip_prefix` this test
        // passes or fails according to `RUST_TEST_THREADS` in the environment
        // that started it — a premise borrowed from the machine rather than
        // established here. Measured with it set to 1.
        let verdict = told
            .lines()
            .find_map(|line| line.split_once("nc-log-handover: ").map(|(_, said)| said))
            .unwrap_or_else(|| panic!("the child said nothing about the handover: {told}"));
        assert!(
            verdict.contains("returned"),
            "the handover waited for room in a queue nothing was draining: `{verdict}`. \
             `to_stderr_line` runs on the drain thread — five times in its `recv` loop \
             and three more in `Drop` — so a handover that waits puts that thread back \
             inside the stderr write: measured at `on_stderr(554) + reported_drops(1616) \
             = 2170 of 3000`, with 830 lines named by no number"
        );
        let refused: u64 = verdict
            .split("refused=")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|count| count.parse().ok())
            .unwrap_or_else(|| panic!("the child did not say how many were refused: {verdict}"));
        // **A hundred and not one**, because a working stderr refuses a handful
        // too: measured with the child's stderr on `/dev/null`, the producer
        // outran the writer and `refused=9` — transient fullness, drained a
        // moment later. A blocked pipe absorbs one buffer and then refuses every
        // line past `STDERR_QUEUE`, which is hundreds.
        assert!(
            refused >= 128,
            "the full queue refused only {refused} line(s), which a *working* stderr \
             also does when the producer outruns the writer — so nothing here says the \
             pipe was blocked: `{verdict}`"
        );
        // **And the number a refusal leaves behind reaches a person.**
        // `UNREPORTED` is written at the refusal and read where the next line
        // is built, and the three tests around that pair drive the two halves
        // directly — so without this the *write* is held by nothing: measured,
        // the add replaced by `UNREPORTED.fetch_add(0, …)`, this crate green
        // and `cargo xtask check` at `check passed`, with
        // `nc-log: N line(s) were lost while stderr was not draining` never
        // appearing again for the life of the process. Here the refusals are
        // real and the notice is read back off the child's stdout.
        let notice = told
            .lines()
            .find_map(|line| line.split_once("nc-log-backlog: ").map(|(_, said)| said))
            .unwrap_or_else(|| panic!("the child said nothing about the backlog: {told}"));
        let carried: u64 = notice
            .strip_prefix("nc-log: ")
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|count| count.parse().ok())
            .unwrap_or_else(|| {
                panic!(
                    "the line after {refused} refusal(s) named no backlog at all, so \
                     \"counted as lost\" reaches nobody: `{notice}`"
                )
            });
        assert!(
            carried >= 128 && notice.contains("were lost while stderr was not draining"),
            "the line after {refused} refusal(s) carried {carried}: `{notice}`"
        );
    }

    /// The child of the test above: it floods a stderr nobody is draining.
    #[test]
    fn the_child_that_floods_a_blocked_stderr() {
        // Only the child runs the body; the parent's copy returns at once, so
        // this costs an ordinary run nothing.
        if std::env::var_os("NC_LOG_BLOCKED_STDERR_CHILD").is_none() {
            return;
        }
        // Four kilobytes a line, so one pipe buffer is full after sixteen of
        // them: what has to be under test is the queue behind the blocked
        // writer, not the buffer in front of it.
        let padding = "x".repeat(4 * 1024);
        let over = u64::try_from(super::STDERR_QUEUE).expect("a small number") + 512;
        let (done, finished) = std::sync::mpsc::sync_channel::<u64>(1);
        std::thread::Builder::new()
            .name("nc-log-flood".to_owned())
            .spawn(move || {
                for index in 0..over {
                    super::to_stderr_line(format!("nc-log: blocked {index} {padding}\n"));
                }
                let _ = done.send(super::LOST.load(Ordering::Relaxed));
            })
            .expect("the flood thread starts");
        // The verdict this exists to produce: the handover returned, or it is
        // still waiting for room that is never coming. Not joined — if it waits,
        // that thread is inside a `send` for a queue nothing will drain, and the
        // process leaves without it, which is what a blocked stderr costs.
        let verdict = match finished.recv_timeout(core::time::Duration::from_secs(30)) {
            Ok(refused) => format!("returned, refused={refused}"),
            Err(_) => "waited".to_owned(),
        };
        // **Refused a hundred and twenty-eight times in a row before the notice
        // is built.** A handover the queue takes carries the backlog so far
        // into its line, so a writer that dequeues one more line late in the
        // flood takes the count into a line it will never write, and the notice
        // built next names only what came after it: measured under a gate's
        // parallel load, 502 refused and 5 carried. Handed over until the queue
        // has refused this many in a row — which a pipe nobody drains always
        // comes to — the notice carries at least as many, however the writer
        // ran.
        let (mut in_a_row, more) = (0_u32, "x".repeat(4 * 1024));
        let tries = u64::try_from(super::STDERR_QUEUE).expect("a small number") * 16;
        for index in 0..tries {
            let before = super::LOST.load(Ordering::Relaxed);
            super::to_stderr_line(format!("nc-log: after {index} {more}\n"));
            if super::LOST.load(Ordering::Relaxed) > before {
                in_a_row += 1;
                if in_a_row >= 128 {
                    break;
                }
            } else {
                in_a_row = 0;
            }
        }
        // Said on stdout, which the parent drains — stderr is the blocked pipe
        // under test, and a word written there would join the queue rather than
        // reach anybody.
        #[expect(
            clippy::print_stdout,
            reason = "the child's only channel out: stderr is the blocked pipe under test"
        )]
        {
            println!("nc-log-handover: {over} handed over and it {verdict}");
            // And what the next line would carry, which is the whole of "counted
            // as lost" reaching a person.
            let (line, _) = super::the_line_to_write("nc-log: after the flood\n".to_owned());
            println!(
                "nc-log-backlog: {}",
                line.lines().next().unwrap_or("nothing at all")
            );
        }
    }
}
