//! The central log: what it may hold, and how a line reaches it.
//!
//! `NOCTUA.md §15.4`. **A central log that receives and displays all errors and
//! logs from the entire platform, interactively** — and that **must not contain
//! user data**, because it stays active in production.
//!
//! That last requirement is the whole design. It is not a rule people follow; it
//! is a bound the compiler enforces:
//!
//! * Every event is a **generated struct** from `catalog/log-events.toml`
//!   ([`events`], written by `cargo xtask build`).
//! * Every field of one is bounded by [`LogValue`], which is implemented for
//!   integers, booleans, durations, error codes, opaque identifier newtypes,
//!   [`Variant`] and [`Literal`], and for **no string type at all** — and **not** for `String` or `&str`.
//! * A source gate rejects `format!` and interpolation at logging call sites, so
//!   the trait cannot be circumvented by pre-formatting a leaked string.
//!
//! The first two are here; the third is `xtask`'s. Together they are one
//! guarantee, and either alone is a rule rather than a guarantee.
//!
//! # What reaches the store, and what does not
//!
//! An event carries its own name, its level, the service and its version, and
//! its declared fields. It does **not** carry a message a caller wrote: there is
//! no such field, because a free-form message is where a name ends up. What a
//! reader sees instead is the event's *description*, which lives in the registry
//! beside its declaration and is the same for every occurrence.

#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
// Every doctest of this crate forbids `unsafe` as the crate does: a
// doctest is a crate of its own, outside the root's `forbid`.
#![doc(test(attr(forbid(unsafe_code))))]
pub mod events;
pub mod ship;
pub mod value;

use ship::Shipper;

pub use value::{Field, Literal, LogValue, Source, Variant};

use std::collections::BTreeMap;

use nc_catalog::Level;

/// What every generated event answers to.
pub trait Event: sealed::Sealed {
    /// The event's dotted name, from the registry.
    fn name(&self) -> &'static str;
    /// The level it is emitted at.
    fn level(&self) -> Level;
    /// Its fields, in the order the registry declares them.
    fn fields(&self) -> Vec<(&'static str, Field)>;
}

/// Which service is speaking, and which build of it.
///
/// `§15.4`'s own table lists "service and version" among what the central log
/// holds, and neither can come from an event's fields: every event would have to
/// declare them, and one that forgot would be a line nobody can place.
/// **`Literal`, not `&'static str`** — and that is a correction. These two were
/// bare `pub &'static str` fields, the only ones in the crate with no wall in
/// front of them, in the two values that appear on **every** line the log emits.
/// A `static OnceLock<String>` yields a genuine `&'static str` from run-time
/// input with no leak spelling anywhere, which is the exact attack
/// `a_leaked_string_cannot_reach_a_literal.rs` names in its own doc comment: the
/// fixture defended `Literal` against it and left `Origin` open beside it.
///
/// Measured before the fix: `Origin { service: held, .. }` where `held` came
/// from a `OnceLock` put a tenant's address in the `service` field of every
/// line, past every compile-fail fixture and a source gate.
///
/// **A number in this sentence has now been wrong twice.** It said *"past nine
/// of them"* when the directory held eleven, was corrected to eleven, and the
/// directory holds twelve — a twelfth fixture arrived in round thirty-four to
/// cover `version` by name. The number belongs to the run that counts them, and
/// no gate reads a numeral in a Rust doc comment: `counts.rs` is scoped to the
/// documents by stated design, which is why this drifts and the printed line
/// does not.
///
/// So the sentence stops carrying one. What the fixtures refuse is written in
/// `xtask/src/wall.rs`'s `SHAPES`, and how many there are is what `check` prints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    /// The service's name.
    pub service: Literal,
    /// Its version.
    pub version: Literal,
}

/// The log itself: an origin, and somewhere to send lines.
///
/// **Not `Clone`.** A second handle would mean a second flush at exit, and the
/// first to run would close the shipper's channel while the other still held a
/// sender — so the second would wait on a queue nothing was draining. One log,
/// one lifetime; a caller that needs to share it wraps it in an `Arc`, which is
/// one `Drop` at the end.
#[derive(Debug)]
pub struct Log {
    origin: Origin,
    shipper: Shipper,
}

impl Log {
    /// Opens a log for `origin`, posting to `endpoint`.
    ///
    /// # Errors
    ///
    /// Returns why the endpoint cannot be spoken to.
    pub fn open(origin: Origin, endpoint: &str) -> Result<Self, String> {
        Ok(Self {
            origin,
            shipper: Shipper::start(endpoint)?,
        })
    }

    /// Records an event.
    ///
    /// Takes no message and returns nothing: there is nothing a caller could
    /// add that the registry has not already declared, and nothing a caller
    /// could do about a log store that is down (`ship`'s module docs).
    pub fn record(&self, event: &impl Event) {
        self.shipper.send(self.line(event));
    }

    /// The one JSON line an event becomes.
    ///
    /// Through `nc-canon`, so the escaping is the same code that escapes every
    /// other boundary — and so a field cannot end the line it is written on.
    #[must_use]
    pub fn line(&self, event: &impl Event) -> String {
        let mut fields: BTreeMap<String, nc_canon::Value> = BTreeMap::new();
        // **The event's fields first, and the origin over them.** They were the
        // other way round, into one map where the last write wins — so an event
        // declaring a field called `service` or `level` overwrote the line's own,
        // and a line could claim a level its event does not have and a service it
        // did not come from. The registry now refuses those four names
        // (`check_events`), and this ordering is the second half: a registry that
        // somehow held one still cannot win.
        for (name, field) in event.fields() {
            fields.insert(name.to_owned(), field.to_canonical());
        }

        // The four the log owns, written last so nothing can displace them.
        // Service and version are `§15.4`'s and come from the log rather than
        // from the event — an event that had to declare them is an event
        // somebody forgets to.
        fields.insert("_msg".to_owned(), nc_canon::Value::text(event.name()));
        fields.insert(
            "level".to_owned(),
            nc_canon::Value::text(level_name(event.level())),
        );
        fields.insert(
            "service".to_owned(),
            nc_canon::Value::text(self.origin.service.get()),
        );
        fields.insert(
            "version".to_owned(),
            nc_canon::Value::text(self.origin.version.get()),
        );

        nc_canon::Value::Object(fields).to_canonical()
    }

    /// How many lines the current batch has dropped, and why.
    ///
    /// **Reset by the shipper on every batch**, and it counts a disconnected
    /// queue as well as a full one — `Shipper::dropped` says so and this
    /// re-export did not, which is the kind of gap that makes a number mean two
    /// things to two readers.
    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.shipper.dropped()
    }

    /// How many lines reached neither the store nor stderr, since this process
    /// began.
    ///
    /// **`dropped()` was the only number a caller could read**, and it counts
    /// one thing: lines refused at the queue. A line the store did not take and
    /// stderr would not write is a different loss, and it was counted in a
    /// `static` nothing outside `ship.rs` could reach — measured with a store
    /// that never answers and stderr on a full disk, 235 of 300 lines in
    /// neither place with `dropped()` reading `0`. A number an operator cannot
    /// reach is not a number they have.
    ///
    /// **Not a monotonic total, and the line that said it was is the finding.**
    /// It read *"Cumulative and never reset, unlike [`Self::dropped`], because
    /// it is a total and a total that goes down is not one"*, and it goes down
    /// on the ordinary path: when stderr blocks, `flush_stderr` charges the
    /// backlog here so an operator can see it at once, and when stderr comes
    /// back the writer un-charges each line it actually writes. That is the
    /// right behaviour — a line that reached stderr in the end did not reach
    /// neither place — and the wrong sentence, because a monitor told to treat
    /// this as a counter reads a recovery as a rewrite of history.
    ///
    /// What it holds is *lines not accounted for right now*: charged when they
    /// are lost or stuck, released when they are written. It is `dropped`'s
    /// per-batch reset that it is unlike, not its direction.
    #[must_use]
    pub fn lost(&self) -> u64 {
        crate::ship::Shipper::lost()
    }
}

/// Nothing outside this crate implements [`Event`].
///
/// **The trait was open, and that was the largest hole in the wall.** Any crate
/// could `impl Event` and return `("who", Field::Id(user_email))` — no leak, no
/// macro, nothing a source gate looks for. Sealing it costs a private supertrait
/// and takes nothing away: every event struct is *generated into this crate*
/// from `catalog/log-events.toml`, so there is nobody outside who should be
/// writing one. An event that is not in the registry does not exist (`§15.2`).
pub(crate) mod sealed {
    /// Implemented for the generated event structs, and for nothing else.
    pub trait Sealed {}
}

/// The level's name, as the store reads it.
#[must_use]
pub const fn level_name(level: Level) -> &'static str {
    // **The registry's own word, not a second copy of it.** This was a `match`
    // over the five variants written out here — a second vocabulary for one
    // judgement, which `Severity`'s own doc names as how they drift apart. The
    // generated crate answers it now, from the same word the pages and the
    // ledger write.
    level.as_str()
}

#[cfg(test)]
mod tests {
    use super::{Event, Field, Log, Origin, level_name};
    use nc_catalog::Level;

    /// An event shaped exactly like a generated one.
    struct Probe;

    impl crate::sealed::Sealed for Probe {}

    impl Event for Probe {
        fn name(&self) -> &'static str {
            "probe.happened"
        }
        fn level(&self) -> Level {
            Level::Info
        }
        fn fields(&self) -> Vec<(&'static str, Field)> {
            vec![
                ("count", Field::Unsigned(3)),
                ("table", Field::Static("access_records")),
            ]
        }
    }

    /// An event whose values hold everything that could end a line.
    ///
    /// `Probe`'s two values are a number and an ordinary name, so the rule about
    /// line endings was asked of text that has none. This one carries a newline,
    /// a carriage return, a quote and a backslash.
    #[expect(
        non_camel_case_types,
        reason = "the name is a sentence about what the fixture is for"
    )]
    struct Ends_A_Line;

    impl crate::sealed::Sealed for Ends_A_Line {}

    impl Event for Ends_A_Line {
        fn name(&self) -> &'static str {
            "probe.ends_a_line"
        }
        fn level(&self) -> Level {
            Level::Info
        }
        fn fields(&self) -> Vec<(&'static str, Field)> {
            vec![("table", Field::Static("a\nb\rc\"d\\e"))]
        }
    }

    /// The number `lost()` reads is the number the shipper holds.
    ///
    /// **The repair was made one level down.** `ship.rs`'s
    /// `the_accessor_answers_the_counter` closes `Shipper::lost()`; this is a
    /// different accessor in a different file, and its only driver is the one
    /// that test's own doc rejects — `after > before` in the child whose stderr
    /// is full, where the counter starts at zero, so any monotone transform
    /// survives. Measured: `Shipper::lost()` wrapped in a `saturating_sub(1)`
    /// here left all 1305 tests green.
    ///
    /// Its sibling `Log::dropped()` has exactly this test; `lost()` had nothing
    /// of the kind, and it is the number an operator reads when the store is
    /// down *and* stderr is refusing — lines that reached neither.
    #[test]
    fn the_number_lost_reads_is_the_shippers() {
        let log = log();
        assert_eq!(
            log.lost(),
            crate::ship::Shipper::lost(),
            "the accessor and the counter disagree"
        );
        let before = log.lost();
        crate::ship::Shipper::charge_lost_for_tests(5);
        assert_eq!(
            log.lost(),
            before + 5,
            "five lines were charged and the accessor moved by {}",
            log.lost() - before
        );
        crate::ship::Shipper::discharge_lost_for_tests(5);
    }

    /// The number a caller reads is the number the shipper holds.
    ///
    /// **`Log::dropped()` had no caller anywhere in the workspace.** Measured on
    /// 2026-09-08, `self.shipper.dropped()` replaced by
    /// `self.shipper.dropped().saturating_sub(1)`:
    /// `cargo run --quiet -p xtask -- check --offline` answered `check passed`
    /// at `ok 1223 test(s)` and `ok 35 target(s) linted`. An under-reported drop
    /// count is an outage an operator does not see, and it is the one number
    /// they have for a full queue. Its sibling `Log::lost()` is driven through
    /// the public accessor by `ship.rs`; this was driven by nothing — a first
    /// plant of `self.lost()` was caught only by `-D dead-code` noticing that
    /// `Shipper::dropped` had lost its last caller, which is the compiler
    /// noticing an unused function rather than a test noticing a wrong number.
    ///
    /// **A store that stalls rather than one that is absent.** Against a port
    /// nothing listens on, every exchange is refused at once and the drain
    /// thread keeps up with the sender: three times the queue depth was handed
    /// over and *nothing* was refused, so the equality would have been about
    /// zero and `saturating_sub(1)` invisible. What fills a bounded queue is a
    /// drain that is blocked, so this holds the answer back and reads the count
    /// while the thread is still inside its first exchange — which is also the
    /// only moment the number is stable, because the shipper resets it after
    /// every batch.
    ///
    /// `mem::forget`, because the shutdown this test does not exercise costs
    /// `FLUSH + GRACE` of waiting against a store that will not answer, and this
    /// test is about one accessor. The thread and its socket go when the process
    /// does.
    #[test]
    fn the_number_of_drops_a_caller_reads_is_the_number_the_shipper_holds() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("bound").port();
        std::thread::spawn(move || {
            // Accepted and never answered: the exchange the sender is waiting
            // behind lasts as long as this test does.
            let held: Vec<std::net::TcpStream> = listener.incoming().flatten().collect();
            drop(held);
        });
        let log = Log::open(
            Origin {
                service: crate::literal!("test"),
                version: crate::literal!("0.0.0-fixture"),
            },
            &format!("http://127.0.0.1:{port}/insert"),
        )
        .expect("a valid endpoint");
        for _ in 0..(crate::ship::QUEUE * 3) {
            log.record(&Probe);
        }
        let read = log.dropped();
        assert!(
            read > 0,
            "a queue of {} was handed three times its depth and refused nothing, so the \
             equality below is about zero",
            crate::ship::QUEUE
        );
        assert_eq!(
            read,
            log.shipper.dropped(),
            "the number a caller reads is not the number the shipper holds, so an \
             operator reading `dropped()` is reading something else"
        );
        core::mem::forget(log);
    }

    fn log() -> Log {
        Log::open(
            Origin {
                service: crate::literal!("test"),
                // **Deliberately not this tree's version.** The release gate walks every
                // file for the version it is at, so a fixture that names the real one
                // reads as a file the release would leave behind — the same trap
                // `AGENTS.md`'s spelling example fell into. A fixture's version is
                // arbitrary; this one says so.
                version: crate::literal!("9.9.9"),
            },
            "http://127.0.0.1:1/insert",
        )
        .expect("a valid endpoint")
    }

    #[test]
    fn a_line_carries_the_event_its_level_and_where_it_came_from() {
        // Service and version are §15.4's, and they come from the log rather
        // than from the event — an event that had to declare them is an event
        // somebody forgets to.
        let line = log().line(&Probe);
        assert!(line.contains("\"_msg\":\"probe.happened\""), "{line}");
        assert!(line.contains("\"level\":\"info\""), "{line}");
        assert!(line.contains("\"service\":\"test\""), "{line}");
        assert!(line.contains("\"version\":\"9.9.9\""), "{line}");
        // **A count goes out as text**, and the quotes are the point. JSON has no
        // `u64`, and the alternative was `i64::try_from(value).unwrap_or(-1)` —
        // which turns every value at or above 2^63 into the same number, so a
        // byte count past that boundary would read in the store as a value
        // nothing ever had.
        assert!(line.contains("\"count\":\"3\""), "{line}");
        assert!(line.contains("\"table\":\"access_records\""), "{line}");
    }

    /// A value that could end the line does not end it.
    ///
    /// **This was an assertion filtered by the predicate it then tests.** It
    /// asked `Probe` — whose values are `3` and `access_records` — whether the
    /// rendered line held a control character, and its comment said `nc-canon`
    /// "escapes every control character". It does not, and the workspace pins
    /// that it must not: canon escapes the C0 range, `"` and `\`, and
    /// `nothing_a_specification_writes_literally_is_escaped` walks U+0020
    /// upwards asserting every one of those is written literally. So the
    /// assertion held for every value canon can emit, and what it was written to
    /// catch — a `Log::line` that stops going through canon — was invisible,
    /// because no value it was given contains anything canon escapes.
    ///
    /// Measured: `to_canonical()` followed by `.replace("\\n", "\n")` left the
    /// whole gate green with `Log::line` emitting raw newlines.
    #[test]
    fn a_line_is_one_line() {
        // The property the whole file format rests on: the store reads one JSON
        // object per line, so a field that ends its own line ends the record.
        let line = log().line(&Ends_A_Line);
        assert_eq!(line.lines().count(), 1, "{line}");
        assert!(
            !line.chars().any(|character| character < ' '),
            "a control character reached the line raw: {line}"
        );
        // And the escapes are in it, which is what says the value arrived at all
        // rather than being dropped on the way.
        assert!(line.contains("\\n"), "the newline was not escaped: {line}");
        assert!(line.contains("\\\""), "the quote was not escaped: {line}");
        assert!(
            line.contains("\\\\"),
            "the backslash was not escaped: {line}"
        );
    }

    #[test]
    fn every_level_has_a_name_the_store_reads() {
        // **Each name, and five distinct ones.** This asserted only that a name
        // was non-empty — so collapsing every level onto `"info"` left the suite
        // green, and a central log reporting every `error` line as `info` was
        // invisible to it. `§19.5`: a title is not an assertion.
        let named = [
            (Level::Trace, "trace"),
            (Level::Debug, "debug"),
            (Level::Info, "info"),
            (Level::Warn, "warn"),
            (Level::Error, "error"),
        ];
        for (level, name) in named {
            assert_eq!(level_name(level), name, "{level:?}");
        }
        let distinct: std::collections::BTreeSet<&str> =
            named.iter().map(|(level, _)| level_name(*level)).collect();
        assert_eq!(distinct.len(), named.len(), "two levels share a name");
    }

    #[test]
    fn a_field_an_event_declares_cannot_displace_one_the_log_owns() {
        // The ordering half. The registry refuses a *generated* event declaring
        // one of the four, and this is what holds when something else does — and
        // moving the origin back above the event's fields left the suite green,
        // because the only test event declares none of them.
        use crate::LogValue as _;

        struct Impostor;
        impl crate::sealed::Sealed for Impostor {}
        impl Event for Impostor {
            fn name(&self) -> &'static str {
                "impostor.happened"
            }
            fn level(&self) -> Level {
                Level::Error
            }
            fn fields(&self) -> Vec<(&'static str, Field)> {
                vec![
                    ("_msg", crate::literal!("something else").to_field()),
                    ("level", crate::literal!("info").to_field()),
                    ("service", crate::literal!("somebody-else").to_field()),
                    // **Not the origin's own version.** The impostor declared
                    // `9.9.9` and so does the `Log` this runs against, so the
                    // assertion below was true whichever of the two wrote it —
                    // measured on 2026-09-12, the `version` insert moved above
                    // the event-field loop left `cargo test -p nc-log` at
                    // `59 passed`, while the identical edit one field along was
                    // caught at once.
                    ("version", crate::literal!("0.0.0-impostor").to_field()),
                ]
            }
        }

        let line = log().line(&Impostor);
        assert!(line.contains("\"_msg\":\"impostor.happened\""), "{line}");
        assert!(line.contains("\"level\":\"error\""), "{line}");
        assert!(line.contains("\"service\":\"test\""), "{line}");
        assert!(line.contains("\"version\":\"9.9.9\""), "{line}");
        assert!(!line.contains("somebody-else"), "{line}");
        assert!(!line.contains("0.0.0-impostor"), "{line}");
    }
}
