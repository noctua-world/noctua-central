//! Isolation: a plugin that traps, spins, burns its fuel, outgrows its memory,
//! waits on a host call that never answers or runs past its call tree's
//! deadline is killed and started again, and every other plugin goes on as it
//! was (`NOCTUA.md §11.3`, acceptance criterion 2).
//!
//! **Two copies of one component, side by side.** The reference plugin and its
//! peer — the same component loaded again under the namespace `peer`, which
//! its store's activation key tells it — each run in a store of their own. A
//! test lowers a bound of the reference plugin alone, through its manifest,
//! makes it cross that bound, and then reads both back: the kill's own code,
//! one crash and a new generation for the one, a fresh instance that has run
//! one command since, and for the other the same status, the same instance and
//! the same data; and a journal that still verifies.
//!
//! **The store and the blobs are held in memory, and a test can make either
//! hang.** A call to them that never answers is the host call the kernel holds
//! to a deadline, and the clock that deadline is measured on is paused in that
//! test, so twenty seconds pass the moment nothing else can run. A test can also hold every
//! read at a gate it opens one read at a time, which is how two releases, and
//! a release and a command, are made to overlap in the order it chooses. A call tree's deadline is
//! kept on the monotonic clock, which no test pauses, so a test that reaches
//! it waits its half a minute out.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nc_authz::{EntityRef, MemoryDirectory, Pdp, PrincipalKind};
use nc_canon::Value;
use nc_catalog::{ErrorCode, KERNEL};
use nc_ids::{MachineFlake, Minter, OpaqueId};
use nc_journal::{Entry, Outcome};
use nc_kernel::backend::{MemoryBackend, StoreBackend};
use nc_kernel::blobs::{BlobError, BlobStore, Finished, Hash, MemoryBlobs, Swept};
use nc_kernel::logging::Captured;
use nc_kernel::manifest::PluginManifest;
use nc_kernel::net::Scripted;
use nc_kernel::{
    Caller, Environment, Kernel, KernelConfig, Kill, PluginSlot, PluginStatus, Refusal, RefusalKind,
};
use nc_local::{Expect, LocalError, Quotas, StagedWrite, UnitKind, UnitMeta, UnitRecord};
use nc_plugin_reference::behaviour::{ACTIVATION, Activation, PEER, Variant};

/// The world every policy case is about.
const WORLD: &str = include_str!("../../../policy/world.json");

/// The instant the policy cases call now, `2026-08-26T00:00:00Z`: inside the
/// grant `g.current`.
const NOW_MS: u64 = 1_787_702_400_000;

/// A clock a test moves.
#[derive(Debug, Clone)]
struct Manual {
    now: Arc<AtomicU64>,
}

impl Manual {
    /// Moves the clock on by `by`.
    fn advance(&self, by: u64) {
        self.now.fetch_add(by, Ordering::SeqCst);
    }
}

impl nc_ids::Clock for Manual {
    fn now_ms(&self) -> u64 {
        self.now.load(Ordering::SeqCst)
    }

    fn wait_past(&self, millis: u64) {
        self.now
            .fetch_max(millis.saturating_add(1), Ordering::SeqCst);
    }
}

/// Which of a [`Hanging`] store's calls do not answer, or answer late.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hang {
    /// Every call answers at once.
    Nothing,
    /// Every read of a note from the one numbered so, counting from zero,
    /// never answers.
    NoteReadsFrom(u64),
    /// A listing never answers.
    Listings,
    /// A read of a note answers after this long.
    NoteReadsFor(Duration),
    /// A read of a plugin's activation answers after this long.
    ActivationReadsFor(Duration),
    /// A read of a note answers after the first while, and a read of the
    /// peer's activation after the second.
    NoteAndPeerActivationReadsFor(Duration, Duration),
    /// Every read waits for a permit of the store's gate, which the test
    /// hands out.
    Gated,
    /// Asking the store for a blob's row never answers.
    BlobRows,
    /// Reading a blob's bytes never answers.
    BlobReads,
    /// Beginning a writing never answers.
    BlobCreates,
    /// Writing to a writing never answers.
    BlobWrites,
    /// Finishing a writing never answers.
    BlobFinishes,
}

/// The store held in memory, whose calls a test can make hang.
#[derive(Debug, Clone)]
struct Hanging {
    inner: Arc<MemoryBackend>,
    hang: Arc<Mutex<Hang>>,
    /// How many reads of a note it has been asked for.
    note_reads: Arc<AtomicU64>,
    /// The permits a read takes while the store is [`Hang::Gated`].
    gate: Arc<tokio::sync::Semaphore>,
}

impl Hanging {
    /// What the store does now.
    fn hang(&self) -> Hang {
        *self.hang.lock().expect("nothing panics holding it")
    }

    /// Makes the store do as `hang` says, from the next call it is asked.
    fn set(&self, hang: Hang) {
        *self.hang.lock().expect("nothing panics holding it") = hang;
    }
}

impl StoreBackend for Hanging {
    fn get(
        &self,
        realm: &nc_ids::OpaqueId,
        plugin: &str,
        key: &str,
    ) -> impl Future<Output = Result<Option<nc_local::Entry>, LocalError>> + Send {
        let note = key
            .starts_with("note/")
            .then(|| self.note_reads.fetch_add(1, Ordering::SeqCst));
        let activation = key == ACTIVATION;
        let the_peers = plugin == THE_PEER;
        let hang = self.hang();
        let read = self.inner.get(realm, plugin, key);
        let gate = Arc::clone(&self.gate);
        async move {
            match (hang, note) {
                (Hang::NoteReadsFrom(first), Some(this)) if this >= first => {
                    core::future::pending::<()>().await;
                }
                (Hang::NoteReadsFor(late), Some(_)) => tokio::time::sleep(late).await,
                (Hang::ActivationReadsFor(late), _) if activation => {
                    tokio::time::sleep(late).await;
                }
                (Hang::NoteAndPeerActivationReadsFor(late, _), Some(_)) => {
                    tokio::time::sleep(late).await;
                }
                (Hang::NoteAndPeerActivationReadsFor(_, slow), None) if activation && the_peers => {
                    tokio::time::sleep(slow).await;
                }
                (Hang::Gated, _) => gate
                    .acquire()
                    .await
                    .expect("the gate is never closed")
                    .forget(),
                _ => {}
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
    ) -> impl Future<Output = Result<nc_local::Page, LocalError>> + Send {
        let hangs = self.hang() == Hang::Listings;
        let listed = self.inner.list(realm, plugin, prefix, limit, cursor);
        async move {
            if hangs {
                core::future::pending::<()>().await;
            }
            listed.await
        }
    }

    fn count(
        &self,
        realm: &nc_ids::OpaqueId,
        plugin: &str,
    ) -> impl Future<Output = Result<u64, LocalError>> + Send {
        self.inner.count(realm, plugin)
    }

    fn commit(
        &self,
        unit: &UnitRecord,
        quotas: &Quotas,
    ) -> impl Future<Output = Result<nc_local::Committed, nc_local::CommitError>> + Send {
        self.inner.commit(unit, quotas)
    }

    fn commit_held(
        &self,
        unit: &UnitRecord,
        quotas: &Quotas,
    ) -> impl Future<Output = Result<nc_local::Committed, nc_local::CommitError>> + Send {
        self.inner.commit_held(unit, quotas)
    }

    fn release(
        &self,
        holder: nc_ids::Snowflake,
    ) -> impl Future<Output = Result<u64, LocalError>> + Send {
        self.inner.release(holder)
    }

    fn holders(&self) -> impl Future<Output = Result<Vec<nc_ids::Snowflake>, LocalError>> + Send {
        self.inner.holders()
    }

    fn units(
        &self,
        realm: &OpaqueId,
        principal: &OpaqueId,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<nc_local::UnitRow>, LocalError>> + Send {
        self.inner.units(realm, principal, limit)
    }

    fn steps(
        &self,
        realm: &OpaqueId,
        principal: &OpaqueId,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<nc_local::UnitRow>, LocalError>> + Send {
        self.inner.steps(realm, principal, limit)
    }

    fn unit(
        &self,
        unit: nc_ids::Snowflake,
    ) -> impl Future<Output = Result<Option<nc_local::UnitDetail>, LocalError>> + Send {
        self.inner.unit(unit)
    }

    fn set_state(
        &self,
        unit: nc_ids::Snowflake,
        state: nc_local::UnitState,
    ) -> impl Future<Output = Result<bool, LocalError>> + Send {
        self.inner.set_state(unit, state)
    }

    fn forget_beyond(
        &self,
        realm: &OpaqueId,
        principal: &OpaqueId,
        keep: u32,
    ) -> impl Future<Output = Result<u64, LocalError>> + Send {
        self.inner.forget_beyond(realm, principal, keep)
    }

    fn noted_elsewhere(
        &self,
        note: &str,
        except: nc_ids::Snowflake,
    ) -> impl Future<Output = Result<bool, LocalError>> + Send {
        self.inner.noted_elsewhere(note, except)
    }

    fn blob(
        &self,
        hash: &str,
    ) -> impl Future<Output = Result<Option<nc_local::BlobRow>, LocalError>> + Send {
        let hangs = self.hang() == Hang::BlobRows;
        let row = self.inner.blob(hash);
        async move {
            if hangs {
                core::future::pending::<()>().await;
            }
            row.await
        }
    }

    fn set_pinned(
        &self,
        hash: &str,
        pinned: bool,
        at_ms: u64,
    ) -> impl Future<Output = Result<bool, LocalError>> + Send {
        self.inner.set_pinned(hash, pinned, at_ms)
    }

    fn unpinned_before(
        &self,
        before_ms: u64,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<String>, LocalError>> + Send {
        self.inner.unpinned_before(before_ms, limit)
    }

    fn forget_blob(&self, hash: &str) -> impl Future<Output = Result<bool, LocalError>> + Send {
        self.inner.forget_blob(hash)
    }

    fn pruner(&self) -> Option<nc_prune::sqlite::Sqlite> {
        self.inner.pruner()
    }
}

/// The blobs held in memory, whose calls a test can make hang as the
/// [`Hanging`] store beside it says.
#[derive(Debug)]
struct HangingBlobs {
    inner: Arc<MemoryBlobs>,
    hang: Arc<Mutex<Hang>>,
}

impl HangingBlobs {
    /// Never answers when the store's hang is `hangs`.
    async fn unless(hang: Hang, hangs: Hang) {
        if hang == hangs {
            core::future::pending::<()>().await;
        }
    }

    /// What the store does now.
    fn hang(&self) -> Hang {
        *self.hang.lock().expect("nothing panics holding it")
    }
}

impl BlobStore for HangingBlobs {
    type Writer = <Arc<MemoryBlobs> as BlobStore>::Writer;

    fn size(&self, hash: &Hash) -> impl Future<Output = Result<Option<u64>, BlobError>> + Send {
        self.inner.size(hash)
    }

    fn read(
        &self,
        hash: &Hash,
        offset: u64,
        max: usize,
    ) -> impl Future<Output = Result<Option<Vec<u8>>, BlobError>> + Send {
        let (hang, read) = (self.hang(), self.inner.read(hash, offset, max));
        async move {
            Self::unless(hang, Hang::BlobReads).await;
            read.await
        }
    }

    fn create(&self) -> impl Future<Output = Result<Self::Writer, BlobError>> + Send {
        let (hang, created) = (self.hang(), self.inner.create());
        async move {
            Self::unless(hang, Hang::BlobCreates).await;
            created.await
        }
    }

    fn write(
        &self,
        writer: Self::Writer,
        bytes: Vec<u8>,
    ) -> impl Future<Output = (Self::Writer, Result<(), BlobError>)> + Send {
        let (hang, written) = (self.hang(), self.inner.write(writer, bytes));
        async move {
            Self::unless(hang, Hang::BlobWrites).await;
            written.await
        }
    }

    fn hash_so_far(&self, writer: &Self::Writer) -> Hash {
        self.inner.hash_so_far(writer)
    }

    fn finish(
        &self,
        writer: Self::Writer,
    ) -> impl Future<Output = Result<Finished, BlobError>> + Send {
        let (hang, finished) = (self.hang(), self.inner.finish(writer));
        async move {
            Self::unless(hang, Hang::BlobFinishes).await;
            finished.await
        }
    }

    fn remove(&self, hash: &Hash) -> impl Future<Output = Result<bool, BlobError>> + Send {
        self.inner.remove(hash)
    }

    fn hashes(&self, named: Duration) -> impl Future<Output = Result<Vec<Hash>, BlobError>> + Send {
        self.inner.hashes(named)
    }

    fn forget_unfinished(&self) -> impl Future<Output = Result<u64, BlobError>> + Send {
        self.inner.forget_unfinished()
    }
}

/// A kernel over the [`Hanging`] store, the journal in memory, the [`Manual`]
/// clock and the world's directory.
#[derive(Debug)]
struct Env;

impl Environment for Env {
    type Store = Hanging;
    type Blobs = HangingBlobs;
    type Journal = nc_journal::InMemory;
    type Clock = Manual;
    type Directory = MemoryDirectory;
    type Log = Arc<Captured>;
    type Transport = Arc<Scripted>;
}

/// A kernel with the reference plugin and its peer loaded, and everything a
/// test reads beside it.
struct Fixture {
    kernel: Kernel<Env>,
    store: Hanging,
    /// The bytes the kernel's blobs hold, which a test ages.
    blobs: Arc<MemoryBlobs>,
    /// What the kernel sends a plugin's fetch with, which a test can make
    /// stall.
    transport: Arc<Scripted>,
    log: Arc<Captured>,
    clock: Manual,
    world: MemoryDirectory,
    reference: PluginSlot,
    peer: PluginSlot,
}

/// The identity the reference plugin is loaded under.
const REFERENCE: &str = "noctua:reference";

/// The identity its peer is loaded under.
const THE_PEER: &str = "noctua:reference-peer";

/// The identity the journal's chain is kept under.
fn keeper() -> OpaqueId {
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

/// Commits `value` as `plugin`'s `key` in `store`, as a unit nobody
/// dispatched.
async fn seeded(store: &impl StoreBackend, plugin: &str, key: &str, value: &str) {
    /// The flake the next seed is minted with: each seed's own.
    static FLAKES: AtomicU64 = AtomicU64::new(16);
    let world = MemoryDirectory::from_world(WORLD, &family()).expect("the world is valid");
    let flake = u16::try_from(FLAKES.fetch_add(1, Ordering::SeqCst) % 1_000).expect("fits");
    let mut minter = Minter::new(
        MachineFlake::from_value(flake).expect("a flake"),
        Manual {
            now: Arc::new(AtomicU64::new(NOW_MS)),
        },
    );
    let unit = UnitRecord {
        unit: minter.mint().expect("a unit"),
        meta: UnitMeta {
            realm: world.realm_id("home").expect("home is a realm"),
            principal: world
                .principal(PrincipalKind::User, "ana")
                .expect("ana is a user")
                .id()
                .clone(),
            kind: UnitKind::Step,
            label: "seeding the test".to_owned(),
            committed_at_ms: NOW_MS,
        },
        writes: vec![StagedWrite {
            plugin: plugin.to_owned(),
            key: key.to_owned(),
            expect: Expect::Anything,
            after: Some(value.to_owned()),
        }],
        blobs: Vec::new(),
        notes: Vec::new(),
        unpinned: Vec::new(),
        marks: Vec::new(),
    };
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

/// The reference plugin's manifest as `id` under `namespace`, granted `also`
/// besides what every plugin here is, with `limits`.
///
/// It is granted what the plugin's commands reach for — the units it begins
/// among it — and `example.com` is the one name it fetches from.
fn manifest(id: &str, namespace: &str, also: &[&str], limits: &str) -> PluginManifest {
    let also = also
        .iter()
        .map(|capability| format!(", \"{capability}\""))
        .collect::<Vec<_>>()
        .concat();
    PluginManifest::parse(&format!(
        "id = \"{id}\"\nnamespace = \"{namespace}\"\ncapabilities = [\"store\", \
         \"blobs.read\", \"blobs.write\", \"commands.dispatch\", \"commands.begin\", \
         \"net\"{also}]\n[net]\ndomains = [\"example.com\"]\n{limits}"
    ))
    .expect("a manifest the kernel reads")
}

/// A kernel with the reference plugin loaded under a manifest whose `[limits]`
/// are `limits`, and its peer under the catalog's.
async fn opened(limits: &str) -> Fixture {
    opened_hearing(limits, &[]).await
}

/// The same, the peer granted `events.publish` and `events.subscribe` as well
/// and subscribed to each topic pattern of `heard`, when it names any.
async fn opened_hearing(limits: &str, heard: &[&str]) -> Fixture {
    let pack = family();
    let world = MemoryDirectory::from_world(WORLD, &pack).expect("the world is valid");
    let directory = MemoryDirectory::from_world(WORLD, &pack)
        .expect("the world is valid")
        .with_epoch("home", 7)
        .with_grant(
            "home",
            EntityRef {
                type_name: "User".to_owned(),
                id: "ana".to_owned(),
            },
            "g.current",
        )
        .expect("the realm holds the grant");
    let store = Hanging {
        inner: Arc::new(MemoryBackend::default()),
        hang: Arc::new(Mutex::new(Hang::Nothing)),
        note_reads: Arc::new(AtomicU64::new(0)),
        gate: Arc::new(tokio::sync::Semaphore::new(0)),
    };
    seeded(
        &store,
        THE_PEER,
        ACTIVATION,
        &Activation {
            namespace: PEER.to_owned(),
            variant: Variant::Normal,
            subscribe: heard.iter().map(|&topic| topic.to_owned()).collect(),
        }
        .written(),
    )
    .await;
    let (also, events): (&[&str], String) = if heard.is_empty() {
        (&["timers"], String::new())
    } else {
        let patterns: Vec<String> = heard.iter().map(|topic| format!("\"{topic}\"")).collect();
        (
            &["timers", "events.publish", "events.subscribe"],
            format!("[events]\nsubscribe = [{}]\n", patterns.join(", ")),
        )
    };
    let clock = Manual {
        now: Arc::new(AtomicU64::new(NOW_MS)),
    };
    let log = Arc::new(Captured::default());
    let transport = Arc::new(Scripted::default());
    let blobs = Arc::new(MemoryBlobs::default());
    let kernel = Kernel::open(KernelConfig {
        store: store.clone(),
        blobs: HangingBlobs {
            inner: Arc::clone(&blobs),
            hang: Arc::clone(&store.hang),
        },
        pdp: Ok(pack),
        journal: nc_journal::Journal::new(
            nc_journal::InMemory::default(),
            world.realm_id("home").expect("home is a realm"),
            keeper(),
            Minter::new(MachineFlake::from_value(3).expect("a flake"), clock.clone()),
        ),
        directory,
        clock: clock.clone(),
        log: Arc::clone(&log),
        transport: Arc::clone(&transport),
    })
    .await
    .expect("the kernel opens");
    let component = nc_kernel::component_for_tests("reference");
    let reference = kernel
        .load(
            manifest(REFERENCE, "reference", &[], &format!("[limits]\n{limits}")),
            &component,
        )
        .await
        .unwrap_or_else(|refused| panic!("the reference plugin loads: {refused}"));
    let peer = kernel
        .load(manifest(THE_PEER, PEER, also, &events), &component)
        .await
        .unwrap_or_else(|refused| panic!("the peer loads: {refused}"));
    Fixture {
        kernel,
        store,
        blobs,
        transport,
        log,
        clock,
        world,
        reference,
        peer,
    }
}

impl Fixture {
    /// Dispatches `command` with `args` as `ana`, the guardian who owns the
    /// note `photo`.
    async fn as_ana(&self, command: &str, args: &str) -> Result<String, Refusal> {
        let ana = self
            .world
            .principal(PrincipalKind::User, "ana")
            .expect("ana is a user of the world");
        self.kernel.dispatch(&Caller::new(ana), command, args).await
    }

    /// Dispatches `command` with `args` as `ana`, holding an approval of it
    /// the kernel issued.
    async fn approved(&self, command: &str, args: &str) -> Result<String, Refusal> {
        let ana = self
            .world
            .principal(PrincipalKind::User, "ana")
            .expect("ana is a user of the world");
        let approval = self.kernel.approval_for_tests(&ana, command, args);
        self.kernel
            .dispatch(&Caller::new(ana).approved(approval), command, args)
            .await
    }

    /// How many commands the instance of `namespace`'s plugin has run, this
    /// count included.
    async fn executions(&self, namespace: &str) -> i64 {
        let answer = self
            .as_ana(
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

    /// How the plugin in `slot` stands.
    fn status(&self, slot: PluginSlot) -> PluginStatus {
        self.kernel
            .plugin_status(slot)
            .expect("the slot holds a plugin this kernel loaded")
    }

    /// What `plugin`'s `key` holds in the store, committed.
    async fn held(&self, plugin: &str, key: &str) -> Option<String> {
        self.store
            .inner
            .get(&home(), plugin, key)
            .await
            .expect("the store answers")
            .map(|entry| entry.value)
    }

    /// Where the blob `hash` is, as the reference plugin is told.
    async fn located(&self, hash: &str) -> String {
        let answered = self
            .as_ana("reference.blob.locate", &blob_about(hash))
            .await
            .expect("located");
        field(&answered, "availability")
            .into_text()
            .expect("a word")
    }

    /// Every entry of the kernel's journal.
    async fn entries(&self) -> Vec<Entry> {
        self.kernel
            .journal_for_tests()
            .await
            .expect("the journal reads back")
    }

    /// The values of `field` in every event `name` the kernel logged.
    fn logged(&self, name: &str, field: &str) -> Vec<String> {
        self.log
            .named(name)
            .iter()
            .filter_map(|line| line.text(field))
            .collect()
    }

    /// The journal verified through its own command, as every entry it held
    /// when it was asked.
    async fn verifies(&self) {
        let entries = self.entries().await.len();
        let answer = self
            .as_ana("journal.verify", "{}")
            .await
            .expect("the journal is verified");
        assert_eq!(
            field(&answer, "verified"),
            Value::Integer(i64::try_from(entries).expect("fits")),
            "{answer}: every entry, and only the entries, written before it was asked"
        );
    }
}

/// The field `name` of the canonical JSON object `answer`.
fn field(answer: &str, name: &str) -> Value {
    nc_canon::parse(answer)
        .unwrap_or_else(|refused| panic!("{answer} is not canonical JSON: {refused}"))
        .into_object()
        .and_then(|mut fields| fields.remove(name))
        .unwrap_or_else(|| panic!("{answer} holds no `{name}`"))
}

/// `blob.put`'s arguments: `text`, written once, about the note `photo`, and
/// how it ends when `then` says.
fn blob_put(text: &str, then: Option<&str>) -> String {
    let mut fields = vec![("note", Value::text("photo")), ("text", Value::text(text))];
    if let Some(then) = then {
        fields.push(("then", Value::text(then)));
    }
    Value::object(fields).to_canonical()
}

/// The arguments of a command about the blob `hash`, and the note `photo`.
fn blob_about(hash: &str) -> String {
    Value::object([("hash", Value::text(hash)), ("note", Value::text("photo"))]).to_canonical()
}

/// A note's arguments: `photo`, and `text` when it is written.
fn photo(text: Option<&str>) -> String {
    let mut fields = vec![("note", Value::text("photo"))];
    if let Some(text) = text {
        fields.push(("text", Value::text(text)));
    }
    Value::object(fields).to_canonical()
}

/// The failure an entry records, from its resource.
fn failure_of(entry: &Entry) -> Option<String> {
    entry
        .record
        .resource
        .clone()
        .into_object()
        .and_then(|mut fields| fields.remove("failure"))
        .and_then(Value::into_text)
}

/// The entries that record a dispatch of `command`.
fn of<'a>(entries: &'a [Entry], command: &str) -> Vec<&'a Entry> {
    entries
        .iter()
        .filter(|entry| {
            entry
                .record
                .resource
                .clone()
                .into_object()
                .and_then(|mut fields| fields.remove("command"))
                .and_then(Value::into_text)
                .as_deref()
                == Some(command)
        })
        .collect()
}

/// Holds `entries` to one entry of `command`, failed with the word of `kill`.
///
/// Every kill's word is written here, so that a cause added to [`Kill`] is
/// given one; each is read by a test that kills a plugin so.
#[track_caller]
fn journaled_as_killed(entries: &[Entry], command: &str, kill: Kill) {
    let [entry] = of(entries, command)[..] else {
        panic!("one entry of `{command}`: {entries:#?}");
    };
    assert_eq!(entry.record.result, Outcome::Failed, "`{command}`");
    assert_eq!(
        failure_of(entry).as_deref(),
        Some(match kill {
            Kill::Trap => "trapped",
            Kill::Fuel => "out-of-fuel",
            Kill::Epoch => "epoch-deadline",
            Kill::Memory => "memory-limit",
            Kill::HostCallDeadline => "host-call-deadline",
            Kill::TreeDeadline => "tree-deadline",
        }),
        "`{command}`"
    );
}

/// Runs `command` of the reference plugin and holds all to what a kill leaves.
///
/// The kill's own code and log, one crash and the next generation for the
/// reference plugin and a fresh instance that has run one command since, the
/// peer as it was — status, instance and data — and a journal that
/// verifies.
async fn killed_and_nothing_else(fixture: &Fixture, command: &str, kill: Kill, cause: &str) {
    fixture
        .as_ana(&format!("{PEER}.note.put"), &photo(Some("the peer's")))
        .await
        .expect("the peer writes its note");
    let peer = fixture.status(fixture.peer);
    let peer_ran = fixture.executions(PEER).await;
    let before = fixture.status(fixture.reference);
    let refused = fixture
        .as_ana(command, &photo(None))
        .await
        .expect_err("a plugin that is killed answers nothing");
    assert_eq!(refused.kind(), RefusalKind::Internal, "{refused}");
    assert_eq!(refused.code(), kill.code(), "{refused}");
    let after = fixture.status(fixture.reference);
    assert_eq!(after.crashes(), before.crashes() + 1, "{after:?}");
    assert_eq!(after.generation(), before.generation() + 1, "{after:?}");
    assert_eq!(after.last_kill(), Some(kill));
    assert!(!after.quarantined(), "{after:?}");
    assert_eq!(
        fixture.executions("reference").await,
        1,
        "the plugin was started again in a fresh instance"
    );
    assert_eq!(
        fixture.status(fixture.peer),
        peer,
        "the peer's status moved"
    );
    assert_eq!(
        fixture.executions(PEER).await,
        peer_ran + 1,
        "the peer was started again, or ran something else meanwhile"
    );
    assert_eq!(
        fixture.held(THE_PEER, "note/photo").await,
        Some(r#"{"text":"the peer's"}"#.to_owned())
    );
    assert_eq!(fixture.held(REFERENCE, "note/photo").await, None);
    assert_eq!(fixture.logged("kernel.plugin_killed", "cause"), [cause]);
    assert_eq!(fixture.logged("kernel.plugin_killed", "slot"), ["0"]);
    assert_eq!(
        fixture.logged("kernel.plugin_killed", "code"),
        [kill.code().code()]
    );
    assert_eq!(fixture.logged("kernel.plugin_restarted", "slot"), ["0"]);
    assert_eq!(
        fixture.log.named("kernel.plugin_quarantined"),
        [] as [nc_kernel::logging::CapturedLine; 0]
    );
    journaled_as_killed(&fixture.entries().await, command, kill);
    fixture.verifies().await;
}

#[tokio::test]
async fn a_plugin_that_traps_is_killed_and_restarted_and_its_peer_does_not_notice() {
    let fixture = opened("").await;
    killed_and_nothing_else(&fixture, "reference.abuse.trap", Kill::Trap, "trap").await;
}

/// A loop that never ends is stopped by the epoch: the fuel is the catalog's,
/// seconds of looping, and the deadline five ticks.
#[tokio::test]
async fn a_plugin_that_spins_is_stopped_by_the_epoch_and_restarted() {
    let fixture = opened("epoch_deadline_ticks = 5\n").await;
    killed_and_nothing_else(&fixture, "reference.abuse.spin", Kill::Epoch, "epoch").await;
}

/// A computation without end is stopped by its fuel: the deadline is the
/// catalog's, seconds away, and the fuel a hundredth of a second of looping.
#[tokio::test]
async fn a_plugin_that_burns_its_fuel_is_killed_and_restarted() {
    let fixture = opened("fuel_per_dispatch = 10_000_000\n").await;
    killed_and_nothing_else(&fixture, "reference.abuse.burn", Kill::Fuel, "fuel").await;
}

/// A memory that grows without end is stopped at the manifest's bound, four
/// mebibytes, and told apart from a trap by the guard's own error — and the
/// instance it is started again in is held to the same bound.
///
/// A verifier found the restart's bound held by nothing: built apart from the
/// load's, the restarted instance could be given the catalog's sixteen times
/// larger memory with every suite green.
#[tokio::test]
async fn a_plugin_that_outgrows_its_memory_is_killed_and_restarted() {
    let fixture = opened("memory_bytes = 4_194_304\n").await;
    killed_and_nothing_else(&fixture, "reference.abuse.grow", Kill::Memory, "memory").await;
    let refused = fixture
        .as_ana("reference.abuse.grow", &photo(None))
        .await
        .expect_err("a plugin that is killed answers nothing");
    assert_eq!(refused.code(), ErrorCode::PluginMemoryLimit, "{refused}");
    assert!(
        refused.sentence().contains("past the 4194304 its"),
        "the restarted instance was not held to its manifest's memory: {refused}"
    );
    assert_eq!(fixture.status(fixture.reference).generation(), 2);
}

/// The ungranted functions the probe reaches, and what each answered.
fn denied(probed: &str) -> std::collections::BTreeMap<String, String> {
    nc_canon::parse(probed)
        .unwrap_or_else(|refused| panic!("{probed} is not canonical JSON: {refused}"))
        .into_object()
        .unwrap_or_else(|| panic!("{probed} is not an object"))
        .into_iter()
        .filter_map(|(function, answer)| {
            let answer = answer.into_text()?;
            answer
                .starts_with(&format!("denied: [{}] ", ErrorCode::PluginCapabilityDenied))
                .then_some((function, answer))
        })
        .collect()
}

/// A plugin started again is granted what its manifest names and nothing
/// more: every function its probe was denied before the kill, it is denied
/// after it.
///
/// A verifier found the restart's grants held by nothing: built apart from the
/// load's, the restarted instance could be handed every capability with every
/// suite green.
#[tokio::test]
async fn a_restarted_plugin_is_granted_what_its_manifest_names_and_nothing_more() {
    let fixture = opened("").await;
    let before = denied(
        &fixture
            .as_ana("reference.probe", &photo(None))
            .await
            .expect("the probe answers"),
    );
    for function in ["ui#render", "timers#after", "events#publish"] {
        assert!(
            before.contains_key(function),
            "`{function}` is granted to the reference plugin: {before:#?}"
        );
    }
    let refused = fixture
        .as_ana("reference.abuse.trap", &photo(None))
        .await
        .expect_err("a trap answers nothing");
    assert_eq!(refused.code(), ErrorCode::PluginTrapped, "{refused}");
    assert_eq!(fixture.status(fixture.reference).generation(), 1);
    let after = denied(
        &fixture
            .as_ana("reference.probe", &photo(None))
            .await
            .expect("the probe answers"),
    );
    assert_eq!(
        after, before,
        "the restarted plugin is answered otherwise than it was"
    );
}

/// How long a test waits for what should end at a deadline before it calls
/// the wait for ever: an hour, on a clock that is paused.
const WAITED_ON_FOR_EVER: Duration = Duration::from_secs(3_600);

/// A host call that never answers is abandoned at its deadline and the plugin
/// waiting on it killed: a read of the store, the read a write makes of what
/// it replaces, a listing, and the read a delete makes of what it removes,
/// each in a kernel of its own.
///
/// **On a paused clock**, which runs on to the deadline the moment nothing is
/// left to run: the deadline is twenty seconds, and the test takes none of
/// them. A note is written after the plugin reads it, so the write's own read
/// is the second; the probe is what reaches the listing — mutating, so it
/// runs in a unit, which the kill leaves unkept; and a delete is dispatched
/// with an approval, which the family pack asks of one.
///
/// **Journaled and said as a wait on the host**, which a verifier found held
/// by nothing: the dispatch's entry could record the kill as `trapped`, and
/// its refusal say so, with every suite green.
#[tokio::test(start_paused = true)]
async fn a_host_call_that_never_returns_is_abandoned_and_the_plugin_killed() {
    for (hang, command, args, function, approved) in [
        (
            Hang::NoteReadsFrom(0),
            "reference.note.get",
            photo(None),
            "store#get",
            false,
        ),
        (
            Hang::NoteReadsFrom(1),
            "reference.note.put",
            photo(Some("never kept")),
            "store#put",
            false,
        ),
        (
            Hang::Listings,
            "reference.probe",
            photo(None),
            "store#list",
            false,
        ),
        (
            Hang::NoteReadsFrom(0),
            "reference.note.delete",
            photo(None),
            "store#delete",
            true,
        ),
    ] {
        let fixture = opened("").await;
        abandoned_at_its_deadline(&fixture, hang, (command, &args), function, approved).await;
    }
}

/// A blob's host call that never answers is abandoned at its deadline.
///
/// The plugin waiting on it is killed, as for a store's: asking where a blob
/// is and opening one, which ask the store for its row, reading one's bytes,
/// and beginning, writing and finishing a writing, and the row a finish asks
/// for — the last four leaving no blob of the unit the kill doomed.
///
/// **And no bytes, but one finish's own.** A finish asks for its blob's row
/// before the bytes are named, so a row that never comes leaves nothing named.
/// Found by a review: the row was asked for after, and its bytes outlived the
/// kill under a name no row held and the doomed unit did not know. What is
/// left is a finish the deadline abandons while the store goes on naming the
/// bytes — the thread that renames them is not stopped by the kill — and the
/// sweep deletes those, as bytes no row names.
#[tokio::test(start_paused = true)]
async fn a_blob_host_call_that_never_returns_is_abandoned_and_the_plugin_killed() {
    let kept = Hash::of(b"kept").to_string();
    for (hang, command, args, function) in [
        (
            Hang::BlobRows,
            "reference.blob.locate",
            blob_about(&kept),
            "blobs#locate",
        ),
        (
            Hang::BlobRows,
            "reference.blob.get",
            blob_about(&kept),
            "blobs#open",
        ),
        (
            Hang::BlobReads,
            "reference.blob.get",
            blob_about(&kept),
            "blobs#[method]reader.read",
        ),
        (
            Hang::BlobCreates,
            "reference.blob.put",
            blob_put("never kept", None),
            "blobs#create",
        ),
        (
            Hang::BlobWrites,
            "reference.blob.put",
            blob_put("never kept", None),
            "blobs#[method]writer.write",
        ),
        (
            Hang::BlobFinishes,
            "reference.blob.put",
            blob_put("never kept", None),
            "blobs#[method]writer.finish",
        ),
        (
            Hang::BlobRows,
            "reference.blob.put",
            blob_put("never kept", None),
            "blobs#[method]writer.finish",
        ),
    ] {
        let fixture = opened("").await;
        let putting = command == "reference.blob.put";
        if !putting {
            fixture
                .as_ana("reference.blob.put", &blob_put("kept", None))
                .await
                .expect("a blob is kept to be read");
        }
        abandoned_at_its_deadline(&fixture, hang, (command, &args), function, false).await;
        if putting {
            assert_eq!(
                fixture.located(&Hash::of(b"never kept").to_string()).await,
                "unavailable",
                "`{function}`: a blob a killed unit made is on this machine"
            );
            let named_by_the_store = hang == Hang::BlobFinishes;
            fixture
                .blobs
                .age(Duration::from_millis(KERNEL.blob_orphan_grace_ms + 1));
            assert_eq!(
                fixture.kernel.sweep_blobs().await.expect("swept"),
                Swept {
                    orphans: u64::from(named_by_the_store),
                    ..Swept::default()
                },
                "`{function}` under {hang:?}: the bytes a killed unit's writer named outlived \
                 it, or a finish's own were not left for the sweep"
            );
        }
    }
}

/// A fetch whose transport never answers is abandoned at the host call's
/// deadline, and the plugin waiting on it killed, as a store's call is.
///
/// **On a paused clock moved by hand.** A transport runs on the runtime's
/// blocking pool, and a task there keeps tokio's paused clock from running on
/// by itself, so the test waits until the transport is reached and then
/// moves the clock past the deadline. The transport waits on the real clock
/// for at most `STALLED_AT_MOST`, so a deadline that is missing fails the
/// test when that passes rather than hanging it.
#[tokio::test(start_paused = true)]
async fn a_fetch_that_never_returns_is_abandoned_and_the_plugin_killed() {
    let fixture = opened("").await;
    let peer = fixture.status(fixture.peer);
    let stall = fixture.transport.stall();
    let started = tokio::time::Instant::now();
    let args = Value::object([
        ("note", Value::text("photo")),
        ("url", Value::text("https://example.com/")),
    ])
    .to_canonical();
    let dispatched = fixture.as_ana("reference.abuse.fetch", &args);
    tokio::pin!(dispatched);
    tokio::select! {
        biased;
        answered = &mut dispatched => panic!("answered before the transport was reached: {answered:?}"),
        () = fixture.transport.reached() => {}
    }
    tokio::time::advance(Duration::from_millis(KERNEL.host_call_deadline_ms)).await;
    let refused = dispatched
        .await
        .expect_err("a plugin that is killed answers nothing");
    drop(stall);
    assert_eq!(
        refused.code(),
        ErrorCode::PluginHostCallDeadline,
        "{refused}"
    );
    assert_eq!(
        started.elapsed(),
        Duration::from_millis(KERNEL.host_call_deadline_ms),
        "abandoned at another time than its deadline"
    );
    assert!(
        refused
            .sentence()
            .contains("`net#fetch` did not answer within"),
        "{refused}"
    );
    journaled_as_killed(
        &fixture.entries().await,
        "reference.abuse.fetch",
        Kill::HostCallDeadline,
    );
    let status = fixture.status(fixture.reference);
    assert_eq!(
        (status.crashes(), status.generation(), status.last_kill()),
        (1, 1, Some(Kill::HostCallDeadline))
    );
    assert_eq!(
        fixture.logged("kernel.plugin_killed", "cause"),
        ["deadline"]
    );
    assert_eq!(fixture.executions("reference").await, 1);
    assert_eq!(
        fixture.status(fixture.peer),
        peer,
        "the peer's status moved"
    );
    assert_eq!(
        fixture.transport.asked().len(),
        1,
        "the fetch was sent again"
    );
    fixture.verifies().await;
}

/// Dispatches `command` with `args` into a store that hangs as `hang` says —
/// approved when `approved` — and holds what comes back to a plugin killed for
/// waiting on `function` past the host call's deadline, and nothing else.
async fn abandoned_at_its_deadline(
    fixture: &Fixture,
    hang: Hang,
    (command, args): (&str, &str),
    function: &str,
    approved: bool,
) {
    let peer = fixture.status(fixture.peer);
    *fixture
        .store
        .hang
        .lock()
        .expect("nothing panics holding it") = hang;
    let started = tokio::time::Instant::now();
    let dispatched = async {
        if approved {
            fixture.approved(command, args).await
        } else {
            fixture.as_ana(command, args).await
        }
    };
    let refused = tokio::time::timeout(WAITED_ON_FOR_EVER, dispatched)
        .await
        .unwrap_or_else(|_| panic!("`{command}` waited on its host call for ever"))
        .expect_err("a plugin that is killed answers nothing");
    assert_eq!(
        refused.code(),
        ErrorCode::PluginHostCallDeadline,
        "`{command}`: {refused}"
    );
    assert_eq!(
        started.elapsed(),
        Duration::from_millis(KERNEL.host_call_deadline_ms),
        "`{command}` was abandoned at another time than its deadline"
    );
    assert!(
        refused
            .sentence()
            .contains(&format!("`{function}` did not answer within")),
        "{refused}"
    );
    assert!(
        refused.sentence().contains(&format!(
            "`{command}` waited on a host call past its deadline, and its plugin was \
             killed and restarted"
        )),
        "{refused}"
    );
    journaled_as_killed(&fixture.entries().await, command, Kill::HostCallDeadline);
    let status = fixture.status(fixture.reference);
    assert_eq!(
        (status.crashes(), status.generation(), status.last_kill()),
        (1, 1, Some(Kill::HostCallDeadline)),
        "`{command}`"
    );
    assert_eq!(
        fixture.logged("kernel.plugin_killed", "cause"),
        ["deadline"]
    );
    *fixture
        .store
        .hang
        .lock()
        .expect("nothing panics holding it") = Hang::Nothing;
    assert_eq!(fixture.executions("reference").await, 1, "`{command}`");
    assert_eq!(
        fixture.status(fixture.peer),
        peer,
        "the peer's status moved"
    );
    assert_eq!(fixture.held(REFERENCE, "note/photo").await, None);
    fixture.verifies().await;
}

/// A plugin waiting on the host longer than its own share of the epoch is not
/// killed for it: the epoch bounds what the plugin runs, and the host call has
/// a deadline of its own.
///
/// The plugin is given five ticks and every read of a note takes six times as
/// long: a read of a note that is not there, which answers nothing; a read of
/// one that is, whose text the host writes into the plugin's memory through
/// the plugin's own allocator; and a write, whose own read is late as well and
/// which answers its revision the same way. Counted on the whole call, the
/// plugin is interrupted as soon as its first read returns.
///
/// **The two that answer a value found a real bug.** The allocator is the
/// plugin running inside a host call, and the epoch check at its entry calls
/// the host again: the account that only a call into the host opened was
/// opened again there, the wait before it was counted as the plugin's own
/// running, and a plugin waiting on a slow read was killed as `[NC-PLUG-0004]`
/// the moment the read answered.
#[tokio::test]
async fn a_plugin_waiting_on_the_host_past_its_epoch_share_is_not_killed_for_it() {
    let fixture = opened("epoch_deadline_ticks = 5\n").await;
    let late = Duration::from_millis(KERNEL.epoch_tick_ms * 30);
    let slowed = |hang| {
        *fixture
            .store
            .hang
            .lock()
            .expect("nothing panics holding it") = hang;
    };
    slowed(Hang::NoteReadsFor(late));
    let started = std::time::Instant::now();
    let read = fixture.as_ana("reference.note.get", &photo(None)).await;
    assert!(started.elapsed() >= late, "the read was not late");
    assert_eq!(read, Ok("null".to_owned()));
    slowed(Hang::Nothing);
    fixture
        .as_ana("reference.note.put", &photo(Some("held")))
        .await
        .expect("the note is written");
    slowed(Hang::NoteReadsFor(late));
    let started = std::time::Instant::now();
    let read = fixture.as_ana("reference.note.get", &photo(None)).await;
    assert!(started.elapsed() >= late, "the read was not late");
    assert_eq!(read, Ok(r#"{"text":"held"}"#.to_owned()));
    let started = std::time::Instant::now();
    let written = fixture
        .as_ana("reference.note.put", &photo(Some("again")))
        .await;
    assert!(
        started.elapsed() >= late * 2,
        "the write's reads were not late"
    );
    let rev = field(&written.expect("a late write is not killed"), "rev")
        .into_text()
        .expect("the revision it answered");
    assert!(!rev.is_empty(), "{rev}");
    assert_eq!(
        fixture.held(REFERENCE, "note/photo").await,
        Some(r#"{"text":"again"}"#.to_owned())
    );
    assert_eq!(fixture.status(fixture.reference).crashes(), 0);
    assert_eq!(
        fixture.log.named("kernel.plugin_killed"),
        [] as [nc_kernel::logging::CapturedLine; 0]
    );
}

/// A plugin whose store answers slowly is killed at its call tree's deadline,
/// started again, and spends none of its restart budget on it.
///
/// Every read of a note answers inside its host call's deadline, and a write
/// makes two, which together outlast the tree's; the plugin's own running is a
/// sliver of either. The second read is cut at the tree's deadline rather
/// than waited on to its end. **On the real clock**, since the tree's deadline
/// is kept on the monotonic one, so the test waits the tree out — half a
/// minute. After the kill, one trap short of the budget restarts the plugin
/// every time: the tree's kill counted against it would quarantine it at the
/// last.
///
/// **A reviewer found the tree outlasting its deadline**: a host call was held
/// to its own deadline alone, so the second read ran to its end, past the
/// tree's — and a tree of thirty seconds whose fetches each waited fourteen
/// ran forty-two.
///
/// **A verifier found the budget spent by it**: the same plugin, its store as
/// slow, was quarantined at its third write, though every host answer came
/// inside its deadline and the plugin ran microseconds — and the kill's
/// journal word and log cause were held by nothing.
#[tokio::test]
async fn a_plugin_killed_at_its_trees_deadline_is_restarted_and_spends_no_budget() {
    let late = Duration::from_millis(KERNEL.host_call_deadline_ms - 1_000);
    let tree = Duration::from_millis(KERNEL.dispatch_deadline_ms);
    let slack = Duration::from_secs(4);
    assert!(
        late < tree && late * 2 > tree + slack,
        "a read {late:?} late is past the tree's deadline, or two are inside it and \
         its slack"
    );
    let fixture = opened("").await;
    let peer = fixture.status(fixture.peer);
    let slowed = |hang| {
        *fixture
            .store
            .hang
            .lock()
            .expect("nothing panics holding it") = hang;
    };
    slowed(Hang::NoteReadsFor(late));
    let command = "reference.note.put";
    let started = std::time::Instant::now();
    let refused = fixture
        .as_ana(command, &photo(Some("never kept")))
        .await
        .expect_err("a plugin past its tree's deadline answers nothing");
    let elapsed = started.elapsed();
    assert!(elapsed >= late, "the write's first read was not late");
    assert!(
        elapsed < tree + slack,
        "a call tree of {tree:?} ran {elapsed:?}: its second read was waited on past the \
         tree's deadline"
    );
    slowed(Hang::Nothing);
    assert_eq!(refused.kind(), RefusalKind::Internal, "{refused}");
    assert_eq!(refused.code(), Kill::TreeDeadline.code(), "{refused}");
    assert!(
        refused.sentence().contains(&format!(
            "`{command}` ran past its call tree's deadline, and its plugin was killed, and \
             is started again before the next call tree runs"
        )),
        "{refused}"
    );
    let status = fixture.status(fixture.reference);
    assert_eq!(
        (status.crashes(), status.generation(), status.last_kill()),
        (1, 0, Some(Kill::TreeDeadline)),
        "{status:?}: the tree had no time left to start it again in"
    );
    assert!(!status.quarantined(), "{status:?}");
    assert_eq!(
        fixture.log.named("kernel.plugin_restarted"),
        [] as [nc_kernel::logging::CapturedLine; 0]
    );
    assert_eq!(
        fixture.executions("reference").await,
        1,
        "the next call tree started it again, in a fresh instance, before it ran"
    );
    assert_eq!(fixture.status(fixture.reference).generation(), 1);
    assert_eq!(fixture.logged("kernel.plugin_restarted", "slot"), ["0"]);
    assert_eq!(fixture.logged("kernel.plugin_killed", "cause"), ["tree"]);
    assert_eq!(
        fixture.logged("kernel.plugin_killed", "code"),
        [ErrorCode::KernelDeadline.code()]
    );
    journaled_as_killed(&fixture.entries().await, command, Kill::TreeDeadline);
    assert_eq!(fixture.held(REFERENCE, "note/photo").await, None);
    let budget = KERNEL.restart_budget;
    for trap in 1..budget {
        let refused = fixture
            .as_ana("reference.abuse.trap", &photo(None))
            .await
            .expect_err("a trap answers nothing");
        assert_eq!(
            refused.code(),
            ErrorCode::PluginTrapped,
            "trap {trap} after the tree's kill, which was held against the budget: \
             {refused}"
        );
    }
    let status = fixture.status(fixture.reference);
    assert!(!status.quarantined(), "{status:?}");
    assert_eq!(status.crashes(), budget, "every kill is counted");
    assert_eq!(
        fixture.log.named("kernel.plugin_quarantined"),
        [] as [nc_kernel::logging::CapturedLine; 0]
    );
    assert_eq!(
        fixture.status(fixture.peer),
        peer,
        "the peer's status moved"
    );
    fixture.verifies().await;
}

/// A plugin killed inside a call tree is started again within what the tree
/// has left, and one the tree's deadline stops while it activates waits for
/// the next call tree, quarantined for none of it.
///
/// Every read of a plugin's activation answers nineteen seconds late — inside
/// the host call's deadline. The reference plugin's relay dispatches the
/// peer's trap twice, going on through the first refusal: the first kill
/// starts the peer again in nineteen seconds, inside the tree; the second
/// leaves it eleven, which the tree's deadline cuts its activation at; the
/// relay, waiting on it, is then past the deadline too. **A reviewer found a
/// real bug with it**: each restart was held to a span of its own rather than
/// to what the tree had left, so the tree ran on by the time each plugin on
/// its stack took to come back — forty-nine seconds for one, sixty-eight for
/// two.
#[tokio::test]
async fn a_restart_inside_a_call_tree_ends_with_the_tree_and_quarantines_nothing() {
    let late = Duration::from_millis(KERNEL.host_call_deadline_ms - 1_000);
    let tree = Duration::from_millis(KERNEL.dispatch_deadline_ms);
    let slack = Duration::from_secs(4);
    assert!(
        late < tree && late * 2 > tree + slack,
        "an activation {late:?} late is past the tree's deadline, or two are inside it \
         and its slack"
    );
    let fixture = opened("").await;
    let relayed = Value::object([
        ("args", Value::text(photo(None))),
        ("name", Value::text(format!("{PEER}.abuse.trap"))),
        ("note", Value::text("photo")),
        ("through", Value::Bool(true)),
        ("times", Value::Integer(2)),
    ])
    .to_canonical();
    fixture.store.set(Hang::ActivationReadsFor(late));
    let started = std::time::Instant::now();
    let refused = fixture
        .as_ana("reference.relay.write", &relayed)
        .await
        .expect_err("a relay past its tree's deadline answers nothing");
    let elapsed = started.elapsed();
    fixture.store.set(Hang::Nothing);
    assert!(
        elapsed >= late,
        "the first restart did not wait on its activation"
    );
    assert!(
        elapsed < tree + slack,
        "a call tree of {tree:?} ran {elapsed:?}: a restart inside it was not held to what \
         the tree had left"
    );
    assert_eq!(refused.code(), Kill::TreeDeadline.code(), "{refused}");
    let (reference, peer) = (
        fixture.status(fixture.reference),
        fixture.status(fixture.peer),
    );
    assert_eq!(
        (peer.crashes(), peer.generation(), peer.last_kill()),
        (2, 1, Some(Kill::Trap)),
        "{peer:?}: started again once inside the tree, and not a second time past it"
    );
    assert_eq!(
        (
            reference.crashes(),
            reference.generation(),
            reference.last_kill()
        ),
        (1, 0, Some(Kill::TreeDeadline)),
        "{reference:?}"
    );
    assert!(!peer.quarantined() && !reference.quarantined());
    assert_eq!(
        fixture.log.named("kernel.plugin_quarantined"),
        [] as [nc_kernel::logging::CapturedLine; 0]
    );
    assert_eq!(fixture.logged("kernel.plugin_restarted", "slot"), ["1"]);
    fixture.kernel.pump().await;
    assert_eq!(
        (
            fixture.status(fixture.peer).generation(),
            fixture.status(fixture.reference).generation()
        ),
        (2, 1),
        "the pump, between call trees, started both again"
    );
    assert_eq!(
        fixture.executions(PEER).await,
        1,
        "the peer runs in the instance the pump started"
    );
    assert_eq!(
        fixture.executions("reference").await,
        1,
        "and so does the relay's plugin"
    );
    fixture.verifies().await;
}

/// A timer's callback killed at its call tree's deadline, with no time left
/// to start its plugin again in, leaves the pump's next callback of that
/// plugin a fresh instance: the timer due after it fires and runs.
///
/// The peer — granted `timers`, which the reference plugin is not — sets two
/// timers. The first dispatches the reference plugin's `note.put`, whose two
/// reads of the note each answer nineteen seconds late, so the tree's deadline
/// kills the reference plugin, and the peer waiting on it with it, in a tree
/// with nothing left. **Held by nothing before**: a reviewer took the pump's
/// start before each callback out and every suite stayed green, while the
/// second timer was spent with no callback run and counted as failed.
#[tokio::test]
async fn a_callback_killed_at_its_trees_deadline_leaves_the_next_callback_a_fresh_instance() {
    let late = Duration::from_millis(KERNEL.host_call_deadline_ms - 1_000);
    let fixture = opened("").await;
    for tag in ["put:reference:photo", "after"] {
        let timer = Value::object([
            ("ms", Value::Integer(1_000)),
            ("note", Value::text("photo")),
            ("tag", Value::text(tag)),
        ])
        .to_canonical();
        fixture
            .as_ana("peer.timer.set", &timer)
            .await
            .unwrap_or_else(|refused| panic!("the peer sets its timer `{tag}`: {refused}"));
    }
    fixture.clock.advance(1_000);
    fixture.store.set(Hang::NoteReadsFor(late));
    let pumped = fixture.kernel.pump().await;
    fixture.store.set(Hang::Nothing);
    assert_eq!(
        (pumped.fired, pumped.failed),
        (2, 1),
        "the first callback was killed and the second ran: {pumped:?}"
    );
    let peer = fixture.status(fixture.peer);
    assert_eq!(
        (peer.crashes(), peer.generation(), peer.last_kill()),
        (1, 1, Some(Kill::TreeDeadline)),
        "{peer:?}: killed at its tree's deadline, then started for its next callback"
    );
    let lines: Vec<String> = fixture
        .kernel
        .plugin_log(fixture.peer)
        .expect("the peer is loaded")
        .lines()
        .iter()
        .map(|line| line.text().to_owned())
        .collect();
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("timer after fired")),
        "the second timer's callback ran: {lines:#?}"
    );
    assert_eq!(fixture.held(REFERENCE, "note/photo").await, None);
    fixture.verifies().await;
}

/// The third crash inside the window quarantines the plugin until it is released.
///
/// It is refused every dispatch meanwhile; a release forgives every kill and
/// starts it again — and one that does not come back as it was stays
/// quarantined.
#[tokio::test]
async fn a_plugin_that_crashes_past_its_budget_is_quarantined_and_released() {
    let fixture = opened("").await;
    let budget = KERNEL.restart_budget;
    assert!(budget > 1, "a budget of one quarantines at the first kill");
    let peer = fixture.status(fixture.peer);
    for crash in 1..=budget {
        let refused = fixture
            .as_ana("reference.abuse.trap", &photo(None))
            .await
            .expect_err("a trap answers nothing");
        let expected = if crash < budget {
            ErrorCode::PluginTrapped
        } else {
            ErrorCode::PluginQuarantined
        };
        assert_eq!(refused.code(), expected, "crash {crash}: {refused}");
    }
    let status = fixture.status(fixture.reference);
    assert!(status.quarantined(), "{status:?}");
    assert_eq!(status.crashes(), budget);
    assert_eq!(
        status.generation(),
        u64::from(budget - 1),
        "started again after every crash but the last"
    );
    let refused = fixture
        .as_ana(
            "reference.executions.count",
            r#"{"at_most":1,"note":"photo"}"#,
        )
        .await;
    assert!(
        refused
            .as_ref()
            .is_err_and(|refusal| refusal.kind() == RefusalKind::Unavailable
                && refusal.code() == ErrorCode::PluginQuarantined),
        "{refused:?}"
    );
    assert_eq!(fixture.logged("kernel.plugin_quarantined", "slot"), ["0"]);
    assert_eq!(
        fixture.logged("kernel.plugin_quarantined", "code"),
        [ErrorCode::PluginQuarantined.code()]
    );
    assert_eq!(
        fixture.status(fixture.peer),
        peer,
        "the peer's status moved"
    );
    fixture
        .kernel
        .release(fixture.reference)
        .await
        .expect("the plugin comes back as it was");
    let released = fixture.status(fixture.reference);
    assert!(!released.quarantined(), "{released:?}");
    assert_eq!(released.crashes(), 0, "a release forgives every kill");
    assert_eq!(released.generation(), u64::from(budget));
    assert_eq!(fixture.executions("reference").await, 1);
    for crash in 1..=budget {
        let refused = fixture
            .as_ana("reference.abuse.trap", &photo(None))
            .await
            .expect_err("a trap answers nothing");
        let expected = if crash < budget {
            ErrorCode::PluginTrapped
        } else {
            ErrorCode::PluginQuarantined
        };
        assert_eq!(
            refused.code(),
            expected,
            "crash {crash} since the release, held to the kills before it: {refused}"
        );
    }
    assert!(fixture.status(fixture.reference).quarantined());
    seeded(
        &fixture.store,
        REFERENCE,
        ACTIVATION,
        &Activation {
            variant: Variant::Trap,
            ..Activation::default()
        }
        .written(),
    )
    .await;
    let generation = fixture.status(fixture.reference).generation();
    let refused = fixture
        .kernel
        .release(fixture.reference)
        .await
        .expect_err("a plugin that traps in `activate` does not come back");
    assert_eq!(refused.kind(), RefusalKind::Unavailable, "{refused}");
    assert_eq!(refused.code(), ErrorCode::PluginQuarantined, "{refused}");
    assert!(fixture.status(fixture.reference).quarantined());
    assert_eq!(fixture.status(fixture.reference).crashes(), 0);
    assert_eq!(
        fixture.status(fixture.reference).generation(),
        generation,
        "an instance that never came up was counted as a generation"
    );
}

/// Kills older than the restart window are forgotten: a plugin that crashes
/// as often as its budget, but not inside one window, is started again every
/// time.
#[tokio::test]
async fn kills_older_than_the_restart_window_are_forgotten() {
    let fixture = opened("").await;
    let budget = KERNEL.restart_budget;
    for _ in 1..budget {
        let refused = fixture
            .as_ana("reference.abuse.trap", &photo(None))
            .await
            .expect_err("a trap answers nothing");
        assert_eq!(refused.code(), ErrorCode::PluginTrapped, "{refused}");
    }
    fixture.clock.advance(KERNEL.restart_window_ms);
    let refused = fixture
        .as_ana("reference.abuse.trap", &photo(None))
        .await
        .expect_err("a trap answers nothing");
    assert_eq!(
        refused.code(),
        ErrorCode::PluginTrapped,
        "a kill outside the window counted against the budget: {refused}"
    );
    let status = fixture.status(fixture.reference);
    assert!(!status.quarantined(), "{status:?}");
    assert_eq!(status.crashes(), budget, "every kill is counted");
}

/// Releasing a plugin that runs forgives its kills and starts nothing again.
///
/// The kills it had no longer count against its budget, and the instance it
/// runs in is the one it ran in before the release.
#[tokio::test]
async fn releasing_a_plugin_that_runs_forgives_its_kills_and_starts_nothing() {
    let fixture = opened("").await;
    let budget = KERNEL.restart_budget;
    for _ in 1..budget {
        let refused = fixture
            .as_ana("reference.abuse.trap", &photo(None))
            .await
            .expect_err("a trap answers nothing");
        assert_eq!(refused.code(), ErrorCode::PluginTrapped, "{refused}");
    }
    let before = fixture.status(fixture.reference);
    fixture
        .kernel
        .release(fixture.reference)
        .await
        .expect("a plugin that runs is released");
    let released = fixture.status(fixture.reference);
    assert_eq!(released.crashes(), 0, "{released:?}");
    assert_eq!(
        released.generation(),
        before.generation(),
        "it was started again"
    );
    assert!(!released.quarantined());
    assert_eq!(fixture.executions("reference").await, 1);
    assert_eq!(
        fixture.executions("reference").await,
        2,
        "the release started a new instance"
    );
    for _ in 1..budget {
        let refused = fixture
            .as_ana("reference.abuse.trap", &photo(None))
            .await
            .expect_err("a trap answers nothing");
        assert_eq!(
            refused.code(),
            ErrorCode::PluginTrapped,
            "the kills before the release counted against the budget: {refused}"
        );
    }
    assert_eq!(fixture.log.named("kernel.plugin_quarantined").len(), 0);
}

/// A slot no plugin of this kernel holds has no status and nothing to
/// release.
#[tokio::test]
async fn a_slot_this_kernel_loaded_nothing_in_has_no_status_and_nothing_to_release() {
    let fixture = opened("").await;
    let empty = Kernel::<Env>::open(KernelConfig {
        store: fixture.store.clone(),
        blobs: HangingBlobs {
            inner: Arc::new(MemoryBlobs::default()),
            hang: Arc::clone(&fixture.store.hang),
        },
        pdp: Ok(family()),
        journal: nc_journal::Journal::new(
            nc_journal::InMemory::default(),
            fixture.world.realm_id("home").expect("home is a realm"),
            keeper(),
            Minter::new(
                MachineFlake::from_value(4).expect("a flake"),
                fixture.clock.clone(),
            ),
        ),
        directory: MemoryDirectory::from_world(WORLD, &family()).expect("the world is valid"),
        clock: fixture.clock.clone(),
        log: Arc::new(Captured::default()),
        transport: Arc::clone(&fixture.transport),
    })
    .await
    .expect("the kernel opens");
    assert_eq!(empty.plugin_status(fixture.peer), None);
    assert_eq!(empty.release(fixture.peer).await, Ok(()));
}

/// A plugin dispatching into itself while it runs is refused, and not trapped.
///
/// The refusal is a value it reads: no crash, no new instance, and the count
/// it keeps goes on.
#[tokio::test]
async fn a_plugin_dispatching_into_itself_is_refused_rather_than_trapped() {
    let fixture = opened("").await;
    let answer = fixture
        .as_ana("reference.abuse.reenter", &photo(None))
        .await
        .expect("a refusal it reads is not a failure");
    let told = field(&answer, "refused")
        .into_text()
        .expect("the refusal it was told");
    assert!(
        told.starts_with(&format!("denied: [{}] ", ErrorCode::KernelReentrant)),
        "{told}"
    );
    let status = fixture.status(fixture.reference);
    assert_eq!(
        (status.crashes(), status.generation()),
        (0, 0),
        "{status:?}"
    );
    assert_eq!(
        fixture.executions("reference").await,
        2,
        "the same instance, which ran the dispatch into itself first"
    );
    assert_eq!(
        fixture.log.named("kernel.plugin_killed"),
        [] as [nc_kernel::logging::CapturedLine; 0]
    );
    let entries = fixture.entries().await;
    let [refused] = of(&entries, "reference.executions.count")[..1] else {
        panic!("the refused dispatch is journaled: {entries:#?}");
    };
    assert_eq!(refused.record.result, Outcome::Denied);
    assert_eq!(
        refused.record.deciding_policy.as_deref(),
        Some("kernel.reentrant")
    );
    fixture.verifies().await;
}

/// The plugin a call tree's count stops is the one that asks on past it, not
/// the one whose single dispatch happened to cross it.
///
/// The reference plugin relays to the peer's relay again and again, going on
/// through every refusal; each time, the peer dispatches once — into the
/// reference plugin, refused as re-entry, a value it reads — and stops at its
/// first refusal. Each pair spends two of the tree's dispatches, so the one
/// past the count is the peer's own. **A reviewer found a real bug with it**:
/// that dispatch trapped the peer, which was killed as `trap` and quarantined
/// after `restart_budget` such trees, while the reference plugin, which spent
/// the count, was never killed. Told instead, the peer answers its caller and
/// keeps its status, and the reference plugin, told in turn and asking on, is
/// the one killed — round after round, until its own budget quarantines it.
#[tokio::test]
async fn the_plugin_stopped_at_its_trees_count_is_the_one_that_asks_on_past_it() {
    let fixture = opened("").await;
    let once = Value::object([
        ("args", Value::text(photo(Some("x")))),
        ("name", Value::text("reference.echo")),
        ("note", Value::text("photo")),
    ])
    .to_canonical();
    let spending = Value::object([
        ("args", Value::text(once)),
        ("name", Value::text(format!("{PEER}.relay.read"))),
        ("note", Value::text("photo")),
        ("through", Value::Bool(true)),
        ("times", Value::Integer(2_048)),
    ])
    .to_canonical();
    let peer = fixture.status(fixture.peer);
    for round in 1..=KERNEL.restart_budget {
        let refused = fixture
            .as_ana("reference.relay.read", &spending)
            .await
            .expect_err("the relay asking on past its tree's count is killed");
        assert!(
            refused
                .sentence()
                .contains(&ErrorCode::KernelTooManyDispatches.to_string()),
            "round {round}: {refused}"
        );
        assert_eq!(fixture.status(fixture.peer), peer, "round {round}");
        assert_eq!(
            fixture.status(fixture.reference).crashes(),
            round,
            "round {round}: the relay that spent the count was not the one killed"
        );
        let entries = fixture.entries().await;
        let told = of(&entries, "reference.echo")
            .into_iter()
            .chain(of(&entries, &format!("{PEER}.relay.read")))
            .filter(|entry| {
                entry.record.deciding_policy.as_deref() == Some("kernel.too_many_dispatches")
            })
            .count();
        assert_eq!(
            usize::try_from(round).expect("fits") * 3,
            told,
            "round {round}: the peer told once, the relay told once and refused once more"
        );
    }
    let spender = fixture.status(fixture.reference);
    assert!(spender.quarantined(), "{spender:?}");
    assert_eq!(spender.last_kill(), Some(Kill::Trap));
    assert!(
        fixture
            .logged("kernel.plugin_killed", "slot")
            .iter()
            .all(|slot| slot == "0"),
        "a plugin but the relay was killed: {:?}",
        fixture.logged("kernel.plugin_killed", "slot")
    );
    assert_eq!(fixture.logged("kernel.plugin_quarantined", "slot"), ["0"]);
    fixture
        .as_ana(&format!("{PEER}.note.get"), &photo(None))
        .await
        .expect("the peer runs as it did");
}

/// More permits than every read a test makes once it opens the store's gate.
const GATE: usize = 16;

/// How far apart, on the paused clock, a test opens the store's gate.
const STEP: Duration = Duration::from_secs(1);

/// Traps the plugin of `namespace`, in `slot`, until its budget is spent.
async fn quarantine(fixture: &Fixture, namespace: &str, slot: PluginSlot) {
    for _ in 0..KERNEL.restart_budget {
        fixture
            .as_ana(&format!("{namespace}.abuse.trap"), &photo(None))
            .await
            .expect_err("a trap answers nothing");
    }
    let status = fixture.status(slot);
    assert!(status.quarantined(), "{status:?}");
}

/// A release its caller stops waiting for leaves the plugin one a later
/// release starts.
///
/// The store holds the release's activation at its gate, and the caller gives
/// up on it a step later, as a daemon's handler does when the person who asked
/// goes away; the gate is then opened and the plugin released again. **A
/// reviewer found a real bug with it**: awaited in place, the abandoned
/// release left the slot claimed as starting, so every later release answered
/// `Ok(())` and started nothing, and every dispatch to the plugin was refused
/// as a quarantined one's for as long as the kernel ran.
#[tokio::test(start_paused = true)]
async fn a_release_its_caller_stops_waiting_for_leaves_the_plugin_releasable() {
    let fixture = opened("").await;
    quarantine(&fixture, "reference", fixture.reference).await;
    fixture.store.set(Hang::Gated);
    let abandoned = tokio::time::timeout(STEP, fixture.kernel.release(fixture.reference)).await;
    assert!(
        abandoned.is_err(),
        "the release answered while its activation was held at the gate: {abandoned:?}"
    );
    fixture.store.set(Hang::Nothing);
    fixture.store.gate.add_permits(GATE);
    let again = fixture.kernel.release(fixture.reference).await;
    let status = fixture.status(fixture.reference);
    let dispatched = fixture
        .as_ana(
            "reference.executions.count",
            r#"{"at_most":1,"note":"photo"}"#,
        )
        .await;
    assert!(
        again.is_ok() && !status.quarantined() && dispatched.is_ok(),
        "after a release its caller gave up on, a second release answered {again:?}, the \
         plugin stands {status:?}, and a dispatch to it answered {dispatched:?}"
    );
}

/// Two releases at once start a quarantined plugin once.
///
/// The second finds the plugin being started by the first and leaves it to
/// that one: one generation more and one restart. Meanwhile the plugin is
/// quarantined still, and a dispatch to it is refused as a quarantined
/// plugin's before the decision point goes near its slot. The store holds the
/// first release's activation at its gate until all three are asked. **A
/// verifier found it started twice**: each release saw the plugin stopped,
/// and each started an instance of its own, the later put over the earlier.
#[tokio::test(start_paused = true)]
async fn two_releases_at_once_start_a_quarantined_plugin_once() {
    let fixture = opened("").await;
    quarantine(&fixture, "reference", fixture.reference).await;
    let before = fixture.status(fixture.reference);
    let restarted = fixture.log.named("kernel.plugin_restarted").len();
    fixture.store.set(Hang::Gated);
    let (first, second, (meanwhile, refused)) = tokio::join!(
        fixture.kernel.release(fixture.reference),
        fixture.kernel.release(fixture.reference),
        async {
            let meanwhile = fixture.status(fixture.reference);
            let refused = fixture
                .as_ana(
                    "reference.executions.count",
                    r#"{"at_most":1,"note":"photo"}"#,
                )
                .await;
            fixture.store.gate.add_permits(GATE);
            (meanwhile, refused)
        },
    );
    fixture.store.set(Hang::Nothing);
    assert_eq!((first, second), (Ok(()), Ok(())));
    assert!(
        meanwhile.quarantined(),
        "a plugin being started was not quarantined: {meanwhile:?}"
    );
    assert!(
        refused
            .as_ref()
            .is_err_and(|refusal| refusal.kind() == RefusalKind::Unavailable
                && refusal.code() == ErrorCode::PluginQuarantined),
        "{refused:?}"
    );
    assert_eq!(
        fixture
            .logged("kernel.dispatch_refused", "reason")
            .last()
            .map(String::as_str),
        Some("quarantined"),
        "the dispatch was not refused before it reached the plugin's slot"
    );
    let after = fixture.status(fixture.reference);
    assert_eq!(
        (after.generation(), after.crashes(), after.quarantined()),
        (before.generation() + 1, 0, false),
        "{after:?}"
    );
    assert_eq!(
        fixture.log.named("kernel.plugin_restarted").len(),
        restarted + 1,
        "the two releases started it more than once"
    );
    assert_eq!(
        fixture.executions("reference").await,
        1,
        "the plugin was started in a fresh instance"
    );
}

/// A plugin unloaded while a release starts it stays unloaded.
///
/// The release claims the slot and starts the plugin, its activation held at
/// the store's gate; the unload, which waits on no release, takes the plugin
/// out meanwhile; and the instance the release then brings back is dropped
/// rather than put in a slot nothing runs again — the release answered as
/// one of a slot that holds no plugin.
///
/// **The unload waits a step**, until the release's task has claimed the slot
/// and is held at the gate. A release runs on a task of its own, and an unload
/// asked for in the same poll ran before that task did: the release then found
/// the slot unloaded and started nothing, so the race this test is named for
/// never happened — measured, the restart's arm for an unloaded slot deleted,
/// and this test green.
#[tokio::test(start_paused = true)]
async fn a_plugin_unloaded_while_a_release_starts_it_stays_unloaded() {
    let fixture = opened("").await;
    quarantine(&fixture, "reference", fixture.reference).await;
    let restarted = fixture.log.named("kernel.plugin_restarted").len();
    fixture.store.set(Hang::Gated);
    let (released, unloaded) = tokio::join!(fixture.kernel.release(fixture.reference), async {
        tokio::time::sleep(STEP).await;
        let unloaded = fixture.kernel.unload(fixture.reference).await;
        fixture.store.gate.add_permits(GATE);
        unloaded
    });
    fixture.store.set(Hang::Nothing);
    assert!(unloaded, "a quarantined plugin is unloaded");
    assert_eq!(released, Ok(()), "nothing is left to release");
    assert!(
        fixture.kernel.plugin_status(fixture.reference).is_none(),
        "the instance the release started was put in the slot an unload emptied"
    );
    assert_eq!(
        fixture.log.named("kernel.plugin_restarted").len(),
        restarted,
        "a plugin unloaded is not said to have come back"
    );
    let gone = fixture
        .as_ana(
            "reference.executions.count",
            r#"{"at_most":1,"note":"photo"}"#,
        )
        .await;
    assert!(
        gone.is_err_and(|refusal| refusal.code() == ErrorCode::KernelUnknownCommand),
        "its commands left with it"
    );
}

/// A plugin released twice at once is still refused when it dispatches into
/// itself.
///
/// The peer's `pair.put` writes its note and then dispatches the peer's own
/// `note.put`, and it runs while the second release would still be starting
/// an instance: the store's gate opens to the first release's activation, then
/// to the second's, then to the command's read of its note. **A verifier
/// found it let in**: the second release put the instance it started over the
/// one the command had taken out to run, and the dispatch from inside the
/// command ran in that second instance of the same plugin.
#[tokio::test(start_paused = true)]
async fn a_plugin_released_twice_at_once_is_still_refused_when_it_dispatches_into_itself() {
    let fixture = opened("").await;
    quarantine(&fixture, PEER, fixture.peer).await;
    let before = fixture.status(fixture.peer);
    let gate = &fixture.store.gate;
    fixture.store.set(Hang::Gated);
    let (first, second, paired, ()) = tokio::join!(
        fixture.kernel.release(fixture.peer),
        fixture.kernel.release(fixture.peer),
        async {
            tokio::time::sleep(STEP * 2).await;
            fixture
                .as_ana(&format!("{PEER}.pair.put"), &photo(Some("paired")))
                .await
        },
        async {
            tokio::time::sleep(STEP).await;
            gate.add_permits(1);
            tokio::time::sleep(STEP * 2).await;
            gate.add_permits(1);
            tokio::time::sleep(STEP).await;
            gate.add_permits(GATE);
        },
    );
    fixture.store.set(Hang::Nothing);
    assert_eq!((first, second), (Ok(()), Ok(())));
    let paired = paired.expect("the pair answers, and carries what the peer was told");
    let told = field(&paired, "peer")
        .into_text()
        .expect("what the dispatch into itself was told");
    assert!(
        told.starts_with(&format!("denied: [{}] ", ErrorCode::KernelReentrant)),
        "the plugin was let into itself: {paired}"
    );
    let entries = fixture.entries().await;
    let [refused] = of(&entries, &format!("{PEER}.note.put"))[..] else {
        panic!("the dispatch into itself is journaled once: {entries:#?}");
    };
    assert_eq!(refused.record.result, Outcome::Denied);
    assert_eq!(
        refused.record.deciding_policy.as_deref(),
        Some("kernel.reentrant")
    );
    let after = fixture.status(fixture.peer);
    assert_eq!(
        (after.generation(), after.crashes(), after.quarantined()),
        (before.generation() + 1, 0, false),
        "{after:?}"
    );
    assert_eq!(
        fixture.held(THE_PEER, "note/photo").await,
        Some(r#"{"text":"paired"}"#.to_owned()),
        "the pair's own write was not kept"
    );
    fixture.verifies().await;
}

/// A unit a kill let go names the compensations registered in it, none of
/// which runs: the command's own unit, and the unit of a command it was
/// nested in that answered.
///
/// The request a compensation is registered for waits on a transport that
/// never answers, so its plugin is killed at the host call's deadline after it
/// registered `note.put`. Dispatched at the top, the kill is the command's
/// refusal; nested in a relay that answers, it dooms the relay's unit. The
/// request was handed to the transport once either way, and what was meant to
/// set it right is named, since nothing will run it.
///
/// Found by a review: what an abort answered named no compensation, though the
/// abort's result is meant to name each one.
#[tokio::test(start_paused = true)]
async fn an_aborted_unit_names_the_compensations_that_did_not_run() {
    let post = Value::object([
        ("note", Value::text("photo")),
        ("url", Value::text("https://example.com/")),
    ])
    .to_canonical();
    let relayed = Value::object([
        ("args", Value::text(post.clone())),
        ("name", Value::text(format!("{PEER}.compensated.post"))),
        ("note", Value::text("photo")),
    ])
    .to_canonical();
    for (command, args, code, compensation) in [
        (
            "reference.compensated.post",
            post.clone(),
            ErrorCode::PluginHostCallDeadline,
            "reference.note.put".to_owned(),
        ),
        (
            "reference.relay.write",
            relayed,
            ErrorCode::KernelUnitAborted,
            format!("{PEER}.note.put"),
        ),
    ] {
        let fixture = opened("").await;
        let stall = fixture.transport.stall();
        let dispatched = fixture.as_ana(command, &args);
        tokio::pin!(dispatched);
        tokio::select! {
            biased;
            answered = &mut dispatched => panic!("`{command}` answered before the transport was reached: {answered:?}"),
            () = fixture.transport.reached() => {}
        }
        tokio::time::advance(Duration::from_millis(KERNEL.host_call_deadline_ms)).await;
        let refused = dispatched
            .await
            .expect_err("a unit a kill doomed is not kept");
        drop(stall);
        assert_eq!(refused.code(), code, "`{command}`: {refused}");
        assert!(
            refused.sentence().ends_with(&format!(
                "; the compensation(s) registered in it did not run: `{compensation}`"
            )),
            "`{command}`: {refused}"
        );
        assert_eq!(
            fixture.transport.asked().len(),
            1,
            "`{command}`: the request was not handed to the transport once"
        );
        assert_eq!(fixture.held(REFERENCE, "note/photo").await, None);
    }
}

/// A one-shot timer whose plugin the start before its callback quarantines is
/// held until the plugin is released, and fires then.
///
/// The peer sets two timers. The first callback dispatches the reference
/// plugin's `note.put`, whose reads of the note answer nineteen seconds late,
/// so the call tree's deadline kills both with no time left to start them
/// again in; the start before the second callback then waits on a store that
/// reads the peer's activation past a host call's deadline, and quarantines
/// it. **Written by a reviewer, who found a real bug with it**: the pump asked
/// whether the peer was stopped before starting it, so the second timer was
/// spent on a callback nobody could call — counted as fired and failed — and
/// the release found nothing to fire.
#[tokio::test]
async fn a_timer_whose_plugin_is_quarantined_as_it_fires_is_held_for_its_release() {
    let late = Duration::from_millis(KERNEL.host_call_deadline_ms - 1_000);
    let slow = Duration::from_millis(KERNEL.host_call_deadline_ms + 1_000);
    let fixture = opened("").await;
    for tag in ["put:reference:photo", "after"] {
        fixture
            .as_ana("peer.timer.set", &a_second_tagged(tag))
            .await
            .unwrap_or_else(|refused| panic!("the peer sets its timer `{tag}`: {refused}"));
    }
    fixture.clock.advance(1_000);
    fixture
        .store
        .set(Hang::NoteAndPeerActivationReadsFor(late, slow));
    let first = fixture.kernel.pump().await;
    fixture.store.set(Hang::Nothing);
    let peer = fixture.status(fixture.peer);
    assert!(
        peer.quarantined(),
        "{peer:?}: the start before its second callback waited past a host call's deadline; \
         pumped {first:?}"
    );
    assert_eq!(
        (first.fired, first.failed, first.suspended),
        (1, 1, 1),
        "the first callback was killed and the second timer held: {first:?}"
    );
    fixture
        .kernel
        .release(fixture.peer)
        .await
        .expect("the peer comes back over a store that answers");
    let second = fixture.kernel.pump().await;
    assert_eq!(
        (second.fired, second.failed),
        (1, 0),
        "the timer due while its plugin was quarantined was not held for its release: the \
         first pump {first:?}, the one after the release {second:?}"
    );
}

/// An event whose subscriber the start before its callback quarantines is
/// dropped and counted, not delivered.
///
/// The twin of the timer's: the peer's own timer kills it at its call tree's
/// deadline with no time left to start it again in, and the start before it
/// hears the event it subscribed to waits past a host call's deadline on the
/// read of its activation, which quarantines it.
#[tokio::test]
async fn an_event_whose_subscriber_is_quarantined_as_it_is_delivered_is_dropped() {
    let late = Duration::from_millis(KERNEL.host_call_deadline_ms - 1_000);
    let slow = Duration::from_millis(KERNEL.host_call_deadline_ms + 1_000);
    let fixture = opened_hearing("", &["peer.*"]).await;
    fixture
        .as_ana("peer.timer.set", &a_second_tagged("put:reference:photo"))
        .await
        .unwrap_or_else(|refused| panic!("the peer sets its timer: {refused}"));
    let published = Value::object([
        ("note", Value::text("photo")),
        ("topic", Value::text("peer.noted")),
    ])
    .to_canonical();
    fixture
        .as_ana("peer.event.publish", &published)
        .await
        .unwrap_or_else(|refused| panic!("the peer publishes its event: {refused}"));
    fixture.clock.advance(1_000);
    fixture
        .store
        .set(Hang::NoteAndPeerActivationReadsFor(late, slow));
    let pumped = fixture.kernel.pump().await;
    fixture.store.set(Hang::Nothing);
    let peer = fixture.status(fixture.peer);
    assert!(peer.quarantined(), "{peer:?}: pumped {pumped:?}");
    assert_eq!(
        (
            pumped.fired,
            pumped.failed,
            pumped.delivered,
            pumped.dropped
        ),
        (1, 1, 0, 1),
        "the event its quarantined subscriber would have heard was delivered: {pumped:?}"
    );
}

/// The arguments of a timer about `photo`, of a second, tagged `tag`.
fn a_second_tagged(tag: &str) -> String {
    Value::object([
        ("ms", Value::Integer(1_000)),
        ("note", Value::text("photo")),
        ("tag", Value::text(tag)),
    ])
    .to_canonical()
}
