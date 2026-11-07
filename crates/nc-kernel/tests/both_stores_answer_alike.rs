//! The store held in memory answers every call as the local database does —
//! the same values, the same refusals, the same sentences.
//!
//! **One script, two stores, one transcript.** Each script below runs through
//! `MemoryBackend` and through `nc_local::LocalStore` over the same units, and
//! writes down every answer; the two transcripts must be equal, and each script
//! also asserts what the answers are, so two stores wrong in the same way fail
//! as surely as two that differ. The property test drives both through random
//! sequences of every operation and holds each answer of one to the other's,
//! and the committed keys to a map besides.
//!
//! Both stores are driven through `nc_kernel::backend::StoreBackend`, the trait
//! the kernel reads them through, so what is compared is what the kernel sees.

use std::collections::BTreeMap;

use nc_authz::{MemoryDirectory, Pdp, PrincipalKind};
use nc_ids::{MachineFlake, Minter, OpaqueId, Snowflake, SystemClock};
use nc_kernel::backend::{MemoryBackend, StoreBackend};
use nc_local::{
    BlobCreated, CommitError, Entry, Expect, LocalError, LocalStore, Quotas, Revision, StagedWrite,
    UnitKind, UnitMeta, UnitRecord, UnitState,
};
use proptest::prelude::*;

/// The world every policy case is about, which mints the identifiers a unit is
/// committed under.
const WORLD: &str = include_str!("../../../policy/world.json");

/// What both runs of a script share: who its units are for, and the units.
///
/// **Two realms, because a filter is only tested by what it leaves out.** With
/// one realm and every unit a step, a store that listed and forgot a person's
/// units in every realm, and one that recorded each redo as an undo, would
/// answer every script and every sequence here as the local database does,
/// both planted at once — measured.
struct Fixture {
    realm: OpaqueId,
    office: OpaqueId,
    ana: OpaqueId,
    bruno: OpaqueId,
    units: Vec<Snowflake>,
}

impl Fixture {
    /// Identifiers the directory minted, and `units` units minted in order.
    fn new(units: usize) -> Self {
        let pdp = Pdp::embedded("family").expect("the family pack is carried");
        let directory = MemoryDirectory::from_world(WORLD, &pdp).expect("the world is valid");
        let principal = |name: &str| {
            directory
                .principal(PrincipalKind::User, name)
                .expect("a user of the world")
                .id()
                .clone()
        };
        let mut minter = Minter::new(MachineFlake::from_value(2).expect("a flake"), SystemClock);
        Self {
            realm: directory.realm_id("home").expect("home is a realm"),
            office: directory.realm_id("office").expect("office is a realm"),
            ana: principal("ana"),
            bruno: principal("bruno"),
            units: (0..units).map(|_| minter.mint().expect("a unit")).collect(),
        }
    }

    /// The `n`th unit: a step by `principal`, committing `writes`.
    fn unit(&self, n: usize, principal: &OpaqueId, writes: Vec<StagedWrite>) -> UnitRecord {
        UnitRecord {
            unit: self.units[n],
            meta: UnitMeta {
                realm: self.realm.clone(),
                principal: principal.clone(),
                kind: UnitKind::Step,
                label: format!("unit {n}"),
                committed_at_ms: 1_000 + u64::try_from(n).expect("small"),
            },
            writes,
            blobs: Vec::new(),
            notes: Vec::new(),
            unpinned: Vec::new(),
            marks: Vec::new(),
        }
    }

    /// The `n`th unit's revision of its write at `at`.
    fn revision(&self, n: usize, at: u32) -> Revision {
        Revision {
            unit: self.units[n],
            n: at,
        }
    }
}

/// Room for everything a script does not mean to bound.
fn roomy() -> Quotas {
    Quotas {
        entries_per_plugin: 100,
        lowered: BTreeMap::new(),
        blobs: 100,
        writes_per_unit: 100,
        notes_per_unit: 100,
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

fn hash(fill: char) -> String {
    core::iter::repeat_n(fill, 64).collect()
}

/// Every answer of a script, written down in order.
#[derive(Debug, Default, PartialEq, Eq)]
struct Transcript(Vec<String>);

impl Transcript {
    /// Writes down `answer` under `what`, and hands it back.
    fn said<T: core::fmt::Debug>(&mut self, what: &str, answer: T) -> T {
        self.0.push(format!("{what}: {answer:?}"));
        answer
    }
}

/// A fresh local database in memory.
async fn the_local_database() -> LocalStore {
    LocalStore::in_memory_for_tests()
        .await
        .expect("the local database opens in memory")
}

/// Runs `script` through a store in memory and through the local database,
/// over one fixture, and holds the two transcripts to each other.
macro_rules! through_both {
    ($script:ident, $units:expr) => {{
        let fixture = Fixture::new($units);
        let mut in_memory = Transcript::default();
        $script(&MemoryBackend::default(), &fixture, &mut in_memory).await;
        let mut in_the_file = Transcript::default();
        $script(&the_local_database().await, &fixture, &mut in_the_file).await;
        assert!(
            in_memory.0.len() > 5,
            "a script that says this little proves little: {in_memory:?}"
        );
        assert_eq!(
            in_memory, in_the_file,
            "the store in memory and the local database answered one script differently"
        );
    }};
}

/// A unit's writes, images, notes and blobs, read back, and a second unit
/// written against what the first left.
async fn a_unit_read_back(store: &impl StoreBackend, fixture: &Fixture, said: &mut Transcript) {
    let mut first = fixture.unit(
        0,
        &fixture.ana,
        vec![
            put("p", "b", "2", Expect::Absent),
            put("p", "a", r#"{"x":1}"#, Expect::Anything),
            delete("p", "gone", Expect::Absent),
            put("q", "a", "3", Expect::Value(None)),
        ],
    );
    first.notes = vec![r#"{"note":1}"#.to_owned(), "2".to_owned()];
    first.blobs = vec![BlobCreated {
        hash: hash('a'),
        size: 5,
    }];
    let committed = said
        .said("commit", store.commit(&first, &roomy()).await)
        .expect("the unit commits");
    assert_eq!(
        committed.revisions,
        [
            Some(fixture.revision(0, 0)),
            Some(fixture.revision(0, 1)),
            None,
            Some(fixture.revision(0, 3)),
        ],
        "one revision per write, numbered in order, none for a delete"
    );
    assert_eq!(
        said.said("get p/a", store.get(&fixture.realm, "p", "a").await),
        Ok(Some(Entry {
            value: r#"{"x":1}"#.to_owned(),
            revision: fixture.revision(0, 1),
        }))
    );
    assert_eq!(
        said.said("get q/b", store.get(&fixture.realm, "q", "b").await),
        Ok(None)
    );
    assert_eq!(
        said.said("get r/a", store.get(&fixture.realm, "r", "a").await),
        Ok(None)
    );
    assert_eq!(
        said.said("count p", store.count(&fixture.realm, "p").await),
        Ok(2)
    );
    let detail = said
        .said("unit 0", store.unit(fixture.units[0]).await)
        .expect("answers")
        .expect("held");
    assert_eq!(
        detail
            .images
            .iter()
            .map(|image| (image.plugin.as_str(), image.key.as_str()))
            .collect::<Vec<_>>(),
        [("p", "a"), ("p", "b"), ("p", "gone"), ("q", "a")],
        "images in key order"
    );
    assert_eq!(detail.notes, first.notes);
    assert_eq!(detail.row.state, UnitState::Committed);
    let blob = said
        .said("blob a", store.blob(&hash('a')).await)
        .expect("answers")
        .expect("held");
    assert!(blob.pinned && blob.created_unit == fixture.units[0] && blob.size == 5);
    written_against_the_first(store, fixture, said).await;
}

/// The second unit of [`a_unit_read_back`]: written against the revision the
/// first left, with the first's value as its before-image.
async fn written_against_the_first(
    store: &impl StoreBackend,
    fixture: &Fixture,
    said: &mut Transcript,
) {
    let second = fixture.unit(
        1,
        &fixture.ana,
        vec![put("p", "a", "4", Expect::Revision(fixture.revision(0, 1)))],
    );
    said.said("commit 1", store.commit(&second, &roomy()).await)
        .expect("written against the revision it read");
    let image = said
        .said("unit 1", store.unit(fixture.units[1]).await)
        .expect("answers")
        .expect("held")
        .images
        .remove(0);
    assert_eq!(
        image.before.map(|entry| entry.value).as_deref(),
        Some(r#"{"x":1}"#)
    );
    assert_eq!(
        said.said("unit none", store.unit(fixture.units[2]).await),
        Ok(None)
    );
}

#[tokio::test]
async fn a_committed_unit_is_read_back_whole_from_both_stores_alike() {
    through_both!(a_unit_read_back, 3);
}

/// One commit a store must refuse: what it is, the unit, and how to recognise
/// the refusal.
struct Refusal {
    what: &'static str,
    unit: UnitRecord,
    expected: fn(&CommitError) -> bool,
}

/// The quotas every refusal is held to: a plugin's own lowered to one, the
/// catalog's to two, one blob, three images and one note per unit.
fn tight() -> Quotas {
    Quotas {
        entries_per_plugin: 2,
        lowered: BTreeMap::from([("q".to_owned(), 1)]),
        blobs: 1,
        writes_per_unit: 3,
        notes_per_unit: 1,
    }
}

/// The refusals of a key's expectation, a quota and the bounds one unit may
/// record, over a store holding the key `a` of the plugin `p` from unit 0.
fn refused_writes(fixture: &Fixture) -> Vec<Refusal> {
    let stale = Expect::Revision(fixture.revision(5, 0));
    let unit = |writes| fixture.unit(1, &fixture.ana, writes);
    vec![
        Refusal {
            what: "a stale revision",
            unit: unit(vec![
                put("p", "b", "1", Expect::Absent),
                put("p", "a", "2", stale),
            ]),
            expected: |error| matches!(error, CommitError::Conflict { key, .. } if key == "a"),
        },
        Refusal {
            what: "a key that is held, expected absent",
            unit: unit(vec![put("p", "a", "2", Expect::Absent)]),
            expected: |error| matches!(error, CommitError::Conflict { .. }),
        },
        Refusal {
            what: "a value that is not the one held",
            unit: unit(vec![delete("p", "a", Expect::Value(Some("7".to_owned())))]),
            expected: |error| matches!(error, CommitError::Conflict { .. }),
        },
        Refusal {
            what: "past the plugin's own quota",
            unit: unit(vec![
                put("q", "a", "1", Expect::Absent),
                put("q", "b", "1", Expect::Absent),
            ]),
            expected: |error| matches!(error, CommitError::Quota { plugin } if plugin == "q"),
        },
        Refusal {
            what: "past the catalog's quota",
            unit: unit(vec![
                put("p", "b", "1", Expect::Absent),
                put("p", "c", "1", Expect::Absent),
            ]),
            expected: |error| matches!(error, CommitError::Quota { plugin } if plugin == "p"),
        },
        Refusal {
            what: "a key written twice",
            unit: unit(vec![
                put("p", "z", "1", Expect::Anything),
                put("p", "z", "2", Expect::Anything),
            ]),
            expected: |error| matches!(error, CommitError::Invalid(why) if why.contains("written twice")),
        },
        Refusal {
            what: "a value that is not canonical",
            unit: unit(vec![put("p", "z", "{ }", Expect::Anything)]),
            expected: |error| matches!(error, CommitError::Invalid(why) if why.contains("not its canonical form")),
        },
        Refusal {
            what: "an expected value that is not JSON",
            unit: unit(vec![put(
                "p",
                "z",
                "1",
                Expect::Value(Some("{".to_owned())),
            )]),
            expected: |error| matches!(error, CommitError::Invalid(why) if why.contains("not JSON")),
        },
        Refusal {
            what: "more images than a unit records",
            unit: unit(
                (0..4)
                    .map(|n| put("r", &format!("k{n}"), "1", Expect::Anything))
                    .collect(),
            ),
            expected: |error| matches!(error, CommitError::Invalid(why) if why.contains("store_writes")),
        },
    ]
}

/// The refusals of a unit's notes, blobs, time and marks, and of a unit the
/// store already holds.
fn refused_records(fixture: &Fixture) -> Vec<Refusal> {
    let with = |change: &dyn Fn(&mut UnitRecord)| {
        let mut unit = fixture.unit(1, &fixture.ana, Vec::new());
        change(&mut unit);
        unit
    };
    let blobs = |fills: &[char]| {
        fills
            .iter()
            .map(|fill| BlobCreated {
                hash: hash(*fill),
                size: 1,
            })
            .collect::<Vec<_>>()
    };
    vec![
        Refusal {
            what: "more notes than a unit records",
            unit: with(&|unit| unit.notes = vec!["1".to_owned(), "2".to_owned()]),
            expected: |error| matches!(error, CommitError::Invalid(why) if why.contains("store_notes")),
        },
        Refusal {
            what: "a note that is not canonical",
            unit: with(&|unit| unit.notes = vec![" 1".to_owned()]),
            expected: |error| matches!(error, CommitError::Invalid(why) if why.contains("a note")),
        },
        Refusal {
            what: "a blob hash that is not one",
            unit: with(&|unit| unit.blobs = blobs(&['g'])),
            expected: |error| matches!(error, CommitError::Invalid(why) if why.contains("not a BLAKE3 hash")),
        },
        Refusal {
            what: "a blob recorded twice",
            unit: with(&|unit| unit.blobs = blobs(&['b', 'b'])),
            expected: |error| matches!(error, CommitError::Invalid(why) if why.contains("recorded twice")),
        },
        Refusal {
            what: "more blobs than the machine holds",
            unit: with(&|unit| unit.blobs = blobs(&['b', 'c'])),
            expected: |error| matches!(error, CommitError::TooManyBlobs),
        },
        Refusal {
            what: "a blob past a signed integer",
            unit: with(&|unit| {
                unit.blobs = vec![BlobCreated {
                    hash: hash('d'),
                    size: u64::MAX,
                }];
            }),
            expected: |error| matches!(error, CommitError::Store(LocalError::Unexpected(why)) if why.contains("a blob's size")),
        },
        Refusal {
            what: "a pin taken off what is not a blob's hash",
            unit: with(&|unit| unit.unpinned = vec![hash('g')]),
            expected: |error| matches!(error, CommitError::Invalid(why) if why.contains("not a BLAKE3 hash")),
        },
        Refusal {
            what: "a pin taken off one blob twice",
            unit: with(&|unit| unit.unpinned = vec![hash('a'), hash('a')]),
            expected: |error| matches!(error, CommitError::Invalid(why) if why.contains("unpinned twice")),
        },
        Refusal {
            what: "a pin taken off a blob the same unit pins",
            unit: with(&|unit| {
                unit.blobs = blobs(&['a']);
                unit.unpinned = vec![hash('a')];
            }),
            expected: |error| matches!(error, CommitError::Invalid(why) if why.contains("pinned and unpinned")),
        },
        Refusal {
            what: "a pin taken off a blob the store does not hold",
            unit: with(&|unit| {
                unit.writes = vec![put("p", "n", "1", Expect::Anything)];
                unit.unpinned = vec![hash('f')];
            }),
            expected: |error| matches!(error, CommitError::Invalid(why) if why.contains("unpinned by unit")),
        },
        Refusal {
            what: "a time past a signed integer",
            unit: with(&|unit| unit.meta.committed_at_ms = u64::MAX),
            expected: |error| matches!(error, CommitError::Store(LocalError::Unexpected(why)) if why.contains("a time")),
        },
        Refusal {
            what: "a mark on a unit the store does not hold",
            unit: with(&|unit| {
                unit.writes = vec![put("p", "m", "1", Expect::Anything)];
                unit.marks = vec![(fixture.units[4], UnitState::Committed)];
            }),
            expected: |error| matches!(error, CommitError::Invalid(why) if why.contains("does not hold it")),
        },
        Refusal {
            what: "a unit committed twice",
            unit: fixture.unit(0, &fixture.ana, vec![put("p", "a", "1", Expect::Absent)]),
            expected: |error| matches!(error, CommitError::Invalid(why) if why.contains("commits once")),
        },
    ]
}

/// Every refusal a commit makes, each followed by proof that nothing changed.
async fn every_refusal(store: &impl StoreBackend, fixture: &Fixture, said: &mut Transcript) {
    let base = fixture.unit(0, &fixture.ana, vec![put("p", "a", "1", Expect::Absent)]);
    said.said("base", store.commit(&base, &roomy()).await)
        .expect("the base commits");
    let refusals: Vec<Refusal> = refused_writes(fixture)
        .into_iter()
        .chain(refused_records(fixture))
        .collect();
    assert!(refusals.len() > 15, "{} refusals", refusals.len());
    for refusal in &refusals {
        let what = refusal.what;
        let refused = said
            .said(what, store.commit(&refusal.unit, &tight()).await)
            .expect_err(what);
        assert!((refusal.expected)(&refused), "{what}: {refused:?}");
        assert_eq!(
            said.said("count p", store.count(&fixture.realm, "p").await),
            Ok(1),
            "{what} left a key"
        );
        assert_eq!(
            said.said("count q", store.count(&fixture.realm, "q").await),
            Ok(0),
            "{what}"
        );
        assert_eq!(
            said.said("count r", store.count(&fixture.realm, "r").await),
            Ok(0),
            "{what}"
        );
        assert_eq!(
            said.said("unit 1", store.unit(fixture.units[1]).await),
            Ok(None),
            "{what} left its unit"
        );
        assert_eq!(said.said("blob b", store.blob(&hash('b')).await), Ok(None));
    }
}

#[tokio::test]
async fn every_refusal_of_a_commit_is_the_same_in_both_stores_and_leaves_nothing() {
    through_both!(every_refusal, 6);
}

/// What `p`'s quota is after its manifest lowered it to two, below the keys
/// [`over_a_lowered_quota`] commits for it first.
fn lowered_to_two() -> Quotas {
    Quotas {
        lowered: BTreeMap::from([("p".to_owned(), 2)]),
        ..roomy()
    }
}

/// A plugin holding more keys than its quota: it rewrites and deletes what it
/// holds, and is refused a new key until it is back within the bound.
///
/// **Only a new key counts against a quota**, and every other script and
/// sequence holds a plugin to one number from its first commit, so no plugin
/// there holds more than its quota — which a manifest lowering `store_entries`
/// below what its plugin holds makes. With either store counting a rewrite as
/// a new key, the two would answer a rewrite over the bound one with a refusal
/// and the other with a commit — measured — which this script tells apart.
async fn over_a_lowered_quota(store: &impl StoreBackend, fixture: &Fixture, said: &mut Transcript) {
    let held = ["a", "b", "c", "d"].map(|key| put("p", key, "1", Expect::Absent));
    said.said(
        "commit 0",
        store
            .commit(&fixture.unit(0, &fixture.ana, held.to_vec()), &roomy())
            .await,
    )
    .expect("four keys fit a roomy quota");
    let lowered = lowered_to_two();
    let unit = |n: usize, writes: Vec<StagedWrite>| fixture.unit(n, &fixture.ana, writes);
    let quota = || CommitError::Quota {
        plugin: "p".to_owned(),
    };
    said.said(
        "rewrite over the bound",
        store
            .commit(
                &unit(1, vec![put("p", "a", "2", Expect::Anything)]),
                &lowered,
            )
            .await,
    )
    .expect("a key the plugin holds is not a new key");
    said.said(
        "delete over the bound",
        store
            .commit(&unit(2, vec![delete("p", "d", Expect::Anything)]), &lowered)
            .await,
    )
    .expect("a delete never counts against a quota");
    assert_eq!(
        said.said("count p", store.count(&fixture.realm, "p").await),
        Ok(3)
    );
    assert_eq!(
        said.said(
            "new key over the bound",
            store
                .commit(&unit(3, vec![put("p", "e", "1", Expect::Absent)]), &lowered)
                .await
        ),
        Err(quota()),
        "three held and a fourth refused, under a quota of two"
    );
    assert_eq!(
        said.said(
            "room for one over the bound",
            store
                .commit(
                    &unit(
                        3,
                        vec![
                            delete("p", "c", Expect::Anything),
                            put("p", "e", "1", Expect::Absent),
                        ]
                    ),
                    &lowered
                )
                .await
        ),
        Err(quota()),
        "a key deleted for a new one still leaves three, over a quota of two"
    );
    back_within_a_lowered_quota(store, fixture, said).await;
}

/// The second half of [`over_a_lowered_quota`]: back at its bound, a plugin
/// that makes room for a key has room, and the lowered number is its own.
async fn back_within_a_lowered_quota(
    store: &impl StoreBackend,
    fixture: &Fixture,
    said: &mut Transcript,
) {
    let lowered = lowered_to_two();
    let unit = |n: usize, writes: Vec<StagedWrite>| fixture.unit(n, &fixture.ana, writes);
    assert_eq!(
        said.said("unit 3", store.unit(fixture.units[3]).await),
        Ok(None),
        "a refused unit left nothing"
    );
    said.said(
        "delete to the bound",
        store
            .commit(&unit(3, vec![delete("p", "c", Expect::Anything)]), &lowered)
            .await,
    )
    .expect("a delete commits");
    said.said(
        "room for one at the bound",
        store
            .commit(
                &unit(
                    4,
                    vec![
                        delete("p", "b", Expect::Anything),
                        put("p", "e", "1", Expect::Absent),
                    ],
                ),
                &lowered,
            )
            .await,
    )
    .expect("a key deleted for a new one keeps two, within a quota of two");
    said.said(
        "another plugin",
        store
            .commit(&unit(5, vec![put("q", "a", "1", Expect::Absent)]), &lowered)
            .await,
    )
    .expect("the lowered number is the plugin's own");
    assert_eq!(
        said.said("count p", store.count(&fixture.realm, "p").await),
        Ok(2)
    );
    assert_eq!(
        said.said("get p/a", store.get(&fixture.realm, "p", "a").await)
            .map(|held| held.map(|entry| entry.value)),
        Ok(Some("2".to_owned())),
        "the rewrite over the bound is what the key holds"
    );
}

#[tokio::test]
async fn a_plugin_over_a_lowered_quota_rewrites_and_deletes_but_gains_no_key_alike_in_both_stores()
{
    through_both!(over_a_lowered_quota, 6);
}

/// The keys [`pages`] pages through.
const PAGED: [&str; 7] = ["a", "a/1", "a/2", "ab", "b", "é", "a\u{10FFFF}"];

/// Every page of one plugin's keys under each prefix, by each limit.
async fn pages(store: &impl StoreBackend, fixture: &Fixture, said: &mut Transcript) {
    let first = fixture.unit(
        0,
        &fixture.ana,
        PAGED
            .iter()
            .map(|key| put("p", key, "1", Expect::Anything))
            .chain([put("q", "a/1", "1", Expect::Anything)])
            .collect(),
    );
    said.said("commit 0", store.commit(&first, &roomy()).await)
        .expect("commits");
    for prefix in ["", "a", "a/", "b", "é", "z"] {
        for limit in [0, 1, 2, 10] {
            let mut cursor: Option<String> = None;
            let mut seen = Vec::new();
            loop {
                let page = said
                    .said(
                        &format!("list {prefix:?} {limit} {cursor:?}"),
                        store
                            .list(&fixture.realm, "p", prefix, limit, cursor.as_deref())
                            .await,
                    )
                    .expect("lists");
                seen.extend(page.keys.iter().cloned());
                if page.next.is_none() {
                    break;
                }
                cursor = page.next;
            }
            let mut expected: Vec<&str> = PAGED
                .iter()
                .copied()
                .filter(|key| key.starts_with(prefix))
                .collect();
            expected.sort_unstable();
            assert_eq!(seen, expected, "{prefix:?} by {limit}");
        }
    }
}

#[tokio::test]
async fn every_page_of_keys_is_the_same_in_both_stores() {
    through_both!(pages, 1);
}

/// A person's units, newest first, their states and their forgetting.
async fn units(store: &impl StoreBackend, fixture: &Fixture, said: &mut Transcript) {
    for n in 0..5 {
        let principal = if n % 2 == 0 && n > 0 {
            &fixture.bruno
        } else {
            &fixture.ana
        };
        let mut unit = fixture.unit(
            n,
            principal,
            vec![put("p", "u", &n.to_string(), Expect::Anything)],
        );
        if n == 4 {
            unit.meta.kind = UnitKind::Undo {
                target: fixture.units[3],
            };
            unit.marks = vec![(
                fixture.units[3],
                UnitState::Undone {
                    by: fixture.units[4],
                },
            )];
        }
        said.said(&format!("commit {n}"), store.commit(&unit, &roomy()).await)
            .expect("commits");
    }
    let ana: Vec<Snowflake> = said
        .said(
            "units ana",
            store.units(&fixture.realm, &fixture.ana, 10).await,
        )
        .expect("lists")
        .iter()
        .map(|row| row.unit)
        .collect();
    assert_eq!(
        ana,
        [fixture.units[3], fixture.units[1], fixture.units[0]],
        "newest first"
    );
    assert_eq!(
        said.said(
            "units ana 1",
            store.units(&fixture.realm, &fixture.ana, 1).await
        )
        .map(|rows| rows.len()),
        Ok(1)
    );
    assert_eq!(
        said.said(
            "units ana 0",
            store.units(&fixture.realm, &fixture.ana, 0).await
        ),
        Ok(Vec::new())
    );
    assert_eq!(
        said.said(
            "steps ana",
            store.steps(&fixture.realm, &fixture.ana, 10).await
        )
        .map(|rows| rows.iter().map(|row| row.unit).collect::<Vec<_>>()),
        Ok(vec![fixture.units[3], fixture.units[1], fixture.units[0]]),
        "a person's steps, newest first"
    );
    let undone = said
        .said("unit 3", store.unit(fixture.units[3]).await)
        .expect("answers")
        .expect("held");
    assert_eq!(
        undone.row.state,
        UnitState::Undone {
            by: fixture.units[4]
        },
        "the undo marked its target in its own commit"
    );
    states_and_forgetting(store, fixture, said).await;
}

/// The second half of [`units`]: a state set and refused, and a person's
/// oldest units forgotten while what they wrote stays.
async fn states_and_forgetting(
    store: &impl StoreBackend,
    fixture: &Fixture,
    said: &mut Transcript,
) {
    assert_eq!(
        said.said(
            "set 3",
            store
                .set_state(fixture.units[3], UnitState::Committed)
                .await
        ),
        Ok(true)
    );
    assert_eq!(
        said.said(
            "set 5",
            store
                .set_state(fixture.units[5], UnitState::Committed)
                .await
        ),
        Ok(false)
    );
    assert_eq!(
        said.said(
            "forget ana",
            store.forget_beyond(&fixture.realm, &fixture.ana, 1).await
        ),
        Ok(2)
    );
    assert_eq!(
        said.said(
            "forget ana again",
            store.forget_beyond(&fixture.realm, &fixture.ana, 1).await
        ),
        Ok(0)
    );
    assert_eq!(
        said.said("unit 0", store.unit(fixture.units[0]).await),
        Ok(None)
    );
    assert_eq!(
        said.said("get p/u", store.get(&fixture.realm, "p", "u").await)
            .map(|held| held.map(|entry| entry.value)),
        Ok(Some("4".to_owned())),
        "forgetting a unit keeps what it wrote"
    );
    assert_eq!(
        said.said(
            "units bruno",
            store.units(&fixture.realm, &fixture.bruno, 10).await
        )
        .map(|rows| rows.len()),
        Ok(2)
    );
}

#[tokio::test]
async fn units_and_their_states_are_the_same_in_both_stores() {
    through_both!(units, 6);
}

/// Who and what each of `rows` is: its unit, its realm and its kind.
fn rows_of(
    rows: Result<Vec<nc_local::UnitRow>, LocalError>,
) -> Result<Vec<(Snowflake, OpaqueId, UnitKind)>, LocalError> {
    rows.map(|rows| {
        rows.into_iter()
            .map(|row| (row.unit, row.meta.realm, row.meta.kind))
            .collect()
    })
}

/// A step, an undo and a redo, each read back as its own kind.
///
/// A person's units in one realm are listed and forgotten apart from theirs
/// in another, and their steps listed apart from their undos and redos.
async fn kinds_and_realms(store: &impl StoreBackend, fixture: &Fixture, said: &mut Transcript) {
    let kinds = [
        UnitKind::Step,
        UnitKind::Undo {
            target: fixture.units[0],
        },
        UnitKind::Redo {
            target: fixture.units[1],
        },
    ];
    for (n, kind) in kinds.into_iter().enumerate() {
        let mut unit = fixture.unit(
            n,
            &fixture.ana,
            vec![put("p", "k", &n.to_string(), Expect::Anything)],
        );
        unit.meta.kind = kind;
        said.said(&format!("commit {n}"), store.commit(&unit, &roomy()).await)
            .expect("commits");
        let read = said
            .said(&format!("unit {n}"), store.unit(fixture.units[n]).await)
            .expect("answers")
            .expect("held");
        assert_eq!(
            read.row.meta.kind, kind,
            "unit {n} is read back as its kind"
        );
    }
    let mut elsewhere = fixture.unit(3, &fixture.ana, vec![put("p", "e", "3", Expect::Absent)]);
    elsewhere.meta.realm = fixture.office.clone();
    said.said(
        "commit 3 in office",
        store.commit(&elsewhere, &roomy()).await,
    )
    .expect("commits");
    let home = |n: usize| (fixture.units[n], fixture.realm.clone(), kinds[n]);
    assert_eq!(
        rows_of(said.said(
            "units ana",
            store.units(&fixture.realm, &fixture.ana, 10).await
        )),
        Ok(vec![home(2), home(1), home(0)]),
        "a person's units in one realm, each as its kind, and none of another's"
    );
    assert_eq!(
        rows_of(said.said(
            "units ana in office",
            store.units(&fixture.office, &fixture.ana, 10).await
        )),
        Ok(vec![(
            fixture.units[3],
            fixture.office.clone(),
            UnitKind::Step
        )]),
        "the other realm lists its own unit and none of the first's"
    );
    // **A person's steps, and the bound counts them alone**: one step asked
    // for is the step, though an undo and a redo are newer than it.
    for limit in [10, 1] {
        assert_eq!(
            rows_of(said.said(
                &format!("steps ana {limit}"),
                store.steps(&fixture.realm, &fixture.ana, limit).await
            )),
            Ok(vec![home(0)]),
            "a person's steps in one realm, and none of their undos and redos"
        );
    }
    assert_eq!(
        said.said(
            "steps ana 0",
            store.steps(&fixture.realm, &fixture.ana, 0).await
        ),
        Ok(Vec::new())
    );
    assert_eq!(
        rows_of(said.said(
            "steps ana in office",
            store.steps(&fixture.office, &fixture.ana, 10).await
        )),
        Ok(vec![(
            fixture.units[3],
            fixture.office.clone(),
            UnitKind::Step
        )]),
        "the other realm's step and none of the first's"
    );
    forgotten_in_one_realm(store, fixture, said).await;
}

/// The second half of [`kinds_and_realms`]: forgetting a person's units in one
/// realm forgets none of theirs in another, nor anybody else's.
async fn forgotten_in_one_realm(
    store: &impl StoreBackend,
    fixture: &Fixture,
    said: &mut Transcript,
) {
    assert_eq!(
        said.said(
            "forget ana in office",
            store.forget_beyond(&fixture.office, &fixture.ana, 0).await
        ),
        Ok(1),
        "the one unit of that realm, and none of the other's"
    );
    assert_eq!(
        said.said("unit 3", store.unit(fixture.units[3]).await),
        Ok(None)
    );
    assert_eq!(
        said.said(
            "forget bruno",
            store.forget_beyond(&fixture.realm, &fixture.bruno, 0).await
        ),
        Ok(0),
        "nothing of another person's"
    );
    assert_eq!(
        said.said(
            "units ana after",
            store.units(&fixture.realm, &fixture.ana, 10).await
        )
        .map(|rows| rows.len()),
        Ok(3),
        "forgetting in one realm left the other's units"
    );
    let redo = said
        .said("unit 2", store.unit(fixture.units[2]).await)
        .expect("answers")
        .expect("held");
    assert_eq!(
        redo.row.meta.kind,
        UnitKind::Redo {
            target: fixture.units[1]
        }
    );
}

#[tokio::test]
async fn every_kind_of_unit_and_every_realm_are_kept_apart_alike_in_both_stores() {
    through_both!(kinds_and_realms, 4);
}

/// Whether a committed unit other than one holds a note: any realm's unit
/// of any kind does, and no unit undone does.
///
/// **What an undo asks before it takes a blob's pin off**, so each filter is
/// varied here: the unit asked about, a note that is not exactly the one
/// asked, a step undone, an undo committed and then redone, a redo, and a
/// step of another realm and person.
async fn notes_held_elsewhere(store: &impl StoreBackend, fixture: &Fixture, said: &mut Transcript) {
    let (x, y, z) = (r#"{"note":"x"}"#, r#"{"note":"y"}"#, r#"{"note":"z"}"#);
    let noted = |n: usize, note: &str| {
        let mut unit = fixture.unit(
            n,
            &fixture.ana,
            vec![put("p", &format!("k{n}"), "1", Expect::Anything)],
        );
        unit.notes = vec![note.to_owned()];
        unit
    };
    let none = fixture.units[5];
    said.said("commit 0", store.commit(&noted(0, x), &roomy()).await)
        .expect("commits");
    assert_eq!(
        said.said("x but 0", store.noted_elsewhere(x, fixture.units[0]).await),
        Ok(false),
        "only the unit asked about holds it"
    );
    assert_eq!(
        said.said("x", store.noted_elsewhere(x, none).await),
        Ok(true)
    );
    for other in [r#"{"note": "x"}"#, z] {
        assert_eq!(
            said.said(other, store.noted_elsewhere(other, none).await),
            Ok(false),
            "a note that is not exactly the one asked"
        );
    }
    let mut undo = noted(1, y);
    undo.meta.kind = UnitKind::Undo {
        target: fixture.units[0],
    };
    undo.marks = vec![(
        fixture.units[0],
        UnitState::Undone {
            by: fixture.units[1],
        },
    )];
    said.said("commit 1", store.commit(&undo, &roomy()).await)
        .expect("commits");
    assert_eq!(
        said.said("x after the undo", store.noted_elsewhere(x, none).await),
        Ok(false),
        "a step undone holds nothing"
    );
    assert_eq!(
        said.said("y", store.noted_elsewhere(y, none).await),
        Ok(true),
        "an undo still committed holds its notes, as a step does"
    );
    let mut redo = noted(2, z);
    redo.meta.kind = UnitKind::Redo {
        target: fixture.units[1],
    };
    redo.marks = vec![(
        fixture.units[1],
        UnitState::Undone {
            by: fixture.units[2],
        },
    )];
    said.said("commit 2", store.commit(&redo, &roomy()).await)
        .expect("commits");
    assert_eq!(
        said.said("y after the redo", store.noted_elsewhere(y, none).await),
        Ok(false),
        "an undo redone holds nothing"
    );
    assert_eq!(
        said.said("z", store.noted_elsewhere(z, none).await),
        Ok(true),
        "a redo holds its notes"
    );
    let mut elsewhere = noted(3, x);
    elsewhere.meta.realm = fixture.office.clone();
    elsewhere.meta.principal = fixture.bruno.clone();
    said.said("commit 3", store.commit(&elsewhere, &roomy()).await)
        .expect("commits");
    assert_eq!(
        said.said("x in office", store.noted_elsewhere(x, none).await),
        Ok(true),
        "a step of another realm and person holds it"
    );
}

#[tokio::test]
async fn whether_another_unit_holds_a_note_is_answered_alike_in_both_stores() {
    through_both!(notes_held_elsewhere, 6);
}

/// Holding units from the pruner and letting go of them: a unit committed
/// held holds itself and every unit it marks, its release answers how many it
/// placed, and the holders are listed oldest first, of every realm.
async fn holds(store: &impl StoreBackend, fixture: &Fixture, said: &mut Transcript) {
    let step = fixture.unit(0, &fixture.ana, vec![put("p", "a", "1", Expect::Absent)]);
    said.said("commit 0", store.commit(&step, &roomy()).await)
        .expect("commits");
    assert_eq!(said.said("none", store.holders().await), Ok(Vec::new()));
    let mut undo = fixture.unit(1, &fixture.ana, vec![delete("p", "a", Expect::Anything)]);
    undo.meta.kind = UnitKind::Undo {
        target: fixture.units[0],
    };
    undo.marks = vec![(
        fixture.units[0],
        UnitState::Undone {
            by: fixture.units[1],
        },
    )];
    said.said("commit 1 held", store.commit_held(&undo, &roomy()).await)
        .expect("commits");
    let mut elsewhere = fixture.unit(2, &fixture.bruno, vec![put("p", "b", "1", Expect::Absent)]);
    elsewhere.meta.realm = fixture.office.clone();
    said.said(
        "commit 2 held",
        store.commit_held(&elsewhere, &roomy()).await,
    )
    .expect("commits");
    assert_eq!(
        said.said("held", store.holders().await),
        Ok(vec![fixture.units[1], fixture.units[2]])
    );
    assert_eq!(
        said.said("release 1", store.release(fixture.units[1]).await),
        Ok(2),
        "an undo holds itself and the step it marks"
    );
    assert_eq!(
        said.said("release 1 again", store.release(fixture.units[1]).await),
        Ok(0)
    );
    assert_eq!(
        said.said("still held", store.holders().await),
        Ok(vec![fixture.units[2]])
    );
    assert_eq!(
        said.said("release 2", store.release(fixture.units[2]).await),
        Ok(1)
    );
    assert_eq!(said.said("let go", store.holders().await), Ok(Vec::new()));
}

#[tokio::test]
async fn units_are_held_and_let_go_alike_in_both_stores() {
    through_both!(holds, 3);
}

/// A person's units forgotten past one a kernel still holds from the pruner:
/// the whole delete is refused, nothing forgotten, in the same sentence.
async fn forgetting_past_a_held_unit(
    store: &impl StoreBackend,
    fixture: &Fixture,
    said: &mut Transcript,
) {
    let first = fixture.unit(0, &fixture.ana, vec![put("p", "a", "1", Expect::Absent)]);
    said.said("commit 0 held", store.commit_held(&first, &roomy()).await)
        .expect("commits");
    let second = fixture.unit(1, &fixture.ana, vec![put("p", "b", "1", Expect::Absent)]);
    said.said("commit 1", store.commit(&second, &roomy()).await)
        .expect("commits");
    assert!(
        said.said(
            "forget past the held",
            store.forget_beyond(&fixture.realm, &fixture.ana, 1).await,
        )
        .is_err(),
        "a held unit was forgotten"
    );
    assert_eq!(
        said.said(
            "unit 0 kept",
            store
                .unit(fixture.units[0])
                .await
                .map(|held| held.is_some()),
        ),
        Ok(true)
    );
    assert_eq!(
        said.said(
            "units ana",
            store
                .units(&fixture.realm, &fixture.ana, 10)
                .await
                .map(|rows| rows.len()),
        ),
        Ok(2)
    );
    assert_eq!(
        said.said("holders", store.holders().await),
        Ok(vec![fixture.units[0]])
    );
    assert_eq!(
        said.said("release 0", store.release(fixture.units[0]).await),
        Ok(1)
    );
    assert_eq!(
        said.said(
            "forget past it, let go",
            store.forget_beyond(&fixture.realm, &fixture.ana, 1).await,
        ),
        Ok(1)
    );
}

/// Forgetting past a unit still held is refused alike by both stores.
///
/// **What it holds against**: a store in memory that forgets the unit the
/// file's `ON DELETE RESTRICT` keeps, and holds it from the pruner afterwards
/// though it has no such unit.
#[tokio::test]
async fn forgetting_past_a_held_unit_is_answered_alike_in_both_stores() {
    through_both!(forgetting_past_a_held_unit, 2);
}

/// Pinning, unpinning, the sweep's list and forgetting a blob.
async fn blobs(store: &impl StoreBackend, fixture: &Fixture, said: &mut Transcript) {
    let mut unit = fixture.unit(0, &fixture.ana, Vec::new());
    unit.blobs = ['a', 'b', 'c']
        .into_iter()
        .map(|fill| BlobCreated {
            hash: hash(fill),
            size: 3,
        })
        .collect();
    said.said("commit 0", store.commit(&unit, &roomy()).await)
        .expect("commits");
    for (fill, at) in [('b', 20), ('a', 20), ('c', 10)] {
        assert_eq!(
            said.said(
                &format!("unpin {fill}"),
                store.set_pinned(&hash(fill), false, at).await
            ),
            Ok(true)
        );
    }
    assert_eq!(
        said.said("unpin absent", store.set_pinned(&hash('e'), false, 1).await),
        Ok(false)
    );
    assert!(matches!(
        said.said(
            "unpin past",
            store.set_pinned(&hash('a'), false, u64::MAX).await
        ),
        Err(LocalError::Unexpected(_))
    ));
    assert_eq!(
        said.said("before 21", store.unpinned_before(21, 10).await),
        Ok(vec![hash('c'), hash('a'), hash('b')]),
        "oldest first, then by hash"
    );
    assert_eq!(
        said.said("before 20", store.unpinned_before(20, 10).await),
        Ok(vec![hash('c')])
    );
    assert_eq!(
        said.said("before 21 by 1", store.unpinned_before(21, 1).await),
        Ok(vec![hash('c')])
    );
    assert!(matches!(
        said.said("before past", store.unpinned_before(u64::MAX, 1).await),
        Err(LocalError::Unexpected(_))
    ));
    forgotten_and_pinned_again(store, fixture, said).await;
}

/// The second half of [`blobs`]: a pinned blob is not forgotten, an unpinned
/// one is, and a unit naming a held blob pins it again.
async fn forgotten_and_pinned_again(
    store: &impl StoreBackend,
    fixture: &Fixture,
    said: &mut Transcript,
) {
    assert_eq!(
        said.said("pin b", store.set_pinned(&hash('b'), true, 99).await),
        Ok(true)
    );
    assert_eq!(
        said.said("forget b", store.forget_blob(&hash('b')).await),
        Ok(false)
    );
    assert_eq!(
        said.said("forget a", store.forget_blob(&hash('a')).await),
        Ok(true)
    );
    assert_eq!(
        said.said("forget a again", store.forget_blob(&hash('a')).await),
        Ok(false)
    );
    assert_eq!(said.said("blob a", store.blob(&hash('a')).await), Ok(None));
    let mut again = fixture.unit(1, &fixture.ana, Vec::new());
    again.blobs = vec![BlobCreated {
        hash: hash('c'),
        size: 9,
    }];
    said.said("commit 1", store.commit(&again, &roomy()).await)
        .expect("a held blob is pinned again");
    let row = said
        .said("blob c", store.blob(&hash('c')).await)
        .expect("answers")
        .expect("held");
    assert!(
        row.pinned && row.unpinned_at_ms.is_none() && row.created_unit == fixture.units[0],
        "pinned again, and still the first unit's: {row:?}"
    );
    // A unit takes a pin off as of when it commits, and leaves the others.
    let mut reversal = fixture.unit(2, &fixture.ana, Vec::new());
    reversal.unpinned = vec![hash('c')];
    said.said("commit 2", store.commit(&reversal, &roomy()).await)
        .expect("a pin taken off");
    let row = said
        .said("blob c unpinned", store.blob(&hash('c')).await)
        .expect("answers")
        .expect("held");
    assert_eq!(
        (row.pinned, row.unpinned_at_ms),
        (false, Some(reversal.meta.committed_at_ms))
    );
    let row = said
        .said("blob b kept", store.blob(&hash('b')).await)
        .expect("answers")
        .expect("held");
    assert!(row.pinned, "{row:?}");
}

#[tokio::test]
async fn blobs_are_pinned_swept_and_forgotten_alike_in_both_stores() {
    through_both!(blobs, 3);
}

// ---------------------------------------------------------------------------
// The model
// ---------------------------------------------------------------------------

/// One commit the property test makes: as `ana` or not, in the realm `home`
/// or not, of a kind, with these writes —
/// (plugin, key, what to expect, value or delete) — these blobs and notes,
/// and a mark on a unit, undone or committed.
#[derive(Debug, Clone)]
struct Planned {
    ana: bool,
    home: bool,
    kind: Kinded,
    writes: Vec<(usize, usize, Expecting, Option<i64>)>,
    blobs: Vec<usize>,
    notes: Vec<usize>,
    mark: Option<(usize, bool)>,
}

/// What kind of unit a commit is, its target named by its place among the
/// fixture's units.
#[derive(Debug, Clone, Copy)]
enum Kinded {
    Step,
    Undo(usize),
    Redo(usize),
}

/// One thing done to both stores.
#[derive(Debug, Clone)]
enum Operation {
    /// Commit a unit.
    Commit(Planned),
    /// Commit a unit, holding it from the pruner.
    CommitHeld(Planned),
    /// Let go of the units one holds, and list who holds any.
    Release(usize),
    /// Read one key.
    Get(usize, usize),
    /// Page through one plugin's keys under a prefix.
    List(usize, usize, u32),
    /// Count one plugin's keys.
    Count(usize),
    /// List one person's units in one realm.
    Units(bool, bool, u32),
    /// List one person's steps in one realm.
    Steps(bool, bool, u32),
    /// Read one unit.
    Unit(usize),
    /// Set one unit's state.
    SetState(usize, bool),
    /// Forget one person's units in one realm beyond a number.
    ForgetBeyond(bool, bool, u32),
    /// Ask whether a committed unit other than one holds a note.
    NotedElsewhere(usize, usize),
    /// Read one blob.
    Blob(usize),
    /// Pin or unpin one blob as of a time.
    SetPinned(usize, bool, u64),
    /// List the blobs unpinned before a time.
    UnpinnedBefore(u64, u32),
    /// Forget one blob.
    ForgetBlob(usize),
}

/// What a write expects, before it is resolved against what is held.
#[derive(Debug, Clone, Copy)]
enum Expecting {
    Anything,
    Absent,
    Current,
    Stale,
    Value,
    OtherValue,
}

const PLUGINS: [&str; 2] = ["p", "q"];
const KEYS: [&str; 6] = ["a", "a/1", "ab", "b", "é", "a\u{10FFFF}"];
const PREFIXES: [&str; 4] = ["", "a", "a/", "é"];
/// The blobs a unit may create, the last of which is not a hash at all.
const FILLS: [char; 4] = ['a', 'b', 'c', 'x'];
/// The notes a unit may make, the last of which is not canonical.
const NOTES: [&str; 3] = ["1", r#"{"n":2}"#, " 3"];
/// How many units the property test may commit, and the one more it never
/// does, which marks and stale revisions name.
const UNITS: usize = 12;

fn planned() -> impl Strategy<Value = Planned> {
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
        proptest::option::of(0_i64..20),
    );
    let kind = prop_oneof![
        Just(Kinded::Step),
        (0..UNITS).prop_map(Kinded::Undo),
        (0..UNITS).prop_map(Kinded::Redo),
    ];
    (
        (any::<bool>(), any::<bool>(), kind),
        proptest::collection::vec(write, 0..4),
        proptest::collection::vec(0..FILLS.len(), 0..3),
        proptest::collection::vec(0..NOTES.len(), 0..3),
        proptest::option::of((0..UNITS, any::<bool>())),
    )
        .prop_map(|((ana, home, kind), writes, blobs, notes, mark)| Planned {
            ana,
            home,
            kind,
            writes,
            blobs,
            notes,
            mark,
        })
}

fn operation() -> impl Strategy<Value = Operation> {
    prop_oneof![
        6 => planned().prop_map(Operation::Commit),
        2 => planned().prop_map(Operation::CommitHeld),
        1 => (0..UNITS).prop_map(Operation::Release),
        2 => (0..PLUGINS.len(), 0..KEYS.len()).prop_map(|(plugin, key)| Operation::Get(plugin, key)),
        1 => (0..PLUGINS.len(), 0..PREFIXES.len(), 0_u32..3)
            .prop_map(|(plugin, prefix, limit)| Operation::List(plugin, prefix, limit)),
        1 => (0..PLUGINS.len()).prop_map(Operation::Count),
        1 => (any::<bool>(), any::<bool>(), 0_u32..4)
            .prop_map(|(ana, home, limit)| Operation::Units(ana, home, limit)),
        1 => (any::<bool>(), any::<bool>(), 0_u32..4)
            .prop_map(|(ana, home, limit)| Operation::Steps(ana, home, limit)),
        1 => (0..UNITS).prop_map(Operation::Unit),
        1 => (0..UNITS, any::<bool>()).prop_map(|(unit, undone)| Operation::SetState(unit, undone)),
        1 => (any::<bool>(), any::<bool>(), 0_u32..3)
            .prop_map(|(ana, home, keep)| Operation::ForgetBeyond(ana, home, keep)),
        1 => (0..NOTES.len(), 0..=UNITS)
            .prop_map(|(note, except)| Operation::NotedElsewhere(note, except)),
        1 => (0..FILLS.len()).prop_map(Operation::Blob),
        1 => (0..FILLS.len(), any::<bool>(), 0_u64..30)
            .prop_map(|(blob, pinned, at)| Operation::SetPinned(blob, pinned, at)),
        1 => (0_u64..30, 0_u32..4).prop_map(|(before, limit)| Operation::UnpinnedBefore(before, limit)),
        1 => (0..FILLS.len()).prop_map(Operation::ForgetBlob),
    ]
}

/// Both stores, the units they share, and the map of committed keys they are
/// held to: each by its realm, its plugin and itself.
struct Both {
    memory: MemoryBackend,
    file: LocalStore,
    fixture: Fixture,
    quotas: Quotas,
    model: BTreeMap<(String, String, String), Entry>,
    committed: usize,
}

impl Both {
    async fn new() -> Self {
        Self {
            memory: MemoryBackend::default(),
            file: the_local_database().await,
            fixture: Fixture::new(UNITS + 1),
            quotas: Quotas {
                entries_per_plugin: 4,
                lowered: BTreeMap::from([(PLUGINS[1].to_owned(), 2)]),
                blobs: 2,
                writes_per_unit: 3,
                notes_per_unit: 2,
            },
            model: BTreeMap::new(),
            committed: 0,
        }
    }

    /// The person `ana` names.
    const fn who(&self, ana: bool) -> &OpaqueId {
        if ana {
            &self.fixture.ana
        } else {
            &self.fixture.bruno
        }
    }

    /// The realm `home` names: `home` itself, or `office`.
    const fn realm(&self, home: bool) -> &OpaqueId {
        if home {
            &self.fixture.realm
        } else {
            &self.fixture.office
        }
    }

    /// The unit `planned` describes, its expectations resolved against the map.
    fn unit_of(&self, planned: Planned) -> UnitRecord {
        let next = self.committed;
        let stale = self.fixture.units[UNITS];
        let realm = self.realm(planned.home).as_str();
        let writes = planned
            .writes
            .into_iter()
            .map(|(plugin, key, expecting, value)| {
                let (plugin, key) = (PLUGINS[plugin], KEYS[key]);
                let held = self
                    .model
                    .get(&(realm.to_owned(), plugin.to_owned(), key.to_owned()));
                StagedWrite {
                    plugin: plugin.to_owned(),
                    key: key.to_owned(),
                    expect: match expecting {
                        Expecting::Anything => Expect::Anything,
                        Expecting::Absent => Expect::Absent,
                        Expecting::Current => Expect::Revision(
                            held.map_or(Revision { unit: stale, n: 0 }, |entry| entry.revision),
                        ),
                        Expecting::Stale => Expect::Revision(Revision { unit: stale, n: 99 }),
                        Expecting::Value => Expect::Value(held.map(|entry| entry.value.clone())),
                        Expecting::OtherValue => Expect::Value(Some("-1".to_owned())),
                    },
                    after: value.map(|value| value.to_string()),
                }
            })
            .collect();
        let mut unit = self.fixture.unit(next, self.who(planned.ana), writes);
        unit.meta.realm = self.realm(planned.home).clone();
        unit.meta.kind = match planned.kind {
            Kinded::Step => UnitKind::Step,
            Kinded::Undo(target) => UnitKind::Undo {
                target: self.fixture.units[target],
            },
            Kinded::Redo(target) => UnitKind::Redo {
                target: self.fixture.units[target],
            },
        };
        unit.blobs = planned
            .blobs
            .into_iter()
            .map(|blob| BlobCreated {
                hash: hash(FILLS[blob]),
                size: 1,
            })
            .collect();
        unit.notes = planned
            .notes
            .into_iter()
            .map(|note| NOTES[note].to_owned())
            .collect();
        unit.marks = planned
            .mark
            .map(|(target, undone)| {
                let marked = if undone {
                    UnitState::Undone {
                        by: self.fixture.units[next],
                    }
                } else {
                    UnitState::Committed
                };
                (self.fixture.units[target], marked)
            })
            .into_iter()
            .collect();
        unit
    }

    /// Commits `planned` to both — held from the pruner when `held` — holds
    /// the answers to each other, and keeps the map in step with what was
    /// committed.
    async fn commit(&mut self, planned: Planned, held: bool) -> Result<(), TestCaseError> {
        if self.committed >= UNITS {
            return Ok(());
        }
        let unit = self.unit_of(planned);
        let (in_memory, in_the_file) = if held {
            (
                self.memory.commit_held(&unit, &self.quotas).await,
                self.file.commit_held(&unit, &self.quotas).await,
            )
        } else {
            (
                self.memory.commit(&unit, &self.quotas).await,
                self.file.commit(&unit, &self.quotas).await,
            )
        };
        prop_assert_eq!(&in_memory, &in_the_file, "commit {:?}", unit);
        if let Ok(committed) = in_memory {
            self.committed += 1;
            for (write, revision) in unit.writes.iter().zip(committed.revisions) {
                let slot = (
                    unit.meta.realm.as_str().to_owned(),
                    write.plugin.clone(),
                    write.key.clone(),
                );
                match (&write.after, revision) {
                    (Some(value), Some(revision)) => {
                        self.model.insert(
                            slot,
                            Entry {
                                value: value.clone(),
                                revision,
                            },
                        );
                    }
                    _ => {
                        self.model.remove(&slot);
                    }
                }
            }
        }
        Ok(())
    }

    /// The keys' operations: a read, every page, a count, each in both
    /// realms and each held to the other store and to the map.
    async fn keys(&self, operation: &Operation) -> Result<(), TestCaseError> {
        for home in [true, false] {
            self.keys_in(self.realm(home), operation).await?;
        }
        Ok(())
    }

    /// The keys' operations in `realm`.
    async fn keys_in(&self, realm: &OpaqueId, operation: &Operation) -> Result<(), TestCaseError> {
        match *operation {
            Operation::Get(plugin, key) => {
                let (plugin, key) = (PLUGINS[plugin], KEYS[key]);
                let in_memory = self.memory.get(realm, plugin, key).await;
                prop_assert_eq!(&in_memory, &self.file.get(realm, plugin, key).await);
                prop_assert_eq!(
                    in_memory,
                    Ok(self
                        .model
                        .get(&(realm.as_str().to_owned(), plugin.to_owned(), key.to_owned()))
                        .cloned())
                );
            }
            Operation::List(plugin, prefix, limit) => {
                let (plugin, prefix) = (PLUGINS[plugin], PREFIXES[prefix]);
                let mut cursor: Option<String> = None;
                loop {
                    let in_memory = self
                        .memory
                        .list(realm, plugin, prefix, limit, cursor.as_deref())
                        .await;
                    prop_assert_eq!(
                        &in_memory,
                        &self
                            .file
                            .list(realm, plugin, prefix, limit, cursor.as_deref())
                            .await
                    );
                    match in_memory.map(|page| page.next) {
                        Ok(Some(after)) => cursor = Some(after),
                        _ => break,
                    }
                }
            }
            Operation::Count(plugin) => {
                let in_memory = self.memory.count(realm, PLUGINS[plugin]).await;
                prop_assert_eq!(&in_memory, &self.file.count(realm, PLUGINS[plugin]).await);
                let held = self
                    .model
                    .keys()
                    .filter(|(of, owner, _)| of == realm.as_str() && owner == PLUGINS[plugin])
                    .count();
                prop_assert_eq!(in_memory, Ok(u64::try_from(held).expect("small")));
            }
            _ => {}
        }
        Ok(())
    }

    /// The units' operations, each held to the other store.
    async fn units(&self, operation: &Operation) -> Result<(), TestCaseError> {
        let units = &self.fixture.units;
        match *operation {
            Operation::Units(ana, home, limit) => prop_assert_eq!(
                self.memory
                    .units(self.realm(home), self.who(ana), limit)
                    .await,
                self.file
                    .units(self.realm(home), self.who(ana), limit)
                    .await
            ),
            Operation::Steps(ana, home, limit) => prop_assert_eq!(
                self.memory
                    .steps(self.realm(home), self.who(ana), limit)
                    .await,
                self.file
                    .steps(self.realm(home), self.who(ana), limit)
                    .await
            ),
            Operation::Unit(unit) => prop_assert_eq!(
                self.memory.unit(units[unit]).await,
                self.file.unit(units[unit]).await
            ),
            Operation::SetState(unit, undone) => {
                let marked = if undone {
                    UnitState::Undone { by: units[UNITS] }
                } else {
                    UnitState::Committed
                };
                prop_assert_eq!(
                    self.memory.set_state(units[unit], marked).await,
                    self.file.set_state(units[unit], marked).await
                );
            }
            Operation::ForgetBeyond(ana, home, keep) => prop_assert_eq!(
                self.memory
                    .forget_beyond(self.realm(home), self.who(ana), keep)
                    .await,
                self.file
                    .forget_beyond(self.realm(home), self.who(ana), keep)
                    .await
            ),
            Operation::NotedElsewhere(note, except) => prop_assert_eq!(
                self.memory
                    .noted_elsewhere(NOTES[note], units[except])
                    .await,
                self.file.noted_elsewhere(NOTES[note], units[except]).await
            ),
            Operation::Release(unit) => {
                prop_assert_eq!(
                    self.memory.release(units[unit]).await,
                    self.file.release(units[unit]).await
                );
                prop_assert_eq!(self.memory.holders().await, self.file.holders().await);
            }
            _ => {}
        }
        Ok(())
    }

    /// The blobs' operations, each held to the other store.
    async fn blobs(&self, operation: &Operation) -> Result<(), TestCaseError> {
        match *operation {
            Operation::Blob(blob) => prop_assert_eq!(
                self.memory.blob(&hash(FILLS[blob])).await,
                self.file.blob(&hash(FILLS[blob])).await
            ),
            Operation::SetPinned(blob, pinned, at) => prop_assert_eq!(
                self.memory.set_pinned(&hash(FILLS[blob]), pinned, at).await,
                self.file.set_pinned(&hash(FILLS[blob]), pinned, at).await
            ),
            Operation::UnpinnedBefore(before, limit) => prop_assert_eq!(
                self.memory.unpinned_before(before, limit).await,
                self.file.unpinned_before(before, limit).await
            ),
            Operation::ForgetBlob(blob) => prop_assert_eq!(
                self.memory.forget_blob(&hash(FILLS[blob])).await,
                self.file.forget_blob(&hash(FILLS[blob])).await
            ),
            _ => {}
        }
        Ok(())
    }
}

/// Runs `operations` through both stores.
async fn both_answer(operations: Vec<Operation>) -> Result<(), TestCaseError> {
    let mut both = Both::new().await;
    for operation in operations {
        if let Operation::Commit(planned) | Operation::CommitHeld(planned) = operation.clone() {
            both.commit(planned, matches!(operation, Operation::CommitHeld(_)))
                .await?;
        } else {
            both.keys(&operation).await?;
            both.units(&operation).await?;
            both.blobs(&operation).await?;
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 96, ..ProptestConfig::default() })]

    /// Every sequence of every operation is answered alike by the store in
    /// memory and the local database, and what they hold is what a map of
    /// the committed writes holds.
    #[test]
    fn both_stores_answer_every_sequence_alike(operations in proptest::collection::vec(operation(), 1..40)) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        runtime.block_on(both_answer(operations))?;
    }
}
