//! The decision point: every dispatch is authorized before anything runs and
//! journaled before its answer is returned, and a dispatch that cannot be
//! decided is denied (`NOCTUA.md §11.7`, `§14.2`, acceptance criteria 6 and 7).
//!
//! **Through the real component, the real pack and the real world.** Every
//! dispatch below is decided by the family pack embedded in `nc-authz`, over
//! `policy/world.json`, with grants for `ana`, `bruno`, `carla` and `dora` and
//! the realm `home` at epoch 7, and every command is the reference plugin's,
//! run inside the component `cargo xtask build --plugins` wrote. The store is
//! the local database held in memory and the journal the one in memory —
//! each wrapped so a test can make it refuse what it chooses, the journal
//! also take an entry and lose its answer, or read back a history rewritten
//! after it was chained — and two tests run against PostgreSQL as well, each
//! in a schema of its own.
//!
//! **What a test asserts is what was kept.** An answer is read back as the
//! plugin's store holds it, a refusal as the journal recorded it and the log
//! named it, and a command that was refused before it ran is shown not to
//! have run by the count of what its instance executed.

use core::fmt::Debug;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nc_authz::{
    Directory, DirectoryError, EntityRef, MemoryDirectory, Pdp, Principal, PrincipalKind,
    RealmFacts,
};
use nc_canon::Value;
use nc_catalog::{ErrorCode, KERNEL};
use nc_ids::{MachineFlake, Minter, OpaqueId};
use nc_journal::{Entry, Outcome};
use nc_kernel::backend::StoreBackend;
use nc_kernel::logging::Captured;
use nc_kernel::manifest::PluginManifest;
use nc_kernel::{Caller, Environment, Kernel, KernelConfig, Refusal, RefusalKind};
use nc_local::{Expect, LocalStore, Quotas, StagedWrite, UnitKind, UnitMeta, UnitRecord};
use nc_plugin_reference::behaviour::{ACTIVATION, Activation, Variant};

/// The world every policy case is about.
const WORLD: &str = include_str!("../../../policy/world.json");

/// The instant the policy cases call now, `2026-08-26T00:00:00Z`: inside the
/// grant `g.current`.
const NOW_MS: u64 = 1_787_702_400_000;

/// The epoch the directory holds the realm `home` at.
const EPOCH: u64 = 7;

/// A clock a test moves, and can set to move on by itself at every reading.
#[derive(Debug, Clone)]
struct Manual {
    now: Arc<AtomicU64>,
    step: Arc<AtomicU64>,
}

impl Manual {
    /// A clock reading `ms`, standing still.
    fn at(ms: u64) -> Self {
        Self {
            now: Arc::new(AtomicU64::new(ms)),
            step: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Moves the clock on by `by`.
    fn advance(&self, by: u64) {
        self.now.fetch_add(by, Ordering::SeqCst);
    }

    /// Makes every reading move the clock on by `by` after it.
    fn stepping(&self, by: u64) {
        self.step.store(by, Ordering::SeqCst);
    }
}

impl nc_ids::Clock for Manual {
    fn now_ms(&self) -> u64 {
        self.now
            .fetch_add(self.step.load(Ordering::SeqCst), Ordering::SeqCst)
    }

    fn wait_past(&self, millis: u64) {
        self.now
            .fetch_max(millis.saturating_add(1), Ordering::SeqCst);
    }
}

/// Which entries a [`Failing`] journal store refuses.
type Refuses = fn(&Entry) -> bool;

/// What a [`Failing`] journal store does with what it is given, as a test
/// sets it.
#[derive(Debug, Default)]
struct Told {
    /// The entries it refuses, writing nothing.
    refuses: Option<Refuses>,
    /// The entries it writes and then answers as refused, which is what a
    /// connection lost after the journal's own commit answers.
    lands_then_refuses: Option<Refuses>,
    /// Whether, once it has lost such an answer, it refuses to say how any
    /// unit closed.
    then_says_nothing: bool,
    /// Whether it refuses to say how any unit closed.
    says_nothing: bool,
    /// The one unit it refuses to say how it closed, answering of every other
    /// as it is asked.
    says_nothing_of: Option<nc_ids::Snowflake>,
    /// How it reads its history back.
    reads: Reads,
    /// The entries it stops the whole process at, before writing anything:
    /// what a kernel stopped in the middle of a unit leaves its journal as.
    aborts: Option<Refuses>,
    /// Whether it says of every unit that it is still open, closed or not —
    /// what another writer closing the unit after it was asked looks like.
    forgets_closes: bool,
    /// The units it names as left open, whatever its history holds: an answer
    /// read before another writer closed them.
    lists_open: Option<Vec<nc_ids::Snowflake>>,
}

/// How a [`Failing`] journal store reads its history back, as a test sets it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Reads {
    /// As it was written.
    #[default]
    AsWritten,
    /// With its first entry's action rewritten, as a row edited in place after
    /// it was chained reads back.
    Rewritten,
    /// Not at all: every read is refused, the question of which units are
    /// open among them.
    Refused,
    /// Cut short after this many entries, as a read stopped by a row it
    /// cannot decode is, and saying so.
    CutShort(usize),
}

/// A journal store that refuses the entries a test says, and takes the rest —
/// and, when a test says so, reads its history back rewritten or not at all.
#[derive(Debug, Clone)]
struct Failing<S> {
    inner: S,
    told: Arc<Mutex<Told>>,
}

impl<S: nc_journal::Store + Send + Sync> nc_journal::Store for Failing<S> {
    fn tail(&self) -> impl Future<Output = Result<Option<Entry>, nc_journal::Error>> + Send {
        self.inner.tail()
    }

    fn head(
        &self,
    ) -> impl Future<Output = Result<Option<(u64, [u8; 32])>, nc_journal::Error>> + Send {
        self.inner.head()
    }

    fn append(
        &mut self,
        entry: Entry,
    ) -> impl Future<Output = Result<(), nc_journal::Error>> + Send {
        let (refused, loses_the_answer, stops) = {
            let told = self.told.lock().expect("nothing panics holding it");
            (
                told.refuses.is_some_and(|refuses| refuses(&entry)),
                told.lands_then_refuses.is_some_and(|lands| lands(&entry)),
                told.aborts.is_some_and(|aborts| aborts(&entry)),
            )
        };
        async move {
            if stops {
                std::process::abort();
            }
            if refused {
                return Err(nc_journal::Error::Unavailable(
                    "the journal's store is down, as the test made it".to_owned(),
                ));
            }
            self.inner.append(entry).await?;
            if loses_the_answer {
                let mut told = self.told.lock().expect("nothing panics holding it");
                told.says_nothing = told.then_says_nothing;
                return Err(nc_journal::Error::Unavailable(
                    "the journal's store took the entry and lost its answer, as the test made \
                     it"
                    .to_owned(),
                ));
            }
            Ok(())
        }
    }

    fn from(
        &self,
        sequence: u64,
    ) -> impl Future<Output = Result<nc_journal::Read, nc_journal::Error>> + Send {
        let reads = self.told.lock().expect("nothing panics holding it").reads;
        let read = self.inner.from(sequence);
        async move {
            if reads == Reads::Refused {
                return Err(nc_journal::Error::Unavailable(
                    "the journal's store cannot be read, as the test made it".to_owned(),
                ));
            }
            let mut read = read.await?;
            if reads == Reads::Rewritten
                && let Some(first) = read.entries.first_mut()
            {
                first.record.action.push_str(".rewritten");
            }
            if let Reads::CutShort(kept) = reads
                && read.entries.len() > kept
            {
                read.entries.truncate(kept);
                read.stopped_at = Some((
                    u64::try_from(kept).expect("fits"),
                    "a row this read cannot decode, as the test made it".to_owned(),
                ));
            }
            Ok(read)
        }
    }

    fn unit_closing(
        &self,
        realm: &OpaqueId,
        unit: nc_ids::Snowflake,
    ) -> impl Future<Output = Result<Option<Outcome>, nc_journal::Error>> + Send {
        let (says_nothing, forgets) = {
            let told = self.told.lock().expect("nothing panics holding it");
            (
                told.says_nothing || told.says_nothing_of == Some(unit),
                told.forgets_closes,
            )
        };
        let asked = self.inner.unit_closing(realm, unit);
        async move {
            if says_nothing {
                return Err(nc_journal::Error::Unavailable(
                    "the journal's store cannot say how the unit closed, as the test made it"
                        .to_owned(),
                ));
            }
            if forgets {
                return Ok(None);
            }
            asked.await
        }
    }

    fn unit_opened(
        &self,
        realm: &OpaqueId,
        unit: nc_ids::Snowflake,
    ) -> impl Future<Output = Result<bool, nc_journal::Error>> + Send {
        self.inner.unit_opened(realm, unit)
    }

    fn open_units(
        &self,
        realm: &OpaqueId,
    ) -> impl Future<Output = Result<Vec<nc_ids::Snowflake>, nc_journal::Error>> + Send {
        let (reads, listed) = {
            let told = self.told.lock().expect("nothing panics holding it");
            (told.reads, told.lists_open.clone())
        };
        let asked = self.inner.open_units(realm);
        async move {
            if reads == Reads::Refused {
                return Err(nc_journal::Error::Unavailable(
                    "the journal's store cannot be read, as the test made it".to_owned(),
                ));
            }
            match listed {
                Some(listed) => Ok(listed),
                None => asked.await,
            }
        }
    }
}

/// How a [`Flaky`] directory answers for the realm.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Mode {
    /// As the world says.
    Answers,
    /// It refuses, as a directory that is down does.
    Fails,
    /// It never answers.
    Hangs,
    /// It says the realm is of `profile`, pinning `pin`.
    Repins { profile: String, pin: String },
    /// It says the realm's policies are at this epoch.
    Epoch(u64),
}

/// Which of a directory's three questions a [`Flaky`] directory answers as
/// its mode says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Call {
    /// The realm's facts: its profile, its pin and its epoch.
    Realm,
    /// The grant a principal holds in it.
    Grant,
    /// The entities a decision reads.
    Slice,
}

/// A directory over the world that answers one of its questions as a test
/// says, and the other two as the world does.
#[derive(Debug)]
struct Flaky {
    inner: MemoryDirectory,
    mode: Arc<Mutex<Mode>>,
    call: Arc<Mutex<Call>>,
}

impl Flaky {
    /// How `call` is answered: as the test said when it is the call the test
    /// chose, and as the world says otherwise.
    fn mode_of(&self, call: Call) -> Mode {
        if *self.call.lock().expect("nothing panics holding it") == call {
            self.mode.lock().expect("nothing panics holding it").clone()
        } else {
            Mode::Answers
        }
    }
}

/// `answer`, or the refusal or the silence `mode` puts in its place.
async fn as_told<T>(
    mode: Mode,
    answer: impl Future<Output = Result<T, DirectoryError>>,
) -> Result<T, DirectoryError> {
    match mode {
        Mode::Answers | Mode::Repins { .. } | Mode::Epoch(_) => answer.await,
        Mode::Fails => Err(DirectoryError::Unavailable(
            "down, as the test made it".to_owned(),
        )),
        Mode::Hangs => std::future::pending().await,
    }
}

impl Directory for Flaky {
    fn realm(
        &self,
        realm: &str,
    ) -> impl Future<Output = Result<RealmFacts, DirectoryError>> + Send {
        let mode = self.mode_of(Call::Realm);
        let answer = self.inner.realm(realm);
        async move {
            match mode {
                Mode::Repins { profile, pin } => answer.await.map(|facts| RealmFacts {
                    profile,
                    pinned_pack: pin,
                    ..facts
                }),
                Mode::Epoch(epoch) => answer.await.map(|facts| RealmFacts { epoch, ..facts }),
                other => as_told(other, answer).await,
            }
        }
    }

    fn grant(
        &self,
        realm: &str,
        principal: &EntityRef,
    ) -> impl Future<Output = Result<Option<nc_authz::GrantFacts>, DirectoryError>> + Send {
        as_told(
            self.mode_of(Call::Grant),
            self.inner.grant(realm, principal),
        )
    }

    fn slice(
        &self,
        realm: &str,
        principal: &EntityRef,
        resource: &EntityRef,
        grant: Option<&EntityRef>,
    ) -> impl Future<Output = Result<nc_authz::Slice, DirectoryError>> + Send {
        as_told(
            self.mode_of(Call::Slice),
            self.inner.slice(realm, principal, resource, grant),
        )
    }
}

/// The local database held in memory, whose reads a test can make wait until
/// it lets them go — announcing that one is waiting — whose commit of a
/// reversal or of a step it can make refuse, and whose listing of a person's
/// units and reading of one unit too.
#[derive(Debug, Clone)]
struct Stalling {
    inner: LocalStore,
    stalls: Arc<std::sync::atomic::AtomicBool>,
    reached: Arc<tokio::sync::Notify>,
    released: Arc<tokio::sync::Notify>,
    refuses_reversals: Arc<std::sync::atomic::AtomicBool>,
    refuses_steps: Arc<std::sync::atomic::AtomicBool>,
    refuses_history: Arc<std::sync::atomic::AtomicBool>,
    refuses_units: Arc<std::sync::atomic::AtomicBool>,
}

impl Stalling {
    /// Whether a commit of `unit` is refused: as a reversal the store refuses,
    /// and as a step it answers with a conflict.
    fn refuses(&self, unit: &UnitRecord) -> (bool, bool) {
        // **The kernel's own reversal, and no person's undo**: both are
        // undo units, and only the first is labelled the kernel's.
        let refused = self.refuses_reversals.load(Ordering::SeqCst)
            && matches!(unit.meta.kind, UnitKind::Undo { .. })
            && unit.meta.label == "kernel.revert";
        let conflicts =
            self.refuses_steps.load(Ordering::SeqCst) && unit.meta.kind == UnitKind::Step;
        (refused, conflicts)
    }
}

/// `commit`, refused as `refused` and `conflicts` say.
async fn refusing(
    (refused, conflicts): (bool, bool),
    commit: impl Future<Output = Result<nc_local::Committed, nc_local::CommitError>>,
) -> Result<nc_local::Committed, nc_local::CommitError> {
    if refused {
        return Err(nc_local::CommitError::Store(
            nc_local::LocalError::Unavailable(
                "the store refuses a reversal, as the test made it".to_owned(),
            ),
        ));
    }
    if conflicts {
        return Err(nc_local::CommitError::Conflict {
            plugin: "noctua:reference".to_owned(),
            key: "note/photo".to_owned(),
        });
    }
    commit.await
}

impl StoreBackend for Stalling {
    fn get(
        &self,
        realm: &nc_ids::OpaqueId,
        plugin: &str,
        key: &str,
    ) -> impl Future<Output = Result<Option<nc_local::Entry>, nc_local::LocalError>> + Send {
        let stalls = self.stalls.load(Ordering::SeqCst);
        let (reached, released) = (Arc::clone(&self.reached), Arc::clone(&self.released));
        let read = self.inner.get(realm, plugin, key);
        async move {
            if stalls {
                reached.notify_one();
                released.notified().await;
            }
            read.await
        }
    }

    fn list(
        &self,
        realm: &nc_ids::OpaqueId,
        plugin: &str,
        prefix: &str,
        limit: u32,
        cursor: Option<&str>,
    ) -> impl Future<Output = Result<nc_local::Page, nc_local::LocalError>> + Send {
        self.inner.list(realm, plugin, prefix, limit, cursor)
    }

    fn count(
        &self,
        realm: &nc_ids::OpaqueId,
        plugin: &str,
    ) -> impl Future<Output = Result<u64, nc_local::LocalError>> + Send {
        self.inner.count(realm, plugin)
    }

    fn commit(
        &self,
        unit: &UnitRecord,
        quotas: &Quotas,
    ) -> impl Future<Output = Result<nc_local::Committed, nc_local::CommitError>> + Send {
        refusing(self.refuses(unit), self.inner.commit(unit, quotas))
    }

    fn commit_held(
        &self,
        unit: &UnitRecord,
        quotas: &Quotas,
    ) -> impl Future<Output = Result<nc_local::Committed, nc_local::CommitError>> + Send {
        refusing(self.refuses(unit), self.inner.commit_held(unit, quotas))
    }

    fn release(
        &self,
        holder: nc_ids::Snowflake,
    ) -> impl Future<Output = Result<u64, nc_local::LocalError>> + Send {
        self.inner.release(holder)
    }

    fn holders(
        &self,
    ) -> impl Future<Output = Result<Vec<nc_ids::Snowflake>, nc_local::LocalError>> + Send {
        self.inner.holders()
    }

    fn units(
        &self,
        realm: &OpaqueId,
        principal: &OpaqueId,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<nc_local::UnitRow>, nc_local::LocalError>> + Send {
        let refused = self.refuses_history.load(Ordering::SeqCst);
        let listed = self.inner.units(realm, principal, limit);
        async move {
            if refused {
                return Err(nc_local::LocalError::Unavailable(
                    "the store does not list units, as the test made it".to_owned(),
                ));
            }
            listed.await
        }
    }

    fn steps(
        &self,
        realm: &OpaqueId,
        principal: &OpaqueId,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<nc_local::UnitRow>, nc_local::LocalError>> + Send {
        let refused = self.refuses_history.load(Ordering::SeqCst);
        let listed = self.inner.steps(realm, principal, limit);
        async move {
            if refused {
                return Err(nc_local::LocalError::Unavailable(
                    "the store does not list steps, as the test made it".to_owned(),
                ));
            }
            listed.await
        }
    }

    fn unit(
        &self,
        unit: nc_ids::Snowflake,
    ) -> impl Future<Output = Result<Option<nc_local::UnitDetail>, nc_local::LocalError>> + Send
    {
        let refused = self.refuses_units.load(Ordering::SeqCst);
        let read = self.inner.unit(unit);
        async move {
            if refused {
                return Err(nc_local::LocalError::Unavailable(
                    "the store does not read a unit back, as the test made it".to_owned(),
                ));
            }
            read.await
        }
    }

    fn set_state(
        &self,
        unit: nc_ids::Snowflake,
        state: nc_local::UnitState,
    ) -> impl Future<Output = Result<bool, nc_local::LocalError>> + Send {
        self.inner.set_state(unit, state)
    }

    fn forget_beyond(
        &self,
        realm: &OpaqueId,
        principal: &OpaqueId,
        keep: u32,
    ) -> impl Future<Output = Result<u64, nc_local::LocalError>> + Send {
        self.inner.forget_beyond(realm, principal, keep)
    }

    fn noted_elsewhere(
        &self,
        note: &str,
        except: nc_ids::Snowflake,
    ) -> impl Future<Output = Result<bool, nc_local::LocalError>> + Send {
        self.inner.noted_elsewhere(note, except)
    }

    fn blob(
        &self,
        hash: &str,
    ) -> impl Future<Output = Result<Option<nc_local::BlobRow>, nc_local::LocalError>> + Send {
        self.inner.blob(hash)
    }

    fn set_pinned(
        &self,
        hash: &str,
        pinned: bool,
        at_ms: u64,
    ) -> impl Future<Output = Result<bool, nc_local::LocalError>> + Send {
        self.inner.set_pinned(hash, pinned, at_ms)
    }

    fn unpinned_before(
        &self,
        before_ms: u64,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<String>, nc_local::LocalError>> + Send {
        self.inner.unpinned_before(before_ms, limit)
    }

    fn forget_blob(
        &self,
        hash: &str,
    ) -> impl Future<Output = Result<bool, nc_local::LocalError>> + Send {
        self.inner.forget_blob(hash)
    }

    fn pruner(&self) -> Option<nc_prune::sqlite::Sqlite> {
        self.inner.pruner()
    }
}

/// A kernel over the local database held in memory, the journal store `J`,
/// the [`Manual`] clock and the [`Flaky`] directory.
#[derive(Debug)]
struct Env<J>(PhantomData<J>);

impl<J: nc_journal::Store + Debug + Send + Sync + 'static> Environment for Env<J> {
    type Store = Stalling;
    type Blobs = nc_kernel::blobs::MemoryBlobs;
    type Journal = J;
    type Clock = Manual;
    type Directory = Flaky;
    type Log = Arc<Captured>;
    type Transport = nc_kernel::net::Scripted;
}

/// A kernel and everything a test reads beside it.
struct Fixture<J: nc_journal::Store + Debug + Send + Sync + 'static> {
    kernel: Kernel<Env<J>>,
    store: Stalling,
    log: Arc<Captured>,
    clock: Manual,
    mode: Arc<Mutex<Mode>>,
    call: Arc<Mutex<Call>>,
    world: MemoryDirectory,
}

/// The identity the kernels here journal under, which every one of them
/// shares.
fn the_keeper() -> OpaqueId {
    OpaqueId::new("kernel").expect("a keeper") // allow-logging: OpaqueId::new of a literal this suite writes
}

/// The realm the kernels here serve: `home`, of the world the policy cases
/// are about, whose keys the store holds the plugins' under.
fn home() -> OpaqueId {
    MemoryDirectory::from_world(WORLD, &family())
        .expect("the world is valid")
        .realm_id("home")
        .expect("home is a realm of the world")
}

/// The family pack, as a kernel carries it.
fn family() -> Pdp {
    Pdp::embedded("family").expect("the family pack is carried")
}

/// The reference plugin's manifest as `id` under `namespace`, granted what a
/// note's commands reach for and `tail` after it — and, when `tail` declares
/// `[events]`, the timers and the events a command puts off.
fn manifest(id: &str, namespace: &str, tail: &str) -> PluginManifest {
    let putting_off = if tail.contains(PUTS_OFF) {
        ", \"timers\", \"events.publish\", \"events.subscribe\""
    } else {
        ""
    };
    PluginManifest::parse(&format!(
        "id = \"{id}\"\nnamespace = \"{namespace}\"\ncapabilities = [\"store\", \
         \"commands.dispatch\"{putting_off}]\n{tail}"
    ))
    .expect("a manifest the kernel reads")
}

/// The manifests' tail of a test about what a command puts off: the peer
/// hears every event the reference plugin publishes.
const PUTS_OFF: &str = "[events]\nsubscribe = [\"reference.*\"]\n";

/// Commits `value` as `plugin`'s `key` in `store`, as a unit nobody
/// dispatched.
async fn seeded(store: &LocalStore, plugin: &str, key: &str, value: &str) {
    seeded_unit(store, "ana", |unit| {
        unit.writes = vec![StagedWrite {
            plugin: plugin.to_owned(),
            key: key.to_owned(),
            expect: Expect::Anything,
            after: Some(value.to_owned()),
        }];
    })
    .await;
}

/// Commits a step nobody dispatched in `store`, of `person`'s and shaped
/// by `shape`.
async fn seeded_unit(store: &LocalStore, person: &str, shape: impl FnOnce(&mut UnitRecord)) {
    /// The flake the next seed is minted with: each seed's own, so two seeds
    /// minted at the same instant are two units.
    static FLAKES: AtomicU64 = AtomicU64::new(16);
    let world = MemoryDirectory::from_world(WORLD, &family()).expect("the world is valid");
    let flake = u16::try_from(FLAKES.fetch_add(1, Ordering::SeqCst) % 1_000).expect("fits");
    let mut minter = Minter::new(
        MachineFlake::from_value(flake).expect("a flake"),
        Manual::at(NOW_MS),
    );
    let mut unit = UnitRecord {
        unit: minter.mint().expect("a unit"),
        meta: UnitMeta {
            realm: world.realm_id("home").expect("home is a realm"),
            principal: world
                .principal(PrincipalKind::User, person)
                .unwrap_or_else(|| panic!("{person} is a user"))
                .id()
                .clone(),
            kind: UnitKind::Step,
            label: "seeding the test".to_owned(),
            committed_at_ms: NOW_MS,
        },
        writes: Vec::new(),
        blobs: Vec::new(),
        notes: Vec::new(),
        unpinned: Vec::new(),
        marks: Vec::new(),
    };
    shape(&mut unit);
    let quotas = Quotas {
        entries_per_plugin: 100,
        lowered: std::collections::BTreeMap::new(),
        blobs: 10,
        writes_per_unit: 10,
        notes_per_unit: 10,
    };
    store
        .commit(&unit, &quotas)
        .await
        .expect("the seed commits");
}

/// How `namespace` activates, as the reference plugin reads it from its store.
fn activating(namespace: &str, variant: Variant) -> String {
    Activation {
        namespace: namespace.to_owned(),
        variant,
        subscribe: Vec::new(),
    }
    .written()
}

/// A kernel over `journal`, deciding with `pdp`, with each of `plugins` — an
/// identity and a namespace — loaded under a manifest ending with `tail`.
async fn opened<J: nc_journal::Store + Debug + Send + Sync + 'static>(
    journal: J,
    pdp: Result<Pdp, nc_authz::PdpError>,
    plugins: &[(&str, &str)],
    tail: &str,
) -> Fixture<J> {
    let store = stalling(
        LocalStore::in_memory_for_tests()
            .await
            .expect("the local database opens in memory"),
    );
    for (id, namespace) in plugins {
        if *namespace != "reference" {
            let activation = Activation {
                namespace: (*namespace).to_owned(),
                variant: Variant::Normal,
                subscribe: if tail.contains(PUTS_OFF) {
                    vec!["reference.*".to_owned()]
                } else {
                    Vec::new()
                },
            };
            seeded(&store.inner, id, ACTIVATION, &activation.written()).await;
        }
    }
    let fixture = opening(3, journal, pdp, store, Arc::new(Captured::default()))
        .await
        .expect("the kernel opens");
    let component = nc_kernel::component_for_tests("reference");
    for (id, namespace) in plugins {
        fixture
            .kernel
            .load(manifest(id, namespace, tail), &component)
            .await
            .unwrap_or_else(|refused| panic!("`{id}` loads: {refused}"));
    }
    fixture
}

/// `store`, refusing and stalling nothing until a test says so.
fn stalling(store: LocalStore) -> Stalling {
    Stalling {
        inner: store,
        stalls: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        reached: Arc::new(tokio::sync::Notify::new()),
        released: Arc::new(tokio::sync::Notify::new()),
        refuses_reversals: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        refuses_steps: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        refuses_history: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        refuses_units: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    }
}

/// A kernel over `journal` and `store`, minting on the flake `flake`,
/// deciding with `pdp` and logging to `log`.
///
/// It is opened as a daemon opens one — recovering what the journal holds
/// open — with no plugin loaded, or the answer is why it did not open.
async fn opening<J: nc_journal::Store + Debug + Send + Sync + 'static>(
    flake: u16,
    journal: J,
    pdp: Result<Pdp, nc_authz::PdpError>,
    store: Stalling,
    log: Arc<Captured>,
) -> Result<Fixture<J>, nc_kernel::OpenError> {
    let pack = family();
    let world = MemoryDirectory::from_world(WORLD, &pack).expect("the world is valid");
    let mut directory = MemoryDirectory::from_world(WORLD, &pack)
        .expect("the world is valid")
        .with_epoch("home", EPOCH);
    for person in ["ana", "bruno", "carla", "dora"] {
        directory = directory
            .with_grant(
                "home",
                EntityRef {
                    type_name: "User".to_owned(),
                    id: person.to_owned(),
                },
                "g.current",
            )
            .expect("the realm holds the grant");
    }
    let clock = Manual::at(NOW_MS);
    let mode = Arc::new(Mutex::new(Mode::Answers));
    let call = Arc::new(Mutex::new(Call::Realm));
    let keeper = the_keeper();
    let kernel = Kernel::open(KernelConfig {
        store: store.clone(),
        blobs: nc_kernel::blobs::MemoryBlobs::default(),
        pdp,
        journal: nc_journal::Journal::new(
            journal,
            world.realm_id("home").expect("home is a realm"),
            keeper,
            Minter::new(
                MachineFlake::from_value(flake).expect("a flake"),
                clock.clone(),
            ),
        ),
        directory: Flaky {
            inner: directory,
            mode: Arc::clone(&mode),
            call: Arc::clone(&call),
        },
        clock: clock.clone(),
        log: Arc::clone(&log),
        transport: nc_kernel::net::Scripted::default(),
    })
    .await?;
    Ok(Fixture {
        kernel,
        store,
        log,
        clock,
        mode,
        call,
        world,
    })
}

/// The reference plugin alone, over a journal in memory a test can make refuse.
async fn reference() -> (Fixture<Failing<nc_journal::InMemory>>, Arc<Mutex<Told>>) {
    with(&[("noctua:reference", "reference")], "").await
}

/// The reference plugin and its peer, the same component loaded again under
/// the namespace `peer`.
async fn reference_and_peer() -> Fixture<Failing<nc_journal::InMemory>> {
    with(
        &[
            ("noctua:reference", "reference"),
            ("noctua:reference-peer", "peer"),
        ],
        "",
    )
    .await
    .0
}

/// `plugins`, under manifests ending with `tail`, over a journal in memory a
/// test can make refuse.
async fn with(
    plugins: &[(&str, &str)],
    tail: &str,
) -> (Fixture<Failing<nc_journal::InMemory>>, Arc<Mutex<Told>>) {
    let told = Arc::new(Mutex::new(Told::default()));
    let journal = Failing {
        inner: nc_journal::InMemory::default(),
        told: Arc::clone(&told),
    };
    (opened(journal, Ok(family()), plugins, tail).await, told)
}

impl<J: nc_journal::Store + Debug + Send + Sync + 'static> Fixture<J> {
    /// The user called `name` in the world.
    fn principal(&self, name: &str) -> Principal {
        self.world
            .principal(PrincipalKind::User, name)
            .unwrap_or_else(|| panic!("{name} is a user of the world"))
    }

    /// Dispatches `command` with `args` as the user called `name`.
    async fn as_(&self, name: &str, command: &str, args: &str) -> Result<String, Refusal> {
        self.kernel
            .dispatch(&Caller::new(self.principal(name)), command, args)
            .await
    }

    /// Every entry of the kernel's journal.
    async fn entries(&self) -> Vec<Entry> {
        self.kernel
            .journal_for_tests()
            .await
            .expect("the journal reads back")
    }

    /// Dispatches `limits.prune` with `args` as the user called `name`,
    /// holding a quorum's approval of that very dispatch.
    async fn prune_as(&self, name: &str, args: &str) -> Result<String, Refusal> {
        let principal = self.principal(name);
        let approval = self.kernel.approval_for_tests(&principal, PRUNE, args);
        self.kernel
            .dispatch(&Caller::new(principal).approved(approval), PRUNE, args)
            .await
    }

    /// How many commands the instance of `namespace`'s plugin has run, this
    /// count included.
    async fn executions(&self, namespace: &str) -> i64 {
        let answer = self
            .as_(
                "ana",
                &format!("{namespace}.executions.count"),
                r#"{"at_most":1,"note":"photo"}"#,
            )
            .await
            .expect("the count is answered");
        match field(&answer, "executions") {
            Value::Integer(executions) => executions,
            other => panic!("not a count: {other:?}"),
        }
    }

    /// What `plugin`'s `key` holds in the store, committed.
    async fn held(&self, plugin: &str, key: &str) -> Option<String> {
        self.store
            .get(&home(), plugin, key)
            .await
            .expect("the store answers")
            .map(|entry| entry.value)
    }

    /// The units the store holds for the user called `name`, newest first.
    async fn units_of(&self, name: &str) -> Vec<nc_local::UnitRow> {
        let realm = self.world.realm_id("home").expect("home is a realm");
        self.store
            .units(&realm, self.principal(name).id(), 10)
            .await
            .expect("the store answers")
    }

    /// The names of the events of `name` the kernel logged, by field.
    fn logged(&self, name: &str, field: &str) -> Vec<String> {
        self.log
            .named(name)
            .iter()
            .filter_map(|line| line.text(field))
            .collect()
    }

    /// Has the user called `name` make the blob of `text` about a note, the
    /// one called `note`, and answers its hash.
    async fn made(&self, name: &str, note: &str, text: &str) -> String {
        let args = Value::object([("note", Value::text(note)), ("text", Value::text(text))])
            .to_canonical();
        let made = self
            .as_(name, "reference.blob.put", &args)
            .await
            .unwrap_or_else(|refused| panic!("{name} made the blob: {refused}"));
        text_of(&made, "hash")
    }

    /// Whether the store holds the blob `hash` pinned.
    async fn pinned(&self, hash: &str) -> bool {
        self.store
            .inner
            .blob(hash)
            .await
            .expect("the store answers")
            .is_some_and(|row| row.pinned)
    }
}

/// The command the kernel prunes the local tables with.
const PRUNE: &str = "limits.prune";

/// What the reference plugin's note `photo` holds in `store`, committed.
async fn photo_in(store: &Stalling) -> Option<String> {
    store
        .get(&home(), "noctua:reference", "note/photo")
        .await
        .expect("the store answers")
        .map(|entry| entry.value)
}

/// The tables a prune's answer `answer` names, in its order.
fn tables_of(answer: &str) -> Vec<String> {
    field(answer, "pruned")
        .into_array()
        .unwrap_or_else(|| panic!("`pruned` is a list: {answer}"))
        .into_iter()
        .filter_map(|row| {
            row.into_object()
                .and_then(|mut fields| fields.remove("table"))
                .and_then(Value::into_text)
        })
        .collect()
}

/// Fills the local database's units to one past their bound, answering that
/// bound and the batch the pruner takes from a table past it.
async fn units_past_their_bound(store: &LocalStore) -> (u64, u64) {
    units_past_their_bound_from(store, 0).await
}

/// Fills the local database's units to one past their bound with units whose
/// identifiers follow `from`, answering that bound and the batch the pruner
/// takes from a table past it.
async fn units_past_their_bound_from(store: &LocalStore, from: i64) -> (u64, u64) {
    let limit = nc_catalog::TABLES
        .iter()
        .find(|table| table.name == "store_units")
        .expect("the catalog bounds the units");
    let (most, batch) = (
        limit.max_rows.expect("a bound"),
        limit.prune_batch.expect("a batch"),
    );
    let before = count_units(store).await;
    let seeding = "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < ?1) \
                   INSERT INTO main.store_units (unit, realm, principal, kind, target, label, \
                   state, undone_by, committed_at) SELECT ?2 + i, 'r', 'p', 'step', NULL, 's', \
                   'committed', NULL, 0 FROM n";
    sqlx::query(seeding)
        .bind(i64::try_from(most + 1 - before).expect("fits"))
        .bind(from)
        .execute(store.pool())
        .await
        .expect("the seed inserts");
    assert_eq!(count_units(store).await, most + 1);
    (most, batch)
}

/// How many units the local database holds.
async fn count_units(store: &LocalStore) -> u64 {
    let counted: i64 = sqlx::query_scalar("SELECT count(*) FROM main.store_units")
        .fetch_one(store.pool())
        .await
        .expect("the store counts");
    u64::try_from(counted).expect("a count")
}

/// The field `name` of the canonical JSON object `answer`.
fn field(answer: &str, name: &str) -> Value {
    nc_canon::parse(answer)
        .unwrap_or_else(|refused| panic!("{answer} is not canonical JSON: {refused}"))
        .into_object()
        .and_then(|mut fields| fields.remove(name))
        .unwrap_or_else(|| panic!("{answer} holds no `{name}`"))
}

/// The text the field `name` of `answer` holds.
fn text(answer: &str, name: &str) -> String {
    field(answer, name)
        .into_text()
        .unwrap_or_else(|| panic!("`{name}` of {answer} is not text"))
}

/// The arguments of a relay on the note `photo`: `name` with `args`, `times`
/// times when that is said.
fn relay(name: &str, args: &str, times: Option<i64>) -> String {
    let mut fields = vec![
        ("args", Value::text(args)),
        ("name", Value::text(name)),
        ("note", Value::text("photo")),
    ];
    if let Some(times) = times {
        fields.push(("times", Value::Integer(times)));
    }
    Value::object(fields).to_canonical()
}

/// The arguments of a relay on the note `photo` that goes on through every
/// refusal it is answered with.
///
/// It dispatches `name` with `args` `times` times: a plugin that asks again
/// whatever it is told.
fn relay_through(name: &str, args: &str, times: i64) -> String {
    Value::object([
        ("args", Value::text(args)),
        ("name", Value::text(name)),
        ("note", Value::text("photo")),
        ("through", Value::Bool(true)),
        ("times", Value::Integer(times)),
    ])
    .to_canonical()
}

/// How many entries of `entries` were decided by `decided`.
fn decided_by(entries: &[Entry], decided: &str) -> usize {
    dispatched(entries)
        .into_iter()
        .filter(|entry| deciding(entry) == Some(decided))
        .count()
}

/// A note's arguments: `note`, and `text` when it is written.
fn note(note: &str, text: Option<&str>) -> String {
    let mut fields = vec![("note", Value::text(note))];
    if let Some(text) = text {
        fields.push(("text", Value::text(text)));
    }
    Value::object(fields).to_canonical()
}

/// Whether `entry` is a unit's bracket rather than a dispatch's entry.
fn is_a_bracket(entry: &Entry) -> bool {
    [nc_journal::unit::OPENS, nc_journal::unit::CLOSES].contains(&entry.record.action.as_str())
}

/// The brackets of `entries`, as their action and their result.
fn brackets(entries: &[Entry]) -> Vec<(&str, Outcome)> {
    entries
        .iter()
        .filter(|entry| is_a_bracket(entry))
        .map(|entry| (entry.record.action.as_str(), entry.record.result))
        .collect()
}

/// The entries of dispatches, brackets left out.
fn dispatched(entries: &[Entry]) -> Vec<&Entry> {
    entries
        .iter()
        .filter(|entry| !is_a_bracket(entry))
        .collect()
}

/// The command an entry records, from its resource.
fn command_of(entry: &Entry) -> Option<String> {
    entry
        .record
        .resource
        .clone()
        .into_object()
        .and_then(|mut fields| fields.remove("command"))
        .and_then(Value::into_text)
}

/// The entry that records the dispatch of `command`, which must be the one.
fn only<'a>(entries: &'a [Entry], command: &str) -> &'a Entry {
    let found: Vec<&Entry> = dispatched(entries)
        .into_iter()
        .filter(|entry| command_of(entry).as_deref() == Some(command))
        .collect();
    assert_eq!(found.len(), 1, "entries of `{command}`: {found:#?}");
    found[0]
}

/// Whether `refusal` is of `kind`, carrying `code`.
fn is(refusal: &Result<String, Refusal>, kind: RefusalKind, code: ErrorCode) -> bool {
    refusal
        .as_ref()
        .err()
        .is_some_and(|refusal| refusal.kind() == kind && refusal.code() == code)
}

/// The deciding policy or sentinel of `entry`.
fn deciding(entry: &Entry) -> Option<&str> {
    entry.record.deciding_policy.as_deref()
}

#[tokio::test]
async fn every_mutating_dispatch_writes_exactly_one_dispatch_entry_naming_the_deciding_policy() {
    let (fixture, _) = reference().await;
    one_entry_for_one_mutating_dispatch(&fixture).await;
}

/// The assertions of criterion 6, over any journal store.
async fn one_entry_for_one_mutating_dispatch<J: nc_journal::Store + Debug + Send + Sync>(
    fixture: &Fixture<J>,
) {
    let answer = fixture
        .as_(
            "ana",
            "reference.note.put",
            &note("photo", Some("a day out")),
        )
        .await
        .expect("ana writes her own photo's note");
    let entries = fixture.entries().await;
    let entry = only(&entries, "reference.note.put");
    let unit = entry
        .record
        .work_unit
        .expect("a mutating dispatch runs in a unit");
    assert_eq!(entry.record.result, Outcome::Allowed);
    assert_eq!(entry.record.action, "Write");
    assert_eq!(entry.record.principal.as_str(), "ana");
    assert_eq!(
        deciding(entry),
        Some("family.an_owner_governs_their_own_content")
    );
    assert_eq!(entry.record.policy_pack.as_deref(), Some("family@1"));
    assert_eq!(entry.record.policy_epoch, Some(EPOCH));
    assert_eq!(
        entry.record.grant_used.as_ref().map(OpaqueId::as_str),
        Some("g.current")
    );
    assert_eq!(entry.record.actor_chain, [] as [nc_ids::OpaqueId; 0]);
    assert_eq!(entry.record.recorded_at, NOW_MS);
    assert_eq!(
        entry.record.resource.to_canonical(),
        r#"{"command":"reference.note.put","entity":{"id":"photo","type":"Content"}}"#
    );
    assert_eq!(
        dispatched(&entries).len(),
        1,
        "exactly one dispatch entry for one dispatch: {entries:#?}"
    );
    // **The brackets, separately**: the unit opened before the entry and
    // closed committed after it, and nothing else is in the journal.
    let brackets: Vec<(&str, Outcome)> = entries
        .iter()
        .filter(|entry| is_a_bracket(entry))
        .map(|entry| {
            assert_eq!(
                entry.record.work_unit,
                Some(unit),
                "a bracket of another unit"
            );
            (entry.record.action.as_str(), entry.record.result)
        })
        .collect();
    assert_eq!(
        brackets,
        [
            (nc_journal::unit::OPENS, Outcome::UnitBegun),
            (nc_journal::unit::CLOSES, Outcome::UnitCommitted),
        ]
    );
    assert_eq!(entries.len(), 3);
    assert!(entries[0].sequence < entry.sequence && entry.sequence < entries[2].sequence);
    // And what it wrote is what the store holds, under the revision it was
    // answered with.
    let committed = fixture
        .store
        .get(&home(), "noctua:reference", "note/photo")
        .await
        .expect("the store answers")
        .expect("the note was kept");
    assert_eq!(committed.value, r#"{"text":"a day out"}"#);
    assert_eq!(text(&answer, "rev"), committed.revision.to_string());
    assert_eq!(
        committed.revision.unit, unit,
        "the store's unit is the journal's"
    );
}

#[tokio::test]
async fn every_mutating_dispatch_writes_exactly_one_dispatch_entry_in_postgresql_too() {
    let Some((pool, schema)) = postgres("entries").await else {
        return;
    };
    let journal = nc_journal::postgres::Postgres::in_schema(pool.clone(), &schema);
    let fixture = opened(
        journal,
        Ok(family()),
        &[("noctua:reference", "reference")],
        "",
    )
    .await;
    one_entry_for_one_mutating_dispatch(&fixture).await;
    drop(fixture);
    finish(pool).await;
}

/// A pool in a schema of the test `test`'s own, migrated, or `None` when
/// there is no database and the run said so.
///
/// **Failing closed, and skipping only when asked**, as every database suite of
/// this repository does: `cargo xtask check` sets `NC_NO_STACK` under
/// `--offline` and on a system that runs no stack, saying which.
///
/// **One schema per test, not per process.** The tests of this binary run at
/// once, and two of them sharing `nc_kernel_<pid>` would each drop and create
/// it again under the other — measured, the two failing six runs of six
/// together and the whole binary three of five, which reads every mutation of
/// this crate as killed.
async fn postgres(test: &str) -> Option<(sqlx::PgPool, String)> {
    use sqlx::postgres::PgPoolOptions;

    let schema = format!("{}{test}", this_runs_schemas());
    let url = match std::env::var("DATABASE_URL").ok() {
        Some(url) => url,
        None if std::env::var_os("NC_NO_STACK").is_some() => {
            let why = std::env::var("NC_NO_STACK").unwrap_or_default();
            println!("nc-kernel: skipped, no database: {why}");
            return None;
        }
        None => panic!(
            "DATABASE_URL is not set. This test proves the journal's entries against the store \
             that runs on a server, so skipping it silently is the failure `cargo xtask check` \
             exists to prevent. Run `cargo xtask dev` and then `cargo xtask check`, or \
             `cargo xtask check --offline` to skip it on purpose"
        ),
    };
    let admin = PgPoolOptions::new()
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
    let pool = into_schema(&url, &schema).await;
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("the migrations apply");
    Some((pool, schema))
}

/// A pool whose every connection works in `schema`, of the database at `url`.
async fn into_schema(url: &str, schema: &str) -> sqlx::PgPool {
    let options: sqlx::postgres::PgConnectOptions = url.parse().expect("a valid url");
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect_with(options.options([("search_path", schema)]))
        .await
        .expect("connects into the schema")
}

/// What the name of every schema this run makes begins with: this process's
/// id, so that no run takes another's.
fn this_runs_schemas() -> String {
    format!("nc_kernel_{}_", std::process::id())
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
        .expect("the suite's schema is dropped");
    pool.close().await;
}

/// A unit opened past a row a read of the history cannot decode is found and
/// recovered, and one closed before that row is left as it is.
///
/// **The read of the history stops at that row, and recovery never reads
/// it**: the journal's store names the units left open
/// (`nc_journal::Store::open_units`), from the columns that say so. Read from
/// the history instead, the unit past the row would never be found and the
/// one closed at it would look open — measured so before recovery asked the
/// store, when this test held the opposite as the residual. Nothing about the
/// row is logged at opening: finding it is `journal.verify`'s, which still
/// stops there.
#[tokio::test]
async fn a_unit_opened_past_a_row_a_read_cannot_decode_is_recovered() {
    let mut stopped = a_stopped_journal();
    let closed = stopped
        .open_unit("reference.note.put")
        .await
        .expect("opened");
    stopped
        .close_unit(closed, nc_journal::Closing::Aborted)
        .await
        .expect("closed");
    let unseen = stopped
        .open_unit("reference.note.put")
        .await
        .expect("opened");
    let told = Arc::new(Mutex::new(Told {
        reads: Reads::CutShort(1),
        ..Told::default()
    }));
    let log = Arc::new(Captured::default());
    let fixture = opening(
        3,
        Failing {
            inner: stopped.store().clone(),
            told: Arc::clone(&told),
        },
        Ok(family()),
        stalling(
            LocalStore::in_memory_for_tests()
                .await
                .expect("the local database opens in memory"),
        ),
        Arc::clone(&log),
    )
    .await
    .expect("the kernel opens");
    assert_eq!(
        log.named("kernel.recovered")
            .iter()
            .map(|line| (line.text("unit"), line.fields.get("reverted").cloned()))
            .collect::<Vec<_>>(),
        [(Some(unseen.to_crockford()), Some(Value::Bool(false)))],
        "the unit past the row is recovered, and the one closed is not"
    );
    assert_eq!(
        log.named("journal.broken"),
        [] as [nc_kernel::logging::CapturedLine; 0],
        "an opening reads no history, so it finds no row to report"
    );
    told.lock().expect("nothing panics holding it").reads = Reads::AsWritten;
    let entries = fixture.entries().await;
    let of = |unit: nc_ids::Snowflake| -> Vec<Outcome> {
        entries
            .iter()
            .filter(|entry| entry.record.work_unit == Some(unit) && is_a_bracket(entry))
            .map(|entry| entry.record.result)
            .collect()
    };
    assert_eq!(entries.len(), 4, "only the close was written: {entries:#?}");
    assert_eq!(of(closed), [Outcome::UnitBegun, Outcome::UnitAborted]);
    assert_eq!(of(unseen), [Outcome::UnitBegun, Outcome::UnitAborted]);
}

/// A kernel does not open over a journal holding an opening bracket of its
/// realm that names its unit as no kernel writes one, and says so with the
/// journal's code; nothing is recovered or written.
///
/// **The residual, held as written** (`AGENTS.md`, the journal on the
/// person's machine): the store refuses the row by name rather than skip it,
/// so recovery cannot say which units are open, and a kernel that cannot say
/// does not open — failing closed, until the row is repaired by hand. A
/// recovery that skipped it, or read the unit leniently, would open here.
#[tokio::test]
async fn a_kernel_does_not_open_over_an_opening_naming_its_unit_as_no_kernel_writes_one() {
    let store = stalling(
        LocalStore::in_memory_for_tests()
            .await
            .expect("the local database opens in memory"),
    );
    let realm = MemoryDirectory::from_world(WORLD, &family())
        .expect("the world is valid")
        .realm_id("home")
        .expect("home is a realm");
    sqlx::query(
        "INSERT INTO main.local_journal_entries (sequence, realm, principal, actor_chain, \
         action, resource, result, work_unit, recorded_at, prev_hash, hash) \
         VALUES (0, ?1, 'principal-1', '[]', ?2, 'null', ?3, '01j8xk2m9p51', 1, NULL, ?4)",
    )
    .bind(realm.as_str())
    .bind(nc_journal::unit::OPENS)
    .bind(Outcome::UnitBegun.as_str())
    .bind("ab".repeat(32))
    .execute(store.inner.pool())
    .await
    .expect("whoever can write the file can insert a row at the next sequence");
    let log = Arc::new(Captured::default());
    let refused = match opening(
        3,
        store.inner.journal(),
        Ok(family()),
        store.clone(),
        Arc::clone(&log),
    )
    .await
    {
        Err(nc_kernel::OpenError::Unrecovered(refusal)) => refusal,
        Err(other) => panic!("refused for another reason: {other}"),
        Ok(_) => panic!("opened over a journal that cannot say which units are open"),
    };
    assert_eq!(
        (refused.kind(), refused.code()),
        (RefusalKind::Unavailable, ErrorCode::JournalChainBroken),
        "{refused}"
    );
    assert_eq!(
        log.named("kernel.recovered"),
        [] as [nc_kernel::logging::CapturedLine; 0]
    );
    assert_eq!(
        nc_journal::Store::head(&store.inner.journal())
            .await
            .expect("the head reads")
            .map(|(sequence, _)| sequence),
        Some(0),
        "nothing was written over the row"
    );
    store.inner.close().await;
}

/// A unit the journal's store names as open and the journal then says is
/// closed is left as it is.
///
/// **What the re-check is for**: another writer closed it after the store was
/// asked, so the journal's own answer of how it closed is what decides, and
/// nothing is closed or reverted twice — nor its unit reverted in the store,
/// which holds it committed. Without the re-check the unit would be reverted
/// and logged as recovered, and this test would fail.
#[tokio::test]
async fn a_unit_named_open_and_closed_meanwhile_is_not_recovered() {
    let mut stopped = a_stopped_journal();
    let closed = stopped
        .open_unit("reference.note.put")
        .await
        .expect("opened");
    stopped
        .close_unit(closed, nc_journal::Closing::Committed)
        .await
        .expect("closed");
    let store = stalling(
        LocalStore::in_memory_for_tests()
            .await
            .expect("the local database opens in memory"),
    );
    seeded_unit(&store.inner, "ana", |unit| {
        unit.unit = closed;
        unit.writes = vec![StagedWrite {
            plugin: "noctua:reference".to_owned(),
            key: "note/photo".to_owned(),
            expect: Expect::Anything,
            after: Some(r#"{"text":"kept"}"#.to_owned()),
        }];
    })
    .await;
    let told = Arc::new(Mutex::new(Told {
        lists_open: Some(vec![closed]),
        ..Told::default()
    }));
    let log = Arc::new(Captured::default());
    let fixture = opening(
        3,
        Failing {
            inner: stopped.store().clone(),
            told: Arc::clone(&told),
        },
        Ok(family()),
        store.clone(),
        Arc::clone(&log),
    )
    .await
    .expect("the kernel opens");
    assert!(
        log.named("kernel.recovered").is_empty(),
        "a unit the journal says is closed is not recovered: {:?}",
        log.lines()
    );
    assert!(
        photo_in(&store).await.is_some(),
        "what the store kept of a unit the journal closed is kept"
    );
    assert_eq!(
        fixture.entries().await.len(),
        2,
        "nothing was written over a unit the journal says closed"
    );
}

/// A unit the recovery found open and the journal then refuses to close, as
/// one closed already, does not keep the kernel from opening.
///
/// Another writer closed it after the journal was asked: the close the
/// journal holds is the one there is, nothing is written twice, and the unit
/// is logged as recovered once. With every close error read as a refusal, the
/// kernel would not open, and this test would fail.
#[tokio::test]
async fn a_unit_the_journal_says_was_closed_meanwhile_does_not_keep_the_kernel_from_opening() {
    let mut stopped = a_stopped_journal();
    let closed = stopped
        .open_unit("reference.note.put")
        .await
        .expect("opened");
    stopped
        .close_unit(closed, nc_journal::Closing::Aborted)
        .await
        .expect("closed");
    let told = Arc::new(Mutex::new(Told {
        lists_open: Some(vec![closed]),
        forgets_closes: true,
        ..Told::default()
    }));
    let log = Arc::new(Captured::default());
    let fixture = opening(
        3,
        Failing {
            inner: stopped.store().clone(),
            told: Arc::clone(&told),
        },
        Ok(family()),
        stalling(
            LocalStore::in_memory_for_tests()
                .await
                .expect("the local database opens in memory"),
        ),
        Arc::clone(&log),
    )
    .await
    .expect("a close another writer took first is the unit's close");
    assert_eq!(
        log.named("kernel.recovered")
            .iter()
            .map(|line| (line.text("unit"), line.fields.get("reverted").cloned()))
            .collect::<Vec<_>>(),
        [(Some(closed.to_crockford()), Some(Value::Bool(false)))]
    );
    told.lock().expect("nothing panics holding it").reads = Reads::AsWritten;
    assert_eq!(
        brackets(&fixture.entries().await),
        [
            (nc_journal::unit::OPENS, Outcome::UnitBegun),
            (nc_journal::unit::CLOSES, Outcome::UnitAborted),
        ],
        "nothing was written twice"
    );
}

/// A scratch directory for one test, removed however the test leaves.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Scratch {
    /// The scratch called `name`, empty.
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("nc-kernel-decision-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir(&root).expect("a scratch directory");
        Self(root)
    }
}

/// The environment variable that turns the stopped-kernel test into its own
/// child: the entry the child stops at.
const STOPPED_AT: &str = "NC_KERNEL_STOPPED_AT";

/// The schema the child's journal is in.
const STOPPED_IN_SCHEMA: &str = "NC_KERNEL_STOPPED_IN_SCHEMA";

/// The directory the child's local database is in.
const STOPPED_IN_PROFILE: &str = "NC_KERNEL_STOPPED_IN_PROFILE";

/// A kernel stopped in the middle of a unit leaves nothing of it once it
/// opens again, over the journal in PostgreSQL and the local database in a
/// file.
///
/// **The test runs itself as a child**, which opens a kernel over both and
/// dispatches a note's write, and whose journal calls `std::process::abort()`
/// — no destructor, nothing flushed — as it is handed the dispatch's entry,
/// once, and its unit's close, the second time: stopped while the command ran,
/// with the unit opened in the journal and nothing in the store, and stopped
/// between the store's commit and the journal's close, with the unit committed
/// in the store. A kernel opened again over the same two closes the unit as
/// aborted, reverting what the store kept of it, and the note is not there.
#[tokio::test]
async fn a_kernel_stopped_in_the_middle_of_a_unit_leaves_nothing_of_it_once_it_opens_again() {
    if let (Ok(at), Ok(schema), Ok(profile)) = (
        std::env::var(STOPPED_AT),
        std::env::var(STOPPED_IN_SCHEMA),
        std::env::var(STOPPED_IN_PROFILE),
    ) {
        Box::pin(stopped_at(&at, &schema, std::path::Path::new(&profile))).await;
        return;
    }
    let Some((pool, schema)) = postgres("stopped").await else {
        return;
    };
    let scratch = Scratch::new("stopped");
    for (at, committed, flake) in [("the-entry", false, 7), ("the-close", true, 8)] {
        let profile = scratch.0.join(at);
        let status = the_stopped_child(STOPPED_OVER_POSTGRES, at, &schema, &profile);
        assert!(
            nc_platform::testing::stopped_by_abort(status),
            "{at}: the child did not stop the way `abort` stops it: {status}"
        );
        let store = stalling(
            LocalStore::open(&profile)
                .await
                .expect("the file opens after the stop"),
        );
        nothing_of_the_unit_is_left(
            at,
            committed,
            flake,
            nc_journal::postgres::Postgres::in_schema(pool.clone(), &schema),
            store,
        )
        .await;
    }
    finish(pool).await;
}

/// A kernel stopped in the middle of a unit leaves nothing of it once it
/// opens again, over the journal and the plugin store in one local file.
///
/// **The daemon's case, and the one every system runs**: what
/// `a_kernel_stopped_in_the_middle_of_a_unit_leaves_nothing_of_it_once_it_opens_again`
/// holds over PostgreSQL, held over `nc_local::LocalJournal`, which keeps the
/// journal in the same file as the store — the child stopped with the unit
/// opened and nothing in the store, and with the unit committed in the store
/// and its close never written, and a kernel opened again over the file
/// closing the unit as aborted, reverting what the store kept.
#[tokio::test]
async fn a_kernel_stopped_in_the_middle_of_a_unit_over_the_local_journal_leaves_nothing_of_it() {
    if let (Ok(at), Ok(profile)) = (std::env::var(STOPPED_AT), std::env::var(STOPPED_IN_PROFILE)) {
        let store = stalling(
            LocalStore::open(std::path::Path::new(&profile))
                .await
                .expect("the child's database opens"),
        );
        Box::pin(stops_in_the_middle(&at, store.inner.journal(), store)).await;
        return;
    }
    let scratch = Scratch::new("stopped-in-the-file");
    for (at, committed, flake) in [("the-entry", false, 7), ("the-close", true, 8)] {
        let profile = scratch.0.join(at);
        let status = the_stopped_child(STOPPED_OVER_THE_FILE, at, "", &profile);
        assert!(
            nc_platform::testing::stopped_by_abort(status),
            "{at}: the child did not stop the way `abort` stops it: {status}"
        );
        let store = stalling(
            LocalStore::open(&profile)
                .await
                .expect("the file opens after the stop"),
        );
        nothing_of_the_unit_is_left(at, committed, flake, store.inner.journal(), store).await;
    }
}

/// Holds a kernel opened again over `journal` and `store`, after a child
/// stopped at `at`, to closing the unit the child left as aborted and keeping
/// nothing of it — the store having held it committed when `committed`.
async fn nothing_of_the_unit_is_left<J: nc_journal::Store + Debug + Send + Sync + 'static>(
    at: &str,
    committed: bool,
    flake: u16,
    journal: J,
    store: Stalling,
) {
    assert_eq!(
        photo_in(&store).await.is_some(),
        committed,
        "{at}: what the child left in the store"
    );
    let log = Arc::new(Captured::default());
    let fixture = opening(
        flake,
        journal,
        Ok(family()),
        store.clone(),
        Arc::clone(&log),
    )
    .await
    .expect("the kernel opens again");
    assert_eq!(
        photo_in(&store).await,
        None,
        "{at}: nothing of the unit is left"
    );
    let recovered = log.named("kernel.recovered");
    assert_eq!(recovered.len(), 1, "{at}: {recovered:?}");
    assert_eq!(
        recovered[0].fields.get("reverted"),
        Some(&Value::Bool(committed)),
        "{at}"
    );
    let unit = recovered[0].text("unit").expect("the unit is named");
    let entries = fixture.entries().await;
    let closed: Vec<Outcome> = entries
        .iter()
        .filter(|entry| {
            entry.record.work_unit.map(nc_ids::Snowflake::to_crockford) == Some(unit.clone())
        })
        .filter(|entry| is_a_bracket(entry))
        .map(|entry| entry.record.result)
        .collect();
    assert_eq!(
        closed,
        [Outcome::UnitBegun, Outcome::UnitAborted],
        "{at}: the unit the child left open is closed as aborted"
    );
    drop(fixture);
    store.inner.close().await;
}

/// What the child of the stopped-kernel test does: dispatches a note's write
/// over the journal in `schema` and the database in `profile`, and stops at
/// the entry `at` names.
async fn stopped_at(at: &str, schema: &str, profile: &std::path::Path) {
    let url = std::env::var("DATABASE_URL").expect("the parent had a database");
    let store = stalling(
        LocalStore::open(profile)
            .await
            .expect("the child's database opens"),
    );
    let inner = nc_journal::postgres::Postgres::in_schema(into_schema(&url, schema).await, schema);
    stops_in_the_middle(at, inner, store).await;
}

/// What the child of a stopped-kernel test does over `inner` and `store`:
/// dispatches a note's write, and stops the process at the entry `at` names.
async fn stops_in_the_middle<S>(at: &str, inner: S, store: Stalling)
where
    S: nc_journal::Store + Debug + Clone + Send + Sync + 'static,
{
    // **A flake of each child's own**: the clock stands still, and a second
    // child minting on the first one's flake mints the unit the first left,
    // which the journal refuses as closed.
    let (stops, flake): (Refuses, u16) = match at {
        "the-entry" => (|entry| entry.record.action == "Write", 5),
        _ => (
            |entry| {
                entry.record.action == nc_journal::unit::CLOSES
                    && entry.record.result == Outcome::UnitCommitted
            },
            6,
        ),
    };
    let journal = Failing {
        inner,
        told: Arc::new(Mutex::new(Told {
            aborts: Some(stops),
            ..Told::default()
        })),
    };
    let fixture = Box::pin(opening(
        flake,
        journal,
        Ok(family()),
        store,
        Arc::new(Captured::default()),
    ))
    .await
    .expect("the child's kernel opens");
    fixture
        .kernel
        .load(
            manifest("noctua:reference", "reference", ""),
            &nc_kernel::component_for_tests("reference"),
        )
        .await
        .expect("the reference plugin loads");
    let answered = fixture
        .as_(
            "ana",
            "reference.note.put",
            &note("photo", Some("never kept")),
        )
        .await;
    panic!("the child was to stop before its dispatch answered, and it answered {answered:?}");
}

/// The stopped-kernel test whose child stops over the journal in PostgreSQL.
const STOPPED_OVER_POSTGRES: &str =
    "a_kernel_stopped_in_the_middle_of_a_unit_leaves_nothing_of_it_once_it_opens_again";

/// The stopped-kernel test whose child stops over the journal in the local
/// file.
const STOPPED_OVER_THE_FILE: &str =
    "a_kernel_stopped_in_the_middle_of_a_unit_over_the_local_journal_leaves_nothing_of_it";

/// Runs the stopped-kernel test `test` as a child that stops at `at`, over the
/// journal in `schema` — none for the local journal's — and the database in
/// `profile`.
#[expect(
    clippy::disallowed_methods,
    reason = "the child is this same test binary, run with its output discarded — the one way \
              to stop a kernel in the middle of a unit and look at what it leaves"
)]
fn the_stopped_child(
    test: &str,
    at: &str,
    schema: &str,
    profile: &std::path::Path,
) -> std::process::ExitStatus {
    std::process::Command::new(std::env::current_exe().expect("this test binary"))
        .args(["--exact", test, "--test-threads=1"])
        .env(STOPPED_AT, at)
        .env(STOPPED_IN_SCHEMA, schema)
        .env(STOPPED_IN_PROFILE, profile)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("the child runs")
}

#[tokio::test]
async fn a_denied_dispatch_is_journaled_with_the_forbid_that_decided() {
    let (fixture, _) = reference().await;
    fixture.clock.advance(5);
    let refused = fixture
        .as_(
            "dora",
            "reference.note.put",
            &note("postcard", Some("hello")),
        )
        .await;
    assert!(
        is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
        "dora is suspended, and owns the postcard: {refused:?}"
    );
    let entries = fixture.entries().await;
    let entry = only(&entries, "reference.note.put");
    assert_eq!(entry.record.result, Outcome::Denied);
    assert_eq!(
        deciding(entry),
        Some("family.a_frozen_account_acts_on_nothing")
    );
    assert_eq!(entry.record.principal.as_str(), "dora");
    assert_eq!(entry.record.policy_pack.as_deref(), Some("family@1"));
    assert_eq!(entry.record.policy_epoch, Some(EPOCH));
    // **The grant it rested on and the entity it was asked for**: an entry
    // naming neither fails here.
    assert_eq!(
        entry.record.grant_used.as_ref().map(OpaqueId::as_str),
        Some("g.current")
    );
    assert_eq!(
        entry.record.resource.to_canonical(),
        r#"{"command":"reference.note.put","entity":{"id":"postcard","type":"Content"}}"#
    );
    assert_eq!(entry.record.work_unit, None, "a denial opens no unit");
    // **When it was refused**: every refusal written at the epoch fails this
    // test, as it fails the one of a name no command has.
    assert_eq!(entry.record.recorded_at, NOW_MS + 5);
    assert!(
        entries.iter().all(|entry| !is_a_bracket(entry)),
        "{entries:#?}"
    );
    assert_eq!(
        fixture.held("noctua:reference", "note/postcard").await,
        None
    );
    assert_eq!(
        fixture.logged("kernel.dispatch_refused", "reason"),
        ["denied"]
    );
    assert_eq!(
        fixture.logged("kernel.dispatch_refused", "code"),
        [ErrorCode::PolicyDenied.code()]
    );
    assert_eq!(
        fixture.log.named("kernel.dispatch_refused")[0].fields["mutating"],
        Value::Bool(true),
        "the log says the refused command would have changed state"
    );
    assert_eq!(
        fixture.executions("reference").await,
        1,
        "the plugin never ran the denied command"
    );
}

#[tokio::test]
async fn a_denial_tells_the_caller_its_code_and_not_the_policy_that_decided() {
    let (fixture, _) = reference().await;
    for (who, command, args) in [
        ("dora", "reference.note.put", note("postcard", Some("x"))),
        ("bruno", "reference.note.put", note("photo", Some("x"))),
    ] {
        let refusal = fixture.as_(who, command, &args).await.expect_err("denied");
        assert!(
            !refusal.sentence().contains("family.") && !refusal.sentence().contains("kernel."),
            "the caller was told what decided: {refusal}"
        );
        assert!(
            refusal
                .to_string()
                .starts_with(&format!("[{}] ", ErrorCode::PolicyDenied))
        );
    }
    let entries = fixture.entries().await;
    assert!(
        dispatched(&entries)
            .iter()
            .all(|entry| deciding(entry).is_some_and(|id| !id.is_empty())),
        "the journal records what decided either way"
    );
}

#[tokio::test]
async fn a_read_is_journaled_and_carries_no_unit() {
    let (fixture, _) = reference().await;
    let answer = fixture
        .as_("carla", "reference.note.get", &note("photo", None))
        .await
        .expect("a member reads content shared with the realm");
    assert_eq!(answer, "null", "nothing written yet");
    let entries = fixture.entries().await;
    let entry = only(&entries, "reference.note.get");
    assert_eq!(entry.record.result, Outcome::Allowed);
    assert_eq!(entry.record.action, "Read");
    assert_eq!(
        deciding(entry),
        Some("family.a_member_reads_shared_content")
    );
    assert_eq!(entry.record.work_unit, None);
    assert_eq!(
        entries.len(),
        1,
        "no unit was opened for a read: {entries:#?}"
    );
}

#[tokio::test]
async fn a_dispatch_no_permit_matches_is_denied_by_default() {
    let (fixture, _) = reference().await;
    let refused = fixture
        .as_(
            "bruno",
            "reference.note.put",
            &note("photo", Some("mine now")),
        )
        .await;
    assert!(is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied));
    let entries = fixture.entries().await;
    assert_eq!(
        deciding(only(&entries, "reference.note.put")),
        Some(nc_authz::sentinel::NO_PERMIT_MATCHED)
    );
    assert_eq!(fixture.held("noctua:reference", "note/photo").await, None);
}

#[tokio::test]
async fn a_dispatch_while_the_pack_cannot_load_is_denied_not_allowed() {
    let unbuilt = Pdp::embedded("collective").expect_err("no pack carries this profile");
    let code = unbuilt.code();
    let journal = Failing {
        inner: nc_journal::InMemory::default(),
        told: Arc::new(Mutex::new(Told::default())),
    };
    let fixture = opened(journal, Err(unbuilt), &[], "").await;
    // The kernel's own read of the realm, which no plugin needs to be loaded
    // for — and a guardian, whom the pack would allow.
    let refused = fixture.as_("ana", "journal.verify", "{}").await;
    assert!(is(&refused, RefusalKind::Denied, code), "{refused:?}");
    let entries = fixture.entries().await;
    let entry = only(&entries, "journal.verify");
    assert_eq!(entry.record.result, Outcome::Denied);
    assert_eq!(deciding(entry), Some(nc_authz::sentinel::PDP_UNAVAILABLE));
    assert_eq!(entry.record.policy_pack, None, "no pack decided it");
}

/// How long a test waits on a dispatch the kernel should have given up on
/// long before: an hour, which a paused clock reaches at once.
const WAITED_ON_FOR_EVER: Duration = Duration::from_secs(3_600);

/// A directory that refuses, or never answers, any one of the three questions
/// the decision point asks it is a denial — the realm's facts, the grant and
/// the entities each asked under the deadline of a host call.
///
/// The grant and the slice are each asked under that deadline: with either
/// taken out, the dispatch waits on the directory for ever and this fails.
#[tokio::test]
async fn a_dispatch_while_the_directory_is_unavailable_is_denied() {
    let (fixture, _) = reference().await;
    // **A stopped clock, so a directory that never answers is waited out at
    // once**: the runtime moves time on when nothing else can run. Started
    // again before the store is read, whose pool waits on the same clock.
    tokio::time::pause();
    let cases: Vec<(Call, Mode)> = [Call::Realm, Call::Grant, Call::Slice]
        .into_iter()
        .flat_map(|call| [(call, Mode::Fails), (call, Mode::Hangs)])
        .collect();
    for (call, mode) in &cases {
        *fixture.call.lock().expect("nothing panics holding it") = *call;
        *fixture.mode.lock().expect("nothing panics holding it") = mode.clone();
        let refused = tokio::time::timeout(
            WAITED_ON_FOR_EVER,
            fixture.as_("ana", "reference.note.put", &note("photo", Some("x"))),
        )
        .await
        .unwrap_or_else(|_| panic!("{call:?} {mode:?}: the dispatch waited on the directory"));
        assert!(
            is(
                &refused,
                RefusalKind::Denied,
                ErrorCode::PolicyDirectoryUnavailable
            ),
            "{call:?} {mode:?}: {refused:?}"
        );
    }
    *fixture.mode.lock().expect("nothing panics holding it") = Mode::Answers;
    let entries = fixture.entries().await;
    let refused: Vec<Option<&str>> = dispatched(&entries).into_iter().map(deciding).collect();
    assert_eq!(
        refused,
        vec![Some(nc_authz::sentinel::DIRECTORY_UNAVAILABLE); cases.len()],
        "a directory that refused and one that never answered are each a denial, \
         whichever question it was asked"
    );
    tokio::time::resume();
    assert_eq!(fixture.held("noctua:reference", "note/photo").await, None);
}

#[tokio::test]
async fn a_realm_pinning_a_pack_this_kernel_does_not_carry_is_denied() {
    let (fixture, _) = reference().await;
    // **Both halves of the pin**: another profile pinning this pack's very
    // version, which the pin alone would let through, and this profile
    // pinning a version this kernel does not carry.
    for (profile, pin) in [("collective", "family@1"), ("family", "family@2")] {
        *fixture.mode.lock().expect("nothing panics holding it") = Mode::Repins {
            profile: profile.to_owned(),
            pin: pin.to_owned(),
        };
        let refused = fixture
            .as_("ana", "reference.note.get", &note("photo", None))
            .await;
        assert!(
            is(
                &refused,
                RefusalKind::Denied,
                ErrorCode::PolicyPackNotCarried
            ),
            "{profile} pinning {pin}: {refused:?}"
        );
    }
    let entries = fixture.entries().await;
    for entry in dispatched(&entries) {
        assert_eq!(deciding(entry), Some(nc_authz::sentinel::PACK_NOT_CARRIED));
        assert_eq!(entry.record.policy_epoch, Some(EPOCH));
    }
    assert_eq!(dispatched(&entries).len(), 2);
    let logged = fixture.log.named("kernel.dispatch_refused");
    assert_eq!(logged.len(), 2);
    for line in &logged {
        assert_eq!(
            line.fields["mutating"],
            Value::Bool(false),
            "a refused read is logged as one"
        );
    }
}

/// A realm the directory gives no epoch is denied, as the core pack denies
/// every decision asked with none: the epoch a dispatch is decided under is
/// the directory's, and never one the kernel puts in its place.
///
/// The refusal and the store are read, not only the journal's copy of the
/// epoch: a kernel deciding under an epoch of its own would allow a write on
/// a realm at epoch zero.
#[tokio::test]
async fn a_realm_the_directory_gives_no_epoch_is_denied() {
    let (fixture, _) = reference().await;
    *fixture.mode.lock().expect("nothing panics holding it") = Mode::Epoch(0);
    let refused = fixture
        .as_("ana", "reference.note.put", &note("photo", Some("x")))
        .await;
    assert!(
        is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
        "{refused:?}"
    );
    let entries = fixture.entries().await;
    let entry = only(&entries, "reference.note.put");
    assert_eq!(deciding(entry), Some("core.fail_closed"));
    assert_eq!(entry.record.policy_epoch, Some(0));
    assert_eq!(fixture.held("noctua:reference", "note/photo").await, None);
}

#[tokio::test]
async fn a_mutating_dispatch_that_cannot_be_journaled_leaves_no_state() {
    for (case, refuses) in [
        (
            "the opening bracket",
            (|entry: &Entry| entry.record.result == Outcome::UnitBegun) as Refuses,
        ),
        ("the dispatch's entry", |entry: &Entry| {
            entry.record.action == "Write"
        }),
        ("the closing bracket", |entry: &Entry| {
            entry.record.result == Outcome::UnitCommitted
        }),
    ] {
        let (fixture, told) = reference().await;
        told.lock().expect("nothing panics holding it").refuses = Some(refuses);
        let refused = fixture
            .as_("ana", "reference.note.put", &note("photo", Some("x")))
            .await;
        told.lock().expect("nothing panics holding it").refuses = None;
        assert!(
            is(
                &refused,
                RefusalKind::Unavailable,
                ErrorCode::JournalUnavailable
            ),
            "{case}: {refused:?}"
        );
        assert_eq!(
            fixture.held("noctua:reference", "note/photo").await,
            None,
            "{case}: what the command wrote outlived a journal that did not take it"
        );
        assert_eq!(
            fixture
                .as_("ana", "reference.note.get", &note("photo", None))
                .await
                .as_deref(),
            Ok("null"),
            "{case}"
        );
        what_the_journal_left(&fixture, case).await;
    }
}

/// What a dispatch the journal did not take left behind, by `case`.
async fn what_the_journal_left(fixture: &Fixture<Failing<nc_journal::InMemory>>, case: &str) {
    let entries = fixture.entries().await;
    match case {
        "the opening bracket" => {
            assert!(
                entries
                    .iter()
                    .all(|entry| command_of(entry).as_deref() != Some("reference.note.put")),
                "{entries:#?}"
            );
            assert_eq!(
                fixture.executions("reference").await,
                2,
                "the plugin ran a command whose unit the journal never opened: it ran the \
                 read after it, and this count, and nothing else"
            );
        }
        "the dispatch's entry" => {
            assert_eq!(
                entries
                    .iter()
                    .filter(|entry| entry.record.result == Outcome::UnitAborted)
                    .count(),
                1,
                "the unit is closed aborted: {entries:#?}"
            );
        }
        _ => {
            // **Committed in the store, and reverted at once.** The unit's
            // row is kept, marked undone by its reversal.
            let ana = fixture.principal("ana");
            let units = fixture
                .store
                .units(
                    fixture.world.realm_id("home").as_ref().expect("home"),
                    ana.id(),
                    10,
                )
                .await
                .expect("the store answers");
            let step = units
                .iter()
                .find(|unit| {
                    unit.meta.kind == UnitKind::Step && unit.meta.label == "reference.note.put"
                })
                .expect("the unit committed before its close was refused");
            assert!(
                matches!(step.state, nc_local::UnitState::Undone { .. }),
                "{units:#?}"
            );
            // **And the reversal is in the person's history under the name the
            // kernel's documentation gives it**: a reversal labelled anything
            // else fails here.
            assert!(
                units.iter().any(|unit| {
                    unit.meta.kind == UnitKind::Undo { target: step.unit }
                        && unit.meta.label == "kernel.revert"
                }),
                "{units:#?}"
            );
            assert!(
                entries
                    .iter()
                    .any(|entry| entry.record.result == Outcome::UnitAborted),
                "the journal took the abort once it took entries again: {entries:#?}"
            );
        }
    }
    let (refused, aborted) = (
        fixture.logged("kernel.dispatch_refused", "reason"),
        fixture.logged("kernel.unit_aborted", "cause"),
    );
    if case == "the closing bracket" {
        assert!(refused.is_empty(), "{case}: {refused:?}");
        assert_eq!(aborted, ["journal"], "{case}");
    } else {
        assert_eq!(refused, ["journal_unavailable"], "{case}");
        assert_eq!(
            fixture.logged("kernel.dispatch_refused", "code"),
            [ErrorCode::JournalUnavailable.code()],
            "{case}"
        );
    }
}

/// A read is journaled before its answer is returned, so a read the journal
/// did not take is answered with nothing it read.
///
/// **Withheld as a write's answer is**: a decision point that withheld only a
/// write's answer would fail this, and the timer, event and surface tests
/// after it.
#[tokio::test]
async fn a_read_the_journal_did_not_take_is_withheld() {
    let (fixture, told) = reference().await;
    told.lock().expect("nothing panics holding it").refuses =
        Some(|entry: &Entry| entry.record.action == "Read");
    let refused = fixture
        .as_("carla", "reference.note.get", &note("photo", None))
        .await;
    told.lock().expect("nothing panics holding it").refuses = None;
    assert!(
        is(
            &refused,
            RefusalKind::Unavailable,
            ErrorCode::JournalUnavailable
        ),
        "{refused:?}"
    );
    let refusal = refused.expect_err("withheld");
    assert!(
        refusal.sentence().contains("its answer is withheld"),
        "{refusal}"
    );
    let entries = fixture.entries().await;
    assert!(entries.is_empty(), "{entries:#?}");
    assert_eq!(
        fixture.logged("kernel.dispatch_refused", "reason"),
        ["journal_unavailable"]
    );
    assert_eq!(
        fixture.logged("kernel.dispatch_refused", "code"),
        [ErrorCode::JournalUnavailable.code()]
    );
}

/// The reference plugin and its peer, which hears every event the reference
/// plugin publishes, each granted the timers and the events a command puts off.
async fn putting_off() -> (Fixture<Failing<nc_journal::InMemory>>, Arc<Mutex<Told>>) {
    with(
        &[
            ("noctua:reference", "reference"),
            ("noctua:reference-peer", "peer"),
        ],
        PUTS_OFF,
    )
    .await
}

/// The arguments of a timer about `photo`, of a second, tagged `log`.
fn a_second() -> String {
    Value::object([
        ("ms", Value::Integer(1_000)),
        ("note", Value::text("photo")),
        ("tag", Value::text("log")),
    ])
    .to_canonical()
}

/// A timer a read sets is armed only once the journal took the read's entry,
/// and never for a read whose answer was withheld — at the top of its tree or
/// nested beneath a read the journal refused.
///
/// **What it holds against**: a read that held nothing for later would arm the
/// timer at once, and the pump fire it as ana for a dispatch that is in no
/// journal entry.
#[tokio::test]
async fn a_timer_set_by_a_read_the_journal_did_not_take_never_fires() {
    let (fixture, told) = putting_off().await;
    let remind = "reference.timer.remind";
    told.lock().expect("nothing panics holding it").refuses =
        Some(|entry: &Entry| command_of(entry).as_deref() == Some("reference.timer.remind"));
    let withheld = fixture.as_("ana", remind, &a_second()).await;
    assert!(
        is(
            &withheld,
            RefusalKind::Unavailable,
            ErrorCode::JournalUnavailable
        ),
        "{withheld:?}"
    );
    told.lock().expect("nothing panics holding it").refuses =
        Some(|entry: &Entry| command_of(entry).as_deref() == Some("peer.relay.read"));
    let nested = fixture
        .as_("ana", "peer.relay.read", &relay(remind, &a_second(), None))
        .await;
    assert!(
        is(
            &nested,
            RefusalKind::Unavailable,
            ErrorCode::JournalUnavailable
        ),
        "{nested:?}"
    );
    told.lock().expect("nothing panics holding it").refuses = None;
    fixture.clock.advance(1_000);
    let pumped = fixture.kernel.pump().await;
    assert_eq!(
        pumped.fired, 0,
        "a timer set by a dispatch the journal never recorded fired as ana: {pumped:?}"
    );
    fixture
        .as_("ana", remind, &a_second())
        .await
        .expect("a read the journal takes sets its timer");
    fixture.clock.advance(1_000);
    assert_eq!(
        fixture.kernel.pump().await.fired,
        1,
        "a timer a read the journal took sets fires"
    );
}

/// An event a read publishes reaches the queue only once the journal took the
/// read's entry, and is never heard for a read whose answer was withheld.
///
/// **What it holds against**: the event queued at once, and the peer hearing it
/// as ana for a dispatch that is in no journal entry.
#[tokio::test]
async fn an_event_published_by_a_read_the_journal_did_not_take_is_never_heard() {
    let (fixture, told) = putting_off().await;
    let announce = "reference.event.announce";
    let args = Value::object([
        ("note", Value::text("photo")),
        ("topic", Value::text("reference.noted")),
    ])
    .to_canonical();
    told.lock().expect("nothing panics holding it").refuses =
        Some(|entry: &Entry| command_of(entry).as_deref() == Some("reference.event.announce"));
    let withheld = fixture.as_("ana", announce, &args).await;
    told.lock().expect("nothing panics holding it").refuses = None;
    assert!(
        is(
            &withheld,
            RefusalKind::Unavailable,
            ErrorCode::JournalUnavailable
        ),
        "{withheld:?}"
    );
    let pumped = fixture.kernel.pump().await;
    assert_eq!(
        pumped.delivered, 0,
        "an event published by a dispatch the journal never recorded was heard: {pumped:?}"
    );
    fixture
        .as_("ana", announce, &args)
        .await
        .expect("a read the journal takes publishes its event");
    assert_eq!(
        fixture.kernel.pump().await.delivered,
        1,
        "an event a read the journal took publishes is heard"
    );
}

/// Every surface the kernel shows, as its title and the tree it shows.
fn shown<J: nc_journal::Store + Debug + Send + Sync + 'static>(
    fixture: &Fixture<J>,
) -> Vec<(String, Option<String>)> {
    fixture
        .kernel
        .surfaces()
        .iter()
        .map(|surface| {
            (
                surface.title().to_owned(),
                surface.tree().map(str::to_owned),
            )
        })
        .collect()
}

/// The arguments of the reference plugin's `ui.show`: a surface titled
/// `Notes` showing a text that says `text`.
fn showing(text: &str) -> String {
    let tree = Value::object([("node", Value::text("text")), ("text", Value::text(text))]);
    Value::object([
        ("note", Value::text("photo")),
        ("title", Value::text("Notes")),
        ("tree", Value::text(tree.to_canonical())),
    ])
    .to_canonical()
}

/// A surface a read opens and renders into is never shown when the journal
/// did not take the read's entry, as a timer it sets never fires and an event
/// it publishes is never heard — and the same read the journal takes shows it.
///
/// **What it holds against**: a surface opened and rendered into at once, so
/// that a read whose answer was withheld, in no entry of the journal, would
/// leave a person shown what it found.
#[tokio::test]
async fn a_surface_rendered_by_a_read_the_journal_did_not_take_is_never_shown() {
    let (fixture, told) = with(&[], "").await;
    let manifest = PluginManifest::parse(
        "id = \"noctua:reference\"\nnamespace = \"reference\"\ncapabilities = [\"store\", \
         \"ui\"]\n",
    )
    .expect("a manifest granting the ui");
    fixture
        .kernel
        .load(manifest, &nc_kernel::component_for_tests("reference"))
        .await
        .expect("loads");
    told.lock().expect("nothing panics holding it").refuses =
        Some(|entry: &Entry| command_of(entry).as_deref() == Some("reference.ui.show"));
    let withheld = fixture
        .as_("ana", "reference.ui.show", &showing("what the read found"))
        .await;
    told.lock().expect("nothing panics holding it").refuses = None;
    assert!(
        is(
            &withheld,
            RefusalKind::Unavailable,
            ErrorCode::JournalUnavailable
        ),
        "{withheld:?}"
    );
    assert_eq!(
        dispatched(&fixture.entries().await),
        [] as [&nc_journal::Entry; 0]
    );
    assert!(
        shown(&fixture).is_empty(),
        "a read the journal never recorded is shown on a surface: {:?}",
        shown(&fixture)
    );
    fixture
        .as_("ana", "reference.ui.show", &showing("what the read found"))
        .await
        .expect("a read the journal takes shows what it found");
    assert_eq!(
        shown(&fixture),
        [(
            "Notes".to_owned(),
            Some(r#"{"node":"text","text":"what the read found"}"#.to_owned())
        )]
    );
}

/// A surface rendered in a unit the journal did not keep — by a command
/// nested in another plugin's — is never shown, and the same unit kept shows
/// it.
///
/// **What it holds against**: the unit reverted and its dispatch refused, and
/// the surface going on showing what it did.
#[tokio::test]
async fn a_surface_rendered_in_a_unit_the_journal_did_not_keep_is_never_shown() {
    let (fixture, told) = with(&[], "").await;
    seeded(
        &fixture.store.inner,
        "noctua:reference-peer",
        ACTIVATION,
        &activating("peer", Variant::Normal),
    )
    .await;
    let component = nc_kernel::component_for_tests("reference");
    for (id, namespace, granted) in [
        (
            "noctua:reference",
            "reference",
            "\"store\", \"commands.dispatch\"",
        ),
        ("noctua:reference-peer", "peer", "\"store\", \"ui\""),
    ] {
        let manifest = PluginManifest::parse(&format!(
            "id = \"{id}\"\nnamespace = \"{namespace}\"\ncapabilities = [{granted}]\n"
        ))
        .expect("a manifest");
        fixture
            .kernel
            .load(manifest, &component)
            .await
            .expect("loads");
    }
    let relayed = relay("peer.ui.show", &showing("saved"), None);
    told.lock().expect("nothing panics holding it").refuses =
        Some(|entry: &Entry| entry.record.result == Outcome::UnitCommitted);
    let refused = fixture.as_("ana", "reference.relay.write", &relayed).await;
    told.lock().expect("nothing panics holding it").refuses = None;
    assert!(refused.is_err(), "the unit was not kept: {refused:?}");
    assert!(
        shown(&fixture).is_empty(),
        "a unit the journal did not keep left a surface showing what it did: {:?}",
        shown(&fixture)
    );
    fixture
        .as_("ana", "reference.relay.write", &relayed)
        .await
        .expect("a unit the journal keeps");
    assert_eq!(
        shown(&fixture),
        [(
            "Notes".to_owned(),
            Some(r#"{"node":"text","text":"saved"}"#.to_owned())
        )]
    );
}

/// A unit the journal did not take the close of is reversed with the pins it
/// put on its blobs taken off, in the reversal's own transaction: the bytes are
/// no blob this machine holds any more, and the sweep deletes them once their
/// grace has passed.
///
/// **What a blob made in a unit would otherwise outlive**: with the pins left
/// on, the store's writes would go back and its blobs stay pinned, kept for
/// ever by a unit the journal never said was kept.
#[tokio::test]
async fn a_unit_the_journal_did_not_close_takes_the_pins_off_the_blobs_it_made() {
    let (fixture, told) = with(&[], "").await;
    let manifest = PluginManifest::parse(
        "id = \"noctua:reference\"\nnamespace = \"reference\"\ncapabilities = [\"store\", \
         \"blobs.read\", \"blobs.write\"]\n",
    )
    .expect("a manifest granting the blobs");
    fixture
        .kernel
        .load(manifest, &nc_kernel::component_for_tests("reference"))
        .await
        .expect("loads");
    let kept = fixture
        .as_("ana", "reference.blob.put", &blob("kept", 2))
        .await
        .expect("a blob is kept");
    let kept = text_of(&kept, "hash");
    told.lock().expect("nothing panics holding it").refuses =
        Some(|entry: &Entry| entry.record.result == Outcome::UnitCommitted);
    // The kept blob made again, by a unit the journal does not keep: a pin
    // another unit put on is not the reversal's to take off.
    fixture
        .as_("ana", "reference.blob.put", &blob("kept", 2))
        .await
        .expect_err("the journal did not take the close");
    let refused = fixture
        .as_("ana", "reference.blob.put", &blob("reverted", 2))
        .await
        .expect_err("the journal did not take the close");
    told.lock().expect("nothing panics holding it").refuses = None;
    assert!(refused.sentence().contains("reverted at once"), "{refused}");
    assert_eq!(
        fixture
            .store
            .inner
            .holders()
            .await
            .expect("the store answers"),
        [],
        "a unit reverted at once is held from the pruner still"
    );
    let reverted = nc_kernel::blobs::Hash::of(b"revertedreverted").to_string();
    let row = fixture
        .store
        .inner
        .blob(&reverted)
        .await
        .expect("the store answers")
        .expect("the reversal keeps the row for the sweep");
    assert_eq!(
        (row.pinned, row.unpinned_at_ms),
        (false, Some(NOW_MS)),
        "the reverted unit's blob is still pinned"
    );
    let located = fixture
        .as_("ana", "reference.blob.locate", &hash_of(&reverted))
        .await
        .expect("located");
    assert_eq!(text_of(&located, "availability"), "unavailable");
    let held = fixture
        .store
        .inner
        .blob(&kept)
        .await
        .expect("the store answers")
        .expect("kept");
    assert!(held.pinned, "a blob another unit made lost its pin");
}

/// A blob whose pin a reversal took off, made again by a unit the journal does
/// not keep either, has its pin taken off again by that unit's reversal: a row
/// that names a blob is not a committed unit holding it.
///
/// **A row is not a pin**: with every row taken for one, the second reversal
/// would leave the blob pinned for good — held by no unit the journal kept, and
/// deleted by no sweep — which this test refuses.
#[tokio::test]
async fn a_blob_made_again_after_its_pin_came_off_is_unpinned_by_the_next_reversal() {
    let (fixture, told) = with(&[], "").await;
    let manifest = PluginManifest::parse(
        "id = \"noctua:reference\"\nnamespace = \"reference\"\ncapabilities = [\"store\", \
         \"blobs.read\", \"blobs.write\"]\n",
    )
    .expect("a manifest granting the blobs");
    fixture
        .kernel
        .load(manifest, &nc_kernel::component_for_tests("reference"))
        .await
        .expect("loads");
    told.lock().expect("nothing panics holding it").refuses =
        Some(|entry: &Entry| entry.record.result == Outcome::UnitCommitted);
    for _ in 0..2 {
        fixture
            .as_("ana", "reference.blob.put", &blob("twice", 2))
            .await
            .expect_err("the journal did not take the close");
    }
    told.lock().expect("nothing panics holding it").refuses = None;
    let twice = nc_kernel::blobs::Hash::of(b"twicetwice").to_string();
    let row = fixture
        .store
        .inner
        .blob(&twice)
        .await
        .expect("the store answers")
        .expect("the reversal keeps the row for the sweep");
    assert_eq!(
        (row.pinned, row.unpinned_at_ms),
        (false, Some(NOW_MS)),
        "a blob no kept unit holds stayed pinned after two reversals: {row:?}"
    );
}

/// `blob.put`'s arguments: `text` written `times` over, about the note `photo`.
fn blob(text: &str, times: i64) -> String {
    Value::object([
        ("note", Value::text("photo")),
        ("text", Value::text(text)),
        ("times", Value::Integer(times)),
    ])
    .to_canonical()
}

/// The arguments of a command about the blob `hash`.
fn hash_of(hash: &str) -> String {
    Value::object([("hash", Value::text(hash)), ("note", Value::text("photo"))]).to_canonical()
}

/// The text field `name` of an answer.
fn text_of(answered: &str, name: &str) -> String {
    nc_canon::parse(answered)
        .ok()
        .and_then(Value::into_object)
        .and_then(|mut fields| fields.remove(name))
        .and_then(Value::into_text)
        .unwrap_or_else(|| panic!("`{answered}` has no text `{name}`"))
}

/// A journal that did not take a unit's close, and a store that then refused
/// the unit's reversal, leave the unit committed in the store and open in the
/// journal — and the caller is told exactly that, never that it was reverted.
///
/// What it holds against: the reversal's refusal dropped, and the caller told
/// "reverted at once" while the store keeps the write and every later dispatch
/// can read it.
#[tokio::test]
async fn a_reversal_the_store_refuses_leaves_the_unit_committed_and_says_so() {
    let (fixture, told) = reference().await;
    fixture
        .store
        .refuses_reversals
        .store(true, Ordering::SeqCst);
    told.lock().expect("nothing panics holding it").refuses =
        Some(|entry: &Entry| entry.record.result == Outcome::UnitCommitted);
    let refused = fixture
        .as_(
            "ana",
            "reference.note.put",
            &note("photo", Some("kept after all")),
        )
        .await;
    told.lock().expect("nothing panics holding it").refuses = None;
    fixture
        .store
        .refuses_reversals
        .store(false, Ordering::SeqCst);
    let refusal = refused.expect_err("the journal did not take the close");
    assert_eq!(
        (refusal.kind(), refusal.code()),
        (RefusalKind::Unavailable, ErrorCode::JournalUnavailable)
    );
    for said in [
        "the store refused the reversal",
        "stays committed in the store",
        "until the next prune or the recovery on open reverts it",
    ] {
        assert!(refusal.sentence().contains(said), "{said}: {refusal}");
    }
    assert!(
        !refusal.sentence().contains("reverted at once"),
        "{refusal}"
    );
    assert_eq!(
        fixture
            .held("noctua:reference", "note/photo")
            .await
            .as_deref(),
        Some(r#"{"text":"kept after all"}"#),
        "the store holds what the unit committed"
    );
    let units = fixture.units_of("ana").await;
    assert!(
        units
            .iter()
            .any(|unit| unit.meta.label == "reference.note.put"
                && unit.state == nc_local::UnitState::Committed),
        "{units:#?}"
    );
    assert!(
        units
            .iter()
            .all(|unit| !matches!(unit.meta.kind, UnitKind::Undo { .. })),
        "a reversal the store refused was kept: {units:#?}"
    );
    let entries = fixture.entries().await;
    assert_eq!(
        brackets(&entries),
        [(nc_journal::unit::OPENS, Outcome::UnitBegun)],
        "the unit is left open in the journal, for the recovery to find"
    );
    let unit = only(&entries, "reference.note.put")
        .record
        .work_unit
        .expect("a unit");
    assert_eq!(
        fixture.logged("kernel.revert_failed", "unit"),
        [unit.to_crockford()]
    );
    assert_eq!(
        fixture.logged("kernel.revert_failed", "code"),
        [ErrorCode::DbUnreachable.code()]
    );
    let got = fixture.log.named("kernel.unit_aborted");
    assert!(
        got.is_empty(),
        "a unit the store still holds was logged as aborted: {got:?}"
    );
}

/// A close the journal took and then answered with an error — a connection
/// lost after the journal's own commit — is a unit kept: the journal is asked
/// how the unit closed before anything is reverted.
///
/// What it holds against: such a unit reverted while the hash-chained journal
/// records it as committed.
#[tokio::test]
async fn a_close_the_journal_took_before_its_answer_was_lost_is_kept() {
    let (fixture, told) = reference().await;
    told.lock()
        .expect("nothing panics holding it")
        .lands_then_refuses = Some(|entry: &Entry| entry.record.result == Outcome::UnitCommitted);
    let answered = fixture
        .as_("ana", "reference.note.put", &note("photo", Some("kept")))
        .await;
    told.lock()
        .expect("nothing panics holding it")
        .lands_then_refuses = None;
    answered.expect("the journal took the close");
    assert_eq!(
        fixture
            .held("noctua:reference", "note/photo")
            .await
            .as_deref(),
        Some(r#"{"text":"kept"}"#)
    );
    let entries = fixture.entries().await;
    assert_eq!(
        brackets(&entries),
        [
            (nc_journal::unit::OPENS, Outcome::UnitBegun),
            (nc_journal::unit::CLOSES, Outcome::UnitCommitted),
        ]
    );
    let units = fixture.units_of("ana").await;
    assert!(
        units
            .iter()
            .all(|unit| unit.state == nc_local::UnitState::Committed
                && unit.meta.kind == UnitKind::Step),
        "a unit the journal kept was reverted: {units:#?}"
    );
    assert_eq!(
        fixture.log.named("kernel.unit_aborted"),
        [] as [nc_kernel::logging::CapturedLine; 0]
    );
}

/// A unit whose close the journal took and then answered with an error is
/// kept whole: the timer it set is armed, and it is no longer held from the
/// pruner.
///
/// **The arm of `units::close` that keeps such a unit is read here**: reduced
/// to `return Ok(())`, it would leave the unit's timers, events and surfaces
/// dropped and the unit held from the pruner until the next open.
#[tokio::test]
async fn a_unit_whose_close_landed_before_its_answer_was_lost_arms_its_timer() {
    let (fixture, told) = putting_off().await;
    told.lock()
        .expect("nothing panics holding it")
        .lands_then_refuses = Some(|entry: &Entry| entry.record.result == Outcome::UnitCommitted);
    let answered = fixture.as_("ana", "reference.timer.set", &a_second()).await;
    told.lock()
        .expect("nothing panics holding it")
        .lands_then_refuses = None;
    answered.expect("the journal took the close");
    let got = fixture
        .store
        .inner
        .holders()
        .await
        .expect("the store answers");
    assert!(
        got.is_empty(),
        "a unit the journal kept is still held from the pruner: {got:?}"
    );
    fixture.clock.advance(1_000);
    assert_eq!(
        fixture.kernel.pump().await.fired,
        1,
        "the timer a kept unit set was never armed"
    );
}

/// An undo whose close the journal took and then answered with an error is
/// kept, and logged as the step it reversed, as one whose close was answered.
#[tokio::test]
async fn an_undo_whose_close_landed_before_its_answer_was_lost_is_logged_as_reversed() {
    let (fixture, told) = reference().await;
    fixture
        .as_("ana", "reference.note.put", &note("photo", Some("kept")))
        .await
        .expect("written");
    told.lock()
        .expect("nothing panics holding it")
        .lands_then_refuses = Some(|entry: &Entry| entry.record.result == Outcome::UnitCommitted);
    let undone = fixture.as_("ana", "kernel.undo", "{}").await;
    told.lock()
        .expect("nothing panics holding it")
        .lands_then_refuses = None;
    undone.expect("the journal took the undo's close");
    assert_eq!(fixture.held("noctua:reference", "note/photo").await, None);
    let reversed = fixture.log.named("kernel.step_reversed");
    assert_eq!(
        reversed.len(),
        1,
        "the undo the journal kept is not logged as reversing its step: {reversed:#?}"
    );
    assert_eq!(reversed[0].text("direction").as_deref(), Some("undo"));
    let got = fixture
        .store
        .inner
        .holders()
        .await
        .expect("the store answers");
    assert!(
        got.is_empty(),
        "an undo the journal kept is still held from the pruner: {got:?}"
    );
}

/// A close the journal answered with an error, when the journal then cannot
/// say whether it took it, is reverted — and the caller and the log are told
/// that the journal may record it as kept.
#[tokio::test]
async fn a_close_the_journal_cannot_say_it_took_is_reverted_and_says_so() {
    let (fixture, told) = reference().await;
    {
        let mut told = told.lock().expect("nothing panics holding it");
        told.lands_then_refuses =
            Some(|entry: &Entry| entry.record.result == Outcome::UnitCommitted);
        told.then_says_nothing = true;
    }
    let refused = fixture
        .as_("ana", "reference.note.put", &note("photo", Some("unsure")))
        .await;
    *told.lock().expect("nothing panics holding it") = Told::default();
    let refusal = refused.expect_err("reverted");
    assert_eq!(
        (refusal.kind(), refusal.code()),
        (RefusalKind::Unavailable, ErrorCode::JournalUnavailable)
    );
    for said in [
        "could not then say whether it took it",
        "reverted at once",
        "may record it as kept",
    ] {
        assert!(refusal.sentence().contains(said), "{said}: {refusal}");
    }
    assert_eq!(fixture.held("noctua:reference", "note/photo").await, None);
    let units = fixture.units_of("ana").await;
    assert!(
        units
            .iter()
            .any(|unit| unit.meta.label == "reference.note.put"
                && matches!(unit.state, nc_local::UnitState::Undone { .. })),
        "{units:#?}"
    );
    let entries = fixture.entries().await;
    assert_eq!(
        brackets(&entries),
        [
            (nc_journal::unit::OPENS, Outcome::UnitBegun),
            (nc_journal::unit::CLOSES, Outcome::UnitCommitted),
        ],
        "the journal took the close it could not say it took"
    );
    assert_eq!(
        fixture.logged("kernel.unit_aborted", "cause"),
        ["journal_unknown"]
    );
}

#[tokio::test]
async fn an_irreversible_command_needs_a_confirmation_the_kernel_issued() {
    let fixture = reference_and_peer().await;
    let ana = fixture.principal("ana");
    let args = note("photo", None);
    let done = "reference.irreversible.do";
    let unconfirmed = fixture.as_("ana", done, &args).await;
    assert!(is(
        &unconfirmed,
        RefusalKind::Denied,
        ErrorCode::KernelUnconfirmed
    ));
    // **Issued for other arguments**: not redeemed, and kept for its own.
    let confirmed = fixture.kernel.confirmation_for_tests(&ana, done, &args);
    let caller = Caller::new(ana.clone()).confirmed(confirmed);
    let wrong = fixture
        .kernel
        .dispatch(&caller, done, &note("diary", None))
        .await;
    assert!(is(
        &wrong,
        RefusalKind::Denied,
        ErrorCode::KernelUnconfirmed
    ));
    // **Nested**: the confirmation is the top dispatch's, never a nested one's.
    let nested = fixture
        .kernel
        .dispatch(&caller, "reference.relay.write", &relay(done, &args, None))
        .await
        .expect("the relay itself is allowed");
    assert!(
        text(&nested, "refused")
            .starts_with(&format!("denied: [{}] ", ErrorCode::KernelUnconfirmed)),
        "{nested}"
    );
    assert_eq!(fixture.held("noctua:reference", "done/photo").await, None);
    // The same confirmation, for the dispatch it was issued for, at the top.
    fixture
        .kernel
        .dispatch(&caller, done, &args)
        .await
        .expect("confirmed");
    assert_eq!(
        fixture
            .held("noctua:reference", "done/photo")
            .await
            .as_deref(),
        Some("true")
    );
    // **Reused**: once redeemed, it is no confirmation.
    let reused = fixture.kernel.dispatch(&caller, done, &args).await;
    assert!(is(
        &reused,
        RefusalKind::Denied,
        ErrorCode::KernelUnconfirmed
    ));
    // **Expired**: past its time, it is no confirmation either.
    let late = Caller::new(ana.clone())
        .confirmed(fixture.kernel.confirmation_for_tests(&ana, done, &args));
    fixture.clock.advance(KERNEL.confirmation_ttl_ms + 1);
    let expired = fixture.kernel.dispatch(&late, done, &args).await;
    assert!(is(
        &expired,
        RefusalKind::Denied,
        ErrorCode::KernelUnconfirmed
    ));
    let entries = fixture.entries().await;
    let unconfirmed_entries = dispatched(&entries)
        .into_iter()
        .filter(|entry| deciding(entry) == Some("kernel.irreversible_needs_confirmation"))
        .count();
    assert_eq!(
        unconfirmed_entries, 5,
        "every unconfirmed dispatch is journaled with the sentinel"
    );
}

/// A confirmation is not spent on a dispatch whose decision could not be
/// evaluated, and is honoured by the next one it was issued for.
///
/// The directory answers the realm at an epoch past what the decision point's
/// schema holds, so Cedar's request is refused (`[NC-POL-0005]`). **Not
/// redeemed before the request is built**: redeemed then, the person's
/// single-use confirmation would go with a dispatch the kernel never decided.
#[tokio::test]
async fn a_confirmation_is_not_spent_on_a_decision_that_could_not_be_evaluated() {
    let fixture = reference_and_peer().await;
    let ana = fixture.principal("ana");
    let args = note("photo", None);
    let done = "reference.irreversible.do";
    let caller = Caller::new(ana.clone())
        .confirmed(fixture.kernel.confirmation_for_tests(&ana, done, &args));
    *fixture.call.lock().expect("nothing panics holding it") = Call::Realm;
    *fixture.mode.lock().expect("nothing panics holding it") = Mode::Epoch(u64::MAX);
    let undecided = fixture.kernel.dispatch(&caller, done, &args).await;
    *fixture.mode.lock().expect("nothing panics holding it") = Mode::Answers;
    assert!(
        is(
            &undecided,
            RefusalKind::Denied,
            ErrorCode::PolicyEvaluationFailed
        ),
        "{undecided:?}"
    );
    fixture
        .kernel
        .dispatch(&caller, done, &args)
        .await
        .expect("the confirmation is honoured by the dispatch it was issued for");
    let reused = fixture.kernel.dispatch(&caller, done, &args).await;
    assert!(
        is(&reused, RefusalKind::Denied, ErrorCode::KernelUnconfirmed),
        "redeemed once, it is no confirmation: {reused:?}"
    );
}

/// A confirmation lives no longer than `confirmation_ttl_ms` when the
/// kernel's clock steps back after it was issued.
///
/// **Not on that clock alone**: measured on it alone, with a saturating
/// difference, its life would gain an hour from a clock stepped back an hour.
#[tokio::test]
async fn a_confirmation_lives_no_longer_than_its_time_when_the_clock_steps_back() {
    let fixture = reference_and_peer().await;
    let ana = fixture.principal("ana");
    let args = note("photo", None);
    let done = "reference.irreversible.do";
    let caller = Caller::new(ana.clone())
        .confirmed(fixture.kernel.confirmation_for_tests(&ana, done, &args));
    fixture
        .clock
        .now
        .store(NOW_MS - 3_600_000, Ordering::SeqCst);
    fixture.clock.advance(KERNEL.confirmation_ttl_ms + 1);
    let late = fixture.kernel.dispatch(&caller, done, &args).await;
    assert!(
        is(&late, RefusalKind::Denied, ErrorCode::KernelUnconfirmed),
        "redeemed {} ms of a clock that stepped back after its issue: {late:?}",
        KERNEL.confirmation_ttl_ms + 1
    );
}

#[tokio::test]
async fn a_destructive_command_needs_an_approval_the_kernel_issued() {
    let (fixture, _) = reference().await;
    fixture
        .as_("ana", "reference.note.put", &note("photo", Some("x")))
        .await
        .expect("written");
    let ana = fixture.principal("ana");
    let args = note("photo", None);
    let delete = "reference.note.delete";
    let unapproved = fixture.as_("ana", delete, &args).await;
    assert!(is(
        &unapproved,
        RefusalKind::Denied,
        ErrorCode::PolicyDenied
    ));
    let caller =
        Caller::new(ana.clone()).approved(fixture.kernel.approval_for_tests(&ana, delete, &args));
    fixture
        .kernel
        .dispatch(&caller, delete, &args)
        .await
        .expect("a quorum approved it");
    assert_eq!(fixture.held("noctua:reference", "note/photo").await, None);
    let reused = fixture.kernel.dispatch(&caller, delete, &args).await;
    assert!(is(&reused, RefusalKind::Denied, ErrorCode::PolicyDenied));
    let entries = fixture.entries().await;
    let decided: Vec<Option<&str>> = dispatched(&entries)
        .into_iter()
        .filter(|entry| command_of(entry).as_deref() == Some(delete))
        .map(deciding)
        .collect();
    assert_eq!(
        decided,
        [
            Some("core.destructive_needs_quorum"),
            Some("family.an_owner_governs_their_own_content"),
            Some("core.destructive_needs_quorum"),
        ]
    );
}

#[tokio::test]
async fn malformed_arguments_are_refused_before_the_plugin_runs() {
    let (fixture, _) = reference().await;
    let count = "reference.executions.count";
    let deep = format!(
        r#"{{"at_most":1,"note":"photo","verbose":{}{}}}"#,
        "[".repeat(65),
        "]".repeat(65)
    );
    for (case, args, code) in [
        (
            "a float",
            r#"{"at_most":1.5,"note":"photo"}"#.to_owned(),
            ErrorCode::CanonNumber,
        ),
        (
            "a duplicate key",
            r#"{"at_most":1,"note":"photo","note":"photo"}"#.to_owned(),
            ErrorCode::CanonUnparsable,
        ),
        ("depth 65", deep, ErrorCode::CanonUnparsable),
        (
            "an unknown property",
            r#"{"at_most":1,"note":"photo","what":1}"#.to_owned(),
            ErrorCode::ValidationArgumentsInvalid,
        ),
        (
            "a wrong type",
            r#"{"at_most":"1","note":"photo"}"#.to_owned(),
            ErrorCode::ValidationArgumentsInvalid,
        ),
        (
            "a missing required",
            r#"{"note":"photo"}"#.to_owned(),
            ErrorCode::ValidationArgumentsInvalid,
        ),
        (
            "an entity id no entity can have",
            r#"{"at_most":1,"note":"a photo"}"#.to_owned(),
            ErrorCode::ValidationResourceIdMalformed,
        ),
    ] {
        let refused = fixture.as_("ana", count, &args).await;
        assert!(
            is(&refused, RefusalKind::Invalid, code),
            "{case}: {refused:?}"
        );
    }
    let past = format!(
        r#"{{"at_most":1,"note":"photo","pad":"{}"}}"#,
        "x".repeat(usize::try_from(KERNEL.payload_bytes).expect("fits"))
    );
    assert!(is(
        &fixture.as_("ana", count, &past).await,
        RefusalKind::Invalid,
        ErrorCode::ValidationOutOfRange
    ));
    assert_eq!(
        fixture.executions("reference").await,
        1,
        "a command refused for its arguments reached the plugin"
    );
    let entries = fixture.entries().await;
    let refused = dispatched(&entries)
        .into_iter()
        .filter(|entry| deciding(entry) == Some("kernel.malformed_arguments"))
        .count();
    assert_eq!(refused, 8, "every refusal is journaled: {entries:#?}");
}

#[tokio::test]
async fn a_name_no_command_has_or_could_have_is_refused_and_journaled_without_its_text() {
    let (fixture, _) = reference().await;
    fixture.clock.advance(3);
    let unknown = fixture.as_("ana", "reference.note.rename", "{}").await;
    assert!(is(
        &unknown,
        RefusalKind::NotFound,
        ErrorCode::KernelUnknownCommand
    ));
    fixture.clock.advance(4);
    let malformed = fixture.as_("ana", "Ana's Photo; DROP", "{}").await;
    assert!(is(
        &malformed,
        RefusalKind::Invalid,
        ErrorCode::KernelMalformedName
    ));
    let entries = fixture.entries().await;
    let recorded: Vec<(String, String, Option<&str>, u64)> = entries
        .iter()
        .map(|entry| {
            (
                entry.record.action.clone(),
                entry.record.resource.to_canonical(),
                deciding(entry),
                entry.record.recorded_at,
            )
        })
        .collect();
    assert_eq!(
        recorded,
        [
            (
                "kernel.dispatch".to_owned(),
                r#"{"command":"reference.note.rename"}"#.to_owned(),
                Some("kernel.unknown_command"),
                NOW_MS + 3
            ),
            (
                "kernel.dispatch".to_owned(),
                r#"{"refused":"malformed-name"}"#.to_owned(),
                Some("kernel.malformed_name"),
                NOW_MS + 7
            ),
        ]
    );
    assert_eq!(
        fixture.logged("kernel.dispatch_refused", "reason"),
        ["unknown_command", "malformed_name"]
    );
    // **And the code each was refused with**: a log naming every refusal by a
    // denial's code fails here.
    assert_eq!(
        fixture.logged("kernel.dispatch_refused", "code"),
        [
            ErrorCode::KernelUnknownCommand.code(),
            ErrorCode::KernelMalformedName.code()
        ]
    );
}

#[tokio::test]
async fn a_nested_dispatch_runs_as_the_principal_of_its_tree() {
    let fixture = reference_and_peer().await;
    let put = note("photo", Some("from inside"));
    let allowed = fixture
        .as_(
            "ana",
            "reference.relay.write",
            &relay("peer.note.put", &put, None),
        )
        .await
        .expect("ana relays a write of her own photo");
    assert_eq!(field(&allowed, "answered"), Value::Integer(1), "{allowed}");
    assert_eq!(
        fixture
            .held("noctua:reference-peer", "note/photo")
            .await
            .as_deref(),
        Some(r#"{"text":"from inside"}"#),
        "the nested write was kept with the unit"
    );
    // carla relays from her own recipe, which she may write, into ana's photo,
    // which she may not: the nested dispatch is decided as carla.
    let recipe = Value::object([
        ("args", Value::text(note("photo", Some("not mine")))),
        ("name", Value::text("peer.note.put")),
        ("note", Value::text("recipe")),
    ])
    .to_canonical();
    let refused = fixture
        .as_("carla", "reference.relay.write", &recipe)
        .await
        .expect("carla relays from her own recipe");
    assert!(
        text(&refused, "refused").starts_with(&format!("denied: [{}] ", ErrorCode::PolicyDenied)),
        "{refused}"
    );
    let entries = fixture.entries().await;
    let nested: Vec<(&str, Outcome, bool)> = dispatched(&entries)
        .into_iter()
        .filter(|entry| command_of(entry).as_deref() == Some("peer.note.put"))
        .map(|entry| {
            (
                entry.record.principal.as_str(),
                entry.record.result,
                entry.record.work_unit.is_some(),
            )
        })
        .collect();
    assert_eq!(
        nested,
        [
            ("ana", Outcome::Allowed, true),
            ("carla", Outcome::Denied, true)
        ],
        "each nested dispatch is its tree's principal's, inside its unit"
    );
    for relayed in dispatched(&entries)
        .into_iter()
        .filter(|entry| command_of(entry).as_deref() == Some("peer.note.put"))
    {
        let top = dispatched(&entries)
            .into_iter()
            .find(|entry| {
                command_of(entry).as_deref() == Some("reference.relay.write")
                    && entry.record.principal == relayed.record.principal
            })
            .expect("the relay's own entry");
        assert_eq!(
            top.record.work_unit, relayed.record.work_unit,
            "one unit per tree"
        );
    }
}

#[tokio::test]
async fn a_command_that_changes_state_is_refused_beneath_one_that_only_reads() {
    let (fixture, _) = reference().await;
    let put = note("photo", Some("x"));
    let answer = fixture
        .as_(
            "ana",
            "reference.relay.read",
            &relay("reference.note.put", &put, None),
        )
        .await
        .expect("the read itself is allowed");
    assert!(
        text(&answer, "refused").starts_with(&format!("denied: [{}] ", ErrorCode::KernelReadOnly)),
        "{answer}"
    );
    assert_eq!(fixture.held("noctua:reference", "note/photo").await, None);
    let entries = fixture.entries().await;
    assert_eq!(
        deciding(only(&entries, "reference.note.put")),
        Some("kernel.read_only")
    );
}

#[tokio::test]
async fn a_nested_dispatch_into_the_plugin_still_running_is_refused_as_reentrant() {
    let (fixture, _) = reference().await;
    let answer = fixture
        .as_(
            "ana",
            "reference.relay.write",
            &relay("reference.note.get", &note("photo", None), None),
        )
        .await
        .expect("the relay itself answers");
    assert!(
        text(&answer, "refused").starts_with(&format!("denied: [{}] ", ErrorCode::KernelReentrant)),
        "{answer}"
    );
    assert_eq!(
        fixture.executions("reference").await,
        2,
        "the relay ran, the dispatch into itself did not, and the plugin lives on"
    );
    let entries = fixture.entries().await;
    assert_eq!(
        deciding(only(&entries, "reference.note.get")),
        Some("kernel.reentrant")
    );
}

#[tokio::test]
async fn a_command_that_fails_or_traps_leaves_nothing_it_wrote() {
    let (fixture, _) = reference().await;
    let put = note("photo", Some("half done"));
    let failed = fixture
        .as_("ana", "reference.note.put-then-fail", &put)
        .await;
    assert!(
        is(
            &failed,
            RefusalKind::Invalid,
            ErrorCode::PluginCommandRefused
        ),
        "{failed:?}"
    );
    let trapped = fixture
        .as_("ana", "reference.note.put-then-trap", &put)
        .await;
    assert!(
        is(&trapped, RefusalKind::Internal, ErrorCode::PluginTrapped),
        "{trapped:?}"
    );
    assert_eq!(fixture.held("noctua:reference", "note/photo").await, None);
    assert_eq!(
        fixture.executions("reference").await,
        1,
        "the plugin was started again after it trapped: a fresh instance"
    );
    let entries = fixture.entries().await;
    for (command, failure) in [
        ("reference.note.put-then-fail", "refused"),
        ("reference.note.put-then-trap", "trapped"),
    ] {
        let entry = only(&entries, command);
        assert_eq!(entry.record.result, Outcome::Failed);
        let mut resource = entry
            .record
            .resource
            .clone()
            .into_object()
            .expect("an object");
        assert_eq!(
            resource
                .remove("failure")
                .and_then(Value::into_text)
                .as_deref(),
            Some(failure)
        );
    }
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry.record.result == Outcome::UnitAborted)
            .count(),
        2
    );
    assert_eq!(fixture.logged("kernel.plugin_killed", "cause"), ["trap"]);
    assert_eq!(fixture.log.named("kernel.plugin_restarted").len(), 1);
    assert_eq!(
        fixture.logged("kernel.unit_aborted", "cause"),
        ["failed", "failed"]
    );
}

#[tokio::test]
async fn a_nested_command_that_fails_having_written_aborts_the_whole_unit() {
    let fixture = reference_and_peer().await;
    let put = note("photo", Some("half done"));
    // **In the case of what doomed it**: a doomed unit refused as `internal`
    // whatever doomed it fails here, and would leave a caller unable to tell a
    // refusal it may retry from one it may not.
    for (nested, kind, code) in [
        (
            "peer.note.put-then-fail",
            RefusalKind::Invalid,
            ErrorCode::PluginCommandRefused,
        ),
        (
            "peer.note.put-then-trap",
            RefusalKind::Internal,
            ErrorCode::PluginTrapped,
        ),
    ] {
        let both = Value::object([
            ("args", Value::text(put.clone())),
            ("name", Value::text(nested)),
            ("note", Value::text("photo")),
        ])
        .to_canonical();
        fixture
            .as_("ana", "reference.note.put", &note("diary", Some("before")))
            .await
            .expect("written");
        let refused = fixture.as_("ana", "reference.relay.write", &both).await;
        let refusal = refused.as_ref().expect_err("the unit is not kept");
        assert_eq!(
            (refusal.kind(), refusal.code()),
            (kind, ErrorCode::KernelUnitAborted),
            "{nested}: {refusal}"
        );
        assert!(refusal.sentence().contains(&code.to_string()), "{refusal}");
        assert_eq!(
            fixture.held("noctua:reference-peer", "note/photo").await,
            None,
            "{nested}: the nested write was kept"
        );
    }
    assert_eq!(
        fixture.executions("peer").await,
        1,
        "the peer that trapped was started again"
    );
    assert_eq!(
        fixture.executions("reference").await,
        5,
        "the relaying plugin lived on"
    );
    assert_eq!(
        fixture.logged("kernel.unit_aborted", "cause"),
        ["nested", "nested"]
    );
}

/// A nested command that traps dooms the unit it ran in by itself, having
/// written nothing: what a killed plugin staged cannot be known, and its
/// caller may not keep the unit by ignoring the refusal.
///
/// The peer traps here having written nothing, so a decision point that
/// doomed a unit only for a nested failure that wrote would keep it, and fails
/// this test.
#[tokio::test]
async fn a_trap_dooms_the_unit_it_ran_in_though_it_wrote_nothing() {
    let fixture = reference_and_peer().await;
    let refused = fixture
        .as_(
            "ana",
            "reference.relay.write",
            &relay("peer.abuse.trap", &note("photo", None), None),
        )
        .await;
    let refusal = refused.expect_err("the unit is not kept");
    assert_eq!(refusal.code(), ErrorCode::KernelUnitAborted, "{refusal}");
    assert!(
        refusal
            .sentence()
            .contains(&ErrorCode::PluginTrapped.to_string()),
        "{refusal}"
    );
    let entries = fixture.entries().await;
    assert_eq!(
        brackets(&entries),
        [
            (nc_journal::unit::OPENS, Outcome::UnitBegun),
            (nc_journal::unit::CLOSES, Outcome::UnitAborted),
        ]
    );
    assert_eq!(
        only(&entries, "peer.abuse.trap").record.result,
        Outcome::Failed
    );
    let units = fixture.units_of("ana").await;
    assert!(
        units
            .iter()
            .all(|unit| unit.meta.label != "reference.relay.write"),
        "the unit was kept: {units:#?}"
    );
    assert_eq!(fixture.logged("kernel.plugin_killed", "cause"), ["trap"]);
    assert_eq!(fixture.logged("kernel.unit_aborted", "cause"), ["nested"]);
    // **Every field of each line, not its name and one field**: a log naming
    // another unit as aborted, or another slot as killed, fails here. The unit
    // is the one the journal bracketed, and the plugin killed is the peer,
    // loaded second.
    let unit = entries
        .iter()
        .find(|entry| is_a_bracket(entry))
        .and_then(|entry| entry.record.work_unit)
        .expect("the unit's opening names it");
    assert_eq!(
        fixture.logged("kernel.unit_aborted", "unit"),
        [unit.to_crockford()]
    );
    assert_eq!(fixture.logged("kernel.plugin_killed", "slot"), ["1"]);
    assert_eq!(
        fixture.logged("kernel.plugin_killed", "code"),
        [ErrorCode::PluginTrapped.code()]
    );
    assert_eq!(fixture.logged("kernel.plugin_restarted", "slot"), ["1"]);
    assert_eq!(
        fixture.executions("peer").await,
        1,
        "the peer that trapped was started again"
    );
}

/// A nested dispatch whose entry the journal did not take dooms the unit it
/// ran in, whatever its caller does with the refusal: what it wrote is not
/// kept, since nothing in the journal says it was done.
///
/// The entry refused here is the nested dispatch's: with the doom taken out of
/// the nested case, this test fails, the nested write committed with no entry
/// recording it.
#[tokio::test]
async fn a_nested_dispatch_whose_entry_the_journal_did_not_take_leaves_nothing_it_wrote() {
    let (fixture, told) = with(
        &[
            ("noctua:reference", "reference"),
            ("noctua:reference-peer", "peer"),
        ],
        "",
    )
    .await;
    told.lock().expect("nothing panics holding it").refuses =
        Some(|entry: &Entry| command_of(entry).as_deref() == Some("peer.note.put"));
    let refused = fixture
        .as_(
            "ana",
            "reference.relay.write",
            &relay("peer.note.put", &note("photo", Some("unjournaled")), None),
        )
        .await;
    told.lock().expect("nothing panics holding it").refuses = None;
    let refusal = refused.expect_err("the unit is not kept");
    assert_eq!(
        (refusal.kind(), refusal.code()),
        (RefusalKind::Unavailable, ErrorCode::KernelUnitAborted),
        "a journal that was down is a refusal the caller may retry: {refusal}"
    );
    assert!(
        refusal
            .sentence()
            .contains(&ErrorCode::JournalUnavailable.to_string()),
        "{refusal}"
    );
    assert_eq!(
        fixture.held("noctua:reference-peer", "note/photo").await,
        None,
        "the nested write outlived the entry the journal did not take"
    );
    let entries = fixture.entries().await;
    assert_eq!(
        brackets(&entries),
        [
            (nc_journal::unit::OPENS, Outcome::UnitBegun),
            (nc_journal::unit::CLOSES, Outcome::UnitAborted),
        ]
    );
    assert_eq!(
        dispatched(&entries)
            .into_iter()
            .filter_map(command_of)
            .collect::<Vec<_>>(),
        ["reference.relay.write"]
    );
    assert_eq!(fixture.logged("kernel.unit_aborted", "cause"), ["nested"]);
}

/// A nested command that fails having written nothing leaves the unit to its
/// caller, which may go on and have its unit kept: only a failure that wrote,
/// or a trap, dooms it.
///
/// This half of the rule is read here: a decision point dooming the unit for
/// every nested failure fails this test.
#[tokio::test]
async fn a_nested_failure_that_wrote_nothing_leaves_the_unit_to_its_caller() {
    let fixture = reference_and_peer().await;
    let answer = fixture
        .as_(
            "ana",
            "reference.relay.write",
            &relay("peer.echo", &note("photo", Some("not json")), None),
        )
        .await
        .expect("the relay goes on, and its unit is kept");
    assert!(
        text(&answer, "refused").starts_with(&format!(
            "internal: [{}] ",
            ErrorCode::PluginMalformedResult
        )),
        "{answer}"
    );
    let entries = fixture.entries().await;
    assert_eq!(
        brackets(&entries),
        [
            (nc_journal::unit::OPENS, Outcome::UnitBegun),
            (nc_journal::unit::CLOSES, Outcome::UnitCommitted),
        ]
    );
    assert_eq!(only(&entries, "peer.echo").record.result, Outcome::Failed);
    assert!(
        fixture
            .units_of("ana")
            .await
            .iter()
            .any(|unit| unit.meta.label == "reference.relay.write"),
        "the relay's unit was not kept"
    );
    assert_eq!(
        fixture.log.named("kernel.unit_aborted"),
        [] as [nc_kernel::logging::CapturedLine; 0]
    );
}

#[tokio::test]
async fn a_plugin_that_does_not_come_back_as_it_was_after_a_trap_is_not_run_again() {
    for variant in [Variant::Trap, Variant::EmptyTitle] {
        // **The peer, loaded second**, so the slot its lines name is not the
        // one a field left at its default would.
        let fixture = reference_and_peer().await;
        // How it activates is read from its store at every start: the next
        // one traps, or declares other than what was registered.
        seeded(
            &fixture.store.inner,
            "noctua:reference-peer",
            ACTIVATION,
            &activating("peer", variant),
        )
        .await;
        let trapped = fixture
            .as_("ana", "peer.note.put-then-trap", &note("photo", Some("x")))
            .await;
        assert!(
            is(
                &trapped,
                RefusalKind::Internal,
                ErrorCode::PluginQuarantined
            ),
            "{variant:?}: {trapped:?}"
        );
        let after = fixture
            .as_("ana", "peer.note.get", &note("photo", None))
            .await;
        assert!(
            is(
                &after,
                RefusalKind::Unavailable,
                ErrorCode::PluginQuarantined
            ),
            "{variant:?}: {after:?}"
        );
        let entries = fixture.entries().await;
        let entry = only(&entries, "peer.note.get");
        assert_eq!(entry.record.result, Outcome::Failed);
        assert_eq!(
            fixture.logged("kernel.dispatch_refused", "reason"),
            ["quarantined"]
        );
        assert_eq!(
            fixture.logged("kernel.dispatch_refused", "code"),
            [ErrorCode::PluginQuarantined.code()]
        );
        assert_eq!(fixture.logged("kernel.plugin_quarantined", "slot"), ["1"]);
        assert_eq!(
            fixture.logged("kernel.plugin_quarantined", "code"),
            [ErrorCode::PluginQuarantined.code()]
        );
        assert_eq!(fixture.logged("kernel.plugin_killed", "slot"), ["1"]);
        assert_eq!(
            fixture.log.named("kernel.plugin_restarted"),
            [] as [nc_kernel::logging::CapturedLine; 0]
        );
        assert_eq!(
            fixture.executions("reference").await,
            1,
            "{variant:?}: the plugin beside it is not stopped with it"
        );
    }
}

#[tokio::test]
async fn a_dispatch_its_caller_stops_waiting_for_runs_to_its_end() {
    let fixture = reference_and_peer().await;
    // The peer's read of its note waits until it is let go, and says it is
    // waiting: the tree is then inside both plugins at once, with its unit
    // open, when its caller gives up on it.
    fixture.store.stalls.store(true, Ordering::SeqCst);
    let put = note("photo", Some("kept all the same"));
    let relay = relay("peer.note.put", &put, None);
    tokio::select! {
        answered = fixture.as_("ana", "reference.relay.write", &relay) => {
            panic!("the stalled dispatch answered: {answered:?}")
        }
        () = fixture.store.reached.notified() => {}
    }
    fixture.store.stalls.store(false, Ordering::SeqCst);
    fixture.store.released.notify_one();
    // Each later dispatch waits for the tree before it, so these answer once
    // the abandoned one has ended — and a plugin left half-called would refuse
    // them as dispatches into itself, a unit left open as the journal's.
    fixture
        .as_("ana", "peer.note.get", &note("photo", None))
        .await
        .expect("the peer is run again");
    fixture
        .as_("ana", "reference.note.put", &note("photo", Some("after")))
        .await
        .expect("the relay is run again, in a unit of its own");
    assert_eq!(
        fixture
            .held("noctua:reference-peer", "note/photo")
            .await
            .as_deref(),
        Some(r#"{"text":"kept all the same"}"#),
        "the tree its caller left was not run to its end"
    );
    let entries = fixture.entries().await;
    assert!(
        dispatched(&entries)
            .iter()
            .any(
                |entry| command_of(entry).as_deref() == Some("reference.relay.write")
                    && entry.record.result == Outcome::Allowed
            ),
        "the abandoned dispatch was not journaled"
    );
    assert_eq!(
        fixture.log.named("kernel.plugin_killed"),
        [] as [nc_kernel::logging::CapturedLine; 0]
    );
}

#[tokio::test]
async fn a_call_tree_past_its_deadline_dispatches_nothing_more() {
    let fixture = reference_and_peer().await;
    fixture.clock.stepping(KERNEL.dispatch_deadline_ms / 2);
    let answer = fixture
        .as_(
            "ana",
            "reference.relay.write",
            &relay("peer.note.get", &note("photo", None), None),
        )
        .await
        .expect("the relay began within its deadline");
    fixture.clock.stepping(0);
    assert!(
        text(&answer, "refused")
            .starts_with(&format!("unavailable: [{}] ", ErrorCode::KernelDeadline)),
        "{answer}"
    );
    let entries = fixture.entries().await;
    assert_eq!(
        deciding(only(&entries, "peer.note.get")),
        Some("kernel.deadline")
    );
}

/// A command refused past its tree's deadline is logged as the command it
/// names: a write as one that changes state, and a read as one that does not.
///
/// **What it holds against**: a refusal from the tree's depth, deadline or
/// count logged as `mutating: false` whatever it refused, while a refused write
/// is the one a person may be waiting on (`catalog/log-events.toml`).
#[tokio::test]
async fn a_command_refused_past_its_trees_deadline_is_logged_as_the_command_it_names() {
    let fixture = reference_and_peer().await;
    for (relayed, args) in [
        ("peer.note.put", note("photo", Some("late"))),
        ("peer.note.get", note("photo", None)),
    ] {
        fixture.clock.stepping(KERNEL.dispatch_deadline_ms / 2);
        let answer = fixture
            .as_("ana", "reference.relay.write", &relay(relayed, &args, None))
            .await
            .expect("the relay began within its deadline");
        fixture.clock.stepping(0);
        assert!(
            text(&answer, "refused")
                .starts_with(&format!("unavailable: [{}] ", ErrorCode::KernelDeadline)),
            "{relayed}: {answer}"
        );
    }
    let logged: Vec<Value> = fixture
        .log
        .named("kernel.dispatch_refused")
        .into_iter()
        .filter(|line| line.text("reason").as_deref() == Some("deadline"))
        .map(|line| line.fields["mutating"].clone())
        .collect();
    assert_eq!(
        logged,
        [Value::Bool(true), Value::Bool(false)],
        "`peer.note.put` changes state and `peer.note.get` does not"
    );
}

/// A plugin that asks for more dispatches than its tree may make is told at
/// the first one past the count and killed at the next, however many more it
/// would have asked for, and the tree journals those two refusals and no
/// other.
///
/// Refused as a value and journaled each time, the dispatch past the count
/// would let a relay told to go on through its refusals write one entry for
/// every call past the count, bounded by nothing but its fuel. Trapped at the
/// first one past the count, the plugin would be killed whoever had spent it
/// (`isolation.rs`), which is why it is told first.
#[tokio::test]
async fn a_call_tree_dispatches_no_more_than_its_count() {
    let count = KERNEL.dispatches_per_tree;
    for times in [i64::from(count) + 10, 2_048] {
        let fixture = reference_and_peer().await;
        let refused = fixture
            .as_(
                "ana",
                "reference.relay.read",
                &relay_through("peer.note.get", &note("photo", None), times),
            )
            .await;
        assert!(
            is(&refused, RefusalKind::Internal, ErrorCode::PluginTrapped),
            "{times}: the relay was not killed: {refused:?}"
        );
        let refusal = refused.expect_err("killed");
        assert!(
            refusal
                .sentence()
                .contains(&ErrorCode::KernelTooManyDispatches.to_string()),
            "{refusal}"
        );
        let entries = fixture.entries().await;
        assert_eq!(
            decided_by(&entries, "kernel.too_many_dispatches"),
            2,
            "{times}: one entry for the dispatch past the count it was told of, and one \
             for the dispatch it was killed for asking"
        );
        let answered = dispatched(&entries)
            .into_iter()
            .filter(|entry| entry.record.result == Outcome::Allowed)
            .count();
        assert_eq!(
            answered,
            usize::try_from(count - 1).expect("fits"),
            "{times}: the relay is one of the tree's dispatches"
        );
        assert_eq!(
            only(&entries, "reference.relay.read").record.result,
            Outcome::Failed
        );
        assert_eq!(fixture.logged("kernel.plugin_killed", "cause"), ["trap"]);
        assert_eq!(
            fixture.logged("kernel.dispatch_refused", "reason"),
            ["too_many_dispatches", "too_many_dispatches"]
        );
        assert_eq!(
            fixture.logged("kernel.dispatch_refused", "code"),
            [ErrorCode::KernelTooManyDispatches.code(); 2]
        );
        assert_eq!(
            fixture.executions("reference").await,
            1,
            "the relay's plugin was started again"
        );
    }
}

/// A plugin that asks again past the tree's deadline, whatever it is told,
/// spends the tree's count with each refusal and is told at the count and
/// killed past it, as one whose dispatches ran is.
///
/// Were only a dispatch that passed the budgets counted, a relay going on
/// through its refusals past the deadline would be refused as often as its fuel
/// let it ask, each refusal one more entry.
#[tokio::test]
async fn a_plugin_asking_on_past_the_deadline_is_stopped_at_its_trees_count() {
    let fixture = reference_and_peer().await;
    fixture.clock.stepping(KERNEL.dispatch_deadline_ms / 2);
    let refused = fixture
        .as_(
            "ana",
            "reference.relay.read",
            &relay_through("peer.note.get", &note("photo", None), 2_048),
        )
        .await;
    fixture.clock.stepping(0);
    assert!(
        is(&refused, RefusalKind::Internal, ErrorCode::PluginTrapped),
        "the relay was not stopped: {refused:?}"
    );
    let entries = fixture.entries().await;
    assert_eq!(
        decided_by(&entries, "kernel.deadline"),
        usize::try_from(KERNEL.dispatches_per_tree - 1).expect("fits"),
        "each refusal past the deadline spent one of the tree's dispatches"
    );
    assert_eq!(decided_by(&entries, "kernel.too_many_dispatches"), 2);
    assert_eq!(fixture.logged("kernel.plugin_killed", "cause"), ["trap"]);
}

/// A plugin at the deepest level a tree may reach that asks again whatever it
/// is told spends the tree's count with each refusal of the depth, and is
/// told at the count and killed past it.
///
/// With the depth's refusal left uncounted, the deepest relay, going on through
/// its refusals, would be refused as often as its fuel lets it ask.
#[tokio::test]
async fn a_plugin_asking_on_past_the_depth_is_stopped_at_its_trees_count() {
    let depth = usize::try_from(KERNEL.dispatch_depth).expect("fits");
    let ids: Vec<(String, String)> = (0..=depth)
        .map(|at| (format!("noctua:depth-{at}"), format!("depth{at}")))
        .collect();
    let plugins: Vec<(&str, &str)> = ids
        .iter()
        .map(|(id, namespace)| (id.as_str(), namespace.as_str()))
        .collect();
    let (fixture, _) = with(&plugins, "").await;
    let deepest = format!("depth{depth}.relay.read");
    let mut args = relay_through("journal.verify", "{}", 2_048);
    let mut name = deepest.clone();
    for (_, namespace) in ids.iter().rev().skip(1) {
        args = relay(&name, &args, None);
        name = format!("{namespace}.relay.read");
    }
    let answer = fixture
        .as_("ana", &name, &args)
        .await
        .expect("the top relay answers");
    assert!(
        answer.contains(&ErrorCode::KernelTooManyDispatches.to_string()),
        "the deepest relay was not stopped at the count: {answer}"
    );
    let entries = fixture.entries().await;
    assert_eq!(
        decided_by(&entries, "kernel.too_deep"),
        usize::try_from(KERNEL.dispatches_per_tree).expect("fits") - (depth + 1),
        "each refusal of the depth spent one of the tree's dispatches"
    );
    assert_eq!(decided_by(&entries, "kernel.too_many_dispatches"), 2);
    assert_eq!(only(&entries, &deepest).record.result, Outcome::Failed);
    assert_eq!(fixture.logged("kernel.plugin_killed", "cause"), ["trap"]);
}

#[tokio::test]
async fn a_call_tree_nests_no_deeper_than_its_depth() {
    let depth = usize::try_from(KERNEL.dispatch_depth).expect("fits");
    let ids: Vec<(String, String)> = (0..=depth)
        .map(|at| (format!("noctua:depth-{at}"), format!("depth{at}")))
        .collect();
    let plugins: Vec<(&str, &str)> = ids
        .iter()
        .map(|(id, namespace)| (id.as_str(), namespace.as_str()))
        .collect();
    let (fixture, _) = with(&plugins, "").await;
    // From the inside out: the deepest relay dispatches the kernel's own read
    // of the realm, one level past the bound.
    let (mut name, mut args) = ("journal.verify".to_owned(), "{}".to_owned());
    for (_, namespace) in ids.iter().rev() {
        args = relay(&name, &args, None);
        name = format!("{namespace}.relay.read");
    }
    let answer = fixture
        .as_("ana", &name, &args)
        .await
        .expect("the top relay answers");
    assert!(
        answer.contains(&ErrorCode::KernelTooDeep.to_string()),
        "the dispatch past the bound was refused for it: {answer}"
    );
    let entries = fixture.entries().await;
    let relays = dispatched(&entries)
        .into_iter()
        .filter(|entry| entry.record.result == Outcome::Allowed)
        .count();
    assert_eq!(relays, depth + 1, "every relay down to the bound ran");
    assert_eq!(
        deciding(only(&entries, "journal.verify")),
        Some("kernel.too_deep")
    );
}

#[tokio::test]
async fn an_answer_that_is_not_a_payload_is_refused() {
    let (fixture, _) = reference().await;
    for text in ["not json", r#"{"b":1,"a":2}"#] {
        let echoed = fixture
            .as_("ana", "reference.echo", &note("photo", Some(text)))
            .await;
        assert!(
            is(
                &echoed,
                RefusalKind::Internal,
                ErrorCode::PluginMalformedResult
            ),
            "{text}: {echoed:?}"
        );
    }
    assert_eq!(
        fixture
            .as_(
                "ana",
                "reference.echo",
                &note("photo", Some(r#"{"a":2,"b":1}"#))
            )
            .await
            .as_deref(),
        Ok(r#"{"a":2,"b":1}"#),
        "a payload is answered as it was"
    );
}

#[tokio::test]
async fn a_write_from_a_command_that_only_reads_is_refused() {
    let (fixture, _) = reference().await;
    let answer = fixture
        .as_("ana", "reference.note.scribble", &note("photo", None))
        .await
        .expect("the read answers");
    assert!(
        text(&answer, "refused")
            .starts_with(&format!("denied: [{}] ", ErrorCode::PluginReadOnlyFrame)),
        "{answer}"
    );
    assert_eq!(
        fixture.held("noctua:reference", "scribble/photo").await,
        None
    );
}

/// A command that only reads, dispatched from inside one that changes state,
/// writes nothing either.
///
/// **Its tree holds a unit**, the outer command's, so a unit to stage in is
/// not what refuses it: its frame's own declaration is. With the frame's rule
/// weakened to let every command write, a write from a read at the top of a
/// tree is still refused, for want of a unit, and the one here is not.
#[tokio::test]
async fn a_write_from_a_read_nested_inside_a_change_is_refused_too() {
    let fixture = reference_and_peer().await;
    let answer = fixture
        .as_(
            "ana",
            "reference.relay.write",
            &relay("peer.note.scribble", &note("photo", None), None),
        )
        .await
        .expect("the relay answers");
    let last = text(&answer, "last");
    assert!(
        text(&last, "refused")
            .starts_with(&format!("denied: [{}] ", ErrorCode::PluginReadOnlyFrame)),
        "{answer}"
    );
    assert_eq!(
        fixture
            .held("noctua:reference-peer", "scribble/photo")
            .await,
        None
    );
}

#[tokio::test]
async fn a_write_past_the_plugins_quota_keeps_nothing_of_its_unit() {
    let (fixture, _) = with(
        &[("noctua:reference", "reference")],
        "[limits]\nstore_entries = 1\n",
    )
    .await;
    fixture
        .as_("ana", "reference.note.put", &note("photo", Some("one")))
        .await
        .expect("within the quota");
    let over = fixture
        .as_("ana", "reference.note.put", &note("diary", Some("two")))
        .await;
    assert!(
        is(
            &over,
            RefusalKind::Unavailable,
            ErrorCode::PluginBoundReached
        ),
        "{over:?}"
    );
    assert_eq!(fixture.held("noctua:reference", "note/diary").await, None);
    assert_eq!(fixture.logged("kernel.unit_aborted", "cause"), ["conflict"]);
}

/// The kernel's own commands are decided as any other: `journal.verify` is a
/// guardian's, and `limits.prune` a guardian's holding a quorum's approval.
///
/// A member who is no guardian is denied both, no permit matching, and a
/// guardian pruning without an approval is denied by the core's quorum rule —
/// each journaled with what decided it — while the prune she is approved for
/// runs in a unit of its own, kept, which notes it as having left the machine:
/// so it is a step her undo stops at.
#[tokio::test]
async fn the_kernels_own_commands_are_decided_like_any_other() {
    let (fixture, _) = reference().await;
    let verified = fixture
        .as_("ana", "journal.verify", "{}")
        .await
        .expect("a guardian reads the realm");
    assert_eq!(field(&verified, "verified"), Value::Integer(0));
    assert_eq!(fixture.logged("journal.verified", "from"), ["0"]);
    for member in ["bruno", "carla"] {
        let refused = fixture.as_(member, "journal.verify", "{}").await;
        assert!(
            is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
            "{member}: {refused:?}"
        );
    }
    let unapproved = fixture.as_("ana", PRUNE, "{}").await;
    assert!(
        is(&unapproved, RefusalKind::Denied, ErrorCode::PolicyDenied),
        "{unapproved:?}"
    );
    let not_a_guardian = fixture.prune_as("carla", "{}").await;
    assert!(
        is(
            &not_a_guardian,
            RefusalKind::Denied,
            ErrorCode::PolicyDenied
        ),
        "{not_a_guardian:?}"
    );
    let pruned = fixture
        .prune_as("ana", "{}")
        .await
        .expect("a guardian holding a quorum's approval prunes the realm");
    let local: Vec<&str> = nc_catalog::TABLES
        .iter()
        .filter(|table| table.database == nc_catalog::Database::LocalFile)
        .map(|table| table.name)
        .collect();
    assert_eq!(
        tables_of(&pruned),
        local,
        "every table the local database bounds, in the catalog's order: {pruned}"
    );
    let entries = fixture.entries().await;
    let decided = |command: &str| -> Vec<(Outcome, Option<String>)> {
        dispatched(&entries)
            .into_iter()
            .filter(|entry| command_of(entry).as_deref() == Some(command))
            .map(|entry| (entry.record.result, deciding(entry).map(str::to_owned)))
            .collect()
    };
    let by = |result, policy: &str| (result, Some(policy.to_owned()));
    assert_eq!(
        decided("journal.verify"),
        [
            by(Outcome::Allowed, "family.a_guardian_reads_the_realm"),
            by(Outcome::Denied, "kernel.no_permit_matched"),
            by(Outcome::Denied, "kernel.no_permit_matched"),
        ]
    );
    assert_eq!(
        decided(PRUNE),
        [
            by(Outcome::Denied, "core.destructive_needs_quorum"),
            by(Outcome::Denied, "kernel.no_permit_matched"),
            by(Outcome::Allowed, "family.a_guardian_prunes_the_realm"),
        ]
    );
    let prune = dispatched(&entries)
        .into_iter()
        .rfind(|entry| command_of(entry).as_deref() == Some(PRUNE))
        .expect("the prune is journaled");
    assert_eq!(prune.record.action, "Delete");
    let unit = prune.record.work_unit.expect("a prune runs in a unit");
    let notes = fixture
        .store
        .unit(unit)
        .await
        .expect("the store answers")
        .expect("the prune's unit is kept")
        .notes;
    assert!(
        notes.contains(&r#"{"command":"limits.prune","note":"left_the_machine"}"#.to_owned()),
        "{notes:?}"
    );
    let undone = fixture.as_("ana", "kernel.undo", "{}").await;
    assert!(
        is(&undone, RefusalKind::Denied, ErrorCode::KernelUndoBarrier),
        "what a prune deleted is outside every unit, so undo stops at it: {undone:?}"
    );
    let written = fixture.entries().await.len();
    let verified = fixture
        .as_("ana", "journal.verify", r#"{"from":0}"#)
        .await
        .expect("verified again");
    assert_eq!(
        field(&verified, "verified"),
        Value::Integer(i64::try_from(written).expect("fits")),
        "every entry written before it: {verified}"
    );
}

/// `limits.prune` takes a batch of the oldest units once they are past their
/// bound, and a dry run says what it would take and takes none.
///
/// One table or every one, as asked. A table the local database does not
/// bound — no table at all, or one of the server's — is refused in the
/// pruner's own code as no table this kernel prunes, and nothing is taken.
#[tokio::test]
async fn limits_prune_takes_a_batch_of_the_oldest_units_past_their_bound_and_a_dry_run_takes_none()
{
    let (fixture, _) = reference().await;
    let store = &fixture.store.inner;
    let (most, batch) = units_past_their_bound(store).await;
    let rehearsed = fixture
        .prune_as("ana", r#"{"dry_run":true,"table":"store_units"}"#)
        .await
        .expect("a rehearsal");
    assert_eq!(
        rehearsed,
        format!(r#"{{"pruned":[{{"removed":0,"table":"store_units","would_remove":{batch}}}]}}"#)
    );
    // Nothing went, and the rehearsal's own unit is one more.
    assert_eq!(count_units(store).await, most + 2);
    let pruned = fixture
        .prune_as("ana", r#"{"table":"store_units"}"#)
        .await
        .expect("a prune");
    assert_eq!(
        pruned,
        format!(
            r#"{{"pruned":[{{"removed":{batch},"table":"store_units","would_remove":{batch}}}]}}"#
        )
    );
    assert_eq!(
        count_units(store).await,
        most + 2 - batch + 1,
        "a batch went, and the prune's own unit is one more"
    );
    let oldest: i64 = sqlx::query_scalar("SELECT min(unit) FROM main.store_units")
        .fetch_one(store.pool())
        .await
        .expect("the store answers");
    assert_eq!(
        oldest,
        i64::try_from(batch + 1).expect("fits"),
        "the oldest went first"
    );
    let every = fixture.prune_as("ana", "{}").await.expect("every table");
    every_table_is_answered_as_it_is_bounded(&every);
    a_table_the_local_database_does_not_bound_is_refused(&fixture).await;
}

/// Holds `answer`, a prune of every table that removed nothing, to naming
/// each as the catalog bounds it: one its writer bounds is said to be left to
/// its writer, one a pack bounds — the local journal — to its pack, and every
/// other is the pruner's.
fn every_table_is_answered_as_it_is_bounded(answer: &str) {
    let rows = field(answer, "pruned")
        .into_array()
        .expect("a list of tables");
    for row in rows {
        let row = row.into_object().expect("a table's row");
        let named = row.get("table").cloned().and_then(Value::into_text);
        let left_alone = row.get("left_alone").cloned().and_then(Value::into_text);
        assert_eq!(row.get("removed"), Some(&Value::Integer(0)), "{named:?}");
        let strategy = nc_catalog::TABLES
            .iter()
            .find(|table| Some(table.name) == named.as_deref())
            .map_or_else(
                || panic!("{named:?} is a table of the catalog"),
                |table| table.strategy,
            );
        assert_eq!(
            left_alone.as_deref(),
            match strategy {
                nc_catalog::Strategy::RefuseAtLimit => Some("bounded_by_its_writer"),
                nc_catalog::Strategy::PolicyPack => Some("bounded_by_its_pack"),
                nc_catalog::Strategy::OldestFirst => None,
            },
            "{named:?}"
        );
    }
}

/// A prune asked for a table the local database does not bound — no table at
/// all, or one of the server's — is refused in the pruner's own code, its
/// entry journaled as failed, and keeps no unit.
async fn a_table_the_local_database_does_not_bound_is_refused<
    J: nc_journal::Store + Debug + Send + Sync + 'static,
>(
    fixture: &Fixture<J>,
) {
    let store = &fixture.store.inner;
    let before = count_units(store).await;
    for (table, says) in [
        (
            "nothing_here",
            "`limits.prune` was asked for `nothing_here`, which is not a table \
             catalog/limits.toml bounds"
                .to_owned(),
        ),
        (
            "journal_entries",
            format!(
                "`limits.prune` pruned nothing: [{}] `journal_entries` is a table of the \
                 server's PostgreSQL",
                ErrorCode::DbTableNotBounded
            ),
        ),
    ] {
        let args = Value::object([("table", Value::text(table))]).to_canonical();
        let refused = fixture.prune_as("ana", &args).await;
        assert!(
            is(&refused, RefusalKind::Invalid, ErrorCode::DbTableNotBounded),
            "{table}: {refused:?}"
        );
        let sentence = refused.expect_err("refused").sentence().to_owned();
        assert!(sentence.starts_with(&says), "{table}: {sentence}");
    }
    assert_eq!(
        count_units(store).await,
        before,
        "a refused prune keeps no unit"
    );
    let entries = fixture.entries().await;
    let failed: Vec<Option<String>> = dispatched(&entries)
        .into_iter()
        .filter(|entry| {
            command_of(entry).as_deref() == Some(PRUNE) && entry.record.result == Outcome::Failed
        })
        .map(|entry| {
            entry
                .record
                .resource
                .clone()
                .into_object()
                .and_then(|mut fields| fields.remove("failure"))
                .and_then(Value::into_text)
        })
        .collect();
    assert_eq!(
        failed,
        [
            Some("prune-refused".to_owned()),
            Some("prune-refused".to_owned())
        ]
    );
}

/// A prune whose entry the journal does not take deletes nothing: its answer
/// is withheld, the units past the table's bound all stay, and the same prune
/// the journal takes deletes a batch of them.
///
/// **What it holds against**: a pruner that deletes before the entry is
/// written, outside every unit, so that a prune the journal holds no entry of —
/// its unit closed as aborted — would have taken ten thousand of a person's
/// units.
#[tokio::test]
async fn a_prune_whose_entry_the_journal_did_not_take_deletes_nothing() {
    let (fixture, told) = reference().await;
    let store = &fixture.store.inner;
    let (most, batch) = units_past_their_bound(store).await;
    told.lock().expect("nothing panics holding it").refuses =
        Some(|entry: &Entry| entry.record.action == "Delete");
    let withheld = fixture.prune_as("ana", r#"{"table":"store_units"}"#).await;
    told.lock().expect("nothing panics holding it").refuses = None;
    assert!(
        is(
            &withheld,
            RefusalKind::Unavailable,
            ErrorCode::JournalUnavailable
        ),
        "{withheld:?}"
    );
    let entries = fixture.entries().await;
    assert!(
        entries.iter().all(|entry| entry.record.action != "Delete"),
        "{entries:#?}"
    );
    assert_eq!(
        count_units(store).await,
        most + 1,
        "a prune the journal holds no entry of took units"
    );
    let pruned = fixture
        .prune_as("ana", r#"{"table":"store_units"}"#)
        .await
        .expect("a prune the journal takes");
    assert_eq!(
        field(&pruned, "pruned").into_array().map(|rows| rows.len()),
        Some(1)
    );
    assert_eq!(
        count_units(store).await,
        most + 1 - batch + 1,
        "a batch taken, and the prune's own unit kept"
    );
    assert_eq!(
        dispatched(&fixture.entries().await)
            .iter()
            .filter(|entry| command_of(entry).as_deref() == Some(PRUNE))
            .map(|entry| entry.record.result)
            .collect::<Vec<_>>(),
        [Outcome::Allowed],
        "one entry for the prune that ran, journaled before it deleted"
    );
}

/// A prune takes no unit the journal holds open: it reverts each first, and
/// while the store refuses one's reversal it is refused, as the pump's is.
///
/// A unit whose close the journal did not take and whose reversal the store
/// refused is committed in the store and open in the journal — here the
/// oldest of the units past the table's bound. With its record taken by the
/// pruner, the kernel opened next would close the unit as aborted and its write
/// stay in the store. A dry run is refused as well, and a prune of another
/// table is not held.
#[tokio::test]
async fn a_prune_takes_no_unit_the_journal_holds_open_and_reverts_it_first() {
    let told = Arc::new(Mutex::new(Told::default()));
    let journal = Failing {
        inner: nc_journal::InMemory::default(),
        told: Arc::clone(&told),
    };
    let fixture = opened(
        journal,
        Ok(family()),
        &[("noctua:reference", "reference")],
        "",
    )
    .await;
    let left = left_committed(&fixture, &told).await;
    // **Every seed newer than anything the kernel mints**, so the unit left
    // is the oldest the pruner would take.
    let (_, batch) = units_past_their_bound_from(&fixture.store.inner, 1 << 62).await;
    // **A dry run is held as the run it rehearses**: one that went around the
    // unit would count its record among what the prune removes.
    for args in [
        r#"{"table":"store_units"}"#,
        "{}",
        r#"{"dry_run":true,"table":"store_units"}"#,
        r#"{"dry_run":true}"#,
    ] {
        let held = fixture.prune_as("ana", args).await;
        assert!(
            is(&held, RefusalKind::Unavailable, ErrorCode::DbUnreachable),
            "{args}: {held:?}"
        );
    }
    let blobs = fixture
        .prune_as("ana", r#"{"table":"blobs"}"#)
        .await
        .expect("a table that is not the units' is pruned as ever");
    assert_eq!(tables_of(&blobs), ["blobs"]);
    let pumped = fixture.kernel.pump().await;
    assert_eq!(
        (pumped.pruned, pumped.prune_refused),
        (0, Some(ErrorCode::DbUnreachable))
    );
    assert!(
        fixture
            .store
            .unit(left)
            .await
            .expect("the store answers")
            .is_some(),
        "the record the unit is reverted from is kept"
    );
    assert_eq!(
        photo_in(&fixture.store).await.as_deref(),
        Some(r#"{"text":"kept after all"}"#)
    );
    fixture
        .store
        .refuses_reversals
        .store(false, Ordering::SeqCst);
    let pruned = fixture
        .prune_as("ana", r#"{"table":"store_units"}"#)
        .await
        .expect("reverted, then pruned");
    assert_eq!(
        pruned,
        format!(
            r#"{{"pruned":[{{"removed":{batch},"table":"store_units","would_remove":{batch}}}]}}"#
        )
    );
    reverted_before_it_was_pruned(fixture, left).await;
}

/// Holds the kernel of `fixture`, whose prune took the record of `left` once
/// it was reverted, to having closed `left` as aborted with nothing of it in
/// the store.
///
/// A kernel opened again over the journal the first one wrote and the same
/// store is held to finding nothing left open.
///
/// **Over what the first kernel wrote, read back**: `nc_journal::InMemory` is a
/// value, and opened over the empty one the first kernel's journal was cloned
/// from, this would find nothing left open whatever the first kernel left.
async fn reverted_before_it_was_pruned(
    fixture: Fixture<Failing<nc_journal::InMemory>>,
    left: nc_ids::Snowflake,
) {
    assert_eq!(photo_in(&fixture.store).await, None, "reverted first");
    assert!(
        fixture
            .store
            .unit(left)
            .await
            .expect("the store answers")
            .is_none(),
        "and then pruned, the oldest"
    );
    let entries = fixture.entries().await;
    let closed: Vec<Outcome> = entries
        .iter()
        .filter(|entry| entry.record.work_unit == Some(left) && is_a_bracket(entry))
        .map(|entry| entry.record.result)
        .collect();
    assert_eq!(closed, [Outcome::UnitBegun, Outcome::UnitAborted]);
    assert!(
        fixture
            .logged("kernel.unit_aborted", "unit")
            .contains(&left.to_crockford()),
        "{:?}",
        fixture.log.lines()
    );
    assert_eq!(fixture.kernel.pump().await.prune_refused, None);
    let store = fixture.store.clone();
    let written = written_by(&fixture).await;
    drop(fixture);
    let log = Arc::new(Captured::default());
    let journal = Failing {
        inner: written,
        told: Arc::new(Mutex::new(Told::default())),
    };
    let _again = opening(4, journal, Ok(family()), store.clone(), Arc::clone(&log))
        .await
        .expect("the kernel opens again");
    assert!(
        log.named("kernel.recovered").is_empty(),
        "{:?}",
        log.lines()
    );
    assert_eq!(photo_in(&store).await, None, "nothing of the unit is left");
}

/// A journal holding every entry the kernel of `fixture` wrote, in the order
/// written, which a kernel opened again reads as the first one left it.
async fn written_by(fixture: &Fixture<Failing<nc_journal::InMemory>>) -> nc_journal::InMemory {
    let mut written = nc_journal::InMemory::default();
    let entries = fixture.entries().await;
    assert!(!entries.is_empty(), "the first kernel wrote its journal");
    for entry in entries {
        nc_journal::Store::append(&mut written, entry)
            .await
            .expect("the entries append in the order written");
    }
    written
}

/// Has `ana` write the note `photo` as `kept after all` through `fixture`, and
/// answers the unit: committed in the store and open in the journal.
///
/// The journal `told` makes refuse the unit's close, and the store refuses its
/// reversal.
async fn left_committed(
    fixture: &Fixture<Failing<nc_journal::InMemory>>,
    told: &Arc<Mutex<Told>>,
) -> nc_ids::Snowflake {
    fixture
        .store
        .refuses_reversals
        .store(true, Ordering::SeqCst);
    told.lock().expect("nothing panics holding it").refuses =
        Some(|entry: &Entry| entry.record.result == Outcome::UnitCommitted);
    let refused = fixture
        .as_(
            "ana",
            "reference.note.put",
            &note("photo", Some("kept after all")),
        )
        .await;
    told.lock().expect("nothing panics holding it").refuses = None;
    assert!(
        is(
            &refused,
            RefusalKind::Unavailable,
            ErrorCode::JournalUnavailable
        ),
        "{refused:?}"
    );
    only(&fixture.entries().await, "reference.note.put")
        .record
        .work_unit
        .expect("a unit")
}

/// A unit the journal took the close of after all — its answer lost and the
/// journal then unable to say, so the kernel reverted it and the store
/// refused — is not reverted before a prune once the journal says it kept it,
/// and is not while the journal still cannot say.
///
/// Neither silence is read as a no: one the journal cannot say of stays held,
/// the prune refused with the journal's code; one it says it kept is no longer
/// held, kept, and the prune runs. **The journal is silent of that one unit
/// alone**: one silent of every unit would refuse every entry a unit holds, the
/// prune's own among them, refusing the prune before it asked anything — and a
/// plant reading the silence as "still open" would pass.
#[tokio::test]
async fn a_unit_the_journal_kept_after_all_is_not_reverted_before_a_prune() {
    let (fixture, told) = reference().await;
    {
        let mut told = told.lock().expect("nothing panics holding it");
        told.lands_then_refuses =
            Some(|entry: &Entry| entry.record.result == Outcome::UnitCommitted);
        told.then_says_nothing = true;
    }
    fixture
        .store
        .refuses_reversals
        .store(true, Ordering::SeqCst);
    let refused = fixture
        .as_(
            "ana",
            "reference.note.put",
            &note("photo", Some("kept after all")),
        )
        .await;
    assert!(
        is(
            &refused,
            RefusalKind::Unavailable,
            ErrorCode::JournalUnavailable
        ),
        "{refused:?}"
    );
    let kept = only(&fixture.entries().await, "reference.note.put")
        .record
        .work_unit
        .expect("a unit");
    {
        let mut told = told.lock().expect("nothing panics holding it");
        told.lands_then_refuses = None;
        told.says_nothing = false;
        told.says_nothing_of = Some(kept);
    }
    fixture
        .store
        .refuses_reversals
        .store(false, Ordering::SeqCst);
    let unsure = fixture.prune_as("ana", r#"{"table":"store_units"}"#).await;
    assert!(
        is(
            &unsure,
            RefusalKind::Unavailable,
            ErrorCode::JournalUnavailable
        ),
        "{unsure:?}"
    );
    assert!(
        unsure
            .expect_err("refused")
            .sentence()
            .contains(&format!("could not say how the unit {kept} closed")),
        "the refusal is the held unit's, not the prune's own"
    );
    assert_eq!(
        photo_in(&fixture.store).await.as_deref(),
        Some(r#"{"text":"kept after all"}"#),
        "a unit the journal cannot say of is not reverted"
    );
    told.lock()
        .expect("nothing panics holding it")
        .says_nothing_of = None;
    fixture
        .store
        .refuses_reversals
        .store(true, Ordering::SeqCst);
    let pruned = fixture
        .prune_as("ana", r#"{"table":"store_units"}"#)
        .await
        .expect("the unit the journal kept holds the pruner back no longer");
    assert_eq!(tables_of(&pruned), ["store_units"]);
    assert_eq!(
        photo_in(&fixture.store).await.as_deref(),
        Some(r#"{"text":"kept after all"}"#),
        "what the journal kept is kept"
    );
    assert!(
        !fixture
            .logged("kernel.unit_aborted", "unit")
            .contains(&kept.to_crockford()),
        "{:?}",
        fixture.log.lines()
    );
}

/// A unit held from the pruner that the store cannot read stays held and is
/// not closed: the prune is refused with the store's code, and the next prune
/// the store answers reverts the unit before it runs.
///
/// **A store that cannot read the unit is not one that holds nothing of it.**
/// With the read's error taken for "nothing", the prune would run, and the unit
/// be closed as aborted in the journal with its write left in the store, where
/// nothing reverts it — which this test refuses.
#[tokio::test]
async fn a_unit_held_from_the_pruner_that_the_store_cannot_read_stays_held_and_is_not_closed() {
    let (fixture, told) = reference().await;
    let left = left_committed(&fixture, &told).await;
    let brackets_of_left = |entries: &[Entry]| -> Vec<Outcome> {
        entries
            .iter()
            .filter(|entry| entry.record.work_unit == Some(left) && is_a_bracket(entry))
            .map(|entry| entry.record.result)
            .collect()
    };
    fixture
        .store
        .refuses_reversals
        .store(false, Ordering::SeqCst);
    fixture.store.refuses_units.store(true, Ordering::SeqCst);
    let unread = fixture.prune_as("ana", r#"{"table":"store_units"}"#).await;
    fixture.store.refuses_units.store(false, Ordering::SeqCst);
    assert!(
        is(&unread, RefusalKind::Unavailable, ErrorCode::DbUnreachable),
        "{unread:?}"
    );
    assert!(
        unread
            .expect_err("refused")
            .sentence()
            .contains("is still to be reverted"),
        "the refusal is the held unit's, not the prune's own"
    );
    assert_eq!(
        brackets_of_left(&fixture.entries().await),
        [Outcome::UnitBegun],
        "a unit the store cannot read is not closed"
    );
    assert!(
        !fixture
            .logged("kernel.unit_aborted", "unit")
            .contains(&left.to_crockford()),
        "{:?}",
        fixture.log.lines()
    );
    assert_eq!(
        photo_in(&fixture.store).await.as_deref(),
        Some(r#"{"text":"kept after all"}"#),
        "nor reverted"
    );
    let pruned = fixture
        .prune_as("ana", r#"{"table":"store_units"}"#)
        .await
        .expect("the store reads the unit back, so it is reverted and the prune runs");
    assert_eq!(tables_of(&pruned), ["store_units"]);
    assert_eq!(photo_in(&fixture.store).await, None, "reverted first");
    assert_eq!(
        brackets_of_left(&fixture.entries().await),
        [Outcome::UnitBegun, Outcome::UnitAborted]
    );
}

/// Two units held from the pruner over one key are both reverted before a
/// prune, the newer first, and the prune runs.
///
/// The newer unit's reversal puts back the value the older one left, which is
/// what the older one's reversal is held to — the order the recovery on open
/// keeps too. Pruned oldest first, the older reversal would be refused as a
/// conflict, and with it every prune of the units for as long as the kernel
/// runs.
#[tokio::test]
async fn two_units_held_from_the_pruner_over_one_key_are_reverted_the_newer_first() {
    let (fixture, told) = reference().await;
    fixture
        .store
        .refuses_reversals
        .store(true, Ordering::SeqCst);
    told.lock().expect("nothing panics holding it").refuses =
        Some(|entry: &Entry| entry.record.result == Outcome::UnitCommitted);
    for text in ["first", "second"] {
        let refused = fixture
            .as_("ana", "reference.note.put", &note("photo", Some(text)))
            .await;
        assert!(
            is(
                &refused,
                RefusalKind::Unavailable,
                ErrorCode::JournalUnavailable
            ),
            "{text}: {refused:?}"
        );
    }
    told.lock().expect("nothing panics holding it").refuses = None;
    fixture
        .store
        .refuses_reversals
        .store(false, Ordering::SeqCst);
    assert_eq!(
        photo_in(&fixture.store).await.as_deref(),
        Some(r#"{"text":"second"}"#),
        "both are committed in the store"
    );
    let entries = fixture.entries().await;
    let held: Vec<String> = dispatched(&entries)
        .into_iter()
        .filter(|entry| command_of(entry).as_deref() == Some("reference.note.put"))
        .filter_map(|entry| entry.record.work_unit)
        .map(nc_ids::Snowflake::to_crockford)
        .collect();
    assert_eq!(held.len(), 2, "{held:?}");
    let pruned = fixture
        .prune_as("ana", r#"{"table":"store_units"}"#)
        .await
        .expect("both are reverted, so the prune runs");
    assert_eq!(tables_of(&pruned), ["store_units"]);
    assert_eq!(
        photo_in(&fixture.store).await,
        None,
        "nothing of either is left"
    );
    let aborted: Vec<String> = fixture
        .logged("kernel.unit_aborted", "unit")
        .into_iter()
        .filter(|unit| held.contains(unit))
        .collect();
    assert_eq!(
        aborted,
        [held[1].as_str(), held[0].as_str()],
        "the newer first"
    );
}

/// A kernel does not open over a unit it could not recover, and opens once
/// it can.
///
/// A journal it cannot read or that cannot say how the unit closed, a store
/// that cannot read the unit back, a reversal the store refuses — logged as
/// `kernel.revert_failed`, the unit left committed — and a close the journal
/// does not take each keep it from opening, with the code of what refused.
/// Opened again over the same history, it recovers what is left: a unit whose
/// reversal landed before its close was refused is only closed then.
#[tokio::test]
async fn a_kernel_does_not_open_over_a_unit_it_could_not_recover_and_opens_once_it_can() {
    let (history, store, left) = a_step_left_committed().await;
    let unreadable = unrecovered(
        &history,
        Told {
            reads: Reads::Refused,
            ..Told::default()
        },
        &store,
        Arc::new(Captured::default()),
    )
    .await;
    assert_eq!(
        (unreadable.kind(), unreadable.code()),
        (RefusalKind::Unavailable, ErrorCode::JournalUnavailable),
        "{unreadable}"
    );
    what_cannot_answer_recovers_nothing(&history, &store).await;
    store.refuses_reversals.store(true, Ordering::SeqCst);
    let log = Arc::new(Captured::default());
    let unreverted = unrecovered(&history, Told::default(), &store, Arc::clone(&log)).await;
    store.refuses_reversals.store(false, Ordering::SeqCst);
    assert_eq!(
        (unreverted.kind(), unreverted.code()),
        (RefusalKind::Unavailable, ErrorCode::DbUnreachable),
        "{unreverted}"
    );
    let failed: Vec<(Option<String>, Option<String>)> = log
        .named("kernel.revert_failed")
        .iter()
        .map(|line| (line.text("unit"), line.text("code")))
        .collect();
    assert_eq!(
        failed,
        [(
            Some(left.to_crockford()),
            Some(ErrorCode::DbUnreachable.code().to_owned())
        )]
    );
    assert_eq!(
        log.named("kernel.recovered"),
        [] as [nc_kernel::logging::CapturedLine; 0]
    );
    assert_eq!(
        photo_in(&store).await.as_deref(),
        Some(r#"{"text":"left behind"}"#),
        "a reversal the store refused leaves the unit as it was"
    );
    let log = Arc::new(Captured::default());
    let unclosed = unrecovered(
        &history,
        Told {
            refuses: Some(|entry| entry.record.action == nc_journal::unit::CLOSES),
            ..Told::default()
        },
        &store,
        Arc::clone(&log),
    )
    .await;
    assert_eq!(
        (unclosed.kind(), unclosed.code()),
        (RefusalKind::Unavailable, ErrorCode::JournalUnavailable),
        "{unclosed}"
    );
    assert_eq!(
        photo_in(&store).await,
        None,
        "its reversal landed before the close"
    );
    assert_eq!(
        log.named("kernel.recovered"),
        [] as [nc_kernel::logging::CapturedLine; 0]
    );
    let reversal = store
        .unit(left)
        .await
        .expect("the store answers")
        .map(|detail| detail.row.state);
    assert!(matches!(reversal, Some(nc_local::UnitState::Undone { .. })));
    opens_closing_what_is_reversed_already(history, &store, left, reversal).await;
}

/// Holds a kernel over `history` and `store` — which holds the step a stopped
/// process left committed — to not opening while the store cannot read the
/// unit back or the journal cannot say how it closed, each with the code of
/// what did not answer, and nothing reverted.
///
/// **Neither is read as a no.** A store that cannot read the unit is not one
/// that holds nothing of it, which would close it with its writes kept; and a
/// journal that cannot say is not one that says it is open, which would revert
/// what it may have kept. Either, planted to read so, fails here.
async fn what_cannot_answer_recovers_nothing(history: &nc_journal::InMemory, store: &Stalling) {
    let log = Arc::new(Captured::default());
    store.refuses_units.store(true, Ordering::SeqCst);
    let unread = unrecovered(history, Told::default(), store, Arc::clone(&log)).await;
    store.refuses_units.store(false, Ordering::SeqCst);
    assert_eq!(
        (unread.kind(), unread.code()),
        (RefusalKind::Unavailable, ErrorCode::DbUnreachable),
        "{unread}"
    );
    let unsaid = unrecovered(
        history,
        Told {
            says_nothing: true,
            ..Told::default()
        },
        store,
        Arc::clone(&log),
    )
    .await;
    assert_eq!(
        (unsaid.kind(), unsaid.code()),
        (RefusalKind::Unavailable, ErrorCode::JournalUnavailable),
        "{unsaid}"
    );
    assert!(
        log.named("kernel.recovered").is_empty(),
        "{:?}",
        log.lines()
    );
    assert_eq!(
        photo_in(store).await.as_deref(),
        Some(r#"{"text":"left behind"}"#),
        "nothing was reverted"
    );
}

/// Opens a kernel over `history` and `store` with nothing refusing.
///
/// It holds the kernel to closing `left`, which the store holds reversed
/// already as `reversal`, and to reverting nothing again.
async fn opens_closing_what_is_reversed_already(
    history: nc_journal::InMemory,
    store: &Stalling,
    left: nc_ids::Snowflake,
    reversal: Option<nc_local::UnitState>,
) {
    let log = Arc::new(Captured::default());
    let journal = Failing {
        inner: history,
        told: Arc::new(Mutex::new(Told::default())),
    };
    let fixture = opening(3, journal, Ok(family()), store.clone(), Arc::clone(&log))
        .await
        .expect("the kernel opens once nothing refuses");
    assert_eq!(
        log.named("kernel.recovered")
            .iter()
            .map(|line| (line.text("unit"), line.fields.get("reverted").cloned()))
            .collect::<Vec<_>>(),
        [(Some(left.to_crockford()), Some(Value::Bool(false)))],
        "the unit the store holds reverted is closed, and not reverted again"
    );
    assert_eq!(
        fixture
            .store
            .unit(left)
            .await
            .expect("the store answers")
            .map(|detail| detail.row.state),
        reversal
    );
    assert_eq!(
        brackets(&fixture.entries().await),
        [
            (nc_journal::unit::OPENS, Outcome::UnitBegun),
            (nc_journal::unit::CLOSES, Outcome::UnitAborted),
        ]
    );
}

/// The journal of a process that is not this kernel, for the realm `home`,
/// minting on a flake of its own.
fn a_stopped_journal() -> nc_journal::Journal<nc_journal::InMemory, Manual> {
    let world = MemoryDirectory::from_world(WORLD, &family()).expect("the world is valid");
    // **The stopped process's own keeper**: whoever brackets a unit is the
    // journal's to name, and the world's `ana` is one no hatch is needed for.
    let keeper = world
        .principal(PrincipalKind::User, "ana")
        .expect("ana is a user")
        .id()
        .clone();
    nc_journal::Journal::new(
        nc_journal::InMemory::default(),
        world.realm_id("home").expect("home is a realm"),
        keeper,
        Minter::new(
            MachineFlake::from_value(9).expect("a flake"),
            Manual::at(NOW_MS),
        ),
    )
}

/// A history holding one step its process opened and stopped before
/// closing, and a store holding that step committed — the note `photo`
/// written `left behind` — and the step.
async fn a_step_left_committed() -> (nc_journal::InMemory, Stalling, nc_ids::Snowflake) {
    let mut stopped = a_stopped_journal();
    let left = stopped
        .open_unit("reference.note.put")
        .await
        .expect("opened");
    let store = stalling(
        LocalStore::in_memory_for_tests()
            .await
            .expect("the local database opens in memory"),
    );
    seeded_unit(&store.inner, "ana", |unit| {
        unit.unit = left;
        unit.writes = vec![StagedWrite {
            plugin: "noctua:reference".to_owned(),
            key: "note/photo".to_owned(),
            expect: Expect::Anything,
            after: Some(r#"{"text":"left behind"}"#.to_owned()),
        }];
    })
    .await;
    (stopped.store().clone(), store, left)
}

/// Why a kernel over `history` and `store` did not open.
///
/// With the history read and written as `told` says, it answers the refusal of
/// the unit left open that it did not recover, logged to `log`.
async fn unrecovered(
    history: &nc_journal::InMemory,
    told: Told,
    store: &Stalling,
    log: Arc<Captured>,
) -> Refusal {
    let journal = Failing {
        inner: history.clone(),
        told: Arc::new(Mutex::new(told)),
    };
    match opening(3, journal, Ok(family()), store.clone(), log).await {
        Err(nc_kernel::OpenError::Unrecovered(refusal)) => refusal,
        Err(other) => panic!("refused for another reason: {other}"),
        Ok(_) => panic!("opened over a unit it did not recover"),
    }
}

/// Two units a stopped process left committed over one key are both
/// reverted, the newer first, and the kernel opens.
///
/// The newer unit's reversal puts back the value the older one left, which is
/// what the older one's reversal is held to. Reverted oldest first, the second
/// reversal would be refused as a conflict, and every later open refused the
/// same way, for good.
#[tokio::test]
async fn two_units_left_committed_over_one_key_are_both_reverted_the_newer_first() {
    let mut stopped = a_stopped_journal();
    let store = stalling(
        LocalStore::in_memory_for_tests()
            .await
            .expect("the local database opens in memory"),
    );
    let mut left = Vec::new();
    for text in ["first", "second"] {
        let unit = stopped
            .open_unit("reference.note.put")
            .await
            .expect("opened");
        seeded_unit(&store.inner, "ana", |seed| {
            seed.unit = unit;
            seed.writes = vec![StagedWrite {
                plugin: "noctua:reference".to_owned(),
                key: "note/photo".to_owned(),
                expect: Expect::Anything,
                after: Some(Value::object([("text", Value::text(text))]).to_canonical()),
            }];
        })
        .await;
        left.push(unit);
    }
    let log = Arc::new(Captured::default());
    let journal = Failing {
        inner: stopped.store().clone(),
        told: Arc::new(Mutex::new(Told::default())),
    };
    let fixture = opening(3, journal, Ok(family()), store.clone(), Arc::clone(&log))
        .await
        .expect("both are reverted, so the kernel opens");
    assert_eq!(photo_in(&store).await, None, "nothing of either is left");
    assert_eq!(
        log.named("kernel.recovered")
            .iter()
            .map(|line| (line.text("unit"), line.fields.get("reverted").cloned()))
            .collect::<Vec<_>>(),
        [left[1], left[0]].map(|unit| (Some(unit.to_crockford()), Some(Value::Bool(true))))
    );
    assert_eq!(
        log.named("kernel.revert_failed"),
        [] as [nc_kernel::logging::CapturedLine; 0]
    );
    assert_eq!(
        brackets(&fixture.entries().await),
        [
            (nc_journal::unit::OPENS, Outcome::UnitBegun),
            (nc_journal::unit::OPENS, Outcome::UnitBegun),
            (nc_journal::unit::CLOSES, Outcome::UnitAborted),
            (nc_journal::unit::CLOSES, Outcome::UnitAborted),
        ]
    );
}

/// A chain that does not verify is answered `broken`, at the entry where it
/// breaks and with the code of what broke it, and logged as broken; and a
/// verification asked to start further on starts there.
///
/// A `journal.verify` answering `verified` over a rewritten entry, or one
/// reading from the start whatever it is asked, fails this test: it reads a
/// chain broken where it breaks, and a verification asked to start further on.
#[tokio::test]
async fn a_chain_that_does_not_verify_is_answered_broken_where_it_breaks() {
    let (fixture, told) = reference().await;
    for _ in 0..2 {
        fixture
            .as_("carla", "reference.note.get", &note("photo", None))
            .await
            .expect("a member reads content shared with the realm");
    }
    told.lock().expect("nothing panics holding it").reads = Reads::Rewritten;
    let whole = fixture.as_("ana", "journal.verify", "{}").await;
    let further_on = fixture.as_("ana", "journal.verify", r#"{"from":1}"#).await;
    told.lock().expect("nothing panics holding it").reads = Reads::AsWritten;
    let whole = whole.expect("a guardian verifies the realm's journal");
    let broken = field(&whole, "broken")
        .into_object()
        .unwrap_or_else(|| panic!("`broken` is an object: {whole}"));
    assert_eq!(
        broken.get("sequence"),
        Some(&Value::Integer(0)),
        "the first entry is the one rewritten: {whole}"
    );
    let rewritten = nc_journal::Broken {
        sequence: 0,
        reason: nc_journal::Reason::HashDoesNotMatchContents,
        detail: None,
    };
    assert_eq!(
        broken.get("reason").cloned().and_then(Value::into_text),
        Some(rewritten.code().code().to_owned()),
        "an entry whose contents no longer match its digest: {whole}"
    );
    assert_eq!(fixture.logged("journal.broken", "sequence"), ["0"]);
    assert_eq!(
        fixture.logged("journal.broken", "code"),
        [rewritten.code().code()]
    );
    // From the second entry on, the chain holds: that entry, and the one the
    // first verification wrote.
    let further_on = further_on.expect("verified from the second entry");
    assert_eq!(
        field(&further_on, "verified"),
        Value::Integer(2),
        "{further_on}"
    );
    // **And the log says where it began**: without it, the log would say *the
    // chain verified end to end* of this one, with nothing to tell it from a
    // whole chain read.
    assert_eq!(fixture.logged("journal.verified", "from"), ["1"]);
}

/// A journal that cannot be read is not verified: `journal.verify` is
/// answered `unavailable` with the journal's own code — a refusal its caller
/// may retry — and never with a verdict on the chain.
#[tokio::test]
async fn a_journal_that_cannot_be_read_is_answered_unavailable_and_not_verified() {
    let (fixture, told) = reference().await;
    told.lock().expect("nothing panics holding it").reads = Reads::Refused;
    let refused = fixture.as_("ana", "journal.verify", "{}").await;
    told.lock().expect("nothing panics holding it").reads = Reads::AsWritten;
    assert!(
        is(
            &refused,
            RefusalKind::Unavailable,
            ErrorCode::JournalUnavailable
        ),
        "{refused:?}"
    );
    let entries = fixture.entries().await;
    let entry = only(&entries, "journal.verify");
    assert_eq!(entry.record.result, Outcome::Failed);
    assert_eq!(
        entry.record.resource.to_canonical(),
        r#"{"command":"journal.verify","entity":{"id":"home","type":"Realm"},"failure":"journal-unreadable"}"#
    );
    assert_eq!(
        fixture.log.named("journal.verified"),
        [] as [nc_kernel::logging::CapturedLine; 0]
    );
    assert_eq!(
        fixture.log.named("journal.broken"),
        [] as [nc_kernel::logging::CapturedLine; 0]
    );
}

#[tokio::test]
async fn a_denial_is_logged_with_its_code_and_is_not_journaled() {
    let (fixture, _) = reference().await;
    let probed = fixture
        .as_("ana", "reference.probe", &note("photo", None))
        .await
        .expect("the probe answers, refused or not");
    let capability = format!("denied: [{}] ", ErrorCode::PluginCapabilityDenied);
    assert!(
        text(&probed, "timers#after").starts_with(&capability),
        "{probed}"
    );
    let logged = fixture.log.named("kernel.capability_denied");
    for line in &logged {
        assert_eq!(
            line.text("code").as_deref(),
            Some(ErrorCode::PluginCapabilityDenied.code())
        );
        assert_eq!(line.text("slot").as_deref(), Some("0"));
    }
    // **One line per function refused, however often it was**: the probe
    // reaches `blobs.open` four times, once for itself and once for each
    // method of the reader it would hand out.
    let refused = nc_canon::parse(&probed)
        .expect("canonical")
        .into_object()
        .expect("an object")
        .into_values()
        .filter_map(Value::into_text)
        .filter(|outcome| outcome.starts_with(&capability))
        .count();
    assert!(refused > 1, "{probed}");
    assert_eq!(logged.len(), refused, "{logged:#?}");
    let capabilities: std::collections::BTreeSet<String> = logged
        .iter()
        .filter_map(|line| line.text("capability"))
        .collect();
    assert!(capabilities.contains("timers") && !capabilities.contains("store"));
    let entries = fixture.entries().await;
    assert_eq!(
        dispatched(&entries).len(),
        2,
        "the probe and the one dispatch it made, and no entry for a refused capability: \
         {entries:#?}"
    );
    // **And once, however many commands meet the same refusal**: a second
    // probe in the same instance logs nothing it logged already.
    fixture
        .as_("ana", "reference.probe", &note("photo", None))
        .await
        .expect("the probe answers again");
    assert_eq!(
        fixture.log.named("kernel.capability_denied").len(),
        refused,
        "a refusal logged again for every command that met it"
    );
}

/// A write a command that only reads tries is refused, and logged with its
/// code and the plugin's slot — once for the instance, however often a
/// command of it tries again.
///
/// **What it holds against**: the plugin answered `denied` with
/// `[NC-PLUG-0024]` and nothing recording it, no journal entry and no log
/// line, where a capability denied and a fetch refused are each logged — and
/// every authorization denial is recorded (`NOCTUA.md §15.5`).
#[tokio::test]
async fn a_write_refused_where_nothing_may_change_is_logged_with_its_code_once() {
    let (fixture, _) = reference().await;
    for _ in 0..2 {
        let answer = fixture
            .as_("ana", "reference.note.scribble", &note("photo", None))
            .await
            .expect("the read answers");
        assert!(
            text(&answer, "refused")
                .starts_with(&format!("denied: [{}] ", ErrorCode::PluginReadOnlyFrame)),
            "{answer}"
        );
    }
    let logged = fixture.log.named("kernel.host_call_refused");
    assert_eq!(
        logged
            .iter()
            .map(|line| (line.text("slot"), line.text("code")))
            .collect::<Vec<_>>(),
        [(
            Some("0".to_owned()),
            Some(ErrorCode::PluginReadOnlyFrame.code().to_owned())
        )],
        "{logged:#?}"
    );
}

/// A capability a plugin is refused in `activate` is logged once it is
/// loaded, before anything is dispatched to it, and again by the instance a
/// restart brings back.
///
/// **What it holds against**: what `activate` was refused logged on the return
/// of a later, unrelated call — the probe's reaches no host function at all —
/// and never for a plugin nothing dispatched.
#[tokio::test]
async fn a_capability_refused_in_activate_is_logged_once_the_plugin_is_loaded() {
    let store = stalling(
        LocalStore::in_memory_for_tests()
            .await
            .expect("the local database opens in memory"),
    );
    let fixture = opening(
        3,
        nc_journal::InMemory::default(),
        Ok(family()),
        store,
        Arc::new(Captured::default()),
    )
    .await
    .expect("the kernel opens");
    // The reference plugin's `activate` reads its `activation` key, and this
    // manifest grants it no store.
    let slot = fixture
        .kernel
        .load(
            PluginManifest::parse(
                "id = \"noctua:reference\"\nnamespace = \"reference\"\ncapabilities = []\n",
            )
            .expect("a manifest the kernel reads"),
            &nc_kernel::component_for_tests("reference"),
        )
        .await
        .expect("the reference plugin loads granted nothing");
    let denied = |fixture: &Fixture<nc_journal::InMemory>| {
        fixture
            .log
            .named("kernel.capability_denied")
            .iter()
            .map(|line| (line.text("slot"), line.text("capability")))
            .collect::<Vec<_>>()
    };
    let once = (Some(slot.index().to_string()), Some("store".to_owned()));
    assert_eq!(
        denied(&fixture),
        std::slice::from_ref(&once),
        "the store `activate` was refused is not logged"
    );
    let trapped = fixture
        .as_("ana", "reference.abuse.trap", &note("photo", None))
        .await;
    assert!(
        is(&trapped, RefusalKind::Internal, ErrorCode::PluginTrapped),
        "{trapped:?}"
    );
    assert_eq!(
        denied(&fixture),
        [once.clone(), once],
        "the store the restarted instance's `activate` was refused is not logged"
    );
}

/// Whether `entry` closes the unit of the command `label`: the bracket a
/// journal writes with the label of the unit it opened.
fn closes(entry: &Entry, label: &str) -> bool {
    entry.record.action == nc_journal::unit::CLOSES && entry.record.resource == Value::text(label)
}

/// An undo whose close the journal does not take is reverted at once, as any
/// unit is, and its step is committed again with it: what the undo put back
/// is taken back, the step is one undo reaches once more, and the kernel's
/// own reversal is no undo a redo puts back.
///
/// A redo the journal does not close is reverted the same way: its step is
/// undone again by the same undo, which a redo then reaches as before.
#[tokio::test]
async fn an_undo_or_a_redo_the_journal_did_not_close_is_reverted_with_the_states_it_moved() {
    let (fixture, told) = reference().await;
    fixture
        .as_("ana", "reference.note.put", &note("photo", Some("kept")))
        .await
        .expect("written");
    let step = fixture.units_of("ana").await.remove(0);
    told.lock().expect("nothing panics holding it").refuses =
        Some(|entry: &Entry| closes(entry, "kernel.undo"));
    let refused = fixture
        .as_("ana", "kernel.undo", "{}")
        .await
        .expect_err("the journal did not take the close");
    told.lock().expect("nothing panics holding it").refuses = None;
    assert!(refused.sentence().contains("reverted at once"), "{refused}");
    assert_eq!(
        fixture.held("noctua:reference", "note/photo").await,
        Some(r#"{"text":"kept"}"#.to_owned()),
        "what the undo put back outlived a journal that did not keep it"
    );
    let history = fixture.units_of("ana").await;
    let (revert, undo) = (&history[0], &history[1]);
    assert_eq!(revert.meta.label, "kernel.revert");
    assert_eq!(revert.meta.kind, UnitKind::Undo { target: undo.unit });
    assert_eq!(undo.state, nc_local::UnitState::Undone { by: revert.unit });
    assert_eq!(history[2].state, nc_local::UnitState::Committed, "the step");
    let nothing = fixture.as_("ana", "kernel.redo", "{}").await;
    assert!(
        is(
            &nothing,
            RefusalKind::NotFound,
            ErrorCode::KernelNothingToRedo
        ),
        "the kernel's reversal was put back as an undo: {nothing:?}"
    );

    fixture
        .as_("ana", "kernel.undo", "{}")
        .await
        .expect("the step is undone once more");
    assert_eq!(fixture.held("noctua:reference", "note/photo").await, None);
    let undo = fixture.units_of("ana").await.remove(0);
    told.lock().expect("nothing panics holding it").refuses =
        Some(|entry: &Entry| closes(entry, "kernel.redo"));
    fixture
        .as_("ana", "kernel.redo", "{}")
        .await
        .expect_err("the journal did not take the close");
    told.lock().expect("nothing panics holding it").refuses = None;
    assert_eq!(
        fixture.held("noctua:reference", "note/photo").await,
        None,
        "what the redo put back outlived a journal that did not keep it"
    );
    assert_eq!(
        fixture
            .store
            .unit(step.unit)
            .await
            .expect("the store answers")
            .expect("held")
            .row
            .state,
        nc_local::UnitState::Undone { by: undo.unit },
        "the step is undone again by the same undo"
    );
    fixture
        .as_("ana", "kernel.redo", "{}")
        .await
        .expect("the redo reaches the undo as before");
    assert_eq!(
        fixture.held("noctua:reference", "note/photo").await,
        Some(r#"{"text":"kept"}"#.to_owned())
    );
}

/// An undo is refused when the journal cannot say how the step's unit closed,
/// and when the store cannot list the person's history: in either case with
/// the code of what could not answer, and nothing undone.
///
/// The journal says nothing of the step's unit alone, so that the undo's own
/// unit opens and what refuses it is the question about the step.
#[tokio::test]
async fn an_undo_neither_the_journal_nor_the_store_can_answer_for_is_refused() {
    let (fixture, told) = reference().await;
    fixture
        .as_("ana", "reference.note.put", &note("photo", Some("kept")))
        .await
        .expect("written");
    let step = fixture.units_of("ana").await.remove(0);
    told.lock()
        .expect("nothing panics holding it")
        .says_nothing_of = Some(step.unit);
    let refused = fixture.as_("ana", "kernel.undo", "{}").await;
    told.lock()
        .expect("nothing panics holding it")
        .says_nothing_of = None;
    assert!(
        is(
            &refused,
            RefusalKind::Unavailable,
            ErrorCode::JournalUnavailable
        ),
        "{refused:?}"
    );
    assert!(
        refused.as_ref().is_err_and(|refusal| refusal
            .sentence()
            .contains("could not say how the step's unit closed, so nothing was undone")),
        "refused by the question about the step, before anything was staged: {refused:?}"
    );
    fixture.store.refuses_history.store(true, Ordering::SeqCst);
    let refused = fixture.as_("ana", "kernel.undo", "{}").await;
    fixture.store.refuses_history.store(false, Ordering::SeqCst);
    assert!(
        is(
            &refused,
            RefusalKind::Unavailable,
            nc_local::LocalError::Unavailable(String::new()).code()
        ),
        "{refused:?}"
    );
    assert_eq!(
        fixture.held("noctua:reference", "note/photo").await,
        Some(r#"{"text":"kept"}"#.to_owned())
    );
    fixture
        .as_("ana", "kernel.undo", "{}")
        .await
        .expect("answered once both can answer");
}

/// A redo of an undo the journal never kept is refused.
///
/// The undo's close was not taken and the store refused its reversal, so the
/// undo stays committed in the store, open in the journal — and a redo of it
/// would put back a step on the word of a history the audit log never saw.
#[tokio::test]
async fn a_redo_of_an_undo_the_journal_never_kept_is_refused() {
    let (fixture, told) = reference().await;
    fixture
        .as_("ana", "reference.note.put", &note("photo", Some("kept")))
        .await
        .expect("written");
    fixture
        .store
        .refuses_reversals
        .store(true, Ordering::SeqCst);
    told.lock().expect("nothing panics holding it").refuses =
        Some(|entry: &Entry| closes(entry, "kernel.undo"));
    let refused = fixture
        .as_("ana", "kernel.undo", "{}")
        .await
        .expect_err("the journal did not take the close");
    told.lock().expect("nothing panics holding it").refuses = None;
    fixture
        .store
        .refuses_reversals
        .store(false, Ordering::SeqCst);
    assert!(
        refused.sentence().contains("stays committed in the store"),
        "{refused}"
    );
    let undo = fixture.units_of("ana").await.remove(0);
    assert_eq!(
        (undo.meta.label.as_str(), undo.state),
        ("kernel.undo", nc_local::UnitState::Committed)
    );
    let redo = fixture.as_("ana", "kernel.redo", "{}").await;
    assert!(
        is(
            &redo,
            RefusalKind::Internal,
            ErrorCode::KernelStepUnrecorded
        ),
        "{redo:?}"
    );
    assert_eq!(fixture.held("noctua:reference", "note/photo").await, None);
}

/// A kernel with the reference plugin loaded granted the store and the blobs,
/// over a journal in memory a test can make refuse.
async fn with_the_blobs() -> (Fixture<Failing<nc_journal::InMemory>>, Arc<Mutex<Told>>) {
    let (fixture, told) = with(&[], "").await;
    let manifest = PluginManifest::parse(
        "id = \"noctua:reference\"\nnamespace = \"reference\"\ncapabilities = [\"store\", \
         \"blobs.read\", \"blobs.write\"]\n",
    )
    .expect("a manifest granting the blobs");
    fixture
        .kernel
        .load(manifest, &nc_kernel::component_for_tests("reference"))
        .await
        .expect("loads");
    (fixture, told)
}

/// Has `ana` redo her newest undo while the journal takes no redo's close,
/// and answers the refusal.
async fn redo_not_closed(
    fixture: &Fixture<Failing<nc_journal::InMemory>>,
    told: &Mutex<Told>,
) -> Refusal {
    told.lock().expect("nothing panics holding it").refuses =
        Some(|entry: &Entry| closes(entry, "kernel.redo"));
    let refused = fixture
        .as_("ana", "kernel.redo", "{}")
        .await
        .expect_err("the journal did not take the close");
    told.lock().expect("nothing panics holding it").refuses = None;
    assert!(refused.sentence().contains("reverted at once"), "{refused}");
    refused
}

/// A redo the journal did not close is reverted with only the pins it put
/// back taken off: a blob another unit holds keeps its pin.
///
/// Ana's undo took the pin off her blob and carla's step then made the same
/// bytes, pinning them again: ana's redo finds the blob pinned, records no pin
/// of its own put back, and its reversal takes none off. A blob a committed
/// unit notes it holds keeps its pin through the reversal even where its row
/// said unpinned as the redo ran, because the reversal asks again who holds
/// it. The control is a redo whose blob nothing else holds, whose reversal
/// takes off the pin it put back.
///
/// What it holds against: the reversal taking off the pin carla's committed
/// step holds, and the sweep deleting the bytes.
#[tokio::test]
async fn a_redo_the_journal_did_not_close_leaves_the_pin_another_unit_holds() {
    let (fixture, told) = with_the_blobs().await;
    let shared = fixture.made("ana", "photo", "shared").await;
    fixture
        .as_("ana", "kernel.undo", "{}")
        .await
        .expect("undone");
    assert!(!fixture.pinned(&shared).await, "the undo took the pin off");
    fixture.made("carla", "recipe", "shared").await;
    redo_not_closed(&fixture, &told).await;
    assert!(
        fixture.pinned(&shared).await,
        "carla's committed step holds the blob, and the redo's reversal unpinned it"
    );
    fixture.clock.advance(KERNEL.blob_orphan_grace_ms + 1);
    let swept = fixture.kernel.sweep_blobs().await.expect("swept");
    assert_eq!(swept.forgotten, 0);

    let (fixture, told) = with_the_blobs().await;
    let held = fixture.made("ana", "photo", "held").await;
    fixture
        .as_("ana", "kernel.undo", "{}")
        .await
        .expect("undone");
    seeded_unit(&fixture.store.inner, "bruno", |unit| {
        unit.notes = vec![format!(
            r#"{{"hash":"{held}","note":"blob_created","pinned_before":true}}"#
        )];
    })
    .await;
    assert!(!fixture.pinned(&held).await, "the row says unpinned");
    redo_not_closed(&fixture, &told).await;
    assert!(
        fixture.pinned(&held).await,
        "bruno's committed unit holds the blob, and the redo's reversal unpinned it"
    );

    let (fixture, told) = with_the_blobs().await;
    let alone = fixture.made("ana", "photo", "alone").await;
    fixture
        .as_("ana", "kernel.undo", "{}")
        .await
        .expect("undone");
    redo_not_closed(&fixture, &told).await;
    assert!(
        !fixture.pinned(&alone).await,
        "the redo's reversal left on the pin the redo put back, which nothing else holds"
    );
}

/// A unit that is not kept names the compensations registered in it, none of
/// which runs: the store refused its commit, or the journal its close.
///
/// A compensation runs on undo only, and a unit that is not kept is no step
/// an undo reaches, so whoever dispatched it is told what the plugin meant to
/// set right what the unit sent with. The plugin is granted no network, so its
/// request is refused and the unit holds the compensation and nothing else.
///
/// What it holds against: an aborted unit's refusal naming nothing it
/// registered, though what an abort answers is meant to name each one.
#[tokio::test]
async fn a_unit_not_kept_names_the_compensations_registered_in_it() {
    let (fixture, told) = with(&[], "").await;
    let manifest = PluginManifest::parse(
        "id = \"noctua:reference\"\nnamespace = \"reference\"\ncapabilities = [\"store\", \
         \"commands.begin\"]\n",
    )
    .expect("a manifest granting units");
    fixture
        .kernel
        .load(manifest, &nc_kernel::component_for_tests("reference"))
        .await
        .expect("loads");
    let post = Value::object([
        ("note", Value::text("photo")),
        ("url", Value::text("https://example.com/")),
    ])
    .to_canonical();
    let named = "; the compensation(s) registered in it did not run: `reference.note.put`";
    fixture.store.refuses_steps.store(true, Ordering::SeqCst);
    let refused = fixture
        .as_("ana", "reference.compensated.post", &post)
        .await
        .expect_err("the store refused the commit");
    fixture.store.refuses_steps.store(false, Ordering::SeqCst);
    assert_eq!(refused.code(), ErrorCode::PluginStoreConflict, "{refused}");
    assert!(refused.sentence().ends_with(named), "{refused}");

    told.lock().expect("nothing panics holding it").refuses =
        Some(|entry: &Entry| closes(entry, "reference.compensated.post"));
    let refused = fixture
        .as_("ana", "reference.compensated.post", &post)
        .await
        .expect_err("the journal did not take the close");
    told.lock().expect("nothing panics holding it").refuses = None;
    assert!(
        refused.sentence().contains("reverted at once") && refused.sentence().ends_with(named),
        "{refused}"
    );
    fixture
        .as_("ana", "reference.compensated.post", &post)
        .await
        .expect("kept once the store and the journal take it");
}

/// A second household, `cousins`: every entity of the world moved from the
/// realm `home` into it, and its `diary` given to `bruno`.
///
/// Every entity belongs to exactly one realm, and the same id in another
/// realm is another entity (`NOCTUA.md §10.3`, `§14.3`).
fn the_cousins() -> String {
    fn moved(uid: &mut serde_json::Value) {
        if uid["type"] == "Realm" && uid["id"] == "home" {
            uid["id"] = serde_json::Value::from("cousins");
        }
    }
    let mut world: serde_json::Value = serde_json::from_str(WORLD).expect("the world is JSON");
    for entity in world.as_array_mut().expect("the world is a list") {
        moved(&mut entity["uid"]);
        if let Some(parents) = entity["parents"].as_array_mut() {
            for parent in parents {
                moved(parent);
            }
        }
        if entity["uid"]["type"] == "Content" && entity["uid"]["id"] == "diary" {
            entity["attrs"]["owner"]["__entity"]["id"] = serde_json::Value::from("bruno");
        }
    }
    world.to_string()
}

/// A kernel of `realm` over `world` and `store`, its units minted by `units`,
/// with `people` granted `g.current` there and the reference plugin loaded,
/// and the directory the people are read from.
async fn a_kernel_of(
    units: Units,
    world: &str,
    realm: &str,
    store: Stalling,
    people: &[&str],
) -> (Kernel<Env<nc_journal::InMemory>>, MemoryDirectory) {
    let seen = MemoryDirectory::from_world(world, &family()).expect("the world is valid");
    let mut directory = MemoryDirectory::from_world(world, &family())
        .expect("the world is valid")
        .with_epoch(realm, EPOCH);
    for person in people {
        directory = directory
            .with_grant(
                realm,
                EntityRef {
                    type_name: "User".to_owned(),
                    id: (*person).to_owned(),
                },
                "g.current",
            )
            .expect("the realm holds the grant");
    }
    let clock = Manual::at(NOW_MS);
    let keeper = the_keeper();
    let kernel = Kernel::open(KernelConfig {
        store,
        blobs: nc_kernel::blobs::MemoryBlobs::default(),
        pdp: Ok(family()),
        journal: nc_journal::Journal::sharing(
            nc_journal::InMemory::default(),
            seen.realm_id(realm).expect("a realm of the world"),
            keeper,
            units,
        ),
        directory: Flaky {
            inner: directory,
            mode: Arc::new(Mutex::new(Mode::Answers)),
            call: Arc::new(Mutex::new(Call::Realm)),
        },
        clock,
        log: Arc::new(Captured::default()),
        transport: nc_kernel::net::Scripted::default(),
    })
    .await
    .expect("the kernel opens");
    kernel
        .load(
            manifest("noctua:reference", "reference", ""),
            &nc_kernel::component_for_tests("reference"),
        )
        .await
        .expect("the reference plugin loads");
    (kernel, seen)
}

/// A minter journals may share, as the ones of one machine do.
type Units = Arc<Mutex<Minter<Manual>>>;

/// A minter of its own, on `flake`, at the instant the policy cases call now.
fn minting_on(flake: u16) -> Units {
    Arc::new(Mutex::new(Minter::new(
        MachineFlake::from_value(flake).expect("a flake"),
        Manual::at(NOW_MS),
    )))
}

/// The two kernels of one machine, one of `home` and one of `cousins`, over
/// one local database: as `a_kernel_of` makes them, minting on `flakes`, each
/// with a minter of its own.
async fn two_realms_on_one_machine(
    flakes: (u16, u16),
    at_home: &[&str],
) -> (
    (Kernel<Env<nc_journal::InMemory>>, MemoryDirectory),
    (Kernel<Env<nc_journal::InMemory>>, MemoryDirectory),
) {
    two_realms_minting_with((minting_on(flakes.0), minting_on(flakes.1)), at_home).await
}

/// The two kernels of one machine, as [`two_realms_on_one_machine`], their
/// units minted by `units`.
async fn two_realms_minting_with(
    units: (Units, Units),
    at_home: &[&str],
) -> (
    (Kernel<Env<nc_journal::InMemory>>, MemoryDirectory),
    (Kernel<Env<nc_journal::InMemory>>, MemoryDirectory),
) {
    let store = stalling(
        LocalStore::in_memory_for_tests()
            .await
            .expect("the local database opens in memory"),
    );
    let home = a_kernel_of(units.0, WORLD, "home", store.clone(), at_home).await;
    let cousins = a_kernel_of(units.1, &the_cousins(), "cousins", store, &["bruno"]).await;
    (home, cousins)
}

/// Two realms on the one flake their machine has (`NOCTUA.md §16.2`), their
/// journals sharing the machine's minter, each keep a unit of the same
/// millisecond — and minting with minters of their own on that flake, the
/// second is refused as a unit the file holds already, which is why they
/// share it.
///
/// **Two realms on one flake, as a machine runs them**: given two flakes, two
/// realms minting with minters of their own would never number their units
/// alike, and a test of them would not show the refusal above.
#[tokio::test]
async fn two_realms_sharing_their_machines_minter_each_keep_their_units() {
    let shared = minting_on(7);
    for (units, kept) in [
        ((Arc::clone(&shared), Arc::clone(&shared)), true),
        ((minting_on(7), minting_on(7)), false),
    ] {
        let ((home, home_world), (away, away_world)) =
            two_realms_minting_with(units, &["ana"]).await;
        home.dispatch(
            &caller_of(&home_world, "ana"),
            "reference.note.put",
            &note("diary", Some("what ana keeps at home")),
        )
        .await
        .expect("ana writes her own private diary at home");
        let away_write = away
            .dispatch(
                &caller_of(&away_world, "bruno"),
                "reference.note.put",
                &note("diary", Some("written in cousins")),
            )
            .await;
        assert_eq!(
            away_write.is_ok(),
            kept,
            "sharing the machine's minter: {kept}; bruno's write in `cousins`: {away_write:?}"
        );
    }
}

/// The user `name` of `world`, as a caller.
fn caller_of(world: &MemoryDirectory, name: &str) -> Caller {
    Caller::new(
        world
            .principal(PrincipalKind::User, name)
            .unwrap_or_else(|| panic!("{name} is a user")),
    )
}

/// What a plugin keeps for an entity of one realm is not read in another
/// whose kernel shares the machine's local database.
///
/// **Not by plugin and key alone**: a store that kept a plugin's keys so would
/// let bruno in `cousins` — the owner of that realm's diary — read through the
/// decision point what ana keeps of hers at home, where he is denied it.
#[tokio::test]
async fn what_a_plugin_keeps_in_one_realm_is_not_read_in_another() {
    let ((home, home_world), (away, away_world)) =
        two_realms_on_one_machine((3, 4), &["ana", "bruno"]).await;
    home.dispatch(
        &caller_of(&home_world, "ana"),
        "reference.note.put",
        &note("diary", Some("what ana keeps at home")),
    )
    .await
    .expect("ana writes her own private diary at home");
    let at_home = home
        .dispatch(
            &caller_of(&home_world, "bruno"),
            "reference.note.get",
            &note("diary", None),
        )
        .await;
    assert!(
        is(&at_home, RefusalKind::Denied, ErrorCode::PolicyDenied),
        "at home, bruno does not read ana's private diary: {at_home:?}"
    );
    let away_read = away
        .dispatch(
            &caller_of(&away_world, "bruno"),
            "reference.note.get",
            &note("diary", None),
        )
        .await
        .expect("in `cousins`, bruno owns that realm's diary");
    assert_eq!(
        away_read, "null",
        "nothing was ever written about the diary of the realm `cousins`; what bruno read \
         there is what ana keeps in the realm `home`"
    );
}

/// A write in one realm does not change what a plugin keeps in another whose
/// kernel shares the machine's local database.
///
/// The write half of the one above, which the same bug broke: bruno's write
/// of the `cousins` diary replaced ana's at home.
#[tokio::test]
async fn a_write_in_one_realm_does_not_change_what_another_keeps() {
    let ((home, home_world), (away, away_world)) =
        two_realms_on_one_machine((5, 6), &["ana"]).await;
    let ana = caller_of(&home_world, "ana");
    home.dispatch(
        &ana,
        "reference.note.put",
        &note("diary", Some("what ana keeps at home")),
    )
    .await
    .expect("ana writes her own private diary at home");
    away.dispatch(
        &caller_of(&away_world, "bruno"),
        "reference.note.put",
        &note("diary", Some("written in cousins")),
    )
    .await
    .expect("in `cousins`, bruno owns that realm's diary and writes it");
    let kept = home
        .dispatch(&ana, "reference.note.get", &note("diary", None))
        .await
        .expect("ana reads her own diary at home");
    assert_eq!(
        kept, r#"{"text":"what ana keeps at home"}"#,
        "nobody of the realm `home` changed ana's diary"
    );
    let theirs = away
        .dispatch(
            &caller_of(&away_world, "bruno"),
            "reference.note.get",
            &note("diary", None),
        )
        .await
        .expect("bruno reads his own diary in `cousins`");
    assert_eq!(theirs, r#"{"text":"written in cousins"}"#);
}

/// A confirmation is not spent on a dispatch the directory could not decide,
/// whichever of its three questions it failed at.
///
/// Each time ana dispatches a command that cannot be undone with a fresh
/// confirmation, the directory fails, and she dispatches again with the same
/// confirmation once it answers. **Not asked after the confirmation is
/// redeemed**: asked then, a directory failing there would take the person's
/// single-use confirmation with a dispatch the kernel never decided.
#[tokio::test]
async fn a_confirmation_is_not_spent_on_a_dispatch_the_directory_could_not_decide() {
    let (fixture, _) = reference().await;
    let ana = fixture.principal("ana");
    let done = "reference.irreversible.do";
    let mut spent = Vec::new();
    for (call, subject) in [
        (Call::Realm, "photo"),
        (Call::Grant, "notice"),
        (Call::Slice, "diary"),
    ] {
        let args = note(subject, None);
        let caller = Caller::new(ana.clone())
            .confirmed(fixture.kernel.confirmation_for_tests(&ana, done, &args));
        *fixture.call.lock().expect("nothing panics holding it") = call;
        *fixture.mode.lock().expect("nothing panics holding it") = Mode::Fails;
        let undecided = fixture.kernel.dispatch(&caller, done, &args).await;
        assert!(
            is(
                &undecided,
                RefusalKind::Denied,
                ErrorCode::PolicyDirectoryUnavailable
            ),
            "{call:?}: {undecided:?}"
        );
        *fixture.mode.lock().expect("nothing panics holding it") = Mode::Answers;
        if let Err(refused) = fixture.kernel.dispatch(&caller, done, &args).await {
            spent.push(format!("{call:?}: {refused}"));
        }
    }
    assert!(
        spent.is_empty(),
        "the directory could not answer the first dispatch, so the kernel never decided it, \
         and the confirmation it carried should still be held: {spent:#?}"
    );
}

/// A kernel of `home` whose world holds a device of ana's own identifier,
/// both granted, with the reference plugin loaded granting the store and the
/// context, and the directory the two are read from.
async fn a_kernel_with_a_device_of_anas_id() -> (Kernel<Env<nc_journal::InMemory>>, MemoryDirectory)
{
    let mut world: serde_json::Value = serde_json::from_str(WORLD).expect("the world is JSON");
    world
        .as_array_mut()
        .expect("a list")
        .push(serde_json::json!({
            "uid": {"type": "Device", "id": "ana"},
            "attrs": {
                "owner": {"__entity": {"type": "User", "id": "ana"}},
                "node_key": "0011223344556677",
                "attested": true
            },
            "parents": [{"type": "Realm", "id": "home"}]
        }));
    let world = world.to_string();
    let seen = MemoryDirectory::from_world(&world, &family()).expect("the world is valid");
    let mut directory = MemoryDirectory::from_world(&world, &family())
        .expect("the world is valid")
        .with_epoch("home", EPOCH);
    for kind in ["User", "Device"] {
        directory = directory
            .with_grant(
                "home",
                EntityRef {
                    type_name: kind.to_owned(),
                    id: "ana".to_owned(),
                },
                "g.current",
            )
            .expect("the realm holds the grant");
    }
    let clock = Manual::at(NOW_MS);
    let keeper = the_keeper();
    let store = stalling(
        LocalStore::in_memory_for_tests()
            .await
            .expect("the local database opens in memory"),
    );
    let kernel: Kernel<Env<nc_journal::InMemory>> = Kernel::open(KernelConfig {
        store,
        blobs: nc_kernel::blobs::MemoryBlobs::default(),
        pdp: Ok(family()),
        journal: nc_journal::Journal::new(
            nc_journal::InMemory::default(),
            seen.realm_id("home").expect("home is a realm"),
            keeper,
            Minter::new(MachineFlake::from_value(7).expect("a flake"), clock.clone()),
        ),
        directory: Flaky {
            inner: directory,
            mode: Arc::new(Mutex::new(Mode::Answers)),
            call: Arc::new(Mutex::new(Call::Realm)),
        },
        clock,
        log: Arc::new(Captured::default()),
        transport: nc_kernel::net::Scripted::default(),
    })
    .await
    .expect("the kernel opens");
    kernel
        .load(
            PluginManifest::parse(
                "id = \"noctua:reference\"\nnamespace = \"reference\"\ncapabilities = \
                 [\"store\", \"context\"]\n",
            )
            .expect("a manifest"),
            &nc_kernel::component_for_tests("reference"),
        )
        .await
        .expect("the reference plugin loads");
    (kernel, seen)
}

/// A confirmation issued to the user `ana` is not redeemed by the device
/// `ana`: Cedar holds the two for two principals, and so does the kernel.
///
/// **Not by the principal's identifier alone**: bound to it, a confirmation of
/// the person's would be handed to the device.
#[tokio::test]
async fn a_confirmation_issued_to_a_user_is_not_redeemed_by_a_device_of_the_same_id() {
    let (kernel, seen) = a_kernel_with_a_device_of_anas_id().await;
    let user = seen.principal(PrincipalKind::User, "ana").expect("a user");
    let device = seen
        .principal(PrincipalKind::Device, "ana")
        .expect("a device");
    assert_ne!(user, device, "two principals");
    let read = "reference.context.read";
    let args = note("photo", None);
    let issued_to_the_user = kernel.confirmation_for_tests(&user, read, &args);
    let answered = kernel
        .dispatch(
            &Caller::new(device).confirmed(issued_to_the_user),
            read,
            &args,
        )
        .await
        .expect("an attested device reads what the house shares");
    assert_eq!(
        field(&answered, "confirmed"),
        Value::Bool(false),
        "the device was handed a person's confirmation it was never issued: {answered}"
    );
    let issued_to_the_user = kernel.confirmation_for_tests(&user, read, &args);
    let answered = kernel
        .dispatch(
            &Caller::new(user).confirmed(issued_to_the_user),
            read,
            &args,
        )
        .await
        .expect("ana reads what the house shares");
    assert_eq!(
        field(&answered, "confirmed"),
        Value::Bool(true),
        "the user it was issued to redeems it: {answered}"
    );
}

/// The blob carla's kept step makes of `shared` after ana's `blob.put` of the
/// same bytes was left committed in the store and open in the journal — its
/// close refused, and its reversal — with reversals allowed again after it.
async fn a_blob_left_committed_then_made_again<J>(
    fixture: &Fixture<J>,
    told: &Arc<Mutex<Told>>,
) -> String
where
    J: nc_journal::Store + Clone + Send + Sync + Debug + 'static,
{
    fixture
        .store
        .refuses_reversals
        .store(true, Ordering::SeqCst);
    told.lock().expect("nothing panics holding it").refuses =
        Some(|entry: &Entry| entry.record.result == Outcome::UnitCommitted);
    let args = Value::object([
        ("note", Value::text("photo")),
        ("text", Value::text("shared")),
    ])
    .to_canonical();
    let refused = fixture.as_("ana", "reference.blob.put", &args).await;
    told.lock().expect("nothing panics holding it").refuses = None;
    fixture
        .store
        .refuses_reversals
        .store(false, Ordering::SeqCst);
    assert!(
        is(
            &refused,
            RefusalKind::Unavailable,
            ErrorCode::JournalUnavailable
        ),
        "{refused:?}"
    );
    let shared = fixture.made("carla", "recipe", "shared").await;
    assert!(fixture.pinned(&shared).await, "carla's kept step pins it");
    shared
}

/// A unit reverted before a prune keeps the pin of a blob a step kept since
/// made again: that step holds it.
///
/// **Asked of every pin, not of a redo's alone**: a reversal that asked whether
/// another unit holds only the pins a redo put back would take off every pin
/// the unit itself put on — so the sweep would delete the bytes of carla's kept
/// step.
#[tokio::test]
async fn a_unit_reverted_before_a_prune_keeps_the_pin_a_step_kept_since_holds() {
    let (fixture, told) = with_the_blobs().await;
    let shared = a_blob_left_committed_then_made_again(&fixture, &told).await;
    fixture
        .prune_as("ana", r#"{"table":"store_units"}"#)
        .await
        .expect("the unit left is reverted, and the prune runs");
    assert!(
        fixture.pinned(&shared).await,
        "the reversal of ana's unit took the pin carla's kept step holds"
    );
    fixture.clock.advance(KERNEL.blob_orphan_grace_ms + 1);
    let swept = fixture.kernel.sweep_blobs().await.expect("swept");
    assert_eq!(swept.forgotten, 0, "{swept:?}");
    let read = fixture
        .as_(
            "carla",
            "reference.blob.get",
            &Value::object([
                ("hash", Value::text(shared.clone())),
                ("note", Value::text("recipe")),
            ])
            .to_canonical(),
        )
        .await;
    assert!(read.is_ok(), "carla reads her blob back: {read:?}");
}

/// A unit the kernel opened next recovers keeps the pin of a blob a step kept
/// since made again, as one reverted before a prune does.
#[tokio::test]
async fn a_unit_recovered_on_open_keeps_the_pin_a_step_kept_since_holds() {
    let told = Arc::new(Mutex::new(Told::default()));
    let journal = Failing {
        inner: nc_journal::InMemory::default(),
        told: Arc::clone(&told),
    };
    let fixture = opened(journal, Ok(family()), &[], "").await;
    let manifest = PluginManifest::parse(
        "id = \"noctua:reference\"\nnamespace = \"reference\"\ncapabilities = [\"store\", \
         \"blobs.read\", \"blobs.write\"]\n",
    )
    .expect("a manifest granting the blobs");
    fixture
        .kernel
        .load(manifest, &nc_kernel::component_for_tests("reference"))
        .await
        .expect("loads");
    let shared = a_blob_left_committed_then_made_again(&fixture, &told).await;
    let store = fixture.store.clone();
    let written = written_by(&fixture).await;
    drop(fixture);
    let log = Arc::new(Captured::default());
    let journal = Failing {
        inner: written,
        told: Arc::new(Mutex::new(Told::default())),
    };
    let _again = opening(4, journal, Ok(family()), store.clone(), Arc::clone(&log))
        .await
        .expect("the kernel opens again");
    assert_eq!(log.named("kernel.recovered").len(), 1, "{:?}", log.lines());
    let pinned = store
        .inner
        .blob(&shared)
        .await
        .expect("the store answers")
        .is_some_and(|row| row.pinned);
    assert!(
        pinned,
        "carla's kept step holds the blob, and the recovery of ana's unit took its pin off"
    );
}

/// A kernel opened again over what the first one wrote recovers the unit it
/// left committed in the store and open in the journal: the control that
/// makes "nothing left open" mean something wherever a reopen asserts it.
#[tokio::test]
async fn a_kernel_opened_again_over_what_the_first_wrote_finds_the_unit_it_left_open() {
    let told = Arc::new(Mutex::new(Told::default()));
    let journal = Failing {
        inner: nc_journal::InMemory::default(),
        told: Arc::clone(&told),
    };
    let fixture = opened(
        journal,
        Ok(family()),
        &[("noctua:reference", "reference")],
        "",
    )
    .await;
    let left = left_committed(&fixture, &told).await;
    fixture
        .store
        .refuses_reversals
        .store(false, Ordering::SeqCst);
    let store = fixture.store.clone();
    let written = written_by(&fixture).await;
    drop(fixture);
    let log = Arc::new(Captured::default());
    let journal = Failing {
        inner: written,
        told: Arc::new(Mutex::new(Told::default())),
    };
    let _again = opening(4, journal, Ok(family()), store.clone(), Arc::clone(&log))
        .await
        .expect("the kernel opens again");
    assert!(
        log.named("kernel.recovered")
            .iter()
            .any(|line| line.text("unit") == Some(left.to_crockford())),
        "the unit {left} left open in the first kernel's journal is not found by a kernel \
         opened again over it: {:?}",
        log.lines()
    );
    assert_eq!(
        photo_in(&store).await,
        None,
        "and what it wrote is reverted"
    );
}

/// A blob of ana's, kept by a step, whose undo the store committed with the
/// pin taken off while the journal did not take its close and the store
/// refused its reversal — answered as the blob's hash.
async fn a_blob_its_undo_left_unpinned(
    fixture: &Fixture<Failing<nc_journal::InMemory>>,
    told: &Arc<Mutex<Told>>,
) -> String {
    let made = fixture
        .as_("ana", "reference.blob.put", &blob("owed", 2))
        .await
        .expect("a blob is kept");
    let made = text_of(&made, "hash");
    fixture
        .store
        .refuses_reversals
        .store(true, Ordering::SeqCst);
    told.lock().expect("nothing panics holding it").refuses =
        Some(|entry: &Entry| entry.record.result == Outcome::UnitCommitted);
    let refused = fixture.as_("ana", "kernel.undo", "{}").await;
    told.lock().expect("nothing panics holding it").refuses = None;
    assert!(
        is(
            &refused,
            RefusalKind::Unavailable,
            ErrorCode::JournalUnavailable
        ),
        "{refused:?}"
    );
    let row = fixture
        .store
        .inner
        .blob(&made)
        .await
        .expect("the store answers")
        .expect("the row");
    assert!(
        !row.pinned,
        "the undo, committed in the store, took the pin off"
    );
    made
}

/// A blob an undo took the pin off is on this machine once the undo — whose
/// close the journal did not take and whose reversal the store refused — is
/// reverted by the next prune, though the sweep ran a day in between.
///
/// **The sweep waits for such a unit, as the prune does**: waiting for none, it
/// would forget the row and the bytes the undo had unpinned; the prune would
/// then revert the undo, which pins the blob again — inserting a pinned row for
/// bytes that are gone — and commit the step again as holding it.
#[tokio::test]
async fn a_blob_a_reversal_still_owed_pins_again_is_not_swept_before_it() {
    let (fixture, told) = with_the_blobs().await;
    let made = a_blob_its_undo_left_unpinned(&fixture, &told).await;
    fixture
        .clock
        .advance(nc_catalog::KERNEL.blob_orphan_grace_ms + 1);
    let swept = fixture.kernel.sweep_blobs().await.expect("swept");
    assert_eq!(swept.forgotten, 0, "{swept:?}");
    fixture
        .store
        .refuses_reversals
        .store(false, Ordering::SeqCst);
    let pumped = fixture.kernel.pump().await;
    assert_eq!(pumped.prune_refused, None, "{pumped:?}");
    let steps: Vec<_> = fixture
        .units_of("ana")
        .await
        .into_iter()
        .filter(|unit| unit.meta.label == "reference.blob.put")
        .map(|unit| unit.state)
        .collect();
    assert_eq!(
        steps,
        [nc_local::UnitState::Committed],
        "the reversal of the undo commits the step again"
    );
    let located = fixture
        .as_("ana", "reference.blob.locate", &hash_of(&made))
        .await
        .expect("located");
    assert_eq!(text_of(&located, "availability"), "local");
}

/// A reversal pins again only a blob the store still holds: one whose row is
/// gone gets no pinned row with nothing behind it, and its step's blob is said
/// to be unavailable rather than kept.
///
/// **What it holds against**: the reversal inserting a pinned row for a blob
/// the sweep had taken, which no sweep would take again — a blob said to be
/// kept with no bytes behind it. The sweep leaves such a blob alone; this holds
/// the reversal to it should the row go another way.
#[tokio::test]
async fn a_reversal_pins_again_no_blob_whose_row_is_gone() {
    let (fixture, told) = with_the_blobs().await;
    let made = a_blob_its_undo_left_unpinned(&fixture, &told).await;
    assert!(
        fixture
            .store
            .inner
            .forget_blob(&made)
            .await
            .expect("the store answers"),
        "the row of an unpinned blob is forgotten"
    );
    fixture
        .store
        .refuses_reversals
        .store(false, Ordering::SeqCst);
    let pumped = fixture.kernel.pump().await;
    assert_eq!(pumped.prune_refused, None, "{pumped:?}");
    assert_eq!(
        fixture
            .store
            .inner
            .blob(&made)
            .await
            .expect("the store answers"),
        None,
        "the reversal put a pinned row back for a blob the store no longer held"
    );
    let located = fixture
        .as_("ana", "reference.blob.locate", &hash_of(&made))
        .await
        .expect("located");
    assert_eq!(text_of(&located, "availability"), "unavailable");
}

/// A unit a stopped kernel left committed in the store and open in the
/// journal is reverted when that kernel opens again, though the pruner ran
/// over the file in between — as another realm's kernel, sharing it, runs it.
///
/// **Held from the pruner until the journal's close is heard**: a pruner that
/// took the oldest units of the file, holding back only the ones the kernel
/// running it left, would take the stopped unit's record, and the recovery
/// close it as aborted with its write left in the store. The kernel commits a
/// unit held from the pruner until it hears the journal's close.
#[tokio::test]
async fn a_unit_a_stopped_kernel_left_open_is_reverted_though_the_pruner_ran_meanwhile() {
    let Some((pool, schema)) = postgres("left_open_pruned").await else {
        return;
    };
    let scratch = Scratch::new("left-open-pruned");
    let profile = scratch.0.join("the-close");
    let status = the_stopped_child(STOPPED_OVER_POSTGRES, "the-close", &schema, &profile);
    let after = the_pruner_runs_and_the_kernel_opens_again(status, &profile, |_| {
        nc_journal::postgres::Postgres::in_schema(pool.clone(), &schema)
    })
    .await;
    finish(pool).await;
    after.nothing_of_the_unit_is_left();
}

/// A unit a stopped kernel left committed in the store and open in the local
/// journal is reverted when that kernel opens again, though the pruner ran
/// over the file in between — the file that holds the journal too.
///
/// **The daemon's case, and the one every system runs**: what
/// `a_unit_a_stopped_kernel_left_open_is_reverted_though_the_pruner_ran_meanwhile`
/// holds over PostgreSQL, held over `nc_local::LocalJournal`, where the pruner
/// runs over the very file whose `local_journal_entries` the recovery asks.
/// The table is the realm's pack's to bound (`policy-pack`), so the pruner
/// leaves every row of it, and the unit is found open and reverted.
#[tokio::test]
async fn a_unit_a_stopped_kernel_left_open_in_the_local_journal_is_reverted_though_the_pruner_ran_meanwhile()
 {
    let scratch = Scratch::new("left-open-pruned-in-the-file");
    let profile = scratch.0.join("the-close");
    let status = the_stopped_child(STOPPED_OVER_THE_FILE, "the-close", "", &profile);
    let after =
        the_pruner_runs_and_the_kernel_opens_again(status, &profile, |store| store.inner.journal())
            .await;
    assert!(
        after.journaled.0 > 0,
        "the child's unit is in the file's journal: {:?}",
        after.journaled
    );
    assert_eq!(
        after.journaled.0, after.journaled.1,
        "the pruner took nothing of the journal"
    );
    after.nothing_of_the_unit_is_left();
}

/// What a kernel opened again over the file a stopped child left found, once
/// the pruner had run over it.
struct AfterThePruner {
    /// How many records of the stopped unit the pruner left.
    left: i64,
    /// The note the child's unit wrote, as the store held it once the kernel
    /// opened again.
    photo: Option<String>,
    /// How many rows the file's own journal held before the pruner ran, and
    /// after.
    journaled: (i64, i64),
}

impl AfterThePruner {
    /// Holds the unit's record to have outlived the pruner, and its write to
    /// have been reverted.
    fn nothing_of_the_unit_is_left(&self) {
        assert_eq!(
            self.left, 1,
            "the pruner took the record the recovery reverts from"
        );
        assert_eq!(
            self.photo, None,
            "the journal closed the stopped unit as aborted and its write is still in the store"
        );
    }
}

/// Runs the pruner over a stopped child's file, then opens a kernel over it.
///
/// The child stopped with `status` between the store's commit and the
/// journal's close, and the file in `profile` has its units table filled past
/// its bound; the kernel opens over that file and over the journal handed over
/// by `journal_of`.
async fn the_pruner_runs_and_the_kernel_opens_again<J, F>(
    status: std::process::ExitStatus,
    profile: &std::path::Path,
    journal_of: F,
) -> AfterThePruner
where
    J: nc_journal::Store + Debug + Send + Sync + 'static,
    F: FnOnce(&Stalling) -> J,
{
    assert!(
        nc_platform::testing::stopped_by_abort(status),
        "the child did not stop the way `abort` stops it: {status}"
    );
    let store = stalling(
        LocalStore::open(profile)
            .await
            .expect("the file opens after the stop"),
    );
    assert!(
        photo_in(&store).await.is_some(),
        "the child stopped between the store's commit and the journal's close"
    );
    let stopped: i64 = sqlx::query_scalar("SELECT max(unit) FROM main.store_units")
        .fetch_one(store.inner.pool())
        .await
        .expect("the store counts");
    let before = journal_rows(&store.inner).await;
    let (_, batch) = units_past_their_bound_from(&store.inner, stopped).await;
    let mut target = nc_prune::sqlite::Sqlite::new(store.inner.pool().clone());
    let pruned = nc_prune::run(&mut target, nc_prune::Request::every())
        .await
        .expect("the pruner runs");
    let removed: u64 = pruned.iter().map(|table| table.removed).sum();
    assert_eq!(removed, batch, "{pruned:?}");
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM main.store_units WHERE unit = ?1")
        .bind(stopped)
        .fetch_one(store.inner.pool())
        .await
        .expect("the store counts");
    let after = journal_rows(&store.inner).await;
    let log = Arc::new(Captured::default());
    let fixture = opening(
        8,
        journal_of(&store),
        Ok(family()),
        store.clone(),
        Arc::clone(&log),
    )
    .await
    .expect("the kernel opens again");
    let photo = photo_in(&store).await;
    drop(fixture);
    store.inner.close().await;
    AfterThePruner {
        left,
        photo,
        journaled: (before, after),
    }
}

/// How many rows the local file's own journal holds.
async fn journal_rows(store: &LocalStore) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM main.local_journal_entries")
        .fetch_one(store.pool())
        .await
        .expect("the journal counts")
}

/// The command that proposes a file's name.
const PROPOSE: &str = "naming.propose";

/// The command that reads a file's name back.
const READ_NAME: &str = "naming.read";

/// The deciding `@id` of every naming command a person is allowed.
const NAMES_FILES: &str = "family.a_member_names_files";

/// A ceiling that makes nothing durable, so a minter given it mints nothing.
#[derive(Debug)]
struct NothingDurable;

impl nc_ids::Ceiling for NothingDurable {
    fn raise(&mut self, _at_least: u64) -> Result<u64, nc_ids::CeilingRefused> {
        Err(nc_ids::CeilingRefused)
    }

    fn settle(&mut self, _high_water: u64) -> Result<(), nc_ids::CeilingRefused> {
        Err(nc_ids::CeilingRefused)
    }
}

/// A kernel with no plugin, over the world the policy cases are about, with
/// grants for the household's persons, ana's laptop and ana's assistant —
/// minting on a minter held to `ceiling` when one is given — and that world.
async fn naming(
    ceiling: Option<Box<dyn nc_ids::Ceiling>>,
) -> (Kernel<Env<nc_journal::InMemory>>, MemoryDirectory) {
    let seen = MemoryDirectory::from_world(WORLD, &family()).expect("the world is valid");
    let mut directory = MemoryDirectory::from_world(WORLD, &family())
        .expect("the world is valid")
        .with_epoch("home", EPOCH);
    for (kind, id) in [
        ("User", "ana"),
        ("User", "bruno"),
        ("User", "carla"),
        ("User", "dora"),
        ("Device", "ana-laptop"),
        ("Agent", "ana-assistant"),
    ] {
        directory = directory
            .with_grant(
                "home",
                EntityRef {
                    type_name: kind.to_owned(),
                    id: id.to_owned(),
                },
                "g.current",
            )
            .expect("the realm holds the grant");
    }
    let clock = Manual::at(NOW_MS);
    let mut minter = Minter::new(MachineFlake::from_value(9).expect("a flake"), clock.clone());
    if let Some(ceiling) = ceiling {
        minter = minter.with_ceiling(ceiling, 0);
    }
    let kernel = Kernel::open(KernelConfig {
        store: stalling(
            LocalStore::in_memory_for_tests()
                .await
                .expect("the local database opens in memory"),
        ),
        blobs: nc_kernel::blobs::MemoryBlobs::default(),
        pdp: Ok(family()),
        journal: nc_journal::Journal::new(
            nc_journal::InMemory::default(),
            seen.realm_id("home").expect("home is a realm"),
            the_keeper(),
            minter,
        ),
        directory: Flaky {
            inner: directory,
            mode: Arc::new(Mutex::new(Mode::Answers)),
            call: Arc::new(Mutex::new(Call::Realm)),
        },
        clock,
        log: Arc::new(Captured::default()),
        transport: nc_kernel::net::Scripted::default(),
    })
    .await
    .expect("the kernel opens");
    (kernel, seen)
}

/// The entity an entry records its dispatch was decided on, as canonical JSON.
fn entity_of(entry: &Entry) -> Option<String> {
    entry
        .record
        .resource
        .clone()
        .into_object()
        .and_then(|mut fields| fields.remove("entity"))
        .map(|entity| entity.to_canonical())
}

/// Every person of the household has a name proposed and read, each decided
/// on the naming feature.
///
/// **A person's permit, not a guardian's**: a guardian, an adult and a child
/// are each answered, each read back as the name proposed, and each entry
/// names `Feature::"naming"` and the permit that decided it. Neither command
/// changes anything, so no unit is opened and no entry carries one.
#[tokio::test]
async fn every_person_of_the_household_has_a_name_proposed_and_read_decided_on_the_naming_feature()
{
    let (kernel, world) = naming(None).await;
    let args = Value::object([
        ("title", Value::text("Praia de Copacabana")),
        (
            "signature",
            Value::Array(vec![Value::text("familia"), Value::text("fotos")]),
        ),
        ("keywords", Value::Array(vec![Value::text("brasil")])),
        ("extension", Value::text("jpg")),
    ])
    .to_canonical();
    let persons = ["ana", "bruno", "carla"];
    for person in persons {
        let caller = Caller::new(
            world
                .principal(PrincipalKind::User, person)
                .expect("a person"),
        );
        let proposed = kernel
            .dispatch(&caller, PROPOSE, &args)
            .await
            .unwrap_or_else(|refused| panic!("{person} has no name proposed: {refused}"));
        let name = text(&proposed, "name");
        assert_eq!(
            name,
            format!(
                "==familia=fotos--praia-de-copacabana__brasil@@{}.jpg",
                text(&proposed, "id")
            ),
            "{person}"
        );
        let read = kernel
            .dispatch(
                &caller,
                READ_NAME,
                &Value::object([("name", Value::text(name.as_str()))]).to_canonical(),
            )
            .await
            .unwrap_or_else(|refused| panic!("{person} has no name read: {refused}"));
        assert_eq!(field(&read, "canonical"), Value::Bool(true), "{read}");
        for key in ["extension", "id", "keywords", "name", "signature", "title"] {
            assert_eq!(field(&read, key), field(&proposed, key), "{person}: {key}");
        }
    }
    let entries = kernel.journal_for_tests().await.expect("the journal reads");
    assert_eq!(
        entries.len(),
        persons.len() * 2,
        "a naming command opened a unit: {entries:#?}"
    );
    for entry in &entries {
        assert_eq!(entry.record.result, Outcome::Allowed, "{entry:?}");
        assert_eq!(entry.record.action, "Read", "{entry:?}");
        assert_eq!(deciding(entry), Some(NAMES_FILES), "{entry:?}");
        assert_eq!(entry.record.work_unit, None, "{entry:?}");
        assert_eq!(
            entity_of(entry).as_deref(),
            Some(r#"{"id":"naming","type":"Feature"}"#),
            "{entry:?}"
        );
    }
    let decided: Vec<&str> = entries
        .iter()
        .map(|entry| entry.record.principal.as_str())
        .collect();
    assert_eq!(decided, ["ana", "ana", "bruno", "bruno", "carla", "carla"]);
}

/// A frozen account, a device and an agent have no name proposed.
///
/// Dora's account is suspended, and the floor refuses her whatever a permit
/// says; ana's laptop is attested and holds a grant, and no permit lets a
/// device name a file; and ana's assistant is refused as every agent the
/// kernel is asked for is, until Stage 15 marks a dispatch as an agent's. Each
/// refusal says only the code, and the journal names what decided it.
#[tokio::test]
async fn a_frozen_account_a_device_and_an_agent_have_no_name_proposed() {
    let (kernel, world) = naming(None).await;
    let args = r#"{"title":"Praia"}"#;
    for (kind, id, decided) in [
        (
            PrincipalKind::User,
            "dora",
            "family.a_frozen_account_acts_on_nothing",
        ),
        (
            PrincipalKind::Device,
            "ana-laptop",
            nc_authz::sentinel::NO_PERMIT_MATCHED,
        ),
        (
            PrincipalKind::Agent,
            "ana-assistant",
            "core.an_agent_never_impersonates",
        ),
    ] {
        let caller = Caller::new(world.principal(kind, id).expect("in the world"));
        let refused = kernel.dispatch(&caller, PROPOSE, args).await;
        assert!(
            is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
            "{id}: {refused:?}"
        );
        let entries = kernel.journal_for_tests().await.expect("the journal reads");
        let entry = entries.last().expect("the denial was journaled");
        assert_eq!(
            (entry.record.principal.as_str(), entry.record.result),
            (id, Outcome::Denied)
        );
        assert_eq!(deciding(entry), Some(decided), "{id}");
        assert_eq!(
            entity_of(entry).as_deref(),
            Some(r#"{"id":"naming","type":"Feature"}"#),
            "{id}"
        );
    }
}

/// A proposal without an identifier mints a fresh one, and a rename keeps the
/// identifier it was given.
///
/// **The identity of a file is its identifier** (`NOCTUA.md §16.3`): two
/// proposals of one title are two files, each under an identifier the
/// machine's one minter made; and a file renamed — a new title, the
/// identifier read off its old name — keeps it, in its canonical spelling
/// however it was written, so the renamed file reads back as the same one.
#[tokio::test]
async fn a_rename_keeps_the_identifier_it_was_given() {
    let (kernel, world) = naming(None).await;
    let carla = Caller::new(
        world
            .principal(PrincipalKind::User, "carla")
            .expect("a person"),
    );
    let first = kernel
        .dispatch(&carla, PROPOSE, r#"{"title":"Old title"}"#)
        .await
        .expect("a name is proposed");
    let second = kernel
        .dispatch(&carla, PROPOSE, r#"{"title":"Old title"}"#)
        .await
        .expect("a name is proposed");
    assert_ne!(
        text(&first, "id"),
        text(&second, "id"),
        "two proposals were given one identifier"
    );
    let id = text(&first, "id");
    let kept = Value::object([
        ("title", Value::text("New title")),
        ("id", Value::text(id.to_lowercase())),
    ])
    .to_canonical();
    let renamed = kernel
        .dispatch(&carla, PROPOSE, &kept)
        .await
        .expect("a rename is proposed");
    assert_eq!(text(&renamed, "id"), id, "the rename has another identity");
    assert_eq!(
        text(&renamed, "name"),
        format!("--new-title@@{id}"),
        "{renamed}"
    );
    let read = kernel
        .dispatch(
            &carla,
            READ_NAME,
            &Value::object([("name", Value::text(text(&renamed, "name")))]).to_canonical(),
        )
        .await
        .expect("the renamed file's name reads back");
    assert_eq!(text(&read, "id"), id);
    let unread = kernel
        .dispatch(&carla, PROPOSE, r#"{"id":"U","title":"t"}"#)
        .await;
    assert!(
        is(&unread, RefusalKind::Invalid, ErrorCode::IdsNotAnIdentifier),
        "{unread:?}"
    );
}

/// A plugin proposes a name from inside a read, as the tree's person.
///
/// **Every surface that dispatches reaches the same commands**, a plugin
/// that saves a file among them: nested beneath a read, since neither
/// changes anything, decided as the person the tree runs for, and journaled
/// as the kernel's own command.
#[tokio::test]
async fn a_plugin_proposes_a_name_from_inside_a_read_as_the_trees_person() {
    let (fixture, _) = reference().await;
    let relayed = fixture
        .as_(
            "carla",
            "reference.relay.read",
            &relay(PROPOSE, r#"{"extension":"md","title":"From inside"}"#, None),
        )
        .await
        .expect("carla reads the photo the house shares");
    assert_eq!(field(&relayed, "answered"), Value::Integer(1), "{relayed}");
    let last = text(&relayed, "last");
    assert_eq!(
        text(&last, "name"),
        format!("--from-inside@@{}.md", text(&last, "id")),
        "{last}"
    );
    let entries = fixture.entries().await;
    let nested = only(&entries, PROPOSE);
    assert_eq!(
        (nested.record.principal.as_str(), nested.record.result),
        ("carla", Outcome::Allowed)
    );
    assert_eq!(deciding(nested), Some(NAMES_FILES));
    assert_eq!(nested.record.work_unit, None, "a read's tree opened a unit");
}

/// A journal that mints nothing leaves a proposal unavailable, and a kept
/// identifier still answered.
///
/// **No identifier is never one made up**: the minter's ceiling refuses its
/// raise, so `naming.propose` without an identifier is refused as the journal
/// refused it — unavailable, in the identifiers' own code — while one keeping
/// an identifier mints nothing and is answered, and so is a read.
#[tokio::test]
async fn a_journal_that_mints_nothing_leaves_a_proposal_unavailable() {
    let (kernel, world) = naming(Some(Box::new(NothingDurable))).await;
    let ana = Caller::new(
        world
            .principal(PrincipalKind::User, "ana")
            .expect("a person"),
    );
    let refused = kernel.dispatch(&ana, PROPOSE, r#"{"title":"t"}"#).await;
    assert!(
        is(
            &refused,
            RefusalKind::Unavailable,
            ErrorCode::IdsCeilingNotRaised
        ),
        "{refused:?}"
    );
    let entries = kernel.journal_for_tests().await.expect("the journal reads");
    let entry = only(&entries, PROPOSE);
    assert_eq!(entry.record.result, Outcome::Failed, "{entry:?}");
    let kept = kernel
        .dispatch(&ana, PROPOSE, r#"{"id":"01J8XK2M9P51","title":"t"}"#)
        .await
        .expect("a kept identifier mints nothing");
    assert_eq!(text(&kept, "name"), "--t@@01J8XK2M9P51");
    kernel
        .dispatch(&ana, READ_NAME, r#"{"name":"--t@@01J8XK2M9P51"}"#)
        .await
        .expect("a read mints nothing");
}
