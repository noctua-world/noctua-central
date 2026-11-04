//! A unit's commit: whole or not at all, held to what it expected, bounded,
//! and still whole or absent when the process dies in the middle of it.
//!
//! File-backed, in a scratch directory per test removed however the test
//! leaves — except the model-based test, which runs thousands of units and
//! holds them against a `BTreeMap` in memory, and the one test that plants a
//! temporary trigger, which needs the store's single connection to be the one
//! it plants it on.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use nc_catalog::ErrorCode;
use nc_ids::{OpaqueId, Snowflake};
use nc_local::{
    BlobCreated, CommitError, Entry, Expect, LocalError, LocalStore, Quotas, Revision, StagedWrite,
    UnitKind, UnitMeta, UnitRecord, UnitState,
};
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
            std::env::temp_dir().join(format!("nc-local-units-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir(&root).expect("a scratch directory");
        Self(root)
    }

    fn profile(&self) -> PathBuf {
        self.0.join("profile")
    }
}

/// Room for everything a test does not mean to bound.
const ROOMY: Quotas = Quotas {
    entries_per_plugin: 1_000,
    lowered: BTreeMap::new(),
    blobs: 1_000,
    writes_per_unit: 1_000,
    notes_per_unit: 1_000,
};

/// The unit numbered `n`: the same value in a parent and in its child.
fn unit(n: u64) -> Snowflake {
    Snowflake::from_crockford(&nc_ids::base32::encode(n)).expect("a unit")
}

/// The realm every unit here is of, as [`meta`] makes them.
fn home() -> OpaqueId {
    id("home")
}

fn id(text: &str) -> OpaqueId {
    OpaqueId::new(text).expect("an identifier")
}

fn meta(principal: &str, kind: UnitKind, at: u64) -> UnitMeta {
    UnitMeta {
        realm: id("home"),
        principal: id(principal),
        kind,
        label: format!("unit at {at}"),
        committed_at_ms: at,
    }
}

fn put(plugin: &str, key: &str, value: &str, expect: Expect) -> StagedWrite {
    StagedWrite {
        plugin: plugin.to_owned(),
        key: key.to_owned(),
        expect,
        after: Some(value.to_owned()),
    }
}

fn delete(plugin: &str, key: &str, expect: Expect) -> StagedWrite {
    StagedWrite {
        plugin: plugin.to_owned(),
        key: key.to_owned(),
        expect,
        after: None,
    }
}

/// A step of `writes` and nothing else.
fn step(n: u64, writes: Vec<StagedWrite>) -> UnitRecord {
    UnitRecord {
        unit: unit(n),
        meta: meta("ana", UnitKind::Step, n),
        writes,
        blobs: Vec::new(),
        notes: Vec::new(),
        unpinned: Vec::new(),
        marks: Vec::new(),
    }
}

fn hash(fill: char) -> String {
    core::iter::repeat_n(fill, 64).collect()
}

/// Counts every row the unit could have left anywhere.
async fn traces_of(store: &LocalStore, of: Snowflake) -> (i64, i64, i64, i64) {
    let id = i64::try_from(of.value()).expect("fits");
    let mut counts = [0_i64; 4];
    for (count, sql) in counts.iter_mut().zip([
        "SELECT count(*) FROM main.store_units WHERE unit = ?1",
        "SELECT count(*) FROM main.store_writes WHERE unit = ?1",
        "SELECT count(*) FROM main.store_notes WHERE unit = ?1",
        "SELECT count(*) FROM main.store_entries WHERE rev_unit = ?1",
    ]) {
        *count = sqlx::query_scalar(sql)
            .bind(id)
            .fetch_one(store.pool())
            .await
            .expect("countable");
    }
    (counts[0], counts[1], counts[2], counts[3])
}

/// A committed unit is read back whole: its values, revisions, images and notes.
#[tokio::test]
async fn a_committed_unit_is_read_back_whole() {
    let scratch = Scratch::new("whole");
    let store = LocalStore::open(&scratch.profile()).await.expect("opens");
    let first = UnitRecord {
        notes: vec![r#"{"authorized":"note.put"}"#.to_owned()],
        blobs: vec![BlobCreated {
            hash: hash('a'),
            size: 12,
        }],
        ..step(
            1,
            vec![
                put("notes", "a", "1", Expect::Absent),
                put("notes", "b", r#""two""#, Expect::Anything),
                delete("notes", "never-there", Expect::Anything),
            ],
        )
    };
    let committed = store.commit(&first, &ROOMY).await.expect("commits");
    let at = |n| Some(Revision { unit: unit(1), n });
    assert_eq!(committed.revisions, vec![at(0), at(1), None]);
    assert_eq!(
        store.get(&home(), "notes", "a").await.expect("reads"),
        Some(Entry {
            value: "1".to_owned(),
            revision: Revision {
                unit: unit(1),
                n: 0
            },
        })
    );
    assert_eq!(
        store
            .get(&home(), "notes", "never-there")
            .await
            .expect("reads"),
        None
    );
    assert_eq!(
        store.get(&home(), "other", "a").await.expect("reads"),
        None,
        "another plugin's key"
    );
    assert_eq!(store.count(&home(), "notes").await.expect("counts"), 2);

    let second = step(
        2,
        vec![
            put(
                "notes",
                "a",
                "3",
                Expect::Revision(Revision {
                    unit: unit(1),
                    n: 0,
                }),
            ),
            delete("notes", "b", Expect::Value(Some(r#""two""#.to_owned()))),
        ],
    );
    store.commit(&second, &ROOMY).await.expect("commits");
    let detail = store.unit(unit(2)).await.expect("reads").expect("held");
    assert_eq!(detail.row.state, UnitState::Committed);
    assert_eq!(detail.row.meta, second.meta);
    assert_eq!(detail.images.len(), 2);
    let a = &detail.images[0];
    assert_eq!((a.plugin.as_str(), a.key.as_str()), ("notes", "a"));
    assert_eq!(
        a.before.as_ref().map(|entry| entry.value.as_str()),
        Some("1")
    );
    assert_eq!(
        a.after.as_ref().map(|entry| entry.value.as_str()),
        Some("3")
    );
    let b = &detail.images[1];
    assert_eq!(b.before.as_ref().map(|entry| entry.revision), at(1));
    assert_eq!(b.after, None, "a deleted key's after image is absent");
    let first_detail = store.unit(unit(1)).await.expect("reads").expect("held");
    assert_eq!(first_detail.notes, first.notes);
    assert_eq!(
        first_detail.images.len(),
        3,
        "the delete of nothing is an image too"
    );
    let blob = store.blob(&hash('a')).await.expect("reads").expect("held");
    assert_eq!(
        (blob.size, blob.created_unit, blob.pinned),
        (12, unit(1), true)
    );
    assert_eq!(store.unit(unit(99)).await.expect("reads"), None);
    store.close().await;
}

/// A key that does not hold what the unit expected stops the whole unit.
#[tokio::test]
async fn a_conflict_on_one_key_leaves_nothing_of_the_unit() {
    let scratch = Scratch::new("conflict");
    let store = LocalStore::open(&scratch.profile()).await.expect("opens");
    store
        .commit(&step(1, vec![put("p", "k", "1", Expect::Absent)]), &ROOMY)
        .await
        .expect("commits");
    for (n, expect) in [
        (2, Expect::Absent),
        (
            3,
            Expect::Revision(Revision {
                unit: unit(1),
                n: 1,
            }),
        ),
        (
            4,
            Expect::Revision(Revision {
                unit: unit(7),
                n: 0,
            }),
        ),
        (5, Expect::Value(Some("2".to_owned()))),
        (6, Expect::Value(None)),
    ] {
        let losing = step(
            n,
            vec![
                put("p", "other", "9", Expect::Anything),
                put("p", "k", "2", expect.clone()),
            ],
        );
        let refused = store.commit(&losing, &ROOMY).await.expect_err("a conflict");
        assert_eq!(
            refused,
            CommitError::Conflict {
                plugin: "p".to_owned(),
                key: "k".to_owned()
            },
            "{expect:?}"
        );
        assert_eq!(traces_of(&store, unit(n)).await, (0, 0, 0, 0), "{expect:?}");
        assert_eq!(
            store.get(&home(), "p", "other").await.expect("reads"),
            None,
            "{expect:?}"
        );
    }
    // And a revision that does not exist yet is not the one the key holds.
    let refused = store
        .commit(
            &step(
                8,
                vec![put(
                    "p",
                    "new",
                    "1",
                    Expect::Revision(Revision {
                        unit: unit(1),
                        n: 0,
                    }),
                )],
            ),
            &ROOMY,
        )
        .await
        .expect_err("expecting a revision of an absent key");
    assert!(
        matches!(refused, CommitError::Conflict { .. }),
        "{refused:?}"
    );
    store.close().await;
}

/// A unit committed held is held from the pruner with every unit it marks —
/// none of them can be deleted — until its release lets go of what it placed,
/// and a unit committed plainly holds nothing.
///
/// What a kernel commits a unit it has not yet heard the journal close with:
/// the record the recovery of a stopped process reverts it from, which the
/// pruner of another realm sharing the file would otherwise take.
#[tokio::test]
async fn a_unit_committed_held_holds_itself_and_what_it_marks_until_it_is_released() {
    let scratch = Scratch::new("held");
    let store = LocalStore::open(&scratch.profile()).await.expect("opens");
    store
        .commit(&step(1, vec![put("p", "a", "1", Expect::Absent)]), &ROOMY)
        .await
        .expect("a step commits");
    assert_eq!(
        store.holders().await.expect("reads"),
        [],
        "a plain commit holds nothing"
    );
    let undo = UnitRecord {
        unit: unit(2),
        meta: meta("ana", UnitKind::Undo { target: unit(1) }, 2),
        writes: vec![delete("p", "a", Expect::Value(Some("1".to_owned())))],
        blobs: Vec::new(),
        notes: Vec::new(),
        unpinned: Vec::new(),
        marks: vec![(unit(1), UnitState::Undone { by: unit(2) })],
    };
    store
        .commit_held(&undo, &ROOMY)
        .await
        .expect("an undo commits held");
    assert_eq!(store.holders().await.expect("reads"), [unit(2)]);
    for held in [unit(1), unit(2)] {
        let refused = sqlx::query("DELETE FROM main.store_units WHERE unit = ?1")
            .bind(i64::try_from(held.value()).expect("fits"))
            .execute(store.pool())
            .await;
        assert!(
            refused.is_err(),
            "unit {held}, held, was deleted: {refused:?}"
        );
    }
    assert_eq!(store.release(unit(2)).await.expect("released"), 2);
    assert_eq!(store.holders().await.expect("reads"), []);
    assert_eq!(
        store.release(unit(2)).await.expect("released"),
        0,
        "nothing is left to let go"
    );
    let deleted = sqlx::query("DELETE FROM main.store_units WHERE unit = ?1")
        .bind(i64::try_from(unit(1).value()).expect("fits"))
        .execute(store.pool())
        .await
        .expect("a unit no longer held is the pruner's again");
    assert_eq!(deleted.rows_affected(), 1);
    store.close().await;
}

/// A key is a realm's: a unit of another realm writing the same plugin's same
/// key writes a key of its own, reads nothing of the first and lists nothing
/// of it, and each realm's keys are held to the plugin's quota apart.
///
/// **Not by plugin and key alone**: a store that kept a plugin's keys so would
/// have the kernels of two realms sharing this file read and overwrite each
/// other's — through the decision point, which authorized each for an entity of
/// its own realm.
#[tokio::test]
async fn a_key_is_a_realms_and_another_realm_neither_reads_nor_writes_it() {
    let scratch = Scratch::new("realms");
    let store = LocalStore::open(&scratch.profile()).await.expect("opens");
    let one = Quotas {
        entries_per_plugin: 1,
        ..ROOMY
    };
    store
        .commit(&step(1, vec![put("p", "a", "1", Expect::Absent)]), &one)
        .await
        .expect("home's key");
    let away = id("cousins");
    let mut theirs = step(2, vec![put("p", "a", "2", Expect::Absent)]);
    theirs.meta.realm = away.clone();
    store
        .commit(&theirs, &one)
        .await
        .expect("the same key is absent in another realm, and within its quota there");
    let held = |realm: OpaqueId| {
        let store = store.clone();
        async move {
            store
                .get(&realm, "p", "a")
                .await
                .expect("reads")
                .map(|entry| entry.value)
        }
    };
    assert_eq!(held(home()).await.as_deref(), Some("1"), "home's is home's");
    assert_eq!(held(away.clone()).await.as_deref(), Some("2"));
    for realm in [home(), away.clone()] {
        assert_eq!(store.count(&realm, "p").await.expect("counts"), 1);
        assert_eq!(
            store
                .list(&realm, "p", "", 10, None)
                .await
                .expect("lists")
                .keys,
            ["a"]
        );
    }
    assert_eq!(
        store.count(&id("elsewhere"), "p").await.expect("counts"),
        0,
        "a realm that wrote nothing holds nothing"
    );
    let mut deleted = step(3, vec![delete("p", "a", Expect::Anything)]);
    deleted.meta.realm = away.clone();
    store.commit(&deleted, &one).await.expect("theirs deleted");
    assert_eq!(held(away).await, None);
    assert_eq!(
        held(home()).await.as_deref(),
        Some("1"),
        "a delete in another realm"
    );
    store.close().await;
}

/// A plugin at its quota is refused a new key and keeps every old one.
#[tokio::test]
async fn a_plugin_at_its_quota_is_refused_a_new_key_and_keeps_the_old_ones() {
    let scratch = Scratch::new("quota");
    let store = LocalStore::open(&scratch.profile()).await.expect("opens");
    let two = Quotas {
        entries_per_plugin: 2,
        ..ROOMY
    };
    store
        .commit(
            &step(
                1,
                vec![
                    put("p", "a", "1", Expect::Absent),
                    put("p", "b", "1", Expect::Absent),
                ],
            ),
            &two,
        )
        .await
        .expect("two keys fit a quota of two");
    let refused = store
        .commit(&step(2, vec![put("p", "c", "1", Expect::Absent)]), &two)
        .await
        .expect_err("a third key is past the quota");
    assert_eq!(
        refused,
        CommitError::Quota {
            plugin: "p".to_owned()
        }
    );
    assert_eq!(traces_of(&store, unit(2)).await, (0, 0, 0, 0));
    assert_eq!(store.count(&home(), "p").await.expect("counts"), 2);
    // Rewriting a key it holds is not a new key.
    store
        .commit(&step(3, vec![put("p", "a", "2", Expect::Anything)]), &two)
        .await
        .expect("an update at the quota");
    // Making room in the same unit is room.
    store
        .commit(
            &step(
                4,
                vec![
                    delete("p", "a", Expect::Anything),
                    put("p", "c", "1", Expect::Absent),
                ],
            ),
            &two,
        )
        .await
        .expect("a delete and a put at the quota");
    // The quota is per plugin.
    store
        .commit(&step(5, vec![put("q", "a", "1", Expect::Absent)]), &two)
        .await
        .expect("another plugin has its own quota");
    store.close().await;
}

/// A plugin holding more keys than its quota rewrites and deletes what it
/// holds, and is refused a new key until it is back within the bound.
///
/// **Only a new key counts against a quota**, and the test above holds a plugin
/// at its number, never over it — which a manifest lowering `store_entries`
/// below what its plugin holds makes. Measured, a commit counting every write
/// that leaves a value as a new key would refuse a rewrite over the bound,
/// which this test asks and the one above does not.
#[tokio::test]
async fn a_plugin_over_a_lowered_quota_rewrites_and_deletes_what_it_holds_and_gains_no_key() {
    let scratch = Scratch::new("over-quota");
    let store = LocalStore::open(&scratch.profile()).await.expect("opens");
    let four = ["a", "b", "c", "d"].map(|key| put("p", key, "1", Expect::Absent));
    store
        .commit(&step(1, four.to_vec()), &ROOMY)
        .await
        .expect("four keys fit a roomy quota");
    let lowered = Quotas {
        lowered: BTreeMap::from([("p".to_owned(), 2)]),
        ..ROOMY
    };
    store
        .commit(
            &step(2, vec![put("p", "a", "2", Expect::Anything)]),
            &lowered,
        )
        .await
        .expect("a key the plugin holds is not a new key");
    store
        .commit(&step(3, vec![delete("p", "d", Expect::Anything)]), &lowered)
        .await
        .expect("a delete never counts against a quota");
    let quota = CommitError::Quota {
        plugin: "p".to_owned(),
    };
    for (n, writes) in [
        (4, vec![put("p", "e", "1", Expect::Absent)]),
        (
            5,
            vec![
                delete("p", "c", Expect::Anything),
                put("p", "e", "1", Expect::Absent),
            ],
        ),
    ] {
        let refused = store
            .commit(&step(n, writes), &lowered)
            .await
            .expect_err("a new key while three are held under a quota of two");
        assert_eq!(refused, quota, "unit {n}");
        assert_eq!(traces_of(&store, unit(n)).await, (0, 0, 0, 0), "unit {n}");
    }
    assert_eq!(store.count(&home(), "p").await.expect("counts"), 3);
    store
        .commit(&step(6, vec![delete("p", "c", Expect::Anything)]), &lowered)
        .await
        .expect("a delete down to the bound");
    store
        .commit(
            &step(
                7,
                vec![
                    delete("p", "b", Expect::Anything),
                    put("p", "e", "1", Expect::Absent),
                ],
            ),
            &lowered,
        )
        .await
        .expect("at the bound, making room is room");
    assert_eq!(store.count(&home(), "p").await.expect("counts"), 2);
    assert_eq!(
        store
            .get(&home(), "p", "a")
            .await
            .expect("reads")
            .map(|entry| entry.value),
        Some("2".to_owned())
    );
    store.close().await;
}

/// One unit writing for two plugins holds each to its own quota.
///
/// **Not one number for the whole commit**, which would judge a unit writing
/// three keys of `big`, whose quota is three, and two of `small`, whose
/// manifest lowers it to one, by whichever number the kernel passed: handed
/// one, `big` would be refused inside its three; handed three, the unit would
/// commit and `small` hold two keys against a manifest that says one. A manifest
/// lowers a quota and never raises it past the catalog's.
#[tokio::test]
async fn one_unit_writing_for_two_plugins_holds_each_to_its_own_quota() {
    let scratch = Scratch::new("quota-per-plugin");
    let store = LocalStore::open(&scratch.profile()).await.expect("opens");
    let quotas = Quotas {
        entries_per_plugin: 3,
        lowered: BTreeMap::from([("small".to_owned(), 1), ("greedy".to_owned(), 9)]),
        ..ROOMY
    };
    let writes = |plugin: &str, keys: &[&str]| -> Vec<StagedWrite> {
        keys.iter()
            .map(|key| put(plugin, key, "1", Expect::Absent))
            .collect()
    };
    let both = [
        writes("big", &["a", "b", "c"]),
        writes("small", &["a", "b"]),
    ]
    .concat();
    let refused = store
        .commit(&step(1, both), &quotas)
        .await
        .expect_err("`small` past its own one");
    assert_eq!(
        refused,
        CommitError::Quota {
            plugin: "small".to_owned()
        },
        "the lower quota is the one refused, and the higher one is not"
    );
    assert_eq!(traces_of(&store, unit(1)).await, (0, 0, 0, 0));

    let within = [writes("big", &["a", "b", "c"]), writes("small", &["a"])].concat();
    store
        .commit(&step(2, within), &quotas)
        .await
        .expect("each plugin within its own");
    assert_eq!(
        (
            store.count(&home(), "big").await.expect("counts"),
            store.count(&home(), "small").await.expect("counts")
        ),
        (3, 1)
    );

    // A manifest's number above the catalog's is held to the catalog's.
    let refused = store
        .commit(&step(3, writes("greedy", &["a", "b", "c", "d"])), &quotas)
        .await
        .expect_err("`greedy` past the catalog's three");
    assert_eq!(
        refused,
        CommitError::Quota {
            plugin: "greedy".to_owned()
        }
    );
    store.close().await;
}

/// A unit the store already holds is refused as an invalid record, and the
/// first commit of it is left as it was.
///
/// **Not `NC-DB-0007`**: the primary key refuses the second insert
/// as `UNIQUE constraint failed`, which, read as it comes, is a file this build
/// does not expect — critical, not retryable, and telling the user their data
/// is not what this version expects, for a kernel or a recovery committing one
/// unit twice.
#[tokio::test]
async fn a_unit_committed_twice_is_refused_as_invalid_and_the_first_stays() {
    let scratch = Scratch::new("twice");
    let store = LocalStore::open(&scratch.profile()).await.expect("opens");
    store
        .commit(&step(1, vec![put("p", "a", "1", Expect::Absent)]), &ROOMY)
        .await
        .expect("commits");
    let before = everything_in(&store).await;
    let refused = store
        .commit(&step(1, vec![put("p", "b", "2", Expect::Absent)]), &ROOMY)
        .await
        .expect_err("a unit commits once");
    assert!(
        matches!(&refused, CommitError::Invalid(said)
            if said.contains("is already committed")),
        "{refused:?}"
    );
    assert_eq!(
        everything_in(&store).await,
        before,
        "the second commit left something"
    );
    assert_eq!(store.get(&home(), "p", "b").await.expect("reads"), None);
    store.close().await;
}

/// Every call the store offers runs inside a task the runtime may move to
/// another thread.
///
/// **`open` among them**: `sqlx`'s `Migrator::run` takes whatever can acquire a
/// connection, and the compiler cannot prove that bound for every lifetime a
/// spawned task needs, so a future that awaits it can fail to be `Send`
/// — `tokio::spawn` of such an `open` would fail to compile with
/// "implementation of `Acquire` is not general enough", and so would any kernel
/// future that awaited it behind a `Send` bound. This test is the proof by
/// compiling: a call whose future is not `Send` does not build here.
#[tokio::test]
async fn every_call_the_store_offers_runs_in_a_spawned_task() {
    let scratch = Scratch::new("spawned");
    let profile = scratch.profile();
    let task = tokio::spawn(async move {
        let store = LocalStore::open(&profile).await.expect("opens");
        let record = UnitRecord {
            blobs: vec![BlobCreated {
                hash: hash('a'),
                size: 1,
            }],
            ..step(1, vec![put("p", "k", "1", Expect::Absent)])
        };
        store.commit(&record, &ROOMY).await.expect("commits");
        assert!(store.get(&home(), "p", "k").await.expect("reads").is_some());
        assert_eq!(
            store
                .list(&home(), "p", "", 10, None)
                .await
                .expect("lists")
                .keys,
            ["k"]
        );
        assert_eq!(store.count(&home(), "p").await.expect("counts"), 1);
        assert_eq!(
            store
                .units(&id("home"), &id("ana"), 10)
                .await
                .expect("lists")
                .len(),
            1
        );
        assert!(store.unit(unit(1)).await.expect("reads").is_some());
        assert!(
            store
                .set_state(unit(1), UnitState::Committed)
                .await
                .expect("sets")
        );
        assert!(store.blob(&hash('a')).await.expect("reads").is_some());
        assert!(
            store
                .set_pinned(&hash('a'), false, 1)
                .await
                .expect("unpins")
        );
        assert_eq!(
            store.unpinned_before(2, 10).await.expect("lists"),
            vec![hash('a')]
        );
        assert!(store.forget_blob(&hash('a')).await.expect("forgets"));
        assert_eq!(
            store
                .forget_beyond(&id("home"), &id("ana"), 0)
                .await
                .expect("forgets"),
            1
        );
        store.close().await;
    });
    task.await.expect("the task ran to its end");
}

/// A unit beyond what `forget_beyond` keeps that a kernel still holds from the
/// pruner is not forgotten, and neither is anything else: `store_unit_holds`
/// refuses the whole delete, as the method's `# Errors` says — and is said to,
/// rather than as a trigger no schema here declares.
///
/// **SQLite answers an `ON DELETE RESTRICT` with a trigger's code**, and read
/// by the code alone the refusal would be told as an impossible trigger's.
#[tokio::test]
async fn forgetting_past_a_held_unit_is_refused_and_forgets_nothing() {
    let scratch = Scratch::new("forget-held");
    let store = LocalStore::open(&scratch.profile()).await.expect("opens");
    let held = step(1, vec![put("p", "a", "1", Expect::Absent)]);
    store.commit_held(&held, &ROOMY).await.expect("commits");
    let kept = step(2, vec![put("p", "b", "1", Expect::Absent)]);
    store.commit(&kept, &ROOMY).await.expect("commits");
    let before = everything_in(&store).await;
    let refused = store.forget_beyond(&id("home"), &id("ana"), 1).await;
    assert_eq!(
        refused,
        Err(LocalError::Unexpected(format!(
            "forgetting units: {} — {}",
            nc_local::STILL_REFERRED_TO,
            nc_local::STILL_HELD
        ))),
        "a held unit was forgotten, or refused in other words"
    );
    assert_eq!(
        everything_in(&store).await,
        before,
        "a refused forgetting forgot something"
    );
    store.close().await;
}

/// This machine's blobs are bounded, and pinning one again is not a new one.
#[tokio::test]
async fn the_blob_bound_refuses_a_new_blob_and_not_one_held() {
    let scratch = Scratch::new("blobs");
    let store = LocalStore::open(&scratch.profile()).await.expect("opens");
    let one = Quotas { blobs: 1, ..ROOMY };
    let with_blob = |n, fill| UnitRecord {
        blobs: vec![BlobCreated {
            hash: hash(fill),
            size: 3,
        }],
        ..step(n, Vec::new())
    };
    store
        .commit(&with_blob(1, 'a'), &one)
        .await
        .expect("the first blob fits");
    let refused = store
        .commit(&with_blob(2, 'b'), &one)
        .await
        .expect_err("a second blob is past the bound");
    assert_eq!(refused, CommitError::TooManyBlobs);
    assert_eq!(store.blob(&hash('b')).await.expect("reads"), None);
    assert_eq!(traces_of(&store, unit(2)).await, (0, 0, 0, 0));

    // Unpinned, it is what the sweep may take; created again, it is pinned
    // again and still counted once.
    assert!(
        store
            .set_pinned(&hash('a'), false, 500)
            .await
            .expect("unpins")
    );
    let unpinned = store.blob(&hash('a')).await.expect("reads").expect("held");
    assert_eq!(
        (unpinned.pinned, unpinned.unpinned_at_ms),
        (false, Some(500))
    );
    assert_eq!(
        store.unpinned_before(501, 10).await.expect("lists"),
        vec![hash('a')]
    );
    assert_eq!(
        store.unpinned_before(500, 10).await.expect("lists"),
        [] as [String; 0]
    );
    store
        .commit(&with_blob(3, 'a'), &one)
        .await
        .expect("pinning a held blob again");
    let pinned = store.blob(&hash('a')).await.expect("reads").expect("held");
    assert_eq!((pinned.pinned, pinned.unpinned_at_ms), (true, None));
    assert_eq!(pinned.created_unit, unit(1), "the first creator is kept");

    // A pinned blob is not forgotten; an unpinned one is; a missing one is not.
    assert!(!store.forget_blob(&hash('a')).await.expect("answers"));
    assert!(
        store
            .set_pinned(&hash('a'), false, 600)
            .await
            .expect("unpins")
    );
    assert!(store.forget_blob(&hash('a')).await.expect("answers"));
    assert_eq!(store.blob(&hash('a')).await.expect("reads"), None);
    assert!(!store.forget_blob(&hash('a')).await.expect("answers"));
    assert!(
        !store
            .set_pinned(&hash('c'), true, 0)
            .await
            .expect("answers")
    );
    store.close().await;
}

/// A record no unit can be is refused before a statement runs.
#[tokio::test]
async fn a_record_no_unit_can_be_is_refused_and_nothing_lands() {
    let scratch = Scratch::new("invalid");
    let store = LocalStore::open(&scratch.profile()).await.expect("opens");
    let refusals: [(&str, UnitRecord); 7] = [
        (
            "written twice",
            step(
                1,
                vec![
                    put("p", "k", "1", Expect::Anything),
                    put("p", "k", "2", Expect::Anything),
                ],
            ),
        ),
        (
            "not its canonical form",
            step(2, vec![put("p", "k", r#"{"b":1,"a":2}"#, Expect::Anything)]),
        ),
        (
            "not JSON",
            step(3, vec![put("p", "k", "{", Expect::Anything)]),
        ),
        (
            "not its canonical form",
            step(
                4,
                vec![put("p", "k", "1", Expect::Value(Some(" 1".to_owned())))],
            ),
        ),
        (
            "not JSON",
            UnitRecord {
                notes: vec!["note".to_owned()],
                ..step(5, Vec::new())
            },
        ),
        (
            "not a BLAKE3 hash",
            UnitRecord {
                blobs: vec![BlobCreated {
                    hash: hash('A'),
                    size: 1,
                }],
                ..step(6, Vec::new())
            },
        ),
        (
            "recorded twice",
            UnitRecord {
                blobs: vec![
                    BlobCreated {
                        hash: hash('a'),
                        size: 1,
                    },
                    BlobCreated {
                        hash: hash('a'),
                        size: 1,
                    },
                ],
                ..step(7, Vec::new())
            },
        ),
    ];
    for (why, record) in refusals {
        let refused = store.commit(&record, &ROOMY).await.expect_err(why);
        assert!(
            matches!(&refused, CommitError::Invalid(said) if said.contains(why)),
            "{why}: {refused:?}"
        );
        assert_eq!(traces_of(&store, record.unit).await, (0, 0, 0, 0), "{why}");
    }
    // A mark on a unit the store does not hold is found inside the
    // transaction, and takes the unit with it.
    let marking = UnitRecord {
        unpinned: Vec::new(),
        marks: vec![(unit(40), UnitState::Committed)],
        ..step(8, vec![put("p", "k", "1", Expect::Anything)])
    };
    let refused = store
        .commit(&marking, &ROOMY)
        .await
        .expect_err("a mark on nothing");
    assert!(
        matches!(&refused, CommitError::Invalid(said) if said.contains("does not hold it")),
        "{refused:?}"
    );
    assert_eq!(traces_of(&store, unit(8)).await, (0, 0, 0, 0));
    store.close().await;
}

/// A unit takes the pin off the blobs it names as of when it commits, in the
/// transaction that lands the rest of it — and a unit refused for anything
/// leaves every pin where it was.
///
/// **What a reversal is**: the kernel reverses a unit the journal did not keep
/// by committing its images back and unpinning the blobs it created, and a pin
/// taken off by a reversal the store refused would hand the sweep the bytes of
/// a unit still committed.
#[tokio::test]
async fn a_unit_takes_the_pin_off_the_blobs_it_names_with_the_rest_of_it() {
    let scratch = Scratch::new("unpinned");
    let store = LocalStore::open(&scratch.profile()).await.expect("opens");
    let made = UnitRecord {
        blobs: vec![
            BlobCreated {
                hash: hash('a'),
                size: 3,
            },
            BlobCreated {
                hash: hash('b'),
                size: 4,
            },
        ],
        ..step(1, vec![put("p", "k", "1", Expect::Absent)])
    };
    store.commit(&made, &ROOMY).await.expect("commits");
    let reversal = |n: u64, unpinned: Vec<String>, target: Snowflake| UnitRecord {
        unit: unit(n),
        meta: meta("ana", UnitKind::Undo { target: unit(1) }, n * 100),
        writes: vec![delete("p", "k", Expect::Value(Some("1".to_owned())))],
        blobs: Vec::new(),
        notes: Vec::new(),
        unpinned,
        marks: vec![(target, UnitState::Undone { by: unit(n) })],
    };
    // Refused at its last statement, a mark on a unit the store does not hold:
    // the pin it would have taken off is still on.
    let refused = store
        .commit(&reversal(2, vec![hash('a')], unit(40)), &ROOMY)
        .await
        .expect_err("a mark on nothing");
    assert!(matches!(refused, CommitError::Invalid(_)), "{refused:?}");
    let held = store.blob(&hash('a')).await.expect("reads").expect("held");
    assert_eq!((held.pinned, held.unpinned_at_ms), (true, None));
    // A blob the store does not hold is no blob to unpin, and nothing lands.
    let refused = store
        .commit(&reversal(3, vec![hash('c')], unit(1)), &ROOMY)
        .await
        .expect_err("a blob the store does not hold");
    assert!(
        matches!(&refused, CommitError::Invalid(said) if said.contains("does not hold it")),
        "{refused:?}"
    );
    assert_eq!(traces_of(&store, unit(3)).await, (0, 0, 0, 0));
    assert_eq!(
        store
            .get(&home(), "p", "k")
            .await
            .expect("reads")
            .map(|entry| entry.value),
        Some("1".to_owned())
    );
    // Landed, the one it names is unpinned as of the unit's own time, and the
    // one it does not name keeps its pin.
    store
        .commit(&reversal(4, vec![hash('a')], unit(1)), &ROOMY)
        .await
        .expect("the reversal lands");
    let unpinned = store.blob(&hash('a')).await.expect("reads").expect("held");
    assert_eq!(
        (unpinned.pinned, unpinned.unpinned_at_ms),
        (false, Some(400))
    );
    let kept = store.blob(&hash('b')).await.expect("reads").expect("held");
    assert_eq!((kept.pinned, kept.unpinned_at_ms), (true, None));
    assert_eq!(
        store.unpinned_before(401, 10).await.expect("lists"),
        vec![hash('a')]
    );
    store.close().await;
}

/// A pin taken off what is not a blob's hash, off one blob twice, or off a blob
/// the same unit pins is a record no unit can be, refused before a statement.
#[tokio::test]
async fn a_pin_taken_off_a_blob_no_unit_could_name_so_is_refused() {
    let scratch = Scratch::new("unpinned-invalid");
    let store = LocalStore::open(&scratch.profile()).await.expect("opens");
    for (why, record) in [
        (
            "not a BLAKE3 hash",
            UnitRecord {
                unpinned: vec![hash('g')],
                ..step(1, Vec::new())
            },
        ),
        (
            "unpinned twice, or pinned and unpinned",
            UnitRecord {
                unpinned: vec![hash('a'), hash('a')],
                ..step(2, Vec::new())
            },
        ),
        (
            "unpinned twice, or pinned and unpinned",
            UnitRecord {
                blobs: vec![BlobCreated {
                    hash: hash('a'),
                    size: 1,
                }],
                unpinned: vec![hash('a')],
                ..step(3, Vec::new())
            },
        ),
    ] {
        let refused = store.commit(&record, &ROOMY).await.expect_err(why);
        assert!(
            matches!(&refused, CommitError::Invalid(said) if said.contains(why)),
            "{why}: {refused:?}"
        );
        assert_eq!(traces_of(&store, record.unit).await, (0, 0, 0, 0), "{why}");
    }
    assert_eq!(store.blob(&hash('a')).await.expect("reads"), None);
    store.close().await;
}

/// One unit records as many images and notes as its bound and no more, and a
/// unit past either leaves nothing.
///
/// **This is what bounds `store_writes` and `store_notes`**: their rows go
/// with their unit, so `store_units`' bound is theirs only times what one unit
/// may record — and without it the store would hold a unit to nothing: one unit
/// with 20,000 images and 50,000 notes would commit. At the bound, so a count
/// off by one in either direction is a failure here.
#[tokio::test]
async fn a_unit_past_what_one_unit_may_record_is_refused_and_leaves_nothing() {
    let scratch = Scratch::new("per-unit");
    let store = LocalStore::open(&scratch.profile()).await.expect("opens");
    let two = Quotas {
        writes_per_unit: 2,
        notes_per_unit: 2,
        ..ROOMY
    };
    let notes = |count: usize| (0..count).map(|n| n.to_string()).collect::<Vec<_>>();
    let at_the_bound = UnitRecord {
        notes: notes(2),
        ..step(
            1,
            vec![
                put("p", "a", "1", Expect::Absent),
                put("p", "b", "1", Expect::Absent),
            ],
        )
    };
    store
        .commit(&at_the_bound, &two)
        .await
        .expect("two images and two notes fit a bound of two");
    assert_eq!(traces_of(&store, unit(1)).await, (1, 2, 2, 2));

    let three_images = step(
        2,
        vec![
            put("p", "c", "1", Expect::Absent),
            put("p", "d", "1", Expect::Absent),
            put("p", "e", "1", Expect::Absent),
        ],
    );
    let three_notes = UnitRecord {
        notes: notes(3),
        ..step(3, Vec::new())
    };
    for (record, said) in [
        (
            three_images,
            "records 3 images, and one unit records at most 2",
        ),
        (
            three_notes,
            "records 3 notes, and one unit records at most 2",
        ),
    ] {
        let refused = store
            .commit(&record, &two)
            .await
            .expect_err("a unit past its bound");
        assert!(
            matches!(&refused, CommitError::Invalid(why) if why.contains(said)),
            "{refused:?}"
        );
        assert_eq!(traces_of(&store, record.unit).await, (0, 0, 0, 0), "{said}");
    }
    assert_eq!(store.count(&home(), "p").await.expect("counts"), 2);
    store.close().await;
}

/// An undo marks its target in the transaction that restores its images.
#[tokio::test]
async fn an_undo_and_the_mark_on_its_target_land_together() {
    let scratch = Scratch::new("undo");
    let store = LocalStore::open(&scratch.profile()).await.expect("opens");
    store
        .commit(&step(1, vec![put("p", "k", "1", Expect::Absent)]), &ROOMY)
        .await
        .expect("commits");
    let undo = UnitRecord {
        unit: unit(2),
        meta: meta("ana", UnitKind::Undo { target: unit(1) }, 2),
        writes: vec![delete("p", "k", Expect::Value(Some("1".to_owned())))],
        blobs: Vec::new(),
        notes: Vec::new(),
        unpinned: Vec::new(),
        marks: vec![(unit(1), UnitState::Undone { by: unit(2) })],
    };
    store.commit(&undo, &ROOMY).await.expect("commits");
    let target = store.unit(unit(1)).await.expect("reads").expect("held");
    assert_eq!(target.row.state, UnitState::Undone { by: unit(2) });
    let reversal = store.unit(unit(2)).await.expect("reads").expect("held");
    assert_eq!(reversal.row.meta.kind, UnitKind::Undo { target: unit(1) });
    assert_eq!(store.get(&home(), "p", "k").await.expect("reads"), None);

    // And the state is set on its own, answering whether the unit is there.
    assert!(
        store
            .set_state(unit(1), UnitState::Committed)
            .await
            .expect("sets")
    );
    let target = store.unit(unit(1)).await.expect("reads").expect("held");
    assert_eq!(target.row.state, UnitState::Committed);
    assert!(
        !store
            .set_state(unit(77), UnitState::Committed)
            .await
            .expect("answers")
    );
    store.close().await;
}

/// A redo is read back as a redo of its undo, and an undo as an undo.
///
/// **Each kind is its own row.** Measured, with the arm that records a redo
/// planted to record an undo of the same target, every other suite of this
/// crate stays green, while the kernel's undo and redo tests and
/// `crates/nc-kernel/tests/both_stores_answer_alike.rs` fail; the kernel's
/// redo is read from exactly that row.
#[tokio::test]
async fn a_redo_is_read_back_as_a_redo_and_an_undo_as_an_undo() {
    let scratch = Scratch::new("redo");
    let store = LocalStore::open(&scratch.profile()).await.expect("opens");
    store
        .commit(&step(1, vec![put("p", "k", "1", Expect::Absent)]), &ROOMY)
        .await
        .expect("commits");
    let undo = UnitRecord {
        meta: meta("ana", UnitKind::Undo { target: unit(1) }, 2),
        unpinned: Vec::new(),
        marks: vec![(unit(1), UnitState::Undone { by: unit(2) })],
        ..step(
            2,
            vec![delete("p", "k", Expect::Value(Some("1".to_owned())))],
        )
    };
    store.commit(&undo, &ROOMY).await.expect("commits");
    let redo = UnitRecord {
        meta: meta("ana", UnitKind::Redo { target: unit(2) }, 3),
        unpinned: Vec::new(),
        marks: vec![(unit(2), UnitState::Undone { by: unit(3) })],
        ..step(3, vec![put("p", "k", "1", Expect::Absent)])
    };
    store.commit(&redo, &ROOMY).await.expect("commits");
    let kinds = [
        UnitKind::Redo { target: unit(2) },
        UnitKind::Undo { target: unit(1) },
        UnitKind::Step,
    ];
    for (n, kind) in [3, 2, 1].into_iter().zip(kinds) {
        let read = store.unit(unit(n)).await.expect("reads").expect("held");
        assert_eq!(read.row.meta.kind, kind, "unit {n}");
    }
    let listed: Vec<UnitKind> = store
        .units(&id("home"), &id("ana"), 10)
        .await
        .expect("lists")
        .iter()
        .map(|row| row.meta.kind)
        .collect();
    assert_eq!(listed, kinds, "newest first, each as its kind");
    // **And a person's steps are the rows of a step's kind alone**, which is
    // where an undo looks for the step it undoes: one step asked for is the
    // step, though its undo and its redo are newer.
    for limit in [10, 1] {
        let steps: Vec<UnitKind> = store
            .steps(&id("home"), &id("ana"), limit)
            .await
            .expect("lists")
            .iter()
            .map(|row| row.meta.kind)
            .collect();
        assert_eq!(steps, [UnitKind::Step], "{limit} step(s) asked for");
    }
    assert_eq!(
        store.steps(&id("home"), &id("ana"), 0).await,
        Ok(Vec::new())
    );
    assert_eq!(
        store.steps(&id("office"), &id("ana"), 10).await,
        Ok(Vec::new())
    );
    assert_eq!(
        store.steps(&id("home"), &id("bruno"), 10).await,
        Ok(Vec::new())
    );
    store.close().await;
}

/// Whether another committed unit holds a note: one of any realm and any kind
/// does, and the unit asked about and a unit undone do not.
///
/// What an undo asks before it takes a blob's pin off: a unit that made the
/// same bytes holds the same note — a step, or an undo whose compensation made
/// them — so another's blob keeps its pin.
#[tokio::test]
async fn a_note_is_held_elsewhere_only_by_another_unit_still_committed() {
    let scratch = Scratch::new("noted");
    let store = LocalStore::open(&scratch.profile()).await.expect("opens");
    let (x, y, z) = (r#"{"note":"x"}"#, r#"{"note":"y"}"#, r#"{"note":"z"}"#);
    let noted = |n: u64, note: &str| UnitRecord {
        notes: vec![note.to_owned()],
        ..step(n, vec![put("p", &format!("k{n}"), "1", Expect::Anything)])
    };
    store.commit(&noted(1, x), &ROOMY).await.expect("commits");
    assert_eq!(store.noted_elsewhere(x, unit(1)).await, Ok(false));
    assert_eq!(store.noted_elsewhere(x, unit(9)).await, Ok(true));
    assert_eq!(
        store.noted_elsewhere(r#"{"note": "x"}"#, unit(9)).await,
        Ok(false)
    );
    let undo = UnitRecord {
        meta: meta("ana", UnitKind::Undo { target: unit(1) }, 2),
        marks: vec![(unit(1), UnitState::Undone { by: unit(2) })],
        ..noted(2, y)
    };
    store.commit(&undo, &ROOMY).await.expect("commits");
    assert_eq!(
        store.noted_elsewhere(x, unit(9)).await,
        Ok(false),
        "a step undone holds nothing"
    );
    assert_eq!(
        store.noted_elsewhere(y, unit(9)).await,
        Ok(true),
        "an undo still committed holds its notes, as a step does"
    );
    let redo = UnitRecord {
        meta: meta("ana", UnitKind::Redo { target: unit(2) }, 3),
        marks: vec![(unit(2), UnitState::Undone { by: unit(3) })],
        ..noted(3, z)
    };
    store.commit(&redo, &ROOMY).await.expect("commits");
    assert_eq!(
        store.noted_elsewhere(y, unit(9)).await,
        Ok(false),
        "an undo redone holds nothing"
    );
    assert_eq!(
        store.noted_elsewhere(z, unit(9)).await,
        Ok(true),
        "a redo holds its notes"
    );
    let elsewhere = UnitRecord {
        meta: UnitMeta {
            realm: id("office"),
            ..meta("bruno", UnitKind::Step, 4)
        },
        ..noted(4, x)
    };
    store.commit(&elsewhere, &ROOMY).await.expect("commits");
    assert_eq!(
        store.noted_elsewhere(x, unit(9)).await,
        Ok(true),
        "a step of another realm and person holds it"
    );
    store.close().await;
}

/// Units are listed newest first, per person, and forgotten oldest first.
#[tokio::test]
async fn units_are_listed_newest_first_and_forgotten_oldest_first() {
    let scratch = Scratch::new("history");
    let store = LocalStore::open(&scratch.profile()).await.expect("opens");
    for n in 1..=5 {
        let record = UnitRecord {
            notes: vec![format!("{n}")],
            ..step(n, vec![put("p", &format!("k{n}"), "1", Expect::Anything)])
        };
        store.commit(&record, &ROOMY).await.expect("commits");
    }
    let bruno = UnitRecord {
        meta: meta("bruno", UnitKind::Step, 6),
        ..step(6, vec![put("p", "b", "1", Expect::Anything)])
    };
    store.commit(&bruno, &ROOMY).await.expect("commits");
    let listed: Vec<Snowflake> = store
        .units(&id("home"), &id("ana"), 3)
        .await
        .expect("lists")
        .iter()
        .map(|row| row.unit)
        .collect();
    assert_eq!(listed, vec![unit(5), unit(4), unit(3)]);

    assert_eq!(
        store
            .forget_beyond(&id("home"), &id("ana"), 2)
            .await
            .expect("forgets"),
        3
    );
    let left: Vec<Snowflake> = store
        .units(&id("home"), &id("ana"), 10)
        .await
        .expect("lists")
        .iter()
        .map(|row| row.unit)
        .collect();
    assert_eq!(left, vec![unit(5), unit(4)], "the oldest went");
    for gone in 1..=3 {
        assert_eq!(
            traces_of(&store, unit(gone)).await,
            (0, 0, 0, 1),
            "a forgotten unit's row, images and notes went, and the key it wrote stayed"
        );
    }
    assert_eq!(
        store
            .get(&home(), "p", "k1")
            .await
            .expect("reads")
            .map(|entry| entry.value),
        Some("1".to_owned()),
        "forgetting history does not forget what the store holds"
    );
    assert_eq!(
        store
            .units(&id("home"), &id("bruno"), 10)
            .await
            .expect("lists")
            .len(),
        1,
        "another person's history is theirs"
    );
    assert_eq!(
        store
            .forget_beyond(&id("home"), &id("ana"), 2)
            .await
            .expect("forgets"),
        0
    );
    store.close().await;
}

/// A page ends where the limit does, and the next begins after it.
#[tokio::test]
async fn a_listing_pages_through_one_plugins_keys_under_a_prefix() {
    let scratch = Scratch::new("list");
    let store = LocalStore::open(&scratch.profile()).await.expect("opens");
    let keys = ["a", "note/1", "note/2", "note/3", "note0", "notes", "z"];
    store
        .commit(
            &step(
                1,
                keys.iter()
                    .map(|key| put("p", key, "1", Expect::Absent))
                    .chain([put("q", "note/9", "1", Expect::Absent)])
                    .collect(),
            ),
            &ROOMY,
        )
        .await
        .expect("commits");
    let first = store
        .list(&home(), "p", "note/", 2, None)
        .await
        .expect("lists");
    assert_eq!(first.keys, ["note/1", "note/2"]);
    assert_eq!(first.next.as_deref(), Some("note/2"));
    let second = store
        .list(&home(), "p", "note/", 2, first.next.as_deref())
        .await
        .expect("lists");
    assert_eq!(
        second.keys,
        ["note/3"],
        "`note0` sorts after the prefix and is not in it"
    );
    assert_eq!(second.next, None);
    let everything = store
        .list(&home(), "p", "", 100, None)
        .await
        .expect("lists");
    assert_eq!(everything.keys, keys);
    assert_eq!(everything.next, None);
    let exact = store.list(&home(), "p", "", 7, None).await.expect("lists");
    assert_eq!(
        exact.next, None,
        "a page that holds the last key has no next"
    );
    // **A limit of zero is a page of one**, and still says where the rest
    // begins. A page of nothing would answer `next: None` over keys it has not
    // listed — a listing that says it is over — and an assertion that reads
    // only that the page is empty would agree with it.
    let none_asked = store
        .list(&home(), "p", "note/", 0, None)
        .await
        .expect("lists");
    assert_eq!(none_asked.keys, ["note/1"]);
    assert_eq!(none_asked.next.as_deref(), Some("note/1"));
    let last = store
        .list(&home(), "p", "note/", 0, Some("note/2"))
        .await
        .expect("lists");
    assert_eq!(last.keys, ["note/3"]);
    assert_eq!(last.next, None, "the last key under the prefix has no next");
    store.close().await;
}

/// An image cannot be edited, and the refusal is SQLite's trigger code.
#[tokio::test]
async fn an_image_or_a_note_cannot_be_edited() {
    let scratch = Scratch::new("images");
    let profile = scratch.profile();
    let store = LocalStore::open(&profile).await.expect("opens");
    let record = UnitRecord {
        notes: vec!["1".to_owned()],
        ..step(1, vec![put("p", "k", "1", Expect::Absent)])
    };
    store.commit(&record, &ROOMY).await.expect("commits");
    store.close().await;
    let mut connection = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(profile.join(nc_local::FILE_NAME))
        .pragma("trusted_schema", "OFF")
        .connect()
        .await
        .expect("the file opens");
    for statement in [
        "UPDATE main.store_writes SET after_value = '2'",
        "UPDATE main.store_notes SET note = '2'",
    ] {
        let refused = sqlx::query(statement)
            .execute(&mut connection)
            .await
            .expect_err("an image is never edited");
        assert!(
            nc_local::is_a_trigger_refusal(&refused),
            "{statement}: {refused}"
        );
        assert_eq!(
            refused
                .as_database_error()
                .and_then(sqlx::error::DatabaseError::code)
                .as_deref(),
            Some(nc_local::TRIGGER_REFUSED)
        );
    }
    // Deleting a unit takes its images with it, which is what forgetting is.
    let deleted = sqlx::query("DELETE FROM main.store_units")
        .execute(&mut connection)
        .await
        .expect("a unit is deleted whole");
    assert_eq!(deleted.rows_affected(), 1);
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM main.store_writes")
        .fetch_one(&mut connection)
        .await
        .expect("countable");
    assert_eq!(left, 0);
    connection.close().await.expect("closes");
}

/// A temporary object on the store's connection stops every write.
///
/// **A temporary trigger silences what `main.` cannot**: measured,
/// a `BEFORE INSERT … RAISE(IGNORE)` makes an insert report success having
/// written nothing. The store's one connection is the one planted on, which is
/// why this is the in-memory store.
///
/// **And a release is a write**, in this loop: a temporary trigger that
/// deletes every hold when one is let go keeps the release's own row count
/// exact, so without the check a release would answer `Ok(1)` and let go of
/// another unit's hold too — one the pruner may then take the record of while
/// its recovery still needs it.
#[tokio::test]
async fn a_temporary_object_on_the_connection_stops_every_write() {
    let store = LocalStore::in_memory_for_tests().await.expect("opens");
    for n in [2, 3] {
        store
            .commit_held(
                &step(n, vec![put("p", &format!("held{n}"), "1", Expect::Absent)]),
                &ROOMY,
            )
            .await
            .expect("commits held");
    }
    let mut connection = store.pool().acquire().await.expect("the one connection");
    for planted in [
        "CREATE TEMP TRIGGER swallow BEFORE INSERT ON main.store_entries \
         BEGIN SELECT RAISE(IGNORE); END",
        "CREATE TEMP TRIGGER everything_goes AFTER DELETE ON main.store_unit_holds \
         BEGIN DELETE FROM store_unit_holds; END",
    ] {
        sqlx::query(planted)
            .execute(&mut *connection)
            .await
            .expect("planted");
    }
    drop(connection);
    let refused = store
        .commit(&step(1, vec![put("p", "k", "1", Expect::Absent)]), &ROOMY)
        .await
        .expect_err("a planted connection writes nothing");
    let CommitError::Store(error) = refused else {
        panic!("not a store failure: {refused:?}");
    };
    assert_eq!(error.code(), ErrorCode::DbLocalSchemaUnexpected, "{error}");
    assert!(error.to_string().contains("temporary"), "{error}");
    for refusal in [
        store
            .set_state(unit(1), UnitState::Committed)
            .await
            .map(|_| ()),
        store
            .forget_beyond(&id("home"), &id("ana"), 0)
            .await
            .map(|_| ()),
        store.set_pinned(&hash('a'), true, 0).await.map(|_| ()),
        store.forget_blob(&hash('a')).await.map(|_| ()),
        store.release(unit(2)).await.map(|_| ()),
    ] {
        let error = refusal.expect_err("every write begins with the check");
        assert_eq!(error.code(), ErrorCode::DbLocalSchemaUnexpected, "{error}");
    }
    assert_eq!(store.get(&home(), "p", "k").await.expect("reads"), None);
    assert_eq!(
        store.holders().await.expect("readable"),
        [unit(2), unit(3)],
        "a refused release let go of a hold"
    );
}

/// A database another connection holds for writing is unavailable, not broken.
#[tokio::test]
async fn a_database_held_by_another_writer_is_unavailable() {
    let scratch = Scratch::new("busy");
    let profile = scratch.profile();
    let store = LocalStore::open(&profile).await.expect("opens");
    let mut holder = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(profile.join(nc_local::FILE_NAME))
        .connect()
        .await
        .expect("the file opens");
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut holder)
        .await
        .expect("held for writing");
    let refused = store
        .commit(&step(1, vec![put("p", "k", "1", Expect::Absent)]), &ROOMY)
        .await
        .expect_err("a held database refuses a second writer past its timeout");
    let CommitError::Store(error) = refused else {
        panic!("not a store failure: {refused:?}");
    };
    assert!(matches!(error, LocalError::Unavailable(_)), "{error:?}");
    assert_eq!(error.code(), ErrorCode::DbUnreachable);
    assert!(
        error.code().retryable(),
        "a writer that finishes frees the database"
    );
    sqlx::query("ROLLBACK")
        .execute(&mut holder)
        .await
        .expect("released");
    holder.close().await.expect("closes");
    store
        .commit(&step(1, vec![put("p", "k", "1", Expect::Absent)]), &ROOMY)
        .await
        .expect("commits once the other writer has gone");
    store.close().await;
}

/// One of the store's writing calls, tried through a trigger that silences it.
#[derive(Debug, Clone, Copy)]
enum Silenced {
    /// `commit` of the unit this makes.
    Commit(fn() -> UnitRecord),
    /// `set_state` of unit 1.
    SetState,
    /// `forget_beyond`, keeping one of ana's two units.
    ForgetBeyond,
    /// `set_pinned` of the pinned blob, unpinning it.
    SetPinned,
    /// `forget_blob` of the unpinned blob.
    ForgetBlob,
    /// `commit_held` of a unit of its own.
    CommitHeld,
    /// `release` of the holds unit 2 placed.
    Release,
}

/// The ground every silenced write is tried on: two of ana's units — the
/// second held from the pruner — three keys, a pinned blob and an unpinned
/// one.
async fn the_ground_a_silenced_write_meets(profile: &Path) -> LocalStore {
    let store = LocalStore::open(profile).await.expect("opens");
    let first = UnitRecord {
        blobs: vec![BlobCreated {
            hash: hash('a'),
            size: 3,
        }],
        notes: vec!["1".to_owned()],
        ..step(1, vec![put("p", "a", "1", Expect::Absent)])
    };
    store.commit(&first, &ROOMY).await.expect("commits");
    let second = UnitRecord {
        blobs: vec![BlobCreated {
            hash: hash('e'),
            size: 3,
        }],
        ..step(
            2,
            vec![
                put("p", "b", "1", Expect::Absent),
                put("p", "d", "1", Expect::Absent),
            ],
        )
    };
    store.commit_held(&second, &ROOMY).await.expect("commits");
    assert!(
        store
            .set_pinned(&hash('e'), false, 100)
            .await
            .expect("unpins")
    );
    store
}

/// Every row of every table, as text and in order.
async fn everything_in(store: &LocalStore) -> Vec<String> {
    let mut rows = Vec::new();
    for sql in [
        "SELECT 'entry ' || plugin || ' ' || store_key || ' ' || value || ' ' || rev_unit \
         || '.' || rev_n FROM main.store_entries",
        "SELECT 'unit ' || unit || ' ' || state || ' ' || coalesce(undone_by, '-') \
         FROM main.store_units",
        "SELECT 'image ' || unit || ' ' || plugin || ' ' || store_key FROM main.store_writes",
        "SELECT 'note ' || unit || ' ' || seq || ' ' || note FROM main.store_notes",
        "SELECT 'blob ' || hash || ' ' || pinned || ' ' || coalesce(unpinned_at, '-') \
         FROM main.blobs",
        "SELECT 'hold ' || unit || ' ' || holder FROM main.store_unit_holds",
    ] {
        let mut held: Vec<String> = sqlx::query_scalar(sql)
            .fetch_all(store.pool())
            .await
            .expect("readable");
        held.sort();
        rows.extend(held);
    }
    rows
}

/// The commits a trigger can silence: the statement that plants it, what the
/// store calls the write in its refusal, and the unit whose commit makes it.
const fn the_commits_a_trigger_can_silence() -> [(&'static str, &'static str, Silenced); 10] {
    [
        (
            "CREATE TRIGGER main.silence BEFORE INSERT ON main.store_units \
             BEGIN SELECT RAISE(IGNORE); END",
            "recording the unit",
            Silenced::Commit(|| step(3, vec![put("p", "c", "1", Expect::Absent)])),
        ),
        (
            "CREATE TRIGGER main.silence BEFORE INSERT ON main.store_entries \
             BEGIN SELECT RAISE(IGNORE); END",
            "writing a key",
            Silenced::Commit(|| step(3, vec![put("p", "c", "1", Expect::Absent)])),
        ),
        (
            "CREATE TRIGGER main.silence BEFORE UPDATE ON main.store_entries \
             BEGIN SELECT RAISE(IGNORE); END",
            "writing a key",
            Silenced::Commit(|| step(3, vec![put("p", "a", "2", Expect::Anything)])),
        ),
        (
            "CREATE TRIGGER main.silence BEFORE DELETE ON main.store_entries \
             BEGIN SELECT RAISE(IGNORE); END",
            "deleting a key",
            Silenced::Commit(|| step(3, vec![delete("p", "a", Expect::Anything)])),
        ),
        (
            "CREATE TRIGGER main.silence BEFORE INSERT ON main.store_writes \
             BEGIN SELECT RAISE(IGNORE); END",
            "recording a key's images",
            Silenced::Commit(|| step(3, vec![put("p", "c", "1", Expect::Absent)])),
        ),
        (
            "CREATE TRIGGER main.silence BEFORE INSERT ON main.store_notes \
             BEGIN SELECT RAISE(IGNORE); END",
            "recording a note",
            Silenced::Commit(|| UnitRecord {
                notes: vec!["2".to_owned()],
                ..step(3, Vec::new())
            }),
        ),
        (
            "CREATE TRIGGER main.silence BEFORE INSERT ON main.blobs \
             BEGIN SELECT RAISE(IGNORE); END",
            "recording a blob",
            Silenced::Commit(|| UnitRecord {
                blobs: vec![BlobCreated {
                    hash: hash('c'),
                    size: 1,
                }],
                ..step(3, Vec::new())
            }),
        ),
        (
            "CREATE TRIGGER main.silence BEFORE UPDATE ON main.blobs \
             BEGIN SELECT RAISE(IGNORE); END",
            "pinning a blob again",
            Silenced::Commit(|| UnitRecord {
                blobs: vec![BlobCreated {
                    hash: hash('e'),
                    size: 3,
                }],
                ..step(3, Vec::new())
            }),
        ),
        // **The pin a commit takes off a blob**, which no other case drives:
        // without this one, a dropped row count at that write would pass the
        // list — an undo said to have let go of its step's blobs while they
        // stay pinned.
        (
            "CREATE TRIGGER main.silence BEFORE UPDATE ON main.blobs \
             BEGIN SELECT RAISE(IGNORE); END",
            "unpinning a blob",
            Silenced::Commit(|| UnitRecord {
                unpinned: vec![hash('a')],
                ..step(3, Vec::new())
            }),
        ),
        (
            "CREATE TRIGGER main.silence BEFORE UPDATE ON main.store_units \
             BEGIN SELECT RAISE(IGNORE); END",
            "changing a unit's state",
            Silenced::Commit(|| UnitRecord {
                unit: unit(3),
                meta: meta("ana", UnitKind::Undo { target: unit(2) }, 3),
                writes: vec![delete("p", "b", Expect::Value(Some("1".to_owned())))],
                blobs: Vec::new(),
                notes: Vec::new(),
                unpinned: Vec::new(),
                marks: vec![(unit(2), UnitState::Undone { by: unit(3) })],
            }),
        ),
    ]
}

/// The other writes a trigger can silence, each the store's call of its own.
const fn the_other_writes_a_trigger_can_silence() -> [(&'static str, &'static str, Silenced); 6] {
    [
        (
            "CREATE TRIGGER main.silence BEFORE UPDATE ON main.store_units \
             BEGIN SELECT RAISE(IGNORE); END",
            "changing a unit's state",
            Silenced::SetState,
        ),
        (
            "CREATE TRIGGER main.silence BEFORE DELETE ON main.store_units \
             BEGIN SELECT RAISE(IGNORE); END",
            "forgetting units",
            Silenced::ForgetBeyond,
        ),
        (
            "CREATE TRIGGER main.silence BEFORE UPDATE ON main.blobs \
             BEGIN SELECT RAISE(IGNORE); END",
            "pinning a blob",
            Silenced::SetPinned,
        ),
        (
            "CREATE TRIGGER main.silence BEFORE DELETE ON main.blobs \
             BEGIN SELECT RAISE(IGNORE); END",
            "forgetting a blob",
            Silenced::ForgetBlob,
        ),
        (
            "CREATE TRIGGER main.silence BEFORE INSERT ON main.store_unit_holds \
             BEGIN SELECT RAISE(IGNORE); END",
            "holding a unit from the pruner",
            Silenced::CommitHeld,
        ),
        (
            "CREATE TRIGGER main.silence BEFORE DELETE ON main.store_unit_holds \
             BEGIN SELECT RAISE(IGNORE); END",
            "letting go of a unit's holds",
            Silenced::Release,
        ),
    ]
}

/// One silenced write, tried on ground of its own.
async fn tried_through_a_trigger(
    index: usize,
    trigger: &'static str,
    doing: &str,
    silenced: Silenced,
) {
    let scratch = Scratch::new(&format!("silenced-{index}"));
    let profile = scratch.profile();
    let store = the_ground_a_silenced_write_meets(&profile).await;
    let mut planter = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(profile.join(nc_local::FILE_NAME))
        .connect()
        .await
        .expect("the file opens");
    sqlx::query(trigger)
        .execute(&mut planter)
        .await
        .expect("whoever can write the file can add a trigger");
    planter.close().await.expect("closes");
    let before = everything_in(&store).await;

    let answered = match silenced {
        Silenced::Commit(record) => match store.commit(&record(), &ROOMY).await {
            Ok(committed) => panic!(
                "{doing}: {committed:?} was handed out through a trigger that \
                 silenced the write"
            ),
            Err(CommitError::Store(error)) => Err(error),
            Err(other) => panic!("{doing}: not a store failure: {other:?}"),
        },
        Silenced::SetState => store
            .set_state(unit(1), UnitState::Undone { by: unit(2) })
            .await
            .map(|_| ()),
        Silenced::ForgetBeyond => store
            .forget_beyond(&id("home"), &id("ana"), 1)
            .await
            .map(|_| ()),
        Silenced::SetPinned => store.set_pinned(&hash('a'), false, 200).await.map(|_| ()),
        Silenced::ForgetBlob => store.forget_blob(&hash('e')).await.map(|_| ()),
        Silenced::CommitHeld => {
            let held = step(3, vec![put("p", "c", "1", Expect::Absent)]);
            match store.commit_held(&held, &ROOMY).await {
                Ok(committed) => panic!(
                    "{doing}: {committed:?} was handed out through a trigger that silenced \
                     the hold"
                ),
                Err(CommitError::Store(error)) => Err(error),
                Err(other) => panic!("{doing}: not a store failure: {other:?}"),
            }
        }
        Silenced::Release => store.release(unit(2)).await.map(|_| ()),
    };
    let error = answered.expect_err(&format!("{doing}: a silenced write was reported as done"));
    assert_eq!(
        error.code(),
        ErrorCode::DbLocalSchemaUnexpected,
        "{doing}: {error}"
    );
    assert!(
        error
            .to_string()
            .contains(&format!("{doing} changed 0 row(s)")),
        "{doing}: {error}"
    );
    assert_eq!(
        everything_in(&store).await,
        before,
        "{doing}: a refused write left something behind"
    );
    store.close().await;
}

/// A trigger planted in the file while the store is closed is refused at the
/// next open whatever it is called — the name of a table or an index of the
/// list among them.
///
/// **SQLite keeps triggers in a namespace of their own**: a self-check that
/// matched objects by name alone would open a file holding a trigger called
/// `blobs`, rewriting every value committed, and answer a commit of `1` that
/// reads back as `"planted"`.
#[tokio::test]
async fn a_trigger_planted_under_a_listed_name_is_refused_at_open() {
    const PLANTS: [&str; 3] = [
        "CREATE TRIGGER rewrite AFTER INSERT ON store_entries BEGIN \
         UPDATE store_entries SET value = '\"planted\"' WHERE realm = new.realm \
         AND plugin = new.plugin AND store_key = new.store_key; END",
        "CREATE TRIGGER store_units_by_principal AFTER INSERT ON store_entries BEGIN \
         UPDATE store_entries SET value = '\"planted\"' WHERE realm = new.realm \
         AND plugin = new.plugin AND store_key = new.store_key; END",
        "CREATE TRIGGER blobs AFTER INSERT ON store_entries BEGIN \
         UPDATE store_entries SET value = '\"planted\"' WHERE realm = new.realm \
         AND plugin = new.plugin AND store_key = new.store_key; END",
    ];
    for (index, plant) in PLANTS.iter().enumerate() {
        let scratch = Scratch::new(&format!("namesake-{index}"));
        let profile = scratch.profile();
        LocalStore::open(&profile)
            .await
            .expect("opens")
            .close()
            .await;
        let mut planter = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(profile.join(nc_local::FILE_NAME))
            .connect()
            .await
            .expect("the file opens");
        sqlx::query(*plant)
            .execute(&mut planter)
            .await
            .expect("whoever can write the file can add a trigger");
        planter.close().await.expect("closes");
        let opened = LocalStore::open(&profile).await;
        let error = match opened {
            Ok(store) => {
                store.close().await;
                panic!("{plant}: the file opened past its self-check");
            }
            Err(error) => error,
        };
        assert_eq!(error.code(), ErrorCode::DbLocalSchemaUnexpected, "{error}");
        assert!(
            error
                .to_string()
                .contains("is not one `local-migrations/` builds"),
            "{error}"
        );
    }
}

/// A write a trigger in `main` silences is refused, at every place the store
/// writes, and the store is left exactly as it was.
///
/// **A trigger added to `main` after the store opened is one the open-time
/// self-check cannot see**, and `main.` qualification does not stop it. With
/// the store open, `BEFORE INSERT … RAISE(IGNORE)` planted on `store_entries`
/// through a connection of its own and the row count's check dropped at the
/// one call that writes a key, `commit` would hand out a revision for a key it
/// never wrote. The count can be produced from outside, so every statement the
/// store writes with is driven here through a trigger that silences it, and
/// each must come back `NC-DB-0007` naming the write and having changed
/// nothing.
#[tokio::test]
async fn a_write_a_trigger_silences_is_refused_and_leaves_nothing() {
    let mut tried = 0_usize;
    for (index, (trigger, doing, silenced)) in the_commits_a_trigger_can_silence()
        .into_iter()
        .chain(the_other_writes_a_trigger_can_silence())
        .enumerate()
    {
        tried_through_a_trigger(index, trigger, doing, silenced).await;
        tried += 1;
    }
    assert_eq!(
        tried, 16,
        "every statement the store writes with is one case"
    );
}

// ---------------------------------------------------------------------------
// The kill test
// ---------------------------------------------------------------------------

/// The environment variable that turns this test into its own child.
const KILL_AT: &str = "NC_LOCAL_KILL_AT";
/// Where the child finds the database.
const KILL_IN: &str = "NC_LOCAL_KILL_IN";

/// What the child commits: a rewrite, a new key, a delete, a blob and notes.
fn the_unit_the_child_commits() -> UnitRecord {
    UnitRecord {
        blobs: vec![BlobCreated {
            hash: hash('b'),
            size: 9,
        }],
        notes: vec![r#"{"a":1}"#.to_owned(), r#"{"b":2}"#.to_owned()],
        ..step(
            20,
            vec![
                put(
                    "p",
                    "kept",
                    "2",
                    Expect::Revision(Revision {
                        unit: unit(10),
                        n: 0,
                    }),
                ),
                put("p", "new", "3", Expect::Absent),
                delete("p", "gone", Expect::Anything),
            ],
        )
    }
}

/// What the database holds of the child's unit: `Some(true)` whole,
/// `Some(false)` absent, `None` a half.
async fn whole_or_absent(profile: &Path) -> Option<bool> {
    let store = LocalStore::open(profile)
        .await
        .expect("the file opens, and its self-check passes, after the kill");
    let (units, writes, notes, _) = traces_of(&store, unit(20)).await;
    let kept = store
        .get(&home(), "p", "kept")
        .await
        .expect("reads")
        .map(|entry| entry.value);
    let new = store
        .get(&home(), "p", "new")
        .await
        .expect("reads")
        .map(|entry| entry.value);
    let gone = store
        .get(&home(), "p", "gone")
        .await
        .expect("reads")
        .map(|entry| entry.value);
    let blob = store.blob(&hash('b')).await.expect("reads").is_some();
    store.close().await;
    let absent = (units, writes, notes) == (0, 0, 0)
        && kept.as_deref() == Some("1")
        && new.is_none()
        && gone.as_deref() == Some("1")
        && !blob;
    let whole = (units, writes, notes) == (1, 3, 2)
        && kept.as_deref() == Some("2")
        && new.as_deref() == Some("3")
        && gone.is_none()
        && blob;
    match (absent, whole) {
        (true, false) => Some(false),
        (false, true) => Some(true),
        _ => None,
    }
}

/// A process killed between any two statements of a commit leaves the unit
/// absent or whole, never half — and one killed just after it leaves it whole.
///
/// **The test runs itself as a child**, once for every statement the commit
/// makes: the child opens the database and commits through the door that calls
/// `std::process::abort()` before its `k`-th statement — no destructor, no
/// rollback, nothing flushed — and the parent opens the file afterwards. The
/// count runs until a child finishes without being stopped, so the last
/// stopped child is the one killed after `COMMIT` returned.
#[tokio::test]
async fn a_commit_killed_anywhere_leaves_the_unit_absent_or_whole() {
    if let (Ok(at), Ok(profile)) = (std::env::var(KILL_AT), std::env::var(KILL_IN)) {
        let store = LocalStore::open(Path::new(&profile))
            .await
            .expect("the child opens");
        let at: usize = at.parse().expect("a count");
        store
            .commit_then_abort_for_tests(&the_unit_the_child_commits(), &ROOMY, at)
            .await
            .expect("the child's commit");
        store.close().await;
        return;
    }
    let scratch = Scratch::new("kill");
    let profile = scratch.profile();
    let store = LocalStore::open(&profile).await.expect("opens");
    store
        .commit(
            &step(
                10,
                vec![
                    put("p", "kept", "1", Expect::Absent),
                    put("p", "gone", "1", Expect::Absent),
                ],
            ),
            &ROOMY,
        )
        .await
        .expect("the ground the child writes over");
    store.close().await;

    let mut killed = 0_usize;
    let mut last_was_whole = false;
    for at in 0..200 {
        let status = the_child(&profile, at);
        let held = whole_or_absent(&profile).await;
        let Some(whole) = held else {
            panic!("a child stopped before statement {at} left half a unit behind");
        };
        if status.success() {
            assert!(whole, "a child that finished left the unit absent");
            break;
        }
        assert_eq!(
            std::os::unix::process::ExitStatusExt::signal(&status),
            Some(6),
            "the child did not stop the way `abort` stops it: {status}"
        );
        killed += 1;
        last_was_whole = whole;
        if whole {
            // Killed after `COMMIT`: durable, and nothing further to kill.
            break;
        }
    }
    assert!(
        last_was_whole,
        "no child was stopped after its commit returned"
    );
    assert!(
        killed >= 12,
        "only {killed} children were stopped, fewer than the statements this unit makes"
    );
}

/// Runs this test in a child that stops before statement `at`.
#[expect(
    clippy::disallowed_methods,
    reason = "the kill test's child is this same test binary, run with its output \
              discarded — the one way to stop a process mid-commit and look at the file \
              it leaves"
)]
fn the_child(profile: &Path, at: usize) -> std::process::ExitStatus {
    std::process::Command::new(std::env::current_exe().expect("this test binary"))
        .args([
            "--exact",
            "a_commit_killed_anywhere_leaves_the_unit_absent_or_whole",
            "--test-threads=1",
        ])
        .env(KILL_AT, at.to_string())
        .env(KILL_IN, profile)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("the child runs")
}

// ---------------------------------------------------------------------------
// The model
// ---------------------------------------------------------------------------

/// One thing done to the store.
#[derive(Debug, Clone)]
enum Operation {
    /// Commit these writes: (plugin, key, what to expect, value or delete).
    Commit(Vec<(usize, usize, Expecting, Option<i64>)>),
    /// Read one key.
    Get(usize, usize),
    /// Page through one plugin's keys under a prefix.
    List(usize, usize, u32),
}

/// What a write expects, before it is resolved against the model.
#[derive(Debug, Clone, Copy)]
enum Expecting {
    Anything,
    Absent,
    /// The revision the model holds, or one it does not when the key is absent.
    Current,
    /// A revision that is never the current one.
    Stale,
    /// The value the model holds.
    Value,
    /// A value it does not hold.
    OtherValue,
}

const PLUGINS: [&str; 2] = ["p", "q"];
const KEYS: [&str; 7] = ["a", "a/1", "a/2", "ab", "b", "é", "a\u{10FFFF}"];
const PREFIXES: [&str; 5] = ["", "a", "a/", "b", "é"];
/// Each plugin's own quota, in the order of [`PLUGINS`]: `q`'s manifest lowers
/// the catalog's five to three, so a unit writing for both meets two numbers.
const QUOTAS: [u64; 2] = [5, 3];

/// The quota the model holds a plugin to.
fn quota_of(plugin: &str) -> u64 {
    PLUGINS
        .iter()
        .position(|named| *named == plugin)
        .map_or(QUOTAS[0], |at| QUOTAS[at])
}

fn operation() -> impl Strategy<Value = Operation> {
    let expecting = prop_oneof![
        Just(Expecting::Anything),
        Just(Expecting::Absent),
        Just(Expecting::Current),
        Just(Expecting::Stale),
        Just(Expecting::Value),
        Just(Expecting::OtherValue),
    ];
    let write = (
        0..PLUGINS.len(),
        0..KEYS.len(),
        expecting,
        proptest::option::of(0_i64..50),
    );
    prop_oneof![
        3 => proptest::collection::vec(write, 0..5).prop_map(Operation::Commit),
        1 => (0..PLUGINS.len(), 0..KEYS.len()).prop_map(|(plugin, key)| Operation::Get(plugin, key)),
        1 => (0..PLUGINS.len(), 0..PREFIXES.len(), 0_u32..4)
            .prop_map(|(plugin, prefix, limit)| Operation::List(plugin, prefix, limit)),
    ]
}

/// The store as a map: what it holds, and nothing about how.
type Model = BTreeMap<(String, String), Entry>;

/// The model's answer to one commit, and what the store must then hold.
fn committed_in_model(model: &Model, n: u64, writes: &[StagedWrite]) -> Result<Model, CommitError> {
    let mut next = model.clone();
    let mut grew: Vec<&str> = Vec::new();
    for (index, write) in writes.iter().enumerate() {
        let key = (write.plugin.clone(), write.key.clone());
        let held = model.get(&key);
        let holds = match &write.expect {
            Expect::Anything => true,
            Expect::Absent => held.is_none(),
            Expect::Revision(revision) => held.is_some_and(|entry| entry.revision == *revision),
            Expect::Value(value) => held.map(|entry| &entry.value) == value.as_ref(),
        };
        if !holds {
            return Err(CommitError::Conflict {
                plugin: write.plugin.clone(),
                key: write.key.clone(),
            });
        }
        match &write.after {
            Some(value) => {
                if held.is_none() && !grew.contains(&write.plugin.as_str()) {
                    grew.push(&write.plugin);
                }
                next.insert(
                    key,
                    Entry {
                        value: value.clone(),
                        revision: Revision {
                            unit: unit(n),
                            n: u32::try_from(index).expect("small"),
                        },
                    },
                );
            }
            None => {
                next.remove(&key);
            }
        }
    }
    for plugin in grew {
        let held = next.keys().filter(|(owner, _)| owner == plugin).count();
        if u64::try_from(held).expect("small") > quota_of(plugin) {
            return Err(CommitError::Quota {
                plugin: plugin.to_owned(),
            });
        }
    }
    Ok(next)
}

/// The model's page, where a limit of zero is a page of one.
///
/// **A page of one, not the store's flaw copied**: a model drawn from `1..4`
/// that takes none and says none are left, as a flawed store does, cannot see a
/// limit of zero answer `next: None` over keys it has not listed.
fn listed_in_model(
    model: &Model,
    plugin: &str,
    prefix: &str,
    limit: u32,
    cursor: Option<&str>,
) -> (Vec<String>, Option<String>) {
    let all: Vec<String> = model
        .keys()
        .filter(|(owner, key)| {
            owner == plugin
                && key.starts_with(prefix)
                && cursor.is_none_or(|after| key.as_str() > after)
        })
        .map(|(_, key)| key.clone())
        .collect();
    let limit = usize::try_from(limit.max(1)).expect("small");
    let page: Vec<String> = all.iter().take(limit).cloned().collect();
    let next = if all.len() > limit {
        page.last().cloned()
    } else {
        None
    };
    (page, next)
}

/// Resolves a write's expectation against the model, as a kernel would have read.
fn resolved(model: &Model, plugin: &str, key: &str, expecting: Expecting) -> Expect {
    let held = model.get(&(plugin.to_owned(), key.to_owned()));
    match expecting {
        Expecting::Anything => Expect::Anything,
        Expecting::Absent => Expect::Absent,
        Expecting::Current => Expect::Revision(held.map_or(
            Revision {
                unit: unit(1),
                n: 0,
            },
            |entry| entry.revision,
        )),
        Expecting::Stale => Expect::Revision(Revision {
            unit: unit(1),
            n: 99,
        }),
        Expecting::Value => Expect::Value(held.map(|entry| entry.value.clone())),
        Expecting::OtherValue => Expect::Value(Some("-1".to_owned())),
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, ..ProptestConfig::default() })]

    /// The store answers every sequence of commits, reads and pages the way a
    /// `BTreeMap` does, and a refused commit leaves it exactly as it was.
    #[test]
    fn the_store_answers_as_its_model_does(operations in proptest::collection::vec(operation(), 1..40)) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        runtime.block_on(async {
            let store = LocalStore::in_memory_for_tests().await.expect("opens");
            let quotas = Quotas {
                entries_per_plugin: QUOTAS[0],
                lowered: BTreeMap::from([(PLUGINS[1].to_owned(), QUOTAS[1])]),
                blobs: 10,
                writes_per_unit: 10,
                notes_per_unit: 10,
            };
            let mut model: Model = BTreeMap::new();
            let mut next_unit = 100_u64;
            for operation in operations {
                match operation {
                    Operation::Commit(planned) => {
                        next_unit += 1;
                        let mut writes: Vec<StagedWrite> = Vec::new();
                        for (plugin, key, expecting, value) in planned {
                            let (plugin, key) = (PLUGINS[plugin], KEYS[key]);
                            if writes.iter().any(|write| write.plugin == plugin && write.key == key) {
                                continue;
                            }
                            writes.push(StagedWrite {
                                plugin: plugin.to_owned(),
                                key: key.to_owned(),
                                expect: resolved(&model, plugin, key, expecting),
                                after: value.map(|value| value.to_string()),
                            });
                        }
                        let expected = committed_in_model(&model, next_unit, &writes);
                        let answered = store.commit(&step(next_unit, writes), &quotas).await;
                        match (expected, answered) {
                            (Ok(next), Ok(_)) => model = next,
                            (Err(expected), Err(answered)) => prop_assert_eq!(answered, expected),
                            (expected, answered) => prop_assert!(false, "the model says {:?} and the store {:?}", expected, answered),
                        }
                    }
                    Operation::Get(plugin, key) => {
                        let held = store.get(&home(), PLUGINS[plugin], KEYS[key]).await.expect("reads");
                        prop_assert_eq!(held.as_ref(), model.get(&(PLUGINS[plugin].to_owned(), KEYS[key].to_owned())));
                    }
                    Operation::List(plugin, prefix, limit) => {
                        let mut cursor: Option<String> = None;
                        loop {
                            let page = store
                                .list(&home(), PLUGINS[plugin], PREFIXES[prefix], limit, cursor.as_deref())
                                .await
                                .expect("lists");
                            let (keys, next) = listed_in_model(&model, PLUGINS[plugin], PREFIXES[prefix], limit, cursor.as_deref());
                            prop_assert_eq!(&page.keys, &keys);
                            prop_assert_eq!(&page.next, &next);
                            if next.is_none() {
                                break;
                            }
                            cursor = next;
                        }
                    }
                }
            }
            for plugin in PLUGINS {
                let held = store.count(&home(), plugin).await.expect("counts");
                let modelled = model.keys().filter(|(owner, _)| owner == plugin).count();
                prop_assert_eq!(held, u64::try_from(modelled).expect("small"));
            }
            Ok(())
        })?;
    }
}
