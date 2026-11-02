//! The rest of the world: events, timers, surfaces, who a call runs for, a
//! plugin's own log, and the pump that fires and delivers what was put off
//! (`NOCTUA.md §11.3`, `NOCTUA-PLAN.md`'s Stage 11).
//!
//! **Through the real component, two of it, and a whole kernel.** The
//! reference plugin is loaded twice — as `reference` and as `peer`, each
//! subscribed to the other's topics — over the local database held in memory,
//! deciding with the family pack over `policy/world.json`, at a clock a test
//! moves. A directory beside the world's can take `ana`'s grant away, which is
//! how a test shows a timer or an event decided again when it comes due.
//!
//! **What a test asserts is what was kept.** A callback is shown to have run,
//! and as whom, by the lines it wrote to its plugin's log and the entries its
//! dispatches left in the journal; a change by what the store holds; a surface
//! by what the kernel lists.

use core::fmt::Debug;
use core::future::Future;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use nc_authz::{
    Directory, DirectoryError, EntityRef, MemoryDirectory, Pdp, Principal, PrincipalKind,
    RealmFacts,
};
use nc_canon::Value;
use nc_catalog::{ErrorCode, KERNEL};
use nc_ids::{MachineFlake, Minter, OpaqueId};
use nc_journal::{Entry, Outcome};
use nc_kernel::logging::Captured;
use nc_kernel::manifest::PluginManifest;
use nc_kernel::{
    Caller, Environment, Kernel, KernelConfig, LogLevel, PluginSlot, Pumped, Refusal, RefusalKind,
    SurfaceKind,
};
use nc_local::{Expect, LocalStore, Quotas, StagedWrite, UnitKind, UnitMeta, UnitRecord};
use nc_plugin_reference::behaviour::{ACTIVATION, Activation, Variant};

/// The world every policy case is about.
const WORLD: &str = include_str!("../../../policy/world.json");

/// The instant the policy cases call now, `2026-08-26T00:00:00Z`: inside the
/// grant `g.current`.
const NOW_MS: u64 = 1_787_702_400_000;

/// The identity of the plugin loaded under the namespace `reference`.
const REFERENCE: &str = "noctua:reference";

/// The identity of the same component loaded again, under `peer`.
const PEER: &str = "noctua:reference-peer";

/// What both plugins are granted: everything a note, a blob made, an event, a
/// timer, a surface and the context reach for.
const GRANTED: &str = r#"["store", "blobs.write", "commands.dispatch", "commands.begin", "events.publish", "events.subscribe", "timers", "ui", "context"]"#;

/// A clock a test moves, which moves on by itself only past what an
/// identifier asks it to.
#[derive(Debug, Clone)]
struct Manual(Arc<AtomicU64>);

impl Manual {
    /// Moves the clock on by `by`.
    fn advance(&self, by: u64) {
        self.0.fetch_add(by, Ordering::SeqCst);
    }
}

impl nc_ids::Clock for Manual {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }

    fn wait_past(&self, millis: u64) {
        self.0.fetch_max(millis.saturating_add(1), Ordering::SeqCst);
    }
}

/// The world's directory, which a test can make answer that `ana` holds no
/// grant: her authority withdrawn.
#[derive(Debug)]
struct Revoking {
    inner: MemoryDirectory,
    revoked: Arc<AtomicBool>,
}

impl Directory for Revoking {
    fn realm(
        &self,
        realm: &str,
    ) -> impl Future<Output = Result<RealmFacts, DirectoryError>> + Send {
        self.inner.realm(realm)
    }

    fn grant(
        &self,
        realm: &str,
        principal: &EntityRef,
    ) -> impl Future<Output = Result<Option<nc_authz::GrantFacts>, DirectoryError>> + Send {
        let revoked = self.revoked.load(Ordering::SeqCst) && principal.id == "ana";
        let answer = self.inner.grant(realm, principal);
        async move { if revoked { Ok(None) } else { answer.await } }
    }

    fn slice(
        &self,
        realm: &str,
        principal: &EntityRef,
        resource: &EntityRef,
        grant: Option<&EntityRef>,
    ) -> impl Future<Output = Result<nc_authz::Slice, DirectoryError>> + Send {
        self.inner.slice(realm, principal, resource, grant)
    }
}

/// What a test holds of the journal's openings of a unit: whether they stall
/// now, and the gate a stalled one waits at until the test hands it a permit.
#[derive(Debug, Clone)]
struct Stall {
    stalled: Arc<AtomicBool>,
    gate: Arc<tokio::sync::Semaphore>,
}

impl Stall {
    /// Openings that land at once, until a test stalls them.
    fn none() -> Self {
        Self {
            stalled: Arc::new(AtomicBool::new(false)),
            gate: Arc::new(tokio::sync::Semaphore::new(0)),
        }
    }
}

/// The journal in memory, whose every opening of a unit waits at the gate of
/// its [`Stall`] while the test stalls them, and whose every other entry lands
/// at once.
#[derive(Debug)]
struct Stalling {
    inner: nc_journal::InMemory,
    stall: Stall,
}

impl nc_journal::Store for Stalling {
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
        let stalls =
            self.stall.stalled.load(Ordering::SeqCst) && entry.record.result == Outcome::UnitBegun;
        let gate = Arc::clone(&self.stall.gate);
        let appended = self.inner.append(entry);
        async move {
            if stalls {
                gate.acquire().await.expect("never closed").forget();
            }
            appended.await
        }
    }

    fn from(
        &self,
        sequence: u64,
    ) -> impl Future<Output = Result<nc_journal::Read, nc_journal::Error>> + Send {
        self.inner.from(sequence)
    }

    fn unit_closing(
        &self,
        realm: &OpaqueId,
        unit: nc_ids::Snowflake,
    ) -> impl Future<Output = Result<Option<Outcome>, nc_journal::Error>> + Send {
        self.inner.unit_closing(realm, unit)
    }

    fn unit_opened(
        &self,
        realm: &OpaqueId,
        unit: nc_ids::Snowflake,
    ) -> impl Future<Output = Result<bool, nc_journal::Error>> + Send {
        self.inner.unit_opened(realm, unit)
    }
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
            std::env::temp_dir().join(format!("nc-kernel-world-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir(&root).expect("a scratch directory");
        Self(root)
    }

    /// The directory the local database is opened in, which opening it makes
    /// private.
    fn profile(&self) -> std::path::PathBuf {
        self.0.join("profile")
    }
}

/// The kernel this suite opens.
#[derive(Debug)]
struct Env;

impl Environment for Env {
    type Store = LocalStore;
    type Blobs = nc_kernel::blobs::MemoryBlobs;
    type Journal = Stalling;
    type Clock = Manual;
    type Directory = Revoking;
    type Log = Arc<Captured>;
    type Transport = nc_kernel::net::Scripted;
}

/// A kernel with both plugins loaded, and everything a test reads beside it.
struct Fixture {
    kernel: Kernel<Env>,
    store: LocalStore,
    log: Arc<Captured>,
    clock: Manual,
    revoked: Arc<AtomicBool>,
    /// What stalls the journal's openings of a unit.
    stall: Stall,
    world: MemoryDirectory,
    reference: PluginSlot,
    peer: PluginSlot,
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

/// The reference plugin's manifest as `id` under `namespace`, granted
/// [`GRANTED`] and subscribing to `heard`.
fn manifest(id: &str, namespace: &str, heard: &str) -> PluginManifest {
    PluginManifest::parse(&format!(
        "id = \"{id}\"\nnamespace = \"{namespace}\"\ncapabilities = {GRANTED}\n\n[events]\n\
         subscribe = [\"{heard}\"]\n"
    ))
    .expect("a manifest the kernel reads")
}

/// Commits `value` as `plugin`'s `key`, as a unit of `bruno`'s nobody
/// dispatched: not a step of `ana`, whose history a test reads.
async fn seeded(store: &LocalStore, plugin: &str, key: &str, value: &str) {
    /// The flake the next seed is minted with: each seed's own.
    static FLAKES: AtomicU64 = AtomicU64::new(40);
    let world = MemoryDirectory::from_world(WORLD, &family()).expect("the world is valid");
    let flake = u16::try_from(FLAKES.fetch_add(1, Ordering::SeqCst) % 1_000).expect("fits");
    let mut minter = Minter::new(
        MachineFlake::from_value(flake).expect("a flake"),
        Manual(Arc::new(AtomicU64::new(NOW_MS))),
    );
    let unit = UnitRecord {
        unit: minter.mint().expect("a unit"),
        meta: UnitMeta {
            realm: world.realm_id("home").expect("home is a realm"),
            principal: world
                .principal(PrincipalKind::User, "bruno")
                .expect("bruno is a user")
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
        lowered: BTreeMap::new(),
        blobs: 10,
        writes_per_unit: 10,
        notes_per_unit: 10,
    };
    store
        .commit(&unit, &quotas)
        .await
        .expect("the seed commits");
}

/// A kernel over the local database in memory, `reference` subscribed to the
/// peer's topics and `peer` to the reference's.
async fn opened() -> Fixture {
    let pack = family();
    let world = MemoryDirectory::from_world(WORLD, &pack).expect("the world is valid");
    let mut directory = MemoryDirectory::from_world(WORLD, &pack)
        .expect("the world is valid")
        .with_epoch("home", 7);
    for person in ["ana", "bruno", "carla"] {
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
    let store = LocalStore::in_memory_for_tests()
        .await
        .expect("the local database opens in memory");
    for (id, namespace, heard) in [
        (REFERENCE, "reference", "peer.*"),
        (PEER, "peer", "reference.*"),
    ] {
        let activation = Activation {
            namespace: namespace.to_owned(),
            variant: Variant::Normal,
            subscribe: vec![heard.to_owned()],
        };
        seeded(&store, id, ACTIVATION, &activation.written()).await;
    }
    let clock = Manual(Arc::new(AtomicU64::new(NOW_MS)));
    let log = Arc::new(Captured::default());
    let revoked = Arc::new(AtomicBool::new(false));
    let stall = Stall::none();
    let kernel = kernel_over(
        store.clone(),
        clock.clone(),
        Arc::clone(&log),
        Revoking {
            inner: directory,
            revoked: Arc::clone(&revoked),
        },
        stall.clone(),
    )
    .await;
    let component = nc_kernel::component_for_tests("reference");
    let reference = kernel
        .load(manifest(REFERENCE, "reference", "peer.*"), &component)
        .await
        .unwrap_or_else(|refused| panic!("the reference plugin loads: {refused}"));
    let peer = kernel
        .load(manifest(PEER, "peer", "reference.*"), &component)
        .await
        .unwrap_or_else(|refused| panic!("the peer loads: {refused}"));
    Fixture {
        kernel,
        store,
        log,
        clock,
        revoked,
        stall,
        world,
        reference,
        peer,
    }
}

/// A kernel over `store`, at `clock`, logging to `log`, deciding over
/// `directory` and journaling into a journal `stall` stalls, with no plugin
/// loaded.
async fn kernel_over(
    store: LocalStore,
    clock: Manual,
    log: Arc<Captured>,
    directory: Revoking,
    stall: Stall,
) -> Kernel<Env> {
    let world = MemoryDirectory::from_world(WORLD, &family()).expect("the world is valid");
    let keeper = world
        .principal(PrincipalKind::User, "ana")
        .expect("ana is a user")
        .id()
        .clone();
    Kernel::open(KernelConfig {
        store,
        blobs: nc_kernel::blobs::MemoryBlobs::default(),
        pdp: Ok(family()),
        journal: nc_journal::Journal::new(
            Stalling {
                inner: nc_journal::InMemory::default(),
                stall,
            },
            world.realm_id("home").expect("home is a realm"),
            keeper,
            Minter::new(MachineFlake::from_value(3).expect("a flake"), clock.clone()),
        ),
        directory,
        clock,
        log,
        transport: nc_kernel::net::Scripted::default(),
    })
    .await
    .expect("the kernel opens")
}

impl Fixture {
    /// The user called `name` in the world.
    fn principal(&self, name: &str) -> Principal {
        self.world
            .principal(PrincipalKind::User, name)
            .unwrap_or_else(|| panic!("{name} is a user of the world"))
    }

    /// The opaque identifier of the user called `name`, as `context` says it.
    fn id_of(&self, name: &str) -> String {
        self.principal(name).id().as_str().to_owned()
    }

    /// Dispatches `command` with `args` as the user called `name`.
    async fn as_(&self, name: &str, command: &str, args: &str) -> Result<String, Refusal> {
        self.kernel
            .dispatch(&Caller::new(self.principal(name)), command, args)
            .await
    }

    /// Dispatches `command` with `args` as `ana`, which must answer.
    async fn ana(&self, command: &str, args: &str) -> String {
        self.as_("ana", command, args)
            .await
            .unwrap_or_else(|refused| panic!("`{command}` answers: {refused}"))
    }

    /// Dispatches `reference.log.write` with `args` as `ana`, and holds the
    /// central log to the lines it had before.
    async fn logged_by_the_plugin(&self, args: &str) {
        let before = self.log.lines().len();
        self.ana("reference.log.write", args).await;
        assert_eq!(
            self.log.lines().len(),
            before,
            "a plugin's line reached the central log"
        );
    }

    /// Pumps once.
    async fn pump(&self) -> Pumped {
        self.kernel.pump().await
    }

    /// The texts of every line the plugin in `slot` logged, oldest first.
    fn lines(&self, slot: PluginSlot) -> Vec<String> {
        self.kernel
            .plugin_log(slot)
            .expect("a plugin is loaded there")
            .lines()
            .iter()
            .map(|line| line.text().to_owned())
            .collect()
    }

    /// The lines of the plugin in `slot` that open on `opening`.
    fn lines_opening(&self, slot: PluginSlot, opening: &str) -> Vec<String> {
        self.lines(slot)
            .into_iter()
            .filter(|line| line.starts_with(opening))
            .collect()
    }

    /// Every entry of the kernel's journal.
    async fn entries(&self) -> Vec<Entry> {
        self.kernel
            .journal_for_tests()
            .await
            .expect("the journal reads back")
    }

    /// What `plugin`'s `key` holds in the store, committed.
    async fn held(&self, plugin: &str, key: &str) -> Option<String> {
        self.store
            .get(&home(), plugin, key)
            .await
            .expect("the store answers")
            .map(|entry| entry.value)
    }

    /// Kills the plugin of `namespace` with a trap until it is quarantined.
    async fn quarantine(&self, namespace: &str, slot: PluginSlot) {
        for _ in 0..KERNEL.restart_budget {
            let trapped = self
                .as_(
                    "ana",
                    &format!("{namespace}.abuse.trap"),
                    r#"{"note":"photo"}"#,
                )
                .await;
            assert!(trapped.is_err(), "a trap answers nothing");
        }
        assert!(
            self.kernel
                .plugin_status(slot)
                .expect("loaded")
                .quarantined()
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

/// The text the field `name` of `answer` holds.
fn text(answer: &str, name: &str) -> String {
    field(answer, name)
        .into_text()
        .unwrap_or_else(|| panic!("`{name}` of {answer} is not text"))
}

/// The whole number the field `name` of `answer` holds.
fn number(answer: &str, name: &str) -> i64 {
    match field(answer, name) {
        Value::Integer(number) => number,
        other => panic!("`{name}` of {answer} is not a number: {other:?}"),
    }
}

/// The arguments of an event about `photo` under `topic`, with `extra`.
fn event(topic: &str, extra: &[(&str, Value)]) -> String {
    let mut fields = vec![
        ("note", Value::text("photo")),
        ("topic", Value::text(topic)),
    ];
    fields.extend(extra.iter().cloned());
    Value::object(fields).to_canonical()
}

/// The arguments of a timer about `photo`: `ms`, tagged `tag`, with `extra`.
fn timer(ms: i64, tag: &str, extra: &[(&str, Value)]) -> String {
    let mut fields = vec![
        ("ms", Value::Integer(ms)),
        ("note", Value::text("photo")),
        ("tag", Value::text(tag)),
    ];
    fields.extend(extra.iter().cloned());
    Value::object(fields).to_canonical()
}

/// The identifiers a `timer.set` answered.
fn timers_set(answer: &str) -> Vec<i64> {
    field(answer, "timers")
        .into_array()
        .unwrap_or_else(|| panic!("`timers` of {answer} is not a list"))
        .into_iter()
        .map(|id| match id {
            Value::Integer(id) => id,
            other => panic!("not an identifier: {other:?}"),
        })
        .collect()
}

/// Whether `refused` is of `kind`, carrying `code`.
fn is(refused: &Result<String, Refusal>, kind: RefusalKind, code: ErrorCode) -> bool {
    refused
        .as_ref()
        .err()
        .is_some_and(|refusal| refusal.kind() == kind && refusal.code() == code)
}

/// The entries of dispatches of `command`, brackets left out.
fn entries_of<'a>(entries: &'a [Entry], command: &str) -> Vec<&'a Entry> {
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

#[tokio::test]
async fn an_event_published_inside_an_aborted_unit_is_never_delivered() {
    let fixture = opened().await;
    let refused = fixture
        .as_(
            "ana",
            "reference.event.publish",
            &event("reference.noted", &[("then", Value::text("fail"))]),
        )
        .await;
    assert!(
        is(
            &refused,
            RefusalKind::Invalid,
            ErrorCode::PluginCommandRefused
        ),
        "{refused:?}"
    );
    let pumped = fixture.pump().await;
    assert_eq!(pumped.delivered, 0, "{pumped:?}");
    assert_eq!(
        fixture.lines_opening(fixture.peer, "heard"),
        [] as [String; 0]
    );
    // **The control**: the same event from a unit that is kept is heard,
    // once the pump runs and not before.
    fixture
        .ana("reference.event.publish", &event("reference.noted", &[]))
        .await;
    assert_eq!(
        fixture.lines_opening(fixture.peer, "heard"),
        [] as [String; 0]
    );
    assert_eq!(fixture.pump().await.delivered, 1);
    assert_eq!(
        fixture.lines_opening(fixture.peer, "heard"),
        [format!(
            "heard reference.noted {{}} for {}, confirmed false",
            fixture.id_of("ana")
        )]
    );
}

#[tokio::test]
async fn a_nested_command_that_published_or_set_a_timer_and_failed_dooms_its_unit() {
    // **A review found this unguarded.** A nested command that failed having
    // published, set or cancelled — and only held them, writing nothing — left
    // the unit to a caller that went on, and the caller's commit queued the
    // event, armed the timer and applied the cancel of a command that failed.
    let fixture = opened().await;
    let fails = [("then", Value::text("fail"))];
    let kept = timers_set(
        &fixture
            .ana("reference.timer.set", &timer(1_000, "kept", &[]))
            .await,
    );
    assert_eq!(kept.len(), 1, "{kept:?}");
    let cancel = Value::object([
        ("note", Value::text("photo")),
        ("then", Value::text("fail")),
        ("timer", Value::Integer(kept[0])),
    ])
    .to_canonical();
    // **Each beneath a relay that answers past a refusal**: `relay.write`
    // turns what it was refused into an answer of its own, so only the unit
    // tells the failed command's effects from a kept one's.
    for (name, args) in [
        ("reference.event.publish", event("reference.noted", &fails)),
        (
            "reference.timer.set",
            timer(1_000, "set-and-failed", &fails),
        ),
        ("reference.timer.cancel", cancel),
    ] {
        let relayed = Value::object([
            ("args", Value::text(args)),
            ("name", Value::text(name)),
            ("note", Value::text("photo")),
        ])
        .to_canonical();
        let refused = fixture.as_("ana", "peer.relay.write", &relayed).await;
        assert!(
            refused
                .as_ref()
                .is_err_and(|refusal| refusal.code() == ErrorCode::KernelUnitAborted),
            "`{name}` failed, and the unit it ran in was kept: {refused:?}"
        );
    }
    assert_eq!(
        fixture
            .log
            .named("kernel.unit_aborted")
            .iter()
            .filter_map(|line| line.text("cause"))
            .collect::<Vec<_>>(),
        ["nested", "nested", "nested"]
    );
    fixture.clock.advance(1_000);
    let pumped = fixture.pump().await;
    assert_eq!((pumped.delivered, pumped.fired), (0, 1), "{pumped:?}");
    assert_eq!(
        fixture.lines_opening(fixture.peer, "heard"),
        [] as [String; 0]
    );
    assert_eq!(
        fixture.lines_opening(fixture.reference, "timer "),
        [format!(
            "timer kept fired for {}, confirmed false",
            fixture.id_of("ana")
        )],
        "the timer whose cancel failed fires, and the one whose setter failed does not"
    );
}

#[tokio::test]
async fn an_event_is_delivered_once_in_the_order_published_and_as_its_publisher() {
    let fixture = opened().await;
    for n in 0..3 {
        let data = Value::object([("n", Value::Integer(n))]).to_canonical();
        fixture
            .as_(
                "bruno",
                "reference.event.announce",
                &event("reference.counted", &[("data", Value::text(data))]),
            )
            .await
            .expect("an event published from a read is queued at once");
    }
    let pumped = fixture.pump().await;
    assert_eq!((pumped.delivered, pumped.failed), (3, 0), "{pumped:?}");
    let bruno = fixture.id_of("bruno");
    assert_eq!(
        fixture.lines_opening(fixture.peer, "heard"),
        (0..3)
            .map(|n| format!("heard reference.counted {{\"n\":{n}}} for {bruno}, confirmed false"))
            .collect::<Vec<_>>()
    );
    let seen = fixture.lines_opening(fixture.reference, "heard");
    assert!(
        seen.is_empty(),
        "a plugin hears only what it subscribed to: {seen:?}"
    );
    assert_eq!(fixture.pump().await, Pumped::default(), "at most once");
}

#[tokio::test]
async fn an_event_published_while_the_pump_delivers_waits_for_the_next() {
    let fixture = opened().await;
    let relay = Value::object([("then", Value::text("relay"))]).to_canonical();
    fixture
        .ana(
            "reference.event.publish",
            &event("reference.noted", &[("data", Value::text(relay))]),
        )
        .await;
    assert_eq!(fixture.pump().await.delivered, 1);
    assert_eq!(
        fixture.lines_opening(fixture.peer, "relay"),
        [r#"relay "answered""#.to_owned()]
    );
    assert_eq!(
        fixture.lines_opening(fixture.reference, "heard"),
        [] as [String; 0]
    );
    assert_eq!(fixture.pump().await.delivered, 1, "the relayed event, next");
    assert_eq!(
        fixture.lines_opening(fixture.reference, "heard"),
        [format!(
            "heard peer.relayed {{\"from\":\"reference.noted\"}} for {}, confirmed false",
            fixture.id_of("ana")
        )]
    );
}

#[tokio::test]
async fn an_events_callback_only_reads_and_what_it_dispatches_is_journaled_as_the_publisher() {
    let fixture = opened().await;
    let write = Value::object([("then", Value::text("write"))]).to_canonical();
    fixture
        .ana(
            "reference.event.publish",
            &event("reference.noted", &[("data", Value::text(write))]),
        )
        .await;
    assert_eq!(fixture.pump().await.delivered, 1);
    let written = fixture.lines_opening(fixture.peer, "write");
    assert!(
        written.len() == 1
            && written[0].contains(&format!("denied: [{}]", ErrorCode::KernelReadOnly)),
        "{written:?}"
    );
    assert_eq!(fixture.held(REFERENCE, "note/photo").await, None);
    let entries = fixture.entries().await;
    let refused = entries_of(&entries, "reference.note.put");
    assert_eq!(refused.len(), 1, "{refused:#?}");
    assert_eq!(refused[0].record.result, Outcome::Denied);
    assert_eq!(
        refused[0].record.deciding_policy.as_deref(),
        Some("kernel.read_only")
    );
    assert_eq!(refused[0].record.principal, *fixture.principal("ana").id());
}

#[tokio::test]
async fn what_a_callback_is_refused_is_logged_once_it_returns_for_the_plugin_it_called() {
    // **A review found this unguarded**: no test noticed the callback's frame
    // never taken back, and what it was refused never logged.
    let fixture = opened().await;
    let fetch = Value::object([("then", Value::text("fetch"))]).to_canonical();
    fixture
        .ana(
            "reference.event.publish",
            &event("reference.noted", &[("data", Value::text(fetch))]),
        )
        .await;
    assert_eq!(
        fixture.log.named("kernel.capability_denied"),
        [] as [nc_kernel::logging::CapturedLine; 0]
    );
    assert_eq!(fixture.pump().await.delivered, 1);
    let fetched = fixture.lines_opening(fixture.peer, "fetch");
    assert!(
        fetched.len() == 1
            && fetched[0].contains(&format!("denied: [{}]", ErrorCode::PluginCapabilityDenied)),
        "{fetched:?}"
    );
    let logged = fixture.log.named("kernel.capability_denied");
    assert_eq!(logged.len(), 1, "{logged:#?}");
    let peer = fixture.peer.index().to_string();
    assert_eq!(
        (
            logged[0].text("slot").as_deref(),
            logged[0].text("capability").as_deref(),
            logged[0].text("code").as_deref(),
        ),
        (
            Some(peer.as_str()),
            Some("net"),
            Some(ErrorCode::PluginCapabilityDenied.code())
        ),
        "the subscriber's slot, which was refused, and not the publisher's"
    );
}

#[tokio::test]
async fn a_topic_outside_the_publishers_namespace_or_no_topic_at_all_is_refused() {
    let fixture = opened().await;
    for (topic, case) in [("peer.noted", "denied"), ("Reference.Noted", "invalid")] {
        let answered = fixture
            .ana("reference.event.announce", &event(topic, &[]))
            .await;
        assert_eq!(number(&answered, "published"), 0, "{answered}");
        assert!(
            text(&answered, "refused")
                .starts_with(&format!("{case}: [{}] ", ErrorCode::PluginTopicRefused)),
            "{answered}"
        );
    }
    // **And logged, once for the code**, as a host call refused for whose
    // name it used: the two topics are two refusals of one code, in one
    // instance.
    let logged = fixture.log.named("kernel.host_call_refused");
    assert_eq!(
        logged
            .iter()
            .map(|line| (line.text("slot"), line.text("code")))
            .collect::<Vec<_>>(),
        [(
            Some(fixture.reference.index().to_string()),
            Some(ErrorCode::PluginTopicRefused.code().to_owned())
        )],
        "{logged:#?}"
    );
    let float = fixture
        .ana(
            "reference.event.announce",
            &event("reference.noted", &[("data", Value::text("1.5"))]),
        )
        .await;
    assert!(
        text(&float, "refused").starts_with(&format!("invalid: [{}] ", ErrorCode::CanonNumber)),
        "{float}"
    );
    assert_eq!(fixture.pump().await.delivered, 0, "nothing was queued");
}

#[tokio::test]
async fn the_event_queue_holds_no_more_events_and_no_more_bytes_than_its_bounds() {
    let fixture = opened().await;
    let bound = i64::from(KERNEL.event_queue);
    // **Held in a unit, counted as queued**: the events a unit holds take
    // the room they will need when it is kept.
    let held = fixture
        .ana(
            "reference.event.publish",
            &event("reference.noted", &[("times", Value::Integer(bound + 1))]),
        )
        .await;
    assert_eq!(number(&held, "published"), bound, "{held}");
    assert!(
        text(&held, "refused").starts_with(&format!(
            "unavailable: [{}] ",
            ErrorCode::PluginBoundReached
        )),
        "{held}"
    );
    let full = fixture
        .ana("reference.event.announce", &event("reference.noted", &[]))
        .await;
    assert_eq!(number(&full, "published"), 0, "the queue is full: {full}");
    // **And full for every plugin**: the bounds are the kernel's, not a share
    // each publisher holds, so the peer is refused until the pump empties the
    // queue — what `events.rs` records as not answered.
    let peers = fixture
        .ana("peer.event.announce", &event("peer.noted", &[]))
        .await;
    assert_eq!(number(&peers, "published"), 0, "{peers}");
    assert!(
        text(&peers, "refused").starts_with(&format!(
            "unavailable: [{}] ",
            ErrorCode::PluginBoundReached
        )),
        "{peers}"
    );
    let pumped = fixture.pump().await;
    assert_eq!(pumped.delivered, u32::try_from(bound).expect("fits"));
    // **And bytes**: events of a megabyte each, fewer than the count allows.
    let big = fixture
        .ana(
            "reference.event.announce",
            &event(
                "reference.big",
                &[
                    ("fill", Value::Integer(1_000_000)),
                    ("times", Value::Integer(100)),
                ],
            ),
        )
        .await;
    let one =
        u64::try_from("reference.big".len() + r#"{"fill":""}"#.len() + 1_000_000).expect("fits");
    let fits = i64::try_from(KERNEL.event_queue_bytes / one).expect("fits");
    assert_eq!(number(&big, "published"), fits, "{big}");
    assert!(text(&big, "refused").contains("bytes"), "{big}");
    assert_eq!(
        fixture.pump().await.delivered,
        u32::try_from(fits).expect("fits")
    );
}

#[tokio::test]
async fn an_event_a_quarantined_plugin_would_hear_is_dropped_and_counted() {
    let fixture = opened().await;
    fixture.quarantine("peer", fixture.peer).await;
    fixture
        .ana("reference.event.publish", &event("reference.noted", &[]))
        .await;
    let pumped = fixture.pump().await;
    assert_eq!((pumped.delivered, pumped.dropped), (0, 1), "{pumped:?}");
}

#[tokio::test]
async fn a_callback_that_traps_is_killed_and_restarted_like_a_command() {
    let fixture = opened().await;
    let trap = Value::object([("then", Value::text("trap"))]).to_canonical();
    fixture
        .ana(
            "reference.event.publish",
            &event("reference.noted", &[("data", Value::text(trap))]),
        )
        .await;
    let pumped = fixture.pump().await;
    assert_eq!((pumped.delivered, pumped.failed), (1, 1), "{pumped:?}");
    let status = fixture.kernel.plugin_status(fixture.peer).expect("loaded");
    assert_eq!(
        (status.crashes(), status.generation(), status.quarantined()),
        (1, 1, false)
    );
    assert_eq!(
        fixture.log.named("kernel.plugin_killed")[0]
            .text("slot")
            .as_deref(),
        Some("1")
    );
    // **And the timer's**, the same way.
    fixture
        .ana("reference.timer.set", &timer(1_000, "trap", &[]))
        .await;
    fixture.clock.advance(1_000);
    let pumped = fixture.pump().await;
    assert_eq!((pumped.fired, pumped.failed), (1, 1), "{pumped:?}");
    assert_eq!(
        fixture
            .kernel
            .plugin_status(fixture.reference)
            .expect("loaded")
            .crashes(),
        1
    );
}

#[tokio::test]
async fn an_event_whose_publisher_lost_the_authority_is_dropped_and_journaled() {
    let fixture = opened().await;
    fixture
        .ana("reference.event.publish", &event("reference.noted", &[]))
        .await;
    fixture.revoked.store(true, Ordering::SeqCst);
    let pumped = fixture.pump().await;
    assert_eq!((pumped.delivered, pumped.refused), (0, 1), "{pumped:?}");
    assert_eq!(
        fixture.lines_opening(fixture.peer, "heard"),
        [] as [String; 0]
    );
    let entries = fixture.entries().await;
    let denied: Vec<&Entry> = entries_of(&entries, "reference.event.publish")
        .into_iter()
        .filter(|entry| entry.record.result == Outcome::Denied)
        .collect();
    assert_eq!(denied.len(), 1, "{entries:#?}");
    assert_eq!(denied[0].record.action, "Write");
    assert_eq!(
        denied[0].record.resource.to_canonical(),
        r#"{"command":"reference.event.publish","entity":{"id":"photo","type":"Content"}}"#,
        "the resource it was published on"
    );
    assert_eq!(denied[0].record.principal, *fixture.principal("ana").id());
    assert_eq!(
        denied[0].record.deciding_policy.as_deref(),
        Some("core.provenance")
    );
}

#[tokio::test]
async fn a_timer_fires_on_pump_as_its_setters_principal() {
    let fixture = opened().await;
    let set = fixture
        .ana("reference.timer.set", &timer(1_000, "put:peer:photo", &[]))
        .await;
    assert_eq!(timers_set(&set).len(), 1, "{set}");
    assert_eq!(fixture.pump().await.fired, 0, "not due yet");
    fixture.clock.advance(999);
    assert_eq!(fixture.pump().await.fired, 0, "a millisecond short");
    fixture.clock.advance(1);
    let pumped = fixture.pump().await;
    assert_eq!((pumped.fired, pumped.failed), (1, 0), "{pumped:?}");
    let ana = fixture.id_of("ana");
    assert_eq!(
        fixture.lines_opening(fixture.reference, "timer"),
        [format!(
            "timer put:peer:photo fired for {ana}, confirmed false"
        )]
    );
    assert_eq!(
        fixture.held(PEER, "note/photo").await.as_deref(),
        Some(r#"{"text":"fired"}"#)
    );
    let entries = fixture.entries().await;
    let put = entries_of(&entries, "peer.note.put");
    assert_eq!(put.len(), 1);
    assert_eq!(put[0].record.result, Outcome::Allowed);
    assert_eq!(put[0].record.principal, *fixture.principal("ana").id());
    assert!(put[0].record.work_unit.is_some(), "a step of its own");
    assert_eq!(fixture.pump().await.fired, 0, "a one-shot fires once");
}

#[tokio::test]
async fn a_timer_is_set_only_within_its_bounds() {
    let fixture = opened().await;
    let max = i64::try_from(KERNEL.timer_max_delay_ms).expect("fits");
    let min = i64::try_from(KERNEL.timer_min_interval_ms).expect("fits");
    let every = || ("every", Value::Bool(true));
    for (ms, extra) in [
        (0, vec![]),
        (max + 1, vec![]),
        (min - 1, vec![every()]),
        (max + 1, vec![every()]),
    ] {
        let refused = fixture
            .ana("reference.timer.set", &timer(ms, "t", &extra))
            .await;
        assert!(
            text(&refused, "refused")
                .starts_with(&format!("invalid: [{}] ", ErrorCode::ValidationOutOfRange)),
            "{ms}: {refused}"
        );
    }
    for (ms, extra) in [(1, vec![]), (max, vec![]), (min, vec![every()])] {
        let set = fixture
            .ana("reference.timer.set", &timer(ms, "t", &extra))
            .await;
        assert_eq!(timers_set(&set).len(), 1, "{ms}: {set}");
    }
    let tag = "t".repeat(64);
    let set = fixture
        .ana("reference.timer.set", &timer(1, &tag, &[]))
        .await;
    assert_eq!(timers_set(&set).len(), 1, "a tag of the bound: {set}");
    let refused = fixture
        .ana("reference.timer.set", &timer(1, &format!("{tag}t"), &[]))
        .await;
    assert!(text(&refused, "refused").contains("tag"), "{refused}");
}

#[tokio::test]
async fn a_plugin_holds_no_more_timers_than_its_bound_those_its_unit_holds_counted() {
    let fixture = opened().await;
    let bound = i64::from(KERNEL.timers_per_plugin);
    let set = fixture
        .ana(
            "reference.timer.set",
            &timer(1_000, "log", &[("times", Value::Integer(bound + 1))]),
        )
        .await;
    let ids = timers_set(&set);
    assert_eq!(ids.len(), usize::try_from(bound).expect("fits"), "{set}");
    assert!(
        ids.windows(2).all(|pair| pair[0] < pair[1]),
        "identifiers never go back: {ids:?}"
    );
    assert!(
        text(&set, "refused").starts_with(&format!(
            "unavailable: [{}] ",
            ErrorCode::PluginBoundReached
        )),
        "{set}"
    );
    let more = fixture
        .ana("reference.timer.set", &timer(1_000, "log", &[]))
        .await;
    assert!(timers_set(&more).is_empty(), "armed, at the bound: {more}");
    let peer = fixture
        .ana("peer.timer.set", &timer(1_000, "log", &[]))
        .await;
    assert_eq!(timers_set(&peer).len(), 1, "another plugin's room");
}

#[tokio::test]
async fn a_repeating_timer_fires_at_its_interval_counted_from_when_it_fired() {
    let fixture = opened().await;
    fixture
        .ana(
            "reference.timer.set",
            &timer(1_000, "log", &[("every", Value::Bool(true))]),
        )
        .await;
    fixture.clock.advance(1_000);
    assert_eq!(fixture.pump().await.fired, 1);
    fixture.clock.advance(500);
    assert_eq!(fixture.pump().await.fired, 0);
    fixture.clock.advance(500);
    assert_eq!(fixture.pump().await.fired, 1);
    fixture.clock.advance(5_000);
    assert_eq!(fixture.pump().await.fired, 1, "missed intervals fire once");
    assert_eq!(fixture.lines_opening(fixture.reference, "timer").len(), 3);
}

#[tokio::test]
async fn a_timers_callback_holding_no_unit_sets_and_cancels_timers_at_once() {
    let fixture = opened().await;
    let doomed = fixture
        .ana("reference.timer.set", &timer(2_000, "log", &[]))
        .await;
    let id = timers_set(&doomed)[0];
    fixture
        .ana(
            "reference.timer.set",
            &timer(1_000, &format!("cancel:{id}"), &[]),
        )
        .await;
    fixture
        .ana("reference.timer.set", &timer(1_000, "again:500", &[]))
        .await;
    fixture.clock.advance(1_000);
    assert_eq!(fixture.pump().await.fired, 2);
    assert_eq!(
        fixture.lines_opening(fixture.reference, "cancel"),
        [r#"cancel "answered""#.to_owned()]
    );
    assert_eq!(
        fixture.lines_opening(fixture.reference, "again"),
        [r#"again "answered""#.to_owned()]
    );
    fixture.clock.advance(500);
    assert_eq!(
        fixture.pump().await.fired,
        1,
        "the timer the callback set fires"
    );
    fixture.clock.advance(500);
    assert_eq!(
        fixture.pump().await.fired,
        0,
        "the timer the callback cancelled never does"
    );
    // **And a cancel of nothing it holds, from a callback, is not found.**
    fixture
        .ana("reference.timer.set", &timer(1, "cancel:999999", &[]))
        .await;
    fixture.clock.advance(1);
    fixture.pump().await;
    let cancelled = fixture.lines_opening(fixture.reference, "cancel");
    assert!(
        cancelled.last().is_some_and(
            |line| line.contains(&format!("not-found: [{}]", ErrorCode::PluginNotFound))
        ),
        "{cancelled:?}"
    );
}

#[tokio::test]
async fn a_cancel_naming_a_timer_another_plugin_holds_or_none_is_not_found() {
    let fixture = opened().await;
    let set = fixture
        .ana("reference.timer.set", &timer(1_000, "log", &[]))
        .await;
    let id = timers_set(&set)[0];
    let cancel = |id: i64| {
        Value::object([
            ("note", Value::text("photo")),
            ("timer", Value::Integer(id)),
        ])
        .to_canonical()
    };
    let not_found = format!("not-found: [{}] ", ErrorCode::PluginNotFound);
    for (command, id) in [
        ("peer.timer.cancel", id),
        ("reference.timer.cancel", id + 1_000),
    ] {
        let answered = fixture.ana(command, &cancel(id)).await;
        assert!(
            text(&answered, "cancelled").starts_with(&not_found),
            "{command} {id}: {answered}"
        );
    }
    let answered = fixture.ana("reference.timer.cancel", &cancel(id)).await;
    assert_eq!(text(&answered, "cancelled"), "answered");
    let again = fixture.ana("reference.timer.cancel", &cancel(id)).await;
    assert!(
        text(&again, "cancelled").starts_with(&not_found),
        "cancelled already"
    );
    fixture.clock.advance(1_000);
    assert_eq!(fixture.pump().await.fired, 0);
}

#[tokio::test]
async fn a_timer_set_or_cancelled_in_a_unit_that_is_not_kept_is_as_it_was() {
    let fixture = opened().await;
    let refused = fixture
        .as_(
            "ana",
            "reference.timer.set",
            &timer(1_000, "log", &[("then", Value::text("fail"))]),
        )
        .await;
    assert!(refused.is_err());
    let kept = fixture
        .ana("reference.timer.set", &timer(1_000, "put:peer:photo", &[]))
        .await;
    let id = timers_set(&kept)[0];
    let cancelled = fixture
        .as_(
            "ana",
            "reference.timer.cancel",
            &Value::object([
                ("note", Value::text("photo")),
                ("then", Value::text("fail")),
                ("timer", Value::Integer(id)),
            ])
            .to_canonical(),
        )
        .await;
    assert!(cancelled.is_err());
    fixture.clock.advance(1_000);
    let pumped = fixture.pump().await;
    assert_eq!(
        pumped.fired, 1,
        "the timer the aborted unit set never fires, and the one it cancelled still does"
    );
    assert_eq!(
        fixture
            .lines_opening(fixture.reference, "timer put:peer:photo")
            .len(),
        1
    );
}

#[tokio::test]
async fn a_timer_outlives_its_plugins_restart_and_waits_out_its_quarantine() {
    let fixture = opened().await;
    fixture
        .ana("reference.timer.set", &timer(1_000, "log", &[]))
        .await;
    assert!(
        fixture
            .as_("ana", "reference.abuse.trap", r#"{"note":"photo"}"#)
            .await
            .is_err()
    );
    fixture.clock.advance(1_000);
    assert_eq!(
        fixture.pump().await.fired,
        1,
        "it fires in the new instance"
    );
    fixture
        .ana("reference.timer.set", &timer(1_000, "log", &[]))
        .await;
    fixture.quarantine("reference", fixture.reference).await;
    fixture.clock.advance(1_000);
    let pumped = fixture.pump().await;
    assert_eq!((pumped.fired, pumped.suspended), (0, 1), "{pumped:?}");
    assert_eq!(fixture.pump().await.suspended, 1, "held, not lost");
    fixture
        .kernel
        .release(fixture.reference)
        .await
        .expect("it comes back");
    let pumped = fixture.pump().await;
    assert_eq!((pumped.fired, pumped.suspended), (1, 0), "{pumped:?}");
}

#[tokio::test]
async fn a_timer_whose_setter_lost_the_authority_is_cancelled_and_journaled() {
    let fixture = opened().await;
    fixture
        .ana(
            "reference.timer.set",
            &timer(1_000, "put:peer:photo", &[("every", Value::Bool(true))]),
        )
        .await;
    fixture.revoked.store(true, Ordering::SeqCst);
    fixture.clock.advance(1_000);
    let pumped = fixture.pump().await;
    assert_eq!((pumped.fired, pumped.refused), (0, 1), "{pumped:?}");
    assert_eq!(fixture.held(PEER, "note/photo").await, None);
    let entries = fixture.entries().await;
    let denied: Vec<&Entry> = entries_of(&entries, "reference.timer.set")
        .into_iter()
        .filter(|entry| entry.record.result == Outcome::Denied)
        .collect();
    assert_eq!(denied.len(), 1, "{entries:#?}");
    // **The authority the timer was set under, and no other.** The grant is
    // withdrawn whole, so any action would be denied here: only these two
    // say that what was decided again is what was allowed when it was set.
    assert_eq!(
        denied[0].record.action, "Write",
        "the action it was set under"
    );
    assert_eq!(
        denied[0].record.resource.to_canonical(),
        r#"{"command":"reference.timer.set","entity":{"id":"photo","type":"Content"}}"#,
        "the resource it was set on"
    );
    assert_eq!(
        denied[0].record.deciding_policy.as_deref(),
        Some("core.provenance")
    );
    fixture.revoked.store(false, Ordering::SeqCst);
    fixture.clock.advance(1_000);
    assert_eq!(
        fixture.pump().await,
        Pumped::default(),
        "a repeating timer refused once is cancelled"
    );
}

#[tokio::test]
async fn a_timer_set_where_only_reading_is_allowed_fires_where_only_reading_is() {
    // **A review found this.** Every timer's callback could change state, so
    // a read — or an event's callback, which only reads — set a timer whose
    // callback wrote another plugin's note as the person a millisecond later:
    // the frame that would have refused the write, got around by putting the
    // write off.
    let fixture = opened().await;
    // **From a command that only reads**: one timer dispatches the peer's
    // `note.put`, the other begins a unit and dispatches two commands in it.
    for tag in ["put:peer:photo", "group:peer:photo"] {
        let set = fixture
            .ana("reference.timer.remind", &timer(1, tag, &[]))
            .await;
        assert_eq!(timers_set(&set).len(), 1, "{set}");
    }
    // **From an event's callback**: the peer hears a change and sets a timer
    // that dispatches the publisher's `note.put`.
    let defer = Value::object([("then", Value::text("defer"))]).to_canonical();
    fixture
        .ana(
            "reference.event.publish",
            &event("reference.noted", &[("data", Value::text(defer))]),
        )
        .await;
    assert_eq!(fixture.pump().await.delivered, 1);
    assert_eq!(
        fixture.lines_opening(fixture.peer, "defer"),
        [r#"defer "answered""#.to_owned()],
        "a timer is set where only reading is allowed"
    );
    fixture.clock.advance(1);
    let pumped = fixture.pump().await;
    assert_eq!((pumped.fired, pumped.failed), (3, 0), "{pumped:?}");
    let read_only = format!("denied: [{}]", ErrorCode::KernelReadOnly);
    let put = fixture.lines_opening(fixture.reference, "put");
    assert!(put.len() == 1 && put[0].contains(&read_only), "{put:?}");
    let group = fixture.lines_opening(fixture.reference, "group");
    assert!(
        group.len() == 1
            && group[0].matches(&read_only).count() == 2
            && group[0].contains(&format!("denied: [{}]", ErrorCode::PluginReadOnlyFrame)),
        "both dispatches refused, and the unit never begun: {group:?}"
    );
    let deferred = fixture.lines_opening(fixture.peer, "put");
    assert!(
        deferred.len() == 1 && deferred[0].contains(&read_only),
        "{deferred:?}"
    );
    assert_eq!(fixture.held(PEER, "note/photo").await, None);
    assert_eq!(fixture.held(REFERENCE, "note/photo").await, None);
    let entries = fixture.entries().await;
    for command in ["peer.note.put", "peer.note.put-many", "reference.note.put"] {
        let refused = entries_of(&entries, command);
        assert!(
            !refused.is_empty()
                && refused.iter().all(|entry| {
                    entry.record.result == Outcome::Denied
                        && entry.record.deciding_policy.as_deref() == Some("kernel.read_only")
                }),
            "`{command}`: {refused:#?}"
        );
    }
    // **The control**: the same timer set by a change writes when it fires.
    fixture
        .ana("reference.timer.set", &timer(1, "put:peer:photo", &[]))
        .await;
    fixture.clock.advance(1);
    assert_eq!(fixture.pump().await.fired, 1);
    assert_eq!(
        fixture.held(PEER, "note/photo").await.as_deref(),
        Some(r#"{"text":"fired"}"#)
    );
}

#[tokio::test]
async fn a_timers_callback_keeps_what_it_dispatches_in_a_unit_it_began_as_one_step() {
    let fixture = opened().await;
    let realm = fixture.world.realm_id("home").expect("home is a realm");
    let ana = fixture.principal("ana");
    fixture
        .ana(
            "reference.timer.set",
            &timer(1_000, "group:peer:photo", &[]),
        )
        .await;
    let before = fixture
        .store
        .units(&realm, ana.id(), 100)
        .await
        .expect("the store answers")
        .len();
    fixture.clock.advance(1_000);
    let pumped = fixture.pump().await;
    assert_eq!((pumped.fired, pumped.failed), (1, 0), "{pumped:?}");
    let units = fixture
        .store
        .units(&realm, ana.id(), 100)
        .await
        .expect("the store answers");
    assert_eq!(units.len(), before + 1, "two dispatches, one step");
    assert_eq!(units[0].meta.label, "Fired in a group");
    for key in ["note/photo", "note/photo/0", "note/photo/1"] {
        assert_eq!(
            fixture.held(PEER, key).await.as_deref(),
            Some(r#"{"text":"grouped"}"#),
            "{key}"
        );
    }
    let entries = fixture.entries().await;
    let unit = entries_of(&entries, "peer.note.put-many")[0]
        .record
        .work_unit;
    assert_eq!(
        entries_of(&entries, "peer.note.put")[0].record.work_unit,
        unit
    );
    assert!(
        entries
            .iter()
            .any(|entry| entry.record.action == nc_journal::unit::OPENS
                && entry.record.work_unit == unit
                && entry.record.resource == Value::text(REFERENCE)),
        "the journal's unit is labelled by the plugin's identity"
    );
    // **And one it let go keeps nothing.**
    fixture
        .ana("reference.timer.set", &timer(1_000, "drop:peer:photo", &[]))
        .await;
    fixture.clock.advance(1_000);
    let pumped = fixture.pump().await;
    assert_eq!((pumped.fired, pumped.failed), (1, 1), "{pumped:?}");
    assert_eq!(
        fixture.held(PEER, "note/photo").await.as_deref(),
        Some(r#"{"text":"grouped"}"#),
        "what the dropped unit wrote was not kept"
    );
    assert!(
        fixture
            .log
            .named("kernel.unit_aborted")
            .iter()
            .any(|line| line.text("cause").as_deref() == Some("dropped"))
    );
}

#[tokio::test]
async fn a_timers_callback_killed_in_the_unit_it_began_lets_the_unit_go() {
    // **A review found this untested.** A callback killed after its `begin`
    // could leave its unit open in the journal, unlogged, and the bytes of a
    // blob made in it on disk, and no test would have said so.
    let fixture = opened().await;
    fixture
        .ana(
            "reference.timer.set",
            &timer(1_000, "trapped:peer:photo", &[]),
        )
        .await;
    fixture.clock.advance(1_000);
    let pumped = fixture.pump().await;
    assert_eq!((pumped.fired, pumped.failed), (1, 1), "{pumped:?}");
    assert_eq!(
        fixture.lines_opening(fixture.reference, "trapped"),
        [r#"trapped "answered" "answered" "answered""#.to_owned()],
        "the unit was begun, and a note and a blob made in it, before the trap"
    );
    assert_eq!(
        fixture
            .kernel
            .plugin_status(fixture.reference)
            .expect("loaded")
            .crashes(),
        1
    );
    assert_eq!(fixture.held(PEER, "note/photo").await, None);
    assert!(
        fixture
            .log
            .named("kernel.unit_aborted")
            .iter()
            .any(|line| line.text("cause").as_deref() == Some("failed")),
        "the unit the killed callback began is closed as aborted"
    );
    let entries = fixture.entries().await;
    let unit = entries_of(&entries, "peer.note.put")[0].record.work_unit;
    assert!(
        unit.is_some(),
        "the note was written in the callback's unit"
    );
    assert!(
        entries
            .iter()
            .any(|entry| entry.record.action == nc_journal::unit::CLOSES
                && entry.record.work_unit == unit
                && entry.record.result == Outcome::UnitAborted),
        "the journal closes the unit as aborted: {entries:#?}"
    );
    let swept = fixture.kernel.sweep_blobs().await.expect("the blobs sweep");
    assert_eq!(
        swept.orphans, 0,
        "the bytes of the blob made in the unit went with it: {swept:?}"
    );
}

/// Sets a timer of the reference plugin tagged `tag`, makes it due, and stalls
/// every opening of a unit the journal is given from then on.
async fn due_and_stalled(fixture: &Fixture, tag: &str) {
    fixture
        .ana("reference.timer.set", &timer(1_000, tag, &[]))
        .await;
    fixture.clock.advance(1_000);
    fixture.stall.stalled.store(true, Ordering::SeqCst);
}

/// Lets every stalled opening through and stalls none after it.
fn unstalled(fixture: &Fixture) {
    fixture.stall.stalled.store(false, Ordering::SeqCst);
    fixture.stall.gate.add_permits(1_024);
}

/// The longest a pump may be held by a callback whose tree's time is up: the
/// tree's deadline, and a margin for a loaded machine.
fn past_the_trees_deadline() -> std::time::Duration {
    std::time::Duration::from_millis(KERNEL.dispatch_deadline_ms + 5_000)
}

/// A timer's callback that opens the unit of its step with `begin`, on a
/// journal that stalls every opening of a unit, is killed at the host call's
/// deadline, and the pump ends.
///
/// The control beside the test below: `commands#begin` is a host call held to
/// a deadline of its own. Measured on the monotonic clock, which no test
/// pauses, so it waits that deadline out.
#[tokio::test]
async fn a_callbacks_begin_whose_journal_stalls_is_killed_and_lets_the_pump_go() {
    let fixture = opened().await;
    due_and_stalled(&fixture, "group:peer:photo").await;
    let started = std::time::Instant::now();
    let pumped = tokio::time::timeout(past_the_trees_deadline(), fixture.pump()).await;
    let waited = started.elapsed();
    unstalled(&fixture);
    let pumped = pumped.unwrap_or_else(|_| panic!("the pump was still held after {waited:?}"));
    assert_eq!((pumped.fired, pumped.failed), (1, 1), "{pumped:?}");
    assert!(
        waited < std::time::Duration::from_millis(KERNEL.dispatch_deadline_ms),
        "{waited:?}"
    );
    assert_eq!(
        fixture
            .kernel
            .plugin_status(fixture.reference)
            .and_then(|status| status.last_kill()),
        Some(nc_kernel::Kill::HostCallDeadline)
    );
    assert_eq!(fixture.held(PEER, "note/photo").await, None);
}

/// The same callback opening the same step's unit by dispatching a command
/// that changes state is told the journal did not answer, once a host call
/// would have waited as long, and lets the pump go within its tree's
/// deadline — nothing of the step kept, and nothing killed for it.
///
/// **A reviewer found a real bug with it**: the dispatch nested in the
/// callback waited on the journal with no bound, so the pump — and the bus
/// every call tree takes — was held for as long as the journal stalled, past
/// the callback's tree's deadline, the plugin never stopped for it.
#[tokio::test]
async fn a_callbacks_dispatch_whose_journal_stalls_lets_the_pump_go_within_its_trees_deadline() {
    let fixture = opened().await;
    due_and_stalled(&fixture, "put:peer:photo").await;
    let started = std::time::Instant::now();
    let pumped = tokio::time::timeout(past_the_trees_deadline(), fixture.pump()).await;
    let waited = started.elapsed();
    unstalled(&fixture);
    let pumped = pumped.unwrap_or_else(|_| {
        panic!(
            "the pump was still held {waited:?} after it began, past its callback tree's {} ms",
            KERNEL.dispatch_deadline_ms
        )
    });
    assert_eq!((pumped.fired, pumped.failed), (1, 0), "{pumped:?}");
    assert!(
        (std::time::Duration::from_millis(KERNEL.host_call_deadline_ms)
            ..std::time::Duration::from_millis(KERNEL.dispatch_deadline_ms))
            .contains(&waited),
        "the journal was waited on for {waited:?}, and is given a host call's deadline \
         within the tree's"
    );
    let put = fixture.lines_opening(fixture.reference, "put");
    assert!(
        put.len() == 1 && put[0].contains(&ErrorCode::JournalUnavailable.to_string()),
        "the callback was told the journal did not answer: {put:?}"
    );
    assert_eq!(fixture.held(PEER, "note/photo").await, None);
    let status = fixture
        .kernel
        .plugin_status(fixture.reference)
        .expect("loaded");
    assert_eq!((status.crashes(), status.last_kill()), (0, None));
    let verified = fixture.ana("journal.verify", "{}").await;
    assert!(
        verified.starts_with(r#"{"verified":"#),
        "an opening the dispatch stopped waiting for left the journal whole: {verified}"
    );
}

#[tokio::test]
async fn an_unloaded_plugin_takes_its_commands_timers_and_surfaces_with_it() {
    let fixture = opened().await;
    fixture
        .ana("reference.timer.set", &timer(1_000, "log", &[]))
        .await;
    fixture
        .ana("reference.ui.show", r#"{"note":"photo","title":"Notes"}"#)
        .await;
    fixture
        .ana("reference.event.publish", &event("reference.noted", &[]))
        .await;
    assert_eq!(fixture.kernel.surfaces().len(), 1);
    assert!(fixture.kernel.unload(fixture.reference).await);
    assert!(!fixture.kernel.unload(fixture.reference).await, "once");
    assert!(fixture.kernel.plugin_status(fixture.reference).is_none());
    assert_eq!(fixture.kernel.surfaces(), [] as [nc_kernel::Surface; 0]);
    assert!(
        fixture
            .kernel
            .registry()
            .iter()
            .all(|command| !command.name.starts_with("reference."))
    );
    let gone = fixture
        .as_("ana", "reference.note.get", r#"{"note":"photo"}"#)
        .await;
    assert!(is(
        &gone,
        RefusalKind::NotFound,
        ErrorCode::KernelUnknownCommand
    ));
    // **Nor does it hear any more**: the peer's event finds no subscriber.
    fixture
        .ana("peer.event.announce", &event("peer.noted", &[]))
        .await;
    fixture.clock.advance(1_000);
    assert_eq!(
        fixture.pump().await,
        Pumped::default(),
        "no timer fired, and no event delivered to it or dropped for it"
    );
    // **Its identity is free again**, and its data its own.
    fixture
        .kernel
        .load(
            manifest(REFERENCE, "reference", "peer.*"),
            &nc_kernel::component_for_tests("reference"),
        )
        .await
        .expect("an identity unloaded is free");
}

#[tokio::test]
async fn the_pump_fires_the_timers_due_before_it_delivers_the_events() {
    let fixture = opened().await;
    // **Each callback leaves an entry**: the timer's, the peer's note written
    // as `ana`; the event's, the same command refused beneath a callback that
    // only reads. The journal's order is the pump's.
    let write = Value::object([("then", Value::text("write"))]).to_canonical();
    fixture
        .ana(
            "peer.event.announce",
            &event("peer.noted", &[("data", Value::text(write))]),
        )
        .await;
    fixture
        .ana("reference.timer.set", &timer(1, "put:peer:photo", &[]))
        .await;
    fixture.clock.advance(1);
    let pumped = fixture.pump().await;
    assert_eq!((pumped.fired, pumped.delivered), (1, 1), "{pumped:?}");
    let entries = fixture.entries().await;
    let put = entries_of(&entries, "peer.note.put");
    assert_eq!(
        put.iter()
            .map(|entry| entry.record.result)
            .collect::<Vec<_>>(),
        [Outcome::Allowed, Outcome::Denied],
        "the timer first, then the event published before it"
    );
}

#[tokio::test]
async fn a_ui_interaction_naming_an_unregistered_command_is_refused() {
    let fixture = opened().await;
    let show = |tree: &str| {
        Value::object([
            ("note", Value::text("photo")),
            ("title", Value::text("Notes")),
            ("tree", Value::text(tree)),
        ])
        .to_canonical()
    };
    let unregistered = r#"{"command":"nobody.nothing","label":"Go","node":"button"}"#;
    let shown = fixture.ana("reference.ui.show", &show(unregistered)).await;
    assert_eq!(text(&shown, "opened"), "answered");
    let rendered = text(&shown, "rendered");
    assert!(
        rendered.starts_with(&format!("invalid: [{}] ", ErrorCode::PluginUiInvalid))
            && rendered.contains("/command"),
        "{rendered}"
    );
    let surface = text(&shown, "surface");
    let listed = fixture.kernel.surfaces();
    assert_eq!(listed.len(), 1);
    assert_eq!(
        (
            listed[0].id(),
            listed[0].owner(),
            listed[0].kind(),
            listed[0].title(),
            listed[0].tree()
        ),
        (
            surface.as_str(),
            fixture.reference,
            SurfaceKind::Panel,
            "Notes",
            None
        )
    );
    // **A registered command, with only the arguments it takes**, is kept.
    let valid = r#"{"children":[{"label":"Text","name":"text","node":"input"},{"args":{"note":"photo"},"bind":["text"],"command":"reference.note.put","label":"Save","node":"button"}],"node":"column"}"#;
    let into = |tree: &str| {
        Value::object([
            ("note", Value::text("photo")),
            ("surface", Value::text(surface.clone())),
            ("tree", Value::text(tree)),
        ])
        .to_canonical()
    };
    let kept = fixture.ana("reference.ui.show", &into(valid)).await;
    assert_eq!(text(&kept, "rendered"), "answered", "{kept}");
    assert_eq!(fixture.kernel.surfaces()[0].tree(), Some(valid));
    let wrong_args =
        r#"{"args":{"title":"x"},"command":"reference.note.put","label":"Save","node":"button"}"#;
    let refused = fixture.ana("reference.ui.show", &into(wrong_args)).await;
    assert!(
        text(&refused, "rendered").contains("no argument `reference.note.put` takes"),
        "{refused}"
    );
    assert_eq!(
        fixture.kernel.surfaces()[0].tree(),
        Some(valid),
        "the last tree kept is what is shown"
    );
}

#[tokio::test]
async fn a_surface_another_plugin_holds_is_not_found_and_a_plugin_holds_no_more_than_its_bound() {
    let fixture = opened().await;
    let shown = fixture
        .ana("reference.ui.show", r#"{"note":"photo","title":"Mine"}"#)
        .await;
    let surface = text(&shown, "surface");
    let foreign = fixture
        .ana(
            "peer.ui.show",
            &Value::object([
                ("close", Value::Bool(true)),
                ("note", Value::text("photo")),
                ("surface", Value::text(surface)),
                // **A tree no surface would keep**, so the refusal is the
                // surface's own and not the tree's.
                ("tree", Value::text(r#"{"node":"script"}"#)),
            ])
            .to_canonical(),
        )
        .await;
    let not_found = format!("not-found: [{}] ", ErrorCode::PluginNotFound);
    assert!(
        text(&foreign, "rendered").starts_with(&not_found),
        "{foreign}"
    );
    assert!(
        text(&foreign, "closed").starts_with(&not_found),
        "{foreign}"
    );
    assert_eq!(fixture.kernel.surfaces().len(), 1, "left as it was");
    let bound = i64::from(KERNEL.surfaces_per_plugin);
    let many = fixture
        .ana(
            "reference.ui.show",
            &Value::object([
                ("note", Value::text("photo")),
                ("times", Value::Integer(bound)),
            ])
            .to_canonical(),
        )
        .await;
    assert!(
        text(&many, "opened").starts_with(&format!(
            "unavailable: [{}] ",
            ErrorCode::PluginBoundReached
        )),
        "the one held already, and the bound's worth more: {many}"
    );
    assert_eq!(
        fixture.kernel.surfaces().len(),
        usize::try_from(bound).expect("fits")
    );
    let titled = fixture
        .ana(
            "reference.ui.show",
            &Value::object([
                ("note", Value::text("photo")),
                ("title", Value::text("a\u{202e}b")),
            ])
            .to_canonical(),
        )
        .await;
    assert!(
        text(&titled, "opened").starts_with(&format!("invalid: [{}] ", ErrorCode::PluginUiInvalid)),
        "{titled}"
    );
}

#[tokio::test]
async fn context_reads_who_the_call_runs_for_and_never_a_confirmation_it_did_not_redeem() {
    let fixture = opened().await;
    let args = r#"{"note":"photo"}"#;
    let read = fixture.ana("reference.context.read", args).await;
    assert_eq!(
        read,
        format!(
            r#"{{"actor_chain":[],"confirmed":false,"principal":"{}"}}"#,
            fixture.id_of("ana")
        )
    );
    let ana = fixture.principal("ana");
    let confirmation = fixture
        .kernel
        .confirmation_for_tests(&ana, "reference.context.read", args);
    let confirmed = fixture
        .kernel
        .dispatch(
            &Caller::new(ana.clone()).confirmed(confirmation),
            "reference.context.read",
            args,
        )
        .await
        .expect("answers");
    assert!(
        field(&confirmed, "confirmed") == Value::Bool(true),
        "{confirmed}"
    );
    // **Never nested**: the dispatch at the top redeemed it, and the command
    // it dispatched was not confirmed by anyone.
    let relayed = Value::object([
        ("args", Value::text(args)),
        ("name", Value::text("reference.context.read")),
        ("note", Value::text("photo")),
    ])
    .to_canonical();
    let confirmation = fixture
        .kernel
        .confirmation_for_tests(&ana, "peer.relay.read", &relayed);
    let nested = fixture
        .kernel
        .dispatch(
            &Caller::new(ana).confirmed(confirmation),
            "peer.relay.read",
            &relayed,
        )
        .await
        .expect("answers");
    let last = text(&nested, "last");
    assert!(field(&last, "confirmed") == Value::Bool(false), "{nested}");
    assert_eq!(text(&last, "principal"), fixture.id_of("ana"));
    let bruno = fixture
        .as_("bruno", "reference.context.read", r#"{"note":"notice"}"#)
        .await
        .expect("bruno reads what is shared");
    assert_eq!(text(&bruno, "principal"), fixture.id_of("bruno"));
}

#[tokio::test]
async fn plugin_log_lines_stay_in_the_ring() {
    let fixture = opened().await;
    let lines = i64::from(KERNEL.log_ring_lines);
    let marker = "a line only the plugin's ring holds";
    let written = Value::object([
        ("level", Value::text("warn")),
        ("note", Value::text("photo")),
        ("text", Value::text(format!("{marker} {{n}}\u{1b}"))),
        ("times", Value::Integer(lines + 44)),
    ])
    .to_canonical();
    // **And nowhere else, in any form**: the central log is held to the lines
    // it had before each write. A review found the scan below vacuous — it
    // read a central log that held no line at all.
    fixture.logged_by_the_plugin(&written).await;
    let log = fixture
        .kernel
        .plugin_log(fixture.reference)
        .expect("loaded");
    assert_eq!(log.lines().len(), usize::try_from(lines).expect("fits"));
    assert_eq!(log.dropped(), 44);
    assert_eq!(log.lines()[0].text(), format!("{marker} 44\u{fffd}"));
    assert_eq!(log.lines()[0].level(), LogLevel::Warn);
    let overlong = Value::object([
        ("note", Value::text("photo")),
        ("text", Value::text("é".repeat(2_000))),
    ])
    .to_canonical();
    fixture.logged_by_the_plugin(&overlong).await;
    let log = fixture
        .kernel
        .plugin_log(fixture.reference)
        .expect("loaded");
    let last = log.lines().last().expect("a line");
    assert_eq!(
        last.text(),
        "é".repeat(512),
        "cut at a character, to its bound"
    );
    assert_eq!(last.level(), LogLevel::Info);
    // **The ring is the slot's**: a kill does not empty it, and the instance
    // started again writes to it.
    assert!(
        fixture
            .as_("ana", "reference.abuse.trap", r#"{"note":"photo"}"#)
            .await
            .is_err()
    );
    let lines_before = fixture.lines(fixture.reference);
    assert_eq!(
        lines_before.last(),
        Some(&"é".repeat(512)),
        "what was written before the kill is still there"
    );
    fixture
        .logged_by_the_plugin(r#"{"note":"photo","text":"after the restart"}"#)
        .await;
    // **And what the central log was given carries none of it**: typed
    // fields only — read over the lines the kill wrote, so the scan reads
    // something.
    let given = fixture.log.lines();
    assert!(
        given.iter().any(|line| line.name == "kernel.plugin_killed"),
        "{given:#?}"
    );
    for line in &given {
        for (name, value) in &line.fields {
            let carried = value.to_canonical();
            assert!(
                !carried.contains(marker) && !carried.contains("after the restart"),
                "`{}`'s `{name}` carries the plugin's text",
                line.name
            );
        }
    }
    let after = fixture.lines(fixture.reference);
    assert_eq!(after.last().map(String::as_str), Some("after the restart"));
    assert_eq!(
        after[..after.len() - 1],
        lines_before[1..],
        "one line went, one came"
    );
    // **A slot nothing was loaded in holds no log**: asked of a kernel that
    // loaded nothing.
    let empty = kernel_over(
        fixture.store.clone(),
        fixture.clock.clone(),
        Arc::new(Captured::default()),
        Revoking {
            inner: MemoryDirectory::from_world(WORLD, &family()).expect("the world is valid"),
            revoked: Arc::new(AtomicBool::new(false)),
        },
        Stall::none(),
    )
    .await;
    assert!(empty.plugin_log(fixture.reference).is_none());
}

#[tokio::test]
async fn the_pump_holds_the_local_tables_to_their_bounds_with_the_one_pruner() {
    let fixture = opened().await;
    assert_eq!(fixture.pump().await.pruned, 0, "within its bound");
    let (most, batch) = units_past_their_bound(&fixture.store).await;
    let pumped = fixture.pump().await;
    assert_eq!(
        (pumped.pruned, pumped.prune_refused),
        (batch, None),
        "{pumped:?}"
    );
    assert_eq!(count_units(&fixture.store).await, most + 1 - batch);
    assert!(
        fixture.held(REFERENCE, ACTIVATION).await.is_some(),
        "a unit's rows go, and what a plugin holds stays"
    );
    // **A pruner that cannot ask is told**, and the pump answers it.
    fixture.store.close().await;
    assert!(fixture.pump().await.prune_refused.is_some());
}

#[tokio::test]
async fn a_table_another_writer_holds_is_reported_left_alone_with_its_code() {
    // **A review found this untested.** The pump reads the code of a table
    // the pruner left alone from the tables a run answers, and only a run
    // that left none alone, or one that could not ask at all, was ever
    // pumped: a pump that said nothing was left alone while a table stayed
    // past its bound kept every test green.
    //
    // **Over a file**, because the database held in memory is one connection
    // and a second writer there is a wait for the pool, not a lock.
    let scratch = Scratch::new("pruned-under-a-lock");
    let store = LocalStore::open(&scratch.profile())
        .await
        .expect("the local database opens");
    let kernel = kernel_over(
        store.clone(),
        Manual(Arc::new(AtomicU64::new(NOW_MS))),
        Arc::new(Captured::default()),
        Revoking {
            inner: MemoryDirectory::from_world(WORLD, &family()).expect("the world is valid"),
            revoked: Arc::new(AtomicBool::new(false)),
        },
        Stall::none(),
    )
    .await;
    let (most, batch) = units_past_their_bound(&store).await;
    let mut holder = store.pool().acquire().await.expect("a connection");
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *holder)
        .await
        .expect("held for writing");
    let pumped = kernel.pump().await;
    assert_eq!(
        (pumped.pruned, pumped.prune_refused),
        (0, Some(ErrorCode::DbPruneFailed)),
        "the table another writer held is left alone, and said to be: {pumped:?}"
    );
    sqlx::query("ROLLBACK")
        .execute(&mut *holder)
        .await
        .expect("let go");
    drop(holder);
    assert_eq!(count_units(&store).await, most + 1, "left as it was");
    // **And the next pump prunes it**, the writer gone.
    let pumped = kernel.pump().await;
    assert_eq!(
        (pumped.pruned, pumped.prune_refused),
        (batch, None),
        "{pumped:?}"
    );
    drop(kernel);
    store.close().await;
}

/// Fills the local database's units to one past their bound, answering that
/// bound and the batch the pruner takes from a table past it.
async fn units_past_their_bound(store: &LocalStore) -> (u64, u64) {
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
                   state, undone_by, committed_at) SELECT i, 'r', 'p', 'step', NULL, 's', \
                   'committed', NULL, 0 FROM n";
    sqlx::query(seeding)
        .bind(i64::try_from(most + 1 - before).expect("fits"))
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

/// A timer a command that only reads set, refused when it fires, is logged as
/// the refusal of a read: the command it names declares that it changes
/// nothing.
///
/// **A reviewer found a real bug with it**: every refusal at fire time or at
/// an event's delivery was logged as one of a command that changes state,
/// which the log's field says is the one a person may be waiting on.
#[tokio::test]
async fn a_read_refused_again_at_fire_is_logged_as_a_read() {
    let fixture = opened().await;
    fixture
        .ana(
            "reference.timer.remind",
            &timer(1_000, "put:peer:photo", &[]),
        )
        .await;
    fixture.revoked.store(true, Ordering::SeqCst);
    fixture.clock.advance(1_000);
    let pumped = fixture.pump().await;
    assert_eq!(pumped.refused, 1, "{pumped:?}");
    let entries = fixture.entries().await;
    let denied: Vec<&Entry> = entries_of(&entries, "reference.timer.remind")
        .into_iter()
        .filter(|entry| entry.record.result == Outcome::Denied)
        .collect();
    assert_eq!(denied.len(), 1, "{entries:#?}");
    assert_eq!(denied[0].record.action, "Read", "the authority is a read");
    let logged = fixture.log.named("kernel.dispatch_refused");
    let last = logged.last().expect("the refusal is logged");
    assert_eq!(
        last.fields["mutating"],
        Value::Bool(false),
        "the command refused is `reference.timer.remind`, which only reads: {logged:?}"
    );
}

/// An undo takes back what its step kept and leaves what it set going: a timer
/// the step armed stays armed and fires as the person, and an event it
/// published is still heard — and neither undo says anything of its step left
/// the machine, which is what `beyond_reach` counts.
///
/// **From a reviewer's probe (wave 5, units)**, which found the published
/// sentence saying undo stops at a step that did anything else with no
/// compensation registered, while it undoes past one that armed a timer. The
/// sentence says what undo does now (`docs/commands.md`), and this holds undo
/// to it.
#[tokio::test]
async fn an_undo_leaves_what_its_step_set_going_as_it_is() {
    let fixture = opened().await;
    let set = fixture
        .ana("reference.timer.set", &timer(1_000, "put:peer:photo", &[]))
        .await;
    assert_eq!(timers_set(&set).len(), 1, "{set}");
    let undone = fixture
        .as_("ana", "kernel.undo", "{}")
        .await
        .expect("a step that armed a timer is no barrier");
    assert_eq!(number(&undone, "beyond_reach"), 0, "{undone}");
    fixture
        .ana(
            "reference.event.publish",
            &event("reference.counted", &[("data", Value::text("{}"))]),
        )
        .await;
    let undone = fixture
        .as_("ana", "kernel.undo", "{}")
        .await
        .expect("nor is a step that published an event");
    assert_eq!(number(&undone, "beyond_reach"), 0, "{undone}");
    fixture.clock.advance(1_000);
    let pumped = fixture.pump().await;
    assert_eq!((pumped.fired, pumped.delivered), (1, 1), "{pumped:?}");
    assert_eq!(
        fixture
            .held("noctua:reference-peer", "note/photo")
            .await
            .as_deref(),
        Some(r#"{"text":"fired"}"#),
        "the timer's callback ran as ana"
    );
}
