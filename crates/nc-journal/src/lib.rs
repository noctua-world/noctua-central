//! The command journal, which **is** the audit log.
//!
//! `NOCTUA.md §14.9` and `reference/identity-and-authorization.md §11`, and the
//! sentence that matters most is the short one: **do not build two.** Undo is
//! verified against it; audit reads everything; they have different retentions
//! and one structure. A second log is two answers to "what happened", and the
//! day they disagree there is no way to tell which is lying.
//!
//! # Why this exists before anything that could write to it
//!
//! `NOCTUA-PLAN.md §P0.3` makes it a hard ordering constraint, and the reason is
//! not scheduling: retrofitting audit over existing history is impossible.
//! Entries written before the chain existed cannot be hashed into it afterwards
//! without inventing what they said.
//!
//! # The fields were here before anything filled them
//!
//! An [`Entry`] records the realm, which policy decided, the pack version, the
//! epoch, the grant used and the **actor chain**. The kernel's decision point
//! fills four of the last five on every entry it writes, from what it asked
//! `nc-authz` (`crates/nc-kernel/src/dispatch.rs`), and all five were here
//! before anything set them, because adding a field to a chained structure
//! later **invalidates every hash before it**. The cost of carrying an empty
//! column is nothing; the cost of adding one afterwards is a chain that cannot
//! be verified across the change.
//!
//! **`actor_chain` is the fifth, and it is still empty.** It is the one
//! `§14.6` calls load-bearing — *delegation, never impersonation: the log
//! always shows both* — and `Record::new` sets it to an empty `Vec` on every
//! entry, which the kernel passes on until agents act (`NOCTUA-PLAN.md`,
//! Stage 15). Whoever wires that reads this paragraph to learn what is still
//! empty: measured over a real history,
//! `actor_chain lengths: [0, 0, 0, 0, 0, 0]`. The hash covers it, so the chain
//! is not weaker for it; what was missing is that anybody knew.
//!
//! # A history this code cannot read is not one to extend
//!
//! An unreadable row at the tail **stops appends**, and says which row. That is
//! a deliberate reversal: for a while an undecodable tail fell back to reading
//! the sequence and hash without parsing the record, on the rule that a foreign
//! row must not brick an append-only log. Two measurements retired it.
//!
//! It does not achieve that. `head()` parses the hash *column*, and at the time
//! `hash TEXT` carried no constraint, so one `INSERT` with a non-hex hash
//! refused there too — the fallback changed which sentence a caller reads, not
//! whether the append lands. (`0009` constrains the column now, written after
//! this paragraph and quoting it; the reader's own width guard is kept for the
//! row that gets past a constraint, and is held from both sides.) And where the
//! fallback did work, it handed the next honest entry whatever
//! sixty-four characters the foreign row put in `hash`: measured, *the honest
//! entry chained onto the attacker's chosen hash*.
//!
//! So the choice was between refusing and extending a history this code cannot
//! read, and `§14.9` decides it — the audit log fails closed. **What an operator
//! does** is read the named row, decide what it was, and append a new anchor;
//! the table refuses `DELETE`, so the unreadable row stays where it is and the
//! decision is recorded above it, with a person in it rather than a default.
//!
//! **And that anchor goes in through [`Store`], not through [`Journal`].**
//! `Journal::append` reads the tail before it writes, so it is the thing
//! refusing and cannot be the thing that repairs — a paragraph here said
//! "append a new anchor" without saying which door, and through the only door a
//! reader would try, it is impossible. `Store::append` does not read the tail;
//! it hands PostgreSQL a statement whose `WHERE` checks the sequence and the
//! link against what is actually there, so an operator who has read the
//! unreadable row and decided what it was writes an entry that chains onto it
//! deliberately. That is the whole difference between a repair and a forgery:
//! the link is stated, and refused if it is wrong.
//!
//! # What the chain does and does not prove
//!
//! It proves that no entry was changed or removed **without also rewriting every
//! entry after it**. It does not prove that an entry was ever written, and that
//! is true in both directions:
//!
//!   * **Shorter.** Somebody with write access to the whole table can produce a
//!     shorter, internally consistent history.
//!   * **Rewritten at the tail, and this one is cheapest of all.** The last
//!     entry has nothing after it, so changing it costs no downstream rewrites:
//!     recompute its own hash and the chain is consistent again. Measured
//!     against a real database, in a schema of its own:
//!
//!     ```text
//!     plain UPDATE   -> refused: the journal is append-only
//!     ALTER … USING  -> ALLOWED
//!     rows now: [(0,"user-1","Read"),(1,…),(2,…),(3,"attacker","Escalate")]
//!     verify:   Ok(4)
//!     ```
//!
//!     `ALTER TABLE … USING` rewrites every row and fires no row trigger, so all
//!     three `ENABLE ALWAYS` triggers were still in place while the last entry's
//!     principal and action were replaced. It needs table ownership, which the
//!     application role must not have; that is a deployment property, not
//!     something this crate can hold, and it is written here because the
//!     paragraph above would otherwise read as though the chain stopped it.
//!
//!     What closes it is an anchor kept somewhere the table's owner cannot
//!     reach — the head hash, published or countersigned — and [`Journal::verify`]
//!     already takes the entry before the range so such an anchor has somewhere
//!     to enter. Supplying one is a later stage's, which `NOCTUA.md §14.9`
//!     leaves to choose among the ways it lists (`AGENTS.md`, *What is
//!     deliberately not claimed*); the local kernel's `journal.verify` reads the
//!     chain and keeps no anchor.
//!   * **Longer, and this one is cheaper.** The digest is unkeyed and
//!     [`entry::DOMAIN`] is public, so a party who can do nothing but `INSERT`
//!     can read the tail, chain a fabricated entry onto it and append. `verify`
//!     answers `Ok` about the longer history, because it is a well-formed
//!     chain. The triggers guard UPDATE, DELETE and TRUNCATE and say nothing
//!     about INSERT — by design, since appending is what a journal does.
//!
//! The append direction has a consequence beyond the forged row: an INSERT-only
//! party can write a `unit-committed` bracket for a work unit that is still
//! open, and the legitimate writer is then permanently locked out of its own
//! group with [`Error::UnitClosed`] while the history reads clean. That is an
//! availability failure produced with strictly less privilege than the
//! shortening one needs — and, since [`Store::unit_closing`] reports a unit's
//! first closing bracket, the outcome it reports for that unit is the
//! forger's, so whatever acts on that answer acts on the forger's choice.
//!
//! Detecting either needs an anchor outside the machine — publishing the head
//! hash somewhere the same person does not control — and closing the append
//! direction specifically needs the digest to be *keyed*, so that chaining onto
//! the tail takes something an INSERT-only party does not have. Both are later
//! decisions and neither is claimed here.

#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
// Every doctest of this crate forbids `unsafe` as the crate does: a
// doctest is a crate of its own, outside the root's `forbid`.
#![doc(test(attr(forbid(unsafe_code))))]
pub mod entry;
pub mod memory;
pub mod postgres;
pub mod unit;

pub use entry::{Entry, Outcome, Record};
pub use memory::InMemory;
pub use unit::{Closing, WorkUnit};

use core::future::Future;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use nc_canon::Value;
use nc_catalog::ErrorCode;
use nc_ids::{Clock, Minter, OpaqueId, Snowflake, SystemClock};

/// Where entries are kept.
///
/// A trait so the chain can be exercised without a database. That is not a
/// convenience: acceptance criterion 3 asks that **any** mutation of **any**
/// earlier entry break verification at that point, and proving it at every
/// position of a synthetic history means building hundreds of histories. A test
/// that needed PostgreSQL for each would be a test that runs once a week.
///
/// # Why the methods are asynchronous
///
/// The store that runs is PostgreSQL through `sqlx`, which has no synchronous
/// API. The alternative was a runtime owned inside the store and a `block_on`
/// per call, and that is a trap rather than a compromise: `block_on` panics when
/// it is called from inside an async context, and the kernel that writes to this
/// journal on every command **is** an async context.
///
/// Written as `-> impl Future<…> + Send` rather than as `async fn`, because a
/// bare `async fn` in a trait promises nothing about `Send` and the futures have
/// to cross threads on a multi-threaded runtime. No `async_trait` dependency:
/// Rust 2024 expresses this natively.
///
/// **Not dyn-compatible, and that is a choice a caller has to know about.**
/// Every method returns `impl Future`, so `Box<dyn Store>` is `error[E0038]`,
/// *the trait `Store` is not dyn compatible*, naming every method — the
/// provided `unit_closed` among them. A
/// kernel that picks its store at run time has to be generic over `S: Store`
/// everywhere instead — which is the trade `impl Future` buys: no `async_trait`
/// dependency and no boxed allocation per call.
pub trait Store {
    /// The last entry written, or `None` when the journal is empty.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unavailable`] when the store cannot be reached, and
    /// [`Error::Undecodable`] when the last row is not one this code wrote.
    ///
    /// **And [`Error::Refused`], which every one of these reaches through
    /// `unavailable`.** That function maps SQLSTATE classes 42, 28, 3D and 3F
    /// — a missing `GRANT`, a dropped table, a schema that is not there — to
    /// `Refused` rather than to `Unavailable`, and only `Store::head` named
    /// it. `Unavailable` is retryable and `Refused` is not, so a caller
    /// written against the shorter sentence retries a missing privilege for
    /// ever. Reached through a call rather than built here, which is why
    /// `every_errors_section_names_what_its_body_builds` cannot ask for it
    /// and a reviewer had to.
    fn tail(&self) -> impl Future<Output = Result<Option<Entry>, Error>> + Send;

    /// What a new entry chains onto: the last sequence and its hash.
    ///
    /// **Appending must not depend on being able to read the tail's record.**
    /// A row a second connection inserted — the trigger guards `UPDATE` and
    /// `DELETE` and says nothing about `INSERT` — made `tail()` fail, and
    /// `Journal::append` reads the tail before it can chain, so *every future
    /// write* failed for ever. The table is append-only by design, so the row
    /// could not be removed either: one `INSERT` made the audit log permanently
    /// unwritable.
    ///
    /// Chaining needs two values, and neither is the part that fails to decode.
    /// A poisoned row therefore breaks *verification*, which is what
    /// verification is for, and stops nothing else.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unavailable`] when the store cannot be reached,
    /// [`Error::Refused`] when it answers and refuses — a grant, a schema, a
    /// role — and [`Error::Undecodable`] when the row it reads back is one this
    /// code did not write.
    ///
    /// **The last of those was missing, and it inverts `retryable`.**
    /// `JournalUnavailable` is retryable and `JournalChainBroken` is not, so a
    /// caller written against the old sentence retries a poisoned `hash` column
    /// for ever. `Postgres::head` returns it from four: `try_get("sequence")`,
    /// the unsigned conversion, `try_get("hash")`, and the digest conversion —
    /// the body comment at the fourth opens *"the fourth read, which the
    /// paragraph above said was converted and was not"*, and this sentence said
    /// three anyway.
    fn head(&self) -> impl Future<Output = Result<Option<(u64, [u8; 32])>, Error>> + Send;

    /// Appends an entry. The caller has already chained it.
    ///
    /// **Every implementation refuses an entry it could not read back**, by
    /// calling [`readable`] first. The rule used to live in `Journal::append`,
    /// which is `pub(crate)` — so `Store::append`, which is public and which
    /// this crate's own tests call directly, walked straight past it and
    /// reproduced the defect the rule exists to prevent. `postgres.rs`'s own
    /// module header names that shape: *a rule enforced only where it is
    /// convenient to enforce it is a rule about one code path.*
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unavailable`] when the store cannot be reached,
    /// [`Error::Refused`] when it answers and refuses, [`Error::OutOfOrder`]
    /// when `entry` does not follow the tail — which is what a second writer
    /// racing this one looks like from here — [`Error::TooDeep`] when the entry
    /// nests past what the reader will follow, and [`Error::Unstorable`] for a
    /// hash that is not the hash of the contents, a sequence a store cannot
    /// hold, or a NUL byte in a field.
    ///
    /// It also returns [`Error::UnitClosed`] and [`Error::UnitAlreadyOpen`] for
    /// the two bracket rules, [`Error::Undecodable`] when the tail it chains
    /// onto is a row this code did not write, and [`Error::Contended`] when a
    /// second writer took the sequence between the read and the insert.
    ///
    /// **Four of those were named by nothing.** A trait method has no body, so
    /// the section gate counted this one and judged it against nothing —
    /// measured on 2026-09-08, the link to [`Error::Unstorable`] deleted from
    /// the paragraph above and `cargo run -q -p xtask -- check` still at
    /// `check passed`, `ok 1223 test(s)`. The refusals of a trait method are the
    /// refusals of its implementations, and both of this crate's are read now:
    /// see `what_an_implementation_builds`. `Contended` is reached through a
    /// call rather than built in either body, so it is here by hand.
    ///
    /// **`Unstorable` was missing and it is the commonest of them.** Every
    /// implementation calls [`readable`] as its first statement, so a caller
    /// whose entry is malformed gets it before the store is asked at all — and
    /// this crate's own tampering tests assert exactly that. It is not
    /// retryable and [`Error::Unavailable`] is, which is the pair
    /// `NC-JRNL-0005` was added to keep apart.
    fn append(&mut self, entry: Entry) -> impl Future<Output = Result<(), Error>> + Send;

    /// Every entry from `sequence` onwards, in order, stopping at a row this
    /// code cannot read.
    ///
    /// **The rows before a bad one are returned**, and the bad one is named.
    /// Failing the whole range on the first undecodable row meant one row a
    /// second connection inserted destroyed reads of every *other* row — so a
    /// history of ten thousand entries became unreadable because of the last
    /// one, and the append-only rule meant it could not be removed.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unavailable`] when the store cannot be reached, and
    /// [`Error::Unstorable`] when `sequence` is past what a store can hold —
    /// `Postgres::from`'s first statement is that conversion, before any query
    /// runs, so it is the refusal a caller sees first and this said only the
    /// other one.
    ///
    /// **And [`Error::Refused`], which every one of these reaches through
    /// `unavailable`.** That function maps SQLSTATE classes 42, 28, 3D and 3F
    /// — a missing `GRANT`, a dropped table, a schema that is not there — to
    /// `Refused` rather than to `Unavailable`, and only `Store::head` named
    /// it. `Unavailable` is retryable and `Refused` is not, so a caller
    /// written against the shorter sentence retries a missing privilege for
    /// ever. Reached through a call rather than built here, which is why
    /// `every_errors_section_names_what_its_body_builds` cannot ask for it
    /// and a reviewer had to.
    fn from(&self, sequence: u64) -> impl Future<Output = Result<Read, Error>> + Send;

    /// How `realm` closed `unit`: [`Outcome::UnitCommitted`] or
    /// [`Outcome::UnitAborted`], or `None` while this realm's history holds no
    /// closing bracket for it.
    ///
    /// **The one question a store answers about the end of a unit.**
    /// [`Store::unit_closed`] is this asked whether it has an answer at all, so
    /// "is it closed" and "how did it close" are one predicate and cannot
    /// disagree about which entries are closing brackets. The difference between
    /// the two answers is what a `bool` cannot carry: a unit that committed and
    /// one that was rolled back are both closed, and only the first stands.
    ///
    /// **The first closing bracket, in sequence order.** Every store refuses an
    /// entry for a unit that is already closed, a second closing bracket
    /// included, so there is one — and a second written around that refusal is
    /// not the bracket that closed the unit. Nothing decided which one answered
    /// until `how_a_unit_closed_is_its_first_closing_bracket_not_a_later_one`
    /// and its PostgreSQL twin in `in_postgres.rs` planted a second: both
    /// stores could answer with the last, every suite green.
    ///
    /// **What the first-bracket rule does not answer.** A party that can only
    /// `INSERT` can chain the *first* closing bracket onto a unit that is still
    /// open, with the outcome of its choosing: [`Store::append`] takes a
    /// closing bracket from anyone, and `verify` answers `Ok` over it. This
    /// then reports that party's outcome, and the owner's
    /// [`Journal::close_unit`] is refused [`Error::UnitClosed`]. The rule above
    /// stops a later bracket from changing the answer. Nothing here makes the
    /// first bracket the owner's; only a keyed digest or an anchor off the
    /// machine would (see the crate header). **A caller must not read
    /// `Some(UnitCommitted)` as proof that the unit's owner committed it.**
    /// `a_party_that_can_only_append_chooses_how_an_open_unit_closed` in
    /// `unit.rs` and its PostgreSQL twin in `in_postgres.rs` hold this, and
    /// fail on the day it stops being true.
    ///
    /// A unit is closed when the journal holds a `UnitCommitted` or
    /// `UnitAborted` bracket for it, and that is as true for a second process as
    /// for this one — so the answer comes from the store rather than from a flag
    /// in this process.
    ///
    /// **Three reasons this is a store question and not a read of the
    /// history.** It used to be `from(0)` and a scan, in `Journal::append`, and
    /// each of the three was measured:
    ///
    /// 1. **A poisoned row hid every bracket behind it.** `from` stops at the
    ///    first row it cannot decode — the row `postgres.rs` documents a second
    ///    connection as free to `INSERT` — so one such row made every closed
    ///    unit in the journal open again, and entries landed after their own
    ///    closing bracket. The columns that answer this question are `realm`,
    ///    `work_unit`, `action` and `result`; none of them is the part of a row
    ///    that fails to decode, which is the same argument [`Store::head`]
    ///    makes.
    ///
    ///    **`action` was added and this sentence was not.** Every
    ///    implementation asks four columns and the contract named three, so a
    ///    `Store` written to the contract fails this crate's own tests —
    ///    measured, `a_bracket_is_a_bracket_and_a_unit_is_opened_once` is what
    ///    catches it. `action` is what makes a bracket a bracket: without it an
    ///    ordinary `Write` carrying `Outcome::UnitCommitted` closes somebody
    ///    else's unit, which `memory.rs` records measuring against PostgreSQL.
    /// 2. **It read the whole table on every write.** Measured at 1200 entries:
    ///    3.201s inside a work unit against 0.679s outside one, growing
    ///    linearly, on a table that is append-only by design and whose retention
    ///    is a policy-pack property. O(n) per write is O(n²) over a history.
    /// 3. **It ignored the realm.** A journal opened for `realm-2` refused a
    ///    write on the strength of a bracket that exists only in `realm-1`.
    ///    `Journal::append` overwrites `record.realm` precisely so that one
    ///    realm cannot reach another's history, and the check read all of it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unavailable`] when the store cannot be reached.
    ///
    /// **And [`Error::Refused`], which every one of these reaches through
    /// `unavailable`.** That function maps SQLSTATE classes 42, 28, 3D and 3F
    /// — a missing `GRANT`, a dropped table, a schema that is not there — to
    /// `Refused` rather than to `Unavailable`, and only `Store::head` named
    /// it. `Unavailable` is retryable and `Refused` is not, so a caller
    /// written against the shorter sentence retries a missing privilege for
    /// ever. Reached through a call rather than built here, which is why
    /// `every_errors_section_names_what_its_body_builds` cannot ask for it
    /// and a reviewer had to.
    fn unit_closing(
        &self,
        realm: &OpaqueId,
        unit: nc_ids::Snowflake,
    ) -> impl Future<Output = Result<Option<Outcome>, Error>> + Send;

    /// Whether `realm` has already closed `unit`, either way.
    ///
    /// **Provided, and no implementation in this crate writes its own.** It is
    /// [`Store::unit_closing`] asked whether it has an answer, so "is it closed"
    /// and "how did it close" are one predicate and cannot disagree.
    /// `a_store_answers_whether_a_unit_is_closed_in_one_place` holds that for
    /// every store this crate writes, its tests' included.
    ///
    /// **That is one predicate per question, not one per store.** Each store
    /// writes the closing-bracket predicate three times: in `unit_closing`, and
    /// twice more on its own `append` path — `InMemory`'s reopen guard and its
    /// closed-unit guard, PostgreSQL's `INSERT … WHERE` and the diagnosis in
    /// `why_nothing_landed`. Those copies are held to this one by
    /// `both_stores_answer_the_same_script_the_same_way`, not by construction.
    ///
    /// # Errors
    ///
    /// Whatever [`Store::unit_closing`] returns: [`Error::Unavailable`] when the
    /// store cannot be reached, and [`Error::Refused`] when it answers and
    /// refuses.
    fn unit_closed(
        &self,
        realm: &OpaqueId,
        unit: nc_ids::Snowflake,
    ) -> impl Future<Output = Result<bool, Error>> + Send {
        let closing = self.unit_closing(realm, unit);
        async move { Ok(closing.await?.is_some()) }
    }

    /// Whether this realm's history already holds an opening bracket for `unit`.
    ///
    /// **A unit could be opened twice, and the second one relabelled it.**
    /// `Record.work_unit` is `pub` and `Journal::record` accepted a hand-set one
    /// with `Outcome::UnitBegun` while the unit was open — measured against a
    /// real database:
    ///
    /// ```text
    /// seq 0  kernel          kernel.begin  unit-begun  "a migration"
    /// seq 1  user-1          Write         allowed
    /// seq 2  hostile-agent   kernel.begin  unit-begun  "Delete everything"
    /// verify -> Ok(3)
    /// ```
    ///
    /// `unit.rs`'s `OPENS` exists so that "when did this unit start" finds one
    /// row. It found two, and the label a person reads is whichever the query
    /// picks. Asked of the store rather than of a flag, for the reason
    /// [`Store::unit_closing`] gives: the store is what a second process sees.
    ///
    /// **The columns are `realm`, `work_unit`, `action` and `result`**, the same
    /// four its closing twin asks, and this contract named none of them. Every
    /// one of the four can be switched off in either implementation with
    /// `cargo xtask check` green — measured, five ways — so a `Store` written
    /// against this paragraph could satisfy it and let a unit be opened twice.
    /// `a_bracket_is_a_bracket_and_a_unit_is_opened_once` and
    /// `a_unit_opened_in_another_realm_is_not_open_here` are what decide them.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unavailable`] when the store cannot be reached.
    ///
    /// **And [`Error::Refused`], which every one of these reaches through
    /// `unavailable`.** That function maps SQLSTATE classes 42, 28, 3D and 3F
    /// — a missing `GRANT`, a dropped table, a schema that is not there — to
    /// `Refused` rather than to `Unavailable`, and only `Store::head` named
    /// it. `Unavailable` is retryable and `Refused` is not, so a caller
    /// written against the shorter sentence retries a missing privilege for
    /// ever. Reached through a call rather than built here, which is why
    /// `every_errors_section_names_what_its_body_builds` cannot ask for it
    /// and a reviewer had to.
    fn unit_opened(
        &self,
        realm: &nc_ids::OpaqueId,
        unit: nc_ids::Snowflake,
    ) -> impl Future<Output = Result<bool, Error>> + Send;
}

/// What one read of the history returned.
///
/// **A read can be partial and still be useful**, which is the whole point: the
/// entries before an unreadable row verify normally, and the row that stopped
/// the read is a finding rather than a failure to answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Read {
    /// The entries read, in order.
    pub entries: Vec<Entry>,
    /// The row the read stopped at and why, when one stopped it.
    ///
    /// **The reason travels with the sequence.** A verdict that says only
    /// "entry 3 is unreadable" sends a reader to the row with no idea what to
    /// look for; the width of a hash, an outcome nobody declared and a resource
    /// nested past the parser are three different things to do next.
    pub stopped_at: Option<(u64, String)>,
}

/// Whether an entry is one this crate could read back, and why not if it is not.
///
/// **The one rule every store enforces.** `nc_canon` writes a value of any depth
/// and `nc_canon::parse` refuses one past `MAX_DEPTH`, which is harmless
/// everywhere except in a table that is append-only by design: one record past
/// the bound was accepted, hashed and stored, and from that moment the journal
/// could not be read, could not be verified at any starting sequence, could not
/// be appended to — `tail` cannot decode the tail — and could not be repaired,
/// because the trigger refuses `DELETE`.
///
/// # Errors
///
/// Returns [`Error::TooDeep`] naming both numbers, and [`Error::Unstorable`]
/// naming the field: `hash` when the digest is not the digest of the contents,
/// `sequence` when the number is past what a store can hold, and whatever
/// [`readable_record`] names.
///
/// **It said `TooDeep` alone**, and the two `Unstorable` refusals above it are
/// the ones a caller actually meets — `hash` is what a forged entry trips, and
/// this function exists because `Entry::chain` computes a link that a caller
/// supplying one does not.
pub fn readable(entry: &Entry) -> Result<(), Error> {
    // **And the hash has to be the hash of the contents.** `Entry`'s fields are
    // all public and this checked depth, NUL bytes and integer range and never
    // the one thing that makes an entry an entry — so one call to the public
    // `Store::append` wrote a row `verify` will condemn for ever into an
    // append-only table, with no raw SQL and no test-support feature. Both
    // stores accepted it, and it is the door two of the same round's other
    // findings were reached through.
    // **`Unstorable`, not `Undecodable`, because nothing has been stored.**
    // `Undecodable`'s own doc says *"a row is in the table and this code cannot
    // read it — that is tampering, somebody with write access put a row there"*,
    // and it carries `NC-JRNL-0001`, **critical**, which pages somebody. This
    // check runs before any store is asked, on a value a caller built wrong:
    //
    // ```text
    // readable(Entry { sequence: 9999, hash: [0; 32], .. })
    //   -> JournalChainBroken   critical   "the audit chain is broken"
    // ```
    //
    // The two checks below it in this same function are also pre-store refusals
    // and answer `Unstorable` — `error`, whose registry sentence says in as many
    // words *"so it was refused before the store saw it"*. One function, one
    // moment, and two stories about what happened.
    if entry.hash != entry.compute_hash() {
        return Err(Error::Unstorable {
            field: "hash",
            why: "this entry's hash is not the hash of its contents, so the history it \
                  would join could never verify. `Entry::chain` is what computes one"
                .to_owned(),
        });
    }
    // **The sequence too, which only the entry has.** `readable_record` guards
    // `recorded_at` and `policy_epoch` against PostgreSQL's `BIGINT` and could
    // not guard this one — a `Record` carries no sequence. So the two stores
    // answered differently for a sequence past `i64::MAX`:
    //
    // ```text
    // memory   -> OutOfOrder { expected: 0, found: 18446744073709551615 }
    // postgres -> Unstorable { field: "sequence", … }
    // ```
    //
    // Unreachable through `Journal`, which mints the sequence from the tail,
    // and reachable through the public `Store::append` the whole "both stores
    // answer alike" argument rests on.
    if i64::try_from(entry.sequence).is_err() {
        return Err(Error::Unstorable {
            field: "sequence",
            why: format!(
                "it is {}, which PostgreSQL's BIGINT cannot hold. Casting it would \
                 store a negative number that sorts before every real entry",
                entry.sequence
            ),
        });
    }
    readable_record(&entry.record)
}

/// The same rule, before an entry has been chained.
///
/// # Errors
///
/// Returns [`Error::TooDeep`] naming both numbers, and [`Error::Unstorable`]
/// naming the field: a NUL byte in `action`, `deciding_policy` or
/// `policy_pack`, and a `recorded_at` or `policy_epoch` past what a store's
/// signed column can hold.
pub fn readable_record(record: &Record) -> Result<(), Error> {
    // **Both numbers on one scale.** This compared `Value::depth()`, which
    // counts a scalar as 1, against the parser's bound, which starts the
    // outermost value at 0 — so it refused a resource `nc_canon::parse` accepts,
    // and said "the record's resource nests 65 deep and the journal reads 64",
    // two numbers that were not measurements of the same thing. `MAX_NESTING`
    // is the parser's bound expressed in `depth()`'s units, and `nc-canon` has
    // the test that keeps the two from drifting.
    let depth = record.resource.depth();
    if depth > nc_canon::parse::MAX_NESTING {
        return Err(Error::TooDeep {
            depth,
            limit: nc_canon::parse::MAX_NESTING,
        });
    }
    // **And text the store cannot hold.** This checked the resource's depth and
    // called itself "the one rule every store enforces", while `action`,
    // `deciding_policy` and `policy_pack` are plain `String`s with no validation
    // at all — every principal field is an `OpaqueId` and these three are not.
    // A NUL is hashed, passes this, and comes back from PostgreSQL as
    //
    // ```text
    // invalid byte sequence for encoding "UTF8": 0x00
    // ```
    //
    // which was reported as `Error::Unavailable` → `JournalUnavailable`: the
    // store answered perfectly well and the record was unstorable, and the
    // sentence sent a responder to PostgreSQL. That is exactly what
    // `Undecodable`'s own doc was written to avoid, one direction round.
    //
    // The resource is immune and stays out of this: canonical JSON escapes a
    // NUL to `\u0000`, so what reaches the column is printable either way.
    for (field, text) in [
        ("action", Some(&record.action)),
        ("deciding_policy", record.deciding_policy.as_ref()),
        ("policy_pack", record.policy_pack.as_ref()),
    ] {
        // **Counted in characters, because the sentence says characters.**
        // `str::find` answers in bytes, so `action = "é😀\0b"` reported the NUL
        // "at character 6" when it is the third character — a diagnostic
        // pointing at a place that is not there, which is the `§15.1` shape this
        // crate distinguishes elsewhere: `nc_canon::HexError` documents its
        // offset as a character offset and counts with `chars()`.
        if let Some(text) = text
            && let Some(at) = text.chars().position(|character| character == '\0')
        {
            return Err(Error::Unstorable {
                field,
                why: format!(
                    "it holds a NUL byte at character {at}, and PostgreSQL refuses one in \
                     text: `invalid byte sequence for encoding \"UTF8\": 0x00`"
                ),
            });
        }
    }
    // **And numbers no column can hold.** `recorded_at` is a `u64` and
    // `policy_epoch` an `Option<u64>`, both the caller's, and PostgreSQL's
    // BIGINT is signed — so anything above `i64::MAX` was accepted here, hashed,
    // and refused by the PostgreSQL store alone. Two consequences, both the
    // shape this function exists to prevent: the refusal came back as
    // `Unavailable` — the store answered nothing, having never been asked — and
    // the two `Store` implementations disagreed about what a journal accepts,
    // which is the defect `Postgres::append`'s own comment names.
    //
    // Checked here, so the rule is the journal's rather than one backend's, and
    // so a caller learns before a sequence has been minted.
    for (field, value) in [
        ("recorded_at", Some(record.recorded_at)),
        ("policy_epoch", record.policy_epoch),
    ] {
        if let Some(value) = value
            && i64::try_from(value).is_err()
        {
            return Err(Error::Unstorable {
                field,
                why: format!(
                    "it is {value}, which PostgreSQL's BIGINT cannot hold. Casting it \
                     would store a negative number that sorts before every real entry"
                ),
            });
        }
    }
    Ok(())
}

/// Why a journal operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The machine's clock reads before 1970, so there is no instant to record.
    ///
    /// **Refused rather than recorded as the epoch.** A journal entry's
    /// timestamp is inside its digest, so an entry claiming 1970 is an entry
    /// that verifies for ever. `nc-ids` answers `MintError::BeforeEpoch` to the
    /// same question about the same number.
    ClockBeforeEpoch,
    /// The store did not answer.
    Unavailable(String),
    /// The store answered and refused: a grant, a schema, a role.
    ///
    /// **Not the store being unavailable, and this is the difference that
    /// matters during an incident.** A revoked `INSERT`, a dropped table and a
    /// stopped container all answered `NC-JRNL-0003` — *"the journal store did
    /// not answer"*, `retryable` — so a responder filtering the log could not
    /// tell an outage from a missing `GRANT`, and the platform stops either way
    /// because `§14.9` makes the journal fail closed.
    Refused(String),
    /// An entry did not follow the tail. Two writers, or a store that moved.
    OutOfOrder {
        /// What the journal expected next.
        expected: u64,
        /// What it was handed.
        found: u64,
    },
    /// A work unit that was already committed or aborted received an entry.
    UnitClosed {
        /// Which unit.
        unit: Snowflake,
    },
    /// A work unit that is already open was opened again.
    ///
    /// The same rule as [`Error::UnitClosed`] at the other bracket: a group
    /// begins once, or "when did this unit start" has two answers and the label
    /// a person reads is whichever the query picks.
    UnitAlreadyOpen {
        /// Which unit.
        unit: Snowflake,
    },
    /// A work unit this realm's history holds no opening bracket for was asked
    /// to close.
    ///
    /// Raised by [`Journal::close_unit`] before anything is written. A closing
    /// bracket for an identifier nobody opened would close something that
    /// never began, and every later entry naming that identifier would then be
    /// refused as [`Error::UnitClosed`].
    UnitNeverOpened {
        /// Which unit.
        unit: Snowflake,
    },
    /// An identifier could not be minted for a work unit.
    NoIdentifier(nc_ids::MintError),
    /// A row is in the table and this code cannot read it.
    ///
    /// **Not `Unavailable`.** The store answered perfectly well; what it handed
    /// back is not something this code wrote. That is tampering — somebody with
    /// write access put a row there — and reporting it as the store being down
    /// sends a responder to PostgreSQL (`§15.1`).
    Undecodable {
        /// Which row.
        sequence: u64,
        /// What could not be read.
        why: String,
    },
    /// A record holds something no store could keep.
    ///
    /// **Not `Unavailable`.** The store answered; the record was unstorable.
    /// Reporting the second as the first sends a responder to PostgreSQL
    /// (`§15.1`) — the mirror of the sentence [`Self::Undecodable`] exists to
    /// avoid.
    Unstorable {
        /// Which field of the record.
        field: &'static str,
        /// What is wrong with it, and what the store says about it.
        why: String,
    },
    /// The write was refused and, by the time the reason was asked for, no
    /// reason was left.
    ///
    /// **A refusal whose cause has already been undone by somebody else.** The
    /// statement carries its three checks in one `WHERE`; the diagnosis after
    /// it asks each of them again, and another connection can settle the
    /// disagreement in between — the sequence this caller wanted becomes free
    /// again in no scenario, but a unit that was not yet open when the
    /// statement ran can be open by the time the question is asked. The arm
    /// that reported this printed `expected: N` and `found: N` — the same
    /// number twice, a sentence that says nothing happened — in 11 of 400
    /// contended rounds.
    ///
    /// Only [`crate::postgres::Postgres`] raises it: the in-memory store holds
    /// one lock across the check and the write, so there is no interval for
    /// anything to change in.
    Contended,
    /// A record nests deeper than the reader will follow.
    TooDeep {
        /// How deep the record's resource is.
        depth: usize,
        /// The deepest the reader accepts.
        limit: usize,
    },
}

impl Error {
    /// The catalog code this maps to, which is what a log is queried by.
    ///
    /// Not `const`: one arm compares the two sequences, and `PartialOrd` is not
    /// const yet. The comparison is what separates a race from an entry
    /// numbered past the end of the history.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self {
            // **Same code, two reasons, and the arms stay apart** so a later
            // change to either does not silently change the other. `TooDeep` is
            // "the store could not hold this"; `Unavailable` is "the store did
            // not answer". A responder reads the message, not the code.
            Self::Unavailable(_) => ErrorCode::JournalUnavailable,
            // Answered and refused: not retryable, and it names a different
            // system to go and look at.
            Self::Refused(_) => ErrorCode::JournalRefused,
            // **A record no store could keep is not the store being down.**
            // Both of these are refused before any store is asked, and both used
            // to answer `JournalUnavailable` — the code whose own description is
            // "the journal store did not answer", and which is `retryable`.
            // Measured: a record with a NUL in `action` was hashed, passed
            // `readable_record`, and came back from PostgreSQL as
            // `invalid byte sequence for encoding "UTF8": 0x00` reported as the
            // store being unavailable, which sends a responder to PostgreSQL
            // (§15.1) for a defect in the request. Retrying it produces the same
            // answer forever, which is what `retryable = false` says.
            // **And a clock before 1970 is a record that cannot be stored**, for
            // the same reason as the two above: it is refused before any store
            // is asked, and the entry it would have written would verify for
            // ever while claiming an instant that never happened.
            Self::Unstorable { .. } | Self::TooDeep { .. } | Self::ClockBeforeEpoch => {
                ErrorCode::JournalUnstorable
            }
            // A row this code did not write is the chain being broken. It is not
            // "unavailable" — a responder who cannot tell that from a store that
            // did not answer looks in the wrong place first.
            Self::Undecodable { .. } => ErrorCode::JournalChainBroken,
            // **And `OutOfOrder` is not.** It shared that code, and it is what a
            // second writer racing this one looks like: both read the tail, both
            // computed the same next sequence, one landed. Measured with twelve
            // threads doing the `head()`-then-`append()` this crate itself does,
            // nine were refused and every one was told the hash chain was broken
            // — critical, `retryable = false`, about a chain that was intact.
            // Three quarters of ordinary concurrent commands would lose their
            // audit entry on an instruction not to retry.
            // **Contention only in the direction another writer can cause.**
            // Writing moves the end of the history *forward*, so an entry
            // numbered past it is nobody else's doing — and that half carried
            // `JournalContended`, "Another writer took this sequence first",
            // `retryable = true`. Measured on an empty table with one writer
            // and nobody racing: `Store::append` at sequence 7 answered it, and
            // three retries answered the same. The other direction — the tail
            // moved on while this caller held a stale number — is exactly what
            // a race looks like, and a retry is exactly what fixes it.
            Self::OutOfOrder { expected, found } if found > expected => {
                ErrorCode::JournalSequenceAhead
            }
            // Both say *try again*: one because a sequence moved, one because
            // the reason for a refusal moved. A caller does the same thing with
            // either, and the sentences are what tell them apart.
            Self::OutOfOrder { .. } | Self::Contended => ErrorCode::JournalContended,
            Self::UnitClosed { .. } => ErrorCode::JournalUnitClosed,
            // **Its own code, because the other one says the opposite.** These
            // shared `JournalUnitClosed`, whose description is "a work unit that
            // was already committed or aborted received an entry" — so one error
            // said in one line that a unit was open and that it had finished,
            // and both locales told the reader to start the action again, which
            // is the operation that was just refused. Measured: twelve to
            // fifteen of every sixteen concurrent openers of one unit receive
            // this.
            Self::UnitAlreadyOpen { .. } => ErrorCode::JournalUnitAlreadyOpen,
            // **And its own code again, for the same reason.** Both neighbours
            // say something about a unit that exists — that it finished, that
            // it is under way — and this one does not.
            Self::UnitNeverOpened { .. } => ErrorCode::JournalUnitNeverOpened,
            // **The inner error's own code, not one of them.** This answered
            // `IdsClockBackwards` for every minting failure, so a *stuck* clock
            // — which never clears — was reported as `retryable = true` under a
            // code about a clock that moved back. `MintError` knows which it is.
            Self::NoIdentifier(error) => error.code(),
        }
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Unavailable(why) => write!(formatter, "the journal store did not answer: {why}"),
            Self::Refused(why) => write!(
                formatter,
                "the journal store answered and refused the write: {why}"
            ),
            Self::OutOfOrder { expected, found } => write!(
                formatter,
                "the journal expected sequence {expected} and was handed {found}"
            ),
            Self::Contended => write!(
                formatter,
                "the journal refused the entry and, asked why, found nothing wrong: \
                 another writer settled it in between. Read the tail again and retry"
            ),
            Self::ClockBeforeEpoch => write!(
                formatter,
                "this machine's clock reads before 1970, so there is no instant to \
                 record. A journal entry's timestamp is inside its digest and cannot be \
                 corrected afterwards. Set the clock, then run this again — this \
                 particular error will not clear on its own"
            ),
            Self::UnitClosed { unit } => write!(formatter, "work unit {unit} was already closed"),
            Self::UnitAlreadyOpen { unit } => {
                write!(formatter, "work unit {unit} was already open")
            }
            Self::UnitNeverOpened { unit } => write!(
                formatter,
                "work unit {unit} was never opened in this realm, so there is nothing to close"
            ),
            Self::TooDeep { depth, limit } => write!(
                formatter,
                "the record's resource nests {depth} deep and the journal reads {limit}. \
                 Storing it would write an entry nothing can read back, and this table \
                 is append-only"
            ),
            // **Not "the record's"**, because one of these arrives on a read:
            // `from(sequence)` converts the sequence the same way, and there is
            // no record in hand when a caller asks for one. Naming the field is
            // what a reader needs either way.
            // **Not "before any store was asked".** That is true of every site
            // that builds this from a value it checked itself, and false of the
            // one that builds it from the store's own answer — a data exception
            // the statement provoked. Written unconditionally, that arm read
            // *"The store answered; it is the request it could not take, so this
            // is not the store being unavailable. It was refused before any
            // store was asked…"*: the same disclaimer twice, contradicting
            // itself in one line. What is true of both is that the record was
            // not storable and the store is not what is broken, and the `why`
            // each site carries says which of the two happened.
            Self::Unstorable { field, why } => write!(
                formatter,
                "`{field}` is not something a store could keep: {why}. The store is not \
                 what is broken here — the record is"
            ),
            // **"at entry N", not "entry N is in the table".** This variant is
            // raised on a *write* too — a caller whose entry does not link to
            // the tail — and there the row is nowhere: measured,
            // `entry 4 is in the table…` with the table holding three rows and
            // `verify` answering `Ok(4)`. The sequence is where the trouble is,
            // which is true on both paths; whether a row is there is not.
            Self::Undecodable { sequence, why } => write!(
                formatter,
                "the journal cannot read what belongs at entry {sequence}: {why}. \
                 The store answered; what this is looking at is not what this wrote"
            ),
            Self::NoIdentifier(why) => {
                write!(formatter, "a work unit could not be identified: {why}")
            }
        }
    }
}

impl core::error::Error for Error {}

/// Where verification stopped, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Broken {
    /// The first entry that did not verify.
    pub sequence: u64,
    /// What was wrong with it.
    pub reason: Reason,
    /// What the store said about it, when it said anything.
    ///
    /// **`verify` was handed a reason and threw it away.** `Read::stopped_at`
    /// carries why the row would not decode — that is what its own doc is for,
    /// *"a verdict that says only 'entry 3 is unreadable' sends a reader to the
    /// row with no idea what to look for"* — and the verdict built from it kept
    /// only the number. Worse, when the unreadable thing is the `sequence`
    /// column itself, that number is `u64::MAX`, which the table's own
    /// `CHECK (sequence >= 0)` and primary key make impossible:
    ///
    /// ```text
    /// entry 18446744073709551615 does not verify: it is in the table and this
    ///   code cannot read it, so it was not written here
    /// ```
    ///
    /// `postgres.rs` had already fixed that sentence where it produces it; the
    /// only caller discarded the fix.
    pub detail: Option<String>,
}

impl Broken {
    /// The catalog code this maps to.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self.reason {
            // An entry whose own hash is wrong was edited in place. An entry
            // whose link is wrong means something before it moved. A reader
            // chasing the first should look at one row; the second, at the whole
            // history above it — so they are different codes.
            // A row this code did not write, edited in place or inserted from
            // another connection, is the same kind of finding as one whose hash
            // is wrong: look at that row.
            Reason::HashDoesNotMatchContents | Reason::EntryIsNotReadable => {
                ErrorCode::JournalEntryTampered
            }
            Reason::LinkDoesNotMatchPredecessor
            | Reason::SequenceIsNotContiguous
            | Reason::HistoryDoesNotStartHere
            | Reason::HistoryDoesNotReachTheTail => ErrorCode::JournalChainBroken,
        }
    }
}

/// What was wrong with an entry.
///
/// **Each of these has a sentence, and it used to live inside `Broken`'s
/// `Display` alone** — so a caller with a `Reason` in hand could not render it:
/// `error[E0277]: Reason doesn't implement Display`. The sibling enum `Outcome`
/// carries both `as_str` and `parse` for the same kind of reason, and `Broken`
/// now writes what `Display` here says rather than a second copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// The entry's stored hash is not the hash of the entry. Edited in place.
    HashDoesNotMatchContents,
    /// The entry's link is not its predecessor's hash. Something before it
    /// changed, or an entry was removed.
    LinkDoesNotMatchPredecessor,
    /// A sequence number was skipped, which a dense chain cannot do.
    SequenceIsNotContiguous,
    /// The read did not begin where it was asked to. The rows before it are
    /// gone, and their absence is invisible to a chain of links.
    HistoryDoesNotStartHere,
    /// The read stopped short of the entry the store itself reports as the last
    /// one. Rows in the middle are missing, and the store knows it.
    HistoryDoesNotReachTheTail,
    /// A row is in the table that this code did not write and cannot read.
    ///
    /// Nothing this crate writes is unreadable — every store refuses such an
    /// entry — so a row that will not decode arrived by another route, which is
    /// the definition of tampering.
    EntryIsNotReadable,
}

impl core::fmt::Display for Reason {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::HashDoesNotMatchContents => "its hash is not the hash of its contents",
            Self::LinkDoesNotMatchPredecessor => "its link does not match the entry before it",
            Self::SequenceIsNotContiguous => "the sequence before it is missing",
            Self::HistoryDoesNotStartHere => "the history does not begin where it should",
            Self::HistoryDoesNotReachTheTail => "the history stops short of its own last entry",
            Self::EntryIsNotReadable => {
                "it is in the table and this code cannot read it, so it was not written here"
            }
        })
    }
}

impl core::fmt::Display for Broken {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // One copy of each sentence, on the enum that names it.
        let what = self.reason;
        match &self.detail {
            Some(why) => write!(
                formatter,
                "entry {} does not verify: {what} — {why}",
                self.sequence
            ),
            None => write!(formatter, "entry {} does not verify: {what}", self.sequence),
        }
    }
}

impl core::error::Error for Broken {}

#[cfg(test)]
mod verdict_tests {
    use super::{Broken, Error, Reason};

    /// Every `.rs` file of this crate's source, read from the directory.
    ///
    /// **It was a hand list of five names, written out twice**, and the two
    /// counts that are supposed to disagree when one of them narrows both read
    /// it — so a sixth module was outside both and they agreed about nothing.
    /// The directory is the ground; a floor keeps a walk that finds nothing from
    /// reading as a crate with nothing in it.
    fn every_source_file() -> Vec<String> {
        let directory = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src"));
        let mut found: Vec<String> = std::fs::read_dir(directory)
            .expect("this crate has a source directory")
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|kind| kind == "rs"))
            .map(|path| {
                path.file_name()
                    .expect("a file name")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        found.sort();
        assert!(
            found.len() >= 5,
            "{} source file(s) in this crate, which is not the tree this is about",
            found.len()
        );
        found
    }

    /// Every variant of *this crate's* `Error` a body builds.
    ///
    /// **`Error::` is not a rare spelling.** The scan matched the bare
    /// substring, so `sqlx::Error::Database` — a `match` arm over the driver's
    /// error, in the body of `Postgres::append` — was charged to `Store::append`
    /// as a refusal this crate builds. A qualifier in front of `Error` is
    /// another crate's error unless it is this one's, so the word before the
    /// `::` is read and only `crate`, `self` and `super` are ours.
    ///
    /// Written once, because the per-body rule and the per-implementation rule
    /// are one question asked at two scopes.
    fn the_variants_built_in(body: &str) -> Vec<String> {
        let mut built: Vec<String> = Vec::new();
        for (at, _) in body.match_indices("Error::") {
            if let Some(path) = body[..at].strip_suffix("::") {
                let qualifier: String = path
                    .chars()
                    .rev()
                    .take_while(|character| character.is_alphanumeric() || *character == '_')
                    .collect::<Vec<char>>()
                    .into_iter()
                    .rev()
                    .collect();
                if !matches!(qualifier.as_str(), "crate" | "self" | "super") {
                    continue;
                }
            }
            let variant: String = body[at + "Error::".len()..]
                .chars()
                .take_while(|character| character.is_alphanumeric())
                .collect();
            if !variant.is_empty() && !built.contains(&variant) {
                built.push(variant);
            }
        }
        built
    }

    /// Every `Error::` variant a trait *implementation* builds, by method name.
    ///
    /// **A trait method has no body, and its section was therefore counted and
    /// never judged.** Six of this crate's nineteen `# Errors` sections are
    /// `Store`'s, and the walk below did `checked += 1` and then `continue`d on
    /// every one of them — so `checked == sections` reported nineteen inspected
    /// while six were inspected for nothing. Measured on 2026-09-08,
    /// the link to [`Error::Unstorable`] deleted from `Store::append`'s section
    /// — the
    /// exact variant whose own follow-up paragraph says it *"was missing and it
    /// is the commonest of them"*: `cargo test --workspace --locked` exit 0 and
    /// `cargo run -q -p xtask -- check` at `check passed`, `ok 1223 test(s)`.
    ///
    /// The refusals of a trait method are the refusals of the implementations
    /// this crate ships, and both of them are in the files this walk already
    /// reads. Only `impl … for …` blocks count: an inherent method of the same
    /// name is a different function, and charging a trait's section with one
    /// would be a rule that refuses something legitimate.
    fn what_an_implementation_builds() -> std::collections::BTreeMap<String, Vec<String>> {
        let mut built: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for named in every_source_file() {
            let source = std::fs::read_to_string(
                std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src")).join(&named),
            )
            .unwrap_or_else(|error| panic!("{named} is readable: {error}"));
            let lines: Vec<&str> = source.lines().collect();
            let mut implementing = false;
            for (index, line) in lines.iter().enumerate() {
                let declaration = line.trim_start();
                if !line.starts_with(' ') && !declaration.is_empty() {
                    // A block at column zero ends whatever came before it.
                    implementing = opens_a_trait_implementation(declaration);
                }
                let Some(name) = super::the_method_declared_by(declaration) else {
                    continue;
                };
                if !implementing || declaration.trim_end().ends_with(';') {
                    continue;
                }
                let indent = line.len() - declaration.len();
                let closes = format!("{}}}", " ".repeat(indent));
                let ends = lines[index..]
                    .iter()
                    .position(|line| *line == closes)
                    .map_or(lines.len(), |by| index + by);
                // **The state machine beside this, not `split("//")`.** That
                // knew one spelling of a comment: an ordinary `/* … */`
                // recording history made this walk report a method as building
                // a refusal it cannot build. Measured on 2026-09-12, one screen
                // from the reader written for exactly this in the same crate.
                let body = super::the_code_without_comments(&lines[index..ends].join("\n"));
                // **The body, and not one step down the calls it makes.** One
                // step was tried and it is *too strong*: keyed by name, every
                // body that so much as mentions `append`, `readable` or `tail`
                // is charged with everything any function of that name builds,
                // and the walk demanded `Contended`, `OutOfOrder`, `Unstorable`
                // and `UnitClosed` from `Store::tail`, `head`, `from`,
                // `unit_closed` and `unit_opened` — none of which any of them
                // can return. A rule that refuses something legitimate is the
                // failure this repository calls the worse one. `Unstorable` and
                // `Refused` are reached through a call and are named by hand in
                // the sections above, where a reviewer put them.
                let charged = built.entry(name).or_default();
                for variant in the_variants_built_in(&body) {
                    if !charged.contains(&variant) {
                        charged.push(variant);
                    }
                }
            }
        }
        built
    }

    /// The readers the two section walks share, driven over text.
    ///
    /// **Seven clauses in these functions decided nothing on this tree.** A
    /// reviewer deleted each in turn and the whole suite stayed green: the four
    /// qualifiers `unsafe`, `const`, `pub(crate)` and `pub(super)`; the `super`
    /// arm of the `Error::` qualifier; the ` for ` guard on an `impl` block;
    /// and the attribute skip in the block reader. A clause no case reaches is
    /// a clause nobody can tell is wrong, and two of these are guards against
    /// the *other* kind of failure — the rule refusing something legitimate.
    ///
    /// Written over text rather than over the tree, because a walk that needs
    /// this crate's own source can only be driven by what this crate happens to
    /// contain today.
    #[test]
    fn the_readers_the_section_walks_share_decide_every_clause() {
        for (declaration, named) in [
            ("fn plain() {}", Some("plain")),
            ("pub fn open() {}", Some("open")),
            // Not `begin`, `open_unit`, `close_unit`, `record`, `commit` or
            // `abort`: `section_of` locates an item by `pub async fn {name}(`,
            // and a corpus row spelling one of those out stands earlier in this
            // file than the item does.
            ("pub async fn embark() {}", Some("embark")),
            ("pub(crate) fn bracket() {}", Some("bracket")),
            ("pub(super) fn near() {}", Some("near")),
            ("const fn id() {}", Some("id")),
            ("unsafe fn raw() {}", Some("raw")),
            ("pub const unsafe fn both() {}", Some("both")),
            ("pub struct Journal {", None),
            ("let counted = 1;", None),
        ] {
            assert_eq!(
                super::the_method_declared_by(declaration).as_deref(),
                named,
                "`{declaration}`"
            );
        }
        for (body, built) in [
            ("Err(Error::Contended)", vec!["Contended"]),
            ("Err(crate::Error::Unstorable)", vec!["Unstorable"]),
            ("Err(self::Error::Refused)", vec!["Refused"]),
            ("Err(super::Error::UnitClosed)", vec!["UnitClosed"]),
            ("Err(sqlx::Error::Database)", vec![]),
            ("Err(nc_ids::Error::BeyondRange)", vec![]),
        ] {
            assert_eq!(the_variants_built_in(body), built, "`{body}`");
        }
        for (declaration, implements) in [
            ("impl Store for Postgres {", true),
            ("impl<S: Store> Read for Journal<S> {", true),
            ("impl Journal {", false),
            ("impl<S: Store> Journal<S> {", false),
            ("struct Journal {", false),
        ] {
            assert_eq!(
                opens_a_trait_implementation(declaration),
                implements,
                "`{declaration}`"
            );
        }
    }

    /// Whether a line at column zero opens a trait implementation.
    ///
    /// **`impl Journal` is not an implementation of anything.** The ` for `
    /// clause is what keeps an inherent method of the same name as a trait's
    /// from being charged with that trait's refusals — the walk above says so
    /// at length and calls the alternative "a rule that refuses something
    /// legitimate" — and nothing on this tree reached it: seven inherent `impl`
    /// blocks stand in this crate and the clause could be deleted with the
    /// whole suite green. The corpus below is what decides it now — and writing
    /// the corpus found the other half: `starts_with("impl ")` is one spelling,
    /// so `impl<S: Store> Read for Journal<S>` opened no implementation at all
    /// and every method in it would have been skipped in silence. This crate
    /// has no generic trait implementation today, which is exactly why nothing
    /// noticed.
    fn opens_a_trait_implementation(declaration: &str) -> bool {
        declaration
            .strip_prefix("impl")
            .is_some_and(|rest| rest.starts_with(' ') || rest.starts_with('<'))
            && declaration.contains(" for ")
    }

    /// Every declaration with an `# Errors` section, and the refusals its own
    /// body builds that the section does not name.
    ///
    /// Its own function because the test crossed a hundred lines with it inline.
    /// Returns the misses and how many sections were read, so the caller can
    /// hold the second number against a count taken another way.
    fn the_sections_and_what_they_miss() -> (Vec<String>, usize) {
        let mut silent = Vec::new();
        let mut checked = 0_usize;
        let implementations = what_an_implementation_builds();
        // **The directory, not a hand list of five names.** Both counts read the
        // same five spellings, written out twice and tied to the directory by
        // nothing — so a sixth module, which is the most ordinary change there
        // is, was outside both and they agreed about nothing. Measured with one
        // added: a public function whose section names `Unavailable` and whose
        // body builds `Contended` — every printed count *up*, `953 test(s)`
        // unchanged, `check passed`.
        for named in every_source_file() {
            let source = std::fs::read_to_string(
                std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src")).join(&named),
            )
            .unwrap_or_else(|error| panic!("{named} is readable: {error}"));
            let lines: Vec<&str> = source.lines().collect();
            for (index, line) in lines.iter().enumerate() {
                let declaration = line.trim_start();
                // **`the_method_declared_by`, not a second copy of it.** The
                // qualifier loop was written out here as well, and the two
                // copies had drifted: this one read `pub(crate)` through its
                // general `pub(` arm and the other through two literals, so
                // four of the qualifiers between them were reached by nothing
                // and could each be deleted with the suite green. One reader,
                // and the corpus below drives every arm of it.
                //
                // **The section is the signal, not `Result<` on the same
                // line.** A wrapped declaration puts the return type two lines
                // down and `impl Future<Output = Result<…>>` puts it inside
                // another type; requiring it here read thirteen of nineteen. A
                // function with an `# Errors` section above it is a function
                // making a claim about its refusals, whatever its signature
                // looks like.
                if super::the_method_declared_by(declaration).is_none() {
                    continue;
                }
                let indent = line.len() - declaration.len();
                // The doc block above it, trimmed — a method's is indented —
                // past whatever attributes stand between the two. Three of this
                // crate's sections sit above a `#[doc(hidden)]`, and a walk that
                // stopped at the first non-`///` line read sixteen of nineteen.
                let mut at = super::past_the_attributes(&lines, index);
                while at > 0 && lines[at - 1].trim_start().starts_with("///") {
                    at -= 1;
                }
                let doc = lines[at..index]
                    .iter()
                    .filter(|line| line.trim_start().starts_with("///"))
                    .copied()
                    .collect::<Vec<_>>()
                    .join("\n");
                // **A heading, not the words anywhere in the prose.** The other
                // count reads lines that are exactly `/// # Errors`; a substring
                // match also caught this test's own doc, which names the heading
                // in a sentence — so the two readings disagreed by one and the
                // extra was the reader.
                let Some(heading) = doc.lines().position(|line| line.trim() == "/// # Errors")
                else {
                    continue;
                };
                let errors = doc.lines().skip(heading + 1).collect::<Vec<_>>().join("\n");
                // **A trait method has no body, and taking one runs into the
                // next method's.** The signature ends at `;` before any `{`, and
                // reading past it charged `Store::head` with every refusal the
                // functions below it build. `Store`'s sections are hand-written
                // — a trait method's refusals come from its implementations, and
                // following them is a call graph rather than a scan.
                let signature_ends = lines[index..]
                    .iter()
                    .position(|line| line.contains('{') || line.trim_end().ends_with(';'))
                    .map_or(0, |by| index + by);
                if lines[signature_ends].trim_end().ends_with(';')
                    && !lines[signature_ends].contains('{')
                {
                    // **A trait method is judged by its implementations**, which
                    // this crate ships two of and this walk already reads. It
                    // used to be counted here and judged nowhere; see
                    // [`what_an_implementation_builds`] for what that cost.
                    checked += 1;
                    let Some(name) = super::the_method_declared_by(declaration) else {
                        continue;
                    };
                    for variant in implementations.get(&name).into_iter().flatten() {
                        if errors.contains(variant.as_str()) {
                            continue;
                        }
                        let found = format!(
                            "  {named}: {} is implemented to build `Error::{variant}`",
                            declaration.trim()
                        );
                        if !silent.contains(&found) {
                            silent.push(found);
                        }
                    }
                    continue;
                }
                checked += 1;
                // The body, to a `}` at the declaration's own indent.
                let closes = format!("{}}}", " ".repeat(indent));
                let ends = lines[index..]
                    .iter()
                    .position(|line| *line == closes)
                    .map_or(lines.len(), |by| index + by);
                // Code, not the prose explaining it: a comment naming a variant
                // this function *used to* return is history, not a refusal it
                // builds — `readable_record` carries one.
                // **The state machine beside this, not `split("//")`.** That
                // knew one spelling: an ordinary `/* … */` recording history
                // made this walk report a section as claiming a refusal its
                // body cannot build.
                let body = super::the_code_without_comments(&lines[index..ends].join("\n"));
                for name in the_variants_built_in(&body) {
                    if errors.contains(&name) {
                        continue;
                    }
                    let found = format!("  {named}: {} builds `Error::{name}`", declaration.trim());
                    if !silent.contains(&found) {
                        silent.push(found);
                    }
                }
            }
        }
        silent.sort();
        (silent, checked)
    }

    /// Every `# Errors` section names the refusals its own body builds.
    ///
    /// **Five public entry points named variants the code does not return**, and
    /// the ones they left out are the ones that invert `retryable`:
    /// `Store::append` said `Unavailable`, `OutOfOrder`, `TooDeep` while every
    /// implementation calls `readable` first, so `Unstorable` — not retryable —
    /// is the commonest refusal a caller meets; `Store::from` said `Unavailable`
    /// while `Postgres::from`'s first statement is a conversion that returns
    /// `Unstorable`; `readable` and `readable_record` said `TooDeep` alone.
    /// A caller written against those retries a caller-side defect for ever,
    /// which is the failure `NC-JRNL-0005` was added to end.
    ///
    /// This holds the half a source rule can hold: a variant a function builds
    /// *in its own body* is named in its own section. A variant reached through
    /// a call is the trait methods' case and is written by hand, because
    /// following it is a call graph rather than a scan.
    #[test]
    fn every_errors_section_names_what_its_body_builds() {
        // **Every file of this crate, and every indentation.** This read
        // `lib.rs` alone and matched `pub fn ` at column zero, so it saw two of
        // the crate's `# Errors` sections — the count is `grep -rc '/// # Errors'
        // crates/nc-journal/src`, and its floor was `>= 2`,
        // which is that number by construction. `Journal::record`, the primary
        // entry point, was outside it: its section named `Unavailable` alone
        // while ordinary caller input reaches `Unstorable` and `TooDeep`, and a
        // planted `return Err(Error::Contended)` in its body left this green.
        //
        // A method sits at one level of indentation inside an `impl`, and a
        // trait method at one inside a `trait`, so the declaration is matched
        // trimmed and the body ends at a `}` back at the declaration's own
        // indent.
        let (silent, checked) = the_sections_and_what_they_miss();
        // **The population counted a second way**, because a scan that narrows
        // still meets a floor it wrote for itself: this read two of them all when
        // it matched `pub fn ` at column zero, and its floor was `>= 2`. The
        // other count is the `# Errors` lines themselves — the same ground,
        // read without the declaration walk — so the two disagree exactly when
        // one of them stops seeing something.
        // **Its own walk, because a shared reader is a shared blind spot.**
        // `checked` and this both read `every_source_file()`, and so did the
        // floor below — the exact shape this function's own doc says the hand
        // list was replaced to remove, one level up. Measured: one
        // `found.retain(|name| name != "unit.rs")` at the end of that walk left
        // `checked == sections == 16` of 19, `check passed`, and a planted
        // `return Err(Error::Refused(…))` in `WorkUnit::record` — a public
        // entry point whose `# Errors` section names `Error::UnitClosed` alone —
        // went unreported.
        //
        // Same ground, different code: `read_dir` here, so a narrowing in one
        // cannot narrow the other with it.
        let sections: usize = std::fs::read_dir(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src"
        )))
        .expect("this crate has a source directory")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|kind| kind == "rs"))
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .map(|text| {
            text.lines()
                .filter(|line| line.trim() == "/// # Errors")
                .count()
        })
        .sum();
        assert_eq!(
            checked, sections,
            "{checked} declaration(s) carry an `# Errors` section and {sections} such \
             sections are written. A scan that narrows is a scan whose own floor it \
             still meets, which is how this one read two of them"
        );
        assert!(
            // The written number, not a floor with four of slack: the lines
            // that are exactly `/// # Errors` once trimmed, which is what
            // `grep -rhx '\s*/// # Errors' crates/nc-journal/src | wc -l`
            // counts — a scan that reads fewer has narrowed.
            sections == 23,
            "{sections} `# Errors` section(s) in this crate, which is not the tree this \
             is about"
        );
        assert!(
            silent.is_empty(),
            "{} function(s) build a refusal their own `# Errors` section does not name:\n{}\n\
             The variants that go missing are the ones that invert `retryable`, and a \
             caller written against the section retries a defect it caused",
            silent.len(),
            silent.join("\n")
        );
    }

    /// Every `Error` reads as what it is.
    ///
    /// **Two of the thirteen arms were rendered by no test in the workspace.**
    /// Found by branch instrumentation — markers in 222 branch bodies across
    /// `nc-canon`, `nc-journal` and `nc-catalog-compiler`, one run, 216 hot.
    /// Every test that produces `UnitAlreadyOpen` or `NoIdentifier` asserts with
    /// `matches!(…)` and never calls `to_string()`, so the sentences could say
    /// anything: reworded to *"work unit {unit} was already **closed**"* — the
    /// self-contradiction `Error::code`'s own comment records having removed
    /// once — and to `Unavailable`'s sentence, the whole workspace stayed green.
    ///
    /// Written out rather than derived, for the reason the sibling below gives:
    /// an assertion that read the sentence from the enum it is testing would
    /// agree with any sentence at all.
    #[test]
    fn every_error_reads_as_what_it_is() {
        let sequence = 4;
        let unit = nc_ids::Snowflake::from_crockford("000000000000C").expect("a work unit");
        let named = unit.to_string();
        for (error, sentence) in [
            (
                Error::Unavailable("the pool timed out".to_owned()),
                "the journal store did not answer: the pool timed out".to_owned(),
            ),
            (
                Error::Refused("permission denied".to_owned()),
                "the journal store answered and refused the write: permission denied".to_owned(),
            ),
            (
                Error::OutOfOrder {
                    expected: 3,
                    found: 9,
                },
                "the journal expected sequence 3 and was handed 9".to_owned(),
            ),
            (
                Error::Contended,
                "another writer settled it in between. Read the tail again and retry".to_owned(),
            ),
            (
                Error::ClockBeforeEpoch,
                "this machine's clock reads before 1970".to_owned(),
            ),
            (
                Error::UnitClosed { unit },
                format!("work unit {named} was already closed"),
            ),
            (
                Error::UnitAlreadyOpen { unit },
                format!("work unit {named} was already open"),
            ),
            (
                Error::UnitNeverOpened { unit },
                format!("work unit {named} was never opened in this realm"),
            ),
            (
                Error::TooDeep { depth: 9, limit: 8 },
                "the record's resource nests 9 deep and the journal reads 8".to_owned(),
            ),
            (
                Error::Unstorable {
                    field: "hash",
                    why: "it is not a digest".to_owned(),
                },
                "`hash` is not something a store could keep: it is not a digest".to_owned(),
            ),
            (
                Error::Undecodable {
                    sequence,
                    why: "the column is empty".to_owned(),
                },
                "the journal cannot read what belongs at entry 4: the column is empty".to_owned(),
            ),
        ] {
            let said = error.to_string();
            assert!(
                said.contains(&sentence),
                "`{error:?}` reads `{said}`, which does not say `{sentence}`"
            );
        }

        // `NoIdentifier` carries a minting failure, and the sentence has to say
        // that a *work unit* could not be identified — the caller is not minting
        // anything else here.
        let refused = Error::NoIdentifier(nc_ids::MintError::ClockStuck {
            at: core::time::Duration::from_millis(1),
        });
        let said = refused.to_string();
        assert!(
            said.starts_with("a work unit could not be identified: "),
            "`NoIdentifier` reads `{said}`"
        );
        assert!(
            said.len() > "a work unit could not be identified: ".len(),
            "`NoIdentifier` says nothing about why: `{said}`"
        );

        // And no two variants read the same, which is what makes any of the
        // above worth asserting: a copy-pasted arm satisfies its own `contains`.
        let all = [
            Error::Unavailable("x".to_owned()).to_string(),
            Error::Refused("x".to_owned()).to_string(),
            Error::UnitClosed { unit }.to_string(),
            Error::UnitAlreadyOpen { unit }.to_string(),
            Error::UnitNeverOpened { unit }.to_string(),
            Error::Contended.to_string(),
            Error::ClockBeforeEpoch.to_string(),
        ];
        let mut seen = std::collections::BTreeSet::new();
        for said in &all {
            assert!(seen.insert(said.clone()), "two variants both read `{said}`");
        }
    }

    /// Each reason reads as itself, and no two read alike.
    ///
    /// **The sentence a responder reads was asserted by nobody.** Three tests
    /// check `Broken::detail` — the clause after the dash, which is text the
    /// *row* supplied — and one older test checks `code()`. The clause that
    /// says what kind of damage this is went unread: replacing five of the six
    /// with "the printer tray is empty" left the whole `nc-journal` suite
    /// green, and swapping `EntryIsNotReadable` for `HashDoesNotMatchContents`
    /// at all three construction sites produced the self-contradictory verdict
    ///
    /// ```text
    /// entry 1 does not verify: its hash is not the hash of its contents —
    ///   `invented` is not an outcome this code knows
    /// ```
    ///
    /// with every one of the three `detail` assertions passing, because both
    /// reasons map to the same error code.
    ///
    /// Written out rather than derived: an assertion that read the sentence
    /// from the enum it is testing would agree with any sentence at all.
    #[test]
    fn every_reason_reads_as_what_it_is() {
        let said = |reason| {
            Broken {
                sequence: 7,
                reason,
                detail: None,
            }
            .to_string()
        };
        for (reason, sentence) in [
            (
                Reason::HashDoesNotMatchContents,
                "its hash is not the hash of its contents",
            ),
            (
                Reason::LinkDoesNotMatchPredecessor,
                "its link does not match the entry before it",
            ),
            (
                Reason::SequenceIsNotContiguous,
                "the sequence before it is missing",
            ),
            (
                Reason::HistoryDoesNotStartHere,
                "the history does not begin where it should",
            ),
            (
                Reason::HistoryDoesNotReachTheTail,
                "the history stops short of its own last entry",
            ),
            (
                Reason::EntryIsNotReadable,
                "it is in the table and this code cannot read it, so it was not written here",
            ),
        ] {
            assert_eq!(
                said(reason),
                format!("entry 7 does not verify: {sentence}"),
                "{reason:?} no longer reads as what it is"
            );
        }

        // And the detail is appended rather than replacing it, so a responder
        // gets both the kind of damage and the row's own reason.
        let with_detail = Broken {
            sequence: 7,
            reason: Reason::EntryIsNotReadable,
            detail: Some("`invented` is not an outcome this code knows".to_owned()),
        }
        .to_string();
        assert!(with_detail.starts_with(
            "entry 7 does not verify: it is in the table and this code cannot read it"
        ));
        assert!(with_detail.ends_with("— `invented` is not an outcome this code knows"));
    }
}

/// The journal itself: a store, and the chaining nothing else may do.
#[derive(Debug)]
pub struct Journal<S: Store, C: Clock = SystemClock> {
    store: S,
    realm: OpaqueId,
    /// Who the journal itself acts as when it writes a bracket entry for a work
    /// unit. Not a user: the kernel opened and closed the group.
    keeper: OpaqueId,
    /// What mints the identifier of every unit this journal brackets: its
    /// own, or one every journal of this machine's flake shares
    /// ([`Journal::sharing`]).
    units: Arc<Mutex<Minter<C>>>,
    /// The label of every unit this journal opened and has not yet closed.
    ///
    /// **What lets a close repeat the label its opening carried.** A unit is
    /// closed by its identifier alone, and the label lives in the opening
    /// bracket's resource — reading it back from a store means decoding a
    /// column a foreign row can poison, which is the argument
    /// [`Store::unit_closing`] makes for asking four columns and no more. So the
    /// journal that wrote the opening keeps the text until the close lands, or
    /// until a close is refused because another writer's already has. A unit
    /// left open keeps its entry here, which is what lets a later
    /// [`Journal::close_unit`] still carry it.
    labels: BTreeMap<Snowflake, String>,
}

impl<S: Store, C: Clock> Journal<S, C> {
    /// Opens a journal over `store`, for one realm, minting work-unit
    /// identifiers with `units`.
    ///
    /// The realm is the journal's, not each record's caller's: every entity
    /// belongs to exactly one realm (`§10.3`), and a journal that let callers
    /// name a different one per entry would be a journal somebody could write
    /// another realm's history into.
    ///
    /// **The minter is the caller's**, and that is a correction. This used to
    /// build one internally from a hardcoded machine flake of zero — so two
    /// `Journal`s in one process minted the *same* work-unit identifiers, which
    /// is precisely the hazard `Minter::new` documents as the caller's to
    /// enforce, under a comment here claiming Snowflakes prevented it. One
    /// minter per flake per process is a rule only the caller can keep, so the
    /// type now says so — and a process that keeps the journals of several
    /// realms on its one flake (`NOCTUA.md §16.2`) gives them one minter
    /// between them, through [`Journal::sharing`].
    pub fn new(store: S, realm: OpaqueId, keeper: OpaqueId, units: Minter<C>) -> Self {
        Self::sharing(store, realm, keeper, Arc::new(Mutex::new(units)))
    }

    /// Opens a journal over `store`, for one realm, minting work-unit
    /// identifiers from `units`, a minter it shares with every other journal
    /// this process keeps on the same flake.
    ///
    /// **One sequence for every realm of the machine.** The kernels of the
    /// realms one person belongs to keep their units in one local file, by
    /// identifier: two journals minting from minters of their own on one flake
    /// handed out the same identifier in the same millisecond, and a reviewer
    /// found one realm's write refused as a unit already committed and one
    /// realm's recovery reverting another's kept unit. Sharing the minter is
    /// what makes every identifier this machine mints its own, whichever realm
    /// it brackets.
    pub fn sharing(
        store: S,
        realm: OpaqueId,
        keeper: OpaqueId,
        units: Arc<Mutex<Minter<C>>>,
    ) -> Self {
        Self {
            store,
            realm,
            keeper,
            units,
            labels: BTreeMap::new(),
        }
    }

    /// An identifier from this journal's minter, whoever last held it:
    /// minting panics nowhere.
    fn minted(&self) -> Result<Snowflake, nc_ids::MintError> {
        self.units
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .mint()
    }

    /// The store, for a caller that needs to read it directly.
    pub const fn store(&self) -> &S {
        &self.store
    }

    /// The store, mutably.
    ///
    /// **Behind a feature**, and off unless a test asks for it. It exists so the
    /// tamper-detection tests can do what somebody with write access to the
    /// table would do — and leaving it on a shipped library would mean the
    /// append-only rule of `§14.9` was a convention rather than a guarantee.
    #[cfg(any(test, feature = "test-support"))]
    pub const fn store_mut(&mut self) -> &mut S {
        &mut self.store
    }

    /// The realm this journal writes the history of.
    ///
    /// **One realm, given once**, so a caller holding the journal and needing
    /// the realm reads it here rather than being handed it a second time, where
    /// the two could disagree — the kernel, which records a unit in the local
    /// store under the realm whose journal brackets it.
    pub const fn realm(&self) -> &OpaqueId {
        &self.realm
    }

    /// An identifier from the minter this journal's units come from, for a
    /// unit this journal will not bracket.
    ///
    /// **One minter per flake per process**, which [`Journal::new`] says is the
    /// caller's rule — and a caller that needs a unit identifier besides the
    /// ones [`Journal::open_unit`] mints would otherwise need a second minter,
    /// which collides with this one. The kernel's case: when the store has
    /// committed a unit and the journal will not take its close, the kernel
    /// commits the unit's reversal at once, as a unit of its own in the store,
    /// and the journal is the thing that cannot be written at that moment.
    /// Nothing is written here.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NoIdentifier`] when the machine's clock will not yield
    /// an identifier.
    pub fn mint_unit(&mut self) -> Result<Snowflake, Error> {
        self.minted().map_err(Error::NoIdentifier)
    }

    /// Records what happened, chaining it onto the tail.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unavailable`] when the store cannot be reached,
    /// [`Error::Refused`] when it answers and refuses, [`Error::Unstorable`] and
    /// [`Error::TooDeep`] for a record `readable_record` will not accept,
    /// [`Error::UnitClosed`] or [`Error::UnitAlreadyOpen`] when the work unit is
    /// the reason, [`Error::OutOfOrder`] or [`Error::Contended`] when another
    /// writer settled the tail in between, and [`Error::Undecodable`] when the
    /// tail is not what this code wrote.
    ///
    /// **It named the clock's refusal and could not build it.** This takes its
    /// instant from the caller's [`Record`]; the one place this crate asks the
    /// clock is `Journal::bracket`, which `open_unit` and `close_unit` reach —
    /// and through them `begin`, `commit` and `abort` — and this does not; those
    /// three named it nowhere. The variant is not
    /// spelled in this paragraph on purpose: the test below reads the *whole*
    /// section for it, because a list is not the only place a section can make a
    /// claim. So a caller
    /// of the bracket reading its section saw only "whatever the store
    /// returned", and a wrong machine clock read as the store's fault:
    /// That refusal is `JournalUnstorable`, **not** retryable, whose
    /// sentence ends *"this particular error will not clear on its own"*. That
    /// is the retryable/not-retryable inversion this crate removes at every
    /// other seam, arriving through the documentation instead of the code.
    ///
    /// [`Journal::verify`]'s own doc records finding and removing this exact
    /// claim about this exact variant, and says why nothing caught it: the gate
    /// reads bodies for variants a section does not name, and does not read
    /// sections for variants no body builds. The unheld half had rotted here
    /// too, on the crate's primary entry point.
    ///
    /// **It said `Unavailable` alone**, and ordinary caller input reaches
    /// `Unstorable` — a NUL byte in an action — and `TooDeep`. Neither is
    /// retryable and `Unavailable` is, so a caller written against that sentence
    /// retries a defect it caused, for ever. `WorkUnit::record` one file over
    /// carries the honest catch-all this did not.
    pub async fn record(&mut self, record: Record) -> Result<Entry, Error> {
        self.append(record).await
    }

    /// Opens a work unit behind a handle that borrows this journal until it is
    /// dropped: a group of entries that are one step to undo.
    ///
    /// It is [`Journal::open_unit`], with the identifier wrapped so that
    /// [`WorkUnit::record`] stamps it on every entry and [`WorkUnit::commit`]
    /// or [`WorkUnit::abort`] closes it through [`Journal::close_unit`].
    ///
    /// # Errors
    ///
    /// Returns whatever the store returned, [`Error::NoIdentifier`] when the
    /// machine's clock will not yield an identifier, and
    /// [`Error::ClockBeforeEpoch`] when it will not yield an instant —
    /// `Journal::bracket` asks it, and this reaches it through
    /// [`Journal::open_unit`].
    pub async fn begin(
        &mut self,
        label: impl Into<String> + Send,
    ) -> Result<WorkUnit<'_, S, C>, Error> {
        let label = label.into();
        let id = self.open_unit(label.clone()).await?;
        Ok(WorkUnit::over(self, id, label))
    }

    /// Opens a work unit and returns its identifier: mints one, and writes the
    /// opening bracket, whose resource is `label`.
    ///
    /// The label lives in that resource, so it is hashed with everything else
    /// and survives without a second table to keep in step; it is the text a
    /// person sees in the history (`§11.5`).
    ///
    /// **An identifier rather than a handle**, for a caller that holds a unit
    /// across more than one borrow of this journal: one that opens a unit
    /// before a command runs and closes it only after the command's effects
    /// have committed, with other entries written in between — the shape
    /// `§11.5` gives the kernel. Every entry inside the unit carries
    /// the identifier in `work_unit`; [`WorkUnit`] is the same two calls behind
    /// a handle.
    ///
    /// **A NUL in a `label` is stored escaped, not refused.** The label is the
    /// bracket's resource, and canonical JSON writes a NUL as `\u0000` — the
    /// reason `readable_record` leaves the resource out of its NUL rule — so
    /// both stores take it and the close repeats it as written. The section
    /// below named a refusal for such a label, which no path through this call
    /// builds; `a_label_holding_a_nul_is_stored_escaped_in_both_stores` holds
    /// this paragraph instead.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NoIdentifier`] when the machine's clock will not yield
    /// an identifier, [`Error::ClockBeforeEpoch`] when it will not yield an
    /// instant — `Journal::bracket` asks it — and whatever [`Journal::record`]
    /// returns for the bracket otherwise: [`Error::Unavailable`] or
    /// [`Error::Refused`] from the store, [`Error::UnitClosed`] or
    /// [`Error::UnitAlreadyOpen`] for an identifier some other writer already
    /// bracketed, [`Error::OutOfOrder`] or [`Error::Contended`] when another
    /// writer settled the tail in between, and [`Error::Undecodable`] when the
    /// tail is not what this code wrote.
    ///
    /// **A refused opening leaves nothing behind.** The identifier minted for
    /// it is not returned and its label is not kept, so no later close can
    /// carry it: a unit whose bracket never landed was never opened.
    pub async fn open_unit(&mut self, label: impl Into<String> + Send) -> Result<Snowflake, Error> {
        let unit = self.minted().map_err(Error::NoIdentifier)?;
        let label = label.into();
        // **Only the realm, the keeper and the instant are the journal's.**
        // The action, the outcome and the label are this call's, and
        // `bracket` takes them as parameters rather than as a template to be
        // overwritten — the shape a measured defect removed from `begin` once.
        let opening = self.bracket(
            crate::unit::OPENS,
            unit,
            Value::text(label.clone()),
            Outcome::UnitBegun,
        )?;
        self.append(opening).await?;
        self.labels.insert(unit, label);
        Ok(unit)
    }

    /// Closes a work unit this realm opened, committed or aborted, by writing
    /// its closing bracket.
    ///
    /// The bracket's resource is the label the opening carried when this
    /// journal wrote that opening, and `Value::Null` when it did not — a unit
    /// opened by another process, which a recovery pass closes after a crash.
    /// The opening bracket is where a label is read from; the closing one
    /// repeats it only where it is known, and never guesses.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnitNeverOpened`] when this realm's history holds no
    /// opening bracket for `unit`, [`Error::UnitClosed`] when it is already
    /// closed either way, [`Error::ClockBeforeEpoch`] when the machine has no
    /// instant to record — `Journal::bracket` asks the clock — and whatever
    /// [`Journal::record`] returns for the bracket otherwise:
    /// [`Error::Unavailable`] or [`Error::Refused`] from the store,
    /// [`Error::OutOfOrder`] or [`Error::Contended`] when another writer
    /// settled the tail in between, and [`Error::Undecodable`] when the tail is
    /// not what this code wrote.
    ///
    /// **What the first refusal answers, and what it does not.** It stops a
    /// closing bracket for an identifier nobody began — a caller's defect,
    /// which would otherwise close a unit that does not exist and make every
    /// later entry naming it refused as closed. It is asked here, at the door
    /// the kernel closes through, and not by [`Journal::record`] or
    /// [`Store::append`]: both still take a hand-built closing bracket with no
    /// opening before it, and `in_postgres.rs`'s shared scripts pin that the
    /// two stores take it alike.
    ///
    /// **A close the store refused keeps the label, and one refused as
    /// [`Error::UnitClosed`] drops it.** The first is a unit still open, and the
    /// retry `Error::Unavailable` invites carries the same label. The second is
    /// a unit another writer's closing bracket has already closed: no retry of
    /// this close can land, so a label kept for it would be held for as long as
    /// this journal lives.
    pub async fn close_unit(&mut self, unit: Snowflake, how: Closing) -> Result<Entry, Error> {
        if !self.store.unit_opened(&self.realm, unit).await? {
            return Err(Error::UnitNeverOpened { unit });
        }
        let label = self
            .labels
            .get(&unit)
            .map_or(Value::Null, |label| Value::text(label.clone()));
        // Whether it is already closed is `append`'s question, asked of the
        // store as it is for every entry — and in PostgreSQL asked again inside
        // the `INSERT` itself, so a second closer racing this one cannot land.
        let closing = self.bracket(crate::unit::CLOSES, unit, label, how.outcome())?;
        let landed = self.append(closing).await;
        if matches!(landed, Ok(_) | Err(Error::UnitClosed { .. })) {
            self.labels.remove(&unit);
        }
        landed
    }

    /// Verifies the chain from `sequence` onwards.
    ///
    /// # Errors
    ///
    /// Returns where verification stopped and why. It stops at the **first**
    /// entry that does not verify, because everything after it is chained onto
    /// something that already failed and reporting all of it would bury the one
    /// place a reader has to look.
    ///
    /// The outer `Err` is the store's: [`Error::Unavailable`] when it cannot be
    /// reached, [`Error::Refused`] when it answers and refuses, and
    /// [`Error::Unstorable`] when `from` is past what a store can hold, which is
    /// `Store::from`'s first statement.
    ///
    /// **It used to say [`Error::Undecodable`] here, and that is the one case
    /// this function no longer hands back.** A row that is not one this code
    /// wrote reaches `tail()` as `Undecodable`, and the body catches exactly
    /// that and turns it into `Broken { reason: EntryIsNotReadable }` — *a
    /// verdict, not a failure to answer*, which is what the paragraph beside
    /// that arm says. The other way in, `store.from(..)?`, is documented at
    /// [`Store::from`] as `Unavailable`, `Refused` and `Unstorable` only, and
    /// `Postgres::from` reports a bad row through `stopped_at` rather than
    /// through its `Err`. So the sentence described a case that cannot happen
    /// and said the case that *does* happen cannot be described.
    ///
    /// The gate beside it reads bodies for variants the section does not name;
    /// it does not read sections for variants no body builds, and measured on
    /// 2026-09-08 a line added here naming [`Error::ClockBeforeEpoch`] — which
    /// `verify` cannot build by any path — left `cargo test --workspace
    /// --locked` at exit 0. A caller gets a `match` arm that never fires, which
    /// is milder than a wrong answer and is still prose stating the inverse of
    /// what the code does.
    pub async fn verify(&self, from: u64) -> Result<Result<u64, Broken>, Error> {
        // **The tail first, and the entries after it.** These are two queries
        // and not one transaction, so an append can land between them — and in
        // the other order the tail was newer than the read, `last != tail`, and
        // a concurrent write was reported as **tampering**. In this order the
        // only thing an append can do is put an entry after the tail that was
        // read, which the comparison below allows for.
        // **A row this code cannot read is a verdict, not a failure to answer.**
        // It used to travel up as an error, so `verify` returned *no verdict at
        // all* at any starting sequence — the one question the caller asked went
        // unanswered because somebody had put a row in the table. That is
        // exactly what verification is for.
        //
        // **And an undecodable tail is held, not returned.** It used to return
        // at once, and `tail()` decodes the *last* row — so poisoning the last
        // row hid a forgery anywhere before it: measured, entry 1 rewritten to
        // `principal='attacker'` and entry 2 given an unknown outcome, and
        // `verify` answered `Broken { sequence: 2, reason: EntryIsNotReadable }`.
        // An attacker chose which row a responder would investigate by writing
        // one further on, and `verify`'s own doc promises "the **first** entry
        // that does not verify".
        let mut unreadable: Option<u64> = None;
        let tail = match self.store.tail().await {
            Ok(tail) => tail,
            Err(Error::Undecodable { sequence, .. }) => {
                unreadable = Some(sequence);
                None
            }
            Err(other) => return Err(other),
        };
        // **One entry earlier, when there is one, so the first link is checked.**
        // `verify(from)`'s doc says it verifies the chain from `sequence`
        // onwards, and for `from > 0` it did not verify the *first* link at all:
        // there was no predecessor in the read to compare against, and the head
        // branch below only fires at `from == 0`. Measured on a six-entry
        // history with entry 5 relinked to `[0xAA; 32]` and re-hashed:
        //
        // ```text
        // verify(0) -> Err(Broken { sequence: 5, reason: LinkDoesNotMatchPredecessor })
        // verify(5) -> Ok(1)
        // ```
        //
        // A caller verifying incrementally from a checkpoint — which is what a
        // `from` other than zero is for — accepted a forged entry at exactly the
        // checkpoint. Reading one earlier costs one row and gives the anchor;
        // `anchor` is then split off so every count and every "does it start
        // here" answer is still about `from`.
        let read = self.store.from(from.saturating_sub(1)).await?;
        let mut entries = read.entries;
        let anchor = if from > 0 && entries.first().is_some_and(|e| e.sequence == from - 1) {
            Some(entries.remove(0))
        } else {
            None
        };

        // **A read that returned nothing is not a clean history.** It used to be
        // reported as `Ok(0)`, so a caller writing `if verify(0)?.is_ok()`
        // accepted a wiped table — the `§19.5` failure where the count is
        // reported and never enforced. An empty journal is only clean when the
        // store agrees there is nothing in it.
        let Some(tail) = tail else {
            // No tail at all: either the table is empty, or the last row is one
            // this code cannot read. The second is a verdict of its own, and it
            // comes after the loop below — so it is carried past this point
            // rather than answered here.
            //
            // **And the *first* unreadable row, not the tail.** `unreadable`
            // is where `tail()` stopped, which is the last row in the table;
            // `read.stopped_at` is where the read from `from` stopped, which is
            // the first row at or after it that would not decode. With two
            // unreadable rows — 3 and 4 — this named 4, so an attacker who
            // appended one *more* unreadable row still chose which row a
            // responder investigated, which is the promise the ordering above
            // was written to keep.
            if let Some(sequence) = read.stopped_at.as_ref().map(|(at, _)| *at).or(unreadable) {
                return Ok(Err(verdict_after_checking(
                    &entries,
                    from,
                    anchor.as_ref(),
                    Broken {
                        sequence,
                        reason: Reason::EntryIsNotReadable,
                        detail: read.stopped_at.as_ref().map(|(_, why)| why.clone()),
                    },
                )));
            }
            return Ok(if entries.is_empty() {
                Ok(0)
            } else {
                Err(Broken {
                    sequence: from,
                    reason: Reason::HistoryDoesNotStartHere,
                    detail: None,
                })
            });
        };
        Ok(Self::chained_and_contiguous(
            &entries,
            anchor.as_ref(),
            read.stopped_at.as_ref(),
            from,
            Some(&tail),
        ))
    }

    /// Every link, every hash and every sequence, over a read that reached the tail.
    ///
    /// Split from [`Journal::verify`] because it crossed a hundred lines. The order
    /// these checks run in is the substance and is argued for inline: a forged entry
    /// is named before a row poisoned after it, so an attacker cannot choose which
    /// row a responder investigates by poisoning a later one.
    fn chained_and_contiguous(
        entries: &[Entry],
        anchor: Option<&Entry>,
        stopped_at: Option<&(u64, String)>,
        from: u64,
        // **Optional, because the one caller that had none wrote the whole
        // predicate out again.** `verdict_after_checking` is reached when there
        // is no readable tail, and it was a near-copy of this loop with the
        // final check dropped — six branches, of which four were reached by no
        // test in the workspace, so deleting the contiguity, link, genesis-link
        // and first-row-position blocks from the copy left `cargo test
        // --workspace` green. Two of the four decide `SequenceIsNotContiguous`
        // and `LinkDoesNotMatchPredecessor`, and this file's own comment says
        // those codes exist so a responder knows whether to look at one row or
        // at the whole history above it. One predicate, one implementation.
        tail: Option<&Entry>,
    ) -> Result<u64, Broken> {
        let Some(first) = entries.first() else {
            // **A read that stopped at its very first row says so.** This
            // returned before the `stopped_at` check below it, so when the row
            // at `from` was the unreadable one the reason was discarded and the
            // verdict became "the rows before it are gone" — which
            // `HistoryDoesNotStartHere`'s own doc says, and which is false: the
            // row is on disk and was rewritten in place. Measured, one tamper
            // applied at each position of a five-entry history:
            //
            // ```text
            // position 0 -> HistoryDoesNotStartHere / JournalChainBroken
            // position 1 -> EntryIsNotReadable      / JournalEntryTampered
            // ```
            //
            // The two codes exist so a responder knows whether to look at one
            // row or at the whole history above it, and the sentence a person
            // reads differs too. The sibling branch above already consults
            // `stopped_at` first; the fix existed in one half of this function
            // and not the other.
            if let Some((sequence, why)) = stopped_at {
                return Err(Broken {
                    sequence: *sequence,
                    reason: Reason::EntryIsNotReadable,
                    detail: Some(why.clone()),
                });
            }
            return Err(Broken {
                sequence: from,
                reason: Reason::HistoryDoesNotStartHere,
                detail: None,
            });
        };
        // And a read that began somewhere other than where it was asked is a
        // read whose earlier rows are gone — invisible to a chain of links,
        // because every remaining link is still consistent.
        if first.sequence != from {
            return Err(Broken {
                sequence: from,
                reason: Reason::HistoryDoesNotStartHere,
                detail: None,
            });
        }
        // And the read has to reach the tail the store itself reports. It used
        // to fetch that tail and discard it, so a store whose read hid entries
        // in the middle — while still reporting the real last sequence —
        // verified clean over the ones it did return. The evidence was in hand
        // and thrown away.
        // `entries` is non-empty — the check above returns when it is not — so
        // the fallback is `first`, which is the same entry when there is exactly
        // one. It used to be `from`, a value no input can produce here, which
        // left a reader working out when a read could reach this line and start
        // somewhere other than where it starts.
        let last = entries.last().unwrap_or(first).sequence;
        // The entry before `from`, when the read reached one. It is not counted
        // and not re-verified — it is only what the first link is compared
        // against, which without it is compared against nothing.
        let mut previous: Option<&Entry> = anchor;
        let mut checked = 0_u64;
        for entry in entries {
            // **Where it is, not what it says it is.** `sequence` is one of the
            // fields being verified, so on the path that has just proved the
            // contents were tampered with, reading the position *out of* those
            // contents sends the responder to a row the attacker numbered:
            // measured, `Broken { sequence: 101 }` for a store holding
            // sequences 0..8, and `sequence: 1000000` after a renumber. The
            // unreadable-row path was repaired for exactly this a round ago and
            // this one was not. `previous` is the last entry this loop trusted,
            // so its successor is where the break is.
            let where_it_is = previous.map_or(from, |before| before.sequence + 1);
            if entry.hash != entry.compute_hash() {
                return Err(Broken {
                    sequence: where_it_is,
                    reason: Reason::HashDoesNotMatchContents,
                    // **Both numbers, which this arm alone did not carry.**
                    // `where_it_is` is the safe position — the successor of the
                    // last entry this loop trusted — and it is not necessarily a
                    // row that exists: a tampered entry can be renumbered.
                    // Measured, entry 1 rewritten and moved to 500000 with the
                    // tail left readable, so this ordinary path runs:
                    //
                    // ```text
                    // Err(Broken { sequence: 1, reason: HashDoesNotMatchContents, detail: None })
                    // "entry 1 does not verify: its hash is not the hash of its contents"
                    // ```
                    //
                    // and there is no row numbered 1 to look at. `verdict_after_checking`
                    // carries the row's own number for exactly this reason and its
                    // comment calls the other shape a defect; the contiguity arm
                    // two lines below carries it too. This one reached first.
                    detail: Some(format!("the row there is numbered {}", entry.sequence)),
                });
            }
            if let Some(before) = previous {
                if entry.sequence != before.sequence + 1 {
                    return Err(Broken {
                        sequence: where_it_is,
                        reason: Reason::SequenceIsNotContiguous,
                        // The position is the verdict and the claimed number is
                        // the detail: a responder needs the place the chain
                        // broke, which the attacker does not choose, *and* the
                        // number to grep the table for, which they do.
                        detail: Some(format!("the row there is numbered {}", entry.sequence)),
                    });
                }
                if entry.previous.as_ref() != Some(&before.hash) {
                    return Err(Broken {
                        sequence: where_it_is,
                        reason: Reason::LinkDoesNotMatchPredecessor,
                        detail: None,
                    });
                }
            } else if entry.previous.is_some() {
                // The first entry of the whole history links to nothing. One
                // that links to something means an entry was removed from in
                // front of it — the only tampering a chain cannot otherwise see,
                // because every remaining link is still consistent.
                //
                // **This read `from == 0 && …`, and the conjunct was reached.**
                // `previous` is `None` here in two ways: `from` is 0, or the
                // anchor row — the entry at `from - 1` — is absent, which is
                // what `anchor` returns `None` for. In the second case the
                // conjunct switched the check off and the first entry's
                // backward link was compared against nothing:
                //
                // ```text
                // remove row 10, then verify(11)
                //   with the conjunct:     Ok(29)
                //   without it:            Err(Broken { sequence: 11,
                //                                 reason: LinkDoesNotMatchPredecessor })
                // ```
                //
                // That is the pre-anchor defect reinstated for exactly the
                // histories whose boundary row is missing. §15.8 carves the
                // journal out of row-count pruning, so a predecessor that is
                // not there was removed, whatever `from` is.
                return Err(Broken {
                    sequence: where_it_is,
                    reason: Reason::LinkDoesNotMatchPredecessor,
                    detail: None,
                });
            }
            previous = Some(entry);
            checked += 1;
        }
        // **A read that stopped at a row it could not decode explains itself**,
        // and is the more useful answer than "the history stops short": the
        // short read is the *consequence*.
        //
        // **After the loop, which is what the comment here already claimed.**
        // The `return` was above it, so an entry forged early and a later row
        // poisoned with an unreadable value made verification name the *later*
        // one — measured: entry 1 rewritten to `action='Escalate',
        // principal='attacker'`, entry 4 given an unknown result, and `verify`
        // answered `Broken { sequence: 4, reason: EntryIsNotReadable }`.
        //
        // `verify`'s own doc promises "the **first** entry that does not
        // verify", and `§15.1` asks a diagnostic to point at an exact location.
        // Reported after the loop, an attacker cannot choose which row gets
        // investigated by poisoning a row further on.
        if let Some((sequence, why)) = stopped_at {
            return Err(Broken {
                sequence: *sequence,
                reason: Reason::EntryIsNotReadable,
                detail: Some(why.clone()),
            });
        }
        // And last, the short read itself. `<`, not `!=`: an entry appended
        // after the tail was read is not tampering, and under `!=` it was
        // reported as such. What this catches is the read *falling short* of
        // what the store says is there — a truncated tail, the one shape a chain
        // of links cannot see, because every remaining link is still consistent.
        //
        // **Last of the three**, because it is the most often a consequence: a
        // row that would not decode makes every read short, and answering
        // "the history stops at 0" for a poisoned row 1 sends a reader to the
        // wrong place. Each of these three orderings was wrong once.
        // **Exact, and the only test of it used a five-row shortfall** — so any
        // tolerance up to four was invisible: `if last + 4 < tail.sequence` left
        // `cargo xtask check` green. The comment below says this catches the one
        // shape a chain of links cannot see, because every remaining link is
        // still consistent; a tolerance is precisely the thing that must be zero
        // for that to be true.
        // Only when there is a tail to reach: a caller with no readable tail is
        // asking where the history first breaks, and "it does not reach the
        // tail" is not an answer it can act on.
        if let Some(tail) = tail
            && last < tail.sequence
        {
            return Err(Broken {
                sequence: last,
                reason: Reason::HistoryDoesNotReachTheTail,
                detail: None,
            });
        }
        Ok(checked)
    }

    /// The record a bracket entry for a work unit carries.
    ///
    /// **One function, and it guesses nothing.** There were two: `template`,
    /// and a `blank_record` that supplied an action, a resource and an outcome
    /// which its only caller then overwrote on the next three lines — the same
    /// defect `WorkUnit::begin` had already been fixed for, one level down.
    /// A default a caller must remember to replace is a default that will
    /// eventually be shipped by somebody who does not.
    ///
    /// What the journal supplies is the keeper — not a user: the kernel opened
    /// and closed the group — and the instant. Everything else is a parameter,
    /// the resource included: an opening carries its label, and a closing
    /// carries the label when the journal writing it knows one.
    pub(crate) fn bracket(
        &self,
        action: &str,
        unit: Snowflake,
        resource: Value,
        outcome: Outcome,
    ) -> Result<Record, Error> {
        let mut record = Record::new(self.keeper.clone(), action, resource, outcome, now_ms()?);
        record.work_unit = Some(unit);
        Ok(record)
    }

    /// Chains a record onto the tail and writes it.
    ///
    /// **Three of the rules here are enforced twice, and two of the outer copies
    /// are now decided.** This paragraph used to say all three were written down
    /// rather than tested, and that testing them needed a fake `Store` — "a
    /// fourth implementation of the trait, written to be wrong". That obstacle
    /// was disproved in the same crate: a **forged tail** reaches the outer guard
    /// without any new `Store` at all, and
    /// `a_closed_unit_is_the_answer_before_the_tail_is_read` and
    /// `an_open_unit_is_the_answer_before_the_tail_is_read` do it. Measured on
    /// 2026-09-11, each guard made never to fire is red in its own test, naming
    /// `Undecodable` where `UnitClosed` and `UnitAlreadyOpen` belong.
    ///
    /// **The third was decided by nothing and is decided now.** It is
    /// `WorkUnit::record`'s `if self.closed`: made never to fire, the suite used
    /// to stay green, because both shipped stores re-apply the rule and return
    /// the same variant, so what that copy decides is the *order* of two
    /// refusals — observable only through a third-party `Store`, which the
    /// public trait makes possible. `unit::tests::a_closed_unit_refuses_a_record_the_store_would_have_taken`
    /// supplies one that takes anything, and `mutants.py` carries the operator.
    ///
    /// The paragraph outlived the repair by several rounds, and a reviewer
    /// found it in round seventy-one. A doc telling the next reader that a live
    /// guard is untested invites them either to redo the work or to delete the
    /// guard as redundant.
    ///
    /// Its sibling is tested too:
    /// `a_record_the_caller_wrote_wrong_is_named_before_anything_about_the_store`
    /// pins what the `readable` call decides, which is the same shape. And a
    /// `Store` that disagrees with any of the four is caught by
    /// `both_stores_answer_the_same_script_the_same_way` the moment it is added
    /// to that comparison.
    pub(crate) async fn append(&mut self, mut record: Record) -> Result<Entry, Error> {
        // **Refused before it is written, because there is no way back.** The
        // rule itself is `readable`, which every `Store` also applies — this
        // call is so that the refusal names the caller's own record before a
        // sequence has been minted for it.
        // `nc_canon` writes a value of any depth and `nc_canon::parse` refuses
        // one past `MAX_DEPTH` — an asymmetry that is harmless everywhere except
        // here. One record with a resource nested past the bound was accepted,
        // hashed and stored, and from that moment the journal could not be read
        // (`from` fails to decode the row), could not be verified (no verdict at
        // all, at any starting sequence), and could not be appended to (`tail`
        // cannot decode the tail) — and the append-only trigger this table is
        // built around meant the row could not be deleted either. One call,
        // permanently, from ordinary input: a payload at exactly the limit,
        // which `parse` accepts, wrapped once under a key.
        if let Some(problem) = readable_record(&record).err() {
            return Err(problem);
        }
        // The realm is the journal's. A caller cannot write another realm's
        // history through this one.
        record.realm = self.realm.clone();
        // **And a closed unit is closed for every path in.** `WorkUnit::record`
        // refuses one; `Journal::record` with a hand-set `record.work_unit` did
        // not — measured, an entry naming a committed unit landed *after* its
        // own `UnitCommitted` bracket and was inside the group to every SQL
        // reader, with `verify` answering clean. That is the shape
        // `postgres.rs`'s own header names: a rule enforced only where it is
        // convenient to enforce it is a rule about one code path.
        //
        // The question is asked of the store rather than of a flag, because the
        // store is what a second process would also see.
        //
        // **And the unit is asked first here, where the stores ask the sequence
        // first.** That is not two answers to one question: this path computes
        // the sequence from `head()` four lines down, so it never presents a
        // stale one and there is no ordering question to ask. `Store::append`
        // takes the sequence from its caller, which is where the two can both be
        // wrong at once and where the order decides what is reported.
        if let Some(unit) = record.work_unit
            && self.store.unit_closed(&self.realm, unit).await?
        {
            return Err(Error::UnitClosed { unit });
        }
        // **And a unit is opened once.** The same door one bracket along: a
        // hand-set `work_unit` with `Outcome::UnitBegun` wrote a second opening
        // for an open unit, with a different principal and a different label,
        // and `verify` called the history clean.
        if let Some(unit) = record.work_unit
            && record.action == crate::unit::OPENS
            && record.result == Outcome::UnitBegun
            && self.store.unit_opened(&self.realm, unit).await?
        {
            return Err(Error::UnitAlreadyOpen { unit });
        }
        // **The tail, and it has to verify.** This read `head()` — the last
        // sequence and its hash, without decoding the row — on the reasoning
        // that neither is the part that fails to decode. What that also skipped
        // is whether the row is *its own*: measured, an entry chained onto a
        // forged tail returned `Ok` with no error at all, because the hash guard
        // lived only on the refusal path and a tamperer need never provoke a
        // refusal.
        //
        // Decoding the tail costs one row's parse per append, on a path that
        // already reads it. That is the price of the promise this crate makes,
        // and an undecodable tail is now a refusal rather than something to
        // chain onto — you cannot extend a history you cannot read.
        // **And the fallback this paragraph used to describe is gone.** It read
        // *"an undecodable tail falls back to `head()`, which reads the sequence
        // and the hash without parsing the row"*, and cited a rule with two
        // tests behind it. That is the design this crate deliberately reversed,
        // and it was left standing between the two paragraphs that say so —
        // above the `match` a reader meets first. Writing the code it describes
        // is red in two places: `a_row_this_code_cannot_read_stops_the_unit_
        // being_closed_and_says_which_row` and `a_row_this_code_did_not_write_
        // is_a_verdict_rather_than_a_silence`, both of which `expect_err`.
        //
        // What the rule it cited was about is still true and is served another
        // way: a foreign unreadable row must not brick an append-only log, and
        // it does not — `verify` reports it and says where, and a *new* journal
        // whose store holds nothing unreadable appends as it always did. What
        // changed is that you cannot extend a history you cannot read.
        let (sequence, previous) = match self.store.tail().await {
            Ok(Some(tail)) => {
                if tail.hash != tail.compute_hash() {
                    return Err(Error::Undecodable {
                        sequence: tail.sequence,
                        why: "the entry this one would chain onto does not match its own \
                              contents, so the history it would extend is not the history \
                              it was read from"
                            .to_owned(),
                    });
                }
                (tail.sequence + 1, Some(tail.hash))
            }
            Ok(None) => (0, None),
            // **A tail this code cannot read is a refusal, not a fallback.**
            // The fallback read `head()`, which returns the sequence and hash
            // without parsing the record — on the rule that a foreign
            // unreadable row must not brick an append-only log. Two things
            // measured against it:
            //
            //   * it does not achieve that. `head()` parses the *hash column*,
            //     and `hash TEXT` carries no constraint, so one `INSERT` with a
            //     non-hex hash refuses there too — the append is blocked either
            //     way, and the fallback only changed which sentence says so.
            //   * where it does work, it hands the next honest entry whatever
            //     sixty-four characters the foreign row put in `hash`. Measured:
            //     `the honest entry chained onto the attacker's chosen hash`.
            //
            // So the choice is between refusing and extending a history this
            // code cannot read, and `§14.9` decides it: the journal is the audit
            // log and fails closed. What an operator does is read the row, and
            // — since the table refuses `DELETE` — append a new anchor after
            // deciding what the unreadable row was, which is a decision with a
            // person in it rather than a default.
            Err(other) => return Err(other),
        };
        let entry = Entry::chain(sequence, previous, record);
        self.store.append(entry.clone()).await?;
        Ok(entry)
    }
}

/// Milliseconds since the Unix epoch.
fn now_ms() -> Result<u64, Error> {
    the_instant(std::time::SystemTime::now())
}

/// The millisecond an instant names, or the refusal a clock before 1970 earns.
///
/// **Split from its one caller so a test can supply the clock.** The refusal
/// below was decided by nothing: the assertion that claimed to close it —
/// `record.recorded_at > 1_700_000_000_000` against the real system clock —
/// cannot establish its own premise, because the premise *is* a clock before
/// the epoch. A reviewer measured that in round seventy-one by replacing the
/// `map_err` with `unwrap_or_default()`: `cargo xtask check` printed
/// `check passed` at `ok 1427 test(s)`, clippy was silent, and no census moved.
///
/// Taking the instant from the journal's own `C: Clock` does not work and was
/// tried: `Clock::now_ms` returns a bare `u64` and cannot express the failure,
/// which is why `nc-ids` range-checks the value in `Minter::mint` instead. A
/// parameter is the smallest seam that can be handed one.
fn the_instant(now: std::time::SystemTime) -> Result<u64, Error> {
    // **A clock this cannot read is refused, not rounded to the epoch.** This
    // was `nc-ids`' `SystemClock::now_ms` with the guards taken off: its
    // `map_or(0, …)` turns a clock set before 1970 into `recorded_at = 0`, which
    // is hashed into the chain and stored in a `BIGINT NOT NULL` with no
    // `CHECK`. `nc-ids` follows the identical expression with `within_range` and
    // answers `MintError::BeforeEpoch`, for the same reason and about the same
    // number.
    //
    // A journal entry is the one record here that cannot be corrected
    // afterwards: the timestamp is inside the digest, so an entry claiming 1970
    // is an entry that verifies. Refusing costs a caller an error on a machine
    // whose clock is wrong; accepting costs the audit log its meaning.
    now.duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| Error::ClockBeforeEpoch)
        .and_then(|since| u64::try_from(since.as_millis()).map_err(|_| Error::ClockBeforeEpoch))
}

/// The value a caller hands the journal for the resource it acted on.
///
/// Canonical JSON rather than a string, because the resource of a command is
/// whatever that command's `resource-selector` extracted, and the journal has to
/// hash it reproducibly (`§16.4`).
pub type Resource = Value;

/// The earliest broken link in `entries`, or `otherwise` when there is none.
///
/// **The first entry that does not verify**, which is what `verify` promises.
/// A verdict about a row further on — one this code cannot read — is true and
/// is not the most useful thing to say, and reporting it first let an attacker
/// choose which row a responder investigated.
fn verdict_after_checking(
    entries: &[Entry],
    from: u64,
    anchor: Option<&Entry>,
    otherwise: Broken,
) -> Broken {
    // **The same predicate, not a second copy of it.** This was the loop above
    // written out again with the reach-the-tail check dropped, and four of its
    // six branches were reached by no test in the workspace: deleting the
    // contiguity, link, genesis-link and first-row-position blocks left
    // `cargo test --workspace` green. Deleting the *whole* body was caught, so
    // it was exactly the copied branches that nothing held — which is what a
    // copy of a predicate is for.
    //
    // The consequence is a wrong diagnosis rather than a missed one: with those
    // branches gone a removed or relinked entry is reported as
    // `EntryIsNotReadable`, and `SequenceIsNotContiguous` and
    // `LinkDoesNotMatchPredecessor` exist so a responder knows whether to look
    // at one row or at the whole history above it.
    //
    // An empty read is answered here rather than delegated: with no entries the
    // predicate says `HistoryDoesNotStartHere`, and what this caller knows is
    // the unreadable row it was given.
    if entries.is_empty() {
        return otherwise;
    }
    match Journal::<crate::memory::InMemory>::chained_and_contiguous(
        entries, anchor, None, from, None,
    ) {
        Ok(_) => otherwise,
        Err(broken) => broken,
    }
}

#[cfg(test)]
mod storable_tests {
    use super::{Error, Record, readable_record};
    use nc_canon::Value;
    use nc_ids::OpaqueId;

    fn record() -> Record {
        Record::new(
            OpaqueId::new("did:key:zProbe").expect("a valid identifier"),
            "probe.run",
            Value::text("x"),
            crate::Outcome::Allowed,
            1,
        )
    }

    fn nested(levels: usize) -> Value {
        let mut value = Value::Integer(1);
        for _ in 0..levels {
            value = Value::Array(vec![value]);
        }
        value
    }

    /// The depth bound is where the reader's is, not one short of it.
    ///
    /// **It refused a resource its own reader accepts**, and said so with two
    /// numbers on different scales: `Value::depth()` counts a scalar as 1 and
    /// the parser starts the outermost value at 0. Measured:
    ///
    /// ```text
    /// PROBE-RT nested 64  REFUSED-ON-WRITE the record's resource nests 65 deep
    ///                     and the journal reads 64.
    /// ```
    ///
    /// against a value `nc_canon::parse` takes — `nc-canon`'s own test asserts
    /// that exact text parses. The refusal was in the safe direction, and a
    /// sentence comparing two numbers that do not measure the same thing is the
    /// `§15.1` shape this repository hunts elsewhere.
    #[test]
    fn the_deepest_resource_the_reader_takes_is_one_the_journal_stores() {
        // The boundary from below: the deepest value the parser accepts, which
        // is what a caller can legitimately send.
        let mut at = record();
        at.resource = nested(nc_canon::parse::MAX_DEPTH);
        assert!(
            nc_canon::parse(&at.resource.to_canonical()).is_ok(),
            "this test's premise is that the reader accepts this value"
        );
        readable_record(&at).expect("a resource the reader accepts is one the journal stores");

        // And from above: one deeper is refused, naming both numbers on one
        // scale, so the sentence a reader gets is arithmetic they can check.
        let mut past = record();
        past.resource = nested(nc_canon::parse::MAX_DEPTH + 1);
        let measured = past.resource.depth();
        match readable_record(&past).expect_err("one deeper is refused") {
            Error::TooDeep { depth, limit } => {
                assert_eq!(depth, measured);
                assert_eq!(limit, nc_canon::parse::MAX_NESTING);
                assert!(depth > limit, "{depth} is not past {limit}");
            }
            other => panic!("{other:?}"),
        }
    }

    /// A number no column can hold is refused by the journal, not by one store.
    ///
    /// **`recorded_at` is a `u64` and `policy_epoch` an `Option<u64>`, both the
    /// caller's, and PostgreSQL's BIGINT is signed.** Anything above `i64::MAX`
    /// passed this function, was hashed, and was refused by the PostgreSQL store
    /// alone — as `Unavailable`, the code whose description is "the journal
    /// store did not answer" and which is `retryable`, for a store that had
    /// never been asked. Measured:
    ///
    /// ```text
    /// R8B REFUSED [recorded_at = 2^63] code=JournalUnavailable retryable=true
    /// R8B DIVERGE readable_record: Ok | InMemory: Ok | Postgres: Err
    /// ```
    ///
    /// Two implementations of one documented method disagreeing about what a
    /// journal accepts is the shape `Postgres::append`'s own comment names.
    #[test]
    fn a_number_no_column_can_hold_is_refused_by_the_journal_and_not_by_one_store() {
        for (field, build) in [
            (
                "recorded_at",
                (|r: &mut Record, v: u64| r.recorded_at = v) as fn(&mut Record, u64),
            ),
            ("policy_epoch", |r: &mut Record, v: u64| {
                r.policy_epoch = Some(v);
            }),
        ] {
            // The boundary from below: the largest value BIGINT holds.
            let mut at = record();
            build(&mut at, i64::MAX as u64);
            readable_record(&at).expect("i64::MAX is storable");

            // And one past it, named as the caller's and not as the store's.
            for past in [i64::MAX as u64 + 1, u64::MAX] {
                let mut record = record();
                build(&mut record, past);
                match readable_record(&record).expect_err("past BIGINT is refused") {
                    Error::Unstorable { field: named, why } => {
                        assert_eq!(named, field);
                        assert!(why.contains("BIGINT"), "{why}");
                    }
                    other => panic!("`{field}` = {past} was refused as {other:?}"),
                }
            }
        }

        // And the code is not the store's, so a responder is not sent to
        // PostgreSQL and is not told to retry.
        let mut past = record();
        past.recorded_at = u64::MAX;
        let refusal = readable_record(&past).expect_err("refused");
        assert_ne!(refusal.code(), nc_catalog::ErrorCode::JournalUnavailable);
        assert!(!refusal.code().retryable());
    }

    /// A record no store could keep is refused here, not blamed on the store.
    ///
    /// **`readable_record` called itself "the one rule every store enforces"
    /// and checked only the resource's depth.** `action`, `deciding_policy` and
    /// `policy_pack` are plain `String`s with no validation — every principal
    /// field is an `OpaqueId` and these three are not — so a NUL was hashed,
    /// passed this, and came back from PostgreSQL as
    ///
    /// ```text
    /// PROBE-RT action with a NUL  REFUSED-ON-WRITE the journal store did not answer:
    ///   error returned from database: invalid byte sequence for encoding "UTF8": 0x00
    /// ```
    ///
    /// The store answered perfectly well; the record was unstorable. Reporting
    /// the second as the first sends a responder to PostgreSQL (`§15.1`), which
    /// is the sentence `Undecodable`'s own doc was written to avoid, one
    /// direction round.
    #[test]
    fn text_no_store_could_keep_is_refused_by_name_and_not_called_unavailable() {
        let nul = char::from(0);
        // **The offset is in characters, and only a non-ASCII case can say so.**
        // Every fixture here was `a\0b`, `p\0q`, `v\0w` — ASCII, where the byte
        // offset and the character offset are the same number — so reverting
        // `chars().position(…)` to `find(…)` was invisible. Measured through the
        // fix: `ação\0b` is character 4 and byte 6; `é😀\0b` is character 2 and
        // byte 6.
        for (text, at) in [("a\0b", 1), ("ação\0b", 4), ("é😀\0b", 2)] {
            let mut record = record();
            record.action = text.to_owned();
            match readable_record(&record).expect_err("a NUL is not storable") {
                Error::Unstorable { why, .. } => assert!(
                    why.contains(&format!("at character {at}")),
                    "the offset is not the character position: {why}"
                ),
                other => panic!("{other:?}"),
            }
        }

        for (field, build) in [
            (
                "action",
                (|r: &mut Record, c: char| r.action = format!("a{c}b")) as fn(&mut Record, char),
            ),
            ("deciding_policy", |r: &mut Record, c: char| {
                r.deciding_policy = Some(format!("p{c}q"));
            }),
            ("policy_pack", |r: &mut Record, c: char| {
                r.policy_pack = Some(format!("v{c}w"));
            }),
        ] {
            let mut record = record();
            build(&mut record, nul);
            match readable_record(&record).expect_err("a NUL is not storable") {
                Error::Unstorable { field: named, why } => {
                    assert_eq!(named, field);
                    assert!(why.contains("NUL"), "{why}");
                }
                other => panic!("`{field}` was refused as {other:?}"),
            }
        }

        // **The resource is immune, and stays out of the rule.** Canonical JSON
        // escapes the byte, so what reaches the column is printable either way.
        let mut record = record();
        record.resource = Value::text(format!("a{nul}b"));
        assert!(record.resource.to_canonical().contains("\\u0000"));
        readable_record(&record).expect("a NUL in the resource is escaped, not stored raw");

        // And the code is not the store's. `JournalUnavailable` is `retryable`
        // and its description is "the journal store did not answer"; retrying an
        // unstorable record produces the same answer forever.
        let refused = Error::Unstorable {
            field: "action",
            why: "probe".to_owned(),
        };
        assert_ne!(refused.code(), nc_catalog::ErrorCode::JournalUnavailable);
        assert!(!refused.code().retryable());
        assert_eq!(
            refused.code(),
            Error::TooDeep {
                depth: 66,
                limit: 65
            }
            .code()
        );
    }
}

#[cfg(test)]
mod code_tests {
    use super::{Broken, Error, Reason};
    use nc_catalog::ErrorCode;

    #[test]
    fn the_distinctions_the_codes_claim_are_real() {
        // Both `code` methods carry a comment explaining which failures a
        // responder must be able to tell apart, and neither had a caller — so
        // the claim was prose. A store that did not answer and a store that
        // answered with a broken chain send someone to different places, and so
        // does an entry edited in place versus a history that moved beneath it.
        assert_ne!(
            Error::Unavailable("gone".to_owned()).code(),
            Error::OutOfOrder {
                expected: 1,
                found: 2
            }
            .code(),
        );
        let tampered = Broken {
            sequence: 3,
            reason: Reason::HashDoesNotMatchContents,
            detail: None,
        };
        let moved = Broken {
            sequence: 3,
            reason: Reason::LinkDoesNotMatchPredecessor,
            detail: None,
        };
        assert_ne!(tampered.code(), moved.code());
        assert_eq!(tampered.code(), ErrorCode::JournalEntryTampered);

        // `NoIdentifier` carries a minting failure, so it is reported under the
        // identifier area rather than the journal's: the journal answered, and
        // the machine's clock did not. It was untestable until `Journal` took a
        // clock — it fixed `Minter<SystemClock>`, so no test could make `begin`
        // fail, and the mapping could be repointed anywhere with the suite green.
        assert_eq!(
            Error::NoIdentifier(nc_ids::MintError::BeyondRange)
                .code()
                .area(),
            "IDS"
        );
        assert_ne!(
            Error::NoIdentifier(nc_ids::MintError::BeyondRange).code(),
            Error::Unavailable("gone".to_owned()).code(),
        );

        // **And the three things a unit can be said to be are three codes.**
        // Closed, already open and never opened each tell a caller something
        // different to do, and a shared code is one sentence for all three.
        let unit = nc_ids::Snowflake::from_crockford("000000000000C").expect("a work unit");
        let codes: std::collections::BTreeSet<ErrorCode> = [
            Error::UnitClosed { unit }.code(),
            Error::UnitAlreadyOpen { unit }.code(),
            Error::UnitNeverOpened { unit }.code(),
        ]
        .into_iter()
        .collect();
        assert_eq!(codes.len(), 3, "two unit refusals share a code: {codes:?}");

        // And the four reasons that mean the same thing to a responder — the
        // history moved beneath the entry — deliberately share one code, so that
        // the assertion above is about a real distinction rather than about
        // every variant happening to differ.
        for reason in [
            Reason::LinkDoesNotMatchPredecessor,
            Reason::SequenceIsNotContiguous,
            Reason::HistoryDoesNotStartHere,
            Reason::HistoryDoesNotReachTheTail,
        ] {
            assert_eq!(
                Broken {
                    sequence: 0,
                    reason,
                    detail: None,
                }
                .code(),
                ErrorCode::JournalChainBroken,
                "{reason:?}"
            );
        }
    }
}

/// The question whether a unit is closed, and where it is answered.
#[cfg(test)]
mod the_closed_unit_question {
    /// A store answers whether a unit is closed in one place: `unit_closing`.
    ///
    /// **`Store::unit_closed` is provided, and a provided method can be
    /// written over.** An implementation that does has two answers to one
    /// question — the predicate `unit_closing` holds and a second copy that
    /// can drift from it clause by clause, which is the history of every copy
    /// of the bracket predicate in this crate. Rust has no `final`, so the
    /// rule is held here: across every Rust file in this crate, however deep,
    /// the one `fn unit_closed(` is the trait's own.
    ///
    /// **However deep, because the walk read two directories and not what was
    /// under them.** A store in a suite's shared module, one directory below
    /// `tests`, writing its own `unit_closed` as `Ok(false)` — compiled into a
    /// suite and disagreeing with its `unit_closing` about every unit — left
    /// this green, reported on 2026-09-28; a shared `support` module is exactly
    /// where the kernel's tests keep theirs. It walks from the crate's root now,
    /// and nothing sits there but the manifest, so a walk that stopped
    /// descending reads no Rust at all.
    ///
    /// The second count is the ground: the trait's declaration and every store
    /// this crate writes answer `unit_closing` — `InMemory`, `Postgres`, the
    /// three fault-injecting stores in `unit.rs` and the four in `tampering.rs`
    /// — so a walk that stopped reading a directory, or stopped going down into
    /// one, fails here too.
    #[test]
    fn a_store_answers_whether_a_unit_is_closed_in_one_place() {
        // Assembled, because this file is one of the files read.
        let provided = format!("fn {}(", "unit_closed");
        let required = format!("fn {}(", "unit_closing");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut written: Vec<(String, usize)> = Vec::new();
        let mut answered = 0_usize;
        let mut pending = vec![root.to_path_buf()];
        while let Some(directory) = pending.pop() {
            for entry in std::fs::read_dir(&directory)
                .expect("the crate's directories are readable")
                .flatten()
            {
                let path = entry.path();
                // The entry's own type, so a link to a directory is not
                // followed round a loop.
                if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    pending.push(path);
                    continue;
                }
                if path.extension().is_none_or(|kind| kind != "rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).expect("a source file is readable");
                let code = super::the_code_without_comments(&text);
                let times = code.matches(&provided).count();
                if times > 0 {
                    let named = path.strip_prefix(root).unwrap_or(&path).to_string_lossy();
                    written.push((named.replace('\\', "/"), times));
                }
                answered += code.matches(&required).count();
            }
        }
        written.sort();
        assert_eq!(
            written,
            vec![("src/lib.rs".to_owned(), 1)],
            "`unit_closed` is written somewhere other than the trait's provided method, so \
             one store answers \"is this unit closed\" with a predicate of its own beside \
             the one `unit_closing` holds"
        );
        assert_eq!(
            answered, 10,
            "{answered} declaration(s) of `unit_closing` were read, and the trait and its \
             nine stores in this crate make ten: the walk is not reading the ground it \
             is about"
        );
    }
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

/// One piece of this crate's source with every comment blanked.
///
/// **`//` is one spelling of a comment.** A rule that counts a needle over
/// raw text is paid by a `//` line naming the call; a rule that strips only
/// `//` is paid by a `/* … */`. Both were measured in this tree on
/// 2026-09-11 and 2026-09-12, in three files each.
///
/// A state machine rather than a scan of each line: a line comment runs to
/// its newline, a block comment nests, a raw string ends at its own hash
/// count, and neither opens inside a string. Blanked rather than deleted,
/// so a line here is a line in the file.
///
/// `xtask` owns the same reader and this crate cannot reach it: `xtask` is
/// the gate, not a library, and a dev-dependency on it would put the gate
/// inside the thing it judges.
#[cfg(test)]
fn the_code_without_comments(text: &str) -> String {
    let source: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut at = 0_usize;
    let blank = |out: &mut String, here: char| out.push(if here == '\n' { '\n' } else { ' ' });
    while at < source.len() {
        let here = source[at];
        let next = source.get(at + 1).copied();
        if here == '/' && next == Some('/') {
            while at < source.len() && source[at] != '\n' {
                blank(&mut out, source[at]);
                at += 1;
            }
            continue;
        }
        if here == '/' && next == Some('*') {
            let mut depth = 0_usize;
            while at < source.len() {
                if source[at] == '/' && source.get(at + 1) == Some(&'*') {
                    depth += 1;
                    blank(&mut out, source[at]);
                    blank(&mut out, source[at + 1]);
                    at += 2;
                    continue;
                }
                if source[at] == '*' && source.get(at + 1) == Some(&'/') {
                    depth -= 1;
                    blank(&mut out, source[at]);
                    blank(&mut out, source[at + 1]);
                    at += 2;
                    if depth == 0 {
                        break;
                    }
                    continue;
                }
                blank(&mut out, source[at]);
                at += 1;
            }
            continue;
        }
        if here == 'r' {
            let mut hashes = 0_usize;
            while source.get(at + 1 + hashes) == Some(&'#') {
                hashes += 1;
            }
            if source.get(at + 1 + hashes) == Some(&'"') {
                for _ in 0..=(1 + hashes) {
                    out.push(source[at]);
                    at += 1;
                }
                while at < source.len() {
                    let closes = source[at] == '"'
                        && (0..hashes).all(|step| source.get(at + 1 + step) == Some(&'#'));
                    out.push(source[at]);
                    at += 1;
                    if closes {
                        for _ in 0..hashes {
                            out.push(source[at]);
                            at += 1;
                        }
                        break;
                    }
                }
                continue;
            }
        }
        // **A character literal, which is not a quote.** This knew `//`,
        // `/* */`, raw strings and `"…"` — and `'"'` is a `"` that opens
        // nothing. One such line turns the stripper off from there on, and
        // with it every rule that reads what it hands back: measured in
        // this crate on 2026-09-12, with that line the only difference
        // between green and red. The longest literal Rust has is
        // `'\u{10FFFF}'`, at eleven characters; a `'` that closes none of
        // them is a lifetime and stays where it is.
        if here == '\''
            && let Some(past) = char_literal_at(&source, at)
        {
            for _ in 0..past {
                out.push(source[at]);
                at += 1;
            }
            continue;
        }
        if here == '"' {
            out.push(here);
            at += 1;
            while at < source.len() {
                if source[at] == '\\' {
                    out.push(source[at]);
                    if let Some(after) = source.get(at + 1) {
                        out.push(*after);
                    }
                    at += 2;
                    continue;
                }
                let done = source[at] == '"';
                out.push(source[at]);
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

#[cfg(test)]
/// The name of the method a declaration declares, if it declares one.
///
/// Every qualifier a declaration may carry before `fn`, stripped in a loop,
/// for the reason the walk below strips them: matching two spellings read
/// twelve of twenty.
fn the_method_declared_by(declaration: &str) -> Option<String> {
    let mut bare = declaration;
    loop {
        let next = bare
            .strip_prefix("pub ")
            .or_else(|| bare.strip_prefix("async "))
            .or_else(|| bare.strip_prefix("unsafe "))
            .or_else(|| bare.strip_prefix("const "))
            .or_else(|| {
                bare.strip_prefix("pub(")
                    .and_then(|rest| rest.split_once(american_paren_space()))
                    .map(|(_, rest)| rest)
            });
        match next {
            Some(rest) => bare = rest,
            None => break,
        }
    }
    bare.strip_prefix("fn ").map(|rest| {
        rest.chars()
            .take_while(|character| character.is_alphanumeric() || *character == '_')
            .collect()
    })
}

#[cfg(test)]
/// `") "`, written where it can be seen.
const fn american_paren_space() -> &'static str {
    ") "
}

/// Whether this line opens more brackets than it closes.
///
/// An attribute that wraps leaves its first line unbalanced; one written whole
/// on a line does not. That is what tells a wrapped attribute's opening from a
/// line that merely begins `#[`. The same reader in `xtask/src/gates.rs` says
/// the same sentence — two copies, and the day they disagree is the day one of
/// them is the narrower — which is why `every_copy_of_the_attribute_walk_reads_
/// the_same` in `xtask/src/check.rs` now compares all three bodies, this one
/// included.
///
/// Behind `#[cfg(test)]` because its only caller is, and this crate takes no
/// dead code.
#[cfg(test)]
fn more_opens_than_closes(line: &str) -> bool {
    let mut depth = 0_i32;
    for letter in line.chars() {
        match letter {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
    }
    depth > 0
}

/// The line a doc block starts at, walking up past an item's attributes.
///
/// **A wrapped attribute is still one attribute.** Both readers below stepped
/// over a line beginning `#[` and stopped at anything else — so when rustfmt
/// breaks a long attribute across lines, as it does and as this tree's own style
/// produces, the continuation line stops the walk and the doc block above it is
/// never seen. Measured by a reviewer on 2026-09-13: a `#[doc(alias = …)]` in
/// the house layout above `pub async fn begin` made two tests fail with messages
/// that are false — one says the function has no `# Errors` section, and it has
/// one, untouched. Named by four reviewers over five rounds before it was
/// planted.
///
/// The last line of a wrapped attribute closes with `]`, and its first begins
/// `#[`, so a bounded walk back to that first line is what joins the two.
#[cfg(test)]
fn past_the_attributes(lines: &[&str], mut at: usize) -> usize {
    // Long enough for anything rustfmt writes and short enough that a stray `]`
    // — an array closing above an item — cannot walk the reader out of the
    // block it is reading.
    const WRAPPED_AT_MOST: usize = 12;
    while at > 0 {
        let line = lines[at - 1].trim_end();
        if line.trim_start().starts_with("#[") {
            at -= 1;
            continue;
        }
        // **The second copy of this reader, and it was the older one.** Wave 24
        // repaired the copy in `xtask/src/gates.rs` — a comment ending in `]`
        // is not a wrapped attribute's last line, and a wrapped attribute's
        // first line is the one that leaves brackets open — and did not come
        // here. An intra-doc link ends a doc line in `]`, so this copy walked
        // out of the item and `the_sections_and_what_they_miss` then read every
        // `///` line above it as the item's documentation: the first `# Errors`
        // heading anywhere in the file became the item's section, and the rule
        // accepted silently. Latent rather than live — measured over all 175
        // declarations in this crate, the two readers agree today — and latent
        // is how it would stay until the first item carries such a line. One
        // rule written twice with the second copy the narrower, found in wave
        // 25, in the wave after the one that wrote the repair.
        let is_a_comment = |text: &str| text.trim_start().starts_with("//");
        if line.ends_with(']') && !is_a_comment(line) {
            let mut back = at - 1;
            let mut spent = 0;
            while back > 0 && spent < WRAPPED_AT_MOST {
                if lines[back].trim_start().starts_with("#[") {
                    break;
                }
                back -= 1;
                spent += 1;
            }
            let opens_unclosed = lines.get(back).is_some_and(|first| {
                first.trim_start().starts_with("#[") && more_opens_than_closes(first)
            });
            if opens_unclosed && !is_a_comment(lines[back]) {
                at = back;
                continue;
            }
        }
        break;
    }
    at
}

/// The two fields `Journal::bracket` supplies that no caller can.
///
/// Its own module because what it asks about is the *record a bracket becomes*
/// rather than what a store does with one, and because the two fields were
/// asserted by nothing while the function's own doc named them as the reason it
/// exists.
#[cfg(test)]
mod a_bracket_carries_what_no_caller_supplies {
    /// The clock's reader, named once for the two rules that read this file.
    ///
    /// **A needle written twice is counted twice.** These tests live in the file
    /// they read, so every spelling of the name here is an occurrence of it —
    /// which is the vacuous-assertion shape, one line from the rule holding it.
    /// The census below counts the identifier, so the two literals that used to
    /// be assembled inline were two of the four it expected; splitting the
    /// shape rule out of that test would have made a third. One `const` is the
    /// whole of it, and the count it is held against is three: this line, the
    /// declaration, and the one call.
    const THE_CLOCK: &str = "now_ms";

    /// A wrapped attribute is one attribute, and a stray `]` is not one at all.
    ///
    /// **The repair had no case, in three copies.** It was written after a plant
    /// in `nc-journal` on 2026-09-13 and neutered in all three a day later with
    /// every suite green — `nc-journal` has no multi-line attribute in it, and
    /// neither does the tree this copy reads, so the branch decided nothing
    /// anywhere. The bound is what stops a stray `]` above an item walking the
    /// reader out of the block it is reading, and it is written down here too.
    #[test]
    fn a_doc_block_is_found_past_a_wrapped_attribute() {
        fn lines(text: &str) -> Vec<&str> {
            text.lines().collect()
        }
        // **Assembled, because this file is walked.** A literal `#[` inside a
        // string is an attribute to the gate that reads this tree for a blanket
        // allow, and one that never closes stops it reading the file.
        let opener = ["#", "[doc("].join("");
        let text = format!(
            "/// The summary.\n///\n/// And the rest of it.\n{opener}\n    alias = \"one\",\n    alias = \"two\"\n)]\nfn item() {{}}"
        );
        let wrapped = lines(&text);
        assert_eq!(
            super::past_the_attributes(&wrapped, wrapped.len() - 1),
            3,
            "the walk stopped at the continuation of a wrapped attribute rather than at \
             its first line"
        );
        let one = format!(
            "/// The summary.\n{}\nfn item() {{}}",
            ["#", "[inline]"].join("")
        );
        let plain = lines(&one);
        assert_eq!(super::past_the_attributes(&plain, plain.len() - 1), 1);
        // A closing bracket that belongs to an array, not to an attribute: the
        // walk stops rather than climbing out of the item it is reading.
        let array = lines("const A: [u8; 1] = [\n    1,\n];\nfn item() {}");
        assert_eq!(
            super::past_the_attributes(&array, array.len() - 1),
            array.len() - 1
        );
        // **The case the copy in `xtask/src/gates.rs` was repaired for**, and
        // which this copy answered the other way for a whole wave: an intra-doc
        // link ends a doc line in `]`, and reading it as a wrapped attribute's
        // last line walks the reader out of the item. Found in wave 25.
        let held = format!(
            "{}\nfn other() {{}}\n\n/// See [`other`]\nfn item() {{}}",
            ["#", "[inline]"].join("")
        );
        let linked = lines(&held);
        assert_eq!(
            super::past_the_attributes(&linked, linked.len() - 1),
            linked.len() - 1,
            "a doc line ending in an intra-doc link walked the reader out of the block \
             it is reading"
        );
        // And the direction the first repair got wrong: a wrapped attribute
        // list may hold a blank line, and it must not cost the item its doc.
        let spaced =
            lines("/// Doc.\n#[doc(\n    alias = \"a\",\n\n    alias = \"b\"\n)]\nfn item() {}");
        assert_eq!(
            super::past_the_attributes(&spaced, spaced.len() - 1),
            1,
            "a blank line inside a wrapped attribute list cost the item its doc block"
        );
    }
    /// Every function in one file that reaches `Journal::bracket`, in order.
    ///
    /// **A total over two files is a census of neither.** The reach used to be
    /// held by one number — the calls to `bracket` in `lib.rs` and in `unit.rs`
    /// added together — and a sum does not say where its parts are. A caller
    /// that moved from one file to the other kept it at three; so did a real
    /// caller added in `lib.rs` while the one call there was deleted, and that
    /// exchange is the one that matters, because a test's call is not a way
    /// in. What the reach is made of is *which function* holds each call, so
    /// that is what is compared.
    ///
    /// The enclosing name is the last `fn` declared above the call, where a
    /// declaration is a line whose words before it are visibility and
    /// qualifiers only — so a sentence or a comment that mentions one cannot
    /// become the answer.
    fn calls_to_bracket_in(text: &str) -> Vec<String> {
        // **Comments off, and `the_method_declared_by` rather than a fourth
        // copy of it.** This read the raw text, so a `//` line naming the call
        // paid for one that had gone — the mechanism round 53 closed in three
        // other files and wrote again here — and its own qualifier check was a
        // fourth spelling of the loop two screens down, reached by nothing.
        // Both measured by reviewers on 2026-09-12.
        let code = super::the_code_without_comments(text);
        // Assembled, because one of the two files read is this one.
        let needle = format!(".{}(", "bracket");
        // **And the qualified spelling of the same call.** `bracket` is
        // `pub(crate)`, so `Journal::bracket(self.journal, …)` is the ordinary
        // UFCS form of the line above it: legal anywhere in the crate,
        // `cargo fmt`-clean and invisible to a needle that begins with a dot.
        // Measured by a reviewer on 2026-09-11, in both directions at once: a
        // *new* reach to the clock written that way left this census at
        // `["open", "close"]` while the suite went on requiring `record`'s
        // documentation to stay silent about `ClockBeforeEpoch`; and the live
        // call rewritten that way — the same call, the same reach — turned the
        // census red with a message saying a reach had moved.
        let qualified = format!("::{}(", "bracket");
        let declaration = format!("fn {}(", "bracket");
        let mut current = String::new();
        let mut found = Vec::new();
        for line in code.lines() {
            if let Some(name) = super::the_method_declared_by(line.trim_start()) {
                current = name;
            }
            if line.contains(&needle) || (line.contains(&qualified) && !line.contains(&declaration))
            {
                found.push(current.clone());
            }
        }
        found
    }

    /// The `# Errors` section of one item's own doc block.
    ///
    /// **This item's own block, and the first heading in it.** The anchor was
    /// `rfind("# Errors")` — the *last* match above the function — so one
    /// ordinary sentence containing those eight characters moved it past the
    /// claim, and a section naming a refusal it cannot build passed. The block
    /// is the contiguous run of `///` lines immediately above the item; inside
    /// it, the section runs from the heading to the end.
    ///
    /// **The list for what a section promises, the whole section for what it
    /// must not say.** The first paragraph under the heading is the claim; the
    /// prose below it is why the claim says what it says. But a reviewer
    /// pointed out that reading only the list makes the negative direction one
    /// paragraph wide — a second paragraph beginning *"Also [`Error::…`]"* is a
    /// claim too, and passed. So the two directions read two slices, and the
    /// paragraph that explains a removal names the variant by description
    /// rather than by spelling.
    fn section_of(text: &str, name: &str, whole: bool) -> String {
        let at = text
            .find(&format!("pub async fn {name}("))
            .unwrap_or_else(|| panic!("`{name}` is here"));
        let above = &text[..at];
        // The slice ends mid-line, at the item's own indentation, so the last
        // line it yields is that indentation and the item begins after it.
        let lines: Vec<&str> = above.lines().collect();
        let mut item = lines.len();
        while item > 0 && lines[item - 1].trim().is_empty() {
            item -= 1;
        }
        // Past the attributes first — including a wrapped one, whose
        // continuation lines begin with neither `///` nor `#[`.
        let mut opens = super::past_the_attributes(&lines, item);
        while opens > 0 && lines[opens - 1].trim_start().starts_with("///") {
            opens -= 1;
        }
        let block: Vec<&str> = lines[opens..item]
            .iter()
            .filter(|line| line.trim_start().starts_with("///"))
            .copied()
            .collect();
        let block = block.join("\n");
        let opens = block
            .find("# Errors")
            .unwrap_or_else(|| panic!("`{name}` has an errors section"));
        let section = &block[opens..];
        if whole {
            return section.to_owned();
        }
        section.split("///\n").nth(1).unwrap_or_default().to_owned()
    }

    /// The sibling methods a section points at instead of repeating them.
    ///
    /// A section that says *"As [`WorkUnit::commit`]."* has stated its list.
    /// Requiring every section to spell every variant would refuse that, which
    /// is how a rule written to close one hole starts refusing something
    /// legitimate instead. So a deferral is read as what it is — the items it
    /// names — and each of those is asked the same question.
    fn pointed_at(section: &str) -> Vec<String> {
        let opening = format!("[`{}::", "WorkUnit");
        section
            .match_indices(&opening)
            .filter_map(|(at, _)| {
                section[at + opening.len()..]
                    .split_once("`]")
                    .map(|(name, _)| name.to_owned())
            })
            .collect()
    }

    /// A source with the contents of every string literal spent.
    ///
    /// **Raw strings are not gone before this runs, and the doc here said they
    /// were.** `the_code_without_comments` is a *comment* stripper: its
    /// raw-string branch copies the body through character for character, so it
    /// can tell a `//` inside `r"…"` from a comment without destroying the
    /// source. A rule that knew only `"` and `\` then met `r"C:\"` — an
    /// ordinary Windows path — and ate its own closing quote, blanking the code
    /// after it; and a raw string holding a `"` leaked its body out as code.
    /// Both directions, measured by a reviewer in round seventy-one wave 10.
    ///
    /// So both shapes are spent here. Spending rather than cutting keeps every
    /// offset where it was, which is what the `?` rule beside it reads.
    fn without_literals(code: &str) -> String {
        let letters: Vec<char> = code.chars().collect();
        let mut out = String::with_capacity(code.len());
        let mut at = 0_usize;
        let spend = |out: &mut String, letter: char| {
            out.push(if letter == '\n' { '\n' } else { ' ' });
        };
        while at < letters.len() {
            // `r"…"`, `r#"…"#`, and the byte-string spellings of each. The hash
            // count is the terminator, which is the whole reason raw strings
            // exist and the reason a quote-counting rule cannot read one.
            let raw = {
                let mut from = at;
                if letters.get(from) == Some(&'b') {
                    from += 1;
                }
                if letters.get(from) == Some(&'r') {
                    from += 1;
                    let mut hashes = 0;
                    while letters.get(from + hashes) == Some(&'#') {
                        hashes += 1;
                    }
                    if letters.get(from + hashes) == Some(&'"') {
                        Some((from + hashes + 1, hashes))
                    } else {
                        None
                    }
                } else {
                    None
                }
            };
            if let Some((body, hashes)) = raw {
                for letter in &letters[at..body] {
                    out.push(*letter);
                }
                at = body;
                while let Some(letter) = letters.get(at) {
                    if *letter == '"'
                        && (1..=hashes).all(|step| letters.get(at + step) == Some(&'#'))
                    {
                        for letter in &letters[at..=at + hashes] {
                            out.push(*letter);
                        }
                        at += hashes + 1;
                        break;
                    }
                    spend(&mut out, *letter);
                    at += 1;
                }
                continue;
            }
            // **A character literal is not a quote.** `'"'` is one `"` that
            // opens nothing, and this reader opened a string on it — so
            // everything to the next quote was blanked (a call in that span
            // invisible) and the literal after it came out as code (a finding
            // about nothing). Its sibling three hundred lines up carries a
            // paragraph about exactly this and the helper for it; one rule
            // written twice, and this was the narrower copy. Named by a
            // reviewer in wave 11.
            if letters[at] == '\''
                && let Some(width) = super::char_literal_at(&letters, at)
            {
                {
                    for letter in &letters[at..at + width] {
                        out.push(*letter);
                    }
                    at += width;
                    continue;
                }
            }
            if letters[at] == '"' {
                out.push('"');
                at += 1;
                while let Some(letter) = letters.get(at) {
                    if *letter == '\\' {
                        spend(&mut out, *letter);
                        at += 1;
                        if let Some(escaped) = letters.get(at) {
                            spend(&mut out, *escaped);
                            at += 1;
                        }
                        continue;
                    }
                    if *letter == '"' {
                        out.push('"');
                        at += 1;
                        break;
                    }
                    spend(&mut out, *letter);
                    at += 1;
                }
                continue;
            }
            out.push(letters[at]);
            at += 1;
        }
        out
    }

    /// Every string literal is spent, in each of the shapes this crate writes.
    ///
    /// The two the repaired reader was written against are the last two rows: a
    /// raw string holding a `"` used to leak its body out as code, and one
    /// ending in a backslash used to eat its closing quote and blank the code
    /// after it.
    #[test]
    fn a_literal_is_spent_whatever_shape_it_is_written_in() {
        for (written, left) in [
            ("let it = \"tick()\";", "let it = \"      \";"),
            ("let it = \"a\\\"b\"; ok()", "let it = \"    \"; ok()"),
            ("let it = r\"tick()\"; ok()", "let it = r\"      \"; ok()"),
            (
                "let it = r#\"a \" b\"#; ok()",
                "let it = r#\"     \"#; ok()",
            ),
            ("let it = r\"C:\\\"; ok()", "let it = r\"   \"; ok()"),
            // **A character literal, which opens nothing.** Its `"` used to
            // open a string here: everything to the next quote was blanked and
            // the literal after it came out as code.
            (
                "let it = '\"'; let s = \"tick()\"; ok()",
                "let it = '\"'; let s = \"      \"; ok()",
            ),
        ] {
            assert_eq!(without_literals(written), left, "{written}");
        }
    }

    /// Every call to the clock carries its refusal.
    ///
    /// **Split out of the census beside it, which counts and cannot see a
    /// shape.** That rule counts the identifier, so an *added* reader moves a
    /// number and a *substituted* one does not: `now_ms()?` rewritten to
    /// `now_ms().unwrap_or_default()` right there in `bracket` leaves
    /// `asked_in` at its expected pair and every target of this crate green.
    /// Measured by a reviewer in round seventy-one, with the outcome
    /// `the_instant`'s own doc argues against at length: on a machine whose
    /// clock is before 1970 every bracket is stamped `0`, the stamp is inside
    /// the digest, and the table refuses `UPDATE`.
    ///
    /// What caught it instead was `clippy::unnecessary_wraps` — `now_ms()?` is
    /// `bracket`'s only error path, so removing it makes the `Result`
    /// unnecessary and `check`'s `-D warnings` is red. That is a lint about a
    /// signature, not a rule that knows what the signature is holding, and it
    /// disappears the moment `bracket` grows a second error path. The runner
    /// does not see it either: `mutants.py`'s command for the `journal` key
    /// runs no clippy at all.
    ///
    /// So the call is asked for its shape. `?` is the whole rule: it is the one
    /// spelling that cannot swallow the refusal.
    #[test]
    fn every_call_to_the_clock_carries_its_refusal() {
        let mut loose: Vec<String> = Vec::new();
        let mut shaped = 0_usize;
        for entry in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src"))
            .expect("the crate's sources are readable")
            .flatten()
        {
            let path = entry.path();
            if path.extension().is_none_or(|kind| kind != "rs") {
                continue;
            }
            let named = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_owned();
            let text = std::fs::read_to_string(&path).expect("a source file is readable");
            // **And its literals, which that reader copies through.** `xtask`'s
            // `blanked` spends the characters inside a `"…"`; the reader this
            // crate owns removes comments, nesting block comments and raw-string
            // bodies and leaves an ordinary literal whole — one rule written
            // twice, and the narrower copy is the one here. Measured in round
            // seventy-one wave 9: a `const` whose sentence contained the words
            // `now_ms() and a clock before 1970` was demanded to carry a `?`,
            // and `shaped > 0` — the only statement that this rule inspected a
            // call at all — is a floor a string literal can pay.
            let code = without_literals(&super::the_code_without_comments(&text));
            let call = format!("{THE_CLOCK}()");
            let call = call.as_str();
            let mut from = 0_usize;
            while let Some(found) = code[from..].find(call) {
                let at = from + found;
                from = at + call.len();
                // The declaration is not a call, and it is the one place the
                // name is followed by its return type.
                if code[..at].trim_end().ends_with("fn") {
                    continue;
                }
                shaped += 1;
                if !code[from..].starts_with('?') {
                    let rest: String = code[from..].chars().take(24).collect();
                    loose.push(format!("{named}: `{call}{rest}`"));
                }
            }
        }
        assert!(
            shaped > 0,
            "no call to the clock was read, so this rule inspected nothing"
        );
        assert!(
            loose.is_empty(),
            "the clock is reached by a call that does not carry its refusal: {loose:?}. \
             The reader named above returns a `Result` because a clock before 1970 is \
             refused rather than rounded, and every spelling but `?` is a way of \
             rounding it"
        );
    }

    /// The reach to the clock is the one the sections are read for.
    ///
    /// `ClockBeforeEpoch` has exactly one construction site, so the reach is
    /// three hops and a test can hold all three: the clock is read in one
    /// place, asked in one place, and entered through `Journal::bracket`. Each
    /// of those is counted in the file it is in, and the callers are named
    /// rather than totalled, so the list of sections the test below reads is
    /// the list of ways in.
    #[test]
    fn the_reach_to_the_clock_is_the_one_the_sections_are_read_for() {
        let here = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"))
            .expect("this file is readable");

        // The needles are assembled, because this test lives in one of the
        // files it reads and a spelled-out literal would count itself. That is
        // the vacuous-assertion shape, one line from the rule it is here to
        // hold.
        // **The code, not the file.** Round 54 comment-stripped
        // `calls_to_bracket_in` eight lines below and left these two counting
        // over raw text — so one ordinary `///` line saying the instant comes
        // from `now_ms()?` turned the second red with a sentence that is false,
        // and a second *live* reader spelled `now_ms().unwrap_or_default()`
        // left both green. Too strong and too weak in the same pair, measured
        // on 2026-09-12. One reader, and it is the one beside them.
        let code = super::the_code_without_comments(&here);
        let clock = format!("fn {THE_CLOCK}()");
        // **The name, not the call.** This counted `now_ms(`, so
        // `use crate::now_ms as clock;` and `clock()` — the same reader under
        // another name, in the same method the census was widened for — walked
        // past it with the whole crate green. Measured by a reviewer on
        // 2026-09-13. An alias has to name the item it aliases, so counting the
        // identifier catches the `use` line as well as every call.
        let asked = THE_CLOCK.to_owned();
        assert_eq!(
            code.matches(&clock).count(),
            1,
            "the clock has more than one reader now, so the reach below is not the reach"
        );
        // **Every way of asking, not the one spelling.** This counted
        // `now_ms()?`, so `now_ms().unwrap_or_default()` — a second live reader
        // that swallows the refusal this whole test is about — added nothing.
        // What is counted is the call.
        //
        // **And every file of this crate, not this one.** `now_ms` is private
        // to the crate root, so `crate::now_ms()` is legal in all five modules
        // beneath it, and a count over `lib.rs` alone is a census of one of
        // them. Measured by a reviewer on 2026-09-12: one
        // `crate::now_ms().unwrap_or_default()` in `WorkUnit::record` — the
        // method whose section this same suite requires to stay silent about
        // the clock — left every target of this crate green and `check passed`,
        // and the same line in `lib.rs` turned this red. The numbers are per
        // file now, and a file that asks nothing is not in the list.
        let mut asked_in: Vec<(String, usize)> = Vec::new();
        let mut sources = 0_usize;
        for entry in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src"))
            .expect("the crate's sources are readable")
            .flatten()
        {
            let path = entry.path();
            if path.extension().is_none_or(|kind| kind != "rs") {
                continue;
            }
            sources += 1;
            let text = std::fs::read_to_string(&path).expect("a source file is readable");
            let times = super::the_code_without_comments(&text)
                .matches(&asked)
                .count();
            if times > 0 {
                let named = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_owned();
                asked_in.push((named, times));
            }
        }
        asked_in.sort();
        assert!(
            sources >= 5,
            "the walk read {sources} source file(s), which is not this crate"
        );
        assert_eq!(
            asked_in,
            // **Three, and it was four.** Two of the four were this test's own
            // needles, assembled inline; they are one `const` now — see
            // `THE_CLOCK` — so what is left is that line, the declaration and
            // the one call. The count fell because a spelling of the name went
            // away, which is the one direction this assertion cannot tell from
            // a reader going away: the three are named in the paragraph above
            // so a reader can see which is which.
            vec![("lib.rs".to_owned(), 3)],
            "the clock is asked somewhere other than `bracket`, or its own \
             declaration is gone"
        );
        // **Every file of this crate, not the two that hold a call today.** The
        // clock's own readers are counted across all five above; the callers of
        // `bracket` were read in `lib.rs` and `unit.rs` alone, so a third module
        // reaching it was a census of two files rather than of the crate.
        // Named by a reviewer on 2026-09-13.
        let mut reaching: Vec<(String, Vec<String>)> = Vec::new();
        for entry in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src"))
            .expect("the crate's sources are readable")
            .flatten()
        {
            let path = entry.path();
            if path.extension().is_none_or(|kind| kind != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("a source file is readable");
            let callers = calls_to_bracket_in(&text);
            if !callers.is_empty() {
                let named = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_owned();
                reaching.push((named, callers));
            }
        }
        reaching.sort();
        assert_eq!(
            reaching,
            vec![(
                "lib.rs".to_owned(),
                vec![
                    "open_unit".to_owned(),
                    "close_unit".to_owned(),
                    "a_bracket_names_the_keeper_and_the_instant".to_owned(),
                ]
            )],
            "the bracket is reached from somewhere the sections read below do not account \
             for: `open_unit` is the way in for itself and for `begin`, `close_unit` for \
             itself and for `commit` and `abort` in `unit.rs`, and the third caller is a \
             test"
        );
    }

    /// The sections that name the clock's refusal are the ones that can reach
    /// it.
    ///
    /// **The other half of `every_errors_section_names_what_its_body_builds`,
    /// for the one variant that can be traced by hand.** That gate reads bodies
    /// for variants a section does not name; it does not read sections for
    /// variants no body builds, and its own doc says so. Measured on
    /// 2026-09-11: `Journal::record`'s section named `Error::ClockBeforeEpoch`
    /// and `record` cannot build it — it takes its instant from the caller's
    /// `Record` — while `begin`, `commit` and `abort`, which reach the clock
    /// through `Journal::bracket`, named it nowhere. A caller of the bracket read
    /// "whatever the store returned" and treated a wrong machine clock as
    /// retryable, which it is not.
    ///
    /// The general rule — a section naming a variant no body in its call graph
    /// builds — is still unheld, and saying so here is the point.
    #[test]
    fn the_clocks_refusal_is_named_where_it_can_happen() {
        let here = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"))
            .expect("this file is readable");
        let unit = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/unit.rs"))
            .expect("the unit is readable");

        for (whose, text) in [("the journal", &here), ("the unit", &unit)] {
            assert!(
                !section_of(text, "record", true).contains("ClockBeforeEpoch"),
                "`record` in {whose} names a refusal it cannot build: it takes its instant \
                 from the caller's record, and appending one never asks the clock"
            );
        }
        for (text, name) in [
            (&here, "begin"),
            (&here, "open_unit"),
            (&here, "close_unit"),
            (&unit, "commit"),
        ] {
            assert!(
                section_of(text, name, false).contains("ClockBeforeEpoch"),
                "`{name}` reaches the clock through `bracket` and does not say so, so a \
                 caller reads a refusal that will not clear on its own as the store's \
                 fault"
            );
        }

        // **The block reader reaches its item past an attribute**, which is
        // the clause three of this crate's `#[doc(hidden)]` sections need and
        // which no case in the tree reached: deleting it left the suite green.
        // Driven over text, because the items this file happens to carry are
        // not a corpus.
        let synthetic = concat!(
            "/// Does a thing.\n",
            "///\n",
            "/// # Errors\n",
            "///\n",
            "/// Returns [`Error::ClockBeforeEpoch`] when the clock is behind.\n",
            "#[inline]\n",
            "pub async fn probe() {}\n",
        );
        assert!(
            section_of(synthetic, "probe", true).contains("ClockBeforeEpoch"),
            "an attribute between a doc block and its item ended the block, and a \
             section above one is then read as empty"
        );

        // **And the one that says it by pointing at the one that says it.**
        // `abort` closes through the same `close`, so it refuses for the same
        // reason, and its whole section is *"As [`WorkUnit::commit`]."* —
        // which is how this tree writes a shared list, and is not a hole.
        let aborts = section_of(&unit, "abort", true);
        let deferred = pointed_at(&aborts);
        assert!(
            aborts.contains("ClockBeforeEpoch")
                || (!deferred.is_empty()
                    && deferred
                        .iter()
                        .all(|item| section_of(&unit, item, true).contains("ClockBeforeEpoch"))),
            "`abort` closes through the same `close` and can refuse for the same reason, \
             and its section neither names `ClockBeforeEpoch` nor points at a sibling \
             whose own section does"
        );
    }

    use nc_ids::OpaqueId;

    /// A clock before the epoch is refused rather than rounded.
    ///
    /// **The premise is the test's, not the machine's.** The assertion that used
    /// to stand for this — `recorded_at > 1_700_000_000_000` against the real
    /// system clock — closes two of the three plants its doc claims and cannot
    /// close the third, because the third's premise is a clock set before 1970
    /// and no test can establish that by reading the clock it is testing. A
    /// reviewer measured it in round seventy-one: `unwrap_or_default()` in place
    /// of the refusal left `cargo xtask check` printing `check passed`.
    ///
    /// An entry claiming 1970 is an entry that verifies: the timestamp is inside
    /// the digest, and a journal entry is the one record here that cannot be
    /// corrected afterwards.
    #[test]
    fn a_clock_before_the_epoch_is_refused_rather_than_rounded() {
        let before = std::time::UNIX_EPOCH - std::time::Duration::from_secs(1);
        assert!(
            matches!(
                super::the_instant(before),
                Err(super::Error::ClockBeforeEpoch)
            ),
            "a clock one second before 1970 was not refused, so an entry claiming the \
             epoch is an entry that verifies"
        );
        assert_eq!(
            super::the_instant(std::time::UNIX_EPOCH + std::time::Duration::from_millis(7)),
            Ok(7),
            "an instant the clock can read is the millisecond it names"
        );
    }

    /// A unit minted for the store comes from the journal's own minter.
    ///
    /// So no unit the journal opens is ever the same identifier, and the realm
    /// read back is the one given.
    #[tokio::test]
    async fn a_unit_minted_for_the_store_is_never_one_the_journal_opens() {
        use crate::Store as _;

        let realm = OpaqueId::new("realm-1").expect("a test realm");
        let flake = nc_ids::MachineFlake::candidate(b"a test device", 0);
        let mut journal = crate::Journal::new(
            crate::InMemory::default(),
            realm.clone(),
            OpaqueId::new("did:key:zKernel").expect("a test keeper"),
            nc_ids::Minter::new(flake, nc_ids::SystemClock),
        );
        assert_eq!(journal.realm(), &realm);
        let minted = journal.mint_unit().expect("the clock yields one");
        let opened = journal
            .open_unit("a step")
            .await
            .expect("the store takes it");
        assert_ne!(
            minted, opened,
            "two units of one journal share an identifier"
        );
        assert!(
            !journal
                .store()
                .unit_opened(&realm, minted)
                .await
                .expect("the store answers"),
            "minting a unit wrote its opening bracket"
        );
    }

    /// A bracket names the keeper and carries the instant it was written.
    ///
    /// **`Journal::bracket`'s doc says these are the two things it exists to
    /// supply, and both were asserted by nothing.** `keeper` has exactly one
    /// reader in this crate — that call — and three plants left the whole gate
    /// green: every bracket naming the realm instead of the kernel, every
    /// bracket stamped `0`, and `now_ms` rounding a failure to `0` instead of
    /// refusing.
    ///
    /// `recorded_at` is inside the digest and the table refuses `UPDATE`, so an
    /// entry stamped `0` is an entry that **verifies** and cannot be corrected —
    /// which `now_ms`'s own doc argues at length with nothing behind it.
    #[tokio::test]
    async fn a_bracket_names_the_keeper_and_the_instant() {
        /// Not a fixed instant — a bound, because the clock is the real one.
        ///
        /// Any stamp before this repository existed is one nothing could have
        /// written, and `0` is the value the rounding plant produced.
        const THIS_REPOSITORY_EXISTED_BY: u64 = 1_700_000_000_000;

        let realm = OpaqueId::new("realm-1").expect("a test realm");
        let keeper = OpaqueId::new("did:key:zKernel").expect("a test keeper");
        let flake = nc_ids::MachineFlake::candidate(b"a test device", 0);
        let journal = crate::Journal::new(
            crate::InMemory::default(),
            realm.clone(),
            keeper.clone(),
            nc_ids::Minter::new(flake, nc_ids::SystemClock),
        );
        let unit = nc_ids::Snowflake::from_crockford("01J8XK2M9P58").expect("a test unit");
        let record = journal
            .bracket(
                crate::unit::OPENS,
                unit,
                nc_canon::Value::text("a migration"),
                crate::Outcome::UnitBegun,
            )
            .expect("the clock is after the epoch");

        assert_eq!(
            record.principal, keeper,
            "a bracket names the realm instead of the kernel that opened it, so the \
             audit says a tenancy did what a keeper did"
        );
        assert_ne!(
            record.principal, realm,
            "the keeper and the realm are the same identifier here, so this test cannot \
             tell them apart"
        );
        assert!(
            record.recorded_at > THIS_REPOSITORY_EXISTED_BY,
            "a bracket is stamped {}, which is before this repository existed — and \
             `recorded_at` is inside the digest and the table refuses `UPDATE`, so such \
             an entry verifies and cannot be corrected",
            record.recorded_at
        );
        assert_eq!(record.work_unit, Some(unit));
    }
}
