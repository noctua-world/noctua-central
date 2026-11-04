//! Acceptance criterion 3, in the strong form it is written in.
//!
//! > The journal's hash chain **verifies** over a synthetic history, and **any**
//! > mutation of **any** earlier entry breaks verification at that point.
//!
//! # That sentence is false, and this is where the true one is written
//!
//! The approved plan asks whether criterion 3 *as worded* is satisfied by a
//! chain that notices neither a truncated end nor an appended forged entry, and
//! requires the answer in a document rather than in a test name. The answer is
//! worth more than the question:
//!
//!   * **For those two holes, yes — and the word doing the work is "earlier".**
//!     Truncation mutates no entry; it changes which history you hold, and
//!     "at that point" then has no referent. An append mutates nothing at all:
//!     the criterion is about integrity and that is authenticity. The same word
//!     quietly excuses a **third** hole — `a_rewritten_tail_is_invisible_to_verify_and_visible_to_a_kept_head`,
//!     below —
//!     the tail is not an earlier entry either. Three holes, one word.
//!   * **But the criterion is not satisfied anyway, and the case is one its
//!     wording does cover.** Rewrite an earlier entry and recompute every hash
//!     after it, and `verify` answers `Ok`. Measured on a six-entry history with
//!     entry 2 rewritten to `principal=attacker action=Escalate` and hashes 3–5
//!     relinked: `verify(0)` answers `Ok(6)`. `NOCTUA.md §14.9` says so, and
//!     so does this crate's own header.
//!
//! So the sentence above is quoted because it is the criterion as written, not
//! because it is true. The true one is `nc_journal`'s own header, verbatim: *it
//! proves that no entry was changed or removed **without also rewriting every
//! entry after it**.* That is exactly the boundary and not an approximation — an
//! interior change with no rehash, with only its own rehash, or an interior
//! removal are all caught; a full rebuild, a truncation, a tail rewrite and an
//! append are all permitted by it and all invisible; and where it understates —
//! removal of the head — the chain catches it anyway, which is the safe
//! direction.
//!
//! What it would cost to close any of the three is priced in `§14.9`, and each
//! buys a property criterion 3 does not ask for: a store-owned counter is
//! defeated by `ALTER SEQUENCE … RESTART` and is explicitly not a proof; an
//! anchor's interval *is* the history that can still be removed; a keyed digest
//! stops the log being self-verifying by anyone holding it. Adding one to make a
//! sentence true would be a design decision taken for the sentence's sake, so
//! the smallest honest change is to the wording — which is this block.
//!
//! "Any … any" is not decoration. A chain that only caught tampering at the last
//! entry, or only caught a changed action and not a changed timestamp, would pass
//! a test written against one example and would be worthless — the entry an
//! attacker edits is the one that incriminates them, and it is never the last.
//!
//! So this builds a history and then breaks it once at **every** position, and
//! separately applies **every** edit the entry module lists, and checks that
//! verification stops exactly where the damage is and says why.
//!
//! **The edit list is imported, not copied.** A second list in this file under
//! a header claiming the two cannot drift is a claim that
//! drifts. `nc_journal::entry::tamper::edits` is behind the `test-support`
//! feature precisely so an integration test can reach it, which is the only
//! version of that claim that is true.
//!
//! **Every branch of `verify` is named by a test.** A removal test asserting
//! only *where* verification stopped and never *why* would let the link branch
//! catch the removal instead, and the suite stay green with contiguity deleted.

use nc_canon::Value;
use nc_ids::{OpaqueId, Snowflake};
use nc_journal::entry::tamper::edits;
use nc_journal::{Broken, Entry, InMemory, Journal, Outcome, Reason, Record, Store};

const LENGTH: u64 = 40;

/// A minter for a test, with a flake of its own.
///
/// One per `Journal`, because two journals sharing a flake mint the same
/// work-unit identifiers — which the constructor's signature makes visible
/// instead of hiding.
fn minter() -> nc_ids::Minter<nc_ids::SystemClock> {
    use core::sync::atomic::{AtomicU16, Ordering};
    static NEXT: AtomicU16 = AtomicU16::new(1);
    let flake = NEXT.fetch_add(1, Ordering::Relaxed) % nc_ids::flake::RANGE;
    nc_ids::Minter::new(
        nc_ids::MachineFlake::from_value(flake).unwrap_or_else(|_| unreachable!("masked")),
        nc_ids::SystemClock,
    )
}

fn id(text: &str) -> OpaqueId {
    OpaqueId::new(text).expect("a test identifier")
}

async fn history() -> Journal<InMemory> {
    let mut journal = Journal::new(InMemory::default(), id("realm-1"), id("kernel"), minter());
    for step in 0..LENGTH {
        let mut record = Record::new(
            id(&format!("user-{}", step % 7)),
            if step % 3 == 0 { "Read" } else { "Write" },
            Value::object([("id", Value::Integer(i64::try_from(step).expect("small")))]),
            if step % 11 == 0 {
                Outcome::Denied
            } else {
                Outcome::Allowed
            },
            1_700_000_000_000 + step,
        );
        // **Every identifier field carries something.** `Record::new` leaves
        // `actor_chain` empty and the four optionals `None`, so the case-only
        // edits in `tamper::edits()` — the ones that tell an injective
        // `hashed_form` from a case-folding one — would have nothing to respell
        // in this history and would set a value instead, which any rendering
        // distinguishes from `Null`. A sweep over a history whose authorization
        // fields are all empty is a sweep over none of the fields the kernel
        // fills.
        record.actor_chain = vec![id("operator-1"), id(&format!("agent-{}", step % 5))];
        record.deciding_policy = Some(format!("core.allow_{}", step % 4));
        record.policy_pack = Some("family-1".to_owned());
        record.grant_used = Some(id(&format!("grant-for-{step}")));
        journal
            .record(record)
            .await
            .expect("the store is in memory");
    }
    journal
}

async fn broken(journal: &Journal<InMemory>) -> Option<Broken> {
    journal
        .verify(0)
        .await
        .expect("the store is in memory")
        .err()
}

#[tokio::test]
async fn an_untouched_history_verifies_end_to_end() {
    // The control. Without it every test below could pass against a chain that
    // never verified at all.
    let journal = history().await;
    assert_eq!(journal.verify(0).await.expect("readable"), Ok(LENGTH));
}

#[tokio::test]
async fn changing_any_entry_breaks_verification_at_that_entry() {
    // **The loop bound is the whole claim, so it is held.** Criterion 3 says
    // *any* mutation of *any* earlier entry, and the only thing saying this
    // sweeps more than one entry is the range on the line below: narrowed to
    // `0..1`, without the count, the whole suite stays green and
    // `check passed`, with the chain proven at exactly one position — the one
    // an attacker never edits. So the positions are counted as they are
    // tampered and the count is the assertion, the way `edits()` is floored by
    // `every_field_of_a_record_has_an_edit`.
    let mut tampered = 0_u64;
    for position in 0..LENGTH {
        let mut journal = history().await;
        journal.store_mut().tamper(position, |entry| {
            "Delete".clone_into(&mut entry.record.action);
        });
        let failure = broken(&journal)
            .await
            .unwrap_or_else(|| panic!("the chain accepted an edit at entry {position}"));
        assert_eq!(
            failure.sequence, position,
            "the chain noticed at {} rather than at {position}",
            failure.sequence
        );
        assert_eq!(failure.reason, Reason::HashDoesNotMatchContents);
        tampered += 1;
    }
    assert_eq!(
        tampered, LENGTH,
        "the chain was broken at {tampered} of {LENGTH} position(s). Criterion 3 is \
         written \"any mutation of any earlier entry\", and a history swept at one \
         position proves the chain at that position"
    );
}

#[tokio::test]
async fn changing_any_field_of_any_entry_breaks_verification() {
    // Position and field together: a hashed form that omitted one field would
    // pass the test above, which only ever changes the action.
    for (field, edit) in edits() {
        // **And where each edit was tried, for the reason above.** This list is
        // the other bound: narrowed to `[0]`, every field would still be
        // proven, at the first entry only, with the suite green. Collected
        // rather than counted, because "three" is satisfied by three copies of
        // the same position — what the sweep is for is the first entry, the
        // middle one and the last.
        let mut at: Vec<u64> = Vec::new();
        for position in [0, LENGTH / 2, LENGTH - 1] {
            let mut journal = history().await;
            let index = usize::try_from(position).expect("a test history is small");
            let before = journal.store().all()[index].clone();
            journal.store_mut().tamper(position, edit);
            let after = &journal.store().all()[index];
            assert_ne!(
                &before, after,
                "the `{field}` edit at {position} changed nothing, so this proves nothing"
            );
            let failure = broken(&journal).await.unwrap_or_else(|| {
                panic!("`{field}` is not covered by the hash: an edit at {position} passed")
            });
            assert_eq!(failure.sequence, position, "`{field}` at {position}");
            assert_eq!(failure.reason, Reason::HashDoesNotMatchContents);
            at.push(position);
        }
        assert_eq!(
            at,
            [0, LENGTH / 2, LENGTH - 1],
            "the `{field}` edit was tried at {at:?} rather than at the first entry of \
             the history, the middle one and the last"
        );
    }
}

#[tokio::test]
async fn re_hashing_an_edited_entry_still_breaks_the_link_to_the_next_one() {
    // The attacker who knows how the chain works: edit the row *and* fix its
    // hash. That is what the link is for — the entry after it still points at
    // what the entry used to be.
    let mut swept = 0_u64;
    for position in 0..LENGTH - 1 {
        let mut journal = history().await;
        journal.store_mut().tamper(position, |entry| {
            "Delete".clone_into(&mut entry.record.action);
            entry.hash = entry.compute_hash();
        });
        let failure = broken(&journal)
            .await
            .unwrap_or_else(|| panic!("a re-hashed edit at {position} passed"));
        assert_eq!(
            failure.sequence,
            position + 1,
            "the break should surface at the entry that links to it"
        );
        assert_eq!(failure.reason, Reason::LinkDoesNotMatchPredecessor);
        swept += 1;
    }
    assert_eq!(
        swept,
        LENGTH - 1,
        "the sweep re-hashed {swept} of {} entry(ies). The attacker who fixes the hash \
         is what the link is for, and a history swept at one position proves the link \
         at that position",
        LENGTH - 1
    );
}

#[tokio::test]
async fn a_skipped_sequence_is_caught_as_a_skipped_sequence() {
    // The contiguity branch, asserted by name: a removal caught by the *link*
    // branch instead would leave the whole suite green with the contiguity
    // check deleted, unless the test asserts which.
    let mut swept = 0_u64;
    for position in 1..LENGTH - 1 {
        let mut journal = history().await;
        // Renumber one entry forward and relink the next to it, so every link
        // still matches and only the numbering is wrong.
        journal.store_mut().tamper(position, |entry| {
            entry.sequence += 1;
            entry.hash = entry.compute_hash();
        });
        journal.store_mut().tamper(position + 1, |entry| {
            entry.previous = None;
            entry.hash = entry.compute_hash();
        });
        let failure = broken(&journal)
            .await
            .unwrap_or_else(|| panic!("a renumbered entry at {position} passed"));
        // **Reported where the break is, with the claimed number beside it.**
        // The number the entry now *claims* is the attacker's: renumbering the
        // forged row to 500000 and re-hashing it would make a verdict built
        // from it say `sequence: 500000`, sending a responder to a row that is
        // not in the table. The position is what they can trust; the claimed
        // number is what they grep for, and it is in `detail`.
        assert_eq!(failure.sequence, position);
        assert_eq!(
            failure.detail.as_deref(),
            Some(format!("the row there is numbered {}", position + 1).as_str()),
            "the number to look for is not in the verdict"
        );
        assert_eq!(
            failure.reason,
            Reason::SequenceIsNotContiguous,
            "a renumbering has to be caught as a renumbering, not by the link"
        );
        swept += 1;
    }
    assert_eq!(
        swept,
        LENGTH - 2,
        "the sweep renumbered {swept} of {} entry(ies). A renumbering caught as a \
         renumbering at one position says nothing about the branch anywhere else",
        LENGTH - 2
    );
}

/// A tampered entry is reported where it sits, not where it says it sits.
///
/// **Three rules in `chained_and_contiguous`, each held.** The contiguity arm
/// has its own case; the hash arm and the link arm carry the same `where_it_is`
/// and the same `detail`, and without this nothing reaches either — so
/// `sequence: where_it_is` could be `entry.sequence` in both, and the hash
/// arm's `detail` `None`, with every `nc-journal` test and the whole gate
/// passing.
///
/// One rule written three times, with one copy tested. The number the entry
/// claims is the attacker's — renumbering a forged row to 500000 makes the
/// verdict name a row that is not in the table — so the position is the verdict
/// and the claimed number is the detail.
#[tokio::test]
async fn a_tampered_entry_is_reported_where_it_sits_and_not_where_it_claims() {
    let mut swept = 0_u64;
    for position in 1..LENGTH - 1 {
        let mut journal = history().await;
        // Rewrite the contents *and* renumber, leaving the hash stale: the hash
        // arm decides this before contiguity or the link can.
        journal.store_mut().tamper(position, |entry| {
            "Delete".clone_into(&mut entry.record.action);
            entry.sequence = 500_000;
        });
        let failure = broken(&journal)
            .await
            .unwrap_or_else(|| panic!("a tampered entry at {position} passed"));
        assert_eq!(
            failure.reason,
            Reason::HashDoesNotMatchContents,
            "a rewritten entry has to be caught by its own hash"
        );
        assert_eq!(
            failure.sequence, position,
            "the verdict points at {}, which is the number the forged row gave itself",
            failure.sequence
        );
        assert_eq!(
            failure.detail.as_deref(),
            Some("the row there is numbered 500000"),
            "the number to grep the table for is not in the verdict"
        );
        swept += 1;
    }
    assert_eq!(
        swept,
        LENGTH - 2,
        "the sweep tampered {swept} of {} entry(ies). This module's own doc says the \
         where-it-sits rule is written three times with one copy tested, and this is \
         that copy: swept at one position it tests one position",
        LENGTH - 2
    );
}

/// Verifying a suffix is clean: its first entry legitimately links backwards.
///
/// **`verify(from)` with a non-zero `from`, from every position.** The tests
/// that go through `broken` call `verify(0)`, so this is where the suffix path
/// — the anchor read, `first.sequence != from`, the reached count — is walked
/// over a history with nothing wrong in it.
///
/// The sibling below holds the head-of-history guard, and a `from == 0`
/// conjunct on it is not an identity: `anchor` is `Some` only when the row
/// at `from - 1` is really there, so a suffix whose boundary row was removed
/// reaches that branch with `from` non-zero — and the conjunct would
/// answer `Ok` over it.
#[tokio::test]
async fn verifying_from_partway_through_an_intact_history_is_clean() {
    let journal = history().await;
    let mut swept = 0_u64;
    for from in 1..LENGTH {
        let reached = journal
            .verify(from)
            .await
            .expect("the store is in memory")
            .unwrap_or_else(|broken| {
                panic!("an untouched history was called broken from {from}: {broken}")
            });
        assert_eq!(
            reached,
            LENGTH - from,
            "verifying from {from} reached {reached} entries of an intact history"
        );
        swept += 1;
    }
    assert_eq!(
        swept,
        LENGTH - 1,
        "the sweep verified from {swept} of {} position(s). A suffix of an intact \
         history is clean from every one of them, and one is not every one",
        LENGTH - 1
    );
}

/// A suffix whose boundary row was removed is not a clean history.
///
/// **A conjunct that would suppress this.** Without this case, deleting it
/// leaves every `nc-journal` test green — in the direction that *reports* the
/// removal. This is the case in the other direction: with `from == 0 &&` in
/// front of the guard, removing row `k` and verifying from `k + 1`
/// answers `Ok`, because `previous` is `None` when the anchor row is missing
/// and the conjunct reads that as "this is the head of the history".
///
/// `NOCTUA.md §15.8` carves the journal out of row-count pruning, so there is no
/// legitimate way for the row before `from` to be absent: a predecessor that is
/// not there was removed.
#[tokio::test]
async fn a_suffix_whose_predecessor_was_removed_is_not_clean() {
    let mut swept = 0_u64;
    for position in 1..LENGTH - 1 {
        let mut journal = history().await;
        journal.store_mut().remove(position);
        let broken = journal
            .verify(position + 1)
            .await
            .expect("the store is in memory")
            .expect_err("a suffix whose predecessor was removed passed as clean");
        assert_eq!(
            broken.reason,
            Reason::LinkDoesNotMatchPredecessor,
            "row {position} was removed and verifying from {} called it {:?}",
            position + 1,
            broken.reason
        );
        swept += 1;
    }
    assert_eq!(
        swept,
        LENGTH - 2,
        "the sweep removed {swept} of {} boundary row(s). §15.8 carves the journal out \
         of row-count pruning, so a predecessor that is not there was removed wherever \
         it sat",
        LENGTH - 2
    );
}

#[tokio::test]
async fn removing_an_entry_breaks_the_chain_where_it_was() {
    // Not the last one: removing the tail is truncation, which a chain of links
    // cannot see and which the test below states plainly rather than hides.
    let mut swept = 0_u64;
    for position in 1..LENGTH - 1 {
        let mut journal = history().await;
        journal.store_mut().remove(position);
        let failure = broken(&journal)
            .await
            .unwrap_or_else(|| panic!("removing entry {position} was not noticed"));
        // **Where the entry was, which is what this test is named for.** Not
        // the number of the entry that *followed* the removal — a number the
        // row itself carries, and therefore one an attacker choosing it
        // decides. The position is `position`: the place the chain stopped
        // being contiguous, which is where the removed entry was.
        assert_eq!(
            failure.sequence, position,
            "the break surfaces where the removed entry was"
        );
        assert_eq!(failure.reason, Reason::SequenceIsNotContiguous);
        swept += 1;
    }
    assert_eq!(
        swept,
        LENGTH - 2,
        "the sweep removed {swept} of {} entry(ies). The break surfaces where the \
         removed entry was, and a history swept at one position says so at one \
         position",
        LENGTH - 2
    );
}

#[tokio::test]
async fn removing_the_very_first_entry_is_noticed() {
    // The one deletion a chain of links alone would miss: every remaining link
    // is still consistent with its neighbour. Two things catch it — the read no
    // longer begins where it was asked to, and the first entry of a whole
    // history must link to nothing — and the first is the one that fires,
    // because it names what actually happened.
    let mut journal = history().await;
    journal.store_mut().remove(0);
    let failure = broken(&journal)
        .await
        .expect("removing the head was not noticed");
    assert_eq!(failure.reason, Reason::HistoryDoesNotStartHere);
}

#[tokio::test]
async fn an_entry_that_links_to_something_at_the_head_of_a_history_is_noticed() {
    // The other guard on the head, on its own: the numbering is intact and only
    // the link is wrong, which is what a forged first entry looks like.
    let mut journal = history().await;
    journal.store_mut().tamper(0, |entry| {
        entry.previous = Some([9; 32]);
        entry.hash = entry.compute_hash();
    });
    let failure = broken(&journal)
        .await
        .expect("a head that links to something is not a head");
    assert_eq!(failure.sequence, 0);
    assert_eq!(failure.reason, Reason::LinkDoesNotMatchPredecessor);
}

#[tokio::test]
async fn a_read_that_returns_nothing_is_not_a_clean_history() {
    // Reported as `Ok(0)`, a caller writing `if verify(0)?.is_ok()` would
    // accept a wiped table — the count reported and never enforced.
    let journal = history().await;
    let wiped = Journal::new(
        ShortStore(journal, 0),
        id("realm-1"),
        id("kernel"),
        minter(),
    );
    let failure = wiped
        .verify(0)
        .await
        .expect("readable")
        .expect_err("an empty read over a non-empty store is not clean");
    assert_eq!(failure.reason, Reason::HistoryDoesNotStartHere);
}

#[tokio::test]
async fn a_read_that_stops_short_of_the_stores_own_tail_is_not_clean() {
    // Fetched and thrown away, the evidence would let a store whose read hid
    // entries in the middle, while still reporting the real last sequence,
    // verify clean over the ones it did return. **One row short as well as
    // five.** The five-row case leaves any tolerance up to four invisible:
    // `if last + 4 < tail.sequence` in `verify` leaves `cargo xtask check`
    // green against it, and one row is the boundary this branch is about — the
    // shape a chain of links cannot see, because every remaining link is still
    // consistent with the one before it. **And the gaps tried, which are the
    // property here.** This list is the whole of what says the tolerance is
    // zero: narrowed to `[5]` the branch `if last + 4 < tail.sequence` is green
    // again and a read four rows short of the tail is a clean history.
    // Collected and asserted, because a count of two is satisfied by two copies
    // of the five-row case.
    let mut tried: Vec<u64> = Vec::new();
    for short_by in [1_u64, 5] {
        let journal = history().await;
        let truncated = Journal::new(
            ShortStore(journal, LENGTH - short_by),
            id("realm-1"),
            id("kernel"),
            minter(),
        );
        let failure = truncated
            .verify(0)
            .await
            .expect("readable")
            .expect_err("a read that stops short of the tail is not a clean history");
        assert_eq!(
            failure.reason,
            Reason::HistoryDoesNotReachTheTail,
            "{short_by} row(s) short came back as {:?}",
            failure.reason
        );
        tried.push(short_by);
    }
    assert_eq!(
        tried,
        [1, 5],
        "the read was cut short by {tried:?} row(s). One row is the boundary this \
         branch is about — a comparison carrying any tolerance at all still \
         refuses a five-row gap, so the five-row case alone proves nothing about \
         the four rows below it"
    );
    assert_eq!(
        tried.first().copied(),
        Some(1),
        "the smallest gap tried was {:?} row(s), so a tolerance of up to {} row(s) \
         would still pass: the comparison has to be exact",
        tried.first(),
        tried.first().copied().unwrap_or(0).saturating_sub(1)
    );
}

#[tokio::test]
async fn a_store_that_forgot_its_tail_while_still_holding_entries_is_not_clean() {
    // The `verify` branch reached when `tail()` says the store is empty while
    // `from()` returns rows — the shape of a truncated index, a replica that
    // lost its last page, or a `tail()` implemented as a separate query that
    // raced the read. Without this, `return Ok(Ok(0))` in its place leaves the
    // whole suite green.
    //
    // `ShortStore(journal, 0)` would be character for character the same as
    // `a_read_that_returns_nothing_is_not_a_clean_history` above, and so would
    // exercise the branch below this one twice.
    let journal = history().await;
    let forgetful = Journal::new(
        ForgetfulStore(journal),
        id("realm-1"),
        id("kernel"),
        minter(),
    );
    let failure = forgetful
        .verify(0)
        .await
        .expect("readable")
        .expect_err("a store holding entries but reporting no tail is not an empty journal");
    assert_eq!(failure.reason, Reason::HistoryDoesNotStartHere);
}

#[tokio::test]
async fn two_journals_mint_different_work_unit_identifiers() {
    // They do not: a `Journal::new` that built its own minter from a hardcoded
    // machine flake of zero would make two journals in one process hand out the
    // same work-unit identifiers — the hazard `Minter::new` documents as the
    // caller's.
    let mut left = Journal::new(InMemory::default(), id("realm-1"), id("kernel"), minter());
    let mut right = Journal::new(InMemory::default(), id("realm-1"), id("kernel"), minter());
    let one = {
        let mut unit = left.begin("one").await.expect("opens");
        unit.commit().await.expect("commits");
        unit.id()
    };
    let other = {
        let mut unit = right.begin("two").await.expect("opens");
        unit.commit().await.expect("commits");
        unit.id()
    };
    assert_ne!(one, other, "two journals handed out one identifier");
}

/// A clock that never moves: every identifier minted on it is of one
/// millisecond.
#[derive(Debug, Clone, Copy)]
struct Frozen;

impl nc_ids::Clock for Frozen {
    fn now_ms(&self) -> u64 {
        1_787_702_400_000
    }

    fn wait_past(&self, _millis: u64) {}
}

/// Journals of two realms sharing their machine's one minter never mint the
/// same work-unit identifier, in one millisecond on one flake — and the same
/// two with minters of their own on that flake do, which is what sharing it is
/// for (`NOCTUA.md §16.2`, one flake for each computer): the kernels of two
/// realms on one machine would number their units alike, and the local file
/// they share keeps a unit by its number.
#[tokio::test]
async fn journals_sharing_their_machines_minter_never_mint_the_same_unit() {
    use std::sync::{Arc, Mutex};

    let flake = nc_ids::MachineFlake::candidate(b"one machine", 0);
    let shared = Arc::new(Mutex::new(nc_ids::Minter::new(flake, Frozen)));
    let mut home = Journal::sharing(
        InMemory::default(),
        id("home"),
        id("kernel"),
        Arc::clone(&shared),
    );
    let mut away = Journal::sharing(InMemory::default(), id("away"), id("kernel"), shared);
    let one = home.open_unit("at home").await.expect("opens");
    let other = away.open_unit("away").await.expect("opens");
    assert_ne!(
        one, other,
        "two journals sharing one minter handed out one identifier"
    );
    let mut home = Journal::new(
        InMemory::default(),
        id("home"),
        id("kernel"),
        nc_ids::Minter::new(flake, Frozen),
    );
    let mut away = Journal::new(
        InMemory::default(),
        id("away"),
        id("kernel"),
        nc_ids::Minter::new(flake, Frozen),
    );
    assert_eq!(
        home.open_unit("at home").await.expect("opens"),
        away.open_unit("away").await.expect("opens"),
        "minters of their own on one flake, in one millisecond, collide: nothing here asks \
         what sharing answers"
    );
}

#[tokio::test]
async fn a_genuinely_empty_journal_verifies() {
    // The other half. Without it, the check above could be satisfied by a
    // `verify` that simply never returns `Ok`.
    let journal = Journal::new(InMemory::default(), id("realm-1"), id("kernel"), minter());
    assert_eq!(journal.verify(0).await.expect("readable"), Ok(0));
}

#[tokio::test]
async fn truncating_the_end_is_not_noticed_and_that_is_stated_rather_than_hidden() {
    // The honest limit. A chain proves nothing was changed without rewriting
    // everything after it; it cannot prove an entry was ever written. Somebody
    // with write access to the whole table can produce a shorter, internally
    // consistent history, and detecting that needs an anchor outside the machine.
    //
    // This test exists so that the limit is written down where somebody would
    // otherwise assume the opposite, and so that a future stage that adds an
    // external anchor has a test to change.
    let mut journal = history().await;
    journal.store_mut().remove(LENGTH - 1);
    assert!(
        broken(&journal).await.is_none(),
        "if this now fails, the journal gained an anchor — update the claim above"
    );
}

#[tokio::test]
async fn a_record_the_caller_wrote_wrong_is_named_before_anything_about_the_store() {
    // **Why the readable check is made twice.** Every `Store` applies it too,
    // so the call in `Journal::append` is behaviour-preserving for a record
    // that is only unreadable — and deleting it would leave the rest of the
    // suite green.
    // What it decides is the order: a record the *caller* built wrong is named
    // as that, before the journal asks the store anything about work units and
    // before a sequence is taken for it.
    //
    // A record that is both too deep and inside a closed unit is the case that
    // tells the two apart. Reported as `UnitClosed`, a caller goes looking at a
    // group that has nothing to do with the resource they nested too far.
    let mut journal = Journal::new(InMemory::default(), id("realm-1"), id("kernel"), minter());
    let mut group = journal.begin("unit").await.expect("begins");
    let unit = group.id();
    group
        .record(Record::new(
            id("user-1"),
            "Read",
            Value::Null,
            Outcome::Allowed,
            1_700_000_000_000,
        ))
        .await
        .expect("records");
    group.commit().await.expect("commits");

    let mut deep = Value::Null;
    for _ in 0..=nc_canon::parse::MAX_NESTING {
        deep = Value::Array(vec![deep]);
    }
    let mut record = Record::new(
        id("user-1"),
        "Read",
        deep,
        Outcome::Allowed,
        1_700_000_000_100,
    );
    record.work_unit = Some(unit);

    let refused = journal.record(record).await;
    assert!(
        matches!(refused, Err(nc_journal::Error::TooDeep { .. })),
        "the caller's own record was not what the refusal named: {refused:?}"
    );
    assert_ne!(
        refused,
        Err(nc_journal::Error::UnitClosed { unit }),
        "a malformed record was reported as somebody else's closed unit"
    );
}

#[tokio::test]
async fn appending_a_forged_entry_is_not_noticed_and_that_is_stated_rather_than_hidden() {
    // **The other honest limit, and the cheaper one.** The truncation above
    // needs write access to the whole table; this needs only `INSERT`. The
    // digest is unkeyed and `entry::DOMAIN` is public, so anyone who can append
    // can read the tail, chain a fabricated entry onto it and write it — and
    // `verify` answers `Ok` about the longer history, because a well-formed
    // chain is exactly what it is.
    //
    // The triggers guard UPDATE, DELETE and TRUNCATE and say nothing about
    // INSERT, by design: appending is what a journal does. Closing this needs
    // the digest to be *keyed*, so that chaining onto the tail takes something
    // an INSERT-only party does not have.
    //
    // Written down here for the same reason as the truncation limit: so that
    // nobody assumes the opposite, and so the stage that keys the digest has a
    // test to change.
    let mut journal = history().await;
    let store = journal.store_mut();
    let (sequence, hash) = store
        .tail()
        .await
        .expect("readable")
        .map(|entry| (entry.sequence, entry.hash))
        .expect("the history is not empty");
    let forged = Record::new(
        id("victim-user"),
        "Delete",
        Value::object([("id", Value::Integer(-1))]),
        Outcome::Allowed,
        1_700_000_000_000 + LENGTH,
    );
    store
        .append(Entry::chain(sequence + 1, Some(hash), forged))
        .await
        .expect("an INSERT-only party can append");

    assert!(
        broken(&journal).await.is_none(),
        "if this now fails, the digest gained a key or the journal gained an \
         anchor — update the claim above and in the crate's header"
    );
    assert_eq!(
        journal.verify(0).await.expect("readable"),
        Ok(LENGTH + 1),
        "the fabricated entry is counted as part of the history, which is the \
         point: it is indistinguishable from one that was written"
    );
}

#[tokio::test]
async fn verification_reports_how_many_entries_it_checked() {
    let journal = history().await;
    assert_eq!(journal.verify(0).await.expect("readable"), Ok(LENGTH));
    assert_eq!(journal.verify(LENGTH - 5).await.expect("readable"), Ok(5));
}

#[tokio::test]
async fn a_store_that_fails_part_way_leaves_what_it_wrote_and_says_so() {
    // A failing store, so the `Unavailable` branch has a test and the error is
    // constructed.
    let mut journal = Journal::new(
        FailingStore::after(3),
        id("realm-1"),
        id("kernel"),
        minter(),
    );
    for step in 0..3 {
        journal
            .record(Record::new(
                id("user-1"),
                "Write",
                Value::Integer(step),
                Outcome::Allowed,
                step.unsigned_abs(),
            ))
            .await
            .expect("the first three land");
    }
    let error = journal
        .record(Record::new(
            id("user-1"),
            "Write",
            Value::Integer(3),
            Outcome::Allowed,
            3,
        ))
        .await
        .expect_err("the fourth does not");
    assert!(matches!(error, nc_journal::Error::Unavailable(_)));
    assert_eq!(error.code(), nc_catalog::ErrorCode::JournalUnavailable);
    // And what did land is still a verifiable chain.
    assert_eq!(journal.verify(0).await.expect("readable"), Ok(3));
}

/// A store whose reads stop at `1`, while its tail still reports the truth.
///
/// What a wiped or partly-hidden table looks like from the journal's side — and
/// the shape whose evidence `verify` holds rather than discards.
#[derive(Debug)]
struct ShortStore(Journal<InMemory>, u64);

impl nc_journal::Store for ShortStore {
    async fn tail(&self) -> Result<Option<Entry>, nc_journal::Error> {
        self.0.store().tail().await
    }
    // Nothing to await: the entry is dropped and the answer is already known.
    fn append(
        &mut self,
        _: Entry,
    ) -> impl core::future::Future<Output = Result<(), nc_journal::Error>> + Send {
        core::future::ready(Ok(()))
    }
    async fn head(&self) -> Result<Option<(u64, [u8; 32])>, nc_journal::Error> {
        self.0.store().head().await
    }
    async fn from(&self, sequence: u64) -> Result<nc_journal::Read, nc_journal::Error> {
        let mut read = self.0.store().from(sequence).await?;
        read.entries
            .truncate(usize::try_from(self.1).unwrap_or(usize::MAX));
        Ok(read)
    }
    async fn unit_opened(
        &self,
        realm: &nc_ids::OpaqueId,
        unit: nc_ids::Snowflake,
    ) -> Result<bool, nc_journal::Error> {
        self.0.store().unit_opened(realm, unit).await
    }

    async fn unit_closing(
        &self,
        realm: &nc_ids::OpaqueId,
        unit: nc_ids::Snowflake,
    ) -> Result<Option<Outcome>, nc_journal::Error> {
        self.0.store().unit_closing(realm, unit).await
    }
}

/// A store that reports no tail while still returning every entry.
///
/// Not a contrived shape: `tail()` is one query and `from()` is another, so any
/// store where the two can disagree can produce it — a lost index page, a
/// replica mid-catch-up, a read that raced a truncation.
#[derive(Debug)]
struct ForgetfulStore(Journal<InMemory>);

impl nc_journal::Store for ForgetfulStore {
    // The three that answer from nothing await nothing.
    fn tail(
        &self,
    ) -> impl core::future::Future<Output = Result<Option<Entry>, nc_journal::Error>> + Send {
        core::future::ready(Ok(None))
    }
    fn append(
        &mut self,
        _: Entry,
    ) -> impl core::future::Future<Output = Result<(), nc_journal::Error>> + Send {
        core::future::ready(Ok(()))
    }
    fn head(
        &self,
    ) -> impl core::future::Future<Output = Result<Option<(u64, [u8; 32])>, nc_journal::Error>> + Send
    {
        core::future::ready(Ok(None))
    }
    async fn from(&self, sequence: u64) -> Result<nc_journal::Read, nc_journal::Error> {
        self.0.store().from(sequence).await
    }
    async fn unit_opened(
        &self,
        realm: &nc_ids::OpaqueId,
        unit: nc_ids::Snowflake,
    ) -> Result<bool, nc_journal::Error> {
        self.0.store().unit_opened(realm, unit).await
    }

    async fn unit_closing(
        &self,
        realm: &nc_ids::OpaqueId,
        unit: nc_ids::Snowflake,
    ) -> Result<Option<Outcome>, nc_journal::Error> {
        self.0.store().unit_closing(realm, unit).await
    }
}

/// A store that stops accepting writes after `limit` of them.
#[derive(Debug)]
struct FailingStore {
    inner: InMemory,
    limit: usize,
}

impl FailingStore {
    fn after(limit: usize) -> Self {
        Self {
            inner: InMemory::default(),
            limit,
        }
    }
}

impl nc_journal::Store for FailingStore {
    async fn tail(&self) -> Result<Option<Entry>, nc_journal::Error> {
        self.inner.tail().await
    }
    async fn append(&mut self, entry: Entry) -> Result<(), nc_journal::Error> {
        if self.inner.len() >= self.limit {
            return Err(nc_journal::Error::Unavailable(
                "this store stops here, on purpose".to_owned(),
            ));
        }
        self.inner.append(entry).await
    }
    async fn head(&self) -> Result<Option<(u64, [u8; 32])>, nc_journal::Error> {
        self.inner.head().await
    }
    async fn from(&self, sequence: u64) -> Result<nc_journal::Read, nc_journal::Error> {
        self.inner.from(sequence).await
    }
    async fn unit_opened(
        &self,
        realm: &nc_ids::OpaqueId,
        unit: nc_ids::Snowflake,
    ) -> Result<bool, nc_journal::Error> {
        self.inner.unit_opened(realm, unit).await
    }

    async fn unit_closing(
        &self,
        realm: &nc_ids::OpaqueId,
        unit: nc_ids::Snowflake,
    ) -> Result<Option<Outcome>, nc_journal::Error> {
        self.inner.unit_closing(realm, unit).await
    }
}

#[tokio::test]
async fn the_hashed_form_is_pinned_by_a_measured_vector() {
    // **The on-disk hash format, pinned.** Without this, all of these leave the
    // whole suite green: renaming `"realm"` to `"realm_id"`, writing
    // `recorded_at` as an integer instead of text, passing a different domain
    // to `nc_canon::hash`, and emptying `DOMAIN`. Each one makes every entry
    // ever written unverifiable, silently — and the versioned-domain rationale
    // in `entry.rs` depends on the shape being pinned.
    //
    // One vector pins all of it at once: the domain, the key names, their
    // order, the u64-as-text encoding and the hex of the link. The constants
    // below are measured from this code, and a deliberate change to the hashed
    // shape is a change to `DOMAIN` **and** to these two lines — which is what
    // makes it deliberate rather than discovered a year later by a chain that
    // no longer verifies.
    let mut record = Record::new(
        id("principal-1"),
        "Read",
        Value::object([("kind", Value::text("content")), ("id", Value::text("c-1"))]),
        Outcome::Allowed,
        1_767_225_600_000,
    );
    record.realm = id("realm-1");
    record.actor_chain = vec![id("agent-1"), id("principal-1")];
    record.deciding_policy = Some("core.no_self_escalation".to_owned());
    record.policy_pack = Some("family@1".to_owned());
    record.policy_epoch = Some(7);
    record.grant_used = Some(id("grant-1"));
    record.work_unit = Some(Snowflake::from_crockford("01J8XK2M9P51").expect("a valid unit"));

    let first = Entry::chain(0, None, record.clone());
    assert_eq!(
        first.hash_hex(),
        "ec9aa6c23f2c9b3d07552b1647bda88cbbe95e156dc4022f9b7e11c378b42686",
        "the hashed shape of an entry changed, and every entry already written is \
         now unverifiable"
    );

    // And chained, so the link is pinned too — a `previous` that stopped
    // entering the hash would leave the first vector correct.
    let second = Entry::chain(1, Some(first.hash), record);
    assert_eq!(
        second.hash_hex(),
        "7ea2fb4223ad6ffbffb54eeb9687acc0b7c76eb426234c74dbd2a5c56b89a2cf",
        "the link no longer enters the hash, so an entry could be moved onto \
         another's predecessor"
    );
}

#[tokio::test]
async fn a_caller_cannot_write_another_realms_history_through_this_journal() {
    // `Record.realm` is a public field, seeded from the principal by
    // `Record::new` and overwritten by `Journal::append`. Without this,
    // deleting that one line would leave the whole suite green — so a caller
    // could set the field and write into another realm's history, which is the
    // thing `Journal::new`'s and `Record::new`'s documentation both say cannot
    // happen.
    //
    // It is not a tidiness rule. `catalog/limits.toml` bounds this table from the
    // policy pack **per realm** (`§15.8`), and a pruner reads this column to know
    // whose retention applies to a row.
    let mut journal = Journal::new(InMemory::default(), id("realm-1"), id("kernel"), minter());
    let mut forged = Record::new(
        id("principal-1"),
        "Read",
        Value::Null,
        Outcome::Allowed,
        1_767_225_600_000,
    );
    forged.realm = id("someone-elses-realm");

    let mut unit = journal.begin("one").await.expect("opens");
    unit.record(forged).await.expect("records");
    unit.commit().await.expect("commits");

    let written = journal.store().all();
    assert!(
        !written.is_empty(),
        "nothing was written, so this proves nothing"
    );
    for entry in written {
        assert_eq!(
            entry.record.realm.to_string(),
            "realm-1",
            "entry {} was written into another realm",
            entry.sequence
        );
    }
}

/// A store whose `tail` is one entry behind what `from` returns.
///
/// **Exactly what a concurrent append looks like to `verify`**, and made
/// deterministic. `verify` reads the tail and then the entries, so a write that
/// lands between the two leaves the read holding more than the tail did — and a
/// comparison of `last != tail.sequence` would call that tampering. Two live
/// queries reproduce it only sometimes; this reproduces it every run, which is
/// what a regression test needs.
struct TailIsOneBehind {
    entries: Vec<Entry>,
}

impl nc_journal::Store for TailIsOneBehind {
    fn tail(
        &self,
    ) -> impl core::future::Future<Output = Result<Option<Entry>, nc_journal::Error>> + Send {
        // The entry before the last one: the tail as it was a moment ago.
        let answer = self.entries.iter().rev().nth(1).cloned();
        async move { Ok(answer) }
    }

    // `-> impl Future` here as above: this one captures nothing, and awaits
    // nothing either, so it says so rather than wrapping a value in an
    // `async` block that never yields.
    fn append(
        &mut self,
        _entry: Entry,
    ) -> impl core::future::Future<Output = Result<(), nc_journal::Error>> + Send {
        core::future::ready(Err(nc_journal::Error::Unavailable("read-only".to_owned())))
    }

    fn head(
        &self,
    ) -> impl core::future::Future<Output = Result<Option<(u64, [u8; 32])>, nc_journal::Error>> + Send
    {
        let answer = self
            .entries
            .last()
            .map(|entry| (entry.sequence, entry.hash));
        async move { Ok(answer) }
    }

    fn from(
        &self,
        sequence: u64,
    ) -> impl core::future::Future<Output = Result<nc_journal::Read, nc_journal::Error>> + Send
    {
        let answer: Vec<Entry> = self
            .entries
            .iter()
            .filter(|entry| entry.sequence >= sequence)
            .cloned()
            .collect();
        async move {
            Ok(nc_journal::Read {
                entries: answer,
                stopped_at: None,
            })
        }
    }

    // This store is read-only and its `append` refuses, so nothing ever asks —
    // and answering "closed" would refuse a write for a reason this fixture is
    // not about. The list is right here, so it answers from it rather than
    // guessing.
    // **Neither of these two ever sees an entry, and that is not a defect.**
    // The trait requires them; no case in this file writes a work unit, so an
    // iterator probe over the whole suite measures both closures running zero
    // times. Written the way the real store writes them anyway — the realm, the
    // unit, the bracket's action and its outcome — so that a case added here
    // gets the same answers, and said out loud so nobody reads them as
    // behaviour this file tests.
    fn unit_opened(
        &self,
        realm: &nc_ids::OpaqueId,
        unit: nc_ids::Snowflake,
    ) -> impl core::future::Future<Output = Result<bool, nc_journal::Error>> + Send {
        let realm = realm.clone();
        let answer = self.entries.iter().any(|entry| {
            entry.record.work_unit == Some(unit)
                && entry.record.realm == realm
                && entry.record.action == nc_journal::unit::OPENS
                && entry.record.result == nc_journal::Outcome::UnitBegun
        });
        async move { Ok(answer) }
    }

    fn unit_closing(
        &self,
        realm: &nc_ids::OpaqueId,
        unit: nc_ids::Snowflake,
    ) -> impl core::future::Future<Output = Result<Option<Outcome>, nc_journal::Error>> + Send {
        let realm = realm.clone();
        let answer = self
            .entries
            .iter()
            .find(|entry| {
                entry.record.work_unit == Some(unit)
                    && entry.record.realm == realm
                    && entry.record.action == nc_journal::unit::CLOSES
                    && matches!(
                        entry.record.result,
                        nc_journal::Outcome::UnitCommitted | nc_journal::Outcome::UnitAborted
                    )
            })
            .map(|entry| entry.record.result);
        async move { Ok(answer) }
    }
}

#[tokio::test]
async fn a_read_that_reaches_past_the_tail_it_saw_is_not_tampering() {
    let built = history().await;
    let entries = built.store().all().to_vec();
    assert!(entries.len() >= 3, "the history is too short to race");

    let racing = Journal::new(
        TailIsOneBehind { entries },
        id("realm-1"),
        id("kernel"),
        minter(),
    );
    let verdict = racing.verify(0).await.expect("the store answers");
    assert!(
        verdict.is_ok(),
        "a read holding one entry more than the tail it saw was called tampering: {verdict:?}"
    );
}

/// Verification compares the whole hash, not a prefix of it.
///
/// **Every other test here survives a comparison over four bytes.** The check
/// is `entry.hash != entry.compute_hash()`, and narrowing it to `[..4]` leaves
/// the rest of the tests in this file green — a real edit changes the prefix
/// with probability essentially one, so every case still fires. What it costs
/// is the property the chain exists for: a forgery then needs about 2³² work,
/// which is seconds, and `verify` calls the result a clean history.
///
/// So this plants a hash that agrees with the true one on its first bytes and
/// differs only late, which is exactly what such a forgery produces and what no
/// ordinary tamper case can reach.
#[tokio::test]
async fn a_hash_that_differs_only_after_its_first_bytes_is_still_a_break() {
    // **And the widths, which are the property and are held here.** The list
    // above is a hand list like any other: narrowed to one element,
    // `entry.hash[..4] != entry.compute_hash()[..4]` in `verify` leaves this
    // file and the whole workspace green, and the forgery this test exists to
    // price at 2²⁵⁶ costs 2³². So the widths are collected and asserted, and
    // the widest of them has to land on the digest's last byte: a comparison
    // stopping anywhere short of it leaves the bytes past that point free.
    let digest = history().await.store().all()[3].hash.len();
    let mut tried: Vec<usize> = Vec::new();
    for shared in [1_usize, 2, 4, 8, 16, 31] {
        let mut journal = history().await;
        journal.store_mut().tamper(3, |entry| {
            // The true hash, with one bit turned over past `shared` bytes. Its
            // first `shared` bytes are the true ones; a comparison that stops
            // there sees nothing wrong.
            let mut planted = entry.compute_hash();
            planted[shared] ^= 0x01;
            entry.hash = planted;
        });
        let failure = broken(&journal).await.unwrap_or_else(|| {
            panic!("a hash sharing its first {shared} byte(s) with the true one was accepted")
        });
        assert_eq!(failure.sequence, 3);
        assert_eq!(failure.reason, Reason::HashDoesNotMatchContents);
        tried.push(shared);
    }
    assert_eq!(
        tried,
        [1, 2, 4, 8, 16, 31],
        "the hash was forged sharing {tried:?} byte(s) with the true one. The \
         widths are what this test is: swept at one width it proves the \
         comparison reaches one byte"
    );
    assert_eq!(
        tried.last().copied(),
        Some(digest - 1),
        "the widest forgery tried agreed on {:?} of the {digest} byte(s) of the \
         digest, so a comparison over the first {} would still pass here and a \
         forgery would cost 2^{} instead of 2^{}",
        tried.last(),
        tried.last().copied().unwrap_or(0),
        tried.last().copied().unwrap_or(0) * 8,
        digest * 8
    );

    // **And the link, which is the same comparison one field along.** A
    // near-miss planted into `entry.previous` with `entry.hash` left alone
    // makes `entry.hash != entry.compute_hash()` fire first, so the assertion
    // (`sequence`, with no `reason`) would be satisfied by the *hash* check
    // every time. Measured that way: narrowing the link comparison to two bytes
    // leaves the tests here and against the live database green, and a forgery
    // ground in 27,687 tries rewrites entry 3 to
    // `action=Escalate, principal=attacker` with **nothing after it touched** —
    // `verify(0)` answers `Ok(6)`.
    //
    // So the entry is re-hashed after the link is planted. Then the hash check
    // cannot fire, the break has to land on the entry *after* it, and the reason
    // has to be the link's.
    // The widths here are the property for the same reason they are above, and
    // this list is the one a two-byte link comparison walks through.
    let mut linked: Vec<usize> = Vec::new();
    for shared in [1_usize, 2, 4, 16, 31] {
        let mut journal = history().await;
        journal.store_mut().tamper(3, |entry| {
            if let Some(mut planted) = entry.previous {
                planted[shared] ^= 0x01;
                entry.previous = Some(planted);
            }
            entry.hash = entry.compute_hash();
        });
        let failure = broken(&journal)
            .await
            .unwrap_or_else(|| panic!("a link differing after {shared} byte(s) was accepted"));
        assert_eq!(
            failure.sequence, 3,
            "the break is at the entry whose link is wrong"
        );
        assert_eq!(
            failure.reason,
            Reason::LinkDoesNotMatchPredecessor,
            "the hash check answered for the link check, which is what made this loop \
             vacuous: it is the link's width that is under test here"
        );
        linked.push(shared);
    }
    assert_eq!(
        linked,
        [1, 2, 4, 16, 31],
        "the link was forged sharing {linked:?} byte(s) with the true one. \
         Narrowing this list is what left `entry.previous` compared over two \
         bytes, and a forgery ground in 27,687 tries"
    );
    assert_eq!(
        linked.last().copied(),
        Some(digest - 1),
        "the widest link forgery tried agreed on {:?} of the {digest} byte(s) of \
         the link, so a comparison over the first {} would still pass here",
        linked.last(),
        linked.last().copied().unwrap_or(0)
    );
}

/// `verify(n)` checks the link of the entry at `n`, not only the ones after it.
///
/// **The head branch fires at `from == 0`, and this fires elsewhere.** A read
/// starting at `from` has no predecessor to compare the first link against, so
/// without the anchor the one entry a checkpointed verification is *about*
/// would be the one entry whose link goes unchecked. Measured that way, on a
/// six-entry history with entry 5 relinked and re-hashed:
///
/// ```text
/// verify(0) -> Err(Broken { sequence: 5, reason: LinkDoesNotMatchPredecessor })
/// verify(5) -> Ok(1)
/// ```
///
/// Verifying incrementally from a checkpoint is what a non-zero `from` is for.
#[tokio::test]
async fn verifying_from_a_checkpoint_checks_the_link_at_the_checkpoint() {
    let mut swept = 0_u64;
    for position in 1..LENGTH {
        let mut journal = history().await;
        journal.store_mut().tamper(position, |entry| {
            entry.previous = Some([0xAA; 32]);
            entry.hash = entry.compute_hash();
        });
        let failure = journal
            .verify(position)
            .await
            .expect("the store is in memory")
            .expect_err("a forged link at the checkpoint was accepted");
        assert_eq!(failure.sequence, position);
        assert_eq!(failure.reason, Reason::LinkDoesNotMatchPredecessor);
        swept += 1;
    }
    assert_eq!(
        swept,
        LENGTH - 1,
        "the sweep forged a link at {swept} of {} checkpoint(s). The one entry a \
         checkpointed verification is about is the entry at `from`, and a sweep \
         narrowed to one checkpoint proves the link at one checkpoint",
        LENGTH - 1
    );

    // And an untouched history still verifies from every position, or the loop
    // above would hold against a `verify` that refuses everything.
    //
    // **Counted too, and this is the worst of the eight.** It is the explicit
    // control for the loop above, so narrowing it takes the floor out from
    // under its own sibling: with `0..1` here, a `verify` that called every
    // history broken from every position but the first satisfies both halves.
    let journal = history().await;
    let mut controlled = 0_u64;
    for position in 0..LENGTH {
        assert_eq!(
            journal.verify(position).await.expect("readable"),
            Ok(LENGTH - position),
            "an untouched history was refused from {position}"
        );
        controlled += 1;
    }
    assert_eq!(
        controlled, LENGTH,
        "the control reached {controlled} of {LENGTH} position(s), and it is the \
         control: it is the whole of what says the sweep above holds against a \
         `verify` that refuses everything"
    );
}

/// A closing bracket is a bracket, and a unit is opened once.
///
/// **Two doors into somebody else's work unit, both measured against a real
/// database.** A `unit_closed` filtering on realm, unit and result and never
/// on `action` lets an ordinary `Write` carrying `Outcome::UnitCommitted` close
/// a group it has nothing to do with:
///
/// ```text
/// seq 0  kernel   kernel.begin  unit-begun
/// seq 1  hostile  Write         unit-committed   <- closes it
/// the owner's next entry -> Err(UnitClosed { … });  verify -> Ok(2)
/// ```
///
/// The group's remaining entries are lost, no `kernel.unit` bracket exists in
/// the history at all, and a reader looking for the close by action finds none
/// while the store says closed. `CLOSES` is named so that "when did this unit
/// end" has one answer.
///
/// And the other bracket: a hand-set `work_unit` with `Outcome::UnitBegun`,
/// unrefused, writes a *second* opening for an open unit, with a different
/// principal and a different label, and `verify` calls the history clean.
#[tokio::test]
async fn a_bracket_is_a_bracket_and_a_unit_is_opened_once() {
    let mut journal = Journal::new(InMemory::default(), id("realm-1"), id("kernel"), minter());
    // The opening bracket is written by hand rather than through `begin`: the
    // handle borrows the journal, and dropping it without `commit` *aborts* —
    // which would close the unit before either rule below is asked. Every write
    // here goes through `Journal::record`, which is the door both rules guard.
    let id_of = nc_ids::Snowflake::from_crockford("2CY4Q0GM0400").expect("a valid identifier");
    let mut opening = Record::new(
        id("kernel"),
        nc_journal::unit::OPENS,
        Value::Text("a migration".to_owned()),
        Outcome::UnitBegun,
        1_699_999_999_999,
    );
    opening.work_unit = Some(id_of);
    journal.record(opening).await.expect("the unit opens");

    // An ordinary entry carrying a bracket's outcome is not a bracket.
    let mut forged = Record::new(
        id("hostile-agent"),
        "Write",
        Value::Integer(9),
        Outcome::UnitCommitted,
        1_700_000_000_000,
    );
    forged.work_unit = Some(id_of);
    journal
        .record(forged)
        .await
        .expect("it is an ordinary entry");

    // So the unit is still open, and its owner can still write into it.
    let mut inside = Record::new(
        id("user-1"),
        "Write",
        Value::Integer(1),
        Outcome::Allowed,
        1_700_000_000_001,
    );
    inside.work_unit = Some(id_of);
    journal
        .record(inside)
        .await
        .expect("an entry that is not a bracket did not close the unit");

    // And it cannot be opened a second time.
    let mut again = Record::new(
        id("hostile-agent"),
        nc_journal::unit::OPENS,
        Value::Text("Delete everything".to_owned()),
        Outcome::UnitBegun,
        1_700_000_000_002,
    );
    again.work_unit = Some(id_of);
    let refused = journal.record(again).await;
    assert!(
        matches!(refused, Err(nc_journal::Error::UnitAlreadyOpen { .. })),
        "a second opening bracket was accepted: {refused:?}"
    );

    // A real bracket does close it, or the assertions above would hold against a
    // journal where nothing closes anything.
    let mut closing = Record::new(
        id("kernel"),
        nc_journal::unit::CLOSES,
        Value::Text("a migration".to_owned()),
        Outcome::UnitCommitted,
        1_700_000_000_003,
    );
    closing.work_unit = Some(id_of);
    journal.record(closing).await.expect("the bracket lands");

    let mut after = Record::new(
        id("user-1"),
        "Write",
        Value::Integer(2),
        Outcome::Allowed,
        1_700_000_000_004,
    );
    after.work_unit = Some(id_of);
    let refused = journal.record(after).await;
    assert!(
        matches!(refused, Err(nc_journal::Error::UnitClosed { .. })),
        "the real bracket did not close the unit: {refused:?}"
    );
}

/// The tail can be rewritten in place, and only a kept head hash sees it.
///
/// **The cheapest of the three attacks the crate header names**: the last entry
/// has nothing after it, so changing it costs no downstream rewrites. Recompute
/// its own hash and `verify` answers `Ok` about a history whose last line now
/// says somebody else did something else.
///
/// This test is the mitigation made concrete rather than promised. `Store::head`
/// returns the sequence and hash of the last entry; a caller that keeps that
/// pair somewhere the table's owner cannot reach — published, countersigned,
/// written to a second store — sees the rewrite even though the chain is
/// internally perfect. Where such a pair is kept is a later stage's choice,
/// which `NOCTUA.md §14.9` leaves open among the ways it lists (`AGENTS.md`,
/// *What is deliberately not claimed*) — the local kernel's `journal.verify`
/// reads the chain and keeps no anchor — and this is the assertion it will be
/// built against.
#[tokio::test]
async fn a_rewritten_tail_is_invisible_to_verify_and_visible_to_a_kept_head() {
    let mut journal = history().await;
    let kept = journal
        .store()
        .head()
        .await
        .expect("the store answers")
        .expect("a history has a head");

    // The whole attack: replace the last entry with one saying something else,
    // and rehash it so the chain is consistent.
    let last = LENGTH - 1;
    journal.store_mut().tamper(last, |entry| {
        entry.record.principal = id("attacker");
        entry.record.action = "Escalate".to_owned();
        entry.hash = entry.compute_hash();
    });

    // `verify` is content, and correctly so: this is a well-formed chain.
    assert!(
        broken(&journal).await.is_none(),
        "the rewritten history does not verify clean, and the point of this test is \
         that it does"
    );

    // The kept head is what notices.
    let now = journal
        .store()
        .head()
        .await
        .expect("the store answers")
        .expect("a history has a head");
    assert_eq!(now.0, kept.0, "the tail is still the same entry");
    assert_ne!(
        now.1, kept.1,
        "the tail was rewritten and the head hash did not move, so a kept anchor \
         would not have seen it either"
    );
}

/// An entry whose hash is not its contents' hash cannot be appended, in either store.
///
/// **The one guard in front of a permanent, unrepairable row,
/// reached.** `Entry`'s fields are all `pub`, so a struct literal builds an
/// entry with any hash at all — no raw SQL, no test-support feature, no
/// privilege. `readable()` checks depth, NUL bytes and integer range, and the
/// hash check beside them is what this reaches. Deleting the
/// whole `if entry.hash != entry.compute_hash()` block without this leaves the
/// entire workspace green, and the consequence is not a wrong answer, it is an
/// outage:
///
/// ```text
/// with the check deleted:
///   InMemory::append(forged hash) -> Ok(())
///   Postgres::append(forged hash) -> Ok(())
///   rows in journal_entries      -> 1
///   append(next honest entry)    -> Ok(())
///   verify(0) -> Broken { sequence: 0, reason: HashDoesNotMatchContents }
/// ```
///
/// The row lands permanently — `DELETE`, `UPDATE` and `TRUNCATE` are all
/// refused by the migrations' triggers — and `verify` condemns the history at
/// it for ever. That is the
/// class `0009_a_hash_is_sixty_four_hex_characters.sql` guards, reached through
/// the public API.
///
/// Driven through `Store::append` on the in-memory store, which is the same
/// `readable()` the PostgreSQL store calls first — and the assertion is on the
/// refusal *and* on the store being untouched, because a store that accepted it
/// and then errored would satisfy the first half alone.
#[tokio::test]
async fn an_entry_whose_hash_is_not_its_contents_is_refused_by_the_store() {
    let honest = nc_journal::Entry::chain(
        0,
        None,
        Record::new(
            id("user-1"),
            "Read",
            Value::object([("id", Value::Integer(1))]),
            Outcome::Allowed,
            1_700_000_000_000,
        ),
    );

    // **And where the forged digest differs from the true one, which is the
    // whole of what this check is worth.** A single fixture of `[0x11; 32]`
    // differs at byte 0: narrowing `readable`'s comparison to
    // `entry.hash[..1] != entry.compute_hash()[..1]` leaves such a test and the
    // whole workspace green, and a forger who matches one byte walks in through
    // the public `Store::append`. So the flipped byte is swept across the
    // digest, the positions are collected, and the last of them has to be the
    // digest's last byte — the same shape, and the same reason, as the width
    // sweeps in
    // `a_hash_that_differs_only_after_its_first_bytes_is_still_a_break` above.
    let width = honest.hash.len();
    let mut flipped: Vec<usize> = Vec::new();
    for at in [0_usize, 1, 4, 16, width - 1] {
        let mut store = InMemory::default();
        let mut planted = honest.hash;
        planted[at] ^= 0x01;
        // Through the door named for what it does: `Entry` is
        // `#[non_exhaustive]`, so a struct literal here would not compile — a
        // hash a caller supplies is a link it did not compute, and this suite
        // is the only place that is ever the point.
        let forged = nc_journal::Entry::forged_for_tests(
            honest.sequence,
            honest.previous,
            honest.record.clone(),
            planted,
        );
        // The floor: the forgery really is one. If `compute_hash` ever returned
        // this the test would be about nothing.
        assert_ne!(
            forged.hash,
            forged.compute_hash(),
            "the fixture's hash is the honest one, so nothing is being forged"
        );

        let refused = match store.append(forged).await {
            Ok(()) => panic!(
                "an entry whose digest differs from its contents at byte {at} was \
                 accepted by the store"
            ),
            Err(refused) => refused,
        };
        // **`Unstorable`, and the code it carries.** Not `Undecodable`, whose
        // own doc reads *"a row is in the table and this code cannot read it —
        // that is tampering"* and whose code is `critical`: a caller handing
        // the store a badly built value would be reported as the audit chain
        // being broken, which pages somebody. Nothing has been stored at this
        // point, and the two sibling checks in the same function say so with
        // `Unstorable`.
        assert!(
            matches!(refused, nc_journal::Error::Unstorable { field: "hash", .. }),
            "a digest forged at byte {at} drew the wrong refusal: {refused:?}"
        );
        assert_eq!(
            refused.code(),
            nc_catalog::ErrorCode::JournalUnstorable,
            "a value refused before any store saw it was reported as the chain being broken"
        );

        // And nothing landed: this is an append-only table, so a row that got in
        // could not be taken out again.
        assert_eq!(
            store.from(0).await.expect("readable").entries.len(),
            0,
            "the entry forged at byte {at} was written and this store has no way \
             to remove it"
        );
        flipped.push(at);
    }
    assert_eq!(
        flipped,
        [0, 1, 4, 16, width - 1],
        "the digest was forged at byte(s) {flipped:?}. A forgery differing at \
         byte 0 is refused by a comparison over one byte, so the positions are \
         what says the comparison reads the whole digest"
    );
    assert_eq!(
        flipped.last().copied(),
        Some(width - 1),
        "the last byte forged was {:?} of a {width}-byte digest, so a comparison \
         over the first {} byte(s) would still pass here",
        flipped.last(),
        flipped.last().copied().unwrap_or(0)
    );

    // The honest entry with the same contents is accepted, so the refusal is
    // about the hash and not about the record.
    let mut store = InMemory::default();
    store.append(honest).await.expect("an honest entry lands");
}

/// The record every entry of the corpus is a departure from.
///
/// Lifted out of the corpus builder so the floor below can compare against it.
/// A named shape is a *relation* to this record — "the same two actors,
/// outermost last" is a claim about two entries and about which field differs —
/// and a floor that could not name this one could only read labels.
fn the_base_record() -> Record {
    let mut record = Record::new(
        id("principal-1"),
        "Read",
        Value::object([("id", Value::Integer(1))]),
        Outcome::Allowed,
        1_700_000_000_000,
    );
    // **The realm, apart from the principal.** `Record::new` copies the
    // principal into the realm — a journal overwrites it on append — and a base
    // record whose two identifier fields hold one value cannot be *exchanged*:
    // an entry named "realm and principal exchanged", set to
    // `realm = principal-1, principal = realm-1` against a base holding
    // `realm-1` in both, would depart in `realm` alone, and the collapse it is
    // named for — a realm rendered as bare text absorbing a principal — be
    // compared against nothing. The field check below is what catches that.
    record.realm = id("realm-1");
    record
}

/// Which fields of `shape` hold something other than `base` holds.
///
/// Every field of `Record`, so that a shape departing in a field this forgot
/// would read as no departure at all. Bound to the derived `PartialEq` at every
/// call site below — an empty answer must mean the two records *are* equal —
/// so a field added to the struct and not to this list fails the moment any
/// corpus entry varies it.
///
/// `String` and not `&'static str`, though every name here is a literal: a
/// function handing back a `&'static str` is a door into the log's type wall
/// whatever it does with it, and `xtask`'s static-string rule reports this
/// signature — correctly, on the shape rather than on the values.
fn where_a_record_departs(shape: &Record, base: &Record) -> Vec<String> {
    every_field_and_whether_it_differs(shape, base)
        .into_iter()
        .filter(|(_, differs)| *differs)
        .map(|(name, _)| name)
        .collect()
}

/// Every field this comparison walks, each with whether the two records differ
/// in it.
///
/// **Lifted out so the field *list* can be read on its own.**
/// [`where_a_record_departs`] only ever hands back the fields some record
/// actually varies, so the names it walks are invisible to a caller unless some
/// entry of the corpus happens to depart in one — which is precisely the case a
/// lost field is not. [`every_field_is_departed_in_by_some_named_shape`] reads
/// this list whole and holds [`MUST_HOLD`] against it.
fn every_field_and_whether_it_differs(shape: &Record, base: &Record) -> Vec<(String, bool)> {
    [
        ("realm", shape.realm != base.realm),
        ("principal", shape.principal != base.principal),
        ("actor_chain", shape.actor_chain != base.actor_chain),
        ("action", shape.action != base.action),
        ("resource", shape.resource != base.resource),
        ("result", shape.result != base.result),
        (
            "deciding_policy",
            shape.deciding_policy != base.deciding_policy,
        ),
        ("policy_pack", shape.policy_pack != base.policy_pack),
        ("policy_epoch", shape.policy_epoch != base.policy_epoch),
        ("grant_used", shape.grant_used != base.grant_used),
        ("work_unit", shape.work_unit != base.work_unit),
        ("recorded_at", shape.recorded_at != base.recorded_at),
    ]
    .into_iter()
    .map(|(name, differs)| (name.to_owned(), differs))
    .collect()
}

/// Records built around every boundary where two could collapse into one.
///
/// Split from the test, which with it inline would cross clippy's line
/// limit. The corpus is the substance: each entry names a way a field's
/// rendering could absorb a neighbour's content, which is what a collision
/// needs.
fn the_shapes_a_flattening_would_collapse() -> Vec<(String, Record)> {
    let base = the_base_record;

    let mut corpus: Vec<(String, Record)> = Vec::new();
    let mut add = |what: &str, record: Record| corpus.push((what.to_owned(), record));

    add("the base record", base());

    // Two fields that could absorb each other if either were rendered as bare
    // text: a principal that looks like a realm, and a realm that looks like a
    // principal.
    let mut swapped = base();
    swapped.realm = id("principal-1");
    swapped.principal = id("realm-1");
    add("realm and principal exchanged", swapped);

    // An actor chain of two, against a principal holding the same two run
    // together — the shape a joined-text rendering collapses.
    let mut chained = base();
    chained.actor_chain = vec![id("a"), id("b")];
    add("an actor chain of two", chained);
    let mut joined = base();
    joined.principal = id("a.b");
    add("a principal spelling the same pair", joined);
    let mut one_actor = base();
    one_actor.actor_chain = vec![id("a.b")];
    add("an actor chain of one, spelled together", one_actor);

    // **The same two actors the other way round, and the same actor twice.**
    // Without these, every multi-actor chain in this file is already in sorted
    // order — this corpus's `[a, b]` and the golden vector's
    // `[agent-1, principal-1]` — and `tamper::edits()` only ever pushes one
    // actor onto an *empty* chain. So a `hashed_form` that sorted the chain, or
    // de-duplicated it, would be byte-identical to the honest one on every
    // input the suite has, and both weakenings pass. Measured with the sorting
    // version:
    //
    // ```text
    // chain A = ["operator-1", "agent-x"]      hash 315e43605fabd8ab…
    // chain B = ["agent-x", "operator-1"]      hash 315e43605fabd8ab…
    // ```
    //
    // `Record::actor_chain`'s own doc is "On whose behalf, outermost first", and
    // `NOCTUA.md §14.6` is delegation and never impersonation — so the order is
    // the difference between an operator acting through an agent and an agent
    // acting as the operator. Collapsed, that swap is forgeable in place: the
    // row still hashes to its stored digest and `verify` answers `Ok`.
    let mut reversed = base();
    reversed.actor_chain = vec![id("b"), id("a")];
    add("the same two actors, outermost last", reversed);
    let mut repeated = base();
    repeated.actor_chain = vec![id("a"), id("a"), id("b")];
    add("an actor appearing twice", repeated);

    // An absent optional against one holding the empty-ish value a flattening
    // would render it as.
    let mut absent = base();
    absent.deciding_policy = None;
    add("no deciding policy", absent);
    let mut empty = base();
    empty.deciding_policy = Some(String::new());
    add("a deciding policy that is empty", empty);
    let mut named = base();
    named.policy_pack = Some(String::new());
    add("an empty policy pack instead", named);

    // Numbers at the boundary where a signed rendering would collapse them.
    //
    // **Both fields that carry one, not one of them.** `entry.rs`'s own note
    // records the collision this encoding prevents —
    // `i64::try_from(x).unwrap_or(i64::MAX)` collapses every value at or above
    // 2^63 onto one, and **two of these fields** carry values that come from
    // outside this crate. With only `recorded_at` varied, that exact saturation
    // on `policy_epoch` alone leaves `cargo xtask check` green over an
    // injectivity claim that is false; the golden vector cannot see it either,
    // because its own `policy_epoch` is small.
    for (what, at) in [
        ("recorded at zero", 0),
        ("recorded at i64::MAX as u64", 9_223_372_036_854_775_807),
        ("recorded one past it", 9_223_372_036_854_775_808),
        ("recorded at u64::MAX", u64::MAX),
    ] {
        let mut record = base();
        record.recorded_at = at;
        add(what, record);
    }
    for (what, epoch) in [
        ("epoch at zero", 0),
        ("epoch at i64::MAX as u64", 9_223_372_036_854_775_807),
        ("epoch one past it", 9_223_372_036_854_775_808),
        ("epoch at u64::MAX", u64::MAX),
    ] {
        let mut record = base();
        record.policy_epoch = Some(epoch);
        add(what, record);
    }

    // **And the two remaining fields that come from outside**, which the corpus
    // also never varied: a work unit and a grant. Both are optional and both are
    // rendered as text, which is the shape an absent-versus-empty flattening
    // collapses.
    let mut unit = base();
    unit.work_unit = Some(nc_ids::Snowflake::from_crockford("01J8XK2M9P51").expect("a unit"));
    add("inside a work unit", unit);
    let mut granted = base();
    granted.grant_used = Some(id("grant-1"));
    add("a grant was used", granted);
    let mut both = base();
    both.grant_used = Some(id("grant-1"));
    both.policy_pack = Some("grant-1".to_owned());
    add("a grant and a pack spelling the same thing", both);

    // The action and the resource, which are the two free-form fields.
    for (what, action) in [
        ("action Read", "Read"),
        ("action Write", "Write"),
        ("action with the resource's text in it", "Read{\"id\":1}"),
    ] {
        let mut record = base();
        action.clone_into(&mut record.action);
        add(what, record);
    }
    // **One record per shape.** `a resource that is the text of the id` and
    // `the same resource written as text` would both hold `Value::text("1")` —
    // two labels over one record, spending on a duplicate nothing declared the
    // one unit of slack the distinct floor carries. The flattening such a label
    // names — the base's `{"id":1}` rendered as the text of its id — is ruled
    // out by the base against that same record, which `VARIANT_PAIRS` names and
    // which therefore has a floor of its own.
    let mut nested = base();
    nested.resource = Value::object([("id", Value::text("1"))]);
    add("an id that is text rather than a number", nested);

    // **Every outcome.** The result is what an attacker most wants to change,
    // so each of the six is an entry of its own here.
    for outcome in [
        Outcome::Allowed,
        Outcome::Denied,
        Outcome::Failed,
        Outcome::UnitBegun,
        Outcome::UnitCommitted,
        Outcome::UnitAborted,
    ] {
        let mut record = base();
        record.result = outcome;
        add(&format!("outcome {outcome:?}"), record);
    }
    corpus.extend(the_shapes_a_normalising_rendering_would_collapse());
    corpus.extend(the_shapes_a_segment_keeping_rendering_would_collapse());
    corpus
}

/// The three spellings of one qualified name, for every field that can hold one.
///
/// **`OpaqueId` admits `.` and `:` on purpose.** Its own doc names `did:key:z…`
/// and `urn:uuid:…` as the candidates `§5.1` is left open for, and without
/// these no value in this apparatus carries a separator — so a `hashed_form`
/// rendering an identifier as one of its *segments* would be the identity on
/// every record here and on every edit `tamper::edits()` makes. Measured that
/// way, `realm` rendered as the part after its last `:`: the whole
/// of `nc-journal` and `nc-canon` green, `cargo xtask check`
/// printing `check passed`, and `urn:uuid:alpha` colliding with `did:web:alpha`
/// on `3a82c132…` with `verify` answering `Ok(4)` over the rewrite. Five more
/// fields the same way.
///
/// Three values close both ends and both separators at once. Two of them differ
/// only *before* the first separator, so every rendering that keeps a suffix —
/// the part after the last `:`, after the last `.`, after either — maps them
/// together; two differ only *after* the last, so every rendering that keeps a
/// prefix does. `recased`'s lesson, one class over: a class needs both ends.
fn the_shapes_a_segment_keeping_rendering_would_collapse() -> Vec<(String, Record)> {
    let base = the_base_record;
    let mut corpus: Vec<(String, Record)> = Vec::new();
    let mut add = |what: &str, record: Record| corpus.push((what.to_owned(), record));

    let mut record = base();
    record.realm = id("urn:uuid.alpha");
    add("a qualified realm name", record);

    let mut record = base();
    record.realm = id("did:uuid.alpha");
    add("the same realm under another authority", record);

    let mut record = base();
    record.realm = id("urn:uuid.beta");
    add("the same realm authority naming another thing", record);

    let mut record = base();
    record.principal = id("urn:uuid.alpha");
    add("a qualified principal name", record);

    let mut record = base();
    record.principal = id("did:uuid.alpha");
    add("the same principal under another authority", record);

    let mut record = base();
    record.principal = id("urn:uuid.beta");
    add("the same principal authority naming another thing", record);

    let mut record = base();
    record.actor_chain = vec![id("urn:uuid.alpha"), id("agent-2")];
    add("a qualified actor chain name", record);

    let mut record = base();
    record.actor_chain = vec![id("did:uuid.alpha"), id("agent-2")];
    add("the same actor chain under another authority", record);

    let mut record = base();
    record.actor_chain = vec![id("urn:uuid.beta"), id("agent-2")];
    add(
        "the same actor chain authority naming another thing",
        record,
    );

    let mut record = base();
    "urn:uuid.alpha".clone_into(&mut record.action);
    add("a qualified action name", record);

    let mut record = base();
    "did:uuid.alpha".clone_into(&mut record.action);
    add("the same action under another authority", record);

    let mut record = base();
    "urn:uuid.beta".clone_into(&mut record.action);
    add("the same action authority naming another thing", record);

    let mut record = base();
    record.resource = Value::text("urn:uuid.alpha");
    add("a qualified resource name", record);

    let mut record = base();
    record.resource = Value::text("did:uuid.alpha");
    add("the same resource under another authority", record);

    let mut record = base();
    record.resource = Value::text("urn:uuid.beta");
    add("the same resource authority naming another thing", record);

    let mut record = base();
    record.deciding_policy = Some("urn:uuid.alpha".to_owned());
    add("a qualified deciding policy name", record);

    let mut record = base();
    record.deciding_policy = Some("did:uuid.alpha".to_owned());
    add("the same deciding policy under another authority", record);

    let mut record = base();
    record.deciding_policy = Some("urn:uuid.beta".to_owned());
    add(
        "the same deciding policy authority naming another thing",
        record,
    );

    let mut record = base();
    record.policy_pack = Some("urn:uuid.alpha".to_owned());
    add("a qualified policy pack name", record);

    let mut record = base();
    record.policy_pack = Some("did:uuid.alpha".to_owned());
    add("the same policy pack under another authority", record);

    let mut record = base();
    record.policy_pack = Some("urn:uuid.beta".to_owned());
    add(
        "the same policy pack authority naming another thing",
        record,
    );

    let mut record = base();
    record.grant_used = Some(id("urn:uuid.alpha"));
    add("a qualified grant name", record);

    let mut record = base();
    record.grant_used = Some(id("did:uuid.alpha"));
    add("the same grant under another authority", record);

    let mut record = base();
    record.grant_used = Some(id("urn:uuid.beta"));
    add("the same grant authority naming another thing", record);

    corpus
}

/// The four fields a Unicode-normalising rendering can collapse.
///
/// Split out because [`the_shapes_a_normalising_rendering_would_collapse`] is at
/// clippy's line limit with these inline — the same reason its neighbours are
/// functions. NFD produces a combining mark where NFC does not, so a title or a
/// principal pasted from one platform and typed on another are two strings that
/// a folding rendering makes one.
fn the_shapes_a_combining_mark_would_collapse() -> Vec<(String, Record)> {
    let base = the_base_record;
    let mut corpus: Vec<(String, Record)> = Vec::new();
    let mut add = |what: &str, record: Record| corpus.push((what.to_owned(), record));

    let mut action_marked = base();
    "Read\u{0301}".clone_into(&mut action_marked.action);
    add("an action carrying a combining mark", action_marked);
    let mut policy_marked = base();
    policy_marked.deciding_policy = Some("core.no_self_escalation\u{0301}".to_owned());
    add(
        "the same deciding policy carrying a combining mark",
        policy_marked,
    );
    let mut pack_marked = base();
    pack_marked.policy_pack = Some("family-1\u{0301}".to_owned());
    add(
        "the same policy pack carrying a combining mark",
        pack_marked,
    );
    let mut resource_marked = base();
    resource_marked.resource = Value::text("Contract-Alice\u{0301}");
    add(
        "the same resource carrying a combining mark",
        resource_marked,
    );

    // **And the same accent written the other way.** Each record above carries
    // a mark on a letter that has no composed form, so a rendering that *joins*
    // a base and its mark — Unicode NFC, which is what somebody adds when they
    // normalise before hashing — would be the identity on all of them. See
    // [`composed`].
    let mut action_apart = base();
    "Cafe\u{301}".clone_into(&mut action_apart.action);
    add("an action whose accent is written apart", action_apart);
    let mut action_together = base();
    "Caf\u{e9}".clone_into(&mut action_together.action);
    add("the same action with the accent composed", action_together);
    let mut policy_apart = base();
    policy_apart.deciding_policy = Some("core.cafe\u{301}".to_owned());
    add(
        "a deciding policy whose accent is written apart",
        policy_apart,
    );
    let mut policy_together = base();
    policy_together.deciding_policy = Some("core.caf\u{e9}".to_owned());
    add(
        "the same deciding policy with the accent composed",
        policy_together,
    );
    let mut pack_apart = base();
    pack_apart.policy_pack = Some("cafe\u{301}-1".to_owned());
    add("a policy pack whose accent is written apart", pack_apart);
    let mut pack_together = base();
    pack_together.policy_pack = Some("caf\u{e9}-1".to_owned());
    add(
        "the same policy pack with the accent composed",
        pack_together,
    );
    let mut resource_apart = base();
    resource_apart.resource = Value::text("Contract-Cafe\u{301}");
    add("a resource whose accent is written apart", resource_apart);
    let mut resource_together = base();
    resource_together.resource = Value::text("Contract-Caf\u{e9}");
    add(
        "the same resource with the accent composed",
        resource_together,
    );

    // **And the variants a rendering could collapse into text.** Every class in
    // `NORMALISATIONS` is a `fn(&str) -> String` reached through `the_text_of`,
    // so a rendering over the *type* is outside the product entirely: measured
    // without these, `hashed_form` turning `Integer`, `Bool` and `Null` into
    // their text gives `check passed` at the baseline verdict line for line,
    // with `Integer(1)` and `Text("1")` on `3218ffca9528cfcf…`, `Bool(true)`
    // and `Text("true")` on `1a14877ac424c154…`, and `Null` and `Text("")` on
    // `f53f6180361eeb31…`. A *nested* variant pair (`{"id":1}` against
    // `{"id":"1"}`) is failed by a recursive collapse and not by a top-level
    // one. What it admits: an attacker who can write only text into `resource`
    // produces the digest of a structured one.
    let mut resource_number = base();
    resource_number.resource = Value::Integer(1);
    add("a resource that is a number", resource_number);
    let mut resource_number_text = base();
    resource_number_text.resource = Value::text("1");
    add("the same resource written as text", resource_number_text);
    let mut resource_flag = base();
    resource_flag.resource = Value::Bool(true);
    add("a resource that is a flag", resource_flag);
    let mut resource_flag_text = base();
    resource_flag_text.resource = Value::text("true");
    add("the same flag written as text", resource_flag_text);
    let mut resource_absent = base();
    resource_absent.resource = Value::Null;
    add("a resource that is absent", resource_absent);
    let mut resource_empty = base();
    resource_empty.resource = Value::text("");
    add("the same absence written as empty text", resource_empty);

    corpus
}

/// Records a rendering that *normalises* an identifier would collapse.
///
/// Split from [`the_shapes_a_flattening_would_collapse`], which with these
/// inline would cross clippy's line limit — the same reason that function is
/// split from its own test.
///
/// A flattening absorbs a neighbour's content; a normalisation throws part of a
/// field's own content away, and no entry above can see one. With every
/// identifier lower-case and shorter than thirteen characters, a `hashed_form`
/// that folded the case of an identifier field, or cut one short, would be the
/// identity on the whole corpus **and** on every edit `tamper::edits()`
/// makes. Measured, seven such weakenings
/// — `realm`, `principal`, `actor_chain`, `deciding_policy`, `policy_pack`
/// and `grant_used` lower-cased, and `realm` and `principal` truncated to
/// twelve characters — pass the whole of `nc-journal` without these and
/// print `check passed` byte-identical to a clean run.
///
/// `nc-ids` accepts `[A-Za-z0-9-_.:]` verbatim and normalises nothing, so
/// `Realm-1` and `realm-1` are two realms: under a folding rendering a stored
/// realm can be respelled to a victim's, or a long principal respelled past its
/// twelfth character, with every digest unmoved and `verify` answering `Ok`.
/// The case pairs are `tamper::edits()`'s other half; the length pairs are the
/// only thing in this file that can see a truncation at all, because an edit
/// that replaces a value replaces its prefix too.
fn the_shapes_a_normalising_rendering_would_collapse() -> Vec<(String, Record)> {
    let base = the_base_record;

    let mut corpus: Vec<(String, Record)> = Vec::new();
    let mut add = |what: &str, record: Record| corpus.push((what.to_owned(), record));

    // The two identifier fields a record is keyed by.
    let mut realm_cased = base();
    realm_cased.realm = id("REALM-1");
    add("a realm respelled in another case", realm_cased);
    let mut principal_cased = base();
    principal_cased.principal = id("PRINCIPAL-1");
    add("a principal respelled in another case", principal_cased);

    let mut long_realm = base();
    long_realm.realm = id("realm-1-under-a-long-name");
    add(
        "a realm that differs only past twelve characters",
        long_realm,
    );
    let mut other_long_realm = base();
    other_long_realm.realm = id("realm-1-under-a-long-alias");
    add("another realm sharing those twelve", other_long_realm);
    let mut long_principal = base();
    long_principal.principal = id("principal-1-under-a-long-name");
    add(
        "a principal that differs only past twelve characters",
        long_principal,
    );
    let mut other_long_principal = base();
    other_long_principal.principal = id("principal-1-under-a-long-alias");
    add(
        "another principal sharing those twelve",
        other_long_principal,
    );

    // The same pair with the case moved, which a chain rendered through a
    // case-folding identifier collapses onto the one above.
    let mut chain_cased = base();
    chain_cased.actor_chain = vec![id("A"), id("B")];
    add("the same two actors in another case", chain_cased);

    // And each of the two named, against itself with the case moved — the only
    // pair a case-folding rendering of these two fields collapses, because
    // every other value they hold here is already lower-case.
    let mut policy = base();
    policy.deciding_policy = Some("core.no_self_escalation".to_owned());
    add("a deciding policy that is named", policy);
    let mut policy_cased = base();
    policy_cased.deciding_policy = Some("CORE.NO_SELF_ESCALATION".to_owned());
    add("the same deciding policy in another case", policy_cased);
    let mut pack = base();
    pack.policy_pack = Some("family-1".to_owned());
    add("a policy pack that is named", pack);
    let mut pack_cased = base();
    pack_cased.policy_pack = Some("FAMILY-1".to_owned());
    add("the same policy pack in another case", pack_cased);

    let mut granted_cased = base();
    granted_cased.grant_used = Some(id("GRANT-1"));
    add("a grant respelled in another case", granted_cased);

    // **And the same four fields with a character outside ASCII.** See
    // `NORMALISATIONS` for the rendering this rules out: with every value in
    // this apparatus ASCII, keeping only ASCII would change nothing anywhere.
    let mut action_wide = base();
    "Read\u{2028}".clone_into(&mut action_wide.action);
    add("an action carrying a character outside ASCII", action_wide);
    let mut policy_wide = base();
    policy_wide.deciding_policy = Some("core.no_self_escalation\u{2028}".to_owned());
    add(
        "the same deciding policy carrying a character outside ASCII",
        policy_wide,
    );
    let mut pack_wide = base();
    pack_wide.policy_pack = Some("family-1\u{2028}".to_owned());
    add(
        "the same policy pack carrying a character outside ASCII",
        pack_wide,
    );
    let mut resource_wide = base();
    resource_wide.resource = Value::text("Contract-Alice\u{2028}");
    add(
        "the same resource carrying a character outside ASCII",
        resource_wide,
    );

    // And the one other field that carries text a caller chose: `resource` is a
    // `Value`, and `Value::Text` is a `String` with no grammar at all.
    let mut resource_spaced = base();
    resource_spaced.resource = Value::text("1 ");
    add("a resource whose text carries whitespace", resource_spaced);

    // **The case class, for the two fields that carry free text too.** Every
    // other `action` in this corpus is `Read`, `Write` or a spelling of the
    // resource, and every other `resource` is `1`, `{"id":1}` or `{"id":"1"}` —
    // all caseless. Measured without these, `hashed_form` rendering
    // `Value::Text` through `to_lowercase`: every `nc-journal` test green, and
    // `Contract-Alice` colliding with `contract-alice` in the field that says
    // *what was acted on*. `action` is caught then only by the golden vector,
    // whose one literal happens to spell `Read` with a capital.
    let mut action_cased = base();
    "READ".clone_into(&mut action_cased.action);
    add("the same action in another case", action_cased);
    let mut resource_named = base();
    resource_named.resource = Value::text("Contract-Alice");
    add("a resource that is named", resource_named);
    let mut resource_cased = base();
    resource_cased.resource = Value::text("contract-alice");
    add("the same resource in another case", resource_cased);

    corpus.extend(the_shapes_a_whitespace_rendering_would_collapse());
    corpus.extend(the_shapes_a_combining_mark_would_collapse());
    corpus.extend(the_longest_values_each_field_admits());
    corpus.extend(the_shapes_a_rendering_reaching_inside_would_collapse());
    corpus.extend(the_shapes_a_deeper_nesting_would_collapse());
    corpus.extend(the_shapes_a_rendering_of_the_keys_would_collapse());
    corpus
}

/// The three fields whose bytes reach the digest as a caller wrote them.
///
/// Its own builder because the class is a *class* — a rendering that touches
/// whitespace anywhere touches all of it — and because the parent would grow
/// past what one function should hold.
fn the_shapes_a_whitespace_rendering_would_collapse() -> Vec<(String, Record)> {
    let base = the_base_record;
    let mut corpus: Vec<(String, Record)> = Vec::new();
    let mut add = |what: &str, record: Record| corpus.push((what.to_owned(), record));

    // **The three fields that are not identifiers.** Every field the parent
    // varies is an `OpaqueId`, whose grammar `[A-Za-z0-9-_.:]` cannot hold a
    // space — so a trimming rendering of those genuinely is the identity, and
    // the case and length pairs are the whole story for them. `action`,
    // `deciding_policy` and `policy_pack` are unconstrained `String`s, and
    // without these every value this file gives them is already trimmed ASCII:
    // measured with these and `entry.rs`'s whitespace edits both absent,
    // `hashed_form` rendering all three through `str::trim` gives
    // `check passed` at the baseline verdict line for line, while two records
    // differing only in a trailing space on `action` land on one digest.
    let mut spaced_action = base();
    "Read ".clone_into(&mut spaced_action.action);
    add("an action carrying whitespace", spaced_action);
    let mut policy_spaced = base();
    policy_spaced.deciding_policy = Some("core.no_self_escalation ".to_owned());
    add(
        "the same deciding policy carrying whitespace",
        policy_spaced,
    );
    let mut pack_spaced = base();
    pack_spaced.policy_pack = Some("family-1 ".to_owned());
    add("the same policy pack carrying whitespace", pack_spaced);

    // **And a tab where the record beside it has a space.** Every other
    // whitespace witness in this apparatus is a single ASCII space at one
    // *end*: `padded` appends one, `led` prepends one, and the values above are
    // `"Read "`, `"core.no_self_escalation "`, `"family-1 "`. So a
    // `hashed_form` normalising a **different** member of the same class would
    // be the identity on all of it — measured, `action.replace('\t', " ")`
    // leaves `nc-journal` green and `check passed` at its baseline without
    // these, with `"Read\tthe file"` and `"Read the file"` on one digest.
    //
    // **A tab is one member and the class has twenty-four.** These pairs stay
    // because the corpus is what `MUST_HOLD` censuses and what every other
    // floor in this file reads; what states the class over the whole alphabet
    // is `every_whitespace_character_this_file_names_is_told_from_a_space`,
    // which quantifies rather than lists. See [`WHITESPACE_ALPHABET`].
    //
    // **Records rather than a `NORMALISATIONS` row**, and that is a decision.
    // A row there is a (field × class) product, and every field admitting the
    // probes would be demanded — `resource` among them, which then needs a
    // witness at every nesting and a key pair. Those cannot be built: the
    // helpers read a resource through `to_canonical`, where a tab is escaped to
    // two characters and no rendering that reads one as a space can collapse
    // anything. So the class is carried by the pairs below, which
    // `no_pair_of_them_lands_on_one_digest` compares directly, and the three
    // fields here are the plain `String`s whose bytes reach the digest as they
    // are.
    let mut action_gapped = base();
    "Read the file".clone_into(&mut action_gapped.action);
    add("an action whose words are parted by a space", action_gapped);
    let mut action_tabbed = base();
    "Read\tthe file".clone_into(&mut action_tabbed.action);
    add("the same action parted by a tab instead", action_tabbed);
    let mut policy_gapped = base();
    policy_gapped.deciding_policy = Some("core.no self_escalation".to_owned());
    add(
        "a deciding policy whose words are parted by a space",
        policy_gapped,
    );
    let mut policy_tabbed = base();
    policy_tabbed.deciding_policy = Some("core.no\tself_escalation".to_owned());
    add(
        "the same deciding policy parted by a tab instead",
        policy_tabbed,
    );
    let mut pack_gapped = base();
    pack_gapped.policy_pack = Some("family 1".to_owned());
    add(
        "a policy pack whose words are parted by a space",
        pack_gapped,
    );
    let mut pack_tabbed = base();
    pack_tabbed.policy_pack = Some("family\t1".to_owned());
    add("the same policy pack parted by a tab instead", pack_tabbed);

    // **And `resource`, which is the field that says what was acted on.** A
    // witness for this class can be built for it: `the_text_of`'s `resource`
    // arm is `Value::Text(text) => text.clone()`, so the top-level witness
    // reads the raw text — which is why the trailing-space witness beside it
    // works — and `with_its_keys_rendered` walks the `Value` tree, so a key
    // pair is constructible too. Measured without these: `hashed_form`
    // rendering `Value::Text` through `text.replace('\t', " ")` leaves every
    // one of `nc-journal`'s tests green and `cargo xtask check` byte-identical
    // to a clean run except for four raw line counters, with `Contract\tAlice`
    // and `Contract Alice` landing on one digest. A front-only `trim_start` is
    // the same.
    //
    // What genuinely cannot be built is the *nested* witness, and the reason is
    // in the rendering rather than in the probe. `to_canonical` escapes a tab to
    // two characters, so a rendering that reads one as a space does not commute
    // with wrapping — `a_rendering_reaches_inside` answers `false` for it, and
    // a class it answers `false` for is asked for no nested witness at all.
    //
    // **The probe carries a tab, and the answer for this rendering is
    // `false`.** The probe is `" A\u{0301}b\tc\u{2028}e\u{0301} "`, so the
    // nested witness is not asked for because the rendering does not commute
    // with wrapping, not because the probe never exercised it.
    let mut resource_gapped = base();
    resource_gapped.resource = Value::text("Contract Alice");
    add(
        "a resource whose text is parted by a space",
        resource_gapped,
    );
    let mut resource_tabbed = base();
    resource_tabbed.resource = Value::text("Contract\tAlice");
    add("the same resource parted by a tab instead", resource_tabbed);
    let mut resource_led = base();
    resource_led.resource = Value::text(" 1");
    add(
        "a resource whose text carries whitespace in front",
        resource_led,
    );

    corpus
}

/// Every character Rust calls whitespace, other than the space itself.
///
/// **The class is more than one character deep.** The pairs above are a tab
/// beside a space, and that closes exactly one member: a newline is in no other
/// value this apparatus holds, `readable_record` refuses only a NUL
/// in `action`, `deciding_policy` and `policy_pack`, and nothing anywhere
/// carries `\r`, a vertical tab, a form feed or any of the Unicode spaces. So
/// a `hashed_form` folding a *different* member of the same class would be the
/// identity on the whole corpus again. Measured, `hashed_form`
/// rendering `action` through `action.replace('\n', " ")`: `nc-journal` green
/// without this, with `Read\nthe file` and `Read the file` both
/// on `cad9d3b88ab85ff6…` while the tab stays apart.
///
/// So the class is stated over the alphabet rather than over the members
/// somebody thought to write, the way [`every_nesting_the_corpus_holds_has_a_witness`]
/// quantifies depth instead of listing it. The table is held against
/// `char::is_whitespace` by the test below, so a character Unicode adds to the
/// property arrives here as a failure rather than as a silence.
const WHITESPACE_ALPHABET: [(&str, char); 24] = [
    ("a tab", '\u{9}'),
    ("a line feed", '\u{a}'),
    ("a vertical tab", '\u{b}'),
    ("a form feed", '\u{c}'),
    ("a carriage return", '\u{d}'),
    ("a next line", '\u{85}'),
    ("a no-break space", '\u{a0}'),
    ("an ogham space mark", '\u{1680}'),
    ("an en quad", '\u{2000}'),
    ("an em quad", '\u{2001}'),
    ("an en space", '\u{2002}'),
    ("an em space", '\u{2003}'),
    ("a three-per-em space", '\u{2004}'),
    ("a four-per-em space", '\u{2005}'),
    ("a six-per-em space", '\u{2006}'),
    ("a figure space", '\u{2007}'),
    ("a punctuation space", '\u{2008}'),
    ("a thin space", '\u{2009}'),
    ("a hair space", '\u{200a}'),
    ("a line separator", '\u{2028}'),
    ("a paragraph separator", '\u{2029}'),
    ("a narrow no-break space", '\u{202f}'),
    ("a medium mathematical space", '\u{205f}'),
    ("an ideographic space", '\u{3000}'),
];

/// The same record, with one word parted by `gap` instead of by a space.
///
/// One builder for the four fields whose type admits a whitespace character:
/// the identifier fields cannot hold one — `OpaqueId`'s grammar is
/// `[A-Za-z0-9-_.:]` — so a pair claimed for them would be a test that cannot
/// fail, which is the same rule [`a_field_admits`] states for the product.
fn the_same_record_parted_by(field: &str, gap: char) -> Record {
    let mut record = the_base_record();
    let text = format!("Read{gap}the file");
    match field {
        "action" => record.action = text,
        "deciding_policy" => record.deciding_policy = Some(text),
        "policy_pack" => record.policy_pack = Some(text),
        "resource" => record.resource = Value::text(text),
        _ => panic!("`{field}` is asked for a whitespace pair and holds no free text"),
    }
    record
}

/// Every character of [`WHITESPACE_ALPHABET`] is told from a space, in every
/// field whose type can hold one.
///
/// The class quantified rather than listed. Each member is checked to be what
/// the table calls it — whitespace, and not the space it stands beside — so the
/// alphabet cannot be padded with characters that make the count larger and the
/// claim no wider, and the set is held against `char::is_whitespace` over the
/// whole of `char`, so nothing can leave it either.
#[test]
fn every_whitespace_character_this_file_names_is_told_from_a_space() {
    use nc_journal::Entry;

    let named: std::collections::BTreeSet<char> =
        WHITESPACE_ALPHABET.iter().map(|(_, gap)| *gap).collect();
    assert_eq!(
        named.len(),
        WHITESPACE_ALPHABET.len(),
        "one character is named twice in `WHITESPACE_ALPHABET`, so the table counts a \
         member it does not have"
    );
    let every: std::collections::BTreeSet<char> = ('\0'..=char::MAX)
        .filter(|character| character.is_whitespace() && *character != ' ')
        .collect();
    assert_eq!(
        named, every,
        "`WHITESPACE_ALPHABET` names one set and `char::is_whitespace` answers another, \
         so a `hashed_form` folding a member this table does not carry would be the \
         identity on every record and every pair in this file"
    );

    for (what, gap) in WHITESPACE_ALPHABET {
        for field in ["action", "deciding_policy", "policy_pack", "resource"] {
            let (spaced, parted) = (
                the_same_record_parted_by(field, ' '),
                the_same_record_parted_by(field, gap),
            );
            assert_eq!(
                where_a_record_departs(&spaced, &parted),
                vec![field.to_owned()],
                "the pair for {what} in `{field}` differs somewhere else as well, so a \
                 rendering that folded {what} onto a space would still tell the two \
                 apart and this member is witnessed by nothing"
            );
            assert_ne!(
                Entry::chain(0, None, spaced).hash,
                Entry::chain(0, None, parted).hash,
                "a record whose `{field}` is parted by {what} lands on the digest of one \
                 parted by a space, so the two rows can be swapped in place: the stored \
                 hash still matches, the link still agrees and `verify` answers `Ok` to \
                 the forgery"
            );
        }
    }
}

/// Resources that are structured, varying only *inside* the nesting.
///
/// **Inside the nesting, two ways.** Every other structured value in the whole
/// apparatus is `{"id":1}`, `{"id":"1"}` or the golden vector's two lower-case
/// texts, `tamper::edits()` writes a top-level `Value::text`, and no
/// other `resource` holds a list at all. So a `hashed_form` that reached one
/// level down would be the identity on all of it. Measured, each with every
/// other test of this file green:
///
/// ```text
/// a nested Value::Text lower-cased
///   {"id":"Contract-Alice"} and {"id":"contract-alice"} -> 7f600b2bfbe54d68…
/// a nested Value::Array sorted and de-duplicated
///   {"id":["a","b"]} and {"id":["b","a"]}               -> 9556c5009fc2a5c5…
/// ```
///
/// `resource` is the field that says what was acted on and a caller may put any
/// `Value` in it, so both are forgeable in place: the row still hashes to its
/// stored digest and `verify` answers `Ok`.
fn the_shapes_a_rendering_reaching_inside_would_collapse() -> Vec<(String, Record)> {
    let base = the_base_record;
    let mut corpus: Vec<(String, Record)> = Vec::new();
    let mut add = |what: &str, record: Record| corpus.push((what.to_owned(), record));

    let inside = |text: &str| Value::object([("id", Value::text(text))]);
    let mut named = base();
    named.resource = inside("Contract-Alice");
    add("a nested resource that is named", named);
    let mut cased = base();
    cased.resource = inside("contract-alice");
    add("the same nested resource in another case", cased);
    let mut marked = base();
    marked.resource = inside("Contract-Alice\u{0301}");
    add("a nested resource carrying a combining mark", marked);
    let mut wide = base();
    wide.resource = inside("Contract-Alice\u{2028}");
    add("a nested resource carrying a character outside ASCII", wide);
    // And the composed spelling, for the same reason the top-level pair exists.
    let mut apart = base();
    apart.resource = inside("Contract-Cafe\u{301}");
    add("a nested resource whose accent is written apart", apart);
    let mut together = base();
    together.resource = inside("Contract-Caf\u{e9}");
    add(
        "the same nested resource with the accent composed",
        together,
    );

    // The order and the repeats one level down, which no class of
    // `NORMALISATIONS` can state: a rendering that sorts or de-duplicates a
    // nested list leaves every character of it in place, so the pair is a
    // relation between two records and not a rendering of either.
    let list = |items: &[&str]| {
        Value::object([(
            "id",
            Value::Array(items.iter().copied().map(Value::text).collect()),
        )])
    };
    let mut ordered = base();
    ordered.resource = list(&["a", "b"]);
    add("a nested list of two", ordered);
    let mut reversed = base();
    reversed.resource = list(&["b", "a"]);
    add("the same nested list, the other way round", reversed);
    let mut repeated = base();
    repeated.resource = list(&["a", "a", "b"]);
    add("a nested list repeating a value", repeated);

    corpus
}

/// Resources whose text sits deeper than one container, and inside a list.
///
/// **Further in than one container.** Every structured resource in
/// [`the_shapes_a_rendering_reaching_inside_would_collapse`] varies a direct
/// string value of the top-level object, and the only text the corpus holds any
/// deeper is the `a` and `b` of a nested list, which no pair respells — so
/// without these a `hashed_form` that reached one container further in would be
/// the identity on every record here and on every edit `tamper::edits()`
/// makes. Measured, `hashed_form` lower-casing every text more than one
/// container down: every other test of this file and `nc-journal`'s library
/// green, with
///
/// ```text
/// {"id":["Contract-Alice"]}          and {"id":["contract-alice"]}
/// {"target":{"id":"Contract-Alice"}} and {"target":{"id":"contract-alice"}}
/// ```
///
/// on one digest apiece. `resource` is the field that says what was acted on and
/// a caller may put a `Value` of any shape in it, so both are forgeable in
/// place: the row still hashes to its stored digest and `verify` answers `Ok`.
///
/// Four spellings at each nesting, not one pair: the renderings that reach
/// inside a container are the three that [`a_rendering_reaches_inside`] answers
/// for, and [`every_nesting_the_corpus_holds_has_a_witness`] asks each of them
/// for a pair at every nesting the corpus keeps a text in.
fn the_shapes_a_deeper_nesting_would_collapse() -> Vec<(String, Record)> {
    let base = the_base_record;
    let mut corpus: Vec<(String, Record)> = Vec::new();
    let mut add = |what: &str, record: Record| corpus.push((what.to_owned(), record));

    let under_two =
        |text: &str| Value::object([("target", Value::object([("id", Value::text(text))]))]);
    let in_a_list = |text: &str| Value::object([("id", Value::Array(vec![Value::text(text)]))]);
    for (how, spelling) in [
        ("that is named", "Contract-Alice"),
        ("in another case", "contract-alice"),
        ("carrying a combining mark", "Contract-Alice\u{0301}"),
        (
            "carrying a character outside ASCII",
            "Contract-Alice\u{2028}",
        ),
        // Six spellings: `composed` reaches inside a container as the three
        // above do, so `every_nesting_the_corpus_holds_has_a_witness` asks it
        // for a pair at every nesting too.
        ("whose accent is written apart", "Contract-Cafe\u{301}"),
        ("with the same accent composed", "Contract-Caf\u{e9}"),
    ] {
        let mut deeper = base();
        deeper.resource = under_two(spelling);
        add(&format!("a resource two containers down {how}"), deeper);
        let mut listed = base();
        listed.resource = in_a_list(spelling);
        add(&format!("a resource inside a list {how}"), listed);
    }

    corpus
}

/// Resources that differ in the key of a nested object and nowhere else.
///
/// **A key, which no rendering of a text can state.** Every other nested object
/// in this file is keyed `id` and the golden vector's are keyed `kind` and `id`
/// — all lower-case ASCII — so a `hashed_form` that normalised the *keys* of a
/// structured resource would be the identity on the whole
/// apparatus. Measured, `hashed_form` lower-casing every key of the value a
/// caller supplied: every other test of this file and `nc-journal`'s library
/// green, with `{"Owner":"alice"}` and `{"owner":"alice"}` on one digest.
///
/// A key says which part of the resource was acted on, so the collapse renames
/// what the entry is about while the row still hashes to its stored digest and
/// `verify` answers `Ok`. One plain spelling stands against all four variants,
/// which is what makes each pair a claim about the key and not about what is
/// under it — see [`the_corpus_can_see_a_rendering_of_the_keys`].
fn the_shapes_a_rendering_of_the_keys_would_collapse() -> Vec<(String, Record)> {
    let base = the_base_record;
    let mut corpus: Vec<(String, Record)> = Vec::new();
    let mut add = |what: &str, record: Record| corpus.push((what.to_owned(), record));

    let keyed = |key: &str| Value::object([(key, Value::text("alice"))]);
    let mut plain = base();
    plain.resource = keyed("owner");
    add("a resource keyed plainly", plain);
    for (how, key) in [
        ("in another case", "Owner"),
        ("with whitespace", "owner "),
        // **And a key whose whitespace is a tab where the one beside it is a
        // space.** Every other whitespace witness in this apparatus is a single
        // space at one *end*, so a rendering that normalised a different member
        // of the class — `key.replace('\t', " ")` — would be the identity on
        // all of it. `with_its_keys_rendered` walks the `Value` tree rather
        // than canonical text, so a key pair can be built, and this is it.
        ("with a space inside it", "owner name"),
        ("with a tab inside it instead", "owner\tname"),
        ("with a combining mark", "owner\u{0301}"),
        // **And a key whose accent is written apart from a key that composes
        // it**, because `NORMALISATIONS` renders a resource that way too, and
        // every class that reaches a value has to reach a key.
        ("with an accent written apart", "owner-cafe\u{301}"),
        ("with the same accent composed", "owner-caf\u{e9}"),
        ("with a character outside ASCII", "owner\u{2028}"),
        // **And a key that is a qualified name**, which a key apparatus with
        // no separator anywhere cannot see, for the same reason a value
        // apparatus cannot. These two differ from the pair above at one end each, so a
        // rendering of the *keys* that keeps a segment collapses one of them.
        ("with a qualified name", "urn:uuid.owner"),
        ("under another authority", "did:uuid.owner"),
        ("naming another thing", "urn:uuid.holder"),
    ] {
        let mut varied = base();
        varied.resource = keyed(key);
        add(&format!("the same resource keyed {how}"), varied);
    }

    corpus
}

/// A pair per text field, as long as the field admits and differing only in the
/// final character.
///
/// **Every length, not twelve characters over two fields.** With the only other
/// length pairs diverging at character 25 and nothing pinning a longer prefix,
/// a `hashed_form` that cut every text field to twenty-six characters would be
/// the identity on the whole corpus. Measured, exactly that: every `nc-journal`
/// test green, and `cargo xtask check` against a clean run moving *only* the
/// four raw line-counters, by the lines the plant added — `check passed`,
/// identical. The collisions it admits:
///
/// ```text
/// realm       acme-production-eu-west-1-a / -b        one digest
/// principal   svc.reporting.read-only.v1a / v1z       one digest
/// deciding_policy  core.no_self_escalation.v1 / .v2   one digest
/// grant_used  grant.break-glass.read-2026 / 2027      one digest
/// ```
///
/// `deciding_policy` is *which policy allowed it* and `principal` is *who*.
///
/// So the pairs are as long as the type admits — `OPAQUE_ID_MAX` is 128 — and
/// they differ in the last character alone, which is what makes a cut at every
/// shorter length collapse them. `the_corpus_can_see_a_truncation_at_every_length`
/// then asks for a witness at every length up to the longest value each field
/// holds, rather than at one length somebody chose.
fn the_longest_values_each_field_admits() -> Vec<(String, Record)> {
    let base = the_base_record;
    let mut corpus: Vec<(String, Record)> = Vec::new();
    let mut add = |what: &str, record: Record| corpus.push((what.to_owned(), record));

    // One character short of the bound, so the pair differs inside it.
    let stem: String = std::iter::repeat_n('a', nc_ids::OPAQUE_ID_MAX - 1).collect();
    let one = format!("{stem}1");
    let two = format!("{stem}2");

    let mut long_realm = base();
    long_realm.realm = id(&one);
    add("a realm as long as the type admits", long_realm);
    let mut other_realm = base();
    other_realm.realm = id(&two);
    add("another realm differing in its last character", other_realm);

    let mut long_principal = base();
    long_principal.principal = id(&one);
    add("a principal as long as the type admits", long_principal);
    let mut other_principal = base();
    other_principal.principal = id(&two);
    add(
        "another principal differing in its last character",
        other_principal,
    );

    let mut long_chain = base();
    long_chain.actor_chain = vec![id(&one)];
    add("an actor as long as the type admits", long_chain);
    let mut other_chain = base();
    other_chain.actor_chain = vec![id(&two)];
    add("another actor differing in its last character", other_chain);

    let mut long_grant = base();
    long_grant.grant_used = Some(id(&one));
    add("a grant as long as the type admits", long_grant);
    let mut other_grant = base();
    other_grant.grant_used = Some(id(&two));
    add("another grant differing in its last character", other_grant);

    let mut long_action = base();
    long_action.action.clone_from(&one);
    add("an action as long as the type admits", long_action);
    let mut other_action = base();
    other_action.action.clone_from(&two);
    add(
        "another action differing in its last character",
        other_action,
    );

    let mut long_policy = base();
    long_policy.deciding_policy = Some(one.clone());
    add("a deciding policy as long as the type admits", long_policy);
    let mut other_policy = base();
    other_policy.deciding_policy = Some(two.clone());
    add(
        "another deciding policy differing in its last character",
        other_policy,
    );

    let mut long_pack = base();
    long_pack.policy_pack = Some(one.clone());
    add("a policy pack as long as the type admits", long_pack);
    let mut other_pack = base();
    other_pack.policy_pack = Some(two.clone());
    add(
        "another policy pack differing in its last character",
        other_pack,
    );

    let mut long_resource = base();
    long_resource.resource = Value::text(one);
    add("a resource as long as the type admits", long_resource);
    let mut other_resource = base();
    other_resource.resource = Value::text(two);
    add(
        "another resource differing in its last character",
        other_resource,
    );

    // **And the fields whose type is a number.** They reach the hash as text —
    // `epoch.to_string()`, `unit.to_crockford()`, `recorded_at.to_string()` —
    // so a cut applies to them exactly as it does to an identifier, and without
    // these the corpus holds nothing long enough to see one. `u64::MAX` is
    // twenty digits and only that one value has twenty, so a cut at nineteen
    // would be the identity on every epoch here.
    let mut long_epoch = base();
    long_epoch.policy_epoch = Some(18_446_744_073_709_551_610);
    add("an epoch as long as the type admits", long_epoch);
    let mut other_epoch = base();
    other_epoch.policy_epoch = Some(18_446_744_073_709_551_611);
    add("another epoch differing in its last digit", other_epoch);

    let mut long_recorded = base();
    long_recorded.recorded_at = 18_446_744_073_709_551_610;
    add("a timestamp as long as the type admits", long_recorded);
    let mut other_recorded = base();
    other_recorded.recorded_at = 18_446_744_073_709_551_611;
    add(
        "another timestamp differing in its last digit",
        other_recorded,
    );

    // **Thirteen characters, not twelve.** A work unit is sixty-three bits and
    // Crockford writes five to a digit, so its widest rendering is thirteen —
    // and twelve is the width `to_crockford` pads *up* to, so a cut at twelve
    // would be the identity on every work unit in a corpus of twelve.
    // `the_reach_a_field_must_have` measures that width by rendering the widest
    // value the layout holds, so shortening these two fails there.
    let mut long_unit = base();
    long_unit.work_unit =
        Some(nc_ids::Snowflake::from_crockford("7ZZZZZZZZZZZZ").expect("a test identifier"));
    add("a work unit as long as the type admits", long_unit);
    let mut other_unit = base();
    other_unit.work_unit =
        Some(nc_ids::Snowflake::from_crockford("7ZZZZZZZZZZZY").expect("a test identifier"));
    add(
        "another work unit differing in its last character",
        other_unit,
    );

    corpus
}

/// Every collapse this corpus exists to rule out is still in it.
///
/// **A count cannot carry a set.** `corpus.len() > 15` over hand-written
/// entries is satisfied by any sixteen of them, and the ones a weakening needs
/// gone are reached first: delete the two actor-order entries from
/// [`the_shapes_a_flattening_would_collapse`] and `hashed_form` may sort the
/// chain, at which point
///
/// ```text
/// chain A = ["operator-1", "agent-x"]      hash 315e43605fabd8ab…
/// chain B = ["agent-x", "operator-1"]      hash 315e43605fabd8ab…
/// ```
///
/// — the swap is forgeable in place, the row still hashes to its stored
/// digest, `verify` answers `Ok`, and `cargo xtask check` is byte-identical to
/// a clean run. Nothing else sees it: the golden vector's chain is already
/// sorted, and `tamper::edits()` only ever pushes one actor onto an *empty*
/// chain.
///
/// So the shapes are named, the way `every_field_of_a_record_has_an_edit` names
/// `FIELDS` against the struct rather than counting them. Written out rather
/// than derived, so an entry renamed in the corpus and not here is a failure
/// rather than a silent agreement: adding a shape needs a line here, and
/// removing one cannot be done quietly.
///
/// **And the names are still only names.** A label pins nothing about the
/// record filed under it: measured with no field read, `reversed.actor_chain`
/// given `[b, c]` instead of `[b, a]` — so the entry called "the same two
/// actors, outermost last" is the reverse of no other entry in the corpus —
/// and `hashed_form` sorting the chain leave every line of `cargo xtask check`
/// identical to a clean run. So each entry of [`MUST_HOLD`] carries
/// the fields its name claims to vary, compared against [`the_base_record`],
/// and the two chain entries are compared to each other.
const MUST_HOLD: [(&str, &[&str]); 160] = [
    // The record everything else is a departure from.
    ("the base record", &[]),
    // Two identifier fields that could absorb each other as bare text.
    ("realm and principal exchanged", &["realm", "principal"]),
    // **The same field respelled, and nothing else moved.** With every other
    // identifier lower-case and under thirteen characters, a folding or
    // truncating `hashed_form` would be the identity on the whole corpus and on
    // every edit `tamper::edits()` makes. The two length pairs are the only
    // records in this file that a truncation collapses.
    ("a realm respelled in another case", &["realm"]),
    ("a principal respelled in another case", &["principal"]),
    (
        "a realm that differs only past twelve characters",
        &["realm"],
    ),
    ("another realm sharing those twelve", &["realm"]),
    (
        "a principal that differs only past twelve characters",
        &["principal"],
    ),
    ("another principal sharing those twelve", &["principal"]),
    // An actor chain against a principal holding the same pair joined.
    ("an actor chain of two", &["actor_chain"]),
    ("a principal spelling the same pair", &["principal"]),
    ("an actor chain of one, spelled together", &["actor_chain"]),
    // **The two that decide the order of the chain**, and so the difference
    // between an operator acting through an agent and an agent acting as
    // the operator — `Record::actor_chain` is "On whose behalf, outermost
    // first" and `NOCTUA.md §14.6` is delegation and never impersonation.
    ("the same two actors, outermost last", &["actor_chain"]),
    ("an actor appearing twice", &["actor_chain"]),
    ("the same two actors in another case", &["actor_chain"]),
    // An absent optional against the empty-ish value a flattening renders
    // it as, and a neighbour that can hold that value.
    ("no deciding policy", &[]),
    ("a deciding policy that is empty", &["deciding_policy"]),
    ("an empty policy pack instead", &["policy_pack"]),
    ("a deciding policy that is named", &["deciding_policy"]),
    (
        "the same deciding policy in another case",
        &["deciding_policy"],
    ),
    ("a policy pack that is named", &["policy_pack"]),
    ("the same policy pack in another case", &["policy_pack"]),
    // **Both fields carrying a number from outside this crate**, at every
    // boundary where a signed rendering would collapse them. Varying one and
    // not the other lets a saturating cast on `policy_epoch` stay green over an
    // injectivity claim that is false.
    ("recorded at zero", &["recorded_at"]),
    ("recorded at i64::MAX as u64", &["recorded_at"]),
    ("recorded one past it", &["recorded_at"]),
    ("recorded at u64::MAX", &["recorded_at"]),
    ("epoch at zero", &["policy_epoch"]),
    ("epoch at i64::MAX as u64", &["policy_epoch"]),
    ("epoch one past it", &["policy_epoch"]),
    ("epoch at u64::MAX", &["policy_epoch"]),
    // The other two fields that come from outside, both optional and both
    // rendered as text.
    ("inside a work unit", &["work_unit"]),
    ("a grant was used", &["grant_used"]),
    ("a grant respelled in another case", &["grant_used"]),
    (
        "a grant and a pack spelling the same thing",
        &["policy_pack", "grant_used"],
    ),
    // The two free-form fields, including an action spelling the resource.
    ("action Read", &[]),
    ("action Write", &["action"]),
    ("action with the resource's text in it", &["action"]),
    ("an id that is text rather than a number", &["resource"]),
    // **Every outcome.** The result is what an attacker most wants to change,
    // so each of the six is named here.
    ("outcome Allowed", &[]),
    ("outcome Denied", &["result"]),
    ("outcome Failed", &["result"]),
    ("outcome UnitBegun", &["result"]),
    ("outcome UnitCommitted", &["result"]),
    ("outcome UnitAborted", &["result"]),
    // **The whitespace class**, which no entry above can see: every other value
    // the corpus holds for these four is already trimmed, so a `hashed_form`
    // that trimmed them would be the identity on the whole of it.
    ("an action carrying whitespace", &["action"]),
    (
        "the same deciding policy carrying whitespace",
        &["deciding_policy"],
    ),
    ("the same policy pack carrying whitespace", &["policy_pack"]),
    // **A tab where the record beside it has a space**, which the trimming rows
    // above cannot hold: a trim reaches the ends, and a space cannot witness a
    // tab. `hashed_form` reading one as the other would otherwise be the
    // identity on the whole corpus and on every edit `tamper::edits()` makes.
    ("an action whose words are parted by a space", &["action"]),
    ("the same action parted by a tab instead", &["action"]),
    (
        "a deciding policy whose words are parted by a space",
        &["deciding_policy"],
    ),
    (
        "the same deciding policy parted by a tab instead",
        &["deciding_policy"],
    ),
    (
        "a policy pack whose words are parted by a space",
        &["policy_pack"],
    ),
    (
        "the same policy pack parted by a tab instead",
        &["policy_pack"],
    ),
    ("a resource whose text carries whitespace", &["resource"]),
    ("a resource whose text is parted by a space", &["resource"]),
    ("the same resource parted by a tab instead", &["resource"]),
    (
        "a resource whose text carries whitespace in front",
        &["resource"],
    ),
    (
        "the same resource keyed with a space inside it",
        &["resource"],
    ),
    (
        "the same resource keyed with a tab inside it instead",
        &["resource"],
    ),
    // **The case class, for the two free-text fields too**: every other value
    // they hold here is caseless. See the corpus for the collision that admits.
    ("the same action in another case", &["action"]),
    ("a resource that is named", &["resource"]),
    ("the same resource in another case", &["resource"]),
    // **The longest value each field admits, and its neighbour one character
    // different.** With the only other length pairs diverging at character 25,
    // a cut at twenty-six would be the identity on the whole corpus.
    ("a realm as long as the type admits", &["realm"]),
    ("another realm differing in its last character", &["realm"]),
    ("a principal as long as the type admits", &["principal"]),
    (
        "another principal differing in its last character",
        &["principal"],
    ),
    ("an actor as long as the type admits", &["actor_chain"]),
    (
        "another actor differing in its last character",
        &["actor_chain"],
    ),
    ("a grant as long as the type admits", &["grant_used"]),
    (
        "another grant differing in its last character",
        &["grant_used"],
    ),
    ("an action as long as the type admits", &["action"]),
    (
        "another action differing in its last character",
        &["action"],
    ),
    (
        "a deciding policy as long as the type admits",
        &["deciding_policy"],
    ),
    (
        "another deciding policy differing in its last character",
        &["deciding_policy"],
    ),
    ("a policy pack as long as the type admits", &["policy_pack"]),
    (
        "another policy pack differing in its last character",
        &["policy_pack"],
    ),
    ("a resource as long as the type admits", &["resource"]),
    (
        "another resource differing in its last character",
        &["resource"],
    ),
    // The numeric fields, whose rendering is text and whose corpus held nothing
    // long enough to see a cut.
    ("an epoch as long as the type admits", &["policy_epoch"]),
    (
        "another epoch differing in its last digit",
        &["policy_epoch"],
    ),
    ("a timestamp as long as the type admits", &["recorded_at"]),
    (
        "another timestamp differing in its last digit",
        &["recorded_at"],
    ),
    ("a work unit as long as the type admits", &["work_unit"]),
    (
        "another work unit differing in its last character",
        &["work_unit"],
    ),
    // The class that keeps only ASCII, which every value here satisfied.
    ("an action carrying a character outside ASCII", &["action"]),
    (
        "the same deciding policy carrying a character outside ASCII",
        &["deciding_policy"],
    ),
    (
        "the same policy pack carrying a character outside ASCII",
        &["policy_pack"],
    ),
    (
        "the same resource carrying a character outside ASCII",
        &["resource"],
    ),
    // The class a Unicode-normalising rendering collapses.
    ("an action carrying a combining mark", &["action"]),
    (
        "the same deciding policy carrying a combining mark",
        &["deciding_policy"],
    ),
    (
        "the same policy pack carrying a combining mark",
        &["policy_pack"],
    ),
    ("the same resource carrying a combining mark", &["resource"]),
    // The variants a rendering over the type could collapse into text — outside
    // the (field × text-class) product entirely, because every class is a
    // `fn(&str) -> String`.
    ("a resource that is a number", &["resource"]),
    ("the same resource written as text", &["resource"]),
    ("a resource that is a flag", &["resource"]),
    ("the same flag written as text", &["resource"]),
    ("a resource that is absent", &["resource"]),
    ("the same absence written as empty text", &["resource"]),
    // **The shapes that vary one level down.** Every other structured value
    // here is `{"id":1}` or `{"id":"1"}` and no other resource holds a list at
    // all, so a rendering that reached inside a nesting would be the identity
    // on the whole corpus. See
    // `the_shapes_a_rendering_reaching_inside_would_collapse` for the two
    // digests that measures.
    ("a nested resource that is named", &["resource"]),
    ("the same nested resource in another case", &["resource"]),
    ("a nested resource carrying a combining mark", &["resource"]),
    (
        "a nested resource carrying a character outside ASCII",
        &["resource"],
    ),
    ("a nested list of two", &["resource"]),
    ("the same nested list, the other way round", &["resource"]),
    ("a nested list repeating a value", &["resource"]),
    // **The shapes that vary further in than one container.** The three rows of
    // `NESTED_PAIRS` all vary a direct string value of the top-level object, so
    // a rendering that reached a text inside a list, or under a second object,
    // would be the identity on the whole corpus without these. See
    // `the_shapes_a_deeper_nesting_would_collapse` for the two digests that
    // measures, and `every_nesting_the_corpus_holds_has_a_witness` for the
    // floor that asks each nesting for its own pairs.
    (
        "a resource two containers down that is named",
        &["resource"],
    ),
    (
        "a resource two containers down in another case",
        &["resource"],
    ),
    (
        "a resource two containers down carrying a combining mark",
        &["resource"],
    ),
    (
        "a resource two containers down carrying a character outside ASCII",
        &["resource"],
    ),
    ("a resource inside a list that is named", &["resource"]),
    ("a resource inside a list in another case", &["resource"]),
    (
        "a resource inside a list carrying a combining mark",
        &["resource"],
    ),
    (
        "a resource inside a list carrying a character outside ASCII",
        &["resource"],
    ),
    // **And the keys, which every rendering of a text is indexed too coarsely
    // to reach.** Every other nested object here is keyed `id`, so a
    // `hashed_form` that normalised the keys of a structured resource would be
    // the identity on all of it. See
    // `the_shapes_a_rendering_of_the_keys_would_collapse`.
    ("a resource keyed plainly", &["resource"]),
    (
        "the same resource keyed with a qualified name",
        &["resource"],
    ),
    (
        "the same resource keyed under another authority",
        &["resource"],
    ),
    (
        "the same resource keyed naming another thing",
        &["resource"],
    ),
    ("the same resource keyed in another case", &["resource"]),
    ("the same resource keyed with whitespace", &["resource"]),
    (
        "the same resource keyed with a combining mark",
        &["resource"],
    ),
    (
        "the same resource keyed with a character outside ASCII",
        &["resource"],
    ),
    // **The qualified-name shapes**, three per field: two differing only before
    // the first separator, which every suffix-keeping rendering maps together,
    // and two differing only after the last, which every prefix-keeping one
    // does. See `the_shapes_a_segment_keeping_rendering_would_collapse`.
    ("a qualified realm name", &["realm"]),
    ("the same realm under another authority", &["realm"]),
    ("the same realm authority naming another thing", &["realm"]),
    ("a qualified principal name", &["principal"]),
    ("the same principal under another authority", &["principal"]),
    (
        "the same principal authority naming another thing",
        &["principal"],
    ),
    ("a qualified actor chain name", &["actor_chain"]),
    (
        "the same actor chain under another authority",
        &["actor_chain"],
    ),
    (
        "the same actor chain authority naming another thing",
        &["actor_chain"],
    ),
    ("a qualified action name", &["action"]),
    ("the same action under another authority", &["action"]),
    (
        "the same action authority naming another thing",
        &["action"],
    ),
    ("a qualified resource name", &["resource"]),
    ("the same resource under another authority", &["resource"]),
    (
        "the same resource authority naming another thing",
        &["resource"],
    ),
    ("a qualified deciding policy name", &["deciding_policy"]),
    (
        "the same deciding policy under another authority",
        &["deciding_policy"],
    ),
    (
        "the same deciding policy authority naming another thing",
        &["deciding_policy"],
    ),
    ("a qualified policy pack name", &["policy_pack"]),
    (
        "the same policy pack under another authority",
        &["policy_pack"],
    ),
    (
        "the same policy pack authority naming another thing",
        &["policy_pack"],
    ),
    ("a qualified grant name", &["grant_used"]),
    ("the same grant under another authority", &["grant_used"]),
    (
        "the same grant authority naming another thing",
        &["grant_used"],
    ),
    // **The composed shapes**, a pair per field a composing rendering can
    // reach, at the top level, one container in, two containers in, inside a
    // list, and in a key. See [`composed`] for what is invisible without them.
    ("an action whose accent is written apart", &["action"]),
    ("the same action with the accent composed", &["action"]),
    (
        "a deciding policy whose accent is written apart",
        &["deciding_policy"],
    ),
    (
        "the same deciding policy with the accent composed",
        &["deciding_policy"],
    ),
    (
        "a policy pack whose accent is written apart",
        &["policy_pack"],
    ),
    (
        "the same policy pack with the accent composed",
        &["policy_pack"],
    ),
    ("a resource whose accent is written apart", &["resource"]),
    ("the same resource with the accent composed", &["resource"]),
    (
        "a nested resource whose accent is written apart",
        &["resource"],
    ),
    (
        "the same nested resource with the accent composed",
        &["resource"],
    ),
    (
        "a resource two containers down whose accent is written apart",
        &["resource"],
    ),
    (
        "a resource two containers down with the same accent composed",
        &["resource"],
    ),
    (
        "a resource inside a list whose accent is written apart",
        &["resource"],
    ),
    (
        "a resource inside a list with the same accent composed",
        &["resource"],
    ),
    (
        "the same resource keyed with an accent written apart",
        &["resource"],
    ),
    (
        "the same resource keyed with the same accent composed",
        &["resource"],
    ),
];

/// Every field of a record is a field some named shape departs in.
///
/// **The floor under the floor.** Each entry of [`MUST_HOLD`] is bound to the
/// fields its own label claims, and [`where_a_record_departs`] is bound to the
/// derived `PartialEq` — so a shape given a value its name does not claim
/// fails, and a field added to `Record` and forgotten by the comparator
/// fails. Neither of those is *coverage*: the comparator's binding only fires
/// for a field some entry varies, so a field that drops out of the corpus
/// altogether is a field nothing else here can see.
///
/// Measured, three parts one commit could carry together: the four `epoch …`
/// shapes setting `record.recorded_at` instead of `record.policy_epoch`, their
/// four rows of [`MUST_HOLD`] saying `&["recorded_at"]` to match,
/// and `entry.rs` returning `policy_epoch`
/// to `Value::text(i64::try_from(epoch).unwrap_or(i64::MAX).to_string())`.
/// Every per-entry assertion above holds — each shape departs exactly where its
/// label says — and `policy_epoch` is varied by nothing at all, so without this
/// the saturating cast collides in silence: `check passed`, every gate line
/// byte-identical to a clean run. The control — the encoding alone, with the
/// corpus left as it is — fails here, three collisions at and above 2^63.
///
/// So the union of the field lists is held against the field list the
/// comparator walks, the way `every_field_of_a_record_has_an_edit` holds
/// `FIELDS` against the struct *and* against `edits()`. A field that leaves the
/// corpus is a failure here rather than a clause nothing decides.
#[track_caller]
fn every_field_is_departed_in_by_some_named_shape() {
    let base = the_base_record();
    let walked: std::collections::BTreeSet<String> =
        every_field_and_whether_it_differs(&base, &base)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
    let named: std::collections::BTreeSet<String> = MUST_HOLD
        .iter()
        .flat_map(|(_, fields)| fields.iter().map(|field| (*field).to_owned()))
        .collect();
    assert_eq!(
        named, walked,
        "the shapes named in `MUST_HOLD` depart in {named:?} between them, and a \
         record holds {walked:?}. A field no entry of the corpus varies is a field \
         this whole file compares against nothing: `hashed_form` may render it any \
         way it likes — a saturating cast, a flattening into a neighbour — and every \
         assertion here still passes"
    );
    // **And the comparator itself is bound to the struct.** The two sets above
    // are a hand list held against a hand list: `MUST_HOLD` names fields,
    // `every_field_and_whether_it_differs` names fields, and the derived
    // `PartialEq` says only that an empty answer means the records are equal —
    // which is true of a comparator that walks eleven of twelve fields as long
    // as no corpus entry varies the twelfth. Measured without this binding: the
    // four `epoch …` shapes deleted with their four `MUST_HOLD` rows and the
    // comparator's one `policy_epoch` line, and `entry.rs` returning
    // `policy_epoch` through a saturating cast — every assertion in this file
    // holds, `cargo xtask check` is byte-identical to a clean run, and `verify`
    // answers `Ok` for a `policy_epoch` respelled at or above 2^63.
    //
    // `Record` is the authority, and `fields_of_the_struct` is the scanner that
    // reads it — the same one `every_field_of_a_record_has_an_edit` holds
    // `FIELDS` against, behind `test-support` so an integration test can reach
    // it rather than keeping a copy that agrees with itself.
    let declared: std::collections::BTreeSet<String> =
        nc_journal::entry::fields_of_the_struct("Record")
            .into_iter()
            .collect();
    assert_eq!(
        walked, declared,
        "`where_a_record_departs` walks {walked:?} and `Record` declares {declared:?}. \
         A field the comparator does not read is a field no shape can be said to \
         depart in, so the union above agrees with it about a field neither of them \
         knows — and `hashed_form` may then render that field any way it likes"
    );
}

/// The base-and-mark pairs [`composed`] joins, and the character each becomes.
///
/// **A named alphabet, and not the Unicode tables.** What the class beside it
/// has to state is that some value here tells a *composing* rendering from the
/// identity; a rendering that composes six pairs states it. This tree carries
/// no Unicode composition data and pulling a crate in for a test fixture is the
/// question `AGENTS.md` rule 3 asks the developer, so the rows are written out
/// and [`every_composition_named_here_is_shaped_like_one`] holds each of them to
/// what a composition looks like — an ASCII letter, a mark in the combining
/// range, and one character out — rather than to a table nothing here has.
const COMPOSES: [(char, char, char); 6] = [
    ('a', '\u{300}', '\u{e0}'),
    ('a', '\u{301}', '\u{e1}'),
    ('c', '\u{327}', '\u{e7}'),
    ('e', '\u{301}', '\u{e9}'),
    ('i', '\u{301}', '\u{ed}'),
    ('o', '\u{303}', '\u{f5}'),
];

/// A text with every pair of [`COMPOSES`] written as the one character it is.
///
/// **The composing half of Unicode normalisation, which the deleting class
/// beside it cannot stand for.** `stripped of its combining marks`
/// maps `Cafe\u{301}` onto `Cafe`; NFC maps it onto `Caf\u{e9}`, and without
/// this no value anywhere in this apparatus is precomposed — the census of
/// every other non-ASCII code point it holds is `U+001F`, `U+0300`, `U+0301`,
/// `U+036F`, `U+200B` and `U+2028`, not one of them a composed
/// character. So NFC and NFD would both be the identity on every record, on
/// every edit `tamper::edits()` makes and on [`PROBES`], and "normalise before
/// you hash" — the single most ordinary thing somebody adds to a text field —
/// would be a weakening this file could not see. Measured, `hashed_form`
/// rendering `action`
/// through `action.replace("e\u{301}", "\u{e9}")`: `nc-journal` green
/// and `check passed` with every line identical to a clean run,
/// with `Caf\u{e9}` and `Cafe\u{301}` both on `2d0f5d64d0504023…` in the field
/// that says what was done.
fn composed(text: &str) -> String {
    let mut written = String::with_capacity(text.len());
    for character in text.chars() {
        let joined = written.chars().next_back().and_then(|before| {
            COMPOSES
                .into_iter()
                .find(|(base, mark, _)| *base == before && *mark == character)
                .map(|(_, _, whole)| whole)
        });
        if let Some(whole) = joined {
            written.pop();
            written.push(whole);
        } else {
            written.push(character);
        }
    }
    written
}

/// Every row of [`COMPOSES`] is shaped like a composition, and [`composed`]
/// leaves what it has already written alone.
///
/// The rows cannot be checked against the Unicode database, because this tree
/// carries none. What is checked is the shape — an ASCII letter, a combining
/// mark, one character out, and no two rows landing on the same one — and the
/// property that makes the class a *rendering* rather than an arbitrary
/// rewrite: composing what is already composed says the same thing again.
#[test]
fn every_composition_named_here_is_shaped_like_one() {
    let mut whole = std::collections::BTreeSet::new();
    for (base, mark, one) in COMPOSES {
        assert!(
            base.is_ascii_alphabetic(),
            "`{base:?}` is named as the base of a composition and is not an ASCII \
             letter, so the row states nothing about a decomposed spelling"
        );
        assert!(
            matches!(mark, '\u{300}'..='\u{36f}'),
            "`{mark:?}` is named as the mark of a composition and is outside the \
             combining range, so `stripped of its combining marks` and this class \
             are not two renderings of one alphabet"
        );
        assert!(
            !one.is_ascii(),
            "`{one:?}` is named as a composed character and is ASCII, so the pair \
             this row claims is not a decomposed-versus-composed pair at all"
        );
        assert!(
            whole.insert(one),
            "`{one:?}` is the composed form of two rows, so one of them collapses a \
             pair the other already collapses and the alphabet is smaller than it \
             counts"
        );
        let apart: String = [base, mark].into_iter().collect();
        let together = String::from(one);
        assert_eq!(
            composed(&apart),
            together,
            "`composed` leaves `{apart:?}` apart, so this row is in the table and in \
             no rendering"
        );
        assert_eq!(
            composed(&together),
            together,
            "`composed` moves `{together:?}`, which it wrote itself — a rendering \
             that is not idempotent is not a normalisation, and a corpus pair it \
             collapses today it may not collapse tomorrow"
        );
    }
}

/// One row of [`NORMALISATIONS`]: what the rendering is called, the rendering
/// itself, and the fields it could collapse.
type Normalisation = (&'static str, fn(&str) -> String, &'static [&'static str]);

/// The renderings this corpus must be able to tell from the identity, and the
/// fields each one can collapse.
///
/// **The corpus is indexed by field and the property is (field × class).**
/// [`MUST_HOLD`] says every field is departed in by some shape,
/// and `every_field_of_a_record_has_an_edit` says every field has an edit —
/// both true, and both satisfied by departures a *normalising* rendering still
/// distinguishes. `action` is varied by three shapes and edited by one, all of
/// them by a different value, so without this `str::trim` applied to it is the
/// identity on the whole suite: measured, `hashed_form`
/// rendering `action`, `deciding_policy` and `policy_pack` through `trim`
/// gives `check passed` at the baseline verdict line for line, while two
/// records differing only in a trailing space on `action` land on one digest.
///
/// So the floor is the product. Adding a class is one row here plus the pairs
/// it needs; dropping a class, or letting a field fall out of one, is a failure
/// rather than a silence. The field lists are what the *type* admits:
/// `OpaqueId`'s grammar `[A-Za-z0-9-_.:]` has no whitespace in it, so trimming
/// an identifier field genuinely is the identity and claiming a pair for it
/// would be a test that cannot fail.
const NORMALISATIONS: [Normalisation; 7] = [
    (
        "case-folded",
        |text| text.to_lowercase(),
        &[
            "realm",
            "principal",
            "actor_chain",
            "action",
            "resource",
            "deciding_policy",
            "policy_pack",
            "grant_used",
        ],
    ),
    (
        "trimmed of its whitespace",
        |text| text.trim().to_owned(),
        &["action", "deciding_policy", "policy_pack", "resource"],
    ),
    // **With everything else in this apparatus ASCII, a rendering that keeps
    // only ASCII would be the identity on all of it.** `recased` says so in its
    // own doc — *"ASCII, because that is the whole of what `OpaqueId::new`
    // admits"* — and it is applied to three fields whose type is an
    // unconstrained `String` and one that is a `Value::Text` with no grammar at
    // all. The row that strips what is not ASCII asks whether the corpus can
    // tell `hashed_form` rendering `action` through
    // `chars().filter(char::is_ascii)` from the identity — a rendering that
    // puts `"Delete"` and `"Delete\u{2028}"` on one digest, in the field that
    // says what was done.
    //
    // These four fields are exactly the ones whose type admits a character
    // outside ASCII; the identifier fields cannot hold one. **And a
    // Unicode-normalising rendering**: were every other value in the apparatus
    // ASCII or ASCII plus `U+2028`, which NFC and NFD leave unchanged alike, no
    // pair could tell a rendering that folds combining marks from the identity.
    // The row that strips combining marks asks it of
    // `action.replace('\u{0301}', "")`, which puts `"Delete\u{0301}"` and
    // `"Delete"` on one digest, `b5d5b03c7abc710b…`. A decomposed-versus-composed
    // pair is exactly what a non-ASCII principal or resource name produces in
    // the wild.
    (
        "stripped of its combining marks",
        |text| {
            text.chars()
                .filter(|character| !matches!(character, '\u{0300}'..='\u{036f}'))
                .collect()
        },
        &["action", "deciding_policy", "policy_pack", "resource"],
    ),
    (
        "stripped of what is not ASCII",
        |text| text.chars().filter(char::is_ascii).collect(),
        &["action", "deciding_policy", "policy_pack", "resource"],
    ),
    // **And the composing half of the same alphabet**, which the deleting row
    // above cannot stand for: it maps `Cafe\u{301}` onto `Cafe`, and the
    // rendering a person actually writes — NFC — maps it onto `Caf\u{e9}`. With
    // nothing else in this apparatus precomposed, both directions of Unicode
    // normalisation would be the identity on every record here. See
    // [`composed`] for the measurement and for why the alphabet is written out.
    (
        "composed",
        composed,
        &["action", "deciding_policy", "policy_pack", "resource"],
    ),
    // **And a rendering that keeps one segment of a qualified name**:
    // `OpaqueId` admits `.` and `:` on purpose — its doc names `did:key:z…` and
    // `urn:uuid:…` — and with no other value in this apparatus carrying either,
    // taking a segment would be the identity on all of it. Measured that way,
    // `realm` rendered as the part after its last `:`: `nc-journal` and
    // `nc-canon` green, `check passed`, and `urn:uuid:alpha` colliding with
    // `did:web:alpha` on `3a82c132…` while `verify` answers `Ok(4)` over the
    // rewrite. Five more fields the same way, and `realm` again on the dot.
    //
    // Both ends, because `recased`'s doc says a class needs them: the pairs that
    // witness these two are the same three values seen from either side.
    (
        "reduced to the part after its last separator",
        |text| {
            text.rsplit([':', '.'])
                .next()
                .unwrap_or_default()
                .to_owned()
        },
        &[
            "realm",
            "principal",
            "actor_chain",
            "action",
            "resource",
            "deciding_policy",
            "policy_pack",
            "grant_used",
        ],
    ),
    (
        "reduced to the part before its first separator",
        |text| text.split([':', '.']).next().unwrap_or_default().to_owned(),
        &[
            "realm",
            "principal",
            "actor_chain",
            "action",
            "resource",
            "deciding_policy",
            "policy_pack",
            "grant_used",
        ],
    ),
];

/// Every field whose value is text a caller chose.
///
/// The truncation floor walks this rather than a list somebody wrote for it: a
/// class of `&["realm", "principal"]`, while eight fields carry text, lets six
/// of them be cut with nothing noticing. **How far into each one a cut must be
/// visible is [`the_reach_a_field_must_have`], not a number in this
/// table.** The list itself is held against the struct by
/// [`the_corpus_can_see_a_truncation_at_every_length`], so a field cannot leave
/// it quietly either.
const CARRIES_TEXT: [&str; 11] = [
    "realm",
    "principal",
    "actor_chain",
    "action",
    "resource",
    "deciding_policy",
    "policy_pack",
    "grant_used",
    // **And the fields whose type is a number and whose rendering is text.** A
    // cut applies to them exactly as it does to an identifier, and the corpus
    // held nothing long enough to see one.
    "policy_epoch",
    "recorded_at",
    "work_unit",
];

/// The longest text `OpaqueId` takes, measured by asking it.
///
/// Asked rather than read: `OPAQUE_ID_MAX` is the constant the corpus builds
/// its long pairs from, so a table reading it would agree with those pairs
/// whatever either of them became.
fn the_longest_text_an_identifier_admits() -> usize {
    (1..=1024)
        .take_while(|length| OpaqueId::new("a".repeat(*length)).is_ok())
        .last()
        .unwrap_or(0)
}

/// How far into a field a cut has to be visible here.
///
/// **The reach is asked of the types, not written as a column of 64s against
/// pairs of 128 characters.** Measured with such a column, the stem cut from
/// 127 characters to 77: every floor in this file stays green with the longest
/// value the corpus holds at 78 rather than 128 — and `hashed_form`
/// cutting `realm` to a hundred characters passes with it, invisible to the
/// whole file. That same cut against the stem as it stands fails, which is the
/// whole difference: what these pairs can see is their own length, and without
/// this the only thing holding that length is the line that writes it.
///
/// So the reach is asked of the types. An identifier's is measured by handing
/// `OpaqueId::new` longer and longer text until it refuses; a `u64` reaches the
/// hash as its digits and a work unit as its Crockford form, so those two are
/// measured by rendering the widest value each layout holds. The four fields
/// whose type is a plain `String` have no maximum at all — writing one for them
/// would be a claim about a type that makes none — and their pairs are built
/// from the identifiers' stem, so the reach they are held to is the length that
/// stem is.
fn the_reach_a_field_must_have(field: &str) -> usize {
    match field {
        "policy_epoch" | "recorded_at" => u64::MAX.to_string().chars().count(),
        // Thirteen: a work unit is sixty-three bits and Crockford writes five to
        // a digit. `to_crockford` pads *up* to twelve, which is what the corpus
        // held and what made a cut at twelve invisible.
        "work_unit" => Snowflake::from_crockford("7ZZZZZZZZZZZZ")
            .expect("the widest value the layout holds")
            .to_crockford()
            .chars()
            .count(),
        _ => the_longest_text_an_identifier_admits(),
    }
}

/// The text one field of a record carries.
///
/// The chain is joined on a byte no identifier admits, so that joining is not
/// itself a flattening — `["a","bc"]` and `["ab","c"]` are two chains here as
/// well as in `hashed_form`.
fn the_text_of(record: &Record, field: &str) -> String {
    match field {
        "realm" => record.realm.as_str().to_owned(),
        "principal" => record.principal.as_str().to_owned(),
        "actor_chain" => record
            .actor_chain
            .iter()
            .map(nc_ids::OpaqueId::as_str)
            .collect::<Vec<_>>()
            .join("\u{1f}"),
        "action" => record.action.clone(),
        "deciding_policy" => record.deciding_policy.clone().unwrap_or_default(),
        "policy_pack" => record.policy_pack.clone().unwrap_or_default(),
        "grant_used" => record
            .grant_used
            .as_ref()
            .map(|grant| grant.as_str().to_owned())
            .unwrap_or_default(),
        // **A value that is not text is rendered as the hash renders it, not
        // through `Debug`, which never enters any hash.** `hashed_form` puts
        // the `Value` in whole, so what a cut or a normalisation of `resource`
        // actually reaches is the canonical text `nc-canon` writes —
        // `{"id":1}`, not `Object({"id": Integer(1)})`. Measured against the
        // nested pairs below: `nc-canon` writes every character above the
        // control range literally and Rust's `Debug` for a string escapes both
        // `U+0301` and `U+2028`, so under `format!("{other:?}")` the two halves
        // of each of those pairs would be pure ASCII and carry no combining
        // mark — a rendering that strips either one the identity on both, and
        // `the_corpus_can_see_a_rendering_that_reaches_inside` unsatisfiable.
        //
        // `Value::Text` stays as its own text: a class in `NORMALISATIONS` is a
        // `fn(&str) -> String`, and the weakening it stands for renders the
        // string a caller chose rather than the quotes put round it.
        "resource" => match &record.resource {
            Value::Text(text) => text.clone(),
            other => other.to_canonical(),
        },
        // **The numeric fields enter the hash as text too.** A `CARRIES_TEXT`
        // of the eight fields whose *type* is text leaves no length witness for
        // any of these — measured, `policy_epoch` cut to nineteen characters:
        // `check passed`, with `18446744073709551615` and
        // `18446744073709551610` on one digest. The corpus's four epoch entries
        // sit at the signed boundary the saturating cast is about, and only
        // `u64::MAX` has twenty digits, so a cut at nineteen would be the
        // identity on all four.
        //
        // Rendered exactly as `hashed_form` renders them, or the witness would
        // be about a different string than the one that is hashed.
        "policy_epoch" => record
            .policy_epoch
            .map(|epoch| epoch.to_string())
            .unwrap_or_default(),
        "work_unit" => record
            .work_unit
            .map(nc_ids::Snowflake::to_crockford)
            .unwrap_or_default(),
        "recorded_at" => record.recorded_at.to_string(),
        _ => panic!("`{field}` is named in `NORMALISATIONS` and carries no text"),
    }
}

/// The three pairs a rendering over the `Value` *type* would collapse.
///
/// **Named, and pinned.** The six entries are three pairs, and `MUST_HOLD`
/// binds each to "departs from base in `resource`" — which one literal
/// satisfies just as well: measured without this, `Value::Integer(1)` changed
/// to `Value::Integer(7)` leaves every other test green, and with `hashed_form`
/// also collapsing `Integer` into its text the suite is *still* green
/// and `cargo xtask check` prints `check passed`, verdict line for line. Every
/// other class here has a floor — `NORMALISATIONS` has one, the truncation walk
/// has one — and this class is outside the `fn(&str) -> String` product by
/// construction.
const VARIANT_PAIRS: [(&str, &str); 3] = [
    (
        "a resource that is a number",
        "the same resource written as text",
    ),
    ("a resource that is a flag", "the same flag written as text"),
    (
        "a resource that is absent",
        "the same absence written as empty text",
    ),
];

/// Each named pair differs in `resource` alone and collapses onto one value
/// under a rendering of the type.
///
/// The floor the class needs: a pair whose two records stop being a pair
/// fails here rather than keeping its label and ruling out nothing.
#[track_caller]
fn the_corpus_can_tell_a_variant_from_its_text(corpus: &[(String, Record)]) -> usize {
    let held = |what: &str| -> &Record {
        &corpus
            .iter()
            .find(|(label, _)| label == what)
            .unwrap_or_else(|| panic!("`{what}` is named in `VARIANT_PAIRS` and is not here"))
            .1
    };
    let flattened = |value: &Value| -> Value {
        match value {
            Value::Integer(number) => Value::text(number.to_string()),
            Value::Bool(flag) => Value::text(flag.to_string()),
            Value::Null => Value::text(String::new()),
            other => other.clone(),
        }
    };
    for (typed, written) in VARIANT_PAIRS {
        let (one, other) = (held(typed), held(written));
        assert_eq!(
            where_a_record_departs(one, other),
            vec!["resource".to_owned()],
            "`{typed}` and `{written}` are named as a pair and differ somewhere else, so \
             a rendering that collapsed the type would still tell them apart and this \
             class rules out nothing"
        );
        assert_ne!(
            one.resource, other.resource,
            "`{typed}` and `{written}` hold the same value, so they are one record twice"
        );
        assert_eq!(
            flattened(&one.resource),
            flattened(&other.resource),
            "`{typed}` and `{written}` do not collapse onto one value under a rendering \
             of the type, so the collapse they are named for is ruled out by nothing"
        );
    }
    VARIANT_PAIRS.len()
}

/// One row of [`NESTED_PAIRS`]: the two entries, and the class of
/// [`NORMALISATIONS`] whose rendering collapses them one level down.
type NestedPair = (&'static str, &'static str, &'static str);

/// The pairs whose two resources are structured and differ only inside the
/// nesting.
///
/// **A class of [`NORMALISATIONS`] says the same thing one level down only if it
/// rewrites every character.** Each class is a `fn(&str) -> String` reached
/// through [`the_text_of`], and a structured resource reaches it as the
/// canonical text that is hashed — so case-folding, a combining-mark fold and an
/// ASCII filter all apply to what is inside the braces. Trimming does not: it
/// touches the two ends of a rendering, and a nested value's rendering ends in a
/// brace. A row here for `trimmed of its whitespace` would be a claim no pair
/// can satisfy, so there is none, and the whitespace pair for `resource` stays
/// the top-level one.
const NESTED_PAIRS: [NestedPair; 4] = [
    (
        "a nested resource that is named",
        "the same nested resource in another case",
        "case-folded",
    ),
    (
        "a nested resource carrying a combining mark",
        "a nested resource that is named",
        "stripped of its combining marks",
    ),
    (
        "a nested resource carrying a character outside ASCII",
        "a nested resource that is named",
        "stripped of what is not ASCII",
    ),
    (
        "a nested resource whose accent is written apart",
        "the same nested resource with the accent composed",
        "composed",
    ),
];

/// Each nested pair is two structured resources that one named class collapses.
///
/// The floor that class needs, and the reason [`the_text_of`] renders a
/// resource that is not text the way the hash renders it: under a `Debug`
/// string the last two rows below could not be satisfied by any pair at all,
/// because Rust escapes a combining mark and a line separator and leaves the
/// rendering pure ASCII.
#[track_caller]
fn the_corpus_can_see_a_rendering_that_reaches_inside(corpus: &[(String, Record)]) -> usize {
    let held = |what: &str| -> &Record {
        &corpus
            .iter()
            .find(|(label, _)| label == what)
            .unwrap_or_else(|| panic!("`{what}` is named in `NESTED_PAIRS` and is not here"))
            .1
    };
    for (left, right, how) in NESTED_PAIRS {
        let (_, render, fields) = NORMALISATIONS
            .into_iter()
            .find(|(name, _, _)| *name == how)
            .unwrap_or_else(|| panic!("`{how}` is named here and not in `NORMALISATIONS`"));
        assert!(
            fields.contains(&"resource"),
            "`{how}` is not applied to `resource`, so a nested pair claimed for it is a \
             pair no rendering of that field ever sees"
        );
        let (one, other) = (held(left), held(right));
        for (what, record) in [(left, one), (right, other)] {
            assert!(
                matches!(record.resource, Value::Object(_) | Value::Array(_)),
                "`{what}` is named in `NESTED_PAIRS` and its resource is a scalar, so a \
                 rendering that reaches inside a nesting has no nesting to reach into and \
                 this pair says only what the top-level pairs already say"
            );
        }
        assert_eq!(
            where_a_record_departs(one, other),
            vec!["resource".to_owned()],
            "`{left}` and `{right}` are named as a pair and differ somewhere else, so a \
             rendering that reached inside would still tell them apart"
        );
        let (here, there) = (the_text_of(one, "resource"), the_text_of(other, "resource"));
        assert_ne!(
            here, there,
            "`{left}` and `{right}` hash the same value, so they are one record twice"
        );
        assert_eq!(
            render(&here),
            render(&there),
            "`{left}` and `{right}` are still two when the resource is {how}, so a \
             `hashed_form` that reached one level down would be the identity on both and \
             two different records would land on one digest"
        );
    }
    NESTED_PAIRS.len()
}

/// A list inside a resource keeps its order and its repeats.
///
/// The relation [`NESTED_PAIRS`] cannot state: a rendering that sorts or
/// de-duplicates a list inside `resource` leaves every character of it in place,
/// so no `fn(&str) -> String` collapses the pair and only two records standing
/// in a relation to each other rule it out — the same shape as the two actor
/// chains in [`the_named_shapes_are_all_here`]. Measured, with `hashed_form`
/// sorting and de-duplicating the lists inside `resource`: every other test of
/// this file green, and `{"id":["a","b"]}` and `{"id":["b","a"]}`
/// on `9556c5009fc2a5c5…`.
#[track_caller]
fn the_nested_lists_keep_their_order_and_their_repeats(corpus: &[(String, Record)]) {
    let list = |what: &str| -> Vec<Value> {
        corpus
            .iter()
            .find(|(label, _)| label == what)
            .map(|(_, record)| record.resource.clone())
            .and_then(Value::into_object)
            .and_then(|mut fields| fields.remove("id"))
            .and_then(Value::into_array)
            .unwrap_or_else(|| panic!("`{what}` is named here and holds no list under `id`"))
    };
    let forward = list("a nested list of two");
    let backward = list("the same nested list, the other way round");
    let flipped: Vec<Value> = backward.iter().rev().cloned().collect();
    assert_eq!(
        flipped, forward,
        "`the same nested list, the other way round` holds {backward:?}, and reversing it \
         gives {flipped:?} rather than `a nested list of two`'s {forward:?}. These two are \
         the whole of what stops `hashed_form` sorting a list inside a resource"
    );
    assert!(
        forward.len() == 2 && forward[0] != forward[1],
        "`a nested list of two` holds {forward:?}, and a list of one — or of one value \
         twice — is its own reverse, so the pair above would compare equal with the order \
         never varied"
    );
    let repeated = list("a nested list repeating a value");
    let once: Vec<&Value> = repeated.iter().fold(Vec::new(), |mut seen, item| {
        if !seen.contains(&item) {
            seen.push(item);
        }
        seen
    });
    assert!(
        repeated.len() > once.len(),
        "`a nested list repeating a value` holds {repeated:?}, which repeats nothing, so a \
         `hashed_form` that de-duplicated a list inside a resource would pass over it"
    );
}

/// Whether a rendering says inside a container what it says on its own.
///
/// A class of [`NORMALISATIONS`] is a `fn(&str) -> String` over the canonical
/// text of a whole resource, so it reaches a text inside a container only when
/// wrapping and rendering commute — rendering the canonical form of a wrapped
/// text has to give the canonical form of the rendered text. Trimming does not:
/// a wrapped rendering ends in a brace or a bracket, so `str::trim` reaches
/// nothing inside one. The other three do, and they do it at one container,
/// which is the step that repeats: a rendering that commutes with one container
/// commutes with a second, so a class witnessed at some nesting is witnessed at
/// any nesting built of the same steps.
fn a_rendering_reaches_inside(render: fn(&str) -> String) -> bool {
    // **Every class this apparatus names, in one string.** A probe carrying
    // only a combining mark, a line separator and a trailing space would answer
    // `true` for a rendering that touches only a *tab* without exercising
    // anything: the rendering leaves such a probe alone at every nesting and at
    // the top, and "reaches inside" would be read off two identities. A probe
    // that a class cannot move is a probe that agrees with any rendering of
    // that class. The interior gap is here for that reason, and so is the
    // leading space: `trim_start` and `trim_end` are different renderings.
    // **And a pair a composing rendering joins**, for the same reason:
    // `A\u{0301}` is not a composition this file names — the alphabet is
    // lower-case — so without one `composed` would leave the whole probe alone
    // and "reaches inside" be read off two identities again.
    let probe = " A\u{0301}b\tc\u{2028}e\u{0301} ";
    [
        (
            Value::object([("id", Value::text(probe))]),
            Value::object([("id", Value::text(render(probe)))]),
        ),
        (
            Value::Array(vec![Value::text(probe)]),
            Value::Array(vec![Value::text(render(probe))]),
        ),
    ]
    .into_iter()
    .all(|(wrapped, rendered)| render(&wrapped.to_canonical()) == rendered.to_canonical())
}

/// Where a value keeps each of its texts, as the containers crossed to reach it.
///
/// The empty answer is a text that is the whole value, and a text in a list
/// under an object is an object step and then an array step.
/// [`every_nesting_the_corpus_holds_has_a_witness`] reads the set of these off
/// the corpus, so a resource shaped in a way nothing here has held before asks
/// for its own witnesses rather than passing under a table that names no such
/// row.
fn where_a_value_keeps_its_texts(value: &Value, at: &str, found: &mut Vec<String>) {
    let deeper = |container: &str| {
        if at.is_empty() {
            container.to_owned()
        } else {
            format!("{at}/{container}")
        }
    };
    match value {
        Value::Text(_) => found.push(at.to_owned()),
        Value::Array(items) => {
            for item in items {
                where_a_value_keeps_its_texts(item, &deeper("array"), found);
            }
        }
        Value::Object(fields) => {
            for field in fields.values() {
                where_a_value_keeps_its_texts(field, &deeper("object"), found);
            }
        }
        Value::Null | Value::Bool(_) | Value::Integer(_) => {}
    }
}

/// The nestings at which two values keep different texts.
///
/// Two values that differ in anything but a text — a key, the length of a list,
/// the kind of a value — answer with something no nesting equals, so a witness
/// for a nesting has to be two values of one shape differing in one text of it.
fn where_two_values_differ(one: &Value, other: &Value, at: &str, found: &mut Vec<String>) {
    if one == other {
        return;
    }
    let deeper = |container: &str| {
        if at.is_empty() {
            container.to_owned()
        } else {
            format!("{at}/{container}")
        }
    };
    match (one, other) {
        (Value::Text(_), Value::Text(_)) => found.push(at.to_owned()),
        (Value::Array(here), Value::Array(there)) if here.len() == there.len() => {
            for (item, another) in here.iter().zip(there) {
                where_two_values_differ(item, another, &deeper("array"), found);
            }
        }
        (Value::Object(here), Value::Object(there)) if here.keys().eq(there.keys()) => {
            for (field, another) in here.values().zip(there.values()) {
                where_two_values_differ(field, another, &deeper("object"), found);
            }
        }
        _ => found.push(format!("{at}, and not only in a text")),
    }
}

/// Whether the corpus holds two records that differ in one text at `at` and
/// nowhere else, and that `render` collapses onto one.
///
/// Split from the walk below, which crossed clippy's line limit with it inline.
fn a_pair_differs_only_inside(
    corpus: &[(String, Record)],
    at: &str,
    render: fn(&str) -> String,
) -> bool {
    corpus.iter().enumerate().any(|(index, (_, one))| {
        corpus[index + 1..].iter().any(|(_, other)| {
            let mut differs = Vec::new();
            where_two_values_differ(&one.resource, &other.resource, "", &mut differs);
            if differs != [at.to_owned()]
                || where_a_record_departs(one, other) != ["resource".to_owned()]
            {
                return false;
            }
            let (here, there) = (the_text_of(one, "resource"), the_text_of(other, "resource"));
            here != there && render(&here) == render(&there)
        })
    })
}

/// Every nesting the corpus keeps a text in has a pair for every rendering that
/// reaches inside one.
///
/// **The floor [`NESTED_PAIRS`] cannot carry, and it is stated over the
/// structure rather than over a list of depths.** All three of those rows vary a
/// direct string value of the top-level object, so a rendering reaching one
/// container further in is the identity on every record those rows hold — and a row per
/// depth would floor the depths somebody thought to write, while `resource`
/// takes a `Value` of any shape at all. So the nestings are read off the corpus
/// itself, the renderings that reach into one are read off [`NORMALISATIONS`]
/// through [`a_rendering_reaches_inside`], and every pair of those asks for a
/// witness: a resource shaped a new way brings its own rows with it, and a
/// witness that leaves is a failure here. Every demand it makes is satisfiable
/// by construction — the partner of a witness is that same resource with the one
/// text respelled — so this cannot become a claim no corpus could meet.
///
/// **What no corpus can reach, written down:** a rendering that reaches only at
/// some depth deeper than anything held here is invisible to every record in
/// this file. The two assertions below hold the corpus at two containers and at
/// both kinds of container; what carries a witness past that is the commuting
/// step [`a_rendering_reaches_inside`] measures, which is the same step at every
/// depth.
#[track_caller]
fn every_nesting_the_corpus_holds_has_a_witness(corpus: &[(String, Record)]) -> usize {
    let mut found: Vec<String> = Vec::new();
    for (_, record) in corpus {
        where_a_value_keeps_its_texts(&record.resource, "", &mut found);
    }
    let nestings: std::collections::BTreeSet<String> =
        found.into_iter().filter(|at| !at.is_empty()).collect();
    let deepest = nestings
        .iter()
        .map(|at| at.split('/').count())
        .max()
        .unwrap_or(0);
    assert!(
        deepest >= 2,
        "the deepest a resource here keeps a text is {deepest} container(s) in, so a \
         `hashed_form` that reached past that would be the identity on every record in \
         this corpus and nothing in this file could see it"
    );
    assert!(
        nestings
            .iter()
            .any(|at| at.split('/').any(|step| step == "array")),
        "no resource here keeps a text inside a list, so a `hashed_form` that reached \
         into one would be the identity on every record in this corpus"
    );
    let mut proved = 0_usize;
    for (how, render, fields) in NORMALISATIONS {
        if !fields.contains(&"resource") || !a_rendering_reaches_inside(render) {
            continue;
        }
        for at in &nestings {
            assert!(
                a_pair_differs_only_inside(corpus, at, render),
                "no two records here differ in `resource` alone at one text held under \
                 {at}, and become equal when it is {how}, so a `hashed_form` rendering a \
                 text that far in would be the identity on every record here and two \
                 different records would land on one digest with `verify` answering `Ok`"
            );
            proved += 1;
        }
    }
    proved
}

/// One row of [`KEY_PAIRS`]: the two entries, and the class of
/// [`NORMALISATIONS`] whose rendering of the keys collapses them.
type KeyPair = (&'static str, &'static str, &'static str);

/// The pairs whose two resources differ in the key of a nested object and
/// nowhere else.
///
/// **A key is not reachable from a class of [`NORMALISATIONS`], which is why
/// this is its own table.** [`the_corpus_can_see_a_rendering_that_reaches_inside`]
/// asks `render(here) == render(there)` over the canonical text
/// [`the_text_of`] hands back, and the two texts of a key pair genuinely do
/// collapse under `case-folded` — a row there would pass while proving nothing
/// about a key as distinct from a value. Widening a class's field list cannot
/// state it either: a class is a `fn(&str) -> String` over the whole canonical
/// text, and a function of that shape cannot tell a key from a value. So the
/// rendering is applied to the keys and to nothing else, by
/// [`with_its_keys_rendered`].
///
/// Every class [`NORMALISATIONS`] applies to `resource` has a row, which is what
/// [`the_corpus_can_see_a_rendering_of_the_keys`] holds this table to — and a
/// class deleted there is then a name this table carries and that one does not,
/// which fails at the lookup rather than moving two counts together.
const KEY_PAIRS: [KeyPair; 7] = [
    (
        "a resource keyed plainly",
        "the same resource keyed in another case",
        "case-folded",
    ),
    (
        "a resource keyed plainly",
        "the same resource keyed with whitespace",
        "trimmed of its whitespace",
    ),
    (
        "a resource keyed plainly",
        "the same resource keyed with a combining mark",
        "stripped of its combining marks",
    ),
    (
        "a resource keyed plainly",
        "the same resource keyed with a character outside ASCII",
        "stripped of what is not ASCII",
    ),
    // The composing class needs two keys that differ by a composition and not
    // by anything else, so its row does not start from the plain key.
    (
        "the same resource keyed with an accent written apart",
        "the same resource keyed with the same accent composed",
        "composed",
    ),
    // The two ends of a qualified key: the first pair shares everything after
    // the last separator, the second everything before the first.
    (
        "the same resource keyed with a qualified name",
        "the same resource keyed under another authority",
        "reduced to the part after its last separator",
    ),
    (
        "the same resource keyed with a qualified name",
        "the same resource keyed naming another thing",
        "reduced to the part before its first separator",
    ),
];

/// A value with its keys rendered and everything under them left alone.
fn with_its_keys_rendered(value: &Value, render: fn(&str) -> String) -> Value {
    match value {
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| with_its_keys_rendered(item, render))
                .collect(),
        ),
        Value::Object(fields) => Value::object(
            fields
                .iter()
                .map(|(key, field)| (render(key), with_its_keys_rendered(field, render))),
        ),
        other => other.clone(),
    }
}

/// Each key pair is two resources one named class collapses through the keys
/// alone.
///
/// The floor that class needs: the pairs are two objects holding the same
/// value under a key spelled two ways, so what the rendering below collapses is
/// the key, and a pair whose two records stop standing in that relation fails
/// here rather than keeping its label and ruling out nothing.
#[track_caller]
fn the_corpus_can_see_a_rendering_of_the_keys(corpus: &[(String, Record)]) -> usize {
    let held = |what: &str| -> &Record {
        &corpus
            .iter()
            .find(|(label, _)| label == what)
            .unwrap_or_else(|| panic!("`{what}` is named in `KEY_PAIRS` and is not here"))
            .1
    };
    let named: std::collections::BTreeSet<&str> =
        KEY_PAIRS.iter().map(|(_, _, how)| *how).collect();
    let applied: std::collections::BTreeSet<&str> = NORMALISATIONS
        .iter()
        .filter(|(_, _, fields)| fields.contains(&"resource"))
        .map(|(how, _, _)| *how)
        .collect();
    assert_eq!(
        named, applied,
        "`KEY_PAIRS` names {named:?} and `NORMALISATIONS` renders a \
         resource {applied:?}. A class in one and not in the other is either a rendering \
         of a key that no pair here can see, or a class that left `NORMALISATIONS` while \
         the pair claiming it stayed"
    );
    for (left, right, how) in KEY_PAIRS {
        let (_, render, _) = NORMALISATIONS
            .into_iter()
            .find(|(name, _, _)| *name == how)
            .unwrap_or_else(|| panic!("`{how}` is named here and not in `NORMALISATIONS`"));
        let (one, other) = (held(left), held(right));
        for (what, record) in [(left, one), (right, other)] {
            assert!(
                matches!(record.resource, Value::Object(_)),
                "`{what}` is named in `KEY_PAIRS` and its resource holds no keys, so a \
                 rendering of the keys has nothing here to render"
            );
        }
        assert_eq!(
            where_a_record_departs(one, other),
            vec!["resource".to_owned()],
            "`{left}` and `{right}` are named as a pair and differ somewhere else, so a \
             rendering of the keys would still tell them apart"
        );
        let (here, there) = (&one.resource, &other.resource);
        assert_ne!(
            here, there,
            "`{left}` and `{right}` hold the same value, so they are one record twice"
        );
        let under = |value: &Value| -> Vec<Value> {
            value
                .clone()
                .into_object()
                .map(|fields| fields.into_values().collect())
                .unwrap_or_default()
        };
        assert_eq!(
            under(here),
            under(there),
            "`{left}` and `{right}` differ under their keys as well, so what a rendering \
             of the keys collapses here is not the key alone"
        );
        assert_eq!(
            with_its_keys_rendered(here, render).to_canonical(),
            with_its_keys_rendered(there, render).to_canonical(),
            "`{left}` and `{right}` are still two when the keys are {how}, so a \
             `hashed_form` that rendered the keys of a resource that way would be the \
             identity on both and two different records would land on one digest"
        );
    }
    KEY_PAIRS.len()
}

/// For every text field, a pair the corpus holds that a cut at *any* length
/// collapses.
///
/// **A length somebody chose is not the property.** A class
/// of `cut to twelve characters` over `realm` and `principal`, with length
/// pairs diverging at character 25, lets a `hashed_form` cutting every text
/// field to twenty-six characters be the identity on all of it, with
/// every `nc-journal` test green and `cargo xtask check` moving nothing but its
/// raw line counters. `deciding_policy` cut at twenty-four passes alone,
/// too: `core.no_self_escalation.v1` and `…v2` on one digest.
///
/// So the floor is every length from one up to the longest value that field
/// holds. The pairs in [`the_longest_values_each_field_admits`] are as long as
/// the type admits and differ in their final character, which is what satisfies
/// it at every length below that.
///
/// **What it cannot reach, written down:** a cut at or above the longest value
/// a field holds here changes no value in this corpus, so nothing in this file
/// can see it. The reach is therefore exactly the length of those pairs, and
/// what says how long that has to be is [`the_reach_a_field_must_have`], which
/// asks each field's type rather than reading the constant the pairs are built
/// from — a table doing that would agree with the pairs however short they got.
#[track_caller]
fn the_corpus_can_see_a_truncation_at_every_length(corpus: &[(String, Record)]) -> usize {
    // **And the table is held against the struct.** A field that leaves
    // `CARRIES_TEXT` takes its own floor with it, so the walk below would prove
    // nothing about it and say nothing about it either. `result` is the one
    // field of a `Record` that is not named here: an `Outcome` reaches the hash
    // as `Allowed` or `Denied`, which differ in their first character, so no cut
    // collapses the two.
    let carried: std::collections::BTreeSet<String> = CARRIES_TEXT
        .iter()
        .map(|field| (*field).to_owned())
        .collect();
    let declared: std::collections::BTreeSet<String> =
        nc_journal::entry::fields_of_the_struct("Record")
            .into_iter()
            .filter(|field| field != "result")
            .collect();
    assert_eq!(
        carried, declared,
        "`CARRIES_TEXT` names {carried:?} and a `Record` declares {declared:?} beside \
         `result`, so a field whose value reaches the hash as text is cut by nothing here"
    );
    let mut proved = 0_usize;
    for field in CARRIES_TEXT {
        let admits = the_reach_a_field_must_have(field);
        let longest = corpus
            .iter()
            .map(|(_, record)| the_text_of(record, field).chars().count())
            .max()
            .unwrap_or(0);
        assert!(
            longest >= admits,
            "the longest `{field}` this corpus holds is {longest} character(s) and the \
             pairs here are built to reach {admits}, so a cut at {} is the identity on \
             every record here and nothing in this file could see it",
            longest + 1
        );
        for cut in 1..longest {
            let found = corpus.iter().enumerate().any(|(at, (_, one))| {
                corpus[at + 1..].iter().any(|(_, other)| {
                    let (here, there) = (the_text_of(one, field), the_text_of(other, field));
                    where_a_record_departs(one, other) == [field.to_owned()]
                        && here != there
                        && here.chars().take(cut).eq(there.chars().take(cut))
                })
            });
            assert!(
                found,
                "no two records differ in `{field}` alone and share their first {cut} \
                 character(s), so a `hashed_form` that cut `{field}` there would be the \
                 identity on this whole corpus — and two records that differ past that \
                 point would land on one digest with `verify` answering `Ok`"
            );
            proved += 1;
        }
    }
    proved
}

/// The texts each field's type is probed with, to find which renderings it can
/// hide.
///
/// A rendering hides a field only if the field can hold two different texts it
/// maps onto one, which is a question about the type and not about this corpus:
/// `OpaqueId`'s grammar admits no whitespace, so trimming an identifier is the
/// identity and a pair claimed for it would be a test that cannot fail. These
/// carry, for each class of [`NORMALISATIONS`], a text that class changes beside
/// the text it changes it into, and the two shapes a number reaches the hash as
/// — a decimal and a Crockford form — so those rows are answered rather than
/// left empty for want of anything to ask.
const PROBES: [&str; 14] = [
    "a",
    "A",
    "a ",
    // **And whitespace at the front.** Every other whitespace-carrying probe
    // here and every whitespace-carrying record in the corpus *appends* one,
    // and `padded` in `entry.rs` is `format!("{text} ")` — so a `hashed_form`
    // that trimmed only the **front** would be the identity on every one of
    // them, and two records differing by a leading space on `action` land on
    // one digest with the suite green. `NORMALISATIONS` cannot tell the two
    // ends apart either: its class is `str::trim`, which is symmetric.
    " a",
    "a\u{0301}",
    // **And the same letter precomposed**, without which `composed` is a class
    // no field is asked for: every other probe here is ASCII or ASCII plus one
    // combining character, so a composing rendering would map each of them to
    // itself and hide nothing anywhere.
    "\u{e1}",
    "a\u{2028}",
    "1",
    "2",
    "ZZZZZZZZZZZJ",
    "zzzzzzzzzzzj",
    // **And a qualified name, which nothing else here carries.** `OpaqueId` admits
    // `.` and `:` on purpose — its doc names `did:key:z…` and `urn:uuid:…` — so
    // a rendering that keeps one *segment* of an identifier is a rendering the
    // types allow, and with no probe holding a separator this reader would
    // conclude no field could hide one. Three spellings close both ends: two differ only
    // before the first separator, two only after the last.
    "urn:uuid.alpha",
    "did:uuid.alpha",
    "urn:uuid.beta",
];

/// Whether a field can hold a text, in the spelling `hashed_form` reads it.
///
/// Round-tripped rather than merely constructed: a work unit accepts its
/// Crockford form in either case and renders it in upper case alone, so a
/// lower-case spelling is a text the hash never sees and a rendering that
/// folds case cannot hide one work unit as another.
fn a_field_admits(field: &str, text: &str) -> bool {
    match field {
        "realm" | "principal" | "actor_chain" | "grant_used" => {
            OpaqueId::new(text).is_ok_and(|held| held.as_str() == text)
        }
        "action" | "deciding_policy" | "policy_pack" | "resource" => true,
        "policy_epoch" | "recorded_at" => text
            .parse::<u64>()
            .is_ok_and(|number| number.to_string() == text),
        "work_unit" => {
            Snowflake::from_crockford(text).is_ok_and(|unit| unit.to_crockford() == text)
        }
        _ => panic!("`{field}` is named in `CARRIES_TEXT` and has no type here"),
    }
}

/// The (rendering, field) pairs the types say this corpus has to be able to see.
///
/// **A table held against itself is not a floor.** Both sides of an equality
/// reading `NORMALISATIONS` move together when a row is deleted, and a floor of
/// twelve pairs against a table naming twenty leaves eight pairs of slack:
/// measured, `action` dropped from the `trimmed of its whitespace` row, with
/// every other test of this file green and `str::trim` witnessed by nothing in
/// the field that says what was done.
///
/// So what each field can hide is decided by [`a_field_admits`], which asks the
/// types, and the walk below is held against that. Asking instead that every
/// class name every field would be the opposite mistake: `OpaqueId`'s grammar
/// has no whitespace in it, so a trim-witness for `realm`, `principal`,
/// `actor_chain` or `grant_used` would be a test that cannot fail, and the
/// suite would be permanently red for want of one. A class deleted whole is
/// refused elsewhere — [`KEY_PAIRS`] and [`NESTED_PAIRS`] name their classes and
/// fail at the lookup when one has left.
fn the_renderings_each_field_can_hide() -> std::collections::BTreeSet<(String, String)> {
    let mut demanded = std::collections::BTreeSet::new();
    for (how, render, _) in NORMALISATIONS {
        for field in CARRIES_TEXT {
            let admitted: Vec<&str> = PROBES
                .into_iter()
                .filter(|text| a_field_admits(field, text))
                .collect();
            let hides = admitted.iter().enumerate().any(|(at, one)| {
                admitted[at + 1..]
                    .iter()
                    .any(|other| render(one) == render(other))
            });
            if hides {
                demanded.insert((how.to_owned(), field.to_owned()));
            }
        }
    }
    demanded
}

/// For every field a rendering could normalise, two records the corpus holds
/// that the rendering collapses onto one.
///
/// A pair, and not a value: a corpus entry on its own says nothing, because a
/// digest is only wrong relative to another digest. The two records must differ
/// **in that field alone** — a pair differing anywhere else still hashes apart
/// under the weakening, and would make this pass for the wrong reason.
#[track_caller]
fn the_corpus_can_tell_a_normalisation_from_the_identity(
    corpus: &[(String, Record)],
) -> std::collections::BTreeSet<(String, String)> {
    let mut proved = std::collections::BTreeSet::new();
    for (how, render, fields) in NORMALISATIONS {
        for field in fields {
            let witness = corpus.iter().enumerate().find_map(|(at, (left, one))| {
                corpus[at + 1..].iter().find_map(|(right, other)| {
                    let departs = where_a_record_departs(one, other);
                    let (here, there) = (the_text_of(one, field), the_text_of(other, field));
                    (departs == [(*field).to_owned()]
                        && here != there
                        && render(&here) == render(&there))
                    .then(|| (left.clone(), right.clone()))
                })
            });
            assert!(
                witness.is_some(),
                "no two records in this corpus differ in `{field}` alone and become equal \
                 when it is {how}. A `hashed_form` that rendered `{field}` that way would \
                 be the identity on every entry here and on every edit \
                 `tamper::edits()` makes, so the whole suite would pass while two \
                 different records landed on one digest and `verify` answered `Ok` to \
                 the forgery"
            );
            proved.insert((how.to_owned(), (*field).to_owned()));
        }
    }
    proved
}

fn the_named_shapes_are_all_here(corpus: &[(String, Record)]) -> usize {
    let labels: std::collections::BTreeSet<&str> =
        corpus.iter().map(|(what, _)| what.as_str()).collect();
    assert_eq!(
        labels.len(),
        corpus.len(),
        "two entries of this corpus carry the same label, so naming one below does \
         not say which record is present"
    );
    let missing: Vec<&str> = MUST_HOLD
        .iter()
        .map(|(what, _)| *what)
        .filter(|what| !labels.contains(what))
        .collect();
    assert!(
        missing.is_empty(),
        "the corpus no longer holds {} of the shapes this test exists to compare, so \
         the collapse each one rules out is ruled out by nothing: {missing:?}",
        missing.len()
    );

    // **And the other direction, which [`MUST_HOLD`]'s own doc claims.** That
    // doc says adding a shape needs a line here; measured without this, a
    // corpus entry carrying a label no row names leaves every test green. A
    // shape nothing names is a record whose collapse this file has no opinion
    // about, arriving unremarked — which is how the corpus would stop being the
    // census the counts below read it as.
    let unnamed: Vec<&str> = labels
        .iter()
        .filter(|what| !MUST_HOLD.iter().any(|(named, _)| named == *what))
        .copied()
        .collect();
    assert!(
        unnamed.is_empty(),
        "the corpus holds {} shape(s) `MUST_HOLD` does not name, so a record arrived \
         here without anyone saying what collapse it rules out: {unnamed:?}",
        unnamed.len()
    );

    let held = |what: &str| -> &Record {
        &corpus
            .iter()
            .find(|(label, _)| label == what)
            .expect("named above and present, which the check above just made true")
            .1
    };

    // **The label is not the shape, and the shape is what a weakening needs
    // gone.** Every assertion above this line reads a *string*: measured, with
    // the named entry hollowed out — `reversed.actor_chain` given `[b, c]`
    // instead of `[b, a]`, so it is no longer the reverse of anything — and
    // `hashed_form` sorting the chain, every `ok` line is identical to a clean
    // run. `labels.len() == corpus.len()` holds, nothing is missing, and
    // `distinct >= named - 3` comes to exactly 29: the corpus intact by every
    // other measure this file takes, while the collapse it exists to rule out
    // is live.
    //
    // So each named shape is compared to `the_base_record()` in the field its
    // own name claims. A shape given a value it does not name, or given nothing
    // at all, fails here rather than counting towards a floor.
    let base = the_base_record();
    for (what, fields) in MUST_HOLD {
        let record = held(what);
        let mut departs = where_a_record_departs(record, &base);
        let mut named: Vec<String> = fields.iter().map(|field| (*field).to_owned()).collect();
        departs.sort_unstable();
        named.sort_unstable();
        assert_eq!(
            departs, named,
            "`{what}` departs from the base record in {departs:?} and its name says \
             {named:?}. A named shape given a value its label does not claim — or given \
             back one `base()` already holds — keeps the label, keeps the count, and \
             rules out nothing"
        );
        // And the comparator above reads every field: an empty answer has to
        // mean the two records are equal, or a field it forgot is a departure
        // nothing here can see.
        assert_eq!(
            departs.is_empty(),
            record == &base,
            "`where_a_record_departs` says {departs:?} about `{what}` and the derived \
             comparison disagrees, so that function is missing a field of `Record`"
        );
    }

    // **And every field is departed in by something**, which no per-entry
    // assertion above can say: each one reads the record filed under one label,
    // and a field the whole corpus stopped varying departs nowhere at all.
    every_field_is_departed_in_by_some_named_shape();

    // **And the one relation that holds between two entries.** "The same two
    // actors, outermost last" is a claim about another entry, which no
    // per-record check can state: both of them depart from the base in
    // `actor_chain`, and both would still do so if one were `[b, c]`.
    let forward = &held("an actor chain of two").actor_chain;
    let backward = &held("the same two actors, outermost last").actor_chain;
    let flipped: Vec<_> = backward.iter().rev().cloned().collect();
    assert_eq!(
        &flipped, forward,
        "`the same two actors, outermost last` holds {backward:?}, and reversing it \
         gives {flipped:?} rather than `an actor chain of two`'s {forward:?}. These two \
         entries are the whole of what stops `hashed_form` sorting the chain, and a \
         sorted chain makes an agent acting as the operator hash the same as an \
         operator acting through the agent"
    );
    assert!(
        forward.len() == 2 && forward[0] != forward[1],
        "`an actor chain of two` holds {forward:?}, and a chain of one — or of one \
         identifier twice — is its own reverse, so the pair above would compare equal \
         with the order never varied"
    );

    // The chain that repeats an actor, which is the other order-sensitive shape.
    let repeated = &held("an actor appearing twice").actor_chain;
    let once: Vec<&OpaqueId> = repeated.iter().fold(Vec::new(), |mut seen, actor| {
        if !seen.contains(&actor) {
            seen.push(actor);
        }
        seen
    });
    assert!(
        repeated.len() > once.len(),
        "`an actor appearing twice` holds {repeated:?}, which repeats nothing, so a \
         `hashed_form` that de-duplicated the chain would pass over it"
    );

    MUST_HOLD.len()
}

/// Every pair of the corpus, compared in both directions.
///
/// Split from the test, which crossed clippy's line limit with it inline — the
/// same reason the corpus itself is three functions.
#[track_caller]
fn no_pair_of_them_lands_on_one_digest(hashed: &[(String, String, [u8; 32])]) {
    let mut collisions = Vec::new();
    for (index, (one, left_shape, left)) in hashed.iter().enumerate() {
        for (other, right_shape, right) in hashed.iter().skip(index + 1) {
            if left == right && left_shape != right_shape {
                collisions.push(format!("  `{one}` and `{other}` hash the same"));
            }
            // And the other direction, which would mean the digest depends on
            // something outside the record.
            if left != right && left_shape == right_shape {
                collisions.push(format!(
                    "  `{one}` and `{other}` are the same record and hash differently"
                ));
            }
        }
    }
    assert!(
        collisions.is_empty(),
        "two different records land on one digest, so one can be swapped for the \
         other and every hash, link and verification still agrees:\n{}",
        collisions.join("\n")
    );
}

/// The labels that stand over one record, which must be the base's and no other.
///
/// **The distinct floor's allowance is exactly the collapse it pays for.** `n`
/// labels over one record cost `n - 1` distinct shapes, not `n`,
/// so `named - restate_the_base` would leave a unit of slack — spendable by a
/// duplicate nothing declared. Measured with that
/// allowance: `corpus = 87`, `named = 87`, `distinct = 83`,
/// `restate_the_base = 4`, `83 >= 83` green, with two repeated shapes in the
/// corpus rather than one — the base's four labels,
/// and `a resource that is the text of the id`
/// beside `the same resource written as text`, both holding `Value::text("1")`.
///
/// So the arithmetic below is derived rather than allowed for, and this says the
/// only record written twice is the one the table declares. A second undeclared
/// duplicate is then a failure here, where it names both labels, rather than a
/// unit quietly taken out of a floor.
#[track_caller]
fn the_only_record_written_twice_is_the_base(hashed: &[(String, String, [u8; 32])]) -> usize {
    let mut by_shape: std::collections::BTreeMap<&str, Vec<&str>> =
        std::collections::BTreeMap::new();
    for (what, shape, _) in hashed {
        by_shape
            .entry(shape.as_str())
            .or_default()
            .push(what.as_str());
    }
    let repeated: Vec<(&str, &Vec<&str>)> = by_shape
        .iter()
        .filter(|(_, labels)| labels.len() > 1)
        .map(|(shape, labels)| (*shape, labels))
        .collect();
    let base_shape = format!("{:?}", the_base_record());
    assert_eq!(
        repeated
            .iter()
            .map(|(shape, _)| *shape)
            .collect::<Vec<&str>>(),
        vec![base_shape.as_str()],
        "the record(s) this corpus writes more than once are {repeated:#?}, and the base \
         record is the only one any label is allowed to restate: a duplicate under two \
         labels is one collapse counted twice, and it is spent against the floor below"
    );
    repeated.iter().map(|(_, labels)| labels.len()).sum()
}

/// No two different records hash the same, over every field and every boundary.
///
/// **Criterion 3 says any mutation of any field breaks verification, and that is
/// a claim about injectivity.** The suite proves it by *editing* fields one at a
/// time, which shows each edit is noticed; it does not show that two records
/// differing in some other way cannot land on one digest. A collision is the one
/// tamper the chain cannot see: swap the record for its twin and every hash,
/// every link and every `verify` still agrees.
///
/// The dangerous shape is a field whose rendering can absorb a neighbour's
/// content — an actor chain written as joined text, a resource flattened to a
/// string, an optional field whose absence renders as the empty string that
/// another field could hold. `hashed_form` writes named pairs through
/// `nc-canon`, which is what makes that impossible; this is what says so.
///
/// Every pair, not a sample: the corpus is small enough to compare exhaustively,
/// which is stronger than any number of random draws.
#[test]
fn no_two_different_records_hash_the_same() {
    use nc_journal::Entry;

    let corpus = the_shapes_a_flattening_would_collapse();

    let named = the_named_shapes_are_all_here(&corpus);
    // And the product of field and normalisation class, which the shapes above
    // are indexed too coarsely to say anything about.
    let classes = the_corpus_can_tell_a_normalisation_from_the_identity(&corpus);
    // **Floored against the types, because a table held against itself is an
    // agreement and not a floor.** Both sides of an equality reading
    // `NORMALISATIONS` move together when a row is deleted, and a floor of
    // twelve pairs against a table naming twenty leaves eight pairs of slack.
    // What each field's type can hide is settled somewhere else entirely, so
    // that is what the product is held against.
    assert_eq!(
        classes,
        the_renderings_each_field_can_hide(),
        "the corpus witnesses {classes:?} and the types admit a different set, so either \
         a field was dropped from a class whose rendering can hide it, or a class claims \
         a field whose type never holds two texts it collapses"
    );
    // And a cut at every length, which one chosen length cannot stand for.
    // And the pairs a rendering over the type collapses, which the product of
    // (field × text-class) cannot reach.
    let variants = the_corpus_can_tell_a_variant_from_its_text(&corpus);
    assert_eq!(
        variants,
        VARIANT_PAIRS.len(),
        "{variants} variant pair(s) were proved and the table names {}",
        VARIANT_PAIRS.len()
    );
    // And the pairs that vary one level down, which every class above is indexed
    // too shallowly to reach: each of them varies a whole field.
    let nested = the_corpus_can_see_a_rendering_that_reaches_inside(&corpus);
    assert_eq!(
        nested,
        NESTED_PAIRS.len(),
        "{nested} nested pair(s) were proved and the table names {}",
        NESTED_PAIRS.len()
    );
    the_nested_lists_keep_their_order_and_their_repeats(&corpus);
    // And every nesting the corpus keeps a text in: the three named pairs above
    // are all at one container, and a resource takes a value of any shape.
    let nestings = every_nesting_the_corpus_holds_has_a_witness(&corpus);
    assert!(
        nestings > NESTED_PAIRS.len(),
        "{nestings} (nesting, rendering) pair(s) were proved and `NESTED_PAIRS` names {} \
         at one container, so the corpus reaches no further in than those rows do",
        NESTED_PAIRS.len()
    );
    // And the keys, which no `fn(&str) -> String` over the canonical text can
    // tell from what is under them.
    let keyed = the_corpus_can_see_a_rendering_of_the_keys(&corpus);
    assert_eq!(
        keyed,
        NORMALISATIONS
            .iter()
            .filter(|(_, _, fields)| fields.contains(&"resource"))
            .count(),
        "{keyed} key pair(s) were proved and `NORMALISATIONS` renders a resource a \
         different number of ways, so one of those renderings is claimed by no key pair"
    );
    let lengths = the_corpus_can_see_a_truncation_at_every_length(&corpus);
    // **The same arithmetic, coming from the types.** With `8 * 63` — 504 —
    // against 1065 proved, the three numeric rows — 49 pairs — could be dropped
    // with this green, and neither floor would move when the stem the long
    // pairs are built from is cut from 127 characters to 77.
    let reach: usize = CARRIES_TEXT
        .into_iter()
        .map(|field| the_reach_a_field_must_have(field) - 1)
        .sum();
    assert!(
        lengths >= reach,
        "{lengths} (field, cut) pair(s) were proved and the types these pairs are built \
         from reach {reach}, so a pair was shortened without the floor moving"
    );

    // **Distinct records, judged by the struct rather than by the digest.** The
    // corpus is written by hand and three of its entries restate a value
    // `base()` already has — so they *are* the base record, and reporting those
    // as colliding reports only that one record hashes consistently. What is
    // being asked is whether two records that *differ* can land on one digest,
    // and `Debug` is the faithful rendering of what differs.
    let hashed: Vec<(String, String, [u8; 32])> = corpus
        .iter()
        .map(|(what, record)| {
            (
                what.clone(),
                format!("{record:?}"),
                Entry::chain(0, None, record.clone()).hash,
            )
        })
        .collect();
    let distinct = hashed
        .iter()
        .map(|(_, shape, _)| shape)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    //
    // Floored against `MUST_HOLD` rather than at a number, so that hollowing an
    // entry out — keeping its label and giving it back a value `base()` already
    // holds — is a failure. Three of the named shapes do that deliberately
    // (`no deciding policy`, `action Read` and `outcome Allowed` each restate
    // the base record, which is what makes the reverse direction below a real
    // comparison), so thirty-two labels stand for twenty-nine distinct records.
    //
    // **Counted from the table rather than written here.** A hand-written `3`
    // is slack a fourth hollowed entry can be spent against; the table names
    // which of them restate the base, by giving them no field to depart in, and
    // `the_named_shapes_are_all_here` has already refused any entry that
    // departs somewhere other than where its own name says.
    let restate_the_base = MUST_HOLD
        .iter()
        .filter(|(_, fields)| fields.is_empty())
        .count();
    // And the arithmetic is `n - 1` and not `n`: labels over one record cost
    // one fewer distinct shape than there are labels. See
    // `the_only_record_written_twice_is_the_base` for the unit that costs and
    // what would spend it. `saturating_sub`, because a table naming no
    // restatement at all costs nothing and must floor at `named`.
    let written_twice = the_only_record_written_twice_is_the_base(&hashed);
    assert_eq!(
        written_twice, restate_the_base,
        "{written_twice} label(s) stand over the base record and the table gives \
         {restate_the_base} of them no field to depart in, so the floor below is \
         allowing for a collapse of a different size than the one that is here"
    );
    assert!(
        distinct >= named - restate_the_base.saturating_sub(1),
        "only {distinct} of {} record(s) here differ from each other, so the comparison \
         below is mostly a record against itself — {} shape(s) are named above and \
         {restate_the_base} of them restate `base()`, which costs {} distinct shape(s) \
         and no more",
        hashed.len(),
        named,
        restate_the_base.saturating_sub(1)
    );

    no_pair_of_them_lands_on_one_digest(&hashed);

    // And the same record twice is the same digest, so the comparison above is
    // about difference and not about a hash that never repeats.
    let (_, first) = &corpus[0];
    assert_eq!(
        Entry::chain(0, None, first.clone()).hash,
        Entry::chain(0, None, first.clone()).hash,
        "one record hashed twice gave two digests"
    );
}

/// One row of [`PAIRS_THAT_MUST_DIFFER`]: what the difference is, and the two
/// resources that carry it.
type BuriedPair = (&'static str, fn() -> (Value, Value), fn(&Value) -> Value);

/// The differences a `hashed_form` must keep, whatever they are wrapped in.
///
/// **Every structural floor in this file holds past one container,
/// because `resource` takes a `Value` of any shape.**
/// [`every_nesting_the_corpus_holds_has_a_witness`] generalises the floor about
/// *text* over depth, and the others — the keys a rendering may rename, the
/// order and repeats of a list, and the variants it may collapse into their
/// text — held at exactly one container would be the identity one container
/// further in. Measured on the shipped rendering with each collapse planted in
/// turn and nothing deeper asked, `cargo xtask check` prints `check passed`
/// while `{"target":{"Owner":"alice"}}` and `{"target":{"owner":"alice"}}` land
/// on one digest, `{"target":{"id":["a","b"]}}` on the digest of `["b","a"]`,
/// and `{"elevated":true}` on the digest of `{"elevated":"true"}`.
///
/// So the property is stated where it belongs — over the digest rather than
/// over the corpus — and the depth is quantified rather than chosen.
/// [`no_pair_that_must_differ_survives_being_buried`] wraps both sides of every
/// row below at every depth up to [`AS_DEEP_AS_THIS_GOES`], through an object
/// and through a list, and asks the two entries for their hashes. A row here
/// cannot be satisfied by a corpus that happens to hold the right shape; it is
/// satisfied only by a rendering that is injective at that depth.
const PAIRS_THAT_MUST_DIFFER: [BuriedPair; 11] = [
    (
        "a key in another case",
        || {
            (
                Value::object([("Owner", Value::text("alice"))]),
                Value::object([("owner", Value::text("alice"))]),
            )
        },
        |value| with_its_keys_rendered(value, str::to_lowercase),
    ),
    (
        "a key spelled with a space",
        || {
            (
                Value::object([("owner", Value::text("alice"))]),
                Value::object([("owner ", Value::text("alice"))]),
            )
        },
        |value| with_its_keys_rendered(value, |key| key.trim().to_owned()),
    ),
    (
        "a list the other way round",
        || {
            (
                Value::Array(vec![Value::text("a"), Value::text("b")]),
                Value::Array(vec![Value::text("b"), Value::text("a")]),
            )
        },
        |value| match value {
            Value::Array(items) => {
                let mut sorted = items.clone();
                sorted.sort_by_key(|item| format!("{item:?}"));
                Value::Array(sorted)
            }
            other => other.clone(),
        },
    ),
    (
        "a list repeating a value",
        || {
            (
                Value::Array(vec![Value::text("a"), Value::text("a")]),
                Value::Array(vec![Value::text("a")]),
            )
        },
        |value| match value {
            Value::Array(items) => {
                let mut kept: Vec<Value> = Vec::new();
                for item in items {
                    if !kept.contains(item) {
                        kept.push(item.clone());
                    }
                }
                Value::Array(kept)
            }
            other => other.clone(),
        },
    ),
    (
        "a trailing ASCII space",
        || {
            (
                Value::text("Contract-Alice"),
                Value::text("Contract-Alice "),
            )
        },
        |value| match value {
            Value::Text(text) => Value::text(text.trim()),
            other => other.clone(),
        },
    ),
    (
        "a flag against its text",
        || (Value::Bool(true), Value::text("true")),
        |value| match value {
            Value::Bool(flag) => Value::text(flag.to_string()),
            other => other.clone(),
        },
    ),
    (
        "an absence against empty text",
        || (Value::Null, Value::text("")),
        |value| match value {
            Value::Null => Value::text(String::new()),
            other => other.clone(),
        },
    ),
    (
        "an invisible formatting character",
        || (Value::text("Delete"), Value::text("Delete\u{200b}")),
        |value| match value {
            Value::Text(text) => {
                Value::text(text.chars().filter(char::is_ascii).collect::<String>())
            }
            other => other.clone(),
        },
    ),
    (
        "a member that is absent",
        || {
            (
                Value::object([("delegated_to", Value::Null)]),
                Value::object(Vec::<(&str, Value)>::new()),
            )
        },
        |value| match value {
            Value::Object(fields) => Value::object(
                fields
                    .iter()
                    .filter(|(_, field)| !matches!(field, Value::Null))
                    .map(|(key, field)| (key.clone(), field.clone())),
            ),
            other => other.clone(),
        },
    ),
    (
        "an empty list against an empty object",
        || {
            (
                Value::Array(vec![]),
                Value::object(Vec::<(&str, Value)>::new()),
            )
        },
        |value| match value {
            Value::Array(items) if items.is_empty() => Value::Null,
            Value::Object(fields) if fields.is_empty() => Value::Null,
            other => other.clone(),
        },
    ),
    (
        "an empty list against an absence",
        || (Value::Array(vec![]), Value::Null),
        |value| match value {
            Value::Array(items) if items.is_empty() => Value::Null,
            Value::Object(fields) if fields.is_empty() => Value::Null,
            other => other.clone(),
        },
    ),
];

/// How deep [`no_pair_that_must_differ_survives_being_buried`] buries a pair.
///
/// Three is one container past the deepest this corpus holds, which is what
/// makes the test say something the corpus cannot. It is not a claim about
/// depth in general: a rendering that reaches only at four is invisible here,
/// and no finite test closes that. What it does close is the shape every
/// falsification of this criterion has actually taken — a rule written for the
/// top level and read as though it were written for the type.
const AS_DEEP_AS_THIS_GOES: usize = 3;

/// The same value, buried `depth` containers further in.
fn buried(value: Value, depth: usize, through_a_list: bool) -> Value {
    let mut held = value;
    for _ in 0..depth {
        held = if through_a_list {
            Value::Array(vec![held])
        } else {
            Value::object([("target", held)])
        };
    }
    held
}

/// Two resources that must differ still differ at every depth.
#[test]
fn no_pair_that_must_differ_survives_being_buried() {
    let digest = |resource: Value| {
        let record = Record::new(
            OpaqueId::new("principal-1").expect("a valid principal"),
            "Read",
            resource,
            Outcome::Allowed,
            1_767_225_600_000,
        );
        Entry::chain(0, None, record).hash_hex()
    };
    // **The depth is floored by the corpus, not chosen.** A depth held to
    // nothing but itself could be set to 0 with this test green and
    // `cargo xtask check` byte-identical — measured — so it is held above the
    // deepest text the corpus holds.
    let mut texts = Vec::new();
    for (_, record) in the_shapes_a_flattening_would_collapse() {
        where_a_value_keeps_its_texts(&record.resource, "", &mut texts);
    }
    let deepest = texts
        .iter()
        .map(|at| {
            if at.is_empty() {
                0
            } else {
                at.split('/').count()
            }
        })
        .max()
        .unwrap_or(0);
    assert!(
        AS_DEEP_AS_THIS_GOES > deepest,
        "the corpus keeps a text {deepest} container(s) in and this buries no deeper \
         than {AS_DEEP_AS_THIS_GOES}, so it says nothing the corpus does not already"
    );
    let mut compared = 0_usize;
    for (what, pair, collapse) in PAIRS_THAT_MUST_DIFFER {
        let (here, there) = pair();
        assert_eq!(
            collapse(&here),
            collapse(&there),
            "the collapse `{what}` names does not merge this row's own two values, so \
             the row carries some other difference under that label"
        );
        assert_ne!(
            here, there,
            "the two sides of `{what}` are the same value, so this row asks nothing"
        );
        for depth in 0..=AS_DEEP_AS_THIS_GOES {
            for through_a_list in [false, true] {
                let one = digest(buried(here.clone(), depth, through_a_list));
                let other = digest(buried(there.clone(), depth, through_a_list));
                let wrapping = if through_a_list { "list" } else { "object" };
                assert_ne!(
                    one, other,
                    "`{what}` is invisible to `hashed_form` when it sits {depth} \
                     {wrapping}(s) in: two records differing only there land on {one}, so \
                     `verify` answers `Ok` over a rewrite of one into the other"
                );
                compared += 1;
            }
        }
    }
    assert_eq!(
        compared,
        PAIRS_THAT_MUST_DIFFER.len() * (AS_DEEP_AS_THIS_GOES + 1) * 2,
        "the loop compared {compared} pair(s), so a row or a depth was skipped and this \
         test is weaker than it reads"
    );
    // **And a buried row per class of `NORMALISATIONS`, derived rather than
    // written.** The eight rows above close `trimmed` and
    // `stripped of what is not ASCII` at depth; without these, `case-folded`
    // and `stripped of its combining marks` have no row at all, and a
    // `hashed_form` folding case three containers in passes
    // `cargo xtask check`.
    let mut classes = 0_usize;
    for (how, render, fields) in NORMALISATIONS {
        if !fields.contains(&"resource") {
            continue;
        }
        let probe = PROBES
            .into_iter()
            .find(|text| render(text) != *text)
            .unwrap_or_else(|| {
                panic!(
                    "no probe in `PROBES` is changed by `{how}`, so the class is closed by nothing"
                )
            });
        let folded = render(probe);
        for depth in 0..=AS_DEEP_AS_THIS_GOES {
            for through_a_list in [false, true] {
                let one = digest(buried(Value::text(probe), depth, through_a_list));
                let other = digest(buried(Value::text(folded.clone()), depth, through_a_list));
                assert_ne!(
                    one, other,
                    "a text {how} is invisible to `hashed_form` when it sits {depth} \
                     container(s) in: two records differing only there land on {one}"
                );
            }
        }
        classes += 1;
    }
    assert_eq!(
        classes,
        NORMALISATIONS
            .iter()
            .filter(|(_, _, fields)| fields.contains(&"resource"))
            .count(),
        "a class of `NORMALISATIONS` that renders a resource was closed by no buried row"
    );
}
