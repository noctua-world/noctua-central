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
    Directory, DirectoryError, EntityRef, LocalDirectory, MemoryDirectory, Pdp, Principal,
    PrincipalKind, RealmFacts,
};
use nc_canon::Value;
use nc_catalog::{ErrorCode, KERNEL};
use nc_ids::{MachineFlake, Minter, OpaqueId};
use nc_journal::{Entry, Outcome};
use nc_kernel::backend::StoreBackend;
use nc_kernel::logging::Captured;
use nc_kernel::manifest::PluginManifest;
use nc_kernel::{Caller, Environment, Kernel, KernelConfig, Refusal, RefusalKind, Session};
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
    /// It answers that the principal of this identifier holds no grant: their
    /// authority withdrawn, and every other question as the world says.
    Withholds(String),
    /// It answers every agent a session names with this delegation, whatever
    /// the session asked for.
    Delegates(nc_authz::Delegation),
}

/// Which of the questions a decision or a session asks a [`Flaky`] directory
/// answers as its mode says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Call {
    /// The realm's facts: its profile, its pin and its epoch.
    Realm,
    /// The grant a principal holds in it.
    Grant,
    /// The entities a decision reads.
    Slice,
    /// The agent a session names, and the person it acts for.
    Delegation,
    /// A change to a person's agents, rehearsed and made.
    Change,
}

/// A keeper that keeps nothing and refuses nothing: the agents a test makes
/// live as long as its kernel.
#[derive(Debug)]
struct Forgetting;

impl nc_authz::Keeper for Forgetting {
    fn keep(
        &self,
        _kept: &nc_authz::Kept,
    ) -> impl Future<Output = Result<(), nc_authz::Unkept>> + Send {
        std::future::ready(Ok(()))
    }
}

/// How many agents and grants a test's directory may hold: more than any test
/// makes.
const BOUNDS: nc_authz::Bounds = nc_authz::Bounds {
    agents: 16,
    grants: 64,
};

/// A directory over the world and the agents a test makes, which answers one
/// of its questions as a test says, and every other as the two do.
#[derive(Debug)]
struct Flaky {
    inner: Arc<LocalDirectory<Forgetting>>,
    mode: Arc<Mutex<Mode>>,
    call: Arc<Mutex<Call>>,
}

/// The agents a test makes in `realm` over `world`: the directory a [`Flaky`]
/// wraps and the test changes.
fn the_agents_over(world: MemoryDirectory, realm: OpaqueId) -> Arc<LocalDirectory<Forgetting>> {
    Arc::new(
        LocalDirectory::open(
            realm,
            world,
            nc_authz::Records::default(),
            Forgetting,
            BOUNDS,
        )
        .expect("a directory holding nothing opens"),
    )
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
        Mode::Answers
        | Mode::Repins { .. }
        | Mode::Epoch(_)
        | Mode::Withholds(_)
        | Mode::Delegates(_) => answer.await,
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
        resource: &EntityRef,
    ) -> impl Future<Output = Result<Option<nc_authz::GrantFacts>, DirectoryError>> + Send {
        let mode = self.mode_of(Call::Grant);
        let withheld = matches!(&mode, Mode::Withholds(id) if *id == principal.id);
        let answer = as_told(mode, self.inner.grant(realm, principal, resource));
        async move { if withheld { Ok(None) } else { answer.await } }
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

    fn delegation(
        &self,
        realm: &str,
        agent: &EntityRef,
    ) -> impl Future<Output = Result<Option<nc_authz::Delegation>, DirectoryError>> + Send {
        let mode = self.mode_of(Call::Delegation);
        let answer = self.inner.delegation(realm, agent);
        async move {
            match mode {
                Mode::Delegates(delegation) => Ok(Some(delegation)),
                other => as_told(other, answer).await,
            }
        }
    }

    fn agents(
        &self,
        realm: &str,
        operator: &EntityRef,
    ) -> impl Future<Output = Result<Vec<nc_authz::HeldAgent>, DirectoryError>> + Send {
        self.inner.agents(realm, operator)
    }

    fn rehearse(
        &self,
        realm: &str,
        change: &nc_authz::Change,
    ) -> impl Future<Output = Result<(), nc_authz::ChangeRefused>> + Send {
        let answer = self.inner.rehearse(realm, change);
        as_changed(self.mode_of(Call::Change), answer)
    }

    fn change(
        &self,
        realm: &str,
        change: &nc_authz::Change,
    ) -> impl Future<Output = Result<(), nc_authz::ChangeRefused>> + Send {
        let answer = self.inner.change(realm, change);
        as_changed(self.mode_of(Call::Change), answer)
    }
}

/// A change's `answer`, or the refusal or the silence `mode` puts in its place.
async fn as_changed(
    mode: Mode,
    answer: impl Future<Output = Result<(), nc_authz::ChangeRefused>>,
) -> Result<(), nc_authz::ChangeRefused> {
    as_told(mode, async { Ok(answer.await) })
        .await
        .map_err(nc_authz::ChangeRefused::Directory)?
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
    /// The agents the kernel's directory holds, which a test makes, grants
    /// and ends grants of as a person's commands would.
    agents: Arc<LocalDirectory<Forgetting>>,
    /// What mints an agent's identifier and each of its grants'.
    minter: Mutex<Minter<Manual>>,
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
    .described_by(nc_kernel::entries_for_tests("reference"))
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
    for (kind, id) in [
        ("User", "ana"),
        ("User", "bruno"),
        ("User", "carla"),
        ("User", "dora"),
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
    let mode = Arc::new(Mutex::new(Mode::Answers));
    let call = Arc::new(Mutex::new(Call::Realm));
    let agents = the_agents_over(directory, home());
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
            inner: Arc::clone(&agents),
            mode: Arc::clone(&mode),
            call: Arc::clone(&call),
        },
        clock: clock.clone(),
        log: Arc::clone(&log),
        transport: nc_kernel::net::Scripted::default(),
    })
    .await?;
    let minter = Mutex::new(Minter::new(
        MachineFlake::from_value(21).expect("a flake"),
        clock.clone(),
    ));
    Ok(Fixture {
        kernel,
        store,
        log,
        clock,
        mode,
        call,
        world,
        agents,
        minter,
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

    /// A new identifier, from the fixture's own minter.
    fn minted(&self) -> nc_ids::Snowflake {
        self.minter
            .lock()
            .expect("nothing panics holding it")
            .mint()
            .expect("an identifier is minted")
    }

    /// Makes `change` in the kernel's directory, as a person's command would.
    async fn changed(&self, change: &nc_authz::Change) {
        self.agents
            .change(home().as_str(), change)
            .await
            .unwrap_or_else(|refused| panic!("{change:?} was refused: {refused}"));
    }

    /// A new agent of the person called `person`: the entity a session names
    /// it by.
    async fn agent_of(&self, person: &str) -> EntityRef {
        self.agent_holding(person, 1_000).await
    }

    /// A new agent of the person called `person`, given a budget of `budget`:
    /// the entity a session names it by.
    async fn agent_holding(&self, person: &str, budget: u32) -> EntityRef {
        let agent = self.minted();
        self.changed(&nc_authz::Change::Make {
            agent,
            operator: self.principal(person),
            label: format!("{person}'s assistant"),
            budget,
        })
        .await;
        EntityRef {
            type_name: "Agent".to_owned(),
            id: agent.to_crockford(),
        }
    }

    /// Grants `agent`, made by `person`, the feature `feature` for `minutes`
    /// from now, answering the grant's identifier as the journal records it.
    async fn granted(
        &self,
        person: &str,
        agent: &EntityRef,
        feature: &str,
        minutes: u64,
    ) -> String {
        let grant = self.minted();
        let from_ms = nc_ids::Clock::now_ms(&self.clock);
        self.changed(&nc_authz::Change::Grant {
            grant,
            agent: agent.id.clone(),
            scope: EntityRef {
                type_name: "Feature".to_owned(),
                id: feature.to_owned(),
            },
            by: self.principal(person),
            reason: "what a test grants".to_owned(),
            from_ms,
            until_ms: from_ms + minutes * 60_000,
        })
        .await;
        grant.to_crockford()
    }

    /// Ends the grant `agent`, which the person called `person` made, holds
    /// on `feature`, now.
    async fn revoked(&self, person: &str, agent: &EntityRef, feature: &str) {
        self.changed(&nc_authz::Change::End {
            agent: agent.id.clone(),
            scope: Some(EntityRef {
                type_name: "Feature".to_owned(),
                id: feature.to_owned(),
            }),
            by: self.principal(person),
            at_ms: nc_ids::Clock::now_ms(&self.clock),
        })
        .await;
    }

    /// The session of the agent `agent` names, for the person called `person`.
    async fn session(&self, agent: &EntityRef, person: &str) -> Session {
        self.kernel
            .session(agent, &self.principal(person))
            .await
            .unwrap_or_else(|refused| panic!("{agent:?} acts for {person}: {refused}"))
    }

    /// The session of the world's own agent, `ana-assistant`, which holds the
    /// realm's grant, for `ana`.
    async fn the_assistant(&self) -> Session {
        self.session(&entity("Agent", "ana-assistant"), "ana").await
    }

    /// Dispatches `command` with `args` in `session`.
    async fn by(&self, session: &Session, command: &str, args: &str) -> Result<String, Refusal> {
        self.kernel.dispatch(session, command, args).await
    }
}

/// An entity of type `type_name` called `id`.
fn entity(type_name: &str, id: &str) -> EntityRef {
    EntityRef {
        type_name: type_name.to_owned(),
        id: id.to_owned(),
    }
}

/// The core and the family pack as they are on disk, with `extra` beside them:
/// a decision point the tree does not ship, built through the same compiler.
fn family_and(extra: &[(&str, &str)]) -> Pdp {
    compiled(&the_family_on_disk(), extra)
}

/// The core and the family pack as they are on disk, less one policy.
///
/// The policy is the one whose `@id` is `id`: a decision point the tree does
/// not ship, built through the same compiler.
fn family_less(id: &str) -> Pdp {
    let mut texts = the_family_on_disk();
    let opening = format!("@id(\"{id}\")");
    let holding: Vec<&mut (String, String)> = texts
        .iter_mut()
        .filter(|(_, text)| text.contains(&opening))
        .collect();
    assert_eq!(holding.len(), 1, "one file holds `{id}`");
    for (_, text) in holding {
        let at = text.find(&opening).expect("held");
        let ends = text[at..].find(';').expect("a policy ends") + at + 1;
        text.replace_range(at..ends, "");
    }
    compiled(&texts, &[])
}

/// Every policy file of the core and the family pack, by its path under the
/// repository's root, beside its text.
fn the_family_on_disk() -> Vec<(String, String)> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut texts: Vec<(String, String)> = Vec::new();
    for directory in ["policy/core", "policy/packs/family"] {
        let mut paths: Vec<std::path::PathBuf> = std::fs::read_dir(root.join(directory))
            .expect("a policy directory")
            .map(|entry| entry.expect("an entry").path())
            .filter(|path| path.extension().is_some_and(|kind| kind == "cedar"))
            .collect();
        paths.sort();
        for path in paths {
            let name = path.file_name().expect("a file").to_string_lossy();
            texts.push((
                format!("{directory}/{name}"),
                std::fs::read_to_string(&path).expect("a readable policy"),
            ));
        }
    }
    texts
}

/// The decision point `texts` and `extra` compile to, for the family profile.
fn compiled(texts: &[(String, String)], extra: &[(&str, &str)]) -> Pdp {
    let mut files: Vec<(&str, &str)> = texts
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    files.extend_from_slice(extra);
    Pdp::from_sources_for_tests(
        include_str!("../../../policy/schema.cedarschema"),
        &files,
        "family",
        include_str!("../../../policy/packs/family/pack.toml"),
    )
    .expect("the pack compiles with what the test adds")
}

/// A permit the family pack does not ship.
///
/// An agent writes, deletes and shares what its operator owns — the agent's
/// half of the commands a person's own content answers to, so that what
/// refuses an agent there is shown to be something else.
const AN_AGENT_WRITES: (&str, &str) = (
    "policy/packs/family/agents_for_tests.cedar",
    "@id(\"family.an_agent_writes_its_operators_notes\")\n\
     permit (principal is Agent, action in [Action::\"Write\", Action::\"Delete\", \
     Action::\"Share\"], resource is Content)\n\
     when { resource.owner == principal.operator };\n",
);

/// The reference plugin alone, over a journal in memory, deciding with the
/// pack and [`AN_AGENT_WRITES`].
async fn reference_an_agent_writes_in() -> Fixture<Failing<nc_journal::InMemory>> {
    let journal = Failing {
        inner: nc_journal::InMemory::default(),
        told: Arc::new(Mutex::new(Told::default())),
    };
    opened(
        journal,
        Ok(family_and(&[AN_AGENT_WRITES])),
        &[("noctua:reference", "reference")],
        "",
    )
    .await
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
    .expect("a manifest granting the ui")
    .described_by(nc_kernel::entries_for_tests("reference"));
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
        .expect("a manifest")
        .described_by(nc_kernel::entries_for_tests("reference"));
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
    .expect("a manifest granting the blobs")
    .described_by(nc_kernel::entries_for_tests("reference"));
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
    .expect("a manifest granting the blobs")
    .described_by(nc_kernel::entries_for_tests("reference"));
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
            .expect("a manifest the kernel reads")
            .described_by(nc_kernel::entries_for_tests("reference")),
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
    .expect("a manifest granting the blobs")
    .described_by(nc_kernel::entries_for_tests("reference"));
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
    .expect("a manifest granting units")
    .described_by(nc_kernel::entries_for_tests("reference"));
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
            inner: the_agents_over(
                directory,
                seen.realm_id(realm).expect("a realm of the world"),
            ),
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
/// whichever of the three questions a decision asks it failed at.
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
            inner: the_agents_over(directory, home()),
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
            .expect("a manifest")
            .described_by(nc_kernel::entries_for_tests("reference")),
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
    .expect("a manifest granting the blobs")
    .described_by(nc_kernel::entries_for_tests("reference"));
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
            inner: the_agents_over(directory, home()),
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
/// device name a file; and ana's assistant, named as a caller with no session
/// of its own, is decided as acting alone, which the core refuses — an agent
/// acts through its session. Each refusal says only the code, and the journal
/// names what decided it.
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

/// The command that finds commands.
const FIND: &str = "kernel.find";

/// The deciding `@id` of every search a person is allowed.
const FINDS_COMMANDS: &str = "family.a_member_finds_commands";

/// The first command `answer` found, its owner, and the words it was found by.
fn first_found(answer: &str) -> (String, String, Vec<(String, String, String)>) {
    let found = field(answer, "found")
        .into_array()
        .expect("`found` is a list")
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("nothing was found: {answer}"))
        .to_canonical();
    let words = field(&found, "words")
        .into_array()
        .expect("`words` is a list")
        .into_iter()
        .map(|word| {
            let word = word.to_canonical();
            (
                text(&word, "field"),
                text(&word, "locale"),
                text(&word, "text"),
            )
        })
        .collect();
    (text(&found, "name"), text(&found, "owner"), words)
}

/// Finding a command is decided on the registry feature.
///
/// **A person's permit, and the denials beside it** (`NOCTUA.md §20.6`): a
/// guardian, an adult and a child each find `kernel.find` by a Portuguese
/// synonym and its Portuguese display name, each decided on
/// `Feature::"registry"` by the permit that gives every person of the realm a
/// search, opening no unit. A frozen account, a device and an agent named with
/// no session of its own, which acts alone, are each refused with the code
/// alone, and the journal names what decided it and the feature it was asked
/// of.
#[tokio::test]
async fn finding_a_command_is_decided_on_the_registry_feature() {
    let (kernel, world) = naming(None).await;
    let args = r#"{"query":"procurar comando"}"#;
    let persons = ["ana", "bruno", "carla"];
    for person in persons {
        let caller = Caller::new(
            world
                .principal(PrincipalKind::User, person)
                .expect("a person"),
        );
        let answered = kernel
            .dispatch(&caller, FIND, args)
            .await
            .unwrap_or_else(|refused| panic!("{person} found nothing: {refused}"));
        let pt = "pt-BR".to_owned();
        assert_eq!(
            first_found(&answered),
            (
                FIND.to_owned(),
                "kernel".to_owned(),
                vec![
                    ("tag".to_owned(), pt.clone(), "procurar".to_owned()),
                    (
                        "display_name".to_owned(),
                        pt,
                        "Encontrar um comando".to_owned()
                    ),
                ]
            ),
            "{person}: {answered}"
        );
    }
    let entries = kernel.journal_for_tests().await.expect("the journal reads");
    assert_eq!(
        entries.len(),
        persons.len(),
        "a search opened a unit: {entries:#?}"
    );
    let registry = Some(r#"{"id":"registry","type":"Feature"}"#);
    for entry in &entries {
        assert_eq!(entry.record.result, Outcome::Allowed, "{entry:?}");
        assert_eq!(entry.record.action, "Read", "{entry:?}");
        assert_eq!(deciding(entry), Some(FINDS_COMMANDS), "{entry:?}");
        assert_eq!(entry.record.work_unit, None, "{entry:?}");
        assert_eq!(entity_of(entry).as_deref(), registry, "{entry:?}");
    }
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
        let refused = kernel.dispatch(&caller, FIND, args).await;
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
        assert_eq!(entity_of(entry).as_deref(), registry, "{id}");
    }
}

/// A plugin's command is found by the texts of its own entry.
///
/// **Every registered command, not the catalog's alone**: with the reference
/// plugin loaded, a Portuguese query finds its `note.get` by the tags its
/// own `plugins/reference/i18n/pt-BR.toml` gives it, and says the command is
/// a plugin's.
#[tokio::test]
async fn a_plugins_command_is_found_by_the_texts_of_its_own_entry() {
    let (fixture, _) = reference().await;
    let answered = fixture
        .as_("carla", FIND, r#"{"query":"ler nota"}"#)
        .await
        .expect("carla finds a command");
    let pt = "pt-BR".to_owned();
    assert_eq!(
        first_found(&answered),
        (
            "reference.note.get".to_owned(),
            "plugin".to_owned(),
            vec![
                ("tag".to_owned(), pt.clone(), "ler".to_owned()),
                ("tag".to_owned(), pt, "nota".to_owned()),
            ]
        ),
        "{answered}"
    );
    let entries = fixture.entries().await;
    assert_eq!(deciding(only(&entries, FIND)), Some(FINDS_COMMANDS));
}

/// The command that reads whether MCP is on.
const MCP_READ: &str = "settings.mcp.read";

/// The command that switches MCP on or off.
const MCP_SET: &str = "settings.mcp.set";

/// The deciding `@id` of every read and switch of MCP a person is allowed.
const SWITCHES_MCP: &str = "family.a_member_switches_mcp";

/// The MCP setting is read and switched by a person alone, decided on its
/// feature.
///
/// **A person's permit, a child's included, and the denials beside it**
/// (`NOCTUA.md §20.6`): a guardian, an adult and a child each switch MCP on,
/// read it on and switch it off, each decided on `Feature::"mcp"` by the
/// permit that gives every person of the realm the switch — the set in a unit
/// of its own, the read in none. A frozen account and a device are each
/// refused with the code alone, the journal naming what decided it, and
/// neither switches anything.
#[tokio::test]
async fn the_mcp_setting_is_read_and_switched_by_a_person_alone_decided_on_its_feature() {
    let (kernel, world) = naming(None).await;
    let mcp = || async { kernel.settings().await.expect("the settings are read").mcp };
    assert!(!mcp().await, "MCP is on before anybody switched it");
    let persons = ["ana", "bruno", "carla"];
    for person in persons {
        let caller = Caller::new(
            world
                .principal(PrincipalKind::User, person)
                .expect("a person"),
        );
        for (command, args, answer, on) in [
            (MCP_SET, r#"{"on":true}"#, r#"{"on":true}"#, true),
            (MCP_READ, "{}", r#"{"on":true}"#, true),
            (MCP_SET, r#"{"on":false}"#, r#"{"on":false}"#, false),
        ] {
            let answered = kernel
                .dispatch(&caller, command, args)
                .await
                .unwrap_or_else(|refused| panic!("{person}: {command} {args}: {refused}"));
            assert_eq!(answered, answer, "{person}: {command} {args}");
            assert_eq!(mcp().await, on, "{person}: {command} {args}");
        }
    }
    let entries = kernel.journal_for_tests().await.expect("the journal reads");
    let decided = dispatched(&entries);
    assert_eq!(decided.len(), persons.len() * 3, "{entries:#?}");
    let feature = Some(r#"{"id":"mcp","type":"Feature"}"#);
    for entry in &decided {
        assert_eq!(entry.record.result, Outcome::Allowed, "{entry:?}");
        assert_eq!(deciding(entry), Some(SWITCHES_MCP), "{entry:?}");
        assert_eq!(entity_of(entry).as_deref(), feature, "{entry:?}");
        match command_of(entry).as_deref() {
            Some(MCP_SET) => {
                assert_eq!(entry.record.action, "Write", "{entry:?}");
                assert!(entry.record.work_unit.is_some(), "a switch ran in no unit");
            }
            Some(MCP_READ) => {
                assert_eq!(entry.record.action, "Read", "{entry:?}");
                assert_eq!(entry.record.work_unit, None, "a read opened a unit");
            }
            other => panic!("{other:?} was dispatched: {entry:?}"),
        }
    }
    assert_eq!(
        brackets(&entries),
        [
            (nc_journal::unit::OPENS, Outcome::UnitBegun),
            (nc_journal::unit::CLOSES, Outcome::UnitCommitted)
        ]
        .repeat(persons.len() * 2),
        "each switch was a unit of its own, kept"
    );
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
    ] {
        let caller = Caller::new(world.principal(kind, id).expect("in the world"));
        for (command, args) in [(MCP_SET, r#"{"on":true}"#), (MCP_READ, "{}")] {
            let refused = kernel.dispatch(&caller, command, args).await;
            assert!(
                is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
                "{id}: {command}: {refused:?}"
            );
            let entries = kernel.journal_for_tests().await.expect("the journal reads");
            let entry = entries.last().expect("the denial was journaled");
            assert_eq!(
                (
                    entry.record.principal.as_str(),
                    entry.record.result,
                    command_of(entry).as_deref()
                ),
                (id, Outcome::Denied, Some(command))
            );
            assert_eq!(deciding(entry), Some(decided), "{id}: {command}");
            assert_eq!(entity_of(entry).as_deref(), feature, "{id}: {command}");
        }
        assert!(!mcp().await, "{id} switched MCP on");
    }
}

/// A plugin switches no MCP from inside a command.
///
/// **Only a person's own call switches what may act for them**: ana runs a
/// command of the reference plugin that writes and one that only reads, each
/// dispatching `settings.mcp.set` as her, and each nested switch is refused
/// with `[NC-KERN-0025]` — before it is decided, and before whether a read's
/// frame may change anything is asked — journaled with the kernel's own
/// sentinel and her as the principal, and logged. MCP stays off. The read of
/// the setting is no switch, and is answered from inside the read.
#[tokio::test]
async fn a_plugin_switches_no_mcp_from_inside_a_command() {
    let (fixture, _) = reference().await;
    for relay_command in ["reference.relay.write", "reference.relay.read"] {
        let answered = fixture
            .as_(
                "ana",
                relay_command,
                &relay(MCP_SET, r#"{"on":true}"#, None),
            )
            .await
            .unwrap_or_else(|refused| panic!("{relay_command} answers: {refused}"));
        let refused = field(&answered, "refused")
            .into_text()
            .unwrap_or_else(|| panic!("{relay_command}: the switch was not refused: {answered}"));
        assert!(
            refused.starts_with(&format!("denied: [{}] ", ErrorCode::KernelOnlyAtTheTop)),
            "{relay_command}: {refused}"
        );
    }
    assert!(
        !fixture
            .kernel
            .settings()
            .await
            .expect("the settings are read")
            .mcp,
        "a plugin switched MCP on"
    );
    let entries = fixture.entries().await;
    let switches: Vec<&Entry> = dispatched(&entries)
        .into_iter()
        .filter(|entry| command_of(entry).as_deref() == Some(MCP_SET))
        .collect();
    assert_eq!(switches.len(), 2, "{switches:#?}");
    for entry in switches {
        assert_eq!(
            (
                entry.record.principal.as_str(),
                entry.record.result,
                deciding(entry)
            ),
            ("ana", Outcome::Denied, Some("kernel.top_level_only")),
            "{entry:?}"
        );
    }
    assert_eq!(
        fixture.logged("kernel.dispatch_refused", "reason"),
        ["top_level_only", "top_level_only"]
    );
    let read = fixture
        .as_("ana", "reference.relay.read", &relay(MCP_READ, "{}", None))
        .await
        .expect("the relay answers");
    assert_eq!(field(&read, "answered"), Value::Integer(1), "{read}");
    assert_eq!(text(&read, "last"), r#"{"on":false}"#, "{read}");
    let entries = fixture.entries().await;
    let nested = only(&entries, MCP_READ);
    assert_eq!(
        (nested.record.principal.as_str(), nested.record.result),
        ("ana", Outcome::Allowed)
    );
    assert_eq!(deciding(nested), Some(SWITCHES_MCP));
}

/// Who an entry says acted, and on whose behalf, outermost first.
type Who<'a> = (&'a str, Vec<&'a str>);

/// Who `entry` says acted, and on whose behalf, outermost first.
fn who(entry: &Entry) -> Who<'_> {
    (
        entry.record.principal.as_str(),
        entry
            .record
            .actor_chain
            .iter()
            .map(OpaqueId::as_str)
            .collect(),
    )
}

/// The entries of `entries` that record a dispatch of `command`.
fn entries_of<'a>(entries: &'a [Entry], command: &str) -> Vec<&'a Entry> {
    dispatched(entries)
        .into_iter()
        .filter(|entry| command_of(entry).as_deref() == Some(command))
        .collect()
}

/// Holds that no entry of `entries` was refused for its arguments, so that
/// what a test reads of the others was decided on arguments its command takes.
fn none_malformed(entries: &[Entry]) {
    assert_eq!(
        decided_by(entries, "kernel.malformed_arguments"),
        0,
        "an entry was refused for its arguments: {entries:#?}"
    );
}

/// An agent is allowed only what it and its operator may, and the journal
/// names both.
///
/// **Two questions, one entry** (`NOCTUA.md §14.6`, `§14.9`): ana's agent,
/// granted the naming feature, proposes and reads a name. Each entry names the
/// agent as its principal and ana in its actor chain, records the agent's
/// permit and then ana's, joined by `;`, rests on the agent's own grant, and
/// opens no unit.
#[tokio::test]
async fn an_agent_is_allowed_only_what_it_and_its_operator_may_and_the_journal_names_both() {
    let (fixture, _) = reference().await;
    let agent = fixture.agent_of("ana").await;
    let grant = fixture.granted("ana", &agent, "naming", 60).await;
    let session = fixture.session(&agent, "ana").await;
    let proposed = fixture
        .by(&session, PROPOSE, r#"{"title":"Praia"}"#)
        .await
        .expect("the agent has a name proposed");
    fixture
        .by(
            &session,
            READ_NAME,
            &Value::object([("name", Value::text(text(&proposed, "name")))]).to_canonical(),
        )
        .await
        .expect("the agent has the name read");
    let entries = fixture.entries().await;
    none_malformed(&entries);
    for command in [PROPOSE, READ_NAME] {
        let entry = only(&entries, command);
        assert_eq!(entry.record.result, Outcome::Allowed, "{entry:?}");
        assert_eq!(who(entry), (agent.id.as_str(), vec!["ana"]), "{command}");
        assert_eq!(
            deciding(entry),
            Some("family.an_agent_names_files;family.a_member_names_files"),
            "{command}"
        );
        assert_eq!(
            entry.record.grant_used.as_ref().map(OpaqueId::as_str),
            Some(grant.as_str()),
            "the agent's grant, and no other"
        );
        assert_eq!(entry.record.work_unit, None, "{command}");
    }
}

/// An agent reaches only the features its person granted it.
///
/// **The agent's half needs a grant of its own**: ana's agent, granted naming
/// alone, finds no command — refused by `core.provenance` on its own half,
/// though ana finds one.
#[tokio::test]
async fn an_agent_reaches_only_the_features_its_person_granted_it() {
    let (fixture, _) = reference().await;
    let agent = fixture.agent_of("ana").await;
    fixture.granted("ana", &agent, "naming", 60).await;
    let session = fixture.session(&agent, "ana").await;
    let query = r#"{"query":"nota"}"#;
    fixture
        .as_("ana", FIND, query)
        .await
        .expect("ana finds a command");
    let refused = fixture.by(&session, FIND, query).await;
    assert!(
        is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
        "{refused:?}"
    );
    fixture
        .by(&session, PROPOSE, r#"{"title":"t"}"#)
        .await
        .expect("the control: the feature it was granted");
    let entries = fixture.entries().await;
    none_malformed(&entries);
    let found: Vec<(Outcome, Who, Option<&str>)> = entries_of(&entries, FIND)
        .into_iter()
        .map(|entry| (entry.record.result, who(entry), deciding(entry)))
        .collect();
    assert_eq!(
        found,
        [
            (Outcome::Allowed, ("ana", vec![]), Some(FINDS_COMMANDS)),
            (
                Outcome::Denied,
                (agent.id.as_str(), vec!["ana"]),
                Some("core.provenance")
            ),
        ]
    );
}

/// An agent is refused what its operator may do and it may not.
///
/// **Both directions of the intersection, here the agent's half**: ana, a
/// guardian, verifies the journal and steps through her history. Her agent,
/// granted naming alone, holds no grant on the realm and is refused each by
/// `core.provenance`; the world's own assistant, which holds the realm's grant,
/// is refused each by its permit — none names an agent on the realm — so the
/// refusal is the permit's and not only the grant's. A prune is refused to
/// either by the core's floors on its half.
#[tokio::test]
async fn an_agent_is_refused_what_its_operator_may_do_and_it_may_not() {
    let (fixture, _) = reference().await;
    let agent = fixture.agent_of("ana").await;
    fixture.granted("ana", &agent, "naming", 60).await;
    let runtime = fixture.session(&agent, "ana").await;
    let assistant = fixture.the_assistant().await;
    let asked = [
        ("journal.verify", "{}"),
        ("kernel.undo", "{}"),
        ("kernel.redo", "{}"),
    ];
    for (command, args) in asked {
        let answered = fixture.as_("ana", command, args).await;
        assert!(
            answered.is_ok()
                || answered
                    .as_ref()
                    .is_err_and(|refused| refused.code() != ErrorCode::PolicyDenied),
            "ana: {command}: {answered:?}"
        );
        for session in [&runtime, &assistant] {
            let refused = fixture.by(session, command, args).await;
            assert!(
                is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
                "{command}: {refused:?}"
            );
        }
    }
    for session in [&runtime, &assistant] {
        let refused = fixture.by(session, PRUNE, r#"{"dry_run":true}"#).await;
        assert!(
            is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
            "{refused:?}"
        );
    }
    let entries = fixture.entries().await;
    none_malformed(&entries);
    for (command, _) in asked {
        let decided: Vec<(Outcome, &str, Option<&str>)> = entries_of(&entries, command)
            .into_iter()
            .map(|entry| (entry.record.result, who(entry).0, deciding(entry)))
            .collect();
        let anas = if command == "journal.verify" {
            Outcome::Allowed
        } else {
            Outcome::Failed
        };
        assert_eq!(
            decided,
            [
                (anas, "ana", decided[0].2),
                (Outcome::Denied, agent.id.as_str(), Some("core.provenance")),
                (
                    Outcome::Denied,
                    "ana-assistant",
                    Some(nc_authz::sentinel::NO_PERMIT_MATCHED)
                ),
            ],
            "{command}"
        );
        assert!(
            decided[0]
                .2
                .is_some_and(|ids| !ids.starts_with("kernel.") && !ids.contains(';')),
            "ana was allowed it by a permit: {decided:?}"
        );
    }
    for entry in entries_of(&entries, PRUNE) {
        let floors = deciding(entry).expect("a denial names what decided it");
        assert!(
            !floors.contains(';')
                && floors.contains("core.destructive_needs_quorum")
                && floors.contains("core.irreversible_needs_a_human"),
            "refused on the agent's half by the core: {floors}"
        );
    }
}

/// An agent granted a feature no agent permit names is refused by the permit.
///
/// **A grant alone allows nothing**: ana grants her agent the `mcp` feature,
/// and the agent's switch is refused on its half with no permit matched —
/// resting on the grant it holds — and MCP stays off.
#[tokio::test]
async fn an_agent_granted_a_feature_no_agent_permit_names_is_refused_by_the_permit() {
    let (fixture, _) = reference().await;
    let agent = fixture.agent_of("ana").await;
    let grant = fixture.granted("ana", &agent, "mcp", 60).await;
    let session = fixture.session(&agent, "ana").await;
    let refused = fixture.by(&session, MCP_SET, r#"{"on":true}"#).await;
    assert!(
        is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
        "{refused:?}"
    );
    let entries = fixture.entries().await;
    none_malformed(&entries);
    let entry = only(&entries, MCP_SET);
    assert_eq!(who(entry), (agent.id.as_str(), vec!["ana"]));
    assert_eq!(deciding(entry), Some(nc_authz::sentinel::NO_PERMIT_MATCHED));
    assert_eq!(
        entry.record.grant_used.as_ref().map(OpaqueId::as_str),
        Some(grant.as_str())
    );
    assert!(
        !fixture
            .kernel
            .settings()
            .await
            .expect("the settings are read")
            .mcp,
        "an agent switched MCP on"
    );
}

/// A runtime agent granted every feature reads no shared content its pack lets
/// any principal read.
///
/// **The exact scope is what refuses it.** The household's read permit admits
/// any principal to `public` and `realm` content, an agent included; ana's
/// agent, granted every feature of the realm, still holds no grant for
/// `Content::"notice"`, because a grant answers for the feature it names and
/// nothing in it, and `core.provenance` refuses its half. The world's own
/// assistant, whose grant is the realm's, is allowed by that permit on both
/// halves — ana's allowed by her owner's permit as well, the notice being
/// hers: the control that shows the scope, and not the permit, refused.
#[tokio::test]
async fn a_runtime_agent_granted_every_feature_reads_no_shared_content_its_pack_lets_any_principal_read()
 {
    let (fixture, _) = reference().await;
    let agent = fixture.agent_of("ana").await;
    for feature in ["gallery", "naming", "registry", "mcp"] {
        fixture.granted("ana", &agent, feature, 60).await;
    }
    let runtime = fixture.session(&agent, "ana").await;
    let get = "reference.note.get";
    let refused = fixture.by(&runtime, get, &note("notice", None)).await;
    assert!(
        is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
        "{refused:?}"
    );
    let assistant = fixture.the_assistant().await;
    let answered = fixture.by(&assistant, get, &note("notice", None)).await;
    let entries = fixture.entries().await;
    none_malformed(&entries);
    let decided: Vec<(Outcome, Who, Option<&str>)> = entries_of(&entries, get)
        .into_iter()
        .map(|entry| (entry.record.result, who(entry), deciding(entry)))
        .collect();
    assert_eq!(
        decided,
        [
            (
                Outcome::Denied,
                (agent.id.as_str(), vec!["ana"]),
                Some("core.provenance")
            ),
            (
                if answered.is_ok() {
                    Outcome::Allowed
                } else {
                    Outcome::Failed
                },
                ("ana-assistant", vec!["ana"]),
                Some(
                    "family.a_member_reads_shared_content;\
             family.a_member_reads_shared_content,family.an_owner_governs_their_own_content"
                )
            ),
        ]
    );
}

/// An agent reaches nothing its operator cannot, even when granted.
///
/// **The operator's half meets the floor**: dora's account is suspended, and
/// her agent, granted naming by her, is refused on her half by
/// `family.a_frozen_account_acts_on_nothing`, though its own permit
/// allowed it.
#[tokio::test]
async fn an_agent_reaches_nothing_its_operator_cannot_even_when_granted() {
    let (fixture, _) = reference().await;
    let agent = fixture.agent_of("dora").await;
    fixture.granted("dora", &agent, "naming", 60).await;
    let session = fixture.session(&agent, "dora").await;
    let refused = fixture.by(&session, PROPOSE, r#"{"title":"t"}"#).await;
    assert!(
        is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
        "{refused:?}"
    );
    let entries = fixture.entries().await;
    none_malformed(&entries);
    let entry = only(&entries, PROPOSE);
    assert_eq!(who(entry), (agent.id.as_str(), vec!["dora"]));
    assert_eq!(
        deciding(entry),
        Some("family.an_agent_names_files;family.a_frozen_account_acts_on_nothing")
    );
}

/// An agent is refused what it may do and its operator may not.
///
/// **The other direction**: a pack with a permit letting any agent read
/// carla's private recipe, which ana may not read. Ana's assistant is allowed
/// on its own half and refused on hers, with no permit matched.
#[tokio::test]
async fn an_agent_is_refused_what_it_may_do_and_its_operator_may_not() {
    let reads_the_recipe = (
        "policy/packs/family/recipe_for_tests.cedar",
        "@id(\"family.an_agent_reads_the_recipe\")\n\
         permit (principal is Agent, action == Action::\"Read\", resource == \
         Content::\"recipe\");\n",
    );
    let journal = Failing {
        inner: nc_journal::InMemory::default(),
        told: Arc::new(Mutex::new(Told::default())),
    };
    let fixture = opened(
        journal,
        Ok(family_and(&[reads_the_recipe])),
        &[("noctua:reference", "reference")],
        "",
    )
    .await;
    let assistant = fixture.the_assistant().await;
    let refused = fixture
        .by(&assistant, "reference.note.get", &note("recipe", None))
        .await;
    assert!(
        is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
        "{refused:?}"
    );
    let entries = fixture.entries().await;
    none_malformed(&entries);
    let entry = only(&entries, "reference.note.get");
    assert_eq!(who(entry), ("ana-assistant", vec!["ana"]));
    assert_eq!(
        deciding(entry),
        Some("family.an_agent_reads_the_recipe;kernel.no_permit_matched")
    );
}

/// An agent loses a feature at the next dispatch once its grant is revoked.
///
/// **Decided again at every call, and nothing cached**: one session, a name
/// proposed, the grant ended between two calls, and the next refused on the
/// agent's half by `core.provenance`.
#[tokio::test]
async fn an_agent_loses_a_feature_at_the_next_dispatch_once_its_grant_is_revoked() {
    let (fixture, _) = reference().await;
    let agent = fixture.agent_of("ana").await;
    fixture.granted("ana", &agent, "naming", 60).await;
    let session = fixture.session(&agent, "ana").await;
    fixture
        .by(&session, PROPOSE, r#"{"title":"t"}"#)
        .await
        .expect("granted");
    fixture.revoked("ana", &agent, "naming").await;
    let refused = fixture.by(&session, PROPOSE, r#"{"title":"t"}"#).await;
    assert!(
        is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
        "{refused:?}"
    );
    let entries = fixture.entries().await;
    none_malformed(&entries);
    let decided: Vec<Option<&str>> = entries_of(&entries, PROPOSE)
        .into_iter()
        .map(deciding)
        .collect();
    assert_eq!(
        decided,
        [
            Some("family.an_agent_names_files;family.a_member_names_files"),
            Some("core.provenance"),
        ]
    );
}

/// An agent loses what its operator loses at the next dispatch.
///
/// **The operator's half is asked at every call**: ana's grant withheld by the
/// directory between two calls of her agent's session refuses the second on
/// her half, and once the directory answers it again, the third is allowed.
#[tokio::test]
async fn an_agent_loses_what_its_operator_loses_at_the_next_dispatch() {
    let (fixture, _) = reference().await;
    let agent = fixture.agent_of("ana").await;
    fixture.granted("ana", &agent, "naming", 60).await;
    let session = fixture.session(&agent, "ana").await;
    *fixture.call.lock().expect("nothing panics holding it") = Call::Grant;
    for mode in [
        Mode::Answers,
        Mode::Withholds("ana".to_owned()),
        Mode::Answers,
    ] {
        *fixture.mode.lock().expect("nothing panics holding it") = mode;
        let _decided = fixture.by(&session, PROPOSE, r#"{"title":"t"}"#).await;
    }
    let entries = fixture.entries().await;
    none_malformed(&entries);
    let decided: Vec<(Outcome, Option<&str>)> = entries_of(&entries, PROPOSE)
        .into_iter()
        .map(|entry| (entry.record.result, deciding(entry)))
        .collect();
    let both = Some("family.an_agent_names_files;family.a_member_names_files");
    assert_eq!(
        decided,
        [
            (Outcome::Allowed, both),
            (
                Outcome::Denied,
                Some("family.an_agent_names_files;core.provenance")
            ),
            (Outcome::Allowed, both),
        ]
    );
}

/// The moment ana's grant `g.current` ends: `2027-01-01T00:00:00Z`.
const ANAS_GRANT_ENDS_MS: u64 = 1_798_761_600_000;

/// An agent's rights end when its operator's grant ends, on the kernel's
/// clock.
///
/// **One moment for both halves, the kernel's**: a second before ana's grant
/// ends her agent has a name proposed; two seconds later it is refused on her
/// half by `core.grant_not_expired`, its own grant running still.
#[tokio::test]
async fn an_agents_rights_end_when_its_operators_grant_ends_on_the_kernels_clock() {
    let (fixture, _) = reference().await;
    fixture.clock.advance(ANAS_GRANT_ENDS_MS - 1_000 - NOW_MS);
    let agent = fixture.agent_of("ana").await;
    fixture.granted("ana", &agent, "naming", 60).await;
    let session = fixture.session(&agent, "ana").await;
    fixture
        .by(&session, PROPOSE, r#"{"title":"t"}"#)
        .await
        .expect("a second before ana's grant ends");
    fixture.clock.advance(2_000);
    let refused = fixture.by(&session, PROPOSE, r#"{"title":"t"}"#).await;
    assert!(
        is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
        "{refused:?}"
    );
    let entries = fixture.entries().await;
    none_malformed(&entries);
    let decided: Vec<Option<&str>> = entries_of(&entries, PROPOSE)
        .into_iter()
        .map(deciding)
        .collect();
    assert_eq!(
        decided,
        [
            Some("family.an_agent_names_files;family.a_member_names_files"),
            Some("family.an_agent_names_files;core.grant_not_expired"),
        ]
    );
}

/// An agent's rights end when its own grant ends, on the kernel's clock.
///
/// **The window its person gave it**: granted for a minute, the agent has a
/// name proposed, and a minute and a millisecond later it is refused on its
/// own half by `core.grant_not_expired`.
#[tokio::test]
async fn an_agents_rights_end_when_its_own_grant_ends_on_the_kernels_clock() {
    let (fixture, _) = reference().await;
    let agent = fixture.agent_of("ana").await;
    fixture.granted("ana", &agent, "naming", 1).await;
    let session = fixture.session(&agent, "ana").await;
    fixture
        .by(&session, PROPOSE, r#"{"title":"t"}"#)
        .await
        .expect("within its minute");
    fixture.clock.advance(60_001);
    let refused = fixture.by(&session, PROPOSE, r#"{"title":"t"}"#).await;
    assert!(
        is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
        "{refused:?}"
    );
    let entries = fixture.entries().await;
    none_malformed(&entries);
    let decided: Vec<Option<&str>> = entries_of(&entries, PROPOSE)
        .into_iter()
        .map(deciding)
        .collect();
    assert_eq!(
        decided,
        [
            Some("family.an_agent_names_files;family.a_member_names_files"),
            Some("core.grant_not_expired"),
        ]
    );
}

/// A nested dispatch in an agent's tree is decided as the agent and its
/// operator.
///
/// **The tree's acting, at every depth**: ana's assistant relays a proposal
/// from inside a read of the photo, which is hers; the read and the proposal
/// nested in it are each decided on both halves, and each entry names the
/// assistant and ana.
#[tokio::test]
async fn a_nested_dispatch_in_an_agents_tree_is_decided_as_the_agent_and_its_operator() {
    let (fixture, _) = reference().await;
    let assistant = fixture.the_assistant().await;
    let relayed = fixture
        .by(
            &assistant,
            "reference.relay.read",
            &relay(PROPOSE, r#"{"title":"From inside"}"#, None),
        )
        .await
        .expect("the assistant reads the photo the house shares");
    assert_eq!(field(&relayed, "answered"), Value::Integer(1), "{relayed}");
    let entries = fixture.entries().await;
    none_malformed(&entries);
    for (command, decided) in [
        (
            "reference.relay.read",
            "family.a_member_reads_shared_content;\
             family.a_member_reads_shared_content,family.an_owner_governs_their_own_content",
        ),
        (
            PROPOSE,
            "family.an_agent_names_files;family.a_member_names_files",
        ),
    ] {
        let entry = only(&entries, command);
        assert_eq!(entry.record.result, Outcome::Allowed, "{entry:?}");
        assert_eq!(who(entry), ("ana-assistant", vec!["ana"]), "{command}");
        assert_eq!(deciding(entry), Some(decided), "{command}");
    }
}

/// An agent named without a session is decided as acting alone, and refused.
///
/// **Unmarked is impersonation**: ana's assistant named as a caller, with no
/// session, is decided as itself — no chain, not acting for anyone — which
/// `core.an_agent_never_impersonates` refuses, and its entry carries no
/// chain.
#[tokio::test]
async fn an_agent_named_without_a_session_is_decided_as_acting_alone_and_refused() {
    let (fixture, _) = reference().await;
    let assistant = fixture
        .world
        .principal(PrincipalKind::Agent, "ana-assistant")
        .expect("an agent of the world");
    let refused = fixture
        .kernel
        .dispatch(&Caller::new(assistant), PROPOSE, r#"{"title":"t"}"#)
        .await;
    assert!(
        is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
        "{refused:?}"
    );
    let entries = fixture.entries().await;
    let entry = only(&entries, PROPOSE);
    assert_eq!(who(entry), ("ana-assistant", vec![]));
    assert_eq!(deciding(entry), Some("core.an_agent_never_impersonates"));
}

/// A session is opened only for an agent its person made.
///
/// **One answer for every agent that is not theirs**: a person named as an
/// agent, carla's agent asked for by ana, an identifier no agent was made
/// under, and the world's assistant asked for by bruno are each refused with
/// `[NC-AI-0001]` in one sentence; a directory that refuses or never answers
/// is `[NC-POL-0006]`; and opening a session journals nothing.
#[tokio::test]
async fn a_session_is_opened_only_for_an_agent_its_person_made() {
    let (fixture, _) = reference().await;
    let anas = fixture.agent_of("ana").await;
    let carlas = fixture.agent_of("carla").await;
    let opened = fixture.session(&anas, "ana").await;
    assert_eq!(opened.agent().entity(), anas);
    assert_eq!(opened.operator(), &fixture.principal("ana"));
    let never_made = entity("Agent", &fixture.minted().to_crockford());
    let mut sentences = Vec::new();
    for (agent, person) in [
        (entity("User", "ana"), "ana"),
        (carlas.clone(), "ana"),
        (anas.clone(), "carla"),
        (never_made, "ana"),
        (entity("Agent", "ana-assistant"), "bruno"),
    ] {
        let refused = fixture
            .kernel
            .session(&agent, &fixture.principal(person))
            .await
            .expect_err("no agent of theirs");
        assert_eq!(
            (refused.kind(), refused.code()),
            (RefusalKind::NotFound, ErrorCode::AiNoSuchAgent),
            "{agent:?} for {person}"
        );
        sentences.push(refused.sentence().to_owned());
    }
    sentences.dedup();
    assert_eq!(sentences.len(), 1, "{sentences:?}");
    tokio::time::pause();
    *fixture.call.lock().expect("nothing panics holding it") = Call::Delegation;
    for mode in [Mode::Fails, Mode::Hangs] {
        *fixture.mode.lock().expect("nothing panics holding it") = mode.clone();
        let refused = tokio::time::timeout(
            WAITED_ON_FOR_EVER,
            fixture.kernel.session(&anas, &fixture.principal("ana")),
        )
        .await
        .unwrap_or_else(|_| panic!("{mode:?}: the session waited on the directory"))
        .expect_err("no session without the directory");
        assert_eq!(
            (refused.kind(), refused.code()),
            (
                RefusalKind::Unavailable,
                ErrorCode::PolicyDirectoryUnavailable
            ),
            "{mode:?}"
        );
    }
    tokio::time::resume();
    assert_eq!(
        fixture.entries().await,
        [],
        "opening a session journals nothing"
    );
}

/// A session is opened only from an answer for the agent it asked for.
///
/// **The directory's answer is held to the question**: a directory that
/// answers ana's own agent with another agent of hers, or answers a session
/// asked under ana's own entity with ana herself, each with ana as the
/// operator, is refused with `[NC-AI-0001]` in the one sentence, as an agent
/// she did not make is; nothing is opened and nothing is journaled.
#[tokio::test]
async fn a_session_is_opened_only_from_an_answer_for_the_agent_it_asked_for() {
    let (fixture, _) = reference().await;
    let anas = fixture.agent_of("ana").await;
    let other = fixture.agent_of("ana").await;
    assert_ne!(anas, other, "two agents of ana's");
    let ana = fixture.principal("ana");
    let another_agent = fixture.session(&other, "ana").await.agent().clone();
    *fixture.call.lock().expect("nothing panics holding it") = Call::Delegation;
    for (asked, answered) in [(anas, another_agent), (ana.entity(), ana.clone())] {
        *fixture.mode.lock().expect("nothing panics holding it") =
            Mode::Delegates(nc_authz::Delegation {
                agent: answered.clone(),
                operator: ana.clone(),
            });
        let refused = fixture
            .kernel
            .session(&asked, &ana)
            .await
            .expect_err("an answer for something else opens nothing");
        assert_eq!(
            (refused.kind(), refused.code()),
            (RefusalKind::NotFound, ErrorCode::AiNoSuchAgent),
            "{asked:?} answered as {answered:?}"
        );
        assert_eq!(
            refused.sentence(),
            "no session was opened: no agent the asking person made is held under that \
             identifier",
            "{asked:?} answered as {answered:?}"
        );
    }
    assert_eq!(
        fixture.entries().await,
        [],
        "opening a session journals nothing"
    );
}

/// An actor chain of two is decided, and one of three is refused by the core.
///
/// **Through the one test door**: ana's agent acting through a second agent of
/// hers is decided on three links, each allowing; through a third, the agent's
/// own half is refused by `core.chain_depth_is_bounded`. Each entry records
/// the chain whole, outermost first.
#[tokio::test]
async fn an_actor_chain_of_two_is_decided_and_one_of_three_is_refused_by_the_core() {
    let (fixture, _) = reference().await;
    let [first, second, third] = [
        fixture.agent_of("ana").await,
        fixture.agent_of("ana").await,
        fixture.agent_of("ana").await,
    ];
    for agent in [&first, &second, &third] {
        fixture.granted("ana", agent, "naming", 60).await;
    }
    let session = fixture.session(&first, "ana").await;
    let second_agent = fixture.session(&second, "ana").await.agent().clone();
    let third_agent = fixture.session(&third, "ana").await.agent().clone();
    let two = session
        .through_for_tests(vec![second_agent.clone()])
        .expect("a chain of two");
    let three = session
        .through_for_tests(vec![second_agent, third_agent])
        .expect("a chain of three, which the type holds");
    fixture
        .by(&two, PROPOSE, r#"{"title":"t"}"#)
        .await
        .expect("two links are within the bound");
    let refused = fixture.by(&three, PROPOSE, r#"{"title":"t"}"#).await;
    assert!(
        is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
        "{refused:?}"
    );
    let entries = fixture.entries().await;
    none_malformed(&entries);
    let decided: Vec<(Who, Option<&str>)> = entries_of(&entries, PROPOSE)
        .into_iter()
        .map(|entry| (who(entry), deciding(entry)))
        .collect();
    assert_eq!(
        decided,
        [
            (
                (first.id.as_str(), vec!["ana", second.id.as_str()]),
                Some(
                    "family.an_agent_names_files;family.a_member_names_files;\
                     family.an_agent_names_files"
                )
            ),
            (
                (
                    first.id.as_str(),
                    vec!["ana", second.id.as_str(), third.id.as_str()]
                ),
                Some("core.chain_depth_is_bounded")
            ),
        ]
    );
}

/// An agent's irreversible command is refused, and nothing is done.
///
/// **Refused until a person can confirm it**: with a pack that lets ana's
/// assistant write what she owns, both halves allow the command, and the
/// kernel's precondition refuses it with `[NC-KERN-0006]`, recorded as
/// `kernel.irreversible_needs_confirmation`; the plugin
/// never ran.
#[tokio::test]
async fn an_agents_irreversible_command_is_refused_and_nothing_is_done() {
    let fixture = reference_an_agent_writes_in().await;
    let assistant = fixture.the_assistant().await;
    let refused = fixture
        .by(
            &assistant,
            "reference.irreversible.do",
            &note("photo", None),
        )
        .await;
    assert!(
        is(&refused, RefusalKind::Denied, ErrorCode::KernelUnconfirmed),
        "{refused:?}"
    );
    let entries = fixture.entries().await;
    none_malformed(&entries);
    let entry = only(&entries, "reference.irreversible.do");
    assert_eq!(who(entry), ("ana-assistant", vec!["ana"]));
    assert_eq!(
        deciding(entry),
        Some("kernel.irreversible_needs_confirmation")
    );
    assert_eq!(fixture.held("noctua:reference", "done/photo").await, None);
}

/// A confirmation issued to the operator is never redeemed by their agent, and
/// stays theirs.
///
/// **A session holds no token, and spends none**: ana holds a confirmation of
/// the very dispatch her assistant asks for; the assistant is refused, and ana
/// then redeems the confirmation and the command runs.
#[tokio::test]
async fn a_confirmation_issued_to_the_operator_is_never_redeemed_by_their_agent_and_stays_theirs() {
    let fixture = reference_an_agent_writes_in().await;
    let (command, args) = ("reference.irreversible.do", note("photo", None));
    let confirmation =
        fixture
            .kernel
            .confirmation_for_tests(&fixture.principal("ana"), command, &args);
    let assistant = fixture.the_assistant().await;
    let refused = fixture.by(&assistant, command, &args).await;
    assert!(
        is(&refused, RefusalKind::Denied, ErrorCode::KernelUnconfirmed),
        "{refused:?}"
    );
    fixture
        .kernel
        .dispatch(
            &Caller::new(fixture.principal("ana")).confirmed(confirmation),
            command,
            &args,
        )
        .await
        .expect("ana's confirmation is hers still");
    assert_eq!(
        fixture
            .held("noctua:reference", "done/photo")
            .await
            .as_deref(),
        Some("true")
    );
}

/// An approval issued to the operator is never redeemed by their agent.
///
/// **No quorum for an agent**: ana holds a quorum's approval of sharing her
/// encrypted diary; her assistant, which the pack lets share what she owns, is
/// refused on its half by the quorum's floor, and ana then shares it with the
/// approval.
#[tokio::test]
async fn an_approval_issued_to_the_operator_is_never_redeemed_by_their_agent() {
    let fixture = reference_an_agent_writes_in().await;
    let (command, args) = ("reference.note.share", note("diary", Some("a key")));
    let approval = fixture
        .kernel
        .approval_for_tests(&fixture.principal("ana"), command, &args);
    let assistant = fixture.the_assistant().await;
    let refused = fixture.by(&assistant, command, &args).await;
    assert!(
        is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
        "{refused:?}"
    );
    fixture
        .kernel
        .dispatch(
            &Caller::new(fixture.principal("ana")).approved(approval),
            command,
            &args,
        )
        .await
        .expect("ana's approval is hers still");
    let entries = fixture.entries().await;
    none_malformed(&entries);
    let decided: Vec<(&str, Option<&str>)> = entries_of(&entries, command)
        .into_iter()
        .map(|entry| (who(entry).0, deciding(entry)))
        .collect();
    assert_eq!(decided.len(), 2, "{decided:?}");
    assert_eq!(decided[0].0, "ana-assistant");
    assert!(
        decided[0].1.is_some_and(|floors| !floors.contains(';')
            && floors.contains("family.giving_out_encrypted_content_needs_quorum")),
        "refused on the agent's half by the quorum's floor: {decided:?}"
    );
    assert_eq!(decided[1].0, "ana");
}

/// An agent named without a session redeems nothing it presents.
///
/// **The core's refusal stands whatever it presents**: a confirmation and an
/// approval of the very dispatch, issued to the agent itself and presented by
/// the agent named as a caller, are each decided as the agent acting alone
/// and refused by the core, and nothing runs. That the core refuses an agent
/// acting alone whatever it holds is why this suite cannot tell a token
/// redeemed from one refused; that neither is redeemed is shown where the
/// kernel binds them, by `an_agent_acting_alone_redeems_nothing_even_what_was_issued_to_it`
/// and `a_confirmation_issued_to_an_agent_is_never_bound` in `dispatch.rs`.
#[tokio::test]
async fn an_agent_named_without_a_session_redeems_nothing_it_presents() {
    let fixture = reference_an_agent_writes_in().await;
    let assistant = fixture
        .world
        .principal(PrincipalKind::Agent, "ana-assistant")
        .expect("an agent of the world");
    let (command, args) = ("reference.irreversible.do", note("photo", None));
    let confirmed = Caller::new(assistant.clone()).confirmed(
        fixture
            .kernel
            .confirmation_for_tests(&assistant, command, &args),
    );
    let approved = Caller::new(assistant.clone()).approved(
        fixture
            .kernel
            .approval_for_tests(&assistant, command, &args),
    );
    for caller in [&confirmed, &approved] {
        let refused = fixture.kernel.dispatch(caller, command, &args).await;
        assert!(
            is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
            "{refused:?}"
        );
    }
    let entries = fixture.entries().await;
    for entry in entries_of(&entries, command) {
        assert_eq!(who(entry), ("ana-assistant", vec![]));
        assert_eq!(deciding(entry), Some("core.an_agent_never_impersonates"));
    }
    assert_eq!(fixture.held("noctua:reference", "done/photo").await, None);
}

/// An argument named as a confirmation is refused by the schema, and confirms
/// nothing.
///
/// **No field of a call is read as a token**: ana's agent proposes a name with
/// each word a confirmation could be smuggled under beside a title, and each
/// is refused for its arguments with `[NC-VAL-0002]` before anything is
/// decided, journaled as the agent's, for ana.
#[tokio::test]
async fn an_argument_named_as_a_confirmation_is_refused_by_the_schema_and_confirms_nothing() {
    let (fixture, _) = reference().await;
    let agent = fixture.agent_of("ana").await;
    fixture.granted("ana", &agent, "naming", 60).await;
    let session = fixture.session(&agent, "ana").await;
    let smuggled = [
        ("confirmed_by_human", Value::Bool(true)),
        ("confirmation", Value::Integer(1)),
        ("confirmed", Value::Bool(true)),
        ("approval", Value::Integer(1)),
        ("serial", Value::Integer(1)),
        ("_meta", Value::object([("confirmed", Value::Bool(true))])),
    ];
    for (key, value) in &smuggled {
        let args =
            Value::object([("title", Value::text("t")), (*key, value.clone())]).to_canonical();
        let refused = fixture.by(&session, PROPOSE, &args).await;
        assert!(
            is(
                &refused,
                RefusalKind::Invalid,
                ErrorCode::ValidationArgumentsInvalid
            ),
            "{key}: {refused:?}"
        );
    }
    let entries = fixture.entries().await;
    let refused = entries_of(&entries, PROPOSE);
    assert_eq!(refused.len(), smuggled.len(), "{entries:#?}");
    for entry in refused {
        assert_eq!(who(entry), (agent.id.as_str(), vec!["ana"]));
        assert_eq!(deciding(entry), Some("kernel.malformed_arguments"));
    }
}

/// An issued serial carried by an argument the schema accepts confirms
/// nothing.
///
/// **A number is not a token**: ana is issued a confirmation of the very
/// dispatch her assistant asks for; the assistant sends it once as asked, and
/// once with the confirmation's serial where the note's name goes. The first
/// is refused by the precondition, the second because that note is nothing
/// the realm holds; neither runs, and ana's confirmation is hers to redeem
/// still.
#[tokio::test]
async fn an_issued_serial_carried_by_an_argument_the_schema_accepts_confirms_nothing() {
    let fixture = reference_an_agent_writes_in().await;
    let command = "reference.irreversible.do";
    let args = note("photo", None);
    let confirmation =
        fixture
            .kernel
            .confirmation_for_tests(&fixture.principal("ana"), command, &args);
    let serial: String = format!("{confirmation:?}")
        .chars()
        .filter(char::is_ascii_digit)
        .collect();
    assert!(!serial.is_empty(), "{confirmation:?}");
    let assistant = fixture.the_assistant().await;
    let unconfirmed = fixture.by(&assistant, command, &args).await;
    assert!(
        is(
            &unconfirmed,
            RefusalKind::Denied,
            ErrorCode::KernelUnconfirmed
        ),
        "{unconfirmed:?}"
    );
    let carried = fixture.by(&assistant, command, &note(&serial, None)).await;
    assert!(
        is(&carried, RefusalKind::Denied, ErrorCode::PolicyDenied),
        "{carried:?}"
    );
    let entries = fixture.entries().await;
    none_malformed(&entries);
    let decided: Vec<Option<&str>> = entries_of(&entries, command)
        .into_iter()
        .map(deciding)
        .collect();
    assert_eq!(
        decided,
        [
            Some("kernel.irreversible_needs_confirmation"),
            Some(nc_authz::sentinel::NO_PERMIT_MATCHED),
        ]
    );
    assert_eq!(fixture.held("noctua:reference", "done/photo").await, None);
    fixture
        .kernel
        .dispatch(
            &Caller::new(fixture.principal("ana")).confirmed(confirmation),
            command,
            &args,
        )
        .await
        .expect("ana's confirmation is hers still");
}

/// A nested irreversible command in an agent's tree is refused.
///
/// **Never confirmed beneath the top**: ana's assistant relays the command
/// that cannot be undone from inside a change to the photo, which the pack
/// lets it make; the nested dispatch is refused with `[NC-KERN-0006]`, and its
/// entry names the assistant and ana.
#[tokio::test]
async fn a_nested_irreversible_command_in_an_agents_tree_is_refused() {
    let fixture = reference_an_agent_writes_in().await;
    let assistant = fixture.the_assistant().await;
    let relayed = fixture
        .by(
            &assistant,
            "reference.relay.write",
            &relay("reference.irreversible.do", &note("photo", None), None),
        )
        .await
        .expect("the relay answers");
    let refused = text(&relayed, "refused");
    assert!(
        refused.starts_with(&format!("denied: [{}] ", ErrorCode::KernelUnconfirmed)),
        "{refused}"
    );
    let entries = fixture.entries().await;
    none_malformed(&entries);
    let entry = only(&entries, "reference.irreversible.do");
    assert_eq!(who(entry), ("ana-assistant", vec!["ana"]));
    assert_eq!(
        deciding(entry),
        Some("kernel.irreversible_needs_confirmation")
    );
    assert_eq!(fixture.held("noctua:reference", "done/photo").await, None);
}

/// An agent's delete is refused by the core, without a human and a quorum.
///
/// **The core's two floors, on the agent's half**: with a pack that lets ana's
/// assistant delete what she owns, its delete of the photo is refused by
/// `core.destructive_needs_quorum` and `core.irreversible_needs_a_human`
/// together, and the photo is there still.
#[tokio::test]
async fn an_agents_delete_is_refused_by_the_core_without_a_human_and_a_quorum() {
    let fixture = reference_an_agent_writes_in().await;
    fixture
        .as_("ana", "reference.note.put", &note("photo", Some("kept")))
        .await
        .expect("ana writes her photo");
    let assistant = fixture.the_assistant().await;
    let refused = fixture
        .by(&assistant, "reference.note.delete", &note("photo", None))
        .await;
    assert!(
        is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
        "{refused:?}"
    );
    let entries = fixture.entries().await;
    none_malformed(&entries);
    let entry = only(&entries, "reference.note.delete");
    assert_eq!(who(entry), ("ana-assistant", vec!["ana"]));
    assert_eq!(
        deciding(entry),
        Some("core.destructive_needs_quorum,core.irreversible_needs_a_human")
    );
    assert!(photo_in(&fixture.store).await.is_some(), "the photo went");
}

/// What decides an agent's proposal: its own permit, then its person's.
const NAMED_FOR_BOTH: &str = "family.an_agent_names_files;family.a_member_names_files";

/// The names of `commands`, in their order.
fn names(commands: Vec<nc_kernel::registry::RegisteredCommand>) -> Vec<String> {
    commands.into_iter().map(|command| command.name).collect()
}

/// A session spent to its budget is refused by the core and journaled with
/// the rule.
///
/// **The budget is a permission** (`NOCTUA.md §14.6`, rule 5): ana's agent,
/// given a budget of two and granted naming, has two names proposed, and its
/// third call is refused on its own half by `core.budget_is_a_permission` —
/// told `[NC-POL-0003]` like any denial, journaled as the agent's for ana,
/// resting on the agent's grant.
#[tokio::test]
async fn a_session_spent_to_its_budget_is_refused_by_the_core_and_journaled_with_the_rule() {
    let (fixture, _) = reference().await;
    let agent = fixture.agent_holding("ana", 2).await;
    let grant = fixture.granted("ana", &agent, "naming", 60).await;
    let session = fixture.session(&agent, "ana").await;
    for _ in 0..2 {
        fixture
            .by(&session, PROPOSE, r#"{"title":"t"}"#)
            .await
            .expect("within the budget");
    }
    let refused = fixture.by(&session, PROPOSE, r#"{"title":"t"}"#).await;
    assert!(
        is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
        "{refused:?}"
    );
    assert_eq!(session.spent(), 3);
    let entries = fixture.entries().await;
    none_malformed(&entries);
    let decided: Vec<(Outcome, Who, Option<&str>)> = entries_of(&entries, PROPOSE)
        .into_iter()
        .map(|entry| (entry.record.result, who(entry), deciding(entry)))
        .collect();
    let theirs = (agent.id.as_str(), vec!["ana"]);
    assert_eq!(
        decided,
        [
            (Outcome::Allowed, theirs.clone(), Some(NAMED_FOR_BOTH)),
            (Outcome::Allowed, theirs.clone(), Some(NAMED_FOR_BOTH)),
            (Outcome::Denied, theirs, Some("core.budget_is_a_permission")),
        ]
    );
    assert_eq!(
        entries_of(&entries, PROPOSE)[2]
            .record
            .grant_used
            .as_ref()
            .map(OpaqueId::as_str),
        Some(grant.as_str())
    );
}

/// A session spends one for each call, and nothing for what the call
/// dispatches.
///
/// **Nested work is the call's**: ana's assistant relays three proposals from
/// inside one read of the photo, and its session has spent one; every nested
/// proposal was decided for both halves.
#[tokio::test]
async fn a_session_spends_one_for_each_call_and_nothing_for_what_the_call_dispatches() {
    let (fixture, _) = reference().await;
    let assistant = fixture.the_assistant().await;
    let relayed = fixture
        .by(
            &assistant,
            "reference.relay.read",
            &relay(PROPOSE, r#"{"title":"t"}"#, Some(3)),
        )
        .await
        .expect("the assistant reads the photo the house shares");
    assert_eq!(field(&relayed, "answered"), Value::Integer(3), "{relayed}");
    assert_eq!(assistant.spent(), 1, "one call, three nested in it");
    let entries = fixture.entries().await;
    none_malformed(&entries);
    let nested: Vec<(Outcome, Who, Option<&str>)> = entries_of(&entries, PROPOSE)
        .into_iter()
        .map(|entry| (entry.record.result, who(entry), deciding(entry)))
        .collect();
    assert_eq!(
        nested,
        vec![
            (
                Outcome::Allowed,
                ("ana-assistant", vec!["ana"]),
                Some(NAMED_FOR_BOTH)
            );
            3
        ]
    );
    fixture
        .by(&assistant, PROPOSE, r#"{"title":"t"}"#)
        .await
        .expect("a call of its own");
    assert_eq!(assistant.spent(), 2);
}

/// A refused call spends as an allowed one does.
///
/// **Whatever refused it**: ana's agent, given a budget of three, names a
/// command no one registered and then gives a proposal arguments its schema
/// refuses — each spending one — so its one proposal is allowed and the next
/// is refused by the budget rule.
#[tokio::test]
async fn a_refused_call_spends_as_an_allowed_one_does() {
    let (fixture, _) = reference().await;
    let agent = fixture.agent_holding("ana", 3).await;
    fixture.granted("ana", &agent, "naming", 60).await;
    let session = fixture.session(&agent, "ana").await;
    let unknown = fixture.by(&session, "naming.nothing", "{}").await;
    assert!(
        is(
            &unknown,
            RefusalKind::NotFound,
            ErrorCode::KernelUnknownCommand
        ),
        "{unknown:?}"
    );
    let malformed = fixture.by(&session, PROPOSE, r#"{"title":1}"#).await;
    assert!(
        is(
            &malformed,
            RefusalKind::Invalid,
            ErrorCode::ValidationArgumentsInvalid
        ),
        "{malformed:?}"
    );
    assert_eq!(session.spent(), 2, "each refusal spent one");
    fixture
        .by(&session, PROPOSE, r#"{"title":"t"}"#)
        .await
        .expect("the last unit of the budget");
    let refused = fixture.by(&session, PROPOSE, r#"{"title":"t"}"#).await;
    assert!(
        is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
        "{refused:?}"
    );
    let entries = fixture.entries().await;
    let last = entries_of(&entries, PROPOSE)
        .into_iter()
        .last()
        .expect("the proposal refused");
    assert_eq!(deciding(last), Some("core.budget_is_a_permission"));
}

/// Each session counts its own budget.
///
/// **A session is the unit of the budget**: ana's agent, given a budget of
/// one, spends it in one session, and a clone of that session is spent too;
/// a second session of the same agent has its own.
#[tokio::test]
async fn each_session_counts_its_own_budget() {
    let (fixture, _) = reference().await;
    let agent = fixture.agent_holding("ana", 1).await;
    fixture.granted("ana", &agent, "naming", 60).await;
    let first = fixture.session(&agent, "ana").await;
    let args = r#"{"title":"t"}"#;
    fixture
        .by(&first, PROPOSE, args)
        .await
        .expect("the first session's one call");
    for spent in [first.clone(), first.clone()] {
        let refused = fixture.by(&spent, PROPOSE, args).await;
        assert!(
            is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
            "a clone spends from the same count: {refused:?}"
        );
    }
    let second = fixture.session(&agent, "ana").await;
    fixture
        .by(&second, PROPOSE, args)
        .await
        .expect("a second session counts from nothing");
    let refused = fixture.by(&second, PROPOSE, args).await;
    assert!(
        is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
        "{refused:?}"
    );
    assert_eq!((first.spent(), second.spent()), (3, 2));
}

/// Without the budget rule nothing else refuses a spent agent.
///
/// **Not separate middleware** (`NOCTUA.md §14.6`, rule 5): the pack less
/// `core.budget_is_a_permission` lets ana's agent, given a budget of two,
/// have a third name proposed, decided by the two permits as the first two
/// were — so the core's rule is what refused it, and nothing before it.
#[tokio::test]
async fn without_the_budget_rule_nothing_else_refuses_a_spent_agent() {
    let pdp = family_less("core.budget_is_a_permission");
    let ids = pdp.ids();
    assert!(!ids.iter().any(|id| id == "core.budget_is_a_permission"));
    assert!(
        ids.iter()
            .any(|id| id == "core.an_agent_never_impersonates"),
        "the rest of the core is there: {ids:?}"
    );
    let journal = Failing {
        inner: nc_journal::InMemory::default(),
        told: Arc::new(Mutex::new(Told::default())),
    };
    let fixture = opened(journal, Ok(pdp), &[("noctua:reference", "reference")], "").await;
    let agent = fixture.agent_holding("ana", 2).await;
    fixture.granted("ana", &agent, "naming", 60).await;
    let session = fixture.session(&agent, "ana").await;
    for _ in 0..3 {
        fixture
            .by(&session, PROPOSE, r#"{"title":"t"}"#)
            .await
            .expect("nothing refuses the call past the budget");
    }
    assert_eq!(session.spent(), 3);
    let entries = fixture.entries().await;
    let decided: Vec<Option<&str>> = entries_of(&entries, PROPOSE)
        .into_iter()
        .map(deciding)
        .collect();
    assert_eq!(decided, [Some(NAMED_FOR_BOTH); 3]);
}

/// An exhausted session may dispatch nothing.
///
/// **The probe reads what the session spent**: ana's agent, given a budget of
/// one and granted naming and finding, may dispatch three commands; once its
/// one call is spent, it may dispatch none, as its next call is refused.
#[tokio::test]
async fn an_exhausted_session_may_dispatch_nothing() {
    let (fixture, _) = reference().await;
    let agent = fixture.agent_holding("ana", 1).await;
    for feature in ["naming", "registry"] {
        fixture.granted("ana", &agent, feature, 60).await;
    }
    let session = fixture.session(&agent, "ana").await;
    assert_eq!(
        names(fixture.kernel.dispatchable(&session).await),
        [FIND, PROPOSE, READ_NAME]
    );
    fixture
        .by(&session, PROPOSE, r#"{"title":"t"}"#)
        .await
        .expect("its one call");
    assert_eq!(
        names(fixture.kernel.dispatchable(&session).await),
        [] as [&str; 0]
    );
    let refused = fixture.by(&session, FIND, r#"{"query":""}"#).await;
    assert!(
        is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
        "{refused:?}"
    );
}

/// Every command whose resource is known without its arguments, beside the
/// simplest arguments its schema accepts.
///
/// **Held both ways to the registry** by
/// `a_session_may_dispatch_exactly_the_commands_decided_for_it_and_its_operator`:
/// a command registered with the realm's or a feature's selector and no row
/// here fails it, and so does a row no such command has.
const FITTING: &[(&str, &str)] = &[
    ("agents.create", r#"{"budget":1,"label":"a"}"#),
    (
        "agents.grant",
        r#"{"agent":"0000000000000","feature":"naming","minutes":1,"reason":"a"}"#,
    ),
    ("agents.list", "{}"),
    ("agents.revoke", r#"{"agent":"0000000000000"}"#),
    ("journal.verify", r#"{"from":0}"#),
    ("kernel.find", r#"{"query":"a"}"#),
    ("kernel.redo", "{}"),
    ("kernel.undo", "{}"),
    ("limits.prune", r#"{"dry_run":true}"#),
    ("naming.propose", r#"{"title":"a"}"#),
    ("naming.read", r#"{"name":"a"}"#),
    ("settings.mcp.read", "{}"),
    ("settings.mcp.set", r#"{"on":true}"#),
];

/// A session may dispatch exactly the commands decided for it and its
/// operator.
///
/// **Never vacuous**: with the reference plugin loaded, ana's agent, granted
/// naming and finding, dispatches every command of [`FITTING`] — each with
/// arguments its schema accepts, none refused for them — and a command is
/// what the probe listed exactly when its entry says the decision allowed it
/// (`allowed`, or `failed` once allowed). Every entry names ana as whom the
/// agent acts for; the commands that change state are each refused on the
/// agent's own half; and no command of the plugin, whose every resource is an
/// argument's, is listed.
#[tokio::test]
async fn a_session_may_dispatch_exactly_the_commands_decided_for_it_and_its_operator() {
    let (fixture, _) = reference().await;
    let fitting: std::collections::BTreeSet<&str> = FITTING.iter().map(|(name, _)| *name).collect();
    let registry = fixture.kernel.registry();
    let selected: std::collections::BTreeSet<&str> = registry
        .iter()
        .filter(|command| {
            command.resource_selector == "realm"
                || command.resource_selector.starts_with("feature:")
        })
        .map(|command| command.name.as_str())
        .collect();
    assert_eq!(fitting, selected, "`FITTING` is the registry's, both ways");
    assert!(
        registry
            .iter()
            .any(|command| command.name.starts_with("reference.")),
        "the reference plugin is loaded"
    );
    let agent = fixture.agent_of("ana").await;
    for feature in ["naming", "registry"] {
        fixture.granted("ana", &agent, feature, 60).await;
    }
    let session = fixture.session(&agent, "ana").await;
    let listed = names(fixture.kernel.dispatchable(&session).await);
    assert!(
        listed.iter().all(|name| fitting.contains(name.as_str())),
        "only a command whose resource is known without arguments: {listed:?}"
    );
    for (name, args) in FITTING {
        let _decided_and_journaled = fixture.by(&session, name, args).await;
    }
    let entries = fixture.entries().await;
    none_malformed(&entries);
    for (name, _) in FITTING {
        let entry = only(&entries, name);
        assert_eq!(who(entry), (agent.id.as_str(), vec!["ana"]), "{name}");
        let allowed = matches!(entry.record.result, Outcome::Allowed | Outcome::Failed);
        assert_eq!(
            listed.iter().any(|listed| listed == name),
            allowed,
            "{name}: {entry:?}"
        );
        let changes = registry
            .iter()
            .find(|command| command.name == *name)
            .is_some_and(|command| command.mutating);
        if changes {
            assert!(
                !allowed && deciding(entry).is_some_and(|decided| !decided.contains(';')),
                "{name} is refused on the agent's half: {entry:?}"
            );
        }
    }
    assert_eq!(listed, [FIND, PROPOSE, READ_NAME]);
}

/// A grant given adds its tools, and one revoked takes them away.
///
/// **What the person grants is what the agent may list**: ana's agent may
/// dispatch nothing until she grants it naming, then its two commands, then
/// finding beside them; once she ends the naming grant, finding alone.
#[tokio::test]
async fn a_grant_given_adds_its_tools_and_one_revoked_takes_them_away() {
    let (fixture, _) = reference().await;
    let agent = fixture.agent_of("ana").await;
    let session = fixture.session(&agent, "ana").await;
    let listed = || async { names(fixture.kernel.dispatchable(&session).await) };
    assert_eq!(listed().await, [] as [&str; 0]);
    fixture.granted("ana", &agent, "naming", 60).await;
    assert_eq!(listed().await, [PROPOSE, READ_NAME]);
    fixture.granted("ana", &agent, "registry", 60).await;
    assert_eq!(listed().await, [FIND, PROPOSE, READ_NAME]);
    fixture.revoked("ana", &agent, "naming").await;
    assert_eq!(listed().await, [FIND]);
    assert_eq!(session.spent(), 0, "listing spends nothing");
}

/// A permit added to the pack adds its command to the list.
///
/// **The list is the pack's decision, and nothing written beside it**: ana's
/// agent, granted finding and the `mcp` feature, may read MCP's switch only
/// with a pack that lets an agent read that feature — and switch it with
/// neither, the permit naming the read alone.
#[tokio::test]
async fn a_permit_added_to_the_pack_adds_its_command_to_the_list() {
    /// The permit the test adds: an agent reads whether MCP is on.
    const AN_AGENT_READS_MCP: (&str, &str) = (
        "policy/packs/family/agents_for_tests.cedar",
        "@id(\"family.an_agent_reads_whether_mcp_is_on\")\n\
         permit (principal is Agent, action == Action::\"Read\", resource == Feature::\"mcp\");\n",
    );
    for (pdp, expected) in [
        (family(), vec![FIND]),
        (family_and(&[AN_AGENT_READS_MCP]), vec![FIND, MCP_READ]),
    ] {
        let journal = Failing {
            inner: nc_journal::InMemory::default(),
            told: Arc::new(Mutex::new(Told::default())),
        };
        let fixture = opened(journal, Ok(pdp), &[("noctua:reference", "reference")], "").await;
        let agent = fixture.agent_of("ana").await;
        for feature in ["registry", "mcp"] {
            fixture.granted("ana", &agent, feature, 60).await;
        }
        let session = fixture.session(&agent, "ana").await;
        assert_eq!(names(fixture.kernel.dispatchable(&session).await), expected);
    }
}

/// An agent's search finds exactly what its session may dispatch.
///
/// **One rule, read twice**: ana's agent, granted naming and finding, searches
/// with an empty query, which matches every command it is handed, and finds
/// what the probe lists, in its order, and counts them; ana's own search finds
/// every registered command.
#[tokio::test]
async fn an_agents_search_finds_exactly_what_its_session_may_dispatch() {
    let (fixture, _) = reference().await;
    let agent = fixture.agent_of("ana").await;
    for feature in ["naming", "registry"] {
        fixture.granted("ana", &agent, feature, 60).await;
    }
    let session = fixture.session(&agent, "ana").await;
    let probed = names(fixture.kernel.dispatchable(&session).await);
    let everything = r#"{"limit":32,"query":""}"#;
    let found_by = |answer: &str| -> (Vec<String>, Value) {
        let found = field(answer, "found")
            .into_array()
            .unwrap_or_else(|| panic!("a list: {answer}"))
            .into_iter()
            .map(|command| {
                command
                    .into_object()
                    .and_then(|mut fields| fields.remove("name"))
                    .and_then(Value::into_text)
                    .expect("each command found is named")
            })
            .collect();
        (found, field(answer, "total"))
    };
    let theirs = fixture
        .by(&session, FIND, everything)
        .await
        .expect("the agent finds commands");
    assert_eq!(
        found_by(&theirs),
        (
            probed.clone(),
            Value::Integer(i64::try_from(probed.len()).expect("a count"))
        )
    );
    let anas = fixture
        .as_("ana", FIND, everything)
        .await
        .expect("ana finds commands");
    assert_eq!(
        found_by(&anas).1,
        Value::Integer(i64::try_from(fixture.kernel.registry().len()).expect("a count")),
        "a person finds every registered command"
    );
}

/// A probe is never journaled.
///
/// **A probe decides nothing that happened**: listing what ana's agent and
/// what ana may dispatch leaves the journal as it was, and the agent's search,
/// cut to its list, journals its own one entry and nothing for the cut.
#[tokio::test]
async fn a_probe_is_never_journaled() {
    let (fixture, _) = reference().await;
    let agent = fixture.agent_of("ana").await;
    for feature in ["naming", "registry"] {
        fixture.granted("ana", &agent, feature, 60).await;
    }
    let session = fixture.session(&agent, "ana").await;
    let before = fixture.entries().await;
    assert_eq!(
        names(fixture.kernel.dispatchable(&session).await),
        [FIND, PROPOSE, READ_NAME]
    );
    let ana = Caller::new(fixture.principal("ana"));
    let anas = names(fixture.kernel.dispatchable(&ana).await);
    assert!(anas.iter().any(|name| name == "kernel.undo"), "{anas:?}");
    assert_eq!(fixture.entries().await, before);
    fixture
        .by(&session, FIND, r#"{"query":""}"#)
        .await
        .expect("the agent finds commands");
    let after = fixture.entries().await;
    assert_eq!(after.len(), before.len() + 1, "{after:#?}");
    assert_eq!(command_of(&after[before.len()]).as_deref(), Some(FIND));
}

/// A command a session was not listed is decided and journaled all the same.
///
/// **The list is advisory, and every call is decided**: ana's agent, granted
/// naming alone, is not listed `journal.verify` or `kernel.find`, and its
/// call of each is decided and refused on its half, journaled as the agent's
/// for ana.
#[tokio::test]
async fn a_command_a_session_was_not_listed_is_decided_and_journaled_all_the_same() {
    let (fixture, _) = reference().await;
    let agent = fixture.agent_of("ana").await;
    fixture.granted("ana", &agent, "naming", 60).await;
    let session = fixture.session(&agent, "ana").await;
    let listed = names(fixture.kernel.dispatchable(&session).await);
    assert_eq!(listed, [PROPOSE, READ_NAME]);
    for (name, args) in [
        ("journal.verify", r#"{"from":0}"#),
        (FIND, r#"{"query":"a"}"#),
    ] {
        let refused = fixture.by(&session, name, args).await;
        assert!(
            is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
            "{name}: {refused:?}"
        );
    }
    assert_eq!(session.spent(), 2, "each spent one");
    let entries = fixture.entries().await;
    none_malformed(&entries);
    for name in ["journal.verify", FIND] {
        let entry = only(&entries, name);
        assert_eq!(who(entry), (agent.id.as_str(), vec!["ana"]), "{name}");
        assert_eq!(deciding(entry), Some("core.provenance"), "{name}");
    }
}

/// Every refusal of an agent's call is journaled with its operator.
///
/// **Whatever refused it, and wherever**: with a pack that lets ana's
/// assistant write what she owns, nine relays deep, an agent's call is refused
/// for a malformed name, an unknown command, malformed arguments, a tree too
/// deep, no permit, a frozen operator, a spent budget and a command that
/// cannot be undone — and each entry names the agent's person in its actor
/// chain. The chain over them verifies, as ana asks it.
#[tokio::test]
async fn every_refusal_of_an_agents_call_is_journaled_with_its_operator() {
    let depth = usize::try_from(KERNEL.dispatch_depth).expect("fits");
    let ids: Vec<(String, String)> = (0..=depth)
        .map(|at| (format!("noctua:depth-{at}"), format!("depth{at}")))
        .collect();
    let plugins: Vec<(&str, &str)> = ids
        .iter()
        .map(|(id, namespace)| (id.as_str(), namespace.as_str()))
        .collect();
    let journal = Failing {
        inner: nc_journal::InMemory::default(),
        told: Arc::new(Mutex::new(Told::default())),
    };
    let fixture = opened(journal, Ok(family_and(&[AN_AGENT_WRITES])), &plugins, "").await;
    let assistant = fixture.the_assistant().await;
    let (mut deep, mut deep_args) = ("journal.verify".to_owned(), "{}".to_owned());
    for (_, namespace) in ids.iter().rev() {
        deep_args = relay(&deep, &deep_args, None);
        deep = format!("{namespace}.relay.read");
    }
    let doras = fixture.agent_of("dora").await;
    fixture.granted("dora", &doras, "naming", 60).await;
    let frozen = fixture.session(&doras, "dora").await;
    let anas = fixture.agent_holding("ana", 1).await;
    fixture.granted("ana", &anas, "naming", 60).await;
    let spending = fixture.session(&anas, "ana").await;
    let the_photo = note("photo", None);
    let refusals: [(&Session, &str, &str, &str); 9] = [
        (&assistant, "Not A Name", "{}", "kernel.malformed_name"),
        (&assistant, "naming.nothing", "{}", "kernel.unknown_command"),
        (
            &assistant,
            PROPOSE,
            r#"{"title":1}"#,
            "kernel.malformed_arguments",
        ),
        (
            &assistant,
            deep.as_str(),
            deep_args.as_str(),
            "kernel.too_deep",
        ),
        (
            &assistant,
            "journal.verify",
            "{}",
            "kernel.no_permit_matched",
        ),
        (
            &frozen,
            PROPOSE,
            r#"{"title":"t"}"#,
            "family.an_agent_names_files;family.a_frozen_account_acts_on_nothing",
        ),
        (&spending, PROPOSE, r#"{"title":"t"}"#, NAMED_FOR_BOTH),
        (
            &spending,
            PROPOSE,
            r#"{"title":"t"}"#,
            "core.budget_is_a_permission",
        ),
        (
            &assistant,
            "depth0.irreversible.do",
            the_photo.as_str(),
            "kernel.irreversible_needs_confirmation",
        ),
    ];
    for (session, name, args, _) in refusals {
        let _decided_and_journaled = fixture.by(session, name, args).await;
    }
    let entries = fixture.entries().await;
    let operator_of =
        |principal: &str| -> &str { if principal == doras.id { "dora" } else { "ana" } };
    for (_, _, _, decided) in refusals {
        let entry = dispatched(&entries)
            .into_iter()
            .find(|entry| deciding(entry) == Some(decided))
            .unwrap_or_else(|| panic!("an entry decided by {decided}: {entries:#?}"));
        let (principal, chain) = who(entry);
        assert_eq!(chain, [operator_of(principal)], "{decided}: {entry:?}");
    }
    for entry in dispatched(&entries) {
        let (principal, chain) = who(entry);
        assert_eq!(chain, [operator_of(principal)], "{entry:?}");
    }
    let verified = fixture
        .as_("ana", "journal.verify", "{}")
        .await
        .expect("ana verifies the chain");
    assert_eq!(
        field(&verified, "verified"),
        Value::Integer(i64::try_from(entries.len()).expect("a count")),
        "{verified}"
    );
}

/// Probing what a session may dispatch spends no confirmation, no budget, and
/// writes nothing.
///
/// **Nothing is presented and nothing is decided that happened**: ana holds a
/// confirmation of a command that cannot be undone, and listing what she may
/// dispatch with it in hand redeems none of it — the probe leaves out what
/// needs one, as it leaves out the prune a quorum must approve — and her
/// assistant's probe spends nothing of its session; the journal is as it was,
/// and ana's confirmation is redeemed when she dispatches it.
#[tokio::test]
async fn probing_what_a_session_may_dispatch_spends_no_confirmation_no_budget_and_writes_nothing() {
    let fixture = reference_an_agent_writes_in().await;
    let ana = fixture.principal("ana");
    let (command, args) = ("reference.irreversible.do", note("photo", None));
    let confirmed = Caller::new(ana.clone())
        .confirmed(fixture.kernel.confirmation_for_tests(&ana, command, &args));
    let assistant = fixture.the_assistant().await;
    let before = fixture.entries().await;
    let anas = names(fixture.kernel.dispatchable(&confirmed).await);
    assert!(
        anas.iter().any(|name| name == "kernel.undo")
            && !anas.iter().any(|name| name == PRUNE || name == command),
        "{anas:?}"
    );
    assert_eq!(
        names(fixture.kernel.dispatchable(&assistant).await),
        [FIND, PROPOSE, READ_NAME]
    );
    assert_eq!(assistant.spent(), 0);
    assert_eq!(fixture.entries().await, before);
    fixture
        .kernel
        .dispatch(&confirmed, command, &args)
        .await
        .expect("the confirmation is still ana's to redeem");
}

/// The command that makes an agent.
const CREATE: &str = "agents.create";

/// The command that lists the asker's agents.
const LIST: &str = "agents.list";

/// The command that grants an agent a feature.
const GRANT: &str = "agents.grant";

/// The command that ends an agent's grants.
const REVOKE: &str = "agents.revoke";

/// The deciding `@id` of every agents' command a person who is not a guardian
/// is allowed.
const KEEPS_AGENTS: &str = "family.a_member_keeps_their_agents";

/// What carla calls the agent she makes, which no entry may hold.
const LABEL: &str = "carla's own assistant";

/// Why carla grants it a feature, which no entry may hold.
const REASON: &str = "to name the photos of the trip";

/// `agents.create`'s arguments: `label`, and a budget of `budget`.
fn making(label: &str, budget: i64) -> String {
    Value::object([
        ("budget", Value::Integer(budget)),
        ("label", Value::text(label)),
    ])
    .to_canonical()
}

/// `agents.grant`'s arguments: `agent` granted `feature` for `minutes`.
fn granting(agent: &str, feature: &str, minutes: i64) -> String {
    Value::object([
        ("agent", Value::text(agent)),
        ("feature", Value::text(feature)),
        ("minutes", Value::Integer(minutes)),
        ("reason", Value::text(REASON)),
    ])
    .to_canonical()
}

/// `agents.revoke`'s arguments: `agent`'s grant on `feature`, or every one.
fn revoking(agent: &str, feature: Option<&str>) -> String {
    let mut fields = vec![("agent", Value::text(agent))];
    if let Some(feature) = feature {
        fields.push(("feature", Value::text(feature)));
    }
    Value::object(fields).to_canonical()
}

/// The entity every agents' command is decided on, as an entry records it.
const ON_THE_AGENTS: &str = r#"{"id":"agents","type":"Feature"}"#;

/// The agents `answer`, an `agents.list` answer, holds: each by its
/// identifier, beside the features of its live grants.
fn listed_in(answer: &str) -> Vec<(String, Vec<String>)> {
    field(answer, "agents")
        .into_array()
        .expect("a list")
        .into_iter()
        .map(|one| {
            let one = one.to_canonical();
            let grants = field(&one, "grants")
                .into_array()
                .expect("a list")
                .into_iter()
                .map(|grant| text(&grant.to_canonical(), "feature"))
                .collect();
            (text(&one, "agent"), grants)
        })
        .collect()
}

/// A person makes an agent, grants it a feature and ends the grant, each
/// journaled with what it made.
///
/// **Through the decision point, as carla's own calls**: each command decided
/// on `Feature::"agents"` by the permit that gives every person their agents,
/// each change in a unit of its own, kept. The agent is answered by the
/// identifier the minter gave it, a grant by its own with its window, a second
/// grant on the feature ends the first, and an ending answers what it ended —
/// the feature named, leaving another live; every one; nothing, once nothing
/// is live. Each entry's resource names what the change
/// made: the window as integers, the grant it ends, the grants ended; and **no
/// entry holds the label or the reason**. What carla grants is what her agent
/// may then do — its session proposes a name — and what she ends it may not.
/// Her undo stops at the step that changed her agents, and a frozen account
/// makes none.
#[tokio::test]
async fn a_person_makes_an_agent_grants_it_a_feature_and_ends_the_grant_each_journaled_with_what_it_made()
 {
    let (fixture, _) = reference().await;
    let made = fixture
        .as_("carla", CREATE, &making(LABEL, 10))
        .await
        .expect("carla makes an agent");
    let agent = text(&made, "agent");
    assert_eq!(agent.len(), nc_ids::base32::MIN_WIDTH, "{made}");
    assert_eq!(
        made,
        Value::object([
            ("agent", Value::text(agent.as_str())),
            ("budget", Value::Integer(10)),
            ("label", Value::text(LABEL)),
        ])
        .to_canonical()
    );
    let list = || async {
        listed_in(
            &fixture
                .as_("carla", LIST, "{}")
                .await
                .expect("carla lists her agents"),
        )
    };
    assert_eq!(list().await, [(agent.clone(), vec![])]);
    let first = fixture
        .as_("carla", GRANT, &granting(&agent, "naming", 60))
        .await
        .expect("carla grants it naming");
    let hour = NOW_MS + 3_600_000;
    let first_grant = text(&first, "grant");
    assert_eq!(first, a_grant(&agent, "naming", &first_grant, hour));
    let session = fixture.session(&entity("Agent", &agent), "carla").await;
    fixture
        .by(&session, PROPOSE, r#"{"title":"t"}"#)
        .await
        .expect("what carla granted her agent does");
    let second = fixture
        .as_("carla", GRANT, &granting(&agent, "naming", 1))
        .await
        .expect("carla grants it naming again");
    let second_grant = text(&second, "grant");
    assert_eq!(text(&second, "ends"), first_grant, "{second}");
    let finding = fixture
        .as_("carla", GRANT, &granting(&agent, "registry", 60))
        .await
        .expect("carla grants it finding");
    let finding_grant = text(&finding, "grant");
    assert_eq!(
        list().await,
        [(
            agent.clone(),
            vec!["naming".to_owned(), "registry".to_owned()]
        )]
    );
    let ended = fixture
        .as_("carla", REVOKE, &revoking(&agent, Some("naming")))
        .await
        .expect("carla ends the grant");
    assert_eq!(
        ended,
        format!(r#"{{"agent":"{agent}","ended":["{second_grant}"]}}"#)
    );
    let refused = fixture.by(&session, PROPOSE, r#"{"title":"t"}"#).await;
    assert!(
        is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
        "{refused:?}"
    );
    let every = fixture
        .as_("carla", REVOKE, &revoking(&agent, None))
        .await
        .expect("carla ends every grant");
    assert_eq!(
        field(&every, "ended"),
        Value::Array(vec![Value::text(finding_grant.as_str())]),
        "the grant on the other feature outlived the first ending"
    );
    let none = fixture
        .as_("carla", REVOKE, &revoking(&agent, None))
        .await
        .expect("an ending of nothing live is still answered");
    assert_eq!(field(&none, "ended"), Value::Array(Vec::new()));
    assert_eq!(list().await, [(agent.clone(), vec![])]);
    let undone = fixture.as_("carla", "kernel.undo", "{}").await;
    assert!(
        is(&undone, RefusalKind::Denied, ErrorCode::KernelUndoBarrier),
        "{undone:?}"
    );
    each_change_noted(
        &fixture.entries().await,
        &agent,
        [&first_grant, &second_grant, &finding_grant],
    );
    nobody_else_makes_one(&fixture).await;
    assert_eq!(
        list().await,
        [(agent, vec![])],
        "a refused call made an agent"
    );
}

/// What `agents.grant` answers for a grant that ends no other.
///
/// A grant of `feature` to `agent` under `grant`, given now and running
/// until `until`.
fn a_grant(agent: &str, feature: &str, grant: &str, until: u64) -> String {
    Value::object([
        ("agent", Value::text(agent)),
        ("feature", Value::text(feature)),
        ("grant", Value::text(grant)),
        ("valid_from", Value::text(nc_authz::datetime(NOW_MS))),
        ("valid_until", Value::text(nc_authz::datetime(until))),
    ])
    .to_canonical()
}

/// Holds that each of carla's calls on her agents was allowed by her permit,
/// a change in a unit and the list in none, and that each entry notes what
/// the change made and never what she wrote.
///
/// `agent` is the agent she made, and `grants` the three grants she gave it:
/// naming for an hour, naming again for a minute, and finding for an hour.
fn each_change_noted(entries: &[Entry], agent: &str, grants: [&str; 3]) {
    let [first_grant, second_grant, finding_grant] = grants;
    let hour = NOW_MS + 3_600_000;
    none_malformed(entries);
    let entity = format!(r#""entity":{ON_THE_AGENTS}"#);
    let resources: Vec<(String, String)> = dispatched(entries)
        .into_iter()
        .filter(|entry| {
            command_of(entry).is_some_and(|command| command.starts_with("agents."))
                && who(entry) == ("carla", vec![])
        })
        .map(|entry| {
            assert_eq!(entry.record.result, Outcome::Allowed, "{entry:?}");
            assert_eq!(deciding(entry), Some(KEEPS_AGENTS), "{entry:?}");
            let command = command_of(entry).expect("named");
            assert_eq!(
                entry.record.work_unit.is_some(),
                command != LIST,
                "{command} ran in a unit as only a change does"
            );
            (command, entry.record.resource.to_canonical())
        })
        .collect();
    let named = |command: &str, fields: &str| {
        (
            command.to_owned(),
            format!(r#"{{{fields}"command":"{command}",{entity}}}"#),
        )
    };
    let window = |feature: &str, until: u64| {
        format!(
            r#","scope":{{"id":"{feature}","type":"Feature"}},"valid_from_ms":{NOW_MS},"valid_until_ms":{until}"#
        )
    };
    let listing = (
        LIST.to_owned(),
        format!(r#"{{"command":"{LIST}",{entity}}}"#),
    );
    assert_eq!(
        resources,
        [
            named(CREATE, &format!(r#""agent":"{agent}","#)),
            listing.clone(),
            (
                GRANT.to_owned(),
                format!(
                    r#"{{"agent":"{agent}","command":"{GRANT}",{entity},"grant":"{first_grant}"{}}}"#,
                    window("naming", hour)
                )
            ),
            (
                GRANT.to_owned(),
                format!(
                    r#"{{"agent":"{agent}","command":"{GRANT}","ends":"{first_grant}",{entity},"grant":"{second_grant}"{}}}"#,
                    window("naming", NOW_MS + 60_000)
                )
            ),
            (
                GRANT.to_owned(),
                format!(
                    r#"{{"agent":"{agent}","command":"{GRANT}",{entity},"grant":"{finding_grant}"{}}}"#,
                    window("registry", hour)
                )
            ),
            listing.clone(),
            (
                REVOKE.to_owned(),
                format!(
                    r#"{{"agent":"{agent}","command":"{REVOKE}","ended":["{second_grant}"],{entity}}}"#
                )
            ),
            (
                REVOKE.to_owned(),
                format!(
                    r#"{{"agent":"{agent}","command":"{REVOKE}","ended":["{finding_grant}"],{entity}}}"#
                )
            ),
            (
                REVOKE.to_owned(),
                format!(r#"{{"agent":"{agent}","command":"{REVOKE}","ended":[],{entity}}}"#)
            ),
            listing,
        ]
    );
    for entry in entries {
        let written = format!("{:?}", entry.record);
        assert!(
            !written.contains(LABEL) && !written.contains(REASON),
            "an entry holds what carla wrote: {written}"
        );
    }
}

/// Holds that a frozen account makes no agent.
///
/// Refused by the pack's floor, which the journal names. Dora holds the realm's
/// grant here, so the floor and not a missing grant is what refuses her; a
/// device is refused by the permit's own `principal is User`, which the
/// pack's cases pin.
async fn nobody_else_makes_one<J: nc_journal::Store + Debug + Send + Sync + 'static>(
    fixture: &Fixture<J>,
) {
    let refused = fixture.as_("dora", CREATE, &making("theirs", 1)).await;
    assert!(
        is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
        "{refused:?}"
    );
    let entries = fixture.entries().await;
    let entry = entries.last().expect("the denial was journaled");
    assert_eq!(
        (entry.record.principal.as_str(), deciding(entry)),
        ("dora", Some("family.a_frozen_account_acts_on_nothing"))
    );
}

/// An agent makes, lists, grants and ends no agent.
///
/// **The permit refuses it, and not the grant**: ana grants her agent the
/// `agents` feature itself, so its own half holds a grant there, and each of
/// the four commands its session sends — making another agent, listing,
/// granting itself more, ending its own grants — is refused on that half
/// because no permit names an agent on the feature, journaled with the agent
/// and ana. What ana's agents hold is as it was.
#[tokio::test]
async fn an_agent_makes_lists_grants_and_ends_no_agent() {
    let (fixture, _) = reference().await;
    let agent = fixture.agent_of("ana").await;
    for feature in ["agents", "naming"] {
        fixture.granted("ana", &agent, feature, 60).await;
    }
    let before = fixture
        .as_("ana", LIST, "{}")
        .await
        .expect("ana lists her agents");
    assert_eq!(
        listed_in(&before),
        [(
            agent.id.clone(),
            vec!["agents".to_owned(), "naming".to_owned()]
        )]
    );
    let session = fixture.session(&agent, "ana").await;
    let asked = [
        (CREATE, making("another", 1)),
        (LIST, "{}".to_owned()),
        (GRANT, granting(&agent.id, "registry", 60)),
        (REVOKE, revoking(&agent.id, None)),
    ];
    for (command, args) in &asked {
        let refused = fixture.by(&session, command, args).await;
        assert!(
            is(&refused, RefusalKind::Denied, ErrorCode::PolicyDenied),
            "{command}: {refused:?}"
        );
    }
    let entries = fixture.entries().await;
    none_malformed(&entries);
    for (command, _) in &asked {
        let by_the_agent: Vec<&Entry> = entries_of(&entries, command)
            .into_iter()
            .filter(|entry| who(entry) == (agent.id.as_str(), vec!["ana"]))
            .collect();
        assert_eq!(by_the_agent.len(), 1, "{command}: {by_the_agent:#?}");
        assert_eq!(
            (by_the_agent[0].record.result, deciding(by_the_agent[0])),
            (Outcome::Denied, Some(nc_authz::sentinel::NO_PERMIT_MATCHED)),
            "{command}"
        );
    }
    assert_eq!(
        fixture
            .as_("ana", LIST, "{}")
            .await
            .expect("ana lists her agents"),
        before,
        "an agent changed its person's agents"
    );
}

/// A person grants and ends nothing of another person's agent.
///
/// **Only the agent's person delegates through it**: ana makes an agent and
/// grants it naming, and carla, whose own permit allows her the agents'
/// commands and who makes an agent of her own, names ana's to grant it a
/// feature, to grant it naming again, and to end its grant on naming and every
/// one. Each is refused with `[NC-AI-0002]` after the decision allowed it,
/// journaled as failed under the refusal's word with no agent noted, and
/// changes nothing: each person's list holds their own agent alone, ana's
/// with the grant she gave it.
#[tokio::test]
async fn a_person_grants_and_ends_nothing_of_another_persons_agent() {
    let (fixture, _) = reference().await;
    let theirs = fixture
        .as_("ana", CREATE, &making("ana's own", 5))
        .await
        .expect("ana makes an agent");
    let theirs = text(&theirs, "agent");
    fixture
        .as_("ana", GRANT, &granting(&theirs, "naming", 60))
        .await
        .expect("ana grants it naming");
    let before = fixture
        .as_("ana", LIST, "{}")
        .await
        .expect("ana lists her agents");
    assert_eq!(
        listed_in(&before),
        [(theirs.clone(), vec!["naming".to_owned()])]
    );
    let own = fixture
        .as_("carla", CREATE, &making(LABEL, 5))
        .await
        .expect("carla makes an agent");
    let own = text(&own, "agent");
    assert_ne!(own, theirs);
    let asked = [
        (GRANT, granting(&theirs, "registry", 60)),
        (GRANT, granting(&theirs, "naming", 1)),
        (REVOKE, revoking(&theirs, Some("naming"))),
        (REVOKE, revoking(&theirs, None)),
    ];
    for (command, args) in &asked {
        let refused = fixture.as_("carla", command, args).await;
        assert!(
            is(&refused, RefusalKind::Denied, ErrorCode::AiNotTheOperator),
            "{command} {args}: {refused:?}"
        );
    }
    let entries = fixture.entries().await;
    none_malformed(&entries);
    let carlas: Vec<(String, Outcome, Option<&str>, String)> = dispatched(&entries)
        .into_iter()
        .filter(|entry| {
            who(entry) == ("carla", vec![])
                && command_of(entry).is_some_and(|command| command == GRANT || command == REVOKE)
        })
        .map(|entry| {
            (
                command_of(entry).expect("named"),
                entry.record.result,
                deciding(entry),
                entry.record.resource.to_canonical(),
            )
        })
        .collect();
    let refusal = |command: &str| {
        (
            command.to_owned(),
            Outcome::Failed,
            Some(KEEPS_AGENTS),
            format!(
                r#"{{"command":"{command}","entity":{ON_THE_AGENTS},"failure":"not-the-operator"}}"#
            ),
        )
    };
    assert_eq!(
        carlas,
        asked
            .iter()
            .map(|(command, _)| refusal(command))
            .collect::<Vec<_>>(),
        "a refused change notes nothing it would have made"
    );
    assert_eq!(
        listed_in(
            &fixture
                .as_("carla", LIST, "{}")
                .await
                .expect("carla lists her agents")
        ),
        [(own, vec![])],
        "carla's list holds another person's agent, or a grant she was refused"
    );
    assert_eq!(
        fixture
            .as_("ana", LIST, "{}")
            .await
            .expect("ana lists her agents"),
        before,
        "another person changed ana's agent"
    );
}

/// A plugin makes, grants and ends no agent from inside a command.
///
/// **Only a person's own call delegates**: ana runs a command of the
/// reference plugin that writes and one that only reads, each dispatching the
/// three commands that change her agents as her, and each nested change is
/// refused with `[NC-KERN-0025]` before it is decided, journaled under the
/// kernel's own sentinel with her as the principal, and logged. Her agents are
/// as they were. The list is no change, and is answered from inside the read.
#[tokio::test]
async fn a_plugin_makes_grants_and_ends_no_agent_from_inside_a_command() {
    let (fixture, _) = reference().await;
    let made = fixture
        .as_("ana", CREATE, &making("ana's own", 5))
        .await
        .expect("ana makes an agent herself");
    let agent = text(&made, "agent");
    let before = fixture
        .as_("ana", LIST, "{}")
        .await
        .expect("ana lists her agents");
    let nested = [
        (CREATE, making("a plugin's", 1)),
        (GRANT, granting(&agent, "naming", 60)),
        (REVOKE, revoking(&agent, None)),
    ];
    for relay_command in ["reference.relay.write", "reference.relay.read"] {
        for (command, args) in &nested {
            let answered = fixture
                .as_("ana", relay_command, &relay(command, args, None))
                .await
                .unwrap_or_else(|refused| panic!("{relay_command} answers: {refused}"));
            let refused = field(&answered, "refused").into_text().unwrap_or_else(|| {
                panic!("{relay_command}: {command} was not refused: {answered}")
            });
            assert!(
                refused.starts_with(&format!("denied: [{}] ", ErrorCode::KernelOnlyAtTheTop)),
                "{relay_command}: {command}: {refused}"
            );
        }
    }
    let entries = fixture.entries().await;
    for (command, _) in &nested {
        let refusals: Vec<(&str, Outcome, Option<&str>)> = entries_of(&entries, command)
            .into_iter()
            .filter(|entry| entry.record.result != Outcome::Allowed)
            .map(|entry| {
                (
                    entry.record.principal.as_str(),
                    entry.record.result,
                    deciding(entry),
                )
            })
            .collect();
        assert_eq!(
            refusals,
            [("ana", Outcome::Denied, Some("kernel.top_level_only")); 2],
            "{command}"
        );
    }
    assert_eq!(
        fixture.logged("kernel.dispatch_refused", "reason"),
        ["top_level_only"; 6]
    );
    let read = fixture
        .as_("ana", "reference.relay.read", &relay(LIST, "{}", None))
        .await
        .expect("the relay answers");
    assert_eq!(field(&read, "answered"), Value::Integer(1), "{read}");
    assert_eq!(text(&read, "last"), before, "{read}");
    assert_eq!(
        fixture
            .as_("ana", LIST, "{}")
            .await
            .expect("ana lists her agents"),
        before,
        "a plugin changed ana's agents"
    );
}

/// A change the journal does not take is never made.
///
/// **Journaled before it is made**: with the journal refusing the entry of a
/// change, the dispatch is withheld as unavailable and the directory is left
/// as it was — no agent made, no grant given, no grant ended — and once the
/// journal takes the entry, the same change is made.
#[tokio::test]
async fn a_change_the_journal_does_not_take_is_never_made() {
    let (fixture, told) = reference().await;
    let refusing = |refuses: Option<fn(&Entry) -> bool>| {
        told.lock().expect("nothing panics holding it").refuses = refuses;
    };
    let list = || async {
        listed_in(
            &fixture
                .as_("carla", LIST, "{}")
                .await
                .expect("carla lists her agents"),
        )
    };
    let withheld = |refused: &Result<String, Refusal>| {
        assert!(
            is(
                refused,
                RefusalKind::Unavailable,
                ErrorCode::JournalUnavailable
            ),
            "{refused:?}"
        );
    };
    refusing(Some(|entry: &Entry| {
        command_of(entry).as_deref() == Some(CREATE) && entry.record.result == Outcome::Allowed
    }));
    withheld(&fixture.as_("carla", CREATE, &making(LABEL, 1)).await);
    refusing(None);
    assert_eq!(
        list().await,
        [],
        "an agent the journal holds no entry of was made"
    );
    let made = fixture
        .as_("carla", CREATE, &making(LABEL, 1))
        .await
        .expect("made once the journal takes it");
    let agent = text(&made, "agent");
    refusing(Some(|entry: &Entry| {
        command_of(entry).as_deref() == Some(GRANT) && entry.record.result == Outcome::Allowed
    }));
    withheld(
        &fixture
            .as_("carla", GRANT, &granting(&agent, "naming", 60))
            .await,
    );
    refusing(None);
    assert_eq!(list().await, [(agent.clone(), vec![])]);
    fixture
        .as_("carla", GRANT, &granting(&agent, "naming", 60))
        .await
        .expect("granted once the journal takes it");
    refusing(Some(|entry: &Entry| {
        command_of(entry).as_deref() == Some(REVOKE) && entry.record.result == Outcome::Allowed
    }));
    withheld(&fixture.as_("carla", REVOKE, &revoking(&agent, None)).await);
    refusing(None);
    assert_eq!(
        list().await,
        [(agent, vec!["naming".to_owned()])],
        "a grant was ended though the journal holds no entry of its end"
    );
}

/// A change the directory cannot answer for is refused and changes nothing.
///
/// **Rehearsed before it is journaled**: a directory that cannot answer for
/// the change refuses it as unavailable with `[NC-POL-0006]`, journaled as
/// failed after the decision allowed it, and nothing is held; once the
/// directory answers, the same change is made.
#[tokio::test]
async fn a_change_the_directory_cannot_answer_for_is_refused_and_changes_nothing() {
    let (fixture, _) = reference().await;
    *fixture.call.lock().expect("nothing panics holding it") = Call::Change;
    *fixture.mode.lock().expect("nothing panics holding it") = Mode::Fails;
    let refused = fixture.as_("carla", CREATE, &making(LABEL, 1)).await;
    assert!(
        is(
            &refused,
            RefusalKind::Unavailable,
            ErrorCode::PolicyDirectoryUnavailable
        ),
        "{refused:?}"
    );
    let entries = fixture.entries().await;
    let entry = only(&entries, CREATE);
    assert_eq!(
        (entry.record.result, deciding(entry)),
        (Outcome::Failed, Some(KEEPS_AGENTS))
    );
    assert_eq!(
        entry.record.resource.to_canonical(),
        format!(
            r#"{{"command":"{CREATE}","entity":{ON_THE_AGENTS},"failure":"directory-unavailable"}}"#
        ),
        "a refused change notes nothing it would have made"
    );
    let listed = fixture
        .as_("carla", LIST, "{}")
        .await
        .expect("the list is no change");
    assert_eq!(listed_in(&listed), []);
    *fixture.mode.lock().expect("nothing panics holding it") = Mode::Answers;
    fixture
        .as_("carla", CREATE, &making(LABEL, 1))
        .await
        .expect("made once the directory answers");
}
