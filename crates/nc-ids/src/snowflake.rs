//! Snowflake identifiers, and the two hazards that make them hard.
//!
//! The layout is 41 bits of millisecond, 10 bits of machine flake and 12 bits of
//! sequence — 63 bits, so the sign bit of a `u64` is always clear and the value
//! survives a trip through anything that only has signed integers.
//!
//! **The two hazards are handled explicitly, because the failure mode of each is
//! a duplicate identifier that nothing detects** (`NOCTUA.md §16.2`):
//!
//! **What this is, and is not, safe against.** The rules below hold for the life
//! of one [`Minter`] in one process. Two minters constructed with the same flake
//! produce the same identifiers, and a process that restarts across a backwards
//! clock step starts again from `last_millis = 0` — so a caller that needs the
//! guarantee across restarts hands the highest timestamp it has seen to
//! [`Minter::resuming_at`]. Saying so is the point: `§16.2`'s rule is
//! "refuse to mint rather than mint a duplicate", and a module that implied more
//! than it enforces would be worse than one that says where the line is.
//!
//! **And nothing in this repository persists that mark, which is a decision and
//! not an omission.** Both halves are here — [`Minter::high_water_mark`] reads
//! the highest millisecond a minter reached, [`Minter::resuming_at`] refuses to
//! go below one — and there is no caller, because nothing in this stage mints an
//! identifier in production. A store chosen here would be a second place the
//! answer lives, in the crate least able to know where the first one is. `§16.2`
//! now carries the rule instead, and it names the ordering that matters: the
//! mark is written *before* the identifier is handed out, since a mark written
//! after is one a crash skips — which is the same duplicate this module refuses
//! to mint while it is running.
//!
//! **Nothing here logs**, and `§16.2` asks for it. The events are declared —
//! `ids.clock_backwards`, `ids.sequence_exhausted`, `ids.flake_collision` in
//! `catalog/log-events.toml` — and emitting them needs the log service, which is
//! a later deliverable. Each error carries the catalog code that line will be
//! queried by, so the wiring is a call site rather than a redesign.
//!
//! * **The clock moving backwards.** Minting refuses. It does not
//!   "borrow" from the sequence and it does not wait indefinitely: a machine
//!   whose clock jumped an hour back would block for an hour, and a machine that
//!   minted anyway would hand out identifiers it had already handed out.
//! * **The sequence exhausting inside one millisecond.** Minting waits for the
//!   next millisecond. 4,096 identifiers per millisecond is four million a
//!   second; a process that reaches it is doing something a wait will not hurt.

use core::time::Duration;

use nc_catalog::ErrorCode;

use crate::base32;
use crate::flake::MachineFlake;

/// Milliseconds from the Unix epoch to this system's own epoch,
/// 2026-01-01T00:00:00Z.
///
/// **Never change this.** Every identifier ever minted is an offset from it, so
/// moving it renumbers history. It is late rather than at the Unix epoch because
/// 41 bits of milliseconds is about 69 years and there is no reason to spend the
/// first 56 of them on time that passed before the platform existed: from here,
/// the range runs to 2095.
pub const EPOCH_MS: u64 = 1_767_225_600_000;

const SEQUENCE_BITS: u32 = 12;
const FLAKE_BITS: u32 = 10;
const TIMESTAMP_BITS: u32 = 41;

const SEQUENCE_MASK: u64 = (1 << SEQUENCE_BITS) - 1;
const SEQUENCE_MASK_U16: u16 = (1 << SEQUENCE_BITS) - 1;
const FLAKE_MASK: u64 = (1 << FLAKE_BITS) - 1;
const TIMESTAMP_MASK: u64 = (1 << TIMESTAMP_BITS) - 1;

/// One identifier.
///
/// Sortable by value, and therefore by creation time across every machine in a
/// realm — which is the property the whole layout exists for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Snowflake(u64);

impl Snowflake {
    /// The raw value.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }

    /// Milliseconds since [`EPOCH_MS`].
    #[must_use]
    pub const fn millis_since_epoch(self) -> u64 {
        (self.0 >> (FLAKE_BITS + SEQUENCE_BITS)) & TIMESTAMP_MASK
    }

    /// Which machine minted it.
    #[must_use]
    pub fn flake(self) -> u16 {
        // Masked to ten bits, which is inside `u16` — so the conversion cannot
        // fail, and `unwrap_or(0)` is unreachable rather than a fallback.
        u16::try_from((self.0 >> SEQUENCE_BITS) & FLAKE_MASK).unwrap_or(0)
    }

    /// Its position within its millisecond.
    #[must_use]
    pub fn sequence(self) -> u16 {
        // Masked to twelve bits, which is inside `u16`.
        u16::try_from(self.0 & SEQUENCE_MASK).unwrap_or(0)
    }

    /// Crockford base32, uppercase — how it appears in a file name (`§16.3`).
    #[must_use]
    pub fn to_crockford(self) -> String {
        base32::encode(self.0)
    }

    /// Reads one back from a file name.
    ///
    /// # Errors
    ///
    /// Returns what was wrong with the text, or `Base32Error::Overflows` when it
    /// decodes to a value with bits outside the 63 this layout uses — which is
    /// the same complaint as too many characters, arrived at by a different
    /// route. It used to report the text's *first* character as "the offending
    /// character", which is normally a perfectly valid digit.
    pub fn from_crockford(text: &str) -> Result<Self, base32::Base32Error> {
        let value = base32::decode(text)?;
        if value >> (TIMESTAMP_BITS + FLAKE_BITS + SEQUENCE_BITS) != 0 {
            return Err(base32::Base32Error::Overflows {
                length: text.chars().count(),
                // 63, not 64: a `Snowflake` is 41 bits of milliseconds, 10 of
                // machine flake and 12 of sequence. The sentence used to say 64
                // for both, so a value this refuses was reported as needing more
                // than 64 bits while `base32::decode` had just returned it.
                bits: TIMESTAMP_BITS + FLAKE_BITS + SEQUENCE_BITS,
            });
        }
        Ok(Self(value))
    }

    /// Assembles one from its parts. Used by the minter and by tests.
    #[must_use]
    const fn assemble(millis: u64, flake: u16, sequence: u16) -> Self {
        Self(
            ((millis & TIMESTAMP_MASK) << (FLAKE_BITS + SEQUENCE_BITS))
                | ((flake as u64 & FLAKE_MASK) << SEQUENCE_BITS)
                | (sequence as u64 & SEQUENCE_MASK),
        )
    }
}

impl core::fmt::Display for Snowflake {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(&self.to_crockford())
    }
}

/// Why minting refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MintError {
    /// The clock moved backwards, so minting would risk a duplicate.
    ClockMovedBackwards {
        /// How far back it went.
        drift: Duration,
    },
    /// The clock is past the range 41 bits can hold — 2095, from [`EPOCH_MS`].
    BeyondRange,
    /// The clock is before this system's epoch, which is 2026-01-01.
    ///
    /// A separate variant, because it used to be reported as `BeyondRange` whose
    /// message says "past this layout's range" — the opposite direction, and
    /// exactly the `§15.1` failure of two causes under one identity.
    BeforeEpoch,
    /// The clock did not advance after waiting for it, so minting would repeat
    /// an identifier it has already handed out.
    ClockStuck {
        /// Where it stopped.
        at: Duration,
    },
    /// A persisted high-water mark is further ahead of the clock than any real
    /// skew, so it is corrupt rather than early.
    ///
    /// **Not `ClockMovedBackwards`, which says retrying works.** A mark inside
    /// the 41-bit layout but at the top of it came back as a drift of sixty-nine
    /// years carrying a retryable code — advice a caller cannot act on. A mark is
    /// read from a store, and a store can hand back a wrong number; this crate's
    /// own doc counts 22 of 64 single-bit flips landing in that window.
    MarkOutOfRange {
        /// The mark that was handed in.
        millis: u64,
    },
}

/// The most a clock may plausibly be behind a mark, in milliseconds.
///
/// A day. Real skew is seconds — NTP steps, a virtual machine resuming, a
/// battery-backed clock read before the network is up — and a mark a whole day
/// ahead of the clock is a number that came back wrong, not a machine that was
/// briefly early. The bound is generous on purpose: what it has to separate is
/// "wait a moment" from "sixty-nine years".
const PLAUSIBLE_DRIFT_MS: u64 = 24 * 60 * 60 * 1_000;

/// How many times minting will wait for a millisecond that does not arrive.
const WAIT_ATTEMPTS: u32 = 1_000;

impl MintError {
    /// The catalog code this maps to, which is what a log is queried by.
    #[must_use]
    pub const fn code(self) -> ErrorCode {
        match self {
            // **One code per outcome a caller can act on.** All three shared
            // `IdsClockBackwards`, whose sentence — "until the time it skipped
            // has passed" — is about a step and is false of a clock set to the
            // wrong century, and whose `retryable` could only be right for one
            // of them. Reserving a second code was argued against on the
            // grounds that nothing would emit it for seventy years; a clock in
            // 1999 emits it today, and a machine with no battery is how.
            Self::ClockMovedBackwards { .. } => ErrorCode::IdsClockBackwards,
            // **Their own code, because waiting fixes one of these and not the
            // other.** A clock that stepped back a few hundred milliseconds
            // clears by itself — measured, the identical call answered `Ok`
            // 601 ms later — and a clock set to 1999 or 2200 does not: 500
            // retries each, the identical refusal every time. Under one code
            // either the transient half was told to give up or the permanent
            // half spun for ever, and both locales' sentence — "until the time
            // it skipped has passed" — is about a step and false of these.
            //
            // **And a corrupt mark joins them**, for the same reason: retrying
            // fixes none of the three, and each needs a person. What separates a
            // corrupt mark from `ClockMovedBackwards` is exactly that — measured,
            // a mark at the top of the layout reported a drift of sixty-nine
            // years under a code the suite certifies as retryable, which is
            // advice no caller can act on.
            Self::BeyondRange | Self::BeforeEpoch => ErrorCode::IdsClockOutOfRange,
            // **Not the clock's code, because the clock is fine here.**
            // `resuming_at` reaches this only after `within_range(now)?` has
            // established that the clock is inside the layout, so
            // NC-IDS-0008's sentence — *"This computer's clock is set to the
            // wrong year … Fix the date and time, then try again"* — is false
            // of it by construction, and its advice is the one action that
            // makes the corruption permanent: moving the clock forward past a
            // corrupt mark is what makes `resuming_at` accept the number it has
            // just diagnosed as wrong.
            //
            // A stored mark beyond what the layout allows **is** a value
            // outside a declared limit, which is what `NC-VAL-0001` says, and
            // reporting it that way adds no code and renumbers none —
            // `flake::FlakeOutOfRange` is the same shape, `bounds()` and all.
            // `catalog/errors.toml` states the rule this settles at
            // NC-IDS-0006: a code is only worth having if its description is
            // true of every failure that carries it. Decided in D5, on the
            // developer's instruction to decide it.
            Self::MarkOutOfRange { .. } => ErrorCode::ValidationOutOfRange,
            // **A clock that stopped is not a wait.** This answered
            // `IdsSequenceExhausted` — severity `info`, description *"this
            // millisecond's sequence was exhausted and minting waited"*, and a
            // code whose own registry comment says a caller never receives it.
            // A caller does receive this one, and what it means is that minting
            // has stopped on this machine until somebody fixes its time:
            // measured, 4096 identifiers and then a refusal that never clears.
            Self::ClockStuck { .. } => ErrorCode::IdsClockStuck,
        }
    }
}

impl core::fmt::Display for MintError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ClockMovedBackwards { drift } => write!(
                formatter,
                "the clock moved {} ms backwards, so minting would risk a duplicate",
                drift.as_millis()
            ),
            Self::BeyondRange => formatter.write_str("the clock is past this layout's range"),
            Self::BeforeEpoch => {
                formatter.write_str("the clock is before this system's epoch of 2026-01-01")
            }
            Self::ClockStuck { at } => write!(
                formatter,
                "the clock did not move past {} ms, so minting would repeat an identifier",
                at.as_millis()
            ),
            Self::MarkOutOfRange { millis } => write!(
                formatter,
                "a stored mark of {millis} ms is further ahead of this clock than any \
                 real skew, so it is a wrong number rather than an early one"
            ),
        }
    }
}

impl core::error::Error for MintError {}

/// Where the minter reads time, so both hazards can be tested without sleeping.
///
/// A trait and not a closure because the minter needs two operations that must
/// agree with each other: reading the clock, and waiting for it to move. A fake
/// that advanced on read but not on wait would make the exhaustion test pass
/// against a minter that never waits.
pub trait Clock {
    /// Milliseconds since the Unix epoch.
    fn now_ms(&self) -> u64;

    /// Waits for [`Clock::now_ms`] to pass `millis`, and **returns either way**.
    ///
    /// **It may not block forever, and that is the contract rather than
    /// advice.** [`Minter::mint`] guards the wait with a bounded loop so a clock
    /// that does not move produces [`MintError::ClockStuck`] rather than a
    /// duplicate — and a `wait_past` that never returns makes that guard
    /// unreachable, because the loop never gets its turn. The shipped
    /// [`SystemClock`] spun on `while now_ms() <= millis` and did exactly that:
    /// every test of the guard used a fake that returns immediately, so on a
    /// frozen or suspended host with an exhausted sequence the journal would
    /// have spun a core for ever instead of returning an error somebody can act
    /// on. An implementation that cannot make progress returns and lets the
    /// minter decide.
    fn wait_past(&self, millis: u64);
}

/// **A clock behind a reference or a box is a clock.** Without these a kernel
/// that owns one clock and wants two minters cannot have them, and one that
/// chooses a clock at run time cannot either — measured:
///
/// ```text
/// error[E0277]: the trait bound `&Shared: Clock` is not satisfied
/// error[E0277]: the trait bound `Box<dyn Clock>: Clock` is not satisfied
/// ```
///
/// The forwarders existed, under `#[cfg(test)]`, for this crate's own fakes.
impl<C: Clock + ?Sized> Clock for &C {
    fn now_ms(&self) -> u64 {
        (**self).now_ms()
    }

    fn wait_past(&self, millis: u64) {
        (**self).wait_past(millis);
    }
}

impl<C: Clock + ?Sized> Clock for Box<C> {
    fn now_ms(&self) -> u64 {
        (**self).now_ms()
    }

    fn wait_past(&self, millis: u64) {
        (**self).wait_past(millis);
    }
}

/// How many yields one [`SystemClock::wait_past`] spends before giving up.
///
/// A yield is under a microsecond on every platform this runs on, so this is a
/// small fraction of the millisecond being waited for on a healthy machine — and
/// on a stopped one it is the difference between an error and a spinning core.
/// `mint` then tries the wait `WAIT_ATTEMPTS` times before refusing, so a clock
/// merely slow to tick still gets its millisecond.
const YIELDS_PER_WAIT: u32 = 10_000;

/// The system clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| {
                u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
            })
    }

    fn wait_past(&self, millis: u64) {
        // A spin with a yield rather than a sleep: the wait is for the remainder
        // of the current millisecond, and a sleep's granularity is coarser than
        // what is being waited for on every platform this runs on.
        //
        // **Bounded, because the caller's guard depends on this returning.**
        // `while self.now_ms() <= millis {}` was unbounded, so `mint`'s
        // `WAIT_ATTEMPTS` loop and its `ClockStuck` error were reachable only
        // through a test fake — on a frozen host this spun a core for ever and
        // the caller never got the error. The bound is generous next to one
        // millisecond and small next to for ever: a clock that has not moved
        // after this many yields is not about to.
        self.wait_past_with(millis, std::thread::yield_now);
    }
}

impl SystemClock {
    /// The wait [`Clock::wait_past`] runs, yielding with `yield_now`: at most
    /// [`YIELDS_PER_WAIT`] times, returning as soon as the clock is past
    /// `millis`.
    ///
    /// **The seam the bound is counted through**, so a test holds the shipped
    /// loop to its yields rather than to how long they took on a loaded core.
    fn wait_past_with(self, millis: u64, mut yield_now: impl FnMut()) {
        for _ in 0..YIELDS_PER_WAIT {
            if self.now_ms() > millis {
                return;
            }
            yield_now();
        }
    }
}

/// Mints identifiers for one machine.
///
/// **One minter is one sequence**, and two threads minting from one without a
/// lock would hand out the same sequence number twice. [`Minter::mint`] takes
/// `&mut self`, so the borrow checker enforces that — which is why `Minter` *is*
/// `Send + Sync` and this paragraph used to claim it was not. The reasoning was
/// right and the stated fact was wrong: what stops two threads is the exclusive
/// borrow, not a missing trait. A caller that needs to mint from several threads
/// wraps it in a mutex, which is cheap next to a lock-free scheme nobody can
/// prove.
#[derive(Debug)]
pub struct Minter<C: Clock> {
    flake: MachineFlake,
    clock: C,
    last_millis: u64,
    sequence: u16,
    /// The millisecond a bounded wait already gave up on, and **what it
    /// concluded**, if one did.
    ///
    /// **The conclusion, kept.** Without it every later `mint` walked the whole
    /// `WAIT_ATTEMPTS` × `YIELDS_PER_WAIT` loop again — measured on a stopped
    /// host at 5.36 s per call, for ever, burning a core to reach the same
    /// answer. It clears itself: the clock moving past this value is what makes
    /// the check below miss.
    ///
    /// **The drift with it, because one memo carried two conclusions.** Both
    /// exits from the wait wrote this field and the early return that reads it
    /// raised `ClockStuck` unconditionally — so a clock that dipped and came
    /// back was diagnosed `ClockMovedBackwards` on the first call and
    /// `ClockStuck` on the next. Measured with a clock scripted `+20, +20, +19,
    /// +20`:
    ///
    /// ```text
    /// first  = ClockMovedBackwards { drift: 1ms }   after 1002 read(s)  NC-IDS-0001
    /// second = ClockStuck { at: 1767225600.02s }    after 1 read(s)     NC-IDS-0007
    /// ```
    ///
    /// `NC-IDS-0001` is `error`; `NC-IDS-0007` is `critical` and says *"the
    /// clock has stopped, so no more identifiers can be minted on this
    /// machine"*. This file already spends three paragraphs on that exact
    /// misdiagnosis in the other direction. A memo of a verdict carries the
    /// verdict.
    concluded: Option<(u64, Verdict)>,
}

/// What a bounded wait concluded about the clock.
///
/// Two machine faults, and a responder acts on them differently: a stopped clock
/// needs the host looked at, and one that moved backwards has already moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// It did not move past the barrier at all.
    Stopped,
    /// It went backwards by this much and the wait gave up.
    MovedBackwards {
        /// How far back, in milliseconds.
        drift: u64,
    },
}

impl<C: Clock> Minter<C> {
    /// A minter for `flake`, reading `clock`.
    ///
    /// **One per flake per process.** Two minters sharing a flake keep two
    /// sequences and hand out the same identifiers; nothing here can detect
    /// that, so it is the caller's to enforce.
    pub const fn new(flake: MachineFlake, clock: C) -> Self {
        Self {
            flake,
            clock,
            last_millis: 0,
            sequence: 0,
            concluded: None,
        }
    }

    /// A minter that will not mint at or before `millis`.
    ///
    /// The high-water mark a process persists, so a restart across a backwards
    /// clock step refuses rather than repeating what it already handed out. A
    /// fresh [`Minter::new`] starts at zero and cannot know.
    ///
    /// The sequence starts exhausted, so the first mint in the same millisecond
    /// as the mark waits for the next rather than reusing a number the previous
    /// process may already have handed out.
    ///
    /// **The mark is checked, and it was not.** It is a `u64` off a disk, and
    /// every value was accepted:
    ///
    /// ```text
    /// mark = 0, 1, EPOCH_MS - 1   -> the protection is silently off
    ///     process A minted : [2097180672, 2097180673, 2097180674]
    ///     process B minted : [2097180672, 2097180673, 2097180674]   literal duplicates
    /// mark = u64::MAX             -> ClockMovedBackwards { drift: 18446742306483951s }
    ///     1000 retries, all identical; a retry can first succeed in ~584942361 years
    /// ```
    ///
    /// A mark outside what this layout can hold, or ahead of the clock, is not a
    /// mark this process wrote, so it is refused here rather than turned into an
    /// outage or a duplicate. What cannot be checked is a mark that is merely
    /// *low* and still behind the clock — that is the persisting caller's to get
    /// right, and this method does not pretend otherwise.
    ///
    /// # Errors
    ///
    /// Returns [`MintError::BeforeEpoch`] or [`MintError::BeyondRange`] when
    /// `millis` is not a time this layout can represent, and
    /// [`MintError::ClockMovedBackwards`] when it is ahead of the clock.
    pub fn resuming_at(flake: MachineFlake, clock: C, millis: u64) -> Result<Self, MintError> {
        Self::within_range(millis)?;
        // **And against the clock, which this held and never asked.** The layout
        // check accepts any value 41 bits can hold — 69 years of them — so a
        // mark corrupted in either direction was taken as a time this process
        // wrote. Measured over the 64 single-bit flips of a mark persisted a
        // moment ago: **16 are accepted as a past mark**, rolling the barrier
        // back by up to 12.4 days and turning the protection off, and **22 are
        // accepted as a future mark**, refusing all minting for up to 8.7 years
        // behind `ClockMovedBackwards`. A mark from the future is not a mark
        // this process wrote; a mark from the past cannot be told from an honest
        // one, and that half stays the persisting caller's to get right.
        // **And the clock itself is in range**, which this did not ask. A host
        // in 2001 answered `ClockMovedBackwards` — `NC-IDS-0001`,
        // `retryable = true` — here and `BeforeEpoch` — `NC-IDS-0008`,
        // `retryable = false` — from `mint`, one fault with two codes and
        // opposite advice depending on which door a caller came through.
        let now = clock.now_ms();
        Self::within_range(now)?;
        if millis > now {
            // **A mark inside the layout but far ahead of the clock is not a
            // clock that moved backwards.** The guard above only refuses a mark
            // outside the 41 bits, and the tests below reach `0`, `1`,
            // `EPOCH_MS - 1`, `u64::MAX`, `EPOCH_MS + 100` and nothing between —
            // so a mark at the top of the layout came back as
            // `ClockMovedBackwards { drift: 2199023255.451s }`, sixty-nine years,
            // carrying `NC-IDS-0007`, which
            // `the_clock_codes_say_retry_where_retrying_works` certifies as
            // *retryable*. A caller told to wait and retry would wait sixty-nine
            // years. This module's own doc says 22 of 64 single-bit flips of a
            // stored mark land in this window.
            //
            // A drift no plausible clock skew explains is a corrupt mark, and a
            // corrupt mark is not something retrying fixes.
            if millis - now > PLAUSIBLE_DRIFT_MS {
                return Err(MintError::MarkOutOfRange { millis });
            }
            return Err(MintError::ClockMovedBackwards {
                drift: Duration::from_millis(millis - now),
            });
        }
        Ok(Self {
            flake,
            clock,
            last_millis: millis,
            // The sequence starts exhausted; the mask is twelve bits, which is
            // inside `u16`.
            sequence: SEQUENCE_MASK_U16,
            concluded: None,
        })
    }

    /// The last millisecond this minter used, for persisting as a high-water
    /// mark — or `None` when it has no mark yet.
    ///
    /// **`Option`, because zero is not a mark.** A fresh [`Minter::new`] starts
    /// at zero and this returned it, so a caller persisting on a timer wrote a
    /// value that [`Minter::resuming_at`] then refuses — reported to a person,
    /// in both locales, as a clock set to the wrong year, on a machine whose
    /// clock is fine. Nothing to persist is a state a caller can act on; zero
    /// is a state it has to know about this type to interpret.
    ///
    /// **A resumed minter has a mark before it has minted**, and the sentence
    /// above used to say `None` "when it has not minted anything" — false on that
    /// path, and it is the sentence a caller reads before deciding what to
    /// persist. `resuming_at` is handed a mark and keeps it, which is the point
    /// of resuming; what `None` means is that there is nothing to write down yet,
    /// whether because nothing was minted or because nothing was resumed from.
    #[must_use]
    pub const fn high_water_mark(&self) -> Option<u64> {
        if self.last_millis == 0 {
            None
        } else {
            Some(self.last_millis)
        }
    }

    /// The next identifier.
    ///
    /// # Errors
    ///
    /// Four, not the two this named: [`MintError::ClockMovedBackwards`] and
    /// [`MintError::BeyondRange`] as below, plus [`MintError::BeforeEpoch`] when
    /// the machine's clock reads before 1970, and [`MintError::ClockStuck`] when
    /// the sequence for this millisecond is exhausted and the clock has not
    /// moved. A caller matching on the documented set writes an
    /// unreachable-looking `_` arm over two real operational failures — clippy's
    /// `missing_errors_doc` only asks that the section exist.
    ///
    /// Refuses when the clock has moved backwards since the last mint, or when
    /// it is past what 41 bits of milliseconds can hold.
    pub fn mint(&mut self) -> Result<Snowflake, MintError> {
        let mut now = self.clock.now_ms();

        if now < self.last_millis {
            return Err(MintError::ClockMovedBackwards {
                drift: Duration::from_millis(self.last_millis - now),
            });
        }

        // **Range first, before anything is remembered.** It used to be checked
        // after `last_millis` was written, so one glitched reading far in the
        // future latched the minter to it — and every later call returned
        // `ClockMovedBackwards` with a drift of nine million seconds, forever,
        // under an error identity that sends a responder to the wrong problem
        // (`NOCTUA.md §15.1`).
        Self::within_range(now)?;

        if now == self.last_millis {
            // **Tested for exhaustion before the increment, never after.** The
            // increment used to come first, and every path out of the wait below
            // is an early return — so a minter that failed once left `sequence`
            // sitting above the mask and raised it again on the next call. After
            // 65 536 of them the `u16` wrapped to zero and the minter re-issued
            // an identifier it had already handed out, in a release build, with
            // no error and no way to notice. Nothing here may leave `sequence`
            // above the mask.
            if self.sequence >= SEQUENCE_MASK_U16 {
                // Exhausted. Wait rather than borrow from the next millisecond:
                // borrowing produces an identifier whose timestamp is a lie, and
                // the whole point of the layout is that the timestamp is not.
                //
                // And **check that the wait happened.** The `Clock` doc names
                // this hazard — a clock that advances on read but not on wait —
                // and an earlier version trusted the call, so a stuck clock
                // minted a literal duplicate. A bounded loop refuses instead.
                // **And once is enough to know.** The conclusion was not kept,
                // so every later `mint` walked the whole bounded loop again:
                // measured on a stopped host, 5.36 s per call, for ever — a
                // caller writing a hundred journal entries waits nine minutes to
                // be told the same thing a hundred times, burning a core. The
                // answer does not change until the clock does, and the clock
                // moving is exactly what clears this.
                //
                // **The memo alone, because the guard beside it decided
                // nothing.** A `now <= self.last_millis` conjunct stood here,
                // written for a clock that resumed inside the stalled
                // millisecond; `now == self.last_millis` holds on every path
                // that reaches this line, so the conjunct was inert — a reviewer
                // replaced it with `true` and all 79 tests passed — and the case
                // it was written for takes the `else` branch above and never
                // arrives. Two comments stood stacked here afterwards, the first
                // still explaining the removed guard as though it were the fix.
                // A conjunct nothing can falsify is not a defence, and a comment
                // for code that is gone is worse than none.
                // **The conclusion that was reached, not a fixed one.** This
                // raised `ClockStuck` for whichever exit wrote the memo, so a
                // clock that dipped and returned came back as `critical` on its
                // second call — see `concluded`.
                if let Some((at, verdict)) = self.concluded
                    && at == now
                {
                    return Err(match verdict {
                        Verdict::Stopped => MintError::ClockStuck {
                            at: Duration::from_millis(self.last_millis),
                        },
                        Verdict::MovedBackwards { drift } => MintError::ClockMovedBackwards {
                            drift: Duration::from_millis(drift),
                        },
                    });
                }
                // **What the clock read when the wait began**, so a clock that
                // *moves* during it is not called stopped. The loop's exit
                // condition is `now <= last_millis`, which a backwards step
                // also satisfies — measured, a clock going from …605000 to
                // …604000 during the wait came out as
                // `ClockStuck { at: 1767225605s }`, `critical`, "the clock has
                // stopped", for a clock that had moved a full second. The two
                // are different machine faults and a responder acts on them
                // differently.
                let began_at = now;
                // **The lowest reading seen, not the last one.** The direction
                // test looked only at the sample taken after the thousandth
                // wait, so a clock that sat a full second in the past for 999
                // samples and returned on the last one came out as
                // `ClockStuck` — `NC-IDS-0007`, **critical**, *"the clock has
                // stopped"* — for a clock that had moved a second. A machine
                // fault reported as a different machine fault sends whoever
                // reads it to the wrong thing.
                let mut lowest = now;
                let mut attempts = 0_u32;
                while now <= self.last_millis {
                    lowest = lowest.min(now);
                    if attempts >= WAIT_ATTEMPTS && lowest < began_at {
                        // **Memoised, like the sibling exit.** The conclusion
                        // was kept for a stopped clock and not for one that had
                        // moved backwards, so that half paid the whole
                        // `WAIT_ATTEMPTS` loop on every later call — measured,
                        // `now_ms()` called 1001 times per `mint`, four rounds
                        // running, against 1001 then 1 then 1 for the stopped
                        // case. With `SystemClock` each of those waits is up to
                        // ten thousand `yield_now()`, which is the 5.36 s per
                        // call this memo exists to stop.
                        let drift = began_at.saturating_sub(lowest);
                        self.concluded = Some((now, Verdict::MovedBackwards { drift }));
                        return Err(MintError::ClockMovedBackwards {
                            drift: Duration::from_millis(drift),
                        });
                    }
                    if attempts >= WAIT_ATTEMPTS {
                        self.concluded = Some((now, Verdict::Stopped));
                        // **The barrier, not the reading.** These are the same
                        // number only when the clock has not moved at all;
                        // after any partial step the message named a
                        // millisecond the clock had already passed. Measured:
                        // `ClockStuck { at: …017 }` with the value that must be
                        // passed being …020, and the sentence reading "the
                        // clock did not move past 017" while it read 017.
                        return Err(MintError::ClockStuck {
                            at: Duration::from_millis(self.last_millis),
                        });
                    }
                    self.clock.wait_past(now);
                    now = self.clock.now_ms();
                    attempts += 1;
                }
                Self::within_range(now)?;
                self.sequence = 0;
            } else {
                self.sequence += 1;
            }
        } else {
            // **The reset, and it could be deleted.** Removing this line left
            // `cargo xtask check` green: `sequence()` then counts up across
            // milliseconds for the life of the process — `ms+0: 0 and 1`,
            // `ms+1: 1 and 2`, `ms+2: 2 and 3` — so its documented meaning
            // ("its position within its millisecond") is false and the header's
            // "4,096 per millisecond" becomes "4,096 per process, then a
            // spurious exhaustion". `an_exhausted_sequence_waits_for_the_next_
            // millisecond` asserts a zero, but that zero comes from the wait
            // above rather than from here, and the proptest steps 0..3.
            self.sequence = 0;
        }
        self.last_millis = now;

        // Checked above, so the subtraction cannot wrap.
        let millis = now.saturating_sub(EPOCH_MS);
        Ok(Snowflake::assemble(
            millis,
            self.flake.value(),
            self.sequence,
        ))
    }

    /// Whether a clock reading is inside what 41 bits from the epoch can hold.
    fn within_range(now: u64) -> Result<(), MintError> {
        let millis = now.checked_sub(EPOCH_MS).ok_or(MintError::BeforeEpoch)?;
        if millis > TIMESTAMP_MASK {
            return Err(MintError::BeyondRange);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    /// A stopped clock is concluded once, not on every call.
    ///
    /// **It cost 5.36 s per `mint`, for ever.** `WAIT_ATTEMPTS` × `YIELDS_PER_WAIT`
    /// is a thousand waits of ten thousand yields, and the conclusion was thrown
    /// away — so a caller writing a hundred journal entries on a host whose clock
    /// has stopped waits about nine minutes, burning a core, to be told the same
    /// thing a hundred times. The crate's own stuck-clock test uses a no-op
    /// `wait_past`, which is why the cost was invisible here.
    #[test]
    fn a_stopped_clock_is_concluded_once() {
        // The clock the sibling test uses, which counts its own waits.
        let clock = StuckClock::at(EPOCH_MS + 5);
        let mut minter = Minter::new(flake(), &clock);
        for _ in 0..=SEQUENCE_MASK {
            minter
                .mint()
                .expect("mints until the sequence is exhausted");
        }
        minter.mint().expect_err("a stuck clock refuses");
        let after_the_first = clock.1.get();
        assert!(
            after_the_first > 0,
            "the first refusal did not wait at all, so this measures nothing"
        );

        for _ in 0..50 {
            let again = minter.mint().expect_err("a stuck clock keeps refusing");
            assert!(matches!(again, MintError::ClockStuck { .. }), "{again:?}");
        }
        assert_eq!(
            clock.1.get(),
            after_the_first,
            "fifty further calls walked the wait loop again, at {after_the_first} waits each"
        );
    }

    /// Two constants that must agree, and only one direction was coupled.
    #[test]
    fn the_two_flake_widths_are_one_width() {
        // **Two constants that must agree, with nothing saying so.**
        // `flake::BITS` is what a `MachineFlake` is masked to and
        // `snowflake::FLAKE_BITS` is where it sits in the identifier. They are
        // ten in two files, and only one direction is coupled by a test:
        // `no_two_flakes_collide_even_at_the_same_millisecond` fails when
        // `flake::BITS` is the *larger* of the two, because flakes then collide
        // — and a `flake::BITS` smaller than `FLAKE_BITS` wastes range with
        // nothing noticing.
        assert_eq!(
            super::FLAKE_BITS,
            nc_ids_flake_bits(),
            "the width a flake is masked to and the width it occupies in an identifier \
             have drifted apart"
        );
    }

    /// `flake::BITS`, read through the module rather than assumed.
    const fn nc_ids_flake_bits() -> u32 {
        crate::flake::BITS
    }

    /// The epoch, written out, and what an identifier minted at it decodes to.
    ///
    /// **The one constant whose doc says "never change this", and nothing was
    /// measuring it.** Every test in this crate is written as `EPOCH_MS + n`,
    /// so the constant was only ever compared against itself: moving it by nine
    /// years — 2026-01-01 to 2017-07-14 — left `cargo xtask check` green with
    /// 657 tests.
    ///
    /// What that costs is not a failing test somewhere. Every identifier ever
    /// minted is an offset from this number, so moving it renumbers history:
    /// `millis_since_epoch()` on a stored id is off by the delta, the 2095
    /// range boundary moves, the "names grow to thirteen characters in 2034"
    /// arithmetic in `base32.rs` moves, and ids minted either side of the
    /// change interleave wrongly wherever anything sorts by decoded time.
    ///
    /// Written out rather than derived, for the reason `WAIT_ATTEMPTS`'s pin
    /// gives: an assertion that reads the value under test cannot disagree with
    /// it. The instant is spelled twice — as the constant, and as the wall-clock
    /// time it is supposed to be — so that changing one of them is a failure
    /// rather than a rename.
    #[test]
    fn the_epoch_is_the_instant_it_says_it_is() {
        // 2026-01-01T00:00:00Z, computed from the calendar rather than quoted:
        // 56 years from 1970, of which 14 were leap years.
        const DAYS: u64 = 56 * 365 + 14;

        assert_eq!(
            super::EPOCH_MS,
            1_767_225_600_000,
            "EPOCH_MS changed. Every identifier ever minted is an offset from it, so \
             this is not a number that can be corrected — if the platform really is \
             starting again, every stored identifier has to move with it"
        );
        assert_eq!(
            super::EPOCH_MS,
            DAYS * 24 * 60 * 60 * 1_000,
            "EPOCH_MS is no longer 2026-01-01T00:00:00Z"
        );
        // And the range it buys, which is the reason the doc gives for it being
        // late: 41 bits of milliseconds from there runs to 2095.
        let last = super::EPOCH_MS + super::TIMESTAMP_MASK;
        let years = (last - super::EPOCH_MS) / (365 * 24 * 60 * 60 * 1_000);
        assert_eq!(years, 69, "41 bits is no longer about 69 years");
        assert_eq!(
            (last / (365 * 24 * 60 * 60 * 1_000)) + 1970,
            2095,
            "the range no longer runs to 2095, which the doc comment claims"
        );
    }

    use super::{
        Clock, EPOCH_MS, MintError, Minter, SEQUENCE_MASK, Snowflake, SystemClock, TIMESTAMP_MASK,
    };
    use crate::flake::MachineFlake;
    use core::cell::Cell;
    use core::time::Duration;

    /// A clock the test drives.
    #[derive(Debug)]
    struct FakeClock {
        millis: Cell<u64>,
        waits: Cell<usize>,
    }

    impl FakeClock {
        fn at(millis: u64) -> Self {
            Self {
                millis: Cell::new(millis),
                waits: Cell::new(0),
            }
        }
        fn set(&self, millis: u64) {
            self.millis.set(millis);
        }
        fn advance(&self) {
            self.millis.set(self.millis.get() + 1);
        }
    }

    // **On the fake itself**, with the reference covered by the blanket
    // `impl<C: Clock> Clock for &C` beside the trait. It used to be written on
    // `&FakeClock` only, which is why the blanket impls were missing: the one
    // caller that needed them had worked around their absence.
    impl Clock for FakeClock {
        fn now_ms(&self) -> u64 {
            self.millis.get()
        }
        fn wait_past(&self, millis: u64) {
            self.waits.set(self.waits.get() + 1);
            self.millis.set(millis + 1);
        }
    }

    /// One clock, two minters — and a clock chosen at run time.
    ///
    /// **Neither compiled.** `Clock` had no `impl` for a reference or a box
    /// outside `#[cfg(test)]`, so a kernel that owns one clock and wants two
    /// minters could not have them, and one that picks a clock at run time could
    /// not either:
    ///
    /// ```text
    /// error[E0277]: the trait bound `&Shared: Clock` is not satisfied
    /// error[E0277]: the trait bound `Box<dyn Clock>: Clock` is not satisfied
    /// ```
    #[test]
    fn one_clock_can_drive_two_minters_and_can_be_chosen_at_run_time() {
        // Two flakes, because that is what two minters on one machine are for:
        // the layout's uniqueness is per flake, and one clock is what they
        // share.
        let clock = FakeClock::at(EPOCH_MS + 5);
        let mut one = Minter::new(MachineFlake::from_value(1).expect("a flake"), &clock);
        let mut two = Minter::new(MachineFlake::from_value(2).expect("a flake"), &clock);
        let first = one.mint().expect("mints");
        let second = two.mint().expect("mints");
        assert_ne!(
            first, second,
            "two minters sharing one clock produced the same identifier"
        );

        let chosen: Box<dyn Clock> = Box::new(FakeClock::at(EPOCH_MS + 5));
        let mut boxed = Minter::new(flake(), chosen);
        assert!(
            boxed.mint().is_ok(),
            "a clock chosen at run time cannot mint"
        );
    }

    fn flake() -> MachineFlake {
        MachineFlake::from_value(7).expect("7 fits in ten bits")
    }

    /// A clock whose `wait_past` does nothing at all.
    ///
    /// The hazard `Clock`'s own documentation names, and the one that produced a
    /// duplicate identifier: a virtualised or suspended host where the wait
    /// returns immediately and the reading never moves.
    #[derive(Debug)]
    struct StuckClock(u64, Cell<u32>);

    impl StuckClock {
        fn at(millis: u64) -> Self {
            Self(millis, Cell::new(0))
        }
    }

    impl Clock for &StuckClock {
        fn now_ms(&self) -> u64 {
            self.0
        }
        fn wait_past(&self, _millis: u64) {
            self.1.set(self.1.get() + 1);
        }
    }

    /// A clock that is stuck, then healthy: `wait_past` advances once armed.
    ///
    /// `StuckClock` can only ever be stuck, which is why it could not reach the
    /// half of the memo below.
    struct RecoveringClock {
        now: Cell<u64>,
        healthy: Cell<bool>,
    }

    impl RecoveringClock {
        fn stuck_at(millis: u64) -> Self {
            Self {
                now: Cell::new(millis),
                healthy: Cell::new(false),
            }
        }
        /// Time starts moving again, and a wait ends rather than spinning.
        fn heal(&self) {
            self.healthy.set(true);
            self.now.set(self.now.get() + 1);
        }
    }

    impl Clock for &RecoveringClock {
        fn now_ms(&self) -> u64 {
            self.now.get()
        }
        fn wait_past(&self, millis: u64) {
            if self.healthy.get() && self.now.get() <= millis {
                self.now.set(millis + 1);
            }
        }
    }

    /// A conclusion belongs to the millisecond it was reached in.
    ///
    /// **`at == now` was decided by nothing.** Dropping it — `if let Some((_at,
    /// verdict)) = self.concluded` — left `cargo test --workspace` and `cargo
    /// xtask check` green, because `concluded` is never cleared and every
    /// existing case asks the same millisecond twice. The memoisation half has
    /// two tests; this half had none, and `AGENTS.md` names this exact repair.
    ///
    /// What it lets through: a machine whose clock stopped for one millisecond,
    /// recovered, and is then refused for the rest of the process with
    /// `NC-IDS-0007` — severity **critical**, *"the clock has stopped, so no
    /// more identifiers can be minted on this machine"* — reported about a clock
    /// that is fine. The memo is an optimisation for a stuck clock; without the
    /// guard it becomes a permanent verdict.
    #[test]
    fn a_conclusion_does_not_outlive_the_millisecond_it_was_reached_in() {
        let clock = RecoveringClock::stuck_at(EPOCH_MS + 5);
        let mut minter = Minter::new(flake(), &clock);
        // Fill the stuck millisecond, then reach the conclusion that writes the memo.
        for _ in 0..=SEQUENCE_MASK {
            minter
                .mint()
                .expect("mints until the sequence is exhausted");
        }
        let stuck = minter.mint().expect_err("a stuck clock refuses");
        assert!(matches!(stuck, MintError::ClockStuck { .. }), "{stuck:?}");

        // The clock recovers. The next millisecond mints normally.
        clock.heal();
        for _ in 0..=SEQUENCE_MASK {
            minter
                .mint()
                .expect("a healthy clock mints through a fresh millisecond");
        }
        // And exhausting *that* millisecond is a wait, not the old conclusion.
        let after = minter
            .mint()
            .expect("the wait runs and the next millisecond mints");
        assert!(
            // `millis_since_epoch` is the offset, and the clock was stuck at
            // offset 5: it healed into 6 and the wait carried this mint to 7.
            after.millis_since_epoch() > 5,
            "the mint that followed a recovered clock landed at {}, in or before the \
             millisecond the machine was stuck in",
            after.millis_since_epoch()
        );
    }

    #[test]
    fn every_minting_failure_is_reported_under_a_code_of_its_own() {
        // The module header names the line each of these is queried by, and the
        // mappings had no reader — `ClockStuck` could be repointed to
        // `IdsClockBackwards` with the whole suite green, which would send a
        // responder to look at a clock that moved when the problem is a clock
        // that did not.
        use core::time::Duration;
        use nc_catalog::ErrorCode;
        assert_eq!(
            MintError::ClockStuck {
                at: Duration::from_millis(1)
            }
            .code(),
            // **Not `IdsSequenceExhausted`.** That code is severity `info` and
            // its description is about a wait a caller is documented never to
            // receive; a stuck clock is a machine that has stopped minting until
            // somebody fixes its time, and a responder reading "waited" about it
            // looks in the wrong place.
            ErrorCode::IdsClockStuck
        );
        assert_eq!(
            MintError::ClockMovedBackwards {
                drift: Duration::from_millis(1)
            }
            .code(),
            ErrorCode::IdsClockBackwards
        );
        // **And the three the name of this test claims and nothing held.**
        // `MintError` has five variants and two of their codes were pinned. The
        // other three were held by nothing: `the_clock_codes_say_retry_where_
        // retrying_works` asserts only that they are not retryable and are not
        // `IdsClockBackwards`, which any non-retryable code in the registry
        // satisfies, and every other fixture matches on the *variant*. Measured
        // in round seventy-one wave 9: `BeyondRange | BeforeEpoch` repointed to
        // `IdsNameTooLong` left `119 passed`, so a clock set to 1999 was
        // reported under *"A file name cannot be built within the filesystem's
        // limits"*. `§15.1` calls a code the line a log is queried by.
        assert_eq!(
            MintError::BeyondRange.code(),
            ErrorCode::IdsClockOutOfRange,
            "a clock past the top of the layout is reported under a code that is not \
             about a clock"
        );
        assert_eq!(
            MintError::BeforeEpoch.code(),
            ErrorCode::IdsClockOutOfRange,
            "a clock before the epoch is reported under a code that is not about a clock"
        );
        // **The fifth, and it is written down as a question rather than a
        // settled mapping.** `MarkOutOfRange` carries this code too, and
        // NC-IDS-0008 renders in both locales as *"This computer's clock is set
        // to the wrong year, so nothing can be saved. Fix the date and time,
        // then try again."* That sentence is true of the two above and false of
        // this one by construction: `resuming_at` reaches `MarkOutOfRange` only
        // after `within_range(now)?` has just established that the clock is
        // inside the layout. The clock is fine; the persisted mark is a wrong
        // number, which is what the variant's own `Display` says.
        //
        // What it costs is not cosmetic: the one condition whose first place to
        // look is the mark store is reported to a person as a machine with the
        // wrong date, **and moving the clock forward past the corrupt mark is
        // the single action that makes `resuming_at` accept the number it has
        // just diagnosed as corrupt**. `catalog/errors.toml` states the rule at
        // NC-IDS-0006 — "a code is only worth having if its description is true
        // of every failure that carries it" — and split that code out on
        // precisely this ground.
        //
        // **Decided, and the pin moved with it.** It stood at
        // `IdsClockOutOfRange` because changing which code a failure carries
        // changes the line a log is queried by, and that is the developer's
        // call rather than this suite's — so the pin was put here to make the
        // decision land on a line somebody has to edit. It was made on
        // 2026-09-27, on the developer's instruction to decide it, and it is
        // the repair this paragraph already named: `ValidationOutOfRange`,
        // which `flake::FlakeOutOfRange` uses for the same thing, adding no
        // code and renumbering none. Reported by the reviewer that closed
        // wave 9.
        //
        // What is still the developer's: a code of its own for a corrupt mark
        // is an `errors.toml` reservation, and reservations are permanent.
        assert_eq!(
            MintError::MarkOutOfRange { millis: 1 }.code(),
            ErrorCode::ValidationOutOfRange,
            "the mapping moved and the paragraph above it did not: this is the one the \
             developer has been asked about"
        );
        // A stuck clock and a clock that moved backwards are different problems
        // and different first places to look.
        assert_ne!(
            MintError::ClockStuck {
                at: Duration::from_millis(1)
            }
            .code(),
            MintError::ClockMovedBackwards {
                drift: Duration::from_millis(1)
            }
            .code()
        );
    }

    #[test]
    fn a_stuck_clock_refuses_for_ever_and_never_repeats_an_identifier() {
        // The route to a literal duplicate. Every exit from the exhausted branch
        // is an early return, so a `sequence` raised before the check stayed
        // raised — and on the 65 536th refusal the `u16` wrapped to zero and the
        // minter handed out an identifier it had already issued. In a release
        // build that is silent; in a debug build it is an overflow panic. The
        // count below is past the wrap on purpose.
        let clock = StuckClock::at(EPOCH_MS + 1);
        let mut minter = Minter::new(flake(), &clock);
        let mut issued = std::collections::BTreeSet::new();
        let mut refusals = 0_u32;
        for _ in 0..70_000_u32 {
            match minter.mint() {
                Ok(identifier) => assert!(
                    issued.insert(identifier),
                    "{identifier:?} was minted twice by a clock that never moved"
                ),
                Err(MintError::ClockStuck { .. }) => refusals += 1,
                Err(other) => panic!("the wrong refusal: {other:?}"),
            }
        }
        // One millisecond holds exactly one sequence's worth, and every call
        // after that is refused. A minter that borrowed from the next
        // millisecond would have kept minting.
        assert_eq!(issued.len() as u64, SEQUENCE_MASK + 1);
        assert_eq!(u64::from(refusals), 70_000 - (SEQUENCE_MASK + 1));
    }

    /// A clock outside the layout is refused when a minter is resumed.
    ///
    /// **The guard was a repair nothing drove.** Deleting
    /// `Self::within_range(now)?` left the whole suite green — and without it
    /// one fault gets two error codes with opposite advice: `resuming_at`
    /// answers `ClockMovedBackwards` (`NC-IDS-0001`, retryable, which
    /// `the_clock_codes_say_retry_where_retrying_works` certifies) while `mint`
    /// answers `BeforeEpoch` (`NC-IDS-0008`, not retryable) for the same host.
    /// Which one a caller is told depends on the door they came through.
    #[test]
    fn a_clock_outside_the_layout_is_refused_when_resuming() {
        let early = FakeClock::at(EPOCH_MS - 1);
        assert!(
            matches!(
                Minter::resuming_at(flake(), &early, EPOCH_MS + 100),
                Err(MintError::BeforeEpoch)
            ),
            "a clock before the epoch is refused as a clock, not as a mark"
        );
        let late = FakeClock::at(EPOCH_MS + TIMESTAMP_MASK + 1);
        assert!(
            matches!(
                Minter::resuming_at(flake(), &late, EPOCH_MS + 100),
                Err(MintError::BeyondRange)
            ),
            "a clock past the range this layout can hold is refused as a clock"
        );
    }

    #[test]
    fn a_resumed_minter_does_not_reissue_within_the_millisecond_it_resumed_at() {
        // What `resuming_at`'s sequence-starts-exhausted rule is **for**, and the
        // case it had no test for: the only `resuming_at` test drives the clock
        // backwards, which takes a different branch entirely.
        //
        // A process issues sequences 0..=4 at millisecond 100 and stops. It
        // restarts with 100 as its high-water mark. With the sequence starting at
        // zero it would hand out (100, 1) — an identifier the previous process
        // already gave away. Starting exhausted, it waits and mints at 101.
        let clock = FakeClock::at(EPOCH_MS + 100);
        let mut minter =
            Minter::resuming_at(flake(), &clock, EPOCH_MS + 100).expect("a mark inside the layout");
        let first = minter.mint().expect("mints");
        assert!(
            first.millis_since_epoch() > 100,
            "it minted at {} — the millisecond the previous process was still using",
            first.millis_since_epoch()
        );
        assert_eq!(clock.waits.get(), 1, "it did not wait");
    }

    #[test]
    fn an_identifier_that_would_set_the_sign_bit_is_refused() {
        // The layout is 63 bits and `Snowflake` is a `u64`, so the sign bit is
        // always clear — a property the module header states and nothing tested.
        // Thirteen characters is a legal Crockford width, so this arrives through
        // the length check and has to be caught by the value check behind it.
        let error = Snowflake::from_crockford("8000000000000").expect_err("must refuse");
        assert!(
            matches!(
                error,
                crate::base32::Base32Error::Overflows { bits: 63, .. }
            ),
            "{error:?}"
        );
        // **And the sentence says 63.** It said 64 for both ceilings, so a value
        // this layout refuses was reported as needing more than the 64 bits
        // `base32::decode` had just fitted it into. Measured:
        //
        // ```text
        // 8000000000000 -> decode = Ok(9223372036854775808)
        //                  from_crockford = Err(Overflows { length: 13 })
        //     "…needs more than the 64 bits an identifier has"
        // ```
        assert!(
            error.to_string().contains("63 bits"),
            "the ceiling this refused is 63 and the sentence says otherwise: {error}"
        );
        // And the largest value that does fit is accepted, so the bound is
        // checked on both sides rather than only above.
        let largest = Snowflake::from_crockford("7ZZZZZZZZZZZZ").expect("63 bits fit");
        assert_eq!(largest.value() >> 63, 0);
    }

    #[test]
    fn a_clock_at_the_far_edge_of_the_layout_is_refused_on_both_sides() {
        // The upper bound, at the boundary rather than either side of it. A bound
        // tested only well inside and well outside is a bound nobody has checked:
        // doubling it here let a clock past the end of the range mint an
        // identifier whose timestamp had wrapped to zero — an identifier from
        // 2095 sorting before every identifier ever minted.
        let last = FakeClock::at(EPOCH_MS + TIMESTAMP_MASK);
        let mut minter = Minter::new(flake(), &last);
        let ok = minter.mint().expect("the last millisecond in range mints");
        assert_eq!(ok.millis_since_epoch(), TIMESTAMP_MASK);

        let past = FakeClock::at(EPOCH_MS + TIMESTAMP_MASK + 1);
        let mut minter = Minter::new(flake(), &past);
        assert!(matches!(minter.mint(), Err(MintError::BeyondRange)));
    }

    #[test]
    fn a_wait_that_lands_past_the_range_is_refused_rather_than_minted() {
        // The range re-check **after** the wait. Without it, a clock that jumped
        // while the minter was waiting out an exhausted sequence minted an
        // identifier with a fabricated timestamp instead of refusing.
        #[derive(Debug)]
        struct JumpingClock {
            millis: Cell<u64>,
        }
        impl Clock for &JumpingClock {
            fn now_ms(&self) -> u64 {
                self.millis.get()
            }
            fn wait_past(&self, _millis: u64) {
                self.millis.set(u64::MAX / 2);
            }
        }
        let clock = JumpingClock {
            millis: Cell::new(EPOCH_MS + 1),
        };
        let mut minter = Minter::new(flake(), &clock);
        for _ in 0..=SEQUENCE_MASK {
            minter.mint().expect("the first millisecond's worth mints");
        }
        assert!(
            matches!(minter.mint(), Err(MintError::BeyondRange)),
            "a clock that jumped past the range during the wait minted anyway"
        );
    }

    #[test]
    fn identifiers_are_strictly_increasing_within_a_process() {
        let clock = FakeClock::at(EPOCH_MS + 1);
        let mut minter = Minter::new(flake(), &clock);
        let mut previous = minter.mint().expect("mints");
        for step in 0..500 {
            if step % 100 == 0 {
                clock.advance();
            }
            let next = minter.mint().expect("mints");
            assert!(next > previous, "{next:?} did not follow {previous:?}");
            previous = next;
        }
    }

    #[test]
    fn no_two_flakes_collide_even_at_the_same_millisecond() {
        // The property the machine flake exists for: machines minting in the
        // same millisecond with the same sequence still differ.
        //
        // **Every flake there is, not two of them.** Acceptance criterion 9 asks
        // for uniqueness "across a simulated set of flakes", and this simulated
        // exactly two — flake 1 and flake 2, which differ in one bit. Two
        // machines is not a set, and a defect in how the flake is placed in the
        // value would have to be very unlucky to show up between neighbours: the
        // whole `RANGE` is what the field can hold, so the whole `RANGE` is what
        // this mints from, at one instant, and every identifier has to be
        // distinct.
        let clock = FakeClock::at(EPOCH_MS + 1);
        let mut minters: Vec<Minter<&FakeClock>> = (0..crate::flake::RANGE)
            .map(|value| Minter::new(MachineFlake::from_value(value).expect("fits"), &clock))
            .collect();
        let mut seen = std::collections::BTreeSet::new();
        // Enough rounds that the sequence field turns over inside the
        // millisecond for every one of them, so the collision this rules out is
        // between *flakes* and not merely between sequences.
        let rounds = 200;
        for _ in 0..rounds {
            for minter in &mut minters {
                assert!(
                    seen.insert(minter.mint().expect("mints")),
                    "two flakes minted the same identifier"
                );
            }
        }
        assert_eq!(seen.len(), rounds * usize::from(crate::flake::RANGE));
        // And every flake really was distinct in the values, or the assertion
        // above could pass on the sequence alone.
        let flakes: std::collections::BTreeSet<u16> = seen.iter().map(|id| id.flake()).collect();
        assert_eq!(flakes.len(), usize::from(crate::flake::RANGE));
    }

    #[test]
    fn a_clock_that_moved_backwards_refuses_rather_than_minting_a_duplicate() {
        let clock = FakeClock::at(EPOCH_MS + 1_000);
        let mut minter = Minter::new(flake(), &clock);
        minter.mint().expect("mints");
        clock.set(EPOCH_MS + 400);
        let error = minter.mint().expect_err("must refuse");
        assert!(matches!(error, MintError::ClockMovedBackwards { .. }));
        assert_eq!(error.code(), nc_catalog::ErrorCode::IdsClockBackwards);
        if let MintError::ClockMovedBackwards { drift } = error {
            assert_eq!(drift.as_millis(), 600);
        }
    }

    /// The bit above the layout is nobody's field.
    ///
    /// **The accessor's own mask was reachable by nothing.** `assemble` masks
    /// each field on the way in and `from_crockford` refuses a value past the
    /// layout, so no `Snowflake` a caller can build has bit 63 set — and
    /// `millis_since_epoch`'s `& TIMESTAMP_MASK` could be deleted with the whole
    /// workspace green. That is not the same as the mask being wrong: it is a
    /// clause in a third state, neither tested nor deleted, in a file whose
    /// sibling test exists because deleting `assemble`'s three masks was also
    /// green.
    ///
    /// The layout is 41 + 10 + 12 = 63 bits of a `u64`, so the sixty-fourth is
    /// the one place a value can carry something the layout does not name. The
    /// accessors are `pub` and total over `u64`; this says what they answer
    /// there, which is the same as what they answer without it.
    #[test]
    fn the_bit_above_the_layout_belongs_to_no_field() {
        let inside = Snowflake::assemble(TIMESTAMP_MASK, u16::MAX, super::SEQUENCE_MASK_U16);
        let above = Snowflake(inside.value() | 1 << 63);
        assert_ne!(
            above.value(),
            inside.value(),
            "the fixture sets no extra bit"
        );
        assert_eq!(
            above.millis_since_epoch(),
            inside.millis_since_epoch(),
            "a bit outside the layout was read as part of the timestamp"
        );
        assert_eq!(above.flake(), inside.flake());
        assert_eq!(above.sequence(), inside.sequence());
    }

    #[test]
    fn every_field_stays_inside_its_own_bits() {
        // **All three masks, from outside.** `mint` guarantees the sequence is
        // at most 4095, so `assemble`'s `& SEQUENCE_MASK` is defence in depth —
        // and deleting it left the whole suite green, while the flake and the
        // timestamp masks sit in the same expression and the module header rests
        // its 63-bit claim on all three. A field that overflowed into its
        // neighbour would produce an identifier whose timestamp is a lie, which
        // is the one thing the layout exists to prevent.
        //
        // `assemble` is private, so this is written through it directly rather
        // than through `mint`, which cannot produce the out-of-range values.
        // **Read as the whole value, and one field at a time.** Reading the
        // accessors back was not enough: `flake()` and `sequence()` re-apply the
        // same masks, so a mask missing from `assemble` was invisible through
        // them — and saturating every field at once hides the overflow inside a
        // neighbour that is already all ones.
        //
        // So each field is given a value too large for it while its neighbours
        // are zero, and the whole `u64` is compared. Bits that leak show up.
        assert_eq!(
            Snowflake::assemble(0, 0, u16::MAX).value(),
            super::SEQUENCE_MASK,
            "the sequence took bits that belong to the flake"
        );
        assert_eq!(
            Snowflake::assemble(0, u16::MAX, 0).value(),
            super::FLAKE_MASK << super::SEQUENCE_BITS,
            "the flake took bits that belong to the timestamp"
        );
        assert_eq!(
            Snowflake::assemble(u64::MAX, 0, 0).value(),
            TIMESTAMP_MASK << (super::FLAKE_BITS + super::SEQUENCE_BITS),
            "the timestamp took the sign bit or its neighbours'"
        );
        // And every field at once still fits in the sixty-three bits the layout
        // claims, with the sign bit clear.
        let over = Snowflake::assemble(u64::MAX, u16::MAX, u16::MAX);
        assert!(over.value() < (1_u64 << 63), "{:#x}", over.value());

        // Each field reads back exactly what it was given, so the assertions
        // above are not passing because everything saturates.
        let ordinary = Snowflake::assemble(1_234, 7, 42);
        assert_eq!(ordinary.millis_since_epoch(), 1_234);
        assert_eq!(ordinary.flake(), 7);
        assert_eq!(ordinary.sequence(), 42);
    }

    /// The shipped clock waits until the millisecond has actually turned.
    ///
    /// **The wait's *success* path was exercised by nothing.** Every test of the
    /// exhaustion guard uses a fake, and the one test of `SystemClock` asks it to
    /// wait past an hour from now — a barrier it cannot pass — so it asserts the
    /// wait *returns* and that it is fast, and both stay true when the wait
    /// stops waiting. Measured on 2026-09-08, `if self.now_ms() > millis` widened
    /// to `>=`:
    ///
    /// ```text
    /// cargo xtask check: check passed, every printed line unchanged
    /// Minter::new(flake, SystemClock), 20 000 mints in a tight loop:
    ///   clean    minted 20000, refusals 0
    ///   planted  minted 16876, refusals 3124  ClockStuck { … } NC-IDS-0007
    /// ```
    ///
    /// One mint in six refused under this crate's only **critical** code, whose
    /// sentence is *"the clock has stopped, so no more identifiers can be minted
    /// on this machine"* — told to a responder about a host whose clock is fine.
    /// The layout's promise is 4 096 per millisecond and *"a process that reaches
    /// it is doing something a wait will not hurt"*; this is that wait.
    ///
    /// Asked to wait past *now*, which is the barrier a real caller hits: it is
    /// one millisecond away and the clock is running, so the wait must succeed
    /// rather than merely return.
    #[test]
    fn the_shipped_clock_waits_until_the_millisecond_has_turned() {
        for _ in 0..8 {
            let before = SystemClock.now_ms();
            SystemClock.wait_past(before);
            let after = SystemClock.now_ms();
            assert!(
                after > before,
                "the wait returned at {after} without passing {before}: a caller that has \
                 spent its 4096 sequence numbers is told the clock has stopped, which is \
                 this crate's only critical error and is about a host that is fine"
            );
        }
    }

    /// The shipped clock's wait returns when the clock will not move, having
    /// yielded exactly [`super::YIELDS_PER_WAIT`] times.
    ///
    /// **Every test of the `ClockStuck` guard uses a fake whose `wait_past`
    /// returns immediately**, and the shipped `SystemClock`'s did not: it was
    /// `while now_ms() <= millis { yield_now() }`, so on a frozen or suspended
    /// host `mint`'s bounded loop never got its turn and the caller spun a core
    /// instead of receiving an error. `SystemClock` is not test-only —
    /// `nc-journal`'s `Journal<S, C = SystemClock>` is what the platform writes
    /// through.
    ///
    /// Asked to wait past an hour from now, which is what a stopped clock looks
    /// like from inside the wait, through the seam the shipped wait runs: the
    /// yields are counted, which is the bound from both sides. A smaller one —
    /// `0..YIELDS_PER_WAIT / 10_000`, measured — turns an ordinary sequence
    /// exhaustion on a healthy host into `ClockStuck`, `NC-IDS-0007`, *the clock
    /// has stopped*: 20 000 mints in a tight loop went from `minted 20000,
    /// refusals 0` to `minted 6527, refusals 4`. **Counted, never timed**: a
    /// reviewer found the five-second bound this replaced failing every run
    /// with four spinners on its core — a red gate on a loaded machine nobody
    /// could tell from a real one — as a 500 ms bound before it had under a
    /// gate's ordinary load.
    #[test]
    fn the_shipped_clocks_wait_returns_after_its_yields_when_the_clock_will_not_move() {
        let far_ahead = SystemClock.now_ms() + 3_600_000;
        let mut yields = 0_u32;
        SystemClock.wait_past_with(far_ahead, || yields += 1);
        assert_eq!(
            yields,
            super::YIELDS_PER_WAIT,
            "the wait does not spend `YIELDS_PER_WAIT` yields on a clock that will not move"
        );
        // **And how long one wait takes, which is the number that decides.**
        // `YIELDS_PER_WAIT` is half of a product: `mint` tries the wait
        // `WAIT_ATTEMPTS` times, so what a caller on a frozen host actually
        // pays is the two multiplied. Measured on the shipped clock:
        //
        // ```text
        // one wait that makes no progress   6.5ms
        // mint() #4097 on a frozen host     5.37s  -> Err(ClockStuck)
        // and the next call pays it again   5.36s
        // ```
        //
        // Raising `YIELDS_PER_WAIT` a hundredfold left the whole crate green in
        // half a second, and would make that five seconds nine minutes — per
        // `mint`, for ever, because a frozen clock stays exhausted.
        assert_eq!(
            super::YIELDS_PER_WAIT,
            10_000,
            "YIELDS_PER_WAIT changed. It is half of what a caller pays on a frozen \
             host — the other half is WAIT_ATTEMPTS — so change the argument in its \
             doc comment too, and say what the product now is"
        );
    }

    #[test]
    fn a_wait_that_makes_no_progress_becomes_an_error_rather_than_a_spin() {
        // The other half, with the sequence exhausted so the wait is actually
        // reached: `WAIT_ATTEMPTS` waits that change nothing produce the error a
        // responder can act on.
        let clock = StuckClock::at(EPOCH_MS + 5);
        let mut minter = Minter::new(flake(), &clock);
        for _ in 0..=SEQUENCE_MASK {
            minter.mint().expect("mints");
        }
        assert_eq!(clock.1.get(), 0, "nothing should have waited yet");
        let error = minter.mint().expect_err("a stuck clock must refuse");
        assert!(matches!(error, MintError::ClockStuck { .. }), "{error:?}");
        assert_eq!(error.code(), nc_catalog::ErrorCode::IdsClockStuck);
        // **The bound itself, which nothing checked**: raising `WAIT_ATTEMPTS`
        // from a thousand to a hundred thousand was green, and the difference
        // between those two is how long a caller waits for an error it is going
        // to get either way.
        // **Written out, not read from the constant.** The assertion said
        // `clock.1.get() == super::WAIT_ATTEMPTS`, which reads the value under
        // test and therefore cannot disagree with it: `WAIT_ATTEMPTS = 1` passed
        // the whole gate. The number is a decision about how long a caller waits
        // for an error it is going to get anyway, so it is written here the way
        // this repository writes every other mapping it refuses to derive.
        assert_eq!(
            clock.1.get(),
            1_000,
            "the wait was not tried a thousand times before refusing"
        );
        assert_eq!(
            super::WAIT_ATTEMPTS,
            1_000,
            "WAIT_ATTEMPTS changed; if that was deliberate, change the number here too \
             and say why in its doc comment"
        );
    }

    #[test]
    fn one_millisecond_backwards_is_already_a_duplicate() {
        // **The boundary, which is the size the failure actually comes in.** The
        // test above steps back six hundred milliseconds and the resumed-minter
        // test steps back fifty; loosening the comparison to `now + 1 <
        // last_millis` — which tolerates exactly the granularity NTP slew and
        // leap-second smearing operate at — left both of them green and the whole
        // gate passing.
        //
        // And it is not a theoretical duplicate. Minting at 99, minting at 100
        // and stepping back to 99 re-issues the identifier handed out at 99: the
        // sequence is per millisecond, so a repeated millisecond repeats it from
        // zero. `§16.2`'s rule is refuse rather than repeat, and the smallest
        // step is the one nobody was watching.
        let clock = FakeClock::at(EPOCH_MS + 99);
        let mut minter = Minter::new(flake(), &clock);
        let first = minter.mint().expect("mints");
        clock.set(EPOCH_MS + 100);
        minter.mint().expect("mints");

        clock.set(EPOCH_MS + 99);
        let error = minter
            .mint()
            .expect_err("one millisecond backwards must refuse");
        assert!(
            matches!(error, MintError::ClockMovedBackwards { drift } if drift.as_millis() == 1),
            "{error:?}"
        );
        // And what refusing prevented: the value the minter would otherwise have
        // handed out a second time.
        clock.set(EPOCH_MS + 101);
        assert_ne!(minter.mint().expect("mints"), first);
    }

    #[test]
    fn an_exhausted_sequence_waits_for_the_next_millisecond() {
        // And the count of waits is asserted, not just the outcome: a minter
        // that silently wrapped the sequence would produce the same identifiers
        // again, and the values alone would not say which happened.
        let clock = FakeClock::at(EPOCH_MS + 5);
        let mut minter = Minter::new(flake(), &clock);
        for _ in 0..=SEQUENCE_MASK {
            minter.mint().expect("mints");
        }
        assert_eq!(clock.waits.get(), 0, "nothing should have waited yet");
        let next = minter.mint().expect("mints after waiting");
        assert_eq!(clock.waits.get(), 1, "the minter did not wait");
        assert_eq!(next.sequence(), 0);
        assert_eq!(next.millis_since_epoch(), 6);
    }

    #[test]
    fn a_clock_that_does_not_advance_refuses_rather_than_repeating_an_identifier() {
        // The `Clock` doc names this hazard and an earlier version trusted the
        // call, so a stuck clock minted a literal duplicate.
        struct Stuck(u64);
        impl super::Clock for &Stuck {
            fn now_ms(&self) -> u64 {
                self.0
            }
            fn wait_past(&self, _: u64) {}
        }
        let stuck = Stuck(EPOCH_MS + 5);
        let mut minter = Minter::new(flake(), &stuck);
        for _ in 0..=SEQUENCE_MASK {
            minter.mint().expect("mints");
        }
        assert!(matches!(minter.mint(), Err(MintError::ClockStuck { .. })));
    }

    #[test]
    fn one_glitched_reading_does_not_poison_the_minter() {
        // A single reading far in the future used to latch `last_millis` to it,
        // and every later call returned `ClockMovedBackwards` with an absurd
        // drift, forever.
        let clock = FakeClock::at(EPOCH_MS + 10);
        let mut minter = Minter::new(flake(), &clock);
        minter.mint().expect("mints");
        clock.set(u64::MAX / 2);
        assert_eq!(minter.mint(), Err(MintError::BeyondRange));
        clock.set(EPOCH_MS + 11);
        minter
            .mint()
            .expect("the minter recovers when the clock does");
    }

    #[test]
    fn a_resumed_minter_refuses_to_repeat_what_a_previous_process_handed_out() {
        // The restart hazard: a fresh minter starts at zero and cannot know.
        let clock = FakeClock::at(EPOCH_MS + 100);
        let mut first = Minter::new(flake(), &clock);
        first.mint().expect("mints");
        let mark = first
            .high_water_mark()
            .expect("a minter that has minted has a mark");

        clock.set(EPOCH_MS + 50);
        let mut fresh = Minter::new(flake(), &clock);
        fresh
            .mint()
            .expect("a fresh minter has no idea, which is the hazard");

        // **Refused at construction, not at the first mint.** A mark ahead of the
        // clock is exactly this hazard, and it is knowable the moment the mark
        // is handed over — so the refusal is there, where a caller is still
        // deciding what to do, rather than one call later inside the thing that
        // was supposed to be safe.
        assert!(matches!(
            Minter::resuming_at(flake(), &clock, mark),
            Err(MintError::ClockMovedBackwards { .. })
        ));

        // And with the clock back where the mark came from, it is accepted and
        // the sequence starts exhausted — the rule this test is named for.
        clock.set(EPOCH_MS + 100);
        let mut resumed =
            Minter::resuming_at(flake(), &clock, mark).expect("a mark the clock has reached");
        assert!(matches!(
            resumed.mint(),
            Err(MintError::ClockStuck { .. }) | Ok(_)
        ));
    }

    #[test]
    fn a_clock_before_the_epoch_is_refused() {
        let clock = FakeClock::at(EPOCH_MS - 1);
        let mut minter = Minter::new(flake(), &clock);
        assert_eq!(minter.mint(), Err(MintError::BeforeEpoch));
        // And the two directions are distinguishable, which they were not: a clock
        // before the epoch was reported under a message saying "past the range".
        assert_ne!(MintError::BeforeEpoch, MintError::BeyondRange);
        assert!(MintError::BeforeEpoch.to_string().contains("before"));
    }

    #[test]
    fn the_parts_come_back_out_of_the_value() {
        let one = Snowflake::assemble(123_456, 789, 1_011);
        assert_eq!(one.millis_since_epoch(), 123_456);
        assert_eq!(one.flake(), 789);
        assert_eq!(one.sequence(), 1_011);
        // And the sign bit stays clear, so a signed reader sees the same number.
        assert!(one.value() < 1 << 63);

        // **Every field at its widest, because two of the three were read only
        // in their low half.** `millis_since_epoch` is asserted at
        // `TIMESTAMP_MASK` and `flake` at 789, which has its top bit set; the
        // sequence was asserted at 42 and at 1_011, both under 2_048, so the
        // top bit of its twelve-bit field was read by no assertion at all.
        // A minter reaches 4_095 inside one millisecond, so a mask one bit
        // short makes every identifier from the upper half of every
        // millisecond report a position that is not its own, and two distinct
        // identifiers report the same one. It is public API of a published
        // crate. Measured: `& (SEQUENCE_MASK >> 1)` left `ok 1130 test(s)` and
        // `check passed`.
        let widest = Snowflake::assemble(
            super::TIMESTAMP_MASK,
            u16::try_from(super::FLAKE_MASK).expect("ten bits fit a u16"),
            super::SEQUENCE_MASK_U16,
        );
        assert_eq!(widest.millis_since_epoch(), super::TIMESTAMP_MASK);
        assert_eq!(u64::from(widest.flake()), super::FLAKE_MASK);
        assert_eq!(widest.sequence(), super::SEQUENCE_MASK_U16);
        assert!(widest.value() < 1 << 63);

        // And one bit into the upper half of each field, so a mask that keeps
        // the top bit and loses the one below it is caught too.
        let halfway = Snowflake::assemble(1 << 40, 1 << 9, 1 << 11);
        assert_eq!(halfway.millis_since_epoch(), 1 << 40);
        assert_eq!(halfway.flake(), 1 << 9);
        assert_eq!(halfway.sequence(), 1 << 11);
    }

    /// A resumed minter reports the mark it was resumed from.
    ///
    /// The other side of `a_minter_that_has_not_minted_has_no_mark`, which only
    /// ever constructs through `Minter::new` — so the doc's claim about `None`
    /// was never tested on the path that contradicts it.
    #[test]
    fn a_resumed_minter_has_the_mark_it_was_given() {
        let clock = FakeClock::at(EPOCH_MS + 500);
        let resumed =
            Minter::resuming_at(flake(), &clock, EPOCH_MS + 50).expect("a mark behind the clock");
        assert_eq!(
            resumed.high_water_mark(),
            Some(EPOCH_MS + 50),
            "a resumed minter has a mark before it has minted, and that is what there \
             is to persist"
        );
    }

    /// A minter that has minted nothing has no mark, and says so.
    ///
    /// **It said zero, which `resuming_at` refuses.** A caller persisting on a
    /// timer wrote that value and was answered `BeforeEpoch` on the next start —
    /// reported to a person, in both locales, as a clock set to the wrong year,
    /// on a machine whose clock is fine. Nothing to persist is a state a caller
    /// can act on; zero is a state it has to know about this type to read.
    #[test]
    fn a_minter_that_has_not_minted_has_no_mark() {
        let clock = FakeClock::at(EPOCH_MS + 100);
        let fresh = Minter::new(flake(), &clock);
        assert_eq!(fresh.high_water_mark(), None);

        // And the moment it has minted, it has one — which `resuming_at`
        // accepts, so the round trip a caller actually makes works.
        let mut minted = Minter::new(flake(), &clock);
        minted.mint().expect("mints");
        let mark = minted
            .high_water_mark()
            .expect("a minter that has minted has a mark");
        assert!(
            Minter::resuming_at(flake(), &clock, mark).is_ok(),
            "the mark a minter hands out is not one it will take back"
        );
    }

    /// A mark inside the layout but far ahead of the clock is refused as corrupt.
    ///
    /// **The guard only refused marks outside the 41 bits**, and the test below
    /// reaches `0`, `1`, `EPOCH_MS - 1`, `u64::MAX` and `EPOCH_MS + 100` —
    /// nothing between. So a mark at the top of the layout came back as
    /// `ClockMovedBackwards { drift: 2199023255.451s }`, sixty-nine years, under
    /// `NC-IDS-0007`, which `the_clock_codes_say_retry_where_retrying_works`
    /// certifies as *retryable*. A caller doing what the code says would wait
    /// sixty-nine years. A mark is read from a store and a store can hand back a
    /// wrong number: this module counts 22 of 64 single-bit flips landing here.
    #[test]
    fn a_mark_further_ahead_than_any_skew_is_corrupt_rather_than_early() {
        let clock = FakeClock::at(EPOCH_MS + 100);
        // Just inside the plausible window: this really is a clock that was early,
        // and retrying really does fix it.
        let soon = EPOCH_MS + 100 + super::PLAUSIBLE_DRIFT_MS;
        assert!(matches!(
            Minter::resuming_at(flake(), &clock, soon),
            Err(MintError::ClockMovedBackwards { .. })
        ));
        // And one past it, which no clock explains.
        for (about, mark) in [
            ("a day and a millisecond ahead", soon + 1),
            ("the top of the layout", EPOCH_MS + TIMESTAMP_MASK),
        ] {
            let refused = Minter::resuming_at(flake(), &clock, mark)
                .err()
                .unwrap_or_else(|| panic!("{about} was accepted"));
            assert!(
                matches!(refused, MintError::MarkOutOfRange { .. }),
                "{about} came back as {refused:?}"
            );
            assert_eq!(
                refused.code(),
                nc_catalog::ErrorCode::ValidationOutOfRange,
                "{about} carries a code that says to wait and retry"
            );
        }
    }

    /// The sequence starts again in each millisecond.
    ///
    /// **The reset could be deleted with `cargo xtask check` green.** Without it
    /// `sequence()` counts up across milliseconds for the life of the process —
    /// `ms+0: 0 and 1`, `ms+1: 1 and 2`, `ms+2: 2 and 3` — so its documented
    /// meaning, *its position within its millisecond*, is false, and the module
    /// header's "4,096 identifiers per millisecond" becomes "4,096 per process,
    /// and then a spurious exhaustion". `an_exhausted_sequence_waits_for_the_next_millisecond`
    /// does assert a zero, but that zero is written by the wait rather than by
    /// this branch, and the property test steps 0..3 which is inside one wait.
    #[test]
    fn each_millisecond_starts_its_sequence_again() {
        let clock = FakeClock::at(EPOCH_MS + 1);
        let mut minter = Minter::new(flake(), &clock);
        for step in 0..4_u64 {
            clock.set(EPOCH_MS + 1 + step);
            let first = minter.mint().expect("the clock is inside the layout");
            let second = minter.mint().expect("the clock is inside the layout");
            assert_eq!(
                (first.sequence(), second.sequence()),
                (0, 1),
                "millisecond {step} did not start its sequence again"
            );
        }
    }

    /// A real skew is still a clock that moved, not a corrupt mark.
    ///
    /// The far side of the boundary above, and the reason the number cannot
    /// simply be made small: a machine that was briefly early has to be told to
    /// wait, and only a mark no clock explains is refused outright.
    #[test]
    fn a_skew_of_seconds_is_a_clock_and_not_a_corrupt_mark() {
        let clock = FakeClock::at(EPOCH_MS + 100_000);
        let refused = Minter::resuming_at(flake(), &clock, EPOCH_MS + 100_000 + 30_000)
            .expect_err("a mark ahead of the clock is refused");
        assert!(
            matches!(refused, MintError::ClockMovedBackwards { .. }),
            "a thirty-second skew came back as {refused:?}, and retrying really does \
             fix that one"
        );
    }

    #[test]
    /// A high-water mark this layout cannot hold is refused, not stored.
    ///
    /// **Every `u64` was accepted.** A mark below the epoch silently turns the
    /// protection off — the minter's barrier is already behind the clock, so a
    /// restart re-issues what the previous process handed out:
    ///
    /// ```text
    /// process A minted : [2097180672, 2097180673, 2097180674]
    /// process B minted : [2097180672, 2097180673, 2097180674]   literal duplicates
    /// ```
    ///
    /// and a mark above it bricks minting behind a `retryable` code: 1000
    /// retries at `u64::MAX` gave `ClockMovedBackwards { drift: 18446742306483951s }`
    /// every time, first clearing in about 584 942 361 years.
    fn a_high_water_mark_outside_the_layout_is_refused() {
        for (about, mark) in [
            ("zero", 0),
            ("one", 1),
            ("the millisecond before the epoch", EPOCH_MS - 1),
        ] {
            assert!(
                matches!(
                    Minter::resuming_at(flake(), SystemClock, mark),
                    Err(MintError::BeforeEpoch)
                ),
                "a mark of {about} was taken as a time this process wrote"
            );
        }
        assert!(
            matches!(
                Minter::resuming_at(flake(), SystemClock, u64::MAX),
                Err(MintError::BeyondRange)
            ),
            "a mark past the layout's range was taken as a time this process wrote"
        );

        // The floor: a mark this layout *can* hold is still accepted, or the
        // rule above would be a ban rather than a check.
        assert!(
            Minter::resuming_at(flake(), SystemClock, EPOCH_MS + 100).is_ok(),
            "a mark inside the layout was refused"
        );
    }

    /// The two clock codes say retry in the direction a retry works.
    ///
    /// **They carried it backwards, both of them.** A clock that is set wrong
    /// stays set wrong — 500 retries at 1999 and at 2200 gave `BeforeEpoch` and
    /// `BeyondRange` every time under `retryable = true` — while a stopped clock
    /// is the one condition the next tick clears, and it said do not.
    #[test]
    fn the_clock_codes_say_retry_where_retrying_works() {
        assert!(
            !MintError::BeyondRange.code().retryable(),
            "a clock set to the wrong century is not fixed by asking again"
        );
        assert!(
            !MintError::BeforeEpoch.code().retryable(),
            "a clock set to the wrong century is not fixed by asking again"
        );
        // **And a step back is, which the shared code could not say.** All three
        // carried `IdsClockBackwards`, `retryable = false`, so the ordinary NTP
        // step — measured clearing 601 ms later with the identical call
        // answering `Ok` — was reported as permanent under a sentence telling
        // the reader to wait for it to clear.
        assert!(
            MintError::ClockMovedBackwards {
                drift: Duration::from_millis(600),
            }
            .code()
            .retryable(),
            "a clock that stepped back is exactly what waiting fixes"
        );
        assert_ne!(
            MintError::ClockMovedBackwards {
                drift: Duration::from_millis(600),
            }
            .code(),
            MintError::BeyondRange.code(),
            "one code cannot say both wait and do not"
        );
        assert!(
            MintError::ClockStuck {
                at: Duration::from_millis(EPOCH_MS),
            }
            .code()
            .retryable(),
            "a stopped clock is exactly what a later attempt clears"
        );
    }

    /// A refusal names the millisecond the clock has to pass, not the one it is
    /// on.
    ///
    /// They are the same number only when the clock has not moved at all. After
    /// a partial step the message named a millisecond the clock had already
    /// passed: `ClockStuck { at: …017 }` while the value to beat was …020, and
    /// the sentence read "the clock did not move past 017" with the clock
    /// reading 017.
    #[test]
    fn a_stopped_clock_names_the_millisecond_it_has_to_pass() {
        // **A genuinely stopped clock**, which is what `ClockStuck` is for: it
        // reads the same millisecond for ever, and that millisecond is *below*
        // the barrier the mark set.
        // Two readings of the same millisecond: `resuming_at` consumes the
        // first, and every later read is the second — a clock that has stopped
        // exactly on the barrier, which is the case this refusal is for.
        let clock = StepThenStop {
            readings: std::sync::Mutex::new(vec![EPOCH_MS + 20, EPOCH_MS + 20]),
        };
        let mut minter =
            Minter::resuming_at(flake(), &clock, EPOCH_MS + 20).expect("the clock has reached it");
        let refused = minter.mint().expect_err("the clock does not move");
        let MintError::ClockStuck { at } = refused else {
            panic!("{refused:?}");
        };
        assert_eq!(
            at,
            Duration::from_millis(EPOCH_MS + 20),
            "the refusal named the reading rather than the barrier: {refused}"
        );
    }

    /// Both exits from the wait are concluded once.
    ///
    /// **The conclusion was kept for one of them.** A stopped clock costs the
    /// whole `WAIT_ATTEMPTS` loop once and one clock read afterwards; a clock
    /// that had moved backwards paid the loop again on every later call —
    /// measured, `now_ms()` called 1001 times per `mint`, four rounds running,
    /// against 1001 then 1 then 1 for the stopped case. With `SystemClock` each
    /// of those waits is up to ten thousand `yield_now()`, which is the 5.36 s
    /// per call the memo exists to stop.
    #[test]
    fn both_ways_out_of_the_wait_are_concluded_once() {
        for (about, readings) in [
            ("a stopped clock", vec![EPOCH_MS + 20, EPOCH_MS + 20]),
            (
                "a clock that moved backwards",
                vec![EPOCH_MS + 20, EPOCH_MS + 20, EPOCH_MS + 19],
            ),
        ] {
            let clock = CountingClock {
                readings: std::sync::Mutex::new(readings),
                reads: std::sync::atomic::AtomicUsize::new(0),
            };
            let mut minter = Minter::resuming_at(flake(), &clock, EPOCH_MS + 20)
                .expect("the clock has reached it");
            minter
                .mint()
                .expect_err("the clock does not pass the barrier");
            let first = clock.reads.load(core::sync::atomic::Ordering::Relaxed);
            clock.reads.store(0, core::sync::atomic::Ordering::Relaxed);
            minter.mint().expect_err("the clock still does not pass it");
            let again = clock.reads.load(core::sync::atomic::Ordering::Relaxed);
            assert!(
                first > 100,
                "{about}: the first refusal did not wait at all ({first} read(s))"
            );
            assert!(
                again <= 2,
                "{about}: the second refusal walked the whole wait again ({again} read(s)), \
                 so the conclusion was not kept"
            );
        }
    }

    /// A clock that dips and comes back is not a clock that stopped, twice over.
    ///
    /// **One memo carried two conclusions.** Both exits from the wait wrote
    /// `concluded` and the early return that reads it raised `ClockStuck`
    /// whatever had been concluded — so the *second* call about a clock that had
    /// moved backwards came out `critical`:
    ///
    /// ```text
    /// first  = ClockMovedBackwards { drift: 1ms }   after 1002 read(s)  NC-IDS-0001, error
    /// second = ClockStuck { at: … }                 after 1 read(s)     NC-IDS-0007, critical
    /// ```
    ///
    /// `NC-IDS-0007` says *"the clock has stopped, so no more identifiers can be
    /// minted on this machine"*. The fixtures beside this one all script a clock
    /// that stays down, so the memo they exercise is only ever the stopped one —
    /// which is why this needs a clock that returns.
    #[test]
    fn a_clock_that_dipped_stays_a_clock_that_dipped() {
        let clock = CountingClock {
            readings: std::sync::Mutex::new(vec![
                EPOCH_MS + 20,
                EPOCH_MS + 20,
                EPOCH_MS + 19,
                EPOCH_MS + 20,
            ]),
            reads: std::sync::atomic::AtomicUsize::new(0),
        };
        let mut minter =
            Minter::resuming_at(flake(), &clock, EPOCH_MS + 20).expect("the clock has reached it");
        let first = minter
            .mint()
            .expect_err("the clock does not pass the barrier");
        assert!(
            matches!(first, MintError::ClockMovedBackwards { .. }),
            "a clock that stepped back was diagnosed {first:?}"
        );
        let again = minter.mint().expect_err("it still does not pass it");
        assert_eq!(
            first, again,
            "the same fault was diagnosed two different ways on two calls — \
             `{first:?}` then `{again:?}`. One of them is `critical` and names a machine \
             that can mint nothing more; the other is an `error` about a clock that has \
             already moved, and a responder acts on them differently"
        );
    }

    /// A clock whose readings are a script, counting how often it is asked.
    struct CountingClock {
        readings: std::sync::Mutex<Vec<u64>>,
        reads: std::sync::atomic::AtomicUsize,
    }

    impl Clock for &CountingClock {
        fn now_ms(&self) -> u64 {
            self.reads
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            let mut readings = self.readings.lock().expect("no panic holds this");
            if readings.len() > 1 {
                readings.remove(0)
            } else {
                readings[0]
            }
        }

        fn wait_past(&self, _millis: u64) {}
    }

    /// A clock that *moves* during the wait is not a clock that stopped.
    ///
    /// **The wait's exit condition is `now <= last_millis`, and a backwards step
    /// satisfies it too.** Measured, a clock going from …605000 to …604000
    /// during the wait: `ClockStuck { at: 1767225605s }`, `NC-IDS-0007`,
    /// **critical**, *"The clock has stopped, so no more identifiers can be
    /// minted on this machine"* — for a clock that had moved a full second. And
    /// `NC-IDS-0007` is `retryable`, so a caller keyed on it retries a machine
    /// fault that is not the one it is told about; the test that used to stand
    /// here asserted the wrong answer as correct, with a fixture that stepped
    /// backwards.
    #[test]
    fn a_clock_that_moves_backwards_during_the_wait_says_so() {
        // **Inside the wait, which the first version of this never entered.**
        // Its fake was consumed by `resuming_at` and the first `mint` read
        // tripped the entry guard, so the branch it is named for had no test at
        // all — and that branch looked only at the *last* sample, so a clock a
        // full second in the past for 999 of 1000 samples came out
        // `ClockStuck`, **critical**, *"the clock has stopped"*.
        //
        // This clock reads the barrier, spends the whole wait a second behind
        // it, and returns to the barrier on the last sample: `now == barrier`
        // on entry, so the sequence-exhausted path is taken and the loop runs.
        let clock = LateThenBack {
            at: std::sync::Mutex::new(0),
        };
        let mut minter = Minter::resuming_at(flake(), &clock, EPOCH_MS + 5_000)
            .expect("the clock has reached it");
        let refused = minter.mint().expect_err("the clock went backwards");
        let MintError::ClockMovedBackwards { drift } = refused else {
            panic!("a clock that moved a full second was called stopped: {refused:?}");
        };
        assert_eq!(drift, Duration::from_secs(1));
        assert!(
            refused.code().retryable(),
            "a clock that stepped back is what waiting fixes"
        );
    }

    /// A clock at the barrier, then a second behind it, then back at the
    /// barrier: it moves, and it never passes.
    struct LateThenBack {
        at: std::sync::Mutex<usize>,
    }

    impl Clock for &LateThenBack {
        fn now_ms(&self) -> u64 {
            let mut at = self.at.lock().expect("no panic holds this");
            *at += 1;
            // The first two readings are `resuming_at`'s and `mint`'s entry
            // read; everything after is the wait, and the last of those is back
            // at the barrier so only the *lowest* sample says which way it went.
            if *at > 2 && *at <= 2 + usize::try_from(super::WAIT_ATTEMPTS).unwrap_or(usize::MAX) {
                EPOCH_MS + 4_000
            } else {
                EPOCH_MS + 5_000
            }
        }

        fn wait_past(&self, _millis: u64) {}
    }

    /// A clock that reads the first value once and the second for ever.
    struct StepThenStop {
        readings: std::sync::Mutex<Vec<u64>>,
    }

    impl Clock for &StepThenStop {
        fn now_ms(&self) -> u64 {
            let mut readings = self.readings.lock().expect("no panic holds this");
            if readings.len() > 1 {
                readings.remove(0)
            } else {
                readings[0]
            }
        }

        fn wait_past(&self, _millis: u64) {}
    }
}

#[cfg(test)]
mod the_numbers_are_written_down {
    use super::{EPOCH_MS, PLAUSIBLE_DRIFT_MS, TIMESTAMP_MASK, WAIT_ATTEMPTS};

    /// The declared bounds are the ones their own paragraphs argue for.
    ///
    /// **`PLAUSIBLE_DRIFT_MS` was pinned by nothing.** Its only test builds its
    /// boundary *from the constant under test* — `soon = now + PLAUSIBLE_DRIFT_MS`
    /// — so every value above about fifty milliseconds satisfies it. Raised to
    /// sixty-three years, `cargo xtask check` passed and a mark a *year* ahead of
    /// the clock came back as `ClockMovedBackwards { drift: 31536000s }` under
    /// `IdsClockBackwards`, which the suite certifies as retryable: the exact
    /// failure the constant was added to stop, told to a caller as "wait".
    ///
    /// `WAIT_ATTEMPTS` and `EPOCH_MS` are written out for the same reason and
    /// were already; this is the one that was missed.
    #[test]
    fn every_declared_bound_is_the_number_its_paragraph_argues_for() {
        assert_eq!(
            PLAUSIBLE_DRIFT_MS,
            24 * 60 * 60 * 1_000,
            "a day. Real skew is seconds — an NTP step, a virtual machine resuming, a \
             clock read before the network is up — and what this has to separate is \
             `wait a moment` from `sixty-nine years`"
        );
        assert_eq!(WAIT_ATTEMPTS, 1_000);
        // The epoch, which every timestamp in the platform is relative to.
        assert_eq!(EPOCH_MS, 1_767_225_600_000);
        // 41 bits, which is what makes the layout 63 and the sign bit clear.
        assert_eq!(TIMESTAMP_MASK, (1 << 41) - 1);
    }
}
