//! The journal's store on the person's machine: `nc_local::LocalJournal`,
//! the hash-chained history in the same file as the plugin store
//! (`local-migrations/0004_local_journal.sql`).
//!
//! **What a test asserts is what the file holds.** The table's own triggers
//! refuse an edit and a deletion whoever sends it; a link that does not follow
//! the tail lands nothing; a write a trigger silences is refused rather than
//! believed; a row written around the store is named where it lies and hides
//! no unit opened after it; and every call answers as the in-memory store —
//! the trait's specification — and PostgreSQL's do, refusal for refusal, over
//! written scripts and over generated ones.
//!
//! The PostgreSQL leg needs `DATABASE_URL`, which `cargo xtask dev` prints;
//! `cargo xtask check` says when no database runs, and the leg stands aside
//! saying so.

use std::path::{Path, PathBuf};

use nc_canon::Value;
use nc_catalog::ErrorCode;
use nc_ids::{OpaqueId, Snowflake};
use nc_journal::{Entry, Error, InMemory, Outcome, Record, Store};
use nc_local::{LocalJournal, LocalStore};
use proptest::prelude::*;
use sqlx::{ConnectOptions as _, Connection as _};

/// A scratch directory for one test, removed however the test leaves.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Scratch {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("nc-local-journal-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir(&root).expect("a scratch directory");
        Self(root)
    }

    /// Where the store's directory goes: one level down, so the store is the
    /// one to create it.
    fn profile(&self) -> PathBuf {
        self.0.join("profile")
    }
}

/// A connection to the file the way somebody with write access to it has one.
async fn raw(profile: &Path) -> sqlx::SqliteConnection {
    sqlx::sqlite::SqliteConnectOptions::new()
        .filename(profile.join(nc_local::FILE_NAME))
        .create_if_missing(false)
        .connect()
        .await
        .expect("the file opens")
}

fn id(text: &str) -> OpaqueId {
    OpaqueId::new(text).expect("a test identifier")
}

/// The work units the scripts name, by index.
const UNITS: [&str; 4] = [
    "01J8XK2M9P51",
    "01J8XK2M9P52",
    "01J8XK2M9P53",
    "01J8XK2M9P54",
];

fn unit(index: usize) -> Snowflake {
    Snowflake::from_crockford(UNITS[index]).expect("a test unit")
}

/// The realms the scripts write.
const REALMS: [&str; 2] = ["realm-1", "realm-2"];

/// A record with every field the chain hashes set — or left empty, every
/// other step — so a column written from the wrong field, or read into one,
/// cannot pass by both being empty.
fn a_record(
    step: usize,
    realm: &str,
    action: &str,
    result: Outcome,
    unit: Option<Snowflake>,
) -> Record {
    let mut record = Record::new(
        id("principal-1"),
        action,
        Value::object([
            ("kind", Value::text("content")),
            ("id", Value::text(format!("c-{step}"))),
            (
                "nested",
                Value::Array(vec![Value::Integer(-1), Value::Bool(true), Value::Null]),
            ),
        ]),
        result,
        1_767_225_600_000 + u64::try_from(step).expect("a short script"),
    );
    record.realm = id(realm);
    record.work_unit = unit;
    if step.is_multiple_of(2) {
        record.actor_chain = vec![id("agent-1"), id("principal-1")];
        record.deciding_policy = Some("core.destructive_needs_quorum".to_owned());
        record.policy_pack = Some("family@1".to_owned());
        record.policy_epoch = Some(7);
        record.grant_used = Some(id("grant-1"));
    }
    record
}

/// Where a step places its entry.
#[derive(Debug, Clone, Copy)]
enum Place {
    /// After the store's head, as `Journal::append` places one.
    Next,
    /// At this sequence, whatever the head is.
    At(u64),
}

/// What a step's entry says it follows.
#[derive(Debug, Clone, Copy)]
enum Link {
    /// The store's head, as `Entry::chain` links one.
    Head,
    /// A hash no entry has.
    Forged,
    /// Nothing, as only the first entry of a history does.
    Nothing,
}

/// One step of a script: a realm, an action, an outcome, the unit it names
/// by index, where it goes and what it links to.
#[derive(Debug, Clone, Copy)]
struct Step {
    realm: usize,
    action: &'static str,
    result: Outcome,
    unit: Option<usize>,
    place: Place,
    link: Link,
}

/// An ordinary step, chained where `Journal::append` would chain it.
const fn step(realm: usize, action: &'static str, result: Outcome, unit: Option<usize>) -> Step {
    Step {
        realm,
        action,
        result,
        unit,
        place: Place::Next,
        link: Link::Head,
    }
}

/// The steps an opening bracket, a commit and an abort are.
const fn opens(unit: usize) -> Step {
    step(0, nc_journal::unit::OPENS, Outcome::UnitBegun, Some(unit))
}
const fn commits(unit: usize) -> Step {
    step(
        0,
        nc_journal::unit::CLOSES,
        Outcome::UnitCommitted,
        Some(unit),
    )
}
const fn aborts(unit: usize) -> Step {
    step(
        0,
        nc_journal::unit::CLOSES,
        Outcome::UnitAborted,
        Some(unit),
    )
}
const fn in_unit(unit: usize) -> Step {
    step(0, "Write", Outcome::Allowed, Some(unit))
}
const fn elsewhere(step: Step) -> Step {
    Step { realm: 1, ..step }
}
const fn at(place: u64, step: Step) -> Step {
    Step {
        place: Place::At(place),
        ..step
    }
}
const fn linked(link: Link, step: Step) -> Step {
    Step { link, ..step }
}

/// The scripts every store is driven through, written out: each separates
/// one rule a store holds an entry to, and the refusal it answers with.
/// <!-- allow-logging: `the_scripts` hands back script names written as literals in this file, and nothing a caller chose -->
fn the_scripts() -> Vec<(&'static str, Vec<Step>)> {
    vec![
        (
            "a unit opened, written in and committed",
            vec![opens(0), in_unit(0), commits(0)],
        ),
        (
            "an entry after its own unit's commit",
            vec![opens(0), commits(0), in_unit(0)],
        ),
        (
            "an entry after its own unit's abort, at the wrong sequence",
            vec![opens(0), aborts(0), at(9, in_unit(0))],
        ),
        ("a unit opened twice", vec![opens(0), in_unit(0), opens(0)]),
        (
            "a unit opened again after it closed",
            vec![opens(0), commits(0), opens(0)],
        ),
        (
            "a unit opened in two realms, and closed in one",
            vec![
                opens(0),
                elsewhere(opens(0)),
                elsewhere(commits(0)),
                in_unit(0),
            ],
        ),
        (
            "an ordinary action carrying a bracket's outcome",
            vec![
                opens(0),
                step(0, "Write", Outcome::UnitCommitted, Some(0)),
                step(0, "Write", Outcome::UnitBegun, Some(1)),
                in_unit(0),
            ],
        ),
        (
            "a begin and a close that were denied",
            vec![
                step(0, nc_journal::unit::OPENS, Outcome::Denied, Some(0)),
                opens(1),
                step(0, nc_journal::unit::CLOSES, Outcome::Denied, Some(1)),
                in_unit(1),
            ],
        ),
        (
            "a first entry at a sequence that is not zero",
            vec![at(7, in_unit(0))],
        ),
        (
            "an entry past the end of the history",
            vec![opens(0), at(5, in_unit(0))],
        ),
        (
            "an entry where one already is",
            vec![opens(0), in_unit(0), at(1, in_unit(0))],
        ),
        (
            "a sequence no store can hold",
            vec![opens(0), at(u64::MAX, in_unit(0))],
        ),
        (
            "an entry linked to a hash no entry has",
            vec![opens(0), linked(Link::Forged, in_unit(0))],
        ),
        (
            "a first entry linked to something",
            vec![linked(Link::Forged, opens(0))],
        ),
        (
            "an entry linked to nothing past the first",
            vec![opens(0), linked(Link::Nothing, in_unit(0))],
        ),
        (
            "an opening at the wrong sequence of a unit already open",
            vec![opens(0), at(9, opens(0))],
        ),
        (
            "units left open in both realms, among others closed",
            vec![
                opens(0),
                opens(1),
                elsewhere(opens(2)),
                commits(0),
                opens(3),
                aborts(3),
                elsewhere(opens(1)),
            ],
        ),
    ]
}

/// What one store answered to a script, in order: each append's answer, then
/// the history and every question about a unit, in every realm.
async fn transcript_of<S: Store>(store: &mut S, script: &[Step]) -> Vec<String> {
    let mut said = Vec::new();
    for (index, step) in script.iter().enumerate() {
        let head = store.head().await;
        said.push(format!("step {index} head: {head:?}"));
        let head = head.ok().flatten();
        let sequence = match step.place {
            Place::Next => head.map_or(0, |(sequence, _)| sequence + 1),
            Place::At(sequence) => sequence,
        };
        let previous = match step.link {
            Link::Head => head.map(|(_, hash)| hash),
            Link::Forged => Some([0xAA; 32]),
            Link::Nothing => None,
        };
        let record = a_record(
            index,
            REALMS[step.realm],
            step.action,
            step.result,
            step.unit.map(unit),
        );
        let answered = store.append(Entry::chain(sequence, previous, record)).await;
        said.push(format!("step {index} {step:?}: {answered:?}"));
    }
    said.push(format!("history: {:?}", store.from(0).await));
    said.push(format!("tail: {:?}", store.tail().await));
    said.push(format!("head: {:?}", store.head().await));
    for realm in REALMS.map(id) {
        said.push(format!(
            "open in {realm}: {:?}",
            store.open_units(&realm).await
        ));
        for index in 0..UNITS.len() {
            let asked = unit(index);
            said.push(format!(
                "{asked} in {realm}: opened {:?}, closing {:?}",
                store.unit_opened(&realm, asked).await,
                store.unit_closing(&realm, asked).await
            ));
        }
    }
    said
}

/// Holds two transcripts equal line for line, naming the first that differs.
fn held_equal(script: &str, specification: &[String], answered: &[String], store: &str) {
    for (expected, said) in specification.iter().zip(answered) {
        assert_eq!(
            said, expected,
            "`{script}`: the local journal answered otherwise than {store}"
        );
    }
    assert_eq!(
        answered.len(),
        specification.len(),
        "`{script}`: the transcripts are of different lengths"
    );
}

/// A store in memory, over a database in memory, for a script of its own.
async fn a_local_journal() -> (LocalStore, LocalJournal) {
    let store = LocalStore::in_memory_for_tests()
        .await
        .expect("the local database opens in memory");
    let journal = store.journal();
    (store, journal)
}

/// The local journal answers every script as the journal in memory does, and
/// as PostgreSQL's does where a database runs: every append's answer, the
/// refusal named, the history read back field for field, and which units are
/// open in each realm.
///
/// **The in-memory store is the trait's specification**, and PostgreSQL's is
/// the store a server runs; a third store answering one call otherwise is a
/// kernel recovering, refusing or retrying differently over the daemon's
/// history than over the one it was proven against.
#[tokio::test]
async fn the_local_journal_answers_every_script_as_the_other_two_stores_do() {
    for (index, (name, script)) in the_scripts().into_iter().enumerate() {
        let specification = transcript_of(&mut InMemory::default(), &script).await;
        let (store, mut local) = a_local_journal().await;
        let answered = transcript_of(&mut local, &script).await;
        held_equal(name, &specification, &answered, "the journal in memory");
        store.close().await;
        let Some((pool, schema)) = postgres(&format!("script_{index}")).await else {
            continue;
        };
        let mut on_the_server = nc_journal::postgres::Postgres::in_schema(pool.clone(), &schema);
        let answered = transcript_of(&mut on_the_server, &script).await;
        held_equal(name, &specification, &answered, "PostgreSQL's journal");
        finish(pool).await;
    }
}

/// One generated step: a realm, an action, an outcome, a unit or none, a place
/// and a link, each drawn from the few that tell the rules apart.
fn a_step() -> impl Strategy<Value = Step> {
    let actions = prop::sample::select(vec![
        nc_journal::unit::OPENS,
        nc_journal::unit::CLOSES,
        "Write",
        "Read",
    ]);
    let results = prop::sample::select(vec![
        Outcome::UnitBegun,
        Outcome::UnitCommitted,
        Outcome::UnitAborted,
        Outcome::Allowed,
        Outcome::Denied,
    ]);
    let places = prop_oneof![8 => Just(Place::Next), 1 => (0_u64..12).prop_map(Place::At)];
    let links = prop_oneof![
        12 => Just(Link::Head),
        1 => Just(Link::Forged),
        1 => Just(Link::Nothing),
    ];
    (
        0..REALMS.len(),
        actions,
        results,
        prop::option::weighted(0.9, 0..UNITS.len()),
        places,
        links,
    )
        .prop_map(|(realm, action, result, unit, place, link)| Step {
            realm,
            action,
            result,
            unit,
            place,
            link,
        })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, ..ProptestConfig::default() })]

    /// The local journal answers every generated script as the journal in
    /// memory does, line for line.
    ///
    /// **The scripts above are a few shapes written out; this is every shape
    /// they are drawn from**, two realms and four units deep, with entries out
    /// of place and links forged among them — so a clause one store holds and
    /// the other does not is a script this finds, whichever clause it is.
    #[test]
    fn the_local_journal_answers_every_generated_script_as_the_one_in_memory_does(
        script in proptest::collection::vec(a_step(), 1..24)
    ) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        runtime.block_on(async {
            let specification = transcript_of(&mut InMemory::default(), &script).await;
            let (store, mut local) = a_local_journal().await;
            let answered = transcript_of(&mut local, &script).await;
            store.close().await;
            prop_assert_eq!(answered, specification);
            Ok(())
        })?;
    }
}

/// A history the journal wrote is the one the next open of the file reads
/// back, entry for entry, and it verifies.
///
/// **What a daemon restarting reads**: another pool, the same file, and a
/// chain that holds across the two.
#[tokio::test]
async fn a_history_written_is_the_one_the_next_open_reads_back() {
    let scratch = Scratch::new("reopened");
    let profile = scratch.profile();
    let store = LocalStore::open(&profile).await.expect("opens");
    let mut journal = store.journal();
    let script = [
        opens(0),
        in_unit(0),
        commits(0),
        opens(1),
        elsewhere(opens(2)),
    ];
    transcript_of(&mut journal, &script).await;
    let written = journal.from(0).await.expect("the history reads");
    store.close().await;
    let reopened = LocalStore::open(&profile).await.expect("opens again");
    let read = reopened.journal();
    let history = read.from(0).await.expect("the history reads");
    assert_eq!(history.stopped_at, None);
    assert_eq!(history.entries.len(), script.len());
    assert_eq!(history, written, "the history read back is the one written");
    assert_eq!(read.open_units(&id("realm-1")).await, Ok(vec![unit(1)]));
    assert_eq!(read.open_units(&id("realm-2")).await, Ok(vec![unit(2)]));
    let mut verifying = nc_journal::Journal::new(
        read.clone(),
        id("realm-1"),
        id("kernel"),
        nc_ids::Minter::new(
            nc_ids::MachineFlake::from_value(1).expect("a flake"),
            nc_ids::SystemClock,
        ),
    );
    assert_eq!(
        verifying.verify(0).await,
        Ok(Ok(5)),
        "the chain holds across opens"
    );
    verifying
        .record(a_record(9, "realm-1", "Read", Outcome::Allowed, None))
        .await
        .expect("the next entry chains onto what the file held");
    assert_eq!(verifying.verify(0).await, Ok(Ok(6)));
    reopened.close().await;
}

/// The table refuses to be edited or emptied, whoever asks: an `UPDATE`, a
/// `DELETE` of one row and a `DELETE` of every row are each refused by the
/// table's own trigger, and the history is as it was.
///
/// **From a connection of its own**, the way a console or another program
/// would come at the file: the refusal is the schema's, not this crate's.
#[tokio::test]
async fn the_table_refuses_to_be_edited_or_emptied() {
    let scratch = Scratch::new("append-only");
    let profile = scratch.profile();
    let store = LocalStore::open(&profile).await.expect("opens");
    let mut journal = store.journal();
    transcript_of(&mut journal, &[opens(0), in_unit(0), commits(0)]).await;
    let before = journal.from(0).await.expect("the history reads");
    assert_eq!(before.entries.len(), 3);
    let mut console = raw(&profile).await;
    for statement in [
        "UPDATE main.local_journal_entries SET action = 'Escalate' WHERE sequence = 1",
        "UPDATE main.local_journal_entries SET principal = 'attacker'",
        "DELETE FROM main.local_journal_entries WHERE sequence = 2",
        "DELETE FROM main.local_journal_entries",
    ] {
        let refused = sqlx::query(statement)
            .execute(&mut console)
            .await
            .expect_err("the journal is append-only");
        assert!(
            nc_local::is_a_trigger_refusal(&refused),
            "{statement}: {refused}"
        );
        assert!(
            refused.to_string().contains("the journal is append-only"),
            "{statement}: {refused}"
        );
    }
    console.close().await.expect("closes");
    assert_eq!(journal.from(0).await.expect("the history reads"), before);
    store.close().await;
}

/// An entry that does not link to the tail is refused, as the journal in
/// memory refuses it, and nothing of it lands in the file.
#[tokio::test]
async fn a_link_that_does_not_follow_the_tail_is_refused_and_lands_nothing() {
    let scratch = Scratch::new("refused-link");
    let profile = scratch.profile();
    let store = LocalStore::open(&profile).await.expect("opens");
    let mut journal = store.journal();
    let mut specification = InMemory::default();
    first(&mut journal).await;
    first(&mut specification).await;
    let forged = Entry::chain(
        1,
        Some([0xAA; 32]),
        a_record(1, "realm-1", "Read", Outcome::Allowed, None),
    );
    let refused = journal.append(forged.clone()).await;
    assert_eq!(refused, specification.append(forged).await);
    assert!(
        matches!(&refused, Err(Error::Undecodable { sequence: 1, why }) if why.contains("does not link")),
        "{refused:?}"
    );
    assert_eq!(
        refused.as_ref().map_err(Error::code),
        Err(ErrorCode::JournalChainBroken)
    );
    let mut console = raw(&profile).await;
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM main.local_journal_entries")
        .fetch_one(&mut console)
        .await
        .expect("the table reads");
    console.close().await.expect("closes");
    assert_eq!(rows, 1, "nothing of the refused entry landed");
    store.close().await;
}

/// Appends the first entry of a history to `store`.
async fn first<S: Store>(store: &mut S) {
    store
        .append(Entry::chain(
            0,
            None,
            a_record(0, "realm-1", "Read", Outcome::Allowed, None),
        ))
        .await
        .expect("the first entry lands");
}

/// An append a trigger silences is refused, and lands nothing — whether the
/// trigger is one another connection added to the file after the store
/// opened, or a temporary one on the store's own connection.
///
/// **Believed for the rows it changed, not for its success** (invariant 22):
/// `BEFORE INSERT … RAISE(IGNORE)` makes SQLite answer an `INSERT` that wrote
/// nothing with success, and a journal that took that answer would tell the
/// kernel an entry is in the audit log that is not.
#[tokio::test]
async fn an_append_a_trigger_silences_is_refused_and_lands_nothing() {
    let scratch = Scratch::new("silenced");
    let profile = scratch.profile();
    let store = LocalStore::open(&profile).await.expect("opens");
    let mut journal = store.journal();
    first(&mut journal).await;
    let mut planter = raw(&profile).await;
    sqlx::query(
        "CREATE TRIGGER main.silence BEFORE INSERT ON main.local_journal_entries \
         BEGIN SELECT RAISE(IGNORE); END",
    )
    .execute(&mut planter)
    .await
    .expect("whoever can write the file can add a trigger");
    planter.close().await.expect("closes");
    let (sequence, hash) = journal.head().await.expect("readable").expect("a head");
    let refused = journal
        .append(Entry::chain(
            sequence + 1,
            Some(hash),
            a_record(1, "realm-1", "Read", Outcome::Allowed, None),
        ))
        .await
        .expect_err("a silenced append is refused");
    assert!(
        matches!(&refused, Error::Refused(why) if why.contains("changed 0 row(s) where it had to change 1")),
        "{refused:?}"
    );
    assert_eq!(refused.code(), ErrorCode::JournalRefused);
    assert_eq!(journal.from(0).await.expect("readable").entries.len(), 1);
    store.close().await;

    let (store, mut journal) = a_local_journal().await;
    first(&mut journal).await;
    let mut connection = store.pool().acquire().await.expect("the one connection");
    sqlx::query(
        "CREATE TEMP TRIGGER swallow BEFORE INSERT ON main.local_journal_entries \
         BEGIN SELECT RAISE(IGNORE); END",
    )
    .execute(&mut *connection)
    .await
    .expect("planted");
    drop(connection);
    let (sequence, hash) = journal.head().await.expect("readable").expect("a head");
    let refused = journal
        .append(Entry::chain(
            sequence + 1,
            Some(hash),
            a_record(1, "realm-1", "Read", Outcome::Allowed, None),
        ))
        .await
        .expect_err("a planted connection writes nothing");
    assert!(
        matches!(&refused, Error::Refused(why) if why.contains("temporary")),
        "{refused:?}"
    );
    assert_eq!(journal.from(0).await.expect("readable").entries.len(), 1);
    store.close().await;
}

/// A row this code cannot decode, written into the middle of the history by
/// another connection, stops a read of the history and hides no unit opened
/// after it: which units are open is asked of the columns that say so.
///
/// **What recovery reads.** A kernel opening over this file reverts every
/// unit `open_units` names; read from the history, the unit opened past this
/// row would never be found and what the store kept of it would outlive the
/// journal that never took it.
#[tokio::test]
async fn a_row_nothing_can_decode_hides_no_unit_opened_after_it() {
    let scratch = Scratch::new("undecodable");
    let profile = scratch.profile();
    let store = LocalStore::open(&profile).await.expect("opens");
    let mut journal = store.journal();
    transcript_of(&mut journal, &[opens(0), in_unit(0)]).await;
    let poisoned = planted(&profile, &journal, "{ \"respelled\" : 1 }", None).await;
    transcript_of(&mut journal, &[opens(1), elsewhere(opens(2))]).await;
    let read = journal.from(0).await.expect("the history reads");
    assert_eq!(read.entries.len(), 2, "the rows before it are kept");
    let (stopped, why) = read.stopped_at.expect("the read stops at the row");
    assert_eq!(stopped, poisoned);
    assert!(why.contains("respelled after it was written"), "{why}");
    assert_eq!(
        journal.open_units(&id("realm-1")).await,
        Ok(vec![unit(0), unit(1)]),
        "the unit opened past the row is named"
    );
    assert_eq!(journal.open_units(&id("realm-2")).await, Ok(vec![unit(2)]));
    let verified = nc_journal::Journal::new(
        journal.clone(),
        id("realm-1"),
        id("kernel"),
        nc_ids::Minter::new(
            nc_ids::MachineFlake::from_value(1).expect("a flake"),
            nc_ids::SystemClock,
        ),
    )
    .verify(0)
    .await
    .expect("the store answers")
    .expect_err("a history holding the row does not verify");
    assert_eq!(
        (verified.sequence, verified.reason, verified.code()),
        (
            poisoned,
            nc_journal::Reason::EntryIsNotReadable,
            ErrorCode::JournalEntryTampered
        ),
        "`journal.verify` still stops at the row"
    );
    store.close().await;
}

/// A row whose stored hash is not the hash of its contents — every column
/// readable, the hash chosen by whoever wrote it — is refused where it lies:
/// a read of the history stops at it, the tail names it, and a verification
/// reports it at its own sequence with the code a tampered row carries.
///
/// **The entry is sealed, so the row is never handed back as one.**
/// `nc_journal::Entry` cannot be built outside its crate with a hash chosen by
/// its caller, so the store reads each row back through `Entry::chain` over
/// its stored link and holds the hash that gives to the one stored: a row
/// edited in place, or written around the store, is refused at that row and
/// not one later, where its link would have stopped matching.
#[tokio::test]
async fn a_row_whose_hash_is_not_its_contents_is_refused_where_it_lies() {
    let scratch = Scratch::new("forged-hash");
    let profile = scratch.profile();
    let store = LocalStore::open(&profile).await.expect("opens");
    let mut journal = store.journal();
    transcript_of(&mut journal, &[opens(0), in_unit(0)]).await;
    let written = journal
        .tail()
        .await
        .expect("readable")
        .expect("a tail")
        .record
        .resource
        .to_canonical();
    let forged = planted(&profile, &journal, &written, Some(unit(0))).await;
    let refused = journal.tail().await.expect_err("the tail is not one");
    assert!(
        matches!(&refused, Error::Undecodable { sequence, why }
            if *sequence == forged && why.contains("not the hash of its contents")),
        "{refused:?}"
    );
    let read = journal.from(0).await.expect("the history reads");
    assert_eq!(read.entries.len(), 2);
    assert_eq!(read.stopped_at.map(|(at, _)| at), Some(forged));
    let verified = nc_journal::Journal::new(
        journal.clone(),
        id("realm-1"),
        id("kernel"),
        nc_ids::Minter::new(
            nc_ids::MachineFlake::from_value(1).expect("a flake"),
            nc_ids::SystemClock,
        ),
    )
    .verify(0)
    .await
    .expect("the store answers")
    .expect_err("a history holding the row does not verify");
    assert_eq!(
        (verified.sequence, verified.code()),
        (forged, ErrorCode::JournalEntryTampered),
        "reported at the row itself"
    );
    store.close().await;
}

/// Writes a row into the file's journal from a connection of its own, after
/// the head, linked to it, holding `resource` as its text and naming `unit`;
/// answers its sequence.
///
/// **What a second connection is free to do**: the table's triggers refuse an
/// edit and a deletion, and an insertion is what a journal is for, so the row
/// lands. Its hash is sixty-four hex characters, as the table's check asks,
/// and the hash of nothing it holds.
async fn planted(
    profile: &Path,
    journal: &LocalJournal,
    resource: &str,
    unit: Option<Snowflake>,
) -> u64 {
    let (sequence, hash) = journal.head().await.expect("readable").expect("a head");
    let mut console = raw(profile).await;
    sqlx::query(
        "INSERT INTO main.local_journal_entries (sequence, realm, principal, actor_chain, \
         action, resource, result, work_unit, recorded_at, prev_hash, hash) \
         VALUES (?1, 'realm-1', 'principal-1', '[]', 'Write', ?2, 'allowed', ?3, 1, ?4, ?5)",
    )
    .bind(i64::try_from(sequence + 1).expect("a short history"))
    .bind(resource)
    .bind(unit.map(Snowflake::to_crockford))
    .bind(nc_canon::to_hex(&hash))
    .bind("ab".repeat(32))
    .execute(&mut console)
    .await
    .expect("the triggers refuse an edit and a deletion, not an insertion");
    console.close().await.expect("closes");
    sequence + 1
}

/// What the name of every schema this run makes begins with: this process's
/// id, so that no run takes another's.
fn this_runs_schemas() -> String {
    format!("nc_local_{}_", std::process::id())
}

/// A pool in a schema of this run's own, migrated by `migrations/`, or `None`
/// where the gate says no database runs.
///
/// **No database is a failure here, not a skip**: `NC_NO_STACK`, which the
/// gate sets under `--offline` and on a system that runs no stack, is the one
/// thing that means "do not touch a database", and the leg says so in its own
/// output.
async fn postgres(test: &str) -> Option<(sqlx::PgPool, String)> {
    let schema = format!("{}{test}", this_runs_schemas());
    let url = match std::env::var("DATABASE_URL").ok() {
        Some(url) => url,
        None if std::env::var_os("NC_NO_STACK").is_some() => {
            let why = std::env::var("NC_NO_STACK").unwrap_or_default();
            println!("nc-local: skipped, no database: {why}");
            return None;
        }
        None => panic!(
            "DATABASE_URL is not set. This test holds the local journal to the store that \
             runs on a server, so skipping it silently is the failure `cargo xtask check` \
             exists to prevent. Run `cargo xtask dev` and then `cargo xtask check`, or \
             `cargo xtask check --offline` to skip it on purpose"
        ),
    };
    let admin = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("DATABASE_URL is set, so it has to connect");
    for statement in [
        format!("DROP SCHEMA IF EXISTS {schema} CASCADE"),
        format!("CREATE SCHEMA {schema}"),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(&*statement))
            .execute(&admin)
            .await
            .expect("the suite's own schema is its to manage");
    }
    admin.close().await;
    let options: sqlx::postgres::PgConnectOptions = url.parse().expect("a valid url");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect_with(options.options([("search_path", schema.as_str())]))
        .await
        .expect("connects into the schema");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("the migrations apply");
    Some((pool, schema))
}

/// Drops a test's schema — only one whose name carries this process's id —
/// and closes its pool.
async fn finish(pool: sqlx::PgPool) {
    let here: Option<String> = sqlx::query_scalar("SELECT current_schema()::text")
        .fetch_one(&pool)
        .await
        .expect("the connection says where it is");
    assert!(
        here.as_deref()
            .is_some_and(|name| name.starts_with(&this_runs_schemas())),
        "this suite will not drop `{here:?}`: it is not a schema this run made"
    );
    sqlx::query("DO $$ BEGIN EXECUTE format('DROP SCHEMA %I CASCADE', current_schema()); END $$")
        .execute(&pool)
        .await
        .expect("the suite's own schema is dropped");
    pool.close().await;
}
