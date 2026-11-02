//! Work units: the group of entries that is one step to undo.
//!
//! `NOCTUA.md §11.5`. A unit is bracketed by two entries of its own — a
//! [`Outcome::UnitBegun`] carrying its label, and a [`Outcome::UnitCommitted`]
//! or [`Outcome::UnitAborted`] closing it — and every entry between them carries
//! the unit's identifier.
//!
//! # Why the entries are written as they happen
//!
//! An earlier version buffered them and wrote them on commit, so dropping the
//! handle wrote nothing at all. That is faithful to "*dropping the handle
//! without `commit` aborts*" read narrowly, and it is wrong: it also erased the
//! **denials**. `§15.5` requires every authorization denial to be recorded, "not
//! optional and not sampled", and `§14.9` splits undo from audit precisely so
//! that one cannot reach into the other. An agent could have wrapped hostile
//! dispatches in a unit, dropped the handle, and left nothing behind.
//!
//! So: entries are appended immediately, and the unit's *outcome* is what the
//! handle decides. Aborting is a fact about what happened, recorded like any
//! other. Undo is verified against the journal ([`Store::unit_closing`]), and
//! what it reverses is read from the local store's units
//! (`crates/nc-kernel/src/history.rs`); audit reads everything and sees that
//! it was reversed.
//!
//! **A handle dropped without either is visible by its absence.** There is no
//! closing entry, so a reader — and a later recovery pass — sees a unit that
//! opened and never closed, which is exactly what a process that crashed
//! mid-group looks like. Nothing is written from `Drop`: a store write in a
//! destructor is a failure nobody can handle.
//!
//! # Two doors, and both are the same two calls
//!
//! [`Journal::open_unit`] mints an identifier and writes the opening bracket;
//! [`Journal::close_unit`] writes the closing one, [`Closing::Committed`] or
//! [`Closing::Aborted`], and refuses an identifier this realm never opened.
//! They name a unit by its identifier, so a unit can outlive any one borrow of
//! the journal — which is what a caller needs that opens a unit before a
//! command runs and closes it only after the command's effects have landed.
//! [`WorkUnit`] is those two calls behind a handle: it borrows the journal while
//! it is open, so nothing else can write, and it stamps its identifier on every
//! record it is handed.
//!
//! # What this crate does not provide
//!
//! `§11.5` calls a unit **ambient** — every command dispatched while it is open
//! joins it, including commands landing in other plugins — and says nested
//! groups flatten, and that **dropping the handle aborts**. **None of the three
//! is here: the kernel provides them**, in `crates/nc-kernel/src/units.rs`,
//! over the two calls above (`NOCTUA.md §11.5`). Ambience needs a dispatch
//! context to hang off, and flattening needs to know which unit is already
//! open in it; neither is a journal's to know. A dropped [`WorkUnit`] leaves
//! its unit *open* rather than aborted: the entries inside it stand, and the
//! missing closing entry is what says the group never completed. Turning that
//! into an abort means writing from a destructor, where a failed write is a
//! failure nobody can handle — so closing an abandoned unit belongs to the
//! kernel's recovery pass, which closes it through [`Journal::close_unit`] like
//! any other. `§11.5`'s own closing line is **never widen the claim**.
//!
//! The scope claim is `§11.5`'s and is not widened either: a unit covers what
//! the kernel can reverse — `store` writes, blobs created inside it — and not
//! effects that already left the machine. Those take a compensating command,
//! which is the kernel's to dispatch and is not represented here.

/// The action the entry that opens a work unit carries.
///
/// Written down and asserted rather than typed in place: this and [`CLOSES`]
/// were two string literals a few lines apart, and swapping them changed which
/// entry a query for "when did this unit start" would find, with the whole
/// suite green.
pub const OPENS: &str = "kernel.begin";

/// The action the entry that closes a work unit carries.
pub const CLOSES: &str = "kernel.unit";

use nc_ids::{Clock, Snowflake, SystemClock};

use crate::{Entry, Error, Journal, Outcome, Record, Store};

/// How a unit ends, for [`Journal::close_unit`].
///
/// **Two of [`Outcome`]'s variants and no other.** An `Outcome` parameter
/// would take `Allowed` as a way to close a unit, and the bracket it wrote
/// would close nothing: every store reads a unit as closed by a [`CLOSES`]
/// entry that committed or aborted, and by nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Closing {
    /// The group completed. Everything between its brackets stands.
    Committed,
    /// The group did not complete. Its entries stand as the record of what was
    /// tried — a denial among them — and this is what says it was abandoned.
    Aborted,
}

impl Closing {
    /// The outcome the closing bracket carries.
    pub(crate) const fn outcome(self) -> Outcome {
        match self {
            Self::Committed => Outcome::UnitCommitted,
            Self::Aborted => Outcome::UnitAborted,
        }
    }
}

/// An open group of entries.
#[derive(Debug)]
pub struct WorkUnit<'a, S: Store, C: Clock = SystemClock> {
    journal: &'a mut Journal<S, C>,
    id: Snowflake,
    label: String,
    closed: bool,
}

impl<'a, S: Store, C: Clock> WorkUnit<'a, S, C> {
    /// A handle over a unit [`Journal::open_unit`] has already opened.
    ///
    /// **It writes nothing.** The opening bracket is `open_unit`'s, so the
    /// handle and the identifier door cannot write two different openings;
    /// what this holds is the borrow, the identifier and the label a person
    /// sees in the history (`§11.5`).
    pub(crate) const fn over(journal: &'a mut Journal<S, C>, id: Snowflake, label: String) -> Self {
        Self {
            journal,
            id,
            label,
            closed: false,
        }
    }

    /// The unit's identifier, which every entry inside it carries.
    #[must_use]
    pub const fn id(&self) -> Snowflake {
        self.id
    }

    /// The text a person sees in the history: "Import album", "Personal agent:
    /// organize photos".
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Records something inside the group, immediately.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnitClosed`] when the unit has already committed or
    /// aborted, and whatever the store returned otherwise.
    pub async fn record(&mut self, mut record: Record) -> Result<Entry, Error> {
        if self.closed {
            return Err(Error::UnitClosed { unit: self.id });
        }
        record.work_unit = Some(self.id);
        self.journal.append(record).await
    }

    /// Closes the group as committed.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnitClosed`] when it is already closed,
    /// [`Error::ClockBeforeEpoch`] when this machine has no instant to record —
    /// `Journal::bracket` asks the clock, and closing a unit goes through it —
    /// and whatever [`Journal::close_unit`] returned otherwise. Takes `&mut self` rather than `self` so that
    /// the closed state is reachable — with a consuming `commit`, the branch
    /// above could never be taken and its error code was unreachable.
    pub async fn commit(&mut self) -> Result<Entry, Error> {
        self.close(Closing::Committed).await
    }

    /// Closes the group as abandoned.
    ///
    /// The entries inside it stand — they are true, and they include any denial
    /// — and this is what says the group did not complete.
    ///
    /// # Errors
    ///
    /// As [`WorkUnit::commit`].
    pub async fn abort(&mut self) -> Result<Entry, Error> {
        self.close(Closing::Aborted).await
    }

    /// Whether the unit has been closed either way.
    #[must_use]
    pub const fn is_closed(&self) -> bool {
        self.closed
    }

    async fn close(&mut self, how: Closing) -> Result<Entry, Error> {
        if self.closed {
            return Err(Error::UnitClosed { unit: self.id });
        }
        let entry = self.journal.close_unit(self.id, how).await?;
        self.closed = true;
        Ok(entry)
    }
}

#[cfg(test)]
mod tests {
    use crate::{Entry, Error, InMemory, Journal, Outcome, Read, Record, Store};
    use core::future::Future;
    use nc_canon::Value;
    use nc_ids::{OpaqueId, Snowflake};

    /// A store that refuses one nominated `append` retryably, then behaves.
    ///
    /// Every other method is the store beneath it: what this changes is one
    /// answer, at one moment, which is the only thing the test below is about.
    /// The moment is named by ordinal rather than set on the way past, because a
    /// live `WorkUnit` borrows the journal that owns the store.
    struct BlinksOnce {
        beneath: InMemory,
        seen: usize,
        blink_at: usize,
    }

    impl Store for BlinksOnce {
        fn tail(&self) -> impl Future<Output = Result<Option<Entry>, Error>> + Send {
            self.beneath.tail()
        }

        fn head(&self) -> impl Future<Output = Result<Option<(u64, [u8; 32])>, Error>> + Send {
            self.beneath.head()
        }

        fn append(&mut self, entry: Entry) -> impl Future<Output = Result<(), Error>> + Send {
            let blink = self.seen == self.blink_at;
            self.seen += 1;
            async move {
                if blink {
                    return Err(Error::Unavailable(
                        "the store blinked, and this is the retryable one".to_owned(),
                    ));
                }
                self.beneath.append(entry).await
            }
        }

        fn from(&self, sequence: u64) -> impl Future<Output = Result<Read, Error>> + Send {
            self.beneath.from(sequence)
        }

        fn unit_opened(
            &self,
            realm: &OpaqueId,
            unit: Snowflake,
        ) -> impl Future<Output = Result<bool, Error>> + Send {
            self.beneath.unit_opened(realm, unit)
        }

        fn unit_closing(
            &self,
            realm: &OpaqueId,
            unit: Snowflake,
        ) -> impl Future<Output = Result<Option<Outcome>, Error>> + Send {
            self.beneath.unit_closing(realm, unit)
        }
    }

    /// A store that takes whatever it is handed.
    ///
    /// **What tells the unit's own refusal from the store's.** `record` refuses
    /// after the group is closed, and `InMemory` refuses an entry that arrives
    /// after the closing bracket — two guards answering the same error on the
    /// same input, so the unit's could be deleted with every test in this crate
    /// still green. Measured on 2026-09-13, after a reviewer found that the file
    /// carries no mutation operator at all. This store answers `Ok` to any
    /// append, which leaves the unit's own guard as the only thing that can
    /// refuse.
    ///
    /// **It keeps what it can, and it never says a unit is closed.** A close
    /// goes through `Journal::close_unit`, which asks whether the unit was ever
    /// opened — so the opening bracket has to be somewhere a store can find it,
    /// and `beneath` is where. Whatever `beneath` refuses is still answered
    /// `Ok`, and `unit_closing` answers `None`, so neither the store nor the
    /// journal's own closed-unit question is what refuses a record after the
    /// commit.
    struct TakesAnything {
        beneath: InMemory,
    }

    impl Store for TakesAnything {
        fn tail(&self) -> impl Future<Output = Result<Option<Entry>, Error>> + Send {
            self.beneath.tail()
        }

        fn head(&self) -> impl Future<Output = Result<Option<(u64, [u8; 32])>, Error>> + Send {
            self.beneath.head()
        }

        async fn append(&mut self, entry: Entry) -> Result<(), Error> {
            // Refused or not, the answer is `Ok`: that is what this store is
            // for.
            if let Err(refused) = self.beneath.append(entry).await {
                drop(refused);
            }
            Ok(())
        }

        fn from(&self, sequence: u64) -> impl Future<Output = Result<Read, Error>> + Send {
            self.beneath.from(sequence)
        }

        fn unit_opened(
            &self,
            realm: &OpaqueId,
            unit: Snowflake,
        ) -> impl Future<Output = Result<bool, Error>> + Send {
            self.beneath.unit_opened(realm, unit)
        }

        fn unit_closing(
            &self,
            _realm: &OpaqueId,
            _unit: Snowflake,
        ) -> impl Future<Output = Result<Option<Outcome>, Error>> + Send {
            core::future::ready(Ok(None))
        }
    }

    /// A store that cannot say whether a unit opened or closed while `silent`
    /// is set, and is the store beneath it otherwise.
    ///
    /// **What tells a store that could not answer from a unit nobody opened.**
    /// `InMemory` always answers, so nothing offline decided whether
    /// `close_unit` hands the store's refusal back or reads it as "never
    /// opened"; the PostgreSQL twin in `in_postgres.rs` decides it, and
    /// `cargo xtask check --offline` does not run that suite. The refusal this
    /// gives is the retryable one, which is the half a misreading turns into
    /// its opposite.
    struct CannotSay {
        beneath: InMemory,
        silent: bool,
    }

    /// What [`CannotSay`] answers while silent.
    fn silence() -> Error {
        Error::Unavailable("the store could not say, and this is the retryable one".to_owned())
    }

    impl Store for CannotSay {
        fn tail(&self) -> impl Future<Output = Result<Option<Entry>, Error>> + Send {
            self.beneath.tail()
        }

        fn head(&self) -> impl Future<Output = Result<Option<(u64, [u8; 32])>, Error>> + Send {
            self.beneath.head()
        }

        fn append(&mut self, entry: Entry) -> impl Future<Output = Result<(), Error>> + Send {
            self.beneath.append(entry)
        }

        fn from(&self, sequence: u64) -> impl Future<Output = Result<Read, Error>> + Send {
            self.beneath.from(sequence)
        }

        async fn unit_opened(&self, realm: &OpaqueId, unit: Snowflake) -> Result<bool, Error> {
            if self.silent {
                return Err(silence());
            }
            self.beneath.unit_opened(realm, unit).await
        }

        async fn unit_closing(
            &self,
            realm: &OpaqueId,
            unit: Snowflake,
        ) -> Result<Option<Outcome>, Error> {
            if self.silent {
                return Err(silence());
            }
            self.beneath.unit_closing(realm, unit).await
        }
    }

    /// The unit refuses after it is closed, whatever the store would have done.
    ///
    /// The test beside this one proves the refusal against `InMemory`, which
    /// refuses the same entry itself — so it proves the pair and not the unit.
    #[tokio::test]
    async fn a_closed_unit_refuses_a_record_the_store_would_have_taken() {
        let mut journal = Journal::new(
            TakesAnything {
                beneath: InMemory::default(),
            },
            id("realm-1"),
            id("kernel"),
            minter(),
        );
        let mut unit = journal.begin("Closed").await.expect("opens");
        unit.commit().await.expect("commits");
        let unit_id = unit.id();
        assert_eq!(
            unit.record(record(1)).await,
            Err(Error::UnitClosed { unit: unit_id }),
            "the store takes anything, so this refusal is the unit's own"
        );
    }

    /// The unit refuses a second close, whatever the store would have done.
    ///
    /// **`close`'s own guard was decided by nothing.** Against a store that
    /// conforms, a second close is refused anyway — `close_unit` writes through
    /// `append`, which answers `UnitClosed` — so `if self.closed` could be
    /// `if false` with every suite green, measured on 2026-09-28. Only a store
    /// that answers otherwise tells the two apart, and under that plant this
    /// one landed a second `kernel.unit` bracket for a unit that had already
    /// committed. The twin of the test above, for the other door out.
    #[tokio::test]
    async fn a_closed_unit_refuses_a_second_close_the_store_would_have_taken() {
        let mut journal = Journal::new(
            TakesAnything {
                beneath: InMemory::default(),
            },
            id("realm-1"),
            id("kernel"),
            minter(),
        );
        let mut unit = journal.begin("Closed").await.expect("opens");
        unit.commit().await.expect("commits");
        let unit_id = unit.id();
        assert_eq!(
            unit.abort().await,
            Err(Error::UnitClosed { unit: unit_id }),
            "the store takes anything, so this refusal is the unit's own"
        );
    }

    #[tokio::test]
    async fn the_bracket_entries_carry_the_actions_a_query_looks_for_and_they_differ() {
        // **Two literals a few lines apart, and nothing named either.** Swapping
        // them changed which entry a query for "when did this unit start" would
        // find, and the whole suite stayed green — so did flattening both to one
        // string, which would make an opening entry and a closing one
        // indistinguishable in the history `§11.5` is about.
        let mut journal = Journal::new(InMemory::default(), id("realm"), id("kernel"), minter());
        let mut unit = journal.begin("a label".to_owned()).await.expect("begins");
        unit.commit().await.expect("commits");

        let entries = journal.store().all();
        assert_eq!(entries.len(), 2, "{entries:#?}");
        // **Written out, because the constants restated prove nothing about
        // them.** These two assertions read `super::OPENS` and `super::CLOSES`,
        // so renaming both to `"zzz.a"` and `"zzz.b"` left the whole workspace
        // green. They are not internal names: they are the literal `action`
        // value written into the audit table and hashed into every bracket
        // entry, so after a rename `unit_opened` and `unit_closed` stop matching
        // the brackets already in the table — and a unit that was closed
        // silently accepts new entries, which is the one thing the closed-unit
        // rule exists to prevent. There is no migration for a hashed column.
        assert_eq!(super::OPENS, "kernel.begin");
        assert_eq!(super::CLOSES, "kernel.unit");
        assert_eq!(entries[0].record.action, "kernel.begin");
        assert_eq!(entries[1].record.action, "kernel.unit");
        assert_ne!(
            super::OPENS,
            super::CLOSES,
            "one action for both brackets makes an opening entry and a closing one \
             indistinguishable"
        );
        // And the label reaches both, since it is what a person reads.
        for entry in entries {
            assert_eq!(entry.record.resource, Value::text("a label"));
            assert!(entry.record.work_unit.is_some());
        }
    }

    fn id(text: &str) -> OpaqueId {
        OpaqueId::new(text).expect("a test identifier")
    }

    fn record(at: u64) -> Record {
        Record::new(
            id("user-1"),
            "Write",
            Value::Integer(1),
            Outcome::Allowed,
            at,
        )
    }

    /// A minter for a test, with a flake of its own.
    ///
    /// One per `Journal`, because two journals sharing a flake mint the same
    /// work-unit identifiers — the defect this signature now makes visible.
    fn minter() -> nc_ids::Minter<nc_ids::SystemClock> {
        use core::sync::atomic::{AtomicU16, Ordering};
        static NEXT: AtomicU16 = AtomicU16::new(1);
        let flake = NEXT.fetch_add(1, Ordering::Relaxed) % nc_ids::flake::RANGE;
        nc_ids::Minter::new(
            nc_ids::MachineFlake::from_value(flake).unwrap_or_else(|_| unreachable!("masked")),
            nc_ids::SystemClock,
        )
    }

    fn journal() -> Journal<InMemory> {
        Journal::new(InMemory::default(), id("realm-1"), id("kernel"), minter())
    }

    /// A close the store refused leaves the unit open.
    ///
    /// **`self.closed = true` sat one line from the append, and swapping the
    /// two left the whole gate green.** With the flag set first, a store that
    /// answers `Unavailable` — the *retryable* one, whose sentence tells the
    /// caller to try again — closes the unit anyway: `is_closed()` answers
    /// `true`, `commit` and `abort` both answer `UnitClosed` from then on, and
    /// the handle can never be closed. No bracket is ever written, so the group
    /// stays open in the table for ever, and `unit_opened` goes on refusing a
    /// second opener for a unit that has no closing bracket and never will.
    ///
    /// Nothing here had a store that could fail, which is why the order was
    /// free. This one blinks once.
    #[tokio::test]
    async fn a_close_the_store_refused_leaves_the_unit_open() {
        let mut journal = Journal::new(
            BlinksOnce {
                beneath: InMemory::default(),
                seen: 0,
                // The third append: the opening bracket, one entry inside the
                // group, and then the close.
                blink_at: 2,
            },
            id("realm-1"),
            id("kernel"),
            minter(),
        );
        let mut unit = journal.begin("Import album").await.expect("opens");
        // **`label` had no caller that read its value.** It is what a person
        // sees in the history, and a systematic scan found it answering nothing
        // anybody checked: the accessor could hand back the unit's identifier,
        // or the empty string, with the whole suite green. `clippy` catches a
        // literal here and nothing catches a wrong field.
        assert_eq!(
            unit.label(),
            "Import album",
            "the handle answers with something other than the label it was opened with"
        );
        unit.record(record(1)).await.expect("open");

        // The next append is the closing bracket, and the store refuses it.
        let refused = unit.commit().await.expect_err("the store blinked");
        assert!(
            matches!(refused, Error::Unavailable(_)),
            "the caller was told something other than what the store said: {refused:?}"
        );
        assert!(
            !unit.is_closed(),
            "a close the store refused marked the unit closed, so the caller the \
             retryable sentence tells to try again can never close it"
        );

        // And trying again works, which is what `retryable` means.
        unit.commit().await.expect("the second attempt closes it");
        assert!(
            unit.is_closed(),
            "the close that landed did not mark it closed"
        );
    }

    #[tokio::test]
    async fn a_closed_unit_is_closed_for_every_way_in() {
        // **`WorkUnit::record` refused a closed unit and `Journal::record` did
        // not.** A caller setting `record.work_unit` by hand landed an entry
        // *after* its own `UnitCommitted` bracket, inside the group to every SQL
        // reader, with `verify` answering clean — the shape `postgres.rs`'s own
        // header names: a rule enforced only where it is convenient to enforce
        // it is a rule about one code path.
        let mut journal = journal();
        let closed = {
            let mut unit = journal.begin("Import album").await.expect("opens");
            unit.record(record(1)).await.expect("open");
            let id = unit.id();
            // **`is_closed` before and after, because nothing called it.** A
            // systematic scan found it the only public function in these crates
            // with no caller anywhere — so `self.closed = true` could be `false`
            // and the whole suite stayed green, leaving an accessor that always
            // answers "still open" for a unit that is not.
            assert!(!unit.is_closed(), "a unit says it is closed before it is");
            unit.commit().await.expect("commits");
            assert!(
                unit.is_closed(),
                "a committed unit still says it is open, and `is_closed` is what a caller \
                 asks before deciding whether to close it again"
            );
            id
        };

        // The same unit, through the other door.
        let mut smuggled = record(2);
        smuggled.work_unit = Some(closed);
        let refused = journal.record(smuggled).await;
        assert!(
            matches!(refused, Err(Error::UnitClosed { unit }) if unit == closed),
            "a committed unit accepted a later entry: {refused:?}"
        );

        // And a unit that is still open takes one, or the refusal above would be
        // a refusal of everything. A handle dropped without closing leaves the
        // unit open, which the test below this one is about.
        let live = {
            let unit = journal.begin("Still going").await.expect("opens");
            unit.id()
        };
        let mut ordinary = record(3);
        ordinary.work_unit = Some(live);
        journal
            .record(ordinary)
            .await
            .expect("an open unit takes an entry");
    }

    #[tokio::test]
    async fn a_committed_unit_brackets_its_entries_and_carries_its_label() {
        let mut journal = journal();
        let unit_id = {
            let mut unit = journal.begin("Import album").await.expect("opens");
            unit.record(record(1)).await.expect("open");
            unit.record(record(2)).await.expect("open");
            unit.commit().await.expect("commits");
            unit.id()
        };
        let entries = journal.store().all().to_vec();
        assert_eq!(entries.len(), 4, "opening, two records, closing");
        assert!(entries.iter().all(|e| e.record.work_unit == Some(unit_id)));
        assert_eq!(entries[0].record.result, Outcome::UnitBegun);
        assert_eq!(entries[0].record.resource, Value::text("Import album"));
        assert_eq!(entries[3].record.result, Outcome::UnitCommitted);
    }

    #[tokio::test]
    async fn an_aborted_unit_keeps_every_entry_including_a_denial() {
        // The whole reason entries are not buffered. §15.5 requires every
        // authorization denial to be recorded, and an aborted group used to
        // erase them — which is what an agent wrapping hostile dispatches in a
        // unit would have relied on.
        let mut journal = journal();
        {
            let mut unit = journal.begin("Tried something").await.expect("opens");
            let mut denied = record(1);
            denied.result = Outcome::Denied;
            unit.record(denied).await.expect("open");
            unit.abort().await.expect("aborts");
        }
        let entries = journal.store().all();
        assert_eq!(entries.len(), 3);
        assert!(entries.iter().any(|e| e.record.result == Outcome::Denied));
        assert_eq!(entries[2].record.result, Outcome::UnitAborted);
    }

    #[tokio::test]
    async fn a_handle_dropped_without_closing_leaves_an_open_unit_rather_than_nothing() {
        // What a process that crashed mid-group looks like, and it looks like
        // itself: an opening entry with no closing one.
        let mut journal = journal();
        {
            let mut unit = journal.begin("Interrupted").await.expect("opens");
            unit.record(record(1)).await.expect("open");
        }
        let entries = journal.store().all();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].record.result, Outcome::UnitBegun);
        assert!(
            !entries.iter().any(|e| matches!(
                e.record.result,
                Outcome::UnitCommitted | Outcome::UnitAborted
            )),
            "an abandoned unit must not look closed"
        );
    }

    #[tokio::test]
    async fn recording_into_a_closed_unit_is_refused_by_name() {
        // Reachable, which it was not: `commit` used to consume the handle, so
        // the closed branch could never be taken and its error code was dead.
        let mut journal = journal();
        let mut unit = journal.begin("Closed").await.expect("opens");
        unit.commit().await.expect("commits");
        let unit_id = unit.id();
        assert_eq!(
            unit.record(record(1)).await,
            Err(Error::UnitClosed { unit: unit_id })
        );
        assert_eq!(
            unit.commit().await,
            Err(Error::UnitClosed { unit: unit_id })
        );
        assert_eq!(
            Error::UnitClosed { unit: unit_id }.code(),
            nc_catalog::ErrorCode::JournalUnitClosed
        );
    }

    #[tokio::test]
    async fn two_units_get_different_identifiers() {
        let mut journal = journal();
        let first = {
            let mut unit = journal.begin("one").await.expect("opens");
            unit.commit().await.expect("commits");
            unit.id()
        };
        let second = {
            let mut unit = journal.begin("two").await.expect("opens");
            unit.commit().await.expect("commits");
            unit.id()
        };
        assert_ne!(first, second);
    }

    #[tokio::test]
    async fn entries_from_a_unit_still_chain_to_everything_before_them() {
        let mut journal = journal();
        journal.record(record(0)).await.expect("records");
        let mut unit = journal.begin("After").await.expect("opens");
        unit.record(record(1)).await.expect("open");
        unit.commit().await.expect("commits");
        assert_eq!(journal.verify(0).await.expect("readable"), Ok(4));
    }

    /// An opening bracket written by hand, as another writer would write one.
    ///
    /// Through `Journal::record`, so it is chained and realm-stamped like any
    /// entry — and not through `open_unit`, so this journal holds no label for
    /// it.
    async fn opened_by_hand(journal: &mut Journal<InMemory>, unit: Snowflake) {
        let mut opening = record(9);
        opening.action = super::OPENS.to_owned();
        opening.result = Outcome::UnitBegun;
        opening.resource = Value::text("written by hand");
        opening.work_unit = Some(unit);
        journal.record(opening).await.expect("an opening lands");
    }

    /// A unit opened by its identifier closes by it, either way, and the
    /// store then says which way.
    #[tokio::test]
    async fn a_unit_opened_by_identifier_closes_by_it_and_the_store_says_which_way() {
        for (how, outcome) in [
            (super::Closing::Committed, Outcome::UnitCommitted),
            (super::Closing::Aborted, Outcome::UnitAborted),
        ] {
            let mut journal = journal();
            let unit = journal.open_unit("Import album").await.expect("opens");
            let mut inside = record(1);
            inside.work_unit = Some(unit);
            journal
                .record(inside)
                .await
                .expect("an open unit takes an entry");
            let closing = journal.close_unit(unit, how).await.expect("closes");

            assert_eq!(closing.record.action, super::CLOSES);
            assert_eq!(
                closing.record.result, outcome,
                "`{how:?}` closed as another way"
            );
            assert_eq!(closing.record.work_unit, Some(unit));
            assert_eq!(
                closing.record.resource,
                Value::text("Import album"),
                "the closing bracket does not repeat the label its opening carried"
            );
            let entries = journal.store().all();
            assert_eq!(
                entries.len(),
                3,
                "opening, the entry, closing: {entries:#?}"
            );
            assert_eq!(entries[0].record.action, super::OPENS);
            assert_eq!(entries[0].record.result, Outcome::UnitBegun);
            assert_eq!(entries[0].record.resource, Value::text("Import album"));
            assert_eq!(
                journal.store().unit_closing(&id("realm-1"), unit).await,
                Ok(Some(outcome)),
                "the store does not say how the unit closed"
            );
            assert!(
                journal.labels.is_empty(),
                "a closed unit's label is still held: {:?}",
                journal.labels
            );
            assert_eq!(journal.verify(0).await.expect("readable"), Ok(3));
        }
    }

    /// A unit this realm never opened is refused by name, and nothing is
    /// written.
    ///
    /// Three ways to have never opened one: an identifier nobody minted, one
    /// opened in another realm, and one whose only bracket is a closing one
    /// somebody wrote by hand. The third is refused as never opened rather than
    /// as closed: the question is asked before the closed one, and a unit with
    /// no opening is the more specific thing wrong with it.
    #[tokio::test]
    async fn closing_a_unit_this_realm_never_opened_is_refused_by_name_and_writes_nothing() {
        let never = Snowflake::from_crockford("01J8XK2M9P61").expect("a test unit");
        let elsewhere = Snowflake::from_crockford("01J8XK2M9P62").expect("a test unit");
        let closed_only = Snowflake::from_crockford("01J8XK2M9P63").expect("a test unit");
        let mut journal = journal();
        // One unit that *is* open, so a refusal cannot be a refusal of all.
        let open = journal.open_unit("Still going").await.expect("opens");

        let mut foreign = record(2);
        foreign.action = super::OPENS.to_owned();
        foreign.result = Outcome::UnitBegun;
        foreign.work_unit = Some(elsewhere);
        foreign.realm = id("another-realm");
        let tail = journal
            .store()
            .tail()
            .await
            .expect("readable")
            .expect("a tail");
        journal
            .store_mut()
            .append(Entry::chain(tail.sequence + 1, Some(tail.hash), foreign))
            .await
            .expect("another realm's opening lands in the shared store");

        let mut closing_only = record(3);
        closing_only.action = super::CLOSES.to_owned();
        closing_only.result = Outcome::UnitCommitted;
        closing_only.work_unit = Some(closed_only);
        journal
            .record(closing_only)
            .await
            .expect("a closing bracket with no opening lands through `record`");

        for unit in [never, elsewhere, closed_only] {
            let before = journal.store().len();
            for how in [super::Closing::Committed, super::Closing::Aborted] {
                let refused = journal.close_unit(unit, how).await;
                assert_eq!(
                    refused,
                    Err(Error::UnitNeverOpened { unit }),
                    "{unit} was never opened in this realm and `{how:?}` did not say so"
                );
            }
            assert_eq!(
                journal.store().len(),
                before,
                "a refused close of {unit} wrote something anyway"
            );
        }

        let refusal = Error::UnitNeverOpened { unit: never };
        assert_eq!(
            refusal.code(),
            nc_catalog::ErrorCode::JournalUnitNeverOpened
        );
        assert!(
            !refusal.code().retryable(),
            "a unit nobody opened is not opened by trying again"
        );
        // And the open one still closes, or the refusals above were of everything.
        journal
            .close_unit(open, super::Closing::Committed)
            .await
            .expect("a unit this realm opened closes");
    }

    /// A unit closed either way cannot be closed again, either way.
    #[tokio::test]
    async fn closing_a_closed_unit_is_refused_as_closed_whichever_way_it_closed() {
        for first in [super::Closing::Committed, super::Closing::Aborted] {
            for second in [super::Closing::Committed, super::Closing::Aborted] {
                let mut journal = journal();
                let unit = journal.open_unit("Once").await.expect("opens");
                journal.close_unit(unit, first).await.expect("closes once");
                let before = journal.store().len();
                assert_eq!(
                    journal.close_unit(unit, second).await,
                    Err(Error::UnitClosed { unit }),
                    "closed by `{first:?}`, closed again by `{second:?}`"
                );
                assert_eq!(journal.store().len(), before, "the second close landed");
                assert_eq!(
                    journal.store().unit_closing(&id("realm-1"), unit).await,
                    Ok(Some(first.outcome())),
                    "the refused close changed how the store says the unit closed"
                );
            }
        }
    }

    /// A close repeats a label only where this journal wrote the opening.
    ///
    /// A unit opened by another writer is closed with no label rather than a
    /// guessed one: the opening bracket is where a label is read from.
    #[tokio::test]
    async fn a_close_repeats_the_label_only_where_this_journal_wrote_the_opening() {
        let mut journal = journal();
        let theirs = Snowflake::from_crockford("01J8XK2M9P64").expect("a test unit");
        opened_by_hand(&mut journal, theirs).await;
        let ours = journal.open_unit("Ours").await.expect("opens");

        let closed = journal
            .close_unit(theirs, super::Closing::Aborted)
            .await
            .expect("a unit this realm opened closes, whoever opened it");
        assert_eq!(
            closed.record.resource,
            Value::Null,
            "a label this journal never held was written on the closing bracket"
        );
        let closed = journal
            .close_unit(ours, super::Closing::Committed)
            .await
            .expect("closes");
        assert_eq!(closed.record.resource, Value::text("Ours"));
    }

    /// A close the store refused keeps the unit's label for the retry.
    ///
    /// The retryable refusal tells the caller to try again, and the second
    /// attempt is the same close: it carries the same label.
    #[tokio::test]
    async fn a_close_the_store_refused_keeps_the_label_for_the_retry() {
        let mut journal = Journal::new(
            BlinksOnce {
                beneath: InMemory::default(),
                seen: 0,
                // The second append: the opening, then the close.
                blink_at: 1,
            },
            id("realm-1"),
            id("kernel"),
            minter(),
        );
        let unit = journal.open_unit("Import album").await.expect("opens");
        let refused = journal
            .close_unit(unit, super::Closing::Committed)
            .await
            .expect_err("the store blinked");
        assert!(matches!(refused, Error::Unavailable(_)), "{refused:?}");
        let closed = journal
            .close_unit(unit, super::Closing::Committed)
            .await
            .expect("the retry closes it");
        assert_eq!(
            closed.record.resource,
            Value::text("Import album"),
            "the retry lost the label the refused close had"
        );
    }

    /// A store that could not say whether a unit opened is not a unit that
    /// never opened.
    ///
    /// **`close_unit` asks `unit_opened` first, and a failure there was one
    /// token from reading as "no".** `!= Ok(true)` in place of its `?` turned
    /// `Unavailable`, which tells the caller to try again, into
    /// `UnitNeverOpened`, which tells them nothing will change — with every
    /// suite but the PostgreSQL one green, reported on 2026-09-28. The retry
    /// has to be the same close, so nothing is written and the label is kept
    /// for it. And the provided `unit_closed` passes the same failure on
    /// rather than answering "not closed" over it.
    #[tokio::test]
    async fn a_store_that_cannot_say_whether_a_unit_opened_is_not_a_unit_that_never_opened() {
        let mut journal = Journal::new(
            CannotSay {
                beneath: InMemory::default(),
                silent: false,
            },
            id("realm-1"),
            id("kernel"),
            minter(),
        );
        let unit = journal.open_unit("Import album").await.expect("opens");
        journal.store_mut().silent = true;
        let before = journal.store().beneath.len();

        let refused = journal
            .close_unit(unit, super::Closing::Committed)
            .await
            .expect_err("the store could not say");
        assert!(
            matches!(refused, Error::Unavailable(_)),
            "a store that could not say whether {unit} opened was read as an answer: \
             {refused:?}"
        );
        assert!(
            refused.code().retryable(),
            "a close the store could not check was reported as one no retry can land"
        );
        assert_eq!(
            journal.store().beneath.len(),
            before,
            "a close the store could not check wrote anyway"
        );
        assert!(
            journal.labels.contains_key(&unit),
            "a close the store could not check dropped the label its retry needs"
        );
        assert_eq!(
            journal.store().unit_closed(&id("realm-1"), unit).await,
            Err(silence()),
            "`unit_closed` answered over a store that could not say how the unit closed"
        );

        journal.store_mut().silent = false;
        let closed = journal
            .close_unit(unit, super::Closing::Committed)
            .await
            .expect("the retry closes it");
        assert_eq!(
            closed.record.resource,
            Value::text("Import album"),
            "the retry lost the label the refused close kept"
        );
    }

    /// A close refused because another writer already closed the unit keeps
    /// no label: no retry of it can ever land.
    ///
    /// **The label was dropped only when the close landed**, so this refusal
    /// kept it for as long as the journal lived — measured on 2026-09-28, the
    /// map still holding `"Import album"` after `UnitClosed`. The closing
    /// bracket here is written by hand through `record`, which is what a second
    /// writer, another process's recovery or an `INSERT`-only party produces.
    #[tokio::test]
    async fn a_close_refused_as_closed_keeps_no_label() {
        let mut journal = journal();
        let unit = journal.open_unit("Import album").await.expect("opens");
        let mut closing = record(2);
        closing.action = super::CLOSES.to_owned();
        closing.result = Outcome::UnitCommitted;
        closing.work_unit = Some(unit);
        journal
            .record(closing)
            .await
            .expect("another writer closes it");
        assert_eq!(
            journal.close_unit(unit, super::Closing::Aborted).await,
            Err(Error::UnitClosed { unit })
        );
        assert!(
            journal.labels.is_empty(),
            "a unit no close can ever land for still holds its label: {:?}",
            journal.labels
        );
    }

    /// An opening the store refused keeps no label.
    ///
    /// Its identifier is never returned, so nothing could close it — a label
    /// kept for it would be held for ever.
    #[tokio::test]
    async fn an_opening_the_store_refused_keeps_no_label() {
        let mut journal = Journal::new(
            BlinksOnce {
                beneath: InMemory::default(),
                seen: 0,
                blink_at: 0,
            },
            id("realm-1"),
            id("kernel"),
            minter(),
        );
        let refused = journal
            .open_unit("Never lands")
            .await
            .expect_err("the store blinked");
        assert!(matches!(refused, Error::Unavailable(_)), "{refused:?}");
        assert!(
            journal.labels.is_empty(),
            "a unit whose opening never landed left its label behind: {:?}",
            journal.labels
        );
        let got = &journal.store().beneath;
        assert!(got.is_empty(), "the refused opening landed: {got:?}");
    }

    /// A party that can only append chooses how a unit that is still open
    /// closed.
    ///
    /// **The first-bracket rule stops a later bracket from changing the answer,
    /// and nothing makes the first one the owner's.** Chained onto the tail
    /// through the public `Store::append` — all a party that can only `INSERT`
    /// needs, since the digest is unkeyed — a closing bracket with the outcome
    /// of that party's choosing lands on a unit its owner still has open.
    /// `unit_closing` then reports that outcome, the owner's own close is
    /// refused as closed, and `verify` answers `Ok` over all of it. Reported on
    /// 2026-09-28 against a crate header that called this only an availability
    /// failure, while the kernel's undo acts on `Some(UnitCommitted)`. Pinned so
    /// that the change which does make the first bracket the owner's fails here
    /// and rewrites `Store::unit_closing`'s paragraph with it.
    #[tokio::test]
    async fn a_party_that_can_only_append_chooses_how_an_open_unit_closed() {
        for (forged, owners) in [
            (Outcome::UnitAborted, super::Closing::Committed),
            (Outcome::UnitCommitted, super::Closing::Aborted),
        ] {
            let mut journal = journal();
            let unit = journal.open_unit("Import album").await.expect("opens");
            let mut closing = record(2);
            closing.realm = id("realm-1");
            closing.principal = id("intruder");
            closing.action = super::CLOSES.to_owned();
            closing.result = forged;
            closing.work_unit = Some(unit);
            let tail = journal
                .store()
                .tail()
                .await
                .expect("readable")
                .expect("a tail");
            journal
                .store_mut()
                .append(Entry::chain(tail.sequence + 1, Some(tail.hash), closing))
                .await
                .expect("the store takes a closing bracket from anyone who can chain one");

            assert_eq!(
                journal.close_unit(unit, owners).await,
                Err(Error::UnitClosed { unit }),
                "the owner's `{owners:?}` was not refused as closed after a closing bracket \
                 it did not write"
            );
            assert_eq!(
                journal.store().unit_closing(&id("realm-1"), unit).await,
                Ok(Some(forged)),
                "the store no longer reports the outcome the appending party chose, so \
                 `Store::unit_closing`'s paragraph on the first bracket is stale"
            );
            assert_eq!(
                journal.verify(0).await.expect("readable"),
                Ok(2),
                "`verify` no longer answers `Ok` over a closing bracket its owner did not \
                 write, so the paragraph saying it does is stale"
            );
        }
    }
}
