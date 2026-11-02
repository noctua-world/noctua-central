//! A journal store in memory.
//!
//! Not a mock. It is a complete implementation of the trait, and it is what
//! makes acceptance criterion 3 provable in the strong form it is written in:
//! *any* mutation of *any* earlier entry breaks verification **at that point**.
//! Proving that means building a history, then breaking it once at every
//! position, and verifying each — hundreds of chains per run. Against a database
//! that would be a test somebody runs weekly and eventually deletes.
//!
//! It also has one capability nothing else will: it can be **tampered with**.
//! `tamper` and `remove` exist so the tests can do what somebody with write
//! access to the table would, and both are behind the `test-support` feature,
//! which is off unless a test asks for it. The PostgreSQL store refuses
//! `UPDATE` and `DELETE` on the journal in a trigger
//! (`migrations/0001_journal.sql`), so
//! there the append-only rule is the product rather than the subject of a test —
//! `the_table_refuses_to_be_edited` watches the trigger fire. This paragraph
//! described both in the future tense after they shipped.

use core::future::Future;

use crate::{Entry, Error, Store};

/// Every entry, in order, in a `Vec`.
#[derive(Debug, Default, Clone)]
pub struct InMemory {
    entries: Vec<Entry>,
}

impl InMemory {
    /// Every entry written so far.
    #[must_use]
    pub fn all(&self) -> &[Entry] {
        &self.entries
    }

    /// How many entries there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing has been written.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Replaces the entry at `sequence`, leaving its stored hash alone.
    ///
    /// **Behind the `test-support` feature, and it is the reason this store
    /// exists.** It does what
    /// somebody with write access to the table would do: change a row and leave
    /// the chain to be discovered.
    ///
    /// # Panics
    ///
    /// When `sequence` is past the end.
    #[cfg(any(test, feature = "test-support"))]
    pub fn tamper(&mut self, sequence: u64, edit: impl FnOnce(&mut Entry)) {
        let index = usize::try_from(sequence).expect("a test history is small");
        edit(&mut self.entries[index]);
    }

    /// Removes the entry at `sequence` without renumbering anything.
    ///
    /// For tests: this is the other shape tampering takes, and the one a chain
    /// catches through its links rather than through a hash.
    ///
    /// # Panics
    ///
    /// When `sequence` is past the end.
    #[cfg(any(test, feature = "test-support"))]
    pub fn remove(&mut self, sequence: u64) {
        let index = usize::try_from(sequence).expect("a test history is small");
        self.entries.remove(index);
    }
}

impl Store for InMemory {
    // **`ready` rather than an `async` block.** Nothing here awaits, and an
    // `async` block would build a state machine to return a value that is
    // already computed. The trait is asynchronous because the store that runs is
    // PostgreSQL; this one simply answers.
    fn tail(&self) -> impl Future<Output = Result<Option<Entry>, Error>> + Send {
        core::future::ready(Ok(self.entries.last().cloned()))
    }

    fn head(&self) -> impl Future<Output = Result<Option<(u64, [u8; 32])>, Error>> + Send {
        let answer = self
            .entries
            .last()
            .map(|entry| (entry.sequence, entry.hash));
        core::future::ready(Ok(answer))
    }

    fn append(&mut self, entry: Entry) -> impl Future<Output = Result<(), Error>> + Send {
        // **Here, not one layer up.** `Journal::append` is `pub(crate)` and this
        // is public, so the rule that used to live there was a rule about one
        // code path — and `InMemory` is what the tamper tests drive directly.
        if let Err(refused) = crate::readable(&entry) {
            return core::future::ready(Err(refused));
        }
        // **A unit is opened once, and that question comes first.** `Journal::append`
        // asks, and `Store::append` is public and this crate's own tests call it
        // directly — so the rule was one about a code path again, which is the
        // shape `postgres.rs`'s header names. Measured: both stores accepted a
        // second opening bracket for an open unit.
        //
        // **Before the sequence, because a unit that is open stays open.** In
        // PostgreSQL the two are true together for every loser of a race to
        // open one unit, and answering the sequence sent twelve to fifteen of
        // sixteen concurrent openers `JournalContended`, `retryable = true`,
        // for a condition no retry clears. The permanent answer is the useful
        // one, and both stores give it.
        //
        // **And closed before open, because a closed unit is still open.** The
        // opening bracket stays in the table when a unit is committed — it is
        // what says the unit began — so "has this unit been opened" answers
        // `true` for one that has finished, and reopening a *closed* unit came
        // back `UnitAlreadyOpen`: *"Finish or cancel it before starting it
        // again"*, about something that already finished. Measured with
        // `unit_opened` and `unit_closed` both `true` on one unit, in both
        // stores.
        if let Some(unit) = entry.record.work_unit
            && entry.record.action == crate::unit::OPENS
            && entry.record.result == crate::Outcome::UnitBegun
        {
            let bracket = |action: &str, closing: bool| {
                self.entries.iter().any(|written| {
                    written.record.work_unit == Some(unit)
                        && written.record.realm == entry.record.realm
                        && written.record.action == action
                        && if closing {
                            matches!(
                                written.record.result,
                                crate::Outcome::UnitCommitted | crate::Outcome::UnitAborted
                            )
                        } else {
                            written.record.result == crate::Outcome::UnitBegun
                        }
                })
            };
            if bracket(crate::unit::CLOSES, true) {
                return core::future::ready(Err(Error::UnitClosed { unit }));
            }
            if bracket(crate::unit::OPENS, false) {
                return core::future::ready(Err(Error::UnitAlreadyOpen { unit }));
            }
        }
        // **The sequence first, and PostgreSQL now answers it first too.** The
        // two stores have to give one answer for one call, and for a while the
        // agreement ran the other way round — this asked the unit first because
        // `postgres.rs` did. What changed is that asking the *unit* first was
        // measurably wrong there: PostgreSQL's diagnosis runs after the refused
        // statement, so a bracket committed in between makes `unit_closed`
        // describe a moment that has passed. Measured, with the closing bracket
        // landing 700 ms into a two-second window: the statement was refused for
        // ordering while the unit was open, and the caller was told the unit was
        // closed — wrong in 95 of 120 unforced rounds.
        //
        // The sequence cannot go stale that way: `entry.sequence` and the tail of
        // this history are both facts about this caller's own attempt. So both
        // stores answer it first, and the shared script test is what holds them
        // to it.
        //
        // **The tail's sequence and one, not the length.** `Postgres::append`
        // asks for `MAX(sequence) + 1` and this asked how many entries there
        // are, which are the same number only while nothing has ever been
        // removed — and `remove` is public behind `test-support` for the tamper
        // tests, which call it at four places. Measured on a five-entry history
        // with `remove(1)`: the legitimate next entry was refused with
        // `expected: 4, found: 5`, and an entry re-using sequence 4 was
        // *accepted*, leaving two rows at sequence 4 in a history whose
        // PostgreSQL twin has that column as its primary key. Two
        // implementations of one trait method disagreeing is the divergence the
        // shared script exists to stop, in the store criterion 3's
        // every-position proof runs on.
        let expected = self.entries.last().map_or(0, |tail| tail.sequence + 1);
        if entry.sequence != expected {
            return core::future::ready(Err(Error::OutOfOrder {
                expected,
                found: entry.sequence,
            }));
        }
        // **And the link, after the sequence.** It verifies the
        // entry against itself — `hash == compute_hash()` — and says nothing
        // about what it claims to follow, so `Entry::chain(1, Some([0xAA; 32]),
        // record)` is self-consistent, lands through the public `Store::append`,
        // and `verify` condemns the history for ever. That is the same door the
        // hash check closed, one field along, and it is the store that knows
        // what the tail is.
        //
        // **Asked after the sequence, because the order is the answer.** This
        // ran first, so an entry with a stale sequence *and* a link that no
        // longer matches came back `Undecodable` here and `OutOfOrder` from
        // PostgreSQL — one call, two variants and two codes, with opposite
        // `retryable`. A caller whose sequence is stale read a different tail
        // and is racing; only one whose sequence is right and whose link is
        // wrong read a tail that is not there.
        // **And an empty history links to nothing.** Written `if let Some(tail)`,
        // the check did not run at all when there was no tail — so
        // `Entry::chain(0, Some([0xAA; 32]), record)` was *accepted* as the
        // first entry of a history, and `verify` then answered
        // `Broken { sequence: 0, LinkDoesNotMatchPredecessor }` for ever, on a
        // table nobody had tampered with. The genesis entry's link is `None`,
        // and that is as much a fact about the tail as any other.
        if entry.previous.as_ref() != self.entries.last().map(|tail| &tail.hash) {
            return core::future::ready(Err(Error::Undecodable {
                sequence: entry.sequence,
                why: "this entry does not link to the tail it would follow, so the history \
                      it joins could never verify. `Entry::chain` is what links one"
                    .to_owned(),
            }));
        }
        if let Some(unit) = entry.record.work_unit
            && self.entries.iter().any(|written| {
                written.record.work_unit == Some(unit)
                    && written.record.realm == entry.record.realm
                    // **And the action, which is what makes it a bracket.** The
                    // third copy of this predicate, and the one the two above it
                    // were fixed without: an ordinary `Write` carrying
                    // `Outcome::UnitCommitted` closed somebody else's group.
                    && written.record.action == crate::unit::CLOSES
                    && matches!(
                        written.record.result,
                        crate::Outcome::UnitCommitted | crate::Outcome::UnitAborted
                    )
            })
        {
            return core::future::ready(Err(Error::UnitClosed { unit }));
        }
        self.entries.push(entry);
        core::future::ready(Ok(()))
    }

    fn from(&self, sequence: u64) -> impl Future<Output = Result<crate::Read, Error>> + Send {
        // In memory nothing is undecodable: every entry here was constructed by
        // this crate, so `stopped_at` is always `None`.
        let entries: Vec<Entry> = self
            .entries
            .iter()
            .filter(|entry| entry.sequence >= sequence)
            .cloned()
            .collect();
        core::future::ready(Ok(crate::Read {
            entries,
            stopped_at: None,
        }))
    }

    fn unit_opened(
        &self,
        realm: &nc_ids::OpaqueId,
        unit: nc_ids::Snowflake,
    ) -> impl Future<Output = Result<bool, Error>> + Send {
        let opened = self.entries.iter().any(|entry| {
            entry.record.work_unit == Some(unit)
                && &entry.record.realm == realm
                && entry.record.action == crate::unit::OPENS
                && entry.record.result == crate::Outcome::UnitBegun
        });
        core::future::ready(Ok(opened))
    }

    fn unit_closing(
        &self,
        realm: &nc_ids::OpaqueId,
        unit: nc_ids::Snowflake,
    ) -> impl Future<Output = Result<Option<crate::Outcome>, Error>> + Send {
        // The same columns PostgreSQL is asked for, and the realm is one of
        // them: a bracket in another realm's history closes nothing here.
        // **And the action, which is what makes it a bracket.** Without it an
        // ordinary `Write` carrying `Outcome::UnitCommitted` closes somebody
        // else's unit — measured against PostgreSQL, and this store answers the
        // same question, so it had the same hole.
        //
        // **The first in order, as PostgreSQL's `ORDER BY sequence` takes it**,
        // and its outcome rather than the fact of it: a unit rolled back is
        // closed and did not commit, and those are two answers.
        let closing = self
            .entries
            .iter()
            .find(|entry| {
                entry.record.work_unit == Some(unit)
                    && &entry.record.realm == realm
                    && entry.record.action == crate::unit::CLOSES
                    && matches!(
                        entry.record.result,
                        crate::Outcome::UnitCommitted | crate::Outcome::UnitAborted
                    )
            })
            .map(|entry| entry.record.result);
        core::future::ready(Ok(closing))
    }
}

#[cfg(test)]
mod tests {
    use super::InMemory;
    use crate::{Entry, Error, Store};
    use crate::{Outcome, Record};
    use nc_canon::Value;
    use nc_ids::OpaqueId;

    #[tokio::test]
    async fn an_entry_out_of_order_is_refused_with_both_numbers() {
        // What a second writer racing this one looks like from here. Refusing is
        // what keeps the sequence dense, and a dense sequence is what makes a
        // gap indistinguishable from nothing rather than from a deletion.
        let mut store = InMemory::default();
        let record = Record::new(
            OpaqueId::new("user-1").expect("a test identifier"),
            "Read",
            Value::Null,
            Outcome::Allowed,
            0,
        );
        let entry = Entry::chain(3, None, record);
        assert_eq!(
            store.append(entry).await,
            Err(Error::OutOfOrder {
                expected: 0,
                found: 3
            })
        );
    }

    /// One entry, chained and appended, so a test can start from a history.
    async fn write(store: &mut InMemory, result: Outcome, unit: Option<nc_ids::Snowflake>) {
        let sequence = u64::try_from(store.entries.len()).expect("a short test history");
        // **A bracket carries the bracket's action**, which is what makes it
        // one: the closed-unit predicate asks for `CLOSES`, so an ordinary
        // `Read` with a bracket's *outcome* is an ordinary entry.
        let mut record = Record::new(
            OpaqueId::new("user-1").expect("a test identifier"),
            match result {
                Outcome::UnitBegun => crate::unit::OPENS,
                Outcome::UnitCommitted | Outcome::UnitAborted => crate::unit::CLOSES,
                _ => "Read",
            },
            Value::Null,
            result,
            sequence,
        );
        record.work_unit = unit;
        let previous = store
            .tail()
            .await
            .expect("readable")
            .map(|entry| entry.hash);
        store
            .append(Entry::chain(sequence, previous, record))
            .await
            .expect("the history is well formed");
    }

    /// Only an opening bracket is asked whether the unit is already open.
    ///
    /// **Both clauses of that guard survived being deleted.** Three tests were
    /// written for the rule and all three place their odd entry where no bracket
    /// exists yet, so the question is never reached and the guard decides
    /// nothing in any of them. With `action == OPENS` gone, an ordinary entry
    /// carrying a bracket's *outcome* is judged an opener; with
    /// `result == UnitBegun` gone, a closing action is; and either way this
    /// store answers `UnitAlreadyOpen` to an entry `postgres.rs` accepts, which
    /// is the divergence between the two implementations that the shared script
    /// exists to stop.
    ///
    /// So the bracket is opened first, and then each half is offered alone.
    #[tokio::test]
    async fn only_an_opening_bracket_is_asked_whether_the_unit_is_open() {
        for (action, result) in [
            // A bracket's outcome on an ordinary action.
            ("Read", Outcome::UnitBegun),
            // A bracket's action with an ordinary outcome.
            (crate::unit::OPENS, Outcome::Allowed),
        ] {
            let unit = nc_ids::Snowflake::from_crockford("01J8XK2M9P51").expect("a test unit");
            let mut store = InMemory::default();
            // The real opener, so the question the guard asks has an answer.
            write(&mut store, Outcome::UnitBegun, Some(unit)).await;

            let previous = store
                .tail()
                .await
                .expect("readable")
                .map(|entry| entry.hash);
            let mut record = Record::new(
                OpaqueId::new("user-1").expect("a test identifier"),
                action,
                Value::Null,
                result,
                1,
            );
            record.work_unit = Some(unit);
            store
                .append(Entry::chain(1, previous, record))
                .await
                .unwrap_or_else(|refused| {
                    panic!(
                        "`{action}` with `{result:?}` was judged an opening bracket and \
                         refused with {refused:?}, which is what the store that runs \
                         accepts"
                    )
                });
        }
    }

    /// Emptiness is what is in the store, not a constant.
    ///
    /// **`is_empty` had no caller that read its value either.** Both it and
    /// `len` are the store's own accounting, and every test here reads `all()`
    /// instead — so `is_empty` could answer `true` always, or `false` always,
    /// with the whole suite green. Both directions are asked, and against
    /// `len`, so the two cannot drift apart.
    #[tokio::test]
    async fn an_empty_store_says_so_and_a_written_one_does_not() {
        let mut store = InMemory::default();
        assert!(
            store.is_empty(),
            "a store with nothing in it says it has something: {store:?}"
        );
        assert_eq!(store.len(), 0, "and its length disagrees with that");
        write(&mut store, Outcome::Allowed, None).await;
        assert!(
            !store.is_empty(),
            "a store with an entry in it says it is empty, which is what a caller asks \
             before deciding whether there is a history at all"
        );
        assert_eq!(store.len(), 1);
    }

    /// After a removal, the next sequence is the tail's and one.
    ///
    /// **A count stood in for a position.** This store answered
    /// `self.entries.len()` and `Postgres::append` answers `MAX(sequence) + 1`,
    /// which are one number only while nothing has been removed — and `remove`
    /// exists for the tamper tests, which call it. Measured on the old code with
    /// this history: the legitimate next entry was refused with
    /// `expected: 4, found: 5`, and one re-using sequence 4 was accepted,
    /// leaving two rows at a sequence that is PostgreSQL's primary key.
    #[tokio::test]
    async fn the_next_sequence_after_a_removal_is_the_tail_s_and_one() {
        /// An entry chained onto whatever this history's tail is.
        async fn chained(store: &InMemory, sequence: u64) -> Entry {
            let previous = store
                .tail()
                .await
                .expect("readable")
                .map(|entry| entry.hash);
            let record = Record::new(
                OpaqueId::new("user-1").expect("a test identifier"),
                "Read",
                Value::Null,
                Outcome::Allowed,
                sequence,
            );
            Entry::chain(sequence, previous, record)
        }

        let mut store = InMemory::default();
        for _ in 0..5 {
            write(&mut store, Outcome::Allowed, None).await;
        }
        store.remove(1);
        // Sequences 0, 2, 3, 4 — four entries whose tail is 4, which is what
        // makes the length and the position two different numbers.
        assert_eq!(store.len(), 4, "the removal did not take one entry");
        assert_eq!(
            store.all().last().map(|entry| entry.sequence),
            Some(4),
            "nothing was renumbered, which is what `remove` is for"
        );

        let duplicate = chained(&store, 4).await;
        assert_eq!(
            store.append(duplicate).await,
            Err(Error::OutOfOrder {
                expected: 5,
                found: 4
            }),
            "a sequence this history already holds was accepted, and the column it \
             lands in is a primary key in the store that runs"
        );
        let next = chained(&store, 5).await;
        store
            .append(next)
            .await
            .expect("the entry after the tail is the next entry");
        assert_eq!(
            store.all().last().map(|entry| entry.sequence),
            Some(5),
            "the legitimate next entry did not land"
        );
    }

    /// A closed unit is closed at this door too.
    ///
    /// **The two stores disagreed here.** `Journal::append` asks
    /// `Store::unit_closed` and PostgreSQL asks it again inside the insert's own
    /// `WHERE`, and this store asked nowhere — so a caller that chained the
    /// entry itself and went straight to `Store::append` was refused by one
    /// implementation of the trait and accepted by the other. Measured before
    /// the fix: the entry landed at sequence 2, *after* its own
    /// `unit-committed` bracket at 1, and `verify` answered `Ok(3)`.
    ///
    /// This is the store criterion 3's every-position proof runs on, so the
    /// lenient half was the wrong half — evidence from here is only worth what
    /// it says about the store that runs in production.
    #[tokio::test]
    async fn an_entry_after_its_own_bracket_is_refused_however_it_arrives() {
        for closing in [Outcome::UnitCommitted, Outcome::UnitAborted] {
            let unit = nc_ids::Snowflake::from_crockford("01J8XK2M9P51").expect("a test unit");
            let mut store = InMemory::default();
            write(&mut store, Outcome::UnitBegun, Some(unit)).await;
            write(&mut store, closing, Some(unit)).await;

            let mut record = Record::new(
                OpaqueId::new("user-1").expect("a test identifier"),
                "Read",
                Value::Null,
                Outcome::Allowed,
                2,
            );
            record.work_unit = Some(unit);
            let previous = store
                .tail()
                .await
                .expect("readable")
                .map(|entry| entry.hash);
            let refused = store.append(Entry::chain(2, previous, record)).await;
            assert!(
                matches!(refused, Err(Error::UnitClosed { unit: named }) if named == unit),
                "a unit closed by `{closing:?}` accepted an entry after its own \
                 bracket: {refused:?}"
            );
            assert_eq!(
                store.entries.len(),
                2,
                "the refused entry was written anyway"
            );
        }
    }

    /// A unit closed in another realm closes nothing here.
    ///
    /// The other half of the rule above, so the refusal cannot be a blanket one
    /// keyed on the unit alone — `unit_closing` reads the realm as one of its
    /// columns and this door has to read it the same way.
    #[tokio::test]
    async fn a_bracket_in_another_realm_closes_nothing() {
        let unit = nc_ids::Snowflake::from_crockford("01J8XK2M9P52").expect("a test unit");
        let mut store = InMemory::default();
        write(&mut store, Outcome::UnitBegun, Some(unit)).await;
        write(&mut store, Outcome::UnitCommitted, Some(unit)).await;

        let mut record = Record::new(
            OpaqueId::new("user-2").expect("a test identifier"),
            "Read",
            Value::Null,
            Outcome::Allowed,
            2,
        );
        record.realm = OpaqueId::new("another-realm").expect("a test realm");
        record.work_unit = Some(unit);
        let previous = store
            .tail()
            .await
            .expect("readable")
            .map(|entry| entry.hash);
        store
            .append(Entry::chain(2, previous, record))
            .await
            .expect("another realm's bracket closes nothing here");
    }

    /// A unit opened in another realm is not open here.
    ///
    /// **The twin of the test above, which `unit_opened` did not have.**
    /// Measured, its realm comparison flipped from `==` to `!=`: the whole
    /// workspace stayed green. After that `UnitAlreadyOpen` is unreachable for a
    /// unit opened in this realm — the guard that stops a second `open` on one
    /// unit — and fires for every unit opened in another, which is the
    /// cross-realm leak the sibling test exists to refuse. `memory.rs`'s own
    /// comment warns that these two doors have to read the realm the same way.
    #[tokio::test]
    async fn a_unit_opened_in_another_realm_is_not_open_here() {
        let unit = nc_ids::Snowflake::from_crockford("01J8XK2M9P53").expect("a test unit");
        let mut store = InMemory::default();
        write(&mut store, Outcome::UnitBegun, Some(unit)).await;

        // `Record::new` seeds the realm from the principal, and `write` writes as
        // `user-1` — so that is the realm this bracket is in.
        let here = store
            .unit_opened(&OpaqueId::new("user-1").expect("a test realm"), unit)
            .await
            .expect("readable");
        assert!(
            here,
            "the unit this store opened is not open in its own realm"
        );

        let elsewhere = store
            .unit_opened(&OpaqueId::new("another-realm").expect("a test realm"), unit)
            .await
            .expect("readable");
        assert!(
            !elsewhere,
            "a unit opened in one realm reads as open in another, so one realm's \
             identifier decides another realm's brackets"
        );
    }

    /// A bracket is a bracket, and an ordinary entry with a bracket's outcome
    /// is not one — asked of `unit_opened`, which had no such test.
    ///
    /// **Every clause of `unit_opened` was switched off with the gate green**,
    /// in both stores: the action, the outcome, and — in PostgreSQL — the realm,
    /// the action and the result each `OR TRUE`. Five plants, five times
    /// `check passed` at the baseline test count. The closing twin's clauses had
    /// been repaired in three copies of the predicate under a paragraph making
    /// exactly this argument; one bracket along, nothing asked.
    ///
    /// What it costs when it is wrong: an ordinary `Write` carrying
    /// `Outcome::UnitBegun` marks a unit open for ever, so the legitimate
    /// `begin` is refused `UnitAlreadyOpen` and the group can never be created.
    #[tokio::test]
    async fn a_bracket_is_a_bracket_and_a_unit_is_opened_once() {
        let unit = nc_ids::Snowflake::from_crockford("01J8XK2M9P54").expect("a test unit");
        let realm = OpaqueId::new("user-1").expect("a test realm");
        let mut store = InMemory::default();

        // An ordinary entry carrying a bracket's *outcome*, stamped with the
        // unit: the outcome matches and the action does not.
        let sequence = u64::try_from(store.entries.len()).expect("a short test history");
        let mut ordinary = Record::new(
            realm.clone(),
            "Write",
            Value::Null,
            Outcome::UnitBegun,
            sequence,
        );
        ordinary.work_unit = Some(unit);
        let previous = store
            .tail()
            .await
            .expect("readable")
            .map(|entry| entry.hash);
        store
            .append(Entry::chain(sequence, previous, ordinary))
            .await
            .expect("the store accepts it");
        assert!(
            !store.unit_opened(&realm, unit).await.expect("readable"),
            "an ordinary `Write` carrying a bracket's outcome opened a unit, so the \
             legitimate `begin` would be refused and the group could never be created"
        );

        // And the mirror: a bracket's *action* with an ordinary outcome. The
        // two clauses are separate, and a test that trips only one leaves the
        // other free — measured, the outcome clause survived the first draft of
        // this test with the whole suite green.
        let sequence = u64::try_from(store.entries.len()).expect("a short test history");
        let mut refused = Record::new(
            realm.clone(),
            crate::unit::OPENS,
            Value::Null,
            Outcome::Denied,
            sequence,
        );
        refused.work_unit = Some(unit);
        let previous = store
            .tail()
            .await
            .expect("readable")
            .map(|entry| entry.hash);
        store
            .append(Entry::chain(sequence, previous, refused))
            .await
            .expect("the store accepts it");
        assert!(
            !store.unit_opened(&realm, unit).await.expect("readable"),
            "a `kernel.begin` that was denied opened a unit, so a rejected attempt to \
             begin one locks the unit out for ever"
        );

        // And the bracket itself, so this cannot pass by never finding one.
        write(&mut store, Outcome::UnitBegun, Some(unit)).await;
        assert!(
            store.unit_opened(&realm, unit).await.expect("readable"),
            "the bracket this store just wrote does not open its own unit"
        );
    }

    /// An entry this store could not read back is refused before it is written.
    ///
    /// **Implemented in both stores and proved in one.** `Store::append`'s doc
    /// says every implementation refuses an entry it could not read back by
    /// calling `readable` first; deleting the call here left the whole suite
    /// green, and the two stores then disagreed about what a journal accepts.
    #[tokio::test]
    async fn an_entry_that_could_not_be_read_back_is_refused() {
        let mut deep = Value::Null;
        for _ in 0..=nc_canon::parse::MAX_NESTING {
            deep = Value::Array(vec![deep]);
        }
        let record = Record::new(
            OpaqueId::new("user-1").expect("a test identifier"),
            "Read",
            deep,
            Outcome::Allowed,
            0,
        );
        let mut store = InMemory::default();
        let refused = store.append(Entry::chain(0, None, record)).await;
        assert!(
            matches!(refused, Err(Error::TooDeep { .. })),
            "an entry the reader would not follow was accepted: {refused:?}"
        );
        assert!(
            store.entries.is_empty(),
            "it was refused and written anyway: {:?}",
            store.entries
        );
    }

    #[tokio::test]
    async fn reading_from_a_sequence_returns_that_entry_onwards() {
        let mut store = InMemory::default();
        for sequence in 0..5 {
            let record = Record::new(
                OpaqueId::new("user-1").expect("a test identifier"),
                "Read",
                Value::Null,
                Outcome::Allowed,
                sequence,
            );
            let previous = store
                .tail()
                .await
                .expect("readable")
                .map(|entry| entry.hash);
            store
                .append(Entry::chain(sequence, previous, record))
                .await
                .expect("appends");
        }
        let tail = store.from(3).await.expect("readable").entries;
        assert_eq!(tail.len(), 2);
        assert_eq!(tail[0].sequence, 3);
    }

    /// A unit opened in another realm does not stop it being opened here.
    ///
    /// **The third copy of the bracket predicate, and its realm clause was
    /// decided by nothing.** `Store::unit_opened` has
    /// `a_unit_opened_in_another_realm_is_not_open_here` and the general
    /// closed-unit predicate has `a_bracket_in_another_realm_closes_nothing`;
    /// the copy inside `append`'s *opening* guard had neither. Measured on
    /// 2026-09-08, `&& written.record.realm == entry.record.realm` deleted from
    /// it: `cargo test --workspace --locked` exit 0 and
    /// `cargo run -q -p xtask -- check` at `check passed`, `ok 1223 test(s)` —
    /// while every other clause of the same two predicates is RED when planted.
    ///
    /// What it costs is verbatim reason 3 of `Store::unit_closing`'s own doc, one
    /// bracket along: *"a journal opened for `realm-2` refused a write on the
    /// strength of a bracket that exists only in `realm-1`"*. `Record.work_unit`
    /// is `pub`, so anything that can append writes an opening bracket carrying
    /// a unit id, and that unit can then never be opened in another realm —
    /// reported as `NC-JRNL-0007`, *"Finish or cancel it before starting it
    /// again"*, about a unit this realm never began. The PostgreSQL store scopes
    /// both of its sub-selects `realm = $2` and accepts.
    #[tokio::test]
    async fn a_unit_opened_in_another_realm_can_still_be_opened_here() {
        let unit = nc_ids::Snowflake::from_crockford("01J8XK2M9P55").expect("a test unit");
        let mut store = InMemory::default();
        write(&mut store, Outcome::UnitBegun, Some(unit)).await;
        let mut opening = Record::new(
            OpaqueId::new("user-2").expect("a test identifier"),
            crate::unit::OPENS,
            Value::Null,
            Outcome::UnitBegun,
            1,
        );
        opening.realm = OpaqueId::new("another-realm").expect("a test realm");
        opening.work_unit = Some(unit);
        let previous = store
            .tail()
            .await
            .expect("readable")
            .map(|entry| entry.hash);
        assert_eq!(
            store.append(Entry::chain(1, previous, opening)).await,
            Ok(()),
            "a unit opened in one realm decided another realm's brackets"
        );
    }

    /// How a unit closed is its closing bracket's outcome, and only a closing
    /// bracket of this realm says it.
    ///
    /// Every clause of the question, each offered alone next to the real
    /// bracket: another realm's closing, an ordinary action carrying a closing
    /// outcome, a `kernel.unit` that was denied, and a unit nobody closed.
    #[tokio::test]
    async fn how_a_unit_closed_is_its_own_closing_bracket_and_nothing_else() {
        let realm = OpaqueId::new("user-1").expect("a test realm");
        for closing in [Outcome::UnitCommitted, Outcome::UnitAborted] {
            let unit = nc_ids::Snowflake::from_crockford("01J8XK2M9P57").expect("a test unit");
            let mut store = InMemory::default();
            write(&mut store, Outcome::UnitBegun, Some(unit)).await;
            assert_eq!(
                store.unit_closing(&realm, unit).await,
                Ok(None),
                "an open unit reads as closed"
            );

            for (action, result, realm_written) in [
                ("Write", closing, "user-1"),
                (crate::unit::CLOSES, Outcome::Denied, "user-1"),
                (crate::unit::CLOSES, closing, "another-realm"),
            ] {
                let sequence = u64::try_from(store.entries.len()).expect("a short test history");
                let mut near = Record::new(
                    OpaqueId::new("user-1").expect("a test identifier"),
                    action,
                    Value::Null,
                    result,
                    sequence,
                );
                near.realm = OpaqueId::new(realm_written).expect("a test realm");
                near.work_unit = Some(unit);
                let previous = store
                    .tail()
                    .await
                    .expect("readable")
                    .map(|entry| entry.hash);
                store
                    .append(Entry::chain(sequence, previous, near))
                    .await
                    .expect("a near miss is an ordinary entry");
                assert_eq!(
                    store.unit_closing(&realm, unit).await,
                    Ok(None),
                    "`{action}` / `{result:?}` in `{realm_written}` closed the unit"
                );
            }

            write(&mut store, closing, Some(unit)).await;
            assert_eq!(
                store.unit_closing(&realm, unit).await,
                Ok(Some(closing)),
                "the unit's own closing bracket does not say how it closed"
            );
            // A realm no entry names: `another-realm` holds a closing bracket
            // of its own from the near misses above.
            assert_eq!(
                store
                    .unit_closing(&OpaqueId::new("a-third-realm").expect("a test realm"), unit)
                    .await,
                Ok(None),
                "a unit closed in one realm reads as closed in another"
            );
            assert!(
                store.unit_closed(&realm, unit).await.expect("readable"),
                "closed and how it closed are one predicate, and they disagree"
            );
        }
    }

    /// How a unit closed is its first closing bracket, not a later one.
    ///
    /// **Which bracket answers was decided by nothing.** `append` refuses a
    /// second closing bracket, so every test here held one, and the scan could
    /// take the last match instead of the first with the whole suite green —
    /// measured on 2026-09-28, alongside PostgreSQL's `ORDER BY sequence`
    /// turned `DESC`. A second one is still reachable: `tamper` here, and a
    /// second connection's `INSERT` there. The difference between the two
    /// answers is the one `unit_closing` exists to carry — a committed unit
    /// stands and an aborted one does not — so a later bracket that flips it
    /// reads a rolled-back unit as one that stands. The later bracket is
    /// planted by `tamper` because `append` refuses it, which is the refusal
    /// that makes the first the one that closed the unit.
    #[tokio::test]
    async fn how_a_unit_closed_is_its_first_closing_bracket_not_a_later_one() {
        let realm = OpaqueId::new("user-1").expect("a test realm");
        let unit = nc_ids::Snowflake::from_crockford("01J8XK2M9P58").expect("a test unit");
        let other = nc_ids::Snowflake::from_crockford("01J8XK2M9P5A").expect("a test unit");
        let mut store = InMemory::default();
        write(&mut store, Outcome::UnitBegun, Some(unit)).await;
        write(&mut store, Outcome::UnitAborted, Some(unit)).await;
        write(&mut store, Outcome::UnitBegun, Some(other)).await;
        store.tamper(2, |entry| {
            entry.record.work_unit = Some(unit);
            entry.record.action = crate::unit::CLOSES.to_owned();
            entry.record.result = Outcome::UnitCommitted;
        });
        assert_eq!(
            store.unit_closing(&realm, unit).await,
            Ok(Some(Outcome::UnitAborted)),
            "a closing bracket written after the one that closed the unit changed how it \
             closed, so a rolled-back unit reads as one that stands"
        );
    }

    /// A unit that was *aborted* is closed for the reopen guard too.
    ///
    /// **The closing set was two outcomes and one of them was free.** Narrowing
    /// `matches!(…, UnitCommitted | UnitAborted)` in `append`'s reopen guard to
    /// `== UnitCommitted` left `cargo run -q -p xtask -- check` at
    /// `check passed`, `ok 1223 test(s)` on 2026-09-08. Reopening an aborted
    /// unit then answers `UnitAlreadyOpen` (`NC-JRNL-0007`) from this store and
    /// `UnitClosed` (`NC-JRNL-0004`) from PostgreSQL, whose `$16` binds both
    /// spellings in the INSERT's `WHERE` and again in `why_nothing_landed`.
    ///
    /// `reopening_a_closed_unit_says_it_is_closed` is the only other test of
    /// this branch and it closes with `UnitCommitted` alone; the scripts that
    /// abort exercise the *general* predicate rather than the reopen one. This
    /// file already learned the lesson once — `a unit closed by being rolled
    /// back`'s own comment says both halves could be narrowed to
    /// committed-only with the whole gate green — and the reopen copy, added
    /// afterwards, was not covered. Both outcomes, in the store that needs no
    /// database, for the reason `an_entry_after_its_own_bracket_is_refused_however_it_arrives`
    /// gives.
    #[tokio::test]
    async fn a_unit_closed_either_way_cannot_be_opened_again() {
        for closed in [Outcome::UnitCommitted, Outcome::UnitAborted] {
            let unit = nc_ids::Snowflake::from_crockford("01J8XK2M9P56").expect("a test unit");
            let mut store = InMemory::default();
            write(&mut store, Outcome::UnitBegun, Some(unit)).await;
            write(&mut store, closed, Some(unit)).await;
            let mut opening = Record::new(
                OpaqueId::new("user-1").expect("a test identifier"),
                crate::unit::OPENS,
                Value::Null,
                Outcome::UnitBegun,
                1,
            );
            opening.work_unit = Some(unit);
            let previous = store
                .tail()
                .await
                .expect("readable")
                .map(|entry| entry.hash);
            assert_eq!(
                store.append(Entry::chain(2, previous, opening)).await,
                Err(Error::UnitClosed { unit }),
                "a unit closed by `{closed:?}` is reported as still open, which tells a \
                 caller to finish something that already finished"
            );
        }
    }
}
