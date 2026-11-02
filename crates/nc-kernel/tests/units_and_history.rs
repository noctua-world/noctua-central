//! Units of work and the history they make: one that is aborted leaves no
//! state, whether its command refused, its plugin was killed in the middle of
//! it — whichever plugin of its call tree that was — or a plugin let a unit it
//! began go (`NOCTUA.md §11.5`, acceptance criterion 3); and a step is undone
//! and redone through `kernel.undo` and `kernel.redo`, dispatched as any
//! command is, across the plugins it touched (acceptance criterion 10).
//!
//! **What a test reads is what was kept.** Every command runs through the
//! decision point into the real component, over the local database — held in
//! memory, and in two tests a file on disk that is closed and opened again
//! before it is read — so a write that was staged and not kept is one the
//! store never holds, and a blob a unit made and did not keep is one neither
//! the database nor the blobs' store holds. A command that writes first is shown to have written by
//! a note committed before it, which the store still holds as it was; the
//! journal shows the unit opened and aborted, and the log why. An undo is read
//! back the same way: the keys as the store holds them, each unit's state, the
//! journal's entries and the log's line.

use core::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use nc_authz::{Directory, DirectoryError, EntityRef, MemoryDirectory, Pdp, PrincipalKind};
use nc_canon::Value;
use nc_catalog::ErrorCode;
use nc_ids::{MachineFlake, Minter, OpaqueId, Snowflake};
use nc_journal::{Entry, Outcome};
use nc_kernel::blobs::{BlobStore as _, Hash, MemoryBlobs};
use nc_kernel::logging::Captured;
use nc_kernel::manifest::PluginManifest;
use nc_kernel::{
    Caller, Environment, Kernel, KernelConfig, Kill, PluginSlot, Refusal, RefusalKind,
};
use nc_local::{
    BlobCreated, Expect, LocalStore, Quotas, StagedWrite, UnitKind, UnitMeta, UnitRecord, UnitRow,
    UnitState,
};
use nc_plugin_reference::behaviour::{ACTIVATION, Abuse, Activation, PEER, Variant};

/// The world every policy case is about.
const WORLD: &str = include_str!("../../../policy/world.json");

/// The instant the policy cases call now, `2026-08-26T00:00:00Z`: inside the
/// grant `g.current`.
const NOW_MS: u64 = 1_787_702_400_000;

/// The identity the reference plugin is loaded under.
const REFERENCE: &str = "noctua:reference";

/// The identity its peer is loaded under.
const THE_PEER: &str = "noctua:reference-peer";

/// The label every seed is committed under, which no command's unit has.
const SEEDING: &str = "seeding the test";

/// A clock that stands at [`NOW_MS`] until a minter waits past it.
#[derive(Debug, Clone)]
struct Still {
    now: Arc<AtomicU64>,
}

impl Still {
    /// The clock at [`NOW_MS`].
    fn new() -> Self {
        Self {
            now: Arc::new(AtomicU64::new(NOW_MS)),
        }
    }
}

impl nc_ids::Clock for Still {
    fn now_ms(&self) -> u64 {
        self.now.load(Ordering::SeqCst)
    }

    fn wait_past(&self, millis: u64) {
        self.now
            .fetch_max(millis.saturating_add(1), Ordering::SeqCst);
    }
}

/// The directory over the world as it is written, or — once a test shifts it —
/// over the same world with the note `photo` given to `carla`.
///
/// **What changed since a step, as a directory sees it.** An undo authorizes
/// again, now, what the step it reverses was allowed; a world in which the
/// photo is no longer the person's is one in which writing it back is no
/// longer theirs either, while undoing — a `Write` on the realm — still is.
#[derive(Debug)]
struct Shifting {
    written: MemoryDirectory,
    shifted: MemoryDirectory,
    shift: Arc<AtomicBool>,
}

impl Shifting {
    /// The directory a question is asked of now.
    fn now(&self) -> &MemoryDirectory {
        if self.shift.load(Ordering::SeqCst) {
            &self.shifted
        } else {
            &self.written
        }
    }
}

impl Directory for Shifting {
    fn realm(
        &self,
        realm: &str,
    ) -> impl Future<Output = Result<nc_authz::RealmFacts, DirectoryError>> + Send {
        self.now().realm(realm)
    }

    fn grant(
        &self,
        realm: &str,
        principal: &EntityRef,
    ) -> impl Future<Output = Result<Option<nc_authz::GrantFacts>, DirectoryError>> + Send {
        self.now().grant(realm, principal)
    }

    fn slice(
        &self,
        realm: &str,
        principal: &EntityRef,
        resource: &EntityRef,
        grant: Option<&EntityRef>,
    ) -> impl Future<Output = Result<nc_authz::Slice, DirectoryError>> + Send {
        self.now().slice(realm, principal, resource, grant)
    }
}

/// The world, with the note `photo` given to `carla`.
fn photo_given_to_carla() -> String {
    let mut world: serde_json::Value = serde_json::from_str(WORLD).expect("the world is JSON");
    let photo = world
        .as_array_mut()
        .expect("a list of entities")
        .iter_mut()
        .find(|entity| entity["uid"]["id"] == "photo")
        .expect("the world holds the photo");
    photo["attrs"]["owner"]["__entity"]["id"] = serde_json::Value::from("carla");
    world.to_string()
}

/// The directory over `world`, as the kernel is given it: every realm at
/// epoch seven, and `g.current` granted to `ana` and to `carla` each.
fn directory_over(world: &str, pack: &Pdp) -> MemoryDirectory {
    let mut directory = MemoryDirectory::from_world(world, pack)
        .expect("the world is valid")
        .with_epoch("home", 7);
    for person in ["ana", "carla"] {
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
    directory
}

/// A kernel over the local database, the journal in memory, the [`Still`]
/// clock and the world's directory, which a test can shift.
#[derive(Debug)]
struct Env;

impl Environment for Env {
    type Store = LocalStore;
    type Blobs = Arc<MemoryBlobs>;
    type Journal = nc_journal::InMemory;
    type Clock = Still;
    type Directory = Shifting;
    type Log = Arc<Captured>;
    type Transport = nc_kernel::net::Scripted;
}

/// A scratch directory for one test, removed however the test leaves.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Scratch {
    /// The scratch called `name`, empty.
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("nc-kernel-units-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir(&root).expect("a scratch directory");
        Self(root)
    }

    /// The directory the local database is opened in, which opening it makes
    /// private.
    fn profile(&self) -> PathBuf {
        self.0.join("profile")
    }
}

/// A kernel and everything a test reads beside it.
struct Fixture {
    kernel: Kernel<Env>,
    store: LocalStore,
    blobs: Arc<MemoryBlobs>,
    log: Arc<Captured>,
    world: MemoryDirectory,
    slots: Vec<PluginSlot>,
    clock: Still,
    shift: Arc<AtomicBool>,
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

/// The user called `name`'s identifier, as the world knows it.
fn id_of(world: &MemoryDirectory, name: &str) -> OpaqueId {
    world
        .principal(PrincipalKind::User, name)
        .unwrap_or_else(|| panic!("{name} is a user of the world"))
        .id()
        .clone()
}

/// Commits `value` as `plugin`'s `key` in `store`, as a unit nobody
/// dispatched.
async fn seeded(store: &LocalStore, plugin: &str, key: &str, value: &str) {
    seeded_by(store, "ana", plugin, key, value).await;
}

/// Commits `value` as `plugin`'s `key` in `store`, as a step of `person`'s
/// nobody dispatched — which the journal therefore never saw.
async fn seeded_by(store: &LocalStore, person: &str, plugin: &str, key: &str, value: &str) {
    seeded_unit(store, person, |unit| {
        unit.writes = vec![StagedWrite {
            plugin: plugin.to_owned(),
            key: key.to_owned(),
            expect: Expect::Anything,
            after: Some(value.to_owned()),
        }];
    })
    .await;
}

/// Commits a step of `person`'s nobody dispatched, shaped by `shape`.
async fn seeded_unit(store: &LocalStore, person: &str, shape: impl FnOnce(&mut UnitRecord)) {
    /// The flake the next seed is minted with: each seed's own.
    static FLAKES: AtomicU64 = AtomicU64::new(16);
    let world = MemoryDirectory::from_world(WORLD, &family()).expect("the world is valid");
    let flake = u16::try_from(FLAKES.fetch_add(1, Ordering::SeqCst) % 1_000).expect("fits");
    let mut minter = Minter::new(
        MachineFlake::from_value(flake).expect("a flake"),
        Still::new(),
    );
    let mut unit = UnitRecord {
        unit: minter.mint().expect("a unit"),
        meta: UnitMeta {
            realm: world.realm_id("home").expect("home is a realm"),
            principal: id_of(&world, person),
            kind: UnitKind::Step,
            label: SEEDING.to_owned(),
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

/// What the reference plugin is granted: what its commands reach for.
const GRANTED: &str =
    r#""store", "blobs.read", "blobs.write", "commands.dispatch", "commands.begin", "net""#;

/// What a reference plugin not granted `commands.dispatch` is granted: what
/// a request and the unit it is sent in reach for.
const WITHOUT_DISPATCH: &str = r#""store", "commands.begin", "net""#;

/// The reference plugin's manifest as `id` under `namespace`.
///
/// It is granted what the commands reach for — the network at `example.com`
/// among it — with `limits` as its `[limits]`.
fn manifest(id: &str, namespace: &str, limits: &str) -> PluginManifest {
    manifest_granting(id, namespace, limits, GRANTED)
}

/// The reference plugin's manifest as `id` under `namespace`, granting what `granted` lists.
///
/// It fetches from `example.com`, and its `[limits]` are `limits`: the
/// manifest every fixture of this suite loads the plugin with.
fn manifest_granting(id: &str, namespace: &str, limits: &str, granted: &str) -> PluginManifest {
    PluginManifest::parse(&format!(
        "id = \"{id}\"\nnamespace = \"{namespace}\"\ncapabilities = [{granted}]\n\
         [net]\ndomains = [\"example.com\"]\n[limits]\n{limits}"
    ))
    .expect("a manifest the kernel reads")
}

/// A kernel over `store` with the reference plugin loaded under `limits`, and
/// its peer beside it when `with_the_peer`.
async fn opened(store: LocalStore, limits: &str, with_the_peer: bool) -> Fixture {
    opened_granting(store, limits, with_the_peer, GRANTED).await
}

/// A kernel over `store` with the reference plugin granted what `granted` lists.
///
/// The plugin is loaded under `limits`; when `with_the_peer`, its peer is
/// loaded beside it, granted what the commands reach for.
async fn opened_granting(
    store: LocalStore,
    limits: &str,
    with_the_peer: bool,
    granted: &str,
) -> Fixture {
    opened_over(
        store,
        nc_journal::InMemory::default(),
        limits,
        with_the_peer,
        granted,
    )
    .await
}

/// A kernel over `store` and the journal's store `history`, opened as a
/// daemon opens one over what a process before it left.
///
/// The reference plugin is granted what `granted` lists and loaded under
/// `limits`, its peer beside it when `with_the_peer`, once the kernel has
/// opened — and recovered whatever `history` left open.
async fn opened_over(
    store: LocalStore,
    history: nc_journal::InMemory,
    limits: &str,
    with_the_peer: bool,
    granted: &str,
) -> Fixture {
    let pack = family();
    let world = MemoryDirectory::from_world(WORLD, &pack).expect("the world is valid");
    let shift = Arc::new(AtomicBool::new(false));
    let directory = Shifting {
        written: directory_over(WORLD, &pack),
        shifted: directory_over(&photo_given_to_carla(), &pack),
        shift: Arc::clone(&shift),
    };
    if with_the_peer {
        // **Bruno's, not ana's**: a seed is a step the journal never saw, and
        // one of ana's is what her undo would find first — a seed's unit is
        // minted on a flake above the journal's, so it sorts as her newest.
        seeded_by(
            &store,
            "bruno",
            THE_PEER,
            ACTIVATION,
            &Activation {
                namespace: PEER.to_owned(),
                variant: Variant::Normal,
                subscribe: Vec::new(),
            }
            .written(),
        )
        .await;
    }
    let (log, clock) = (Arc::new(Captured::default()), Still::new());
    let blobs = Arc::new(MemoryBlobs::default());
    let keeper = the_keeper();
    let kernel = Kernel::open(KernelConfig {
        store: store.clone(),
        blobs: Arc::clone(&blobs),
        pdp: Ok(pack),
        journal: nc_journal::Journal::new(
            history,
            world.realm_id("home").expect("home is a realm"),
            keeper,
            Minter::new(MachineFlake::from_value(3).expect("a flake"), clock.clone()),
        ),
        directory,
        clock: clock.clone(),
        log: Arc::clone(&log),
        transport: nc_kernel::net::Scripted::default(),
    })
    .await
    .expect("the kernel opens");
    let component = nc_kernel::component_for_tests("reference");
    let mut slots = vec![
        kernel
            .load(
                manifest_granting(REFERENCE, "reference", limits, granted),
                &component,
            )
            .await
            .unwrap_or_else(|refused| panic!("the reference plugin loads: {refused}")),
    ];
    if with_the_peer {
        slots.push(
            kernel
                .load(manifest(THE_PEER, PEER, ""), &component)
                .await
                .unwrap_or_else(|refused| panic!("the peer loads: {refused}")),
        );
    }
    Fixture {
        kernel,
        store,
        blobs,
        log,
        world,
        slots,
        clock,
        shift,
    }
}

/// The local database held in memory.
async fn in_memory() -> LocalStore {
    LocalStore::in_memory_for_tests()
        .await
        .expect("the local database opens in memory")
}

impl Fixture {
    /// Dispatches `command` with `args` as `ana`, the guardian who owns the
    /// note `photo`.
    async fn as_ana(&self, command: &str, args: &str) -> Result<String, Refusal> {
        self.as_("ana", command, args).await
    }

    /// Dispatches `command` with `args` as the user called `name`.
    async fn as_(&self, name: &str, command: &str, args: &str) -> Result<String, Refusal> {
        let person = self
            .world
            .principal(PrincipalKind::User, name)
            .unwrap_or_else(|| panic!("{name} is a user of the world"));
        self.kernel
            .dispatch(&Caller::new(person), command, args)
            .await
    }

    /// The units the store keeps that a command wrote, newest first: every one
    /// but the seeds.
    async fn kept(&self) -> Vec<String> {
        units_in(&self.store, &self.world).await
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
}

/// The labels of the units `store` keeps that a command wrote, newest first.
async fn units_in(store: &LocalStore, world: &MemoryDirectory) -> Vec<String> {
    let realm = world.realm_id("home").expect("home is a realm");
    store
        .units(&realm, &id_of(world, "ana"), 100)
        .await
        .expect("the store answers")
        .into_iter()
        .map(|unit| unit.meta.label)
        .filter(|label| label != SEEDING)
        .collect()
}

/// What `plugin`'s `key` holds in `store`, committed.
async fn held(store: &LocalStore, plugin: &str, key: &str) -> Option<String> {
    store
        .get(&home(), plugin, key)
        .await
        .expect("the store answers")
        .map(|entry| entry.value)
}

/// A note `photo` holding `text`, as the plugin's store keeps it.
fn kept_as(text: &str) -> String {
    Value::object([("text", Value::text(text))]).to_canonical()
}

/// A note's arguments: `photo`, holding `text`.
fn photo(text: &str) -> String {
    Value::object([("note", Value::text("photo")), ("text", Value::text(text))]).to_canonical()
}

/// The arguments of a note `photo` written with `text` before its command
/// stops as `abuse` says.
fn abused(text: &str, abuse: Abuse) -> String {
    Value::object([
        ("abuse", Value::text(abuse.word())),
        ("note", Value::text("photo")),
        ("text", Value::text(text)),
    ])
    .to_canonical()
}

/// The brackets of `entries`, as their result.
fn brackets(entries: &[Entry]) -> Vec<Outcome> {
    entries
        .iter()
        .filter(|entry| {
            [nc_journal::unit::OPENS, nc_journal::unit::CLOSES]
                .contains(&entry.record.action.as_str())
        })
        .map(|entry| entry.record.result)
        .collect()
}

/// Whether `refusal` is of `kind`, carrying `code`.
fn is(refusal: &Result<String, Refusal>, kind: RefusalKind, code: ErrorCode) -> bool {
    refusal
        .as_ref()
        .err()
        .is_some_and(|refusal| refusal.kind() == kind && refusal.code() == code)
}

#[tokio::test]
async fn a_command_that_fails_leaves_no_write() {
    let fixture = opened(in_memory().await, "", false).await;
    fixture
        .as_ana("reference.note.put", &photo("kept"))
        .await
        .expect("the note is written");
    let failed = fixture
        .as_ana("reference.note.put-then-fail", &photo("half done"))
        .await;
    assert!(
        is(
            &failed,
            RefusalKind::Invalid,
            ErrorCode::PluginCommandRefused
        ),
        "{failed:?}"
    );
    assert_eq!(
        held(&fixture.store, REFERENCE, "note/photo").await,
        Some(kept_as("kept")),
        "the failed command's write was kept"
    );
    assert_eq!(fixture.kept().await, ["reference.note.put"]);
    assert_eq!(
        brackets(&fixture.entries().await),
        [
            Outcome::UnitBegun,
            Outcome::UnitCommitted,
            Outcome::UnitBegun,
            Outcome::UnitAborted
        ]
    );
    assert_eq!(fixture.logged("kernel.unit_aborted", "cause"), ["failed"]);
}

/// A command killed after it wrote leaves nothing it wrote, however it died.
///
/// A trap, its fuel, its epoch and its memory, each in a kernel whose manifest
/// lowers the one bound it crosses.
#[tokio::test]
async fn a_command_killed_mid_way_leaves_no_state() {
    for (abuse, limits, kill) in [
        (Abuse::Trap, "", Kill::Trap),
        (Abuse::Burn, "fuel_per_dispatch = 10_000_000\n", Kill::Fuel),
        (Abuse::Spin, "epoch_deadline_ticks = 20\n", Kill::Epoch),
        (Abuse::Grow, "memory_bytes = 4_194_304\n", Kill::Memory),
    ] {
        let fixture = opened(in_memory().await, limits, false).await;
        fixture
            .as_ana("reference.note.put", &photo("kept"))
            .await
            .expect("the note is written");
        let killed = fixture
            .as_ana("reference.note.put-then-abuse", &abused("half done", abuse))
            .await;
        assert!(
            is(&killed, RefusalKind::Internal, kill.code()),
            "{abuse:?}: {killed:?}"
        );
        assert_eq!(
            held(&fixture.store, REFERENCE, "note/photo").await,
            Some(kept_as("kept")),
            "{abuse:?}: what the killed command wrote was kept"
        );
        assert_eq!(fixture.kept().await, ["reference.note.put"], "{abuse:?}");
        assert_eq!(
            brackets(&fixture.entries().await)[2..],
            [Outcome::UnitBegun, Outcome::UnitAborted],
            "{abuse:?}"
        );
        assert_eq!(
            fixture.logged("kernel.unit_aborted", "cause"),
            ["failed"],
            "{abuse:?}"
        );
        let status = fixture
            .kernel
            .plugin_status(fixture.slots[0])
            .expect("the plugin is loaded");
        assert_eq!(
            (status.crashes(), status.last_kill()),
            (1, Some(kill)),
            "{abuse:?}"
        );
    }
}

/// A command that made a blob and then failed leaves no blob.
///
/// Refused or killed however it can be, no row names the blob, the blobs'
/// store holds no bytes of it, and the plugin is told it is not on this
/// machine — while a blob kept before it stays.
#[tokio::test]
async fn a_command_that_fails_or_is_killed_mid_way_leaves_no_blob() {
    for (then, limits, code) in [
        ("fail", "", ErrorCode::PluginCommandRefused),
        ("trap", "", Kill::Trap.code()),
        (
            "burn",
            "fuel_per_dispatch = 10_000_000\n",
            Kill::Fuel.code(),
        ),
        ("spin", "epoch_deadline_ticks = 20\n", Kill::Epoch.code()),
        ("grow", "memory_bytes = 4_194_304\n", Kill::Memory.code()),
    ] {
        let fixture = opened(in_memory().await, limits, false).await;
        let kept = blob_put("kept", None);
        fixture
            .as_ana("reference.blob.put", &kept)
            .await
            .expect("a blob is kept");
        let refused = fixture
            .as_ana("reference.blob.put", &blob_put("half done", Some(then)))
            .await
            .expect_err("the command did not end well");
        assert_eq!(refused.code(), code, "`{then}`: {refused}");
        let unkept = Hash::of(b"half done");
        assert_eq!(
            fixture.blobs.size(&unkept).await,
            Ok(None),
            "`{then}`: the bytes of an unkept blob stayed"
        );
        assert_eq!(
            fixture.store.blob(unkept.as_str()).await.expect("answers"),
            None,
            "`{then}`: a row of an unkept blob stayed"
        );
        let located = fixture
            .as_ana("reference.blob.locate", &blob_about(unkept.as_str()))
            .await
            .expect("located");
        assert!(
            located.contains(r#""availability":"unavailable""#),
            "`{then}`: {located}"
        );
        let kept = Hash::of(b"kept");
        assert_eq!(fixture.blobs.size(&kept).await, Ok(Some(4)), "`{then}`");
        assert!(
            fixture
                .store
                .blob(kept.as_str())
                .await
                .expect("answers")
                .is_some_and(|row| row.pinned),
            "`{then}`"
        );
        assert_eq!(
            fixture.logged("kernel.unit_aborted", "cause"),
            ["failed"],
            "`{then}`"
        );
    }
}

/// A nested command that made a blob and then refused dooms the unit it ran
/// in, though the command above it answered, and the blob goes with it.
///
/// **A blob made is a change**, as a write staged is: a failed frame that
/// changed nothing leaves the unit to its caller, and one that made a blob
/// cannot have it kept.
#[tokio::test]
async fn a_nested_command_that_made_a_blob_and_failed_dooms_its_unit_and_the_blob() {
    let fixture = opened(in_memory().await, "", true).await;
    let relayed = Value::object([
        ("args", Value::text(blob_put("made below", Some("fail")))),
        ("name", Value::text("peer.blob.put")),
        ("note", Value::text("photo")),
    ])
    .to_canonical();
    let refused = fixture
        .as_ana("reference.relay.write", &relayed)
        .await
        .expect_err("a unit a failed nested command made a blob in is not kept");
    assert_eq!(refused.code(), ErrorCode::KernelUnitAborted, "{refused}");
    let made = Hash::of(b"made below");
    assert_eq!(
        fixture.blobs.size(&made).await,
        Ok(None),
        "the blob's bytes stayed"
    );
    assert_eq!(
        fixture.store.blob(made.as_str()).await.expect("answers"),
        None,
        "the blob's row was kept"
    );
    assert_eq!(fixture.logged("kernel.unit_aborted", "cause"), ["nested"]);
}

/// `blob.put`'s arguments: `text` written once, about the note `photo`, ending
/// as `then` says.
fn blob_put(text: &str, then: Option<&str>) -> String {
    let mut fields = vec![("note", Value::text("photo")), ("text", Value::text(text))];
    if let Some(then) = then {
        fields.push(("then", Value::text(then)));
    }
    Value::object(fields).to_canonical()
}

/// `blob.put`'s arguments: `text` written once, about the note `note`.
fn blob_put_on(note: &str, text: &str) -> String {
    Value::object([("note", Value::text(note)), ("text", Value::text(text))]).to_canonical()
}

/// The arguments of a command about the blob `hash`.
fn blob_about(hash: &str) -> String {
    Value::object([("hash", Value::text(hash)), ("note", Value::text("photo"))]).to_canonical()
}

/// A plugin killed while it ran inside another's command dooms the whole unit:
/// what the command above it wrote is not kept either, though that command
/// answered, and only the plugin that was killed is started again.
///
/// The control is the same pair with nothing killed, whose writes reach both
/// plugins' stores in one unit.
#[tokio::test]
async fn a_nested_plugin_killed_mid_way_reverts_the_whole_unit_in_both_plugins() {
    let fixture = opened(in_memory().await, "", true).await;
    let paired = fixture
        .as_ana("reference.pair.put", &photo("both"))
        .await
        .expect("the pair writes in both plugins");
    assert!(
        nc_canon::parse(&paired)
            .ok()
            .and_then(Value::into_object)
            .and_then(|mut fields| fields.remove("peer"))
            .and_then(Value::into_text)
            .is_some_and(|peer| peer.starts_with(r#"{"rev":"#)),
        "{paired}"
    );
    assert_eq!(
        held(&fixture.store, REFERENCE, "note/photo").await,
        Some(kept_as("both"))
    );
    assert_eq!(
        held(&fixture.store, THE_PEER, "note/photo").await,
        Some(kept_as("both"))
    );
    let refused = fixture
        .as_ana("reference.pair.put-then-trap", &photo("half done"))
        .await
        .expect_err("a unit a kill doomed is not kept, though its command answered");
    assert_eq!(refused.code(), ErrorCode::KernelUnitAborted, "{refused}");
    assert_eq!(
        refused.kind(),
        RefusalKind::Internal,
        "the kill's own case: {refused}"
    );
    assert!(
        refused
            .sentence()
            .contains(&ErrorCode::PluginTrapped.to_string()),
        "{refused}"
    );
    for plugin in [REFERENCE, THE_PEER] {
        assert_eq!(
            held(&fixture.store, plugin, "note/photo").await,
            Some(kept_as("both")),
            "`{plugin}` kept what the doomed unit wrote"
        );
    }
    assert_eq!(fixture.kept().await, ["reference.pair.put"]);
    assert_eq!(
        brackets(&fixture.entries().await)[2..],
        [Outcome::UnitBegun, Outcome::UnitAborted]
    );
    assert_eq!(fixture.logged("kernel.unit_aborted", "cause"), ["nested"]);
    assert_eq!(fixture.logged("kernel.plugin_killed", "slot"), ["1"]);
    let (reference, peer) = (
        fixture
            .kernel
            .plugin_status(fixture.slots[0])
            .expect("loaded"),
        fixture
            .kernel
            .plugin_status(fixture.slots[1])
            .expect("loaded"),
    );
    assert_eq!((reference.crashes(), reference.generation()), (0, 0));
    assert_eq!((peer.crashes(), peer.generation()), (1, 1));
}

/// A command killed after it wrote leaves nothing it wrote in the file.
///
/// The database is closed once the kernel is gone and opened again, and holds
/// the note committed before the kill and the one unit that wrote it.
#[tokio::test]
async fn a_command_killed_mid_way_leaves_nothing_in_the_file() {
    let scratch = Scratch::new("killed-mid-way");
    let store = LocalStore::open(&scratch.profile())
        .await
        .expect("the local database opens");
    let fixture = opened(store, "", false).await;
    fixture
        .as_ana("reference.note.put", &photo("kept"))
        .await
        .expect("the note is written");
    let killed = fixture
        .as_ana(
            "reference.note.put-then-abuse",
            &abused("half done", Abuse::Trap),
        )
        .await;
    assert!(
        is(&killed, RefusalKind::Internal, ErrorCode::PluginTrapped),
        "{killed:?}"
    );
    let Fixture {
        kernel,
        store,
        world,
        ..
    } = fixture;
    drop(kernel);
    store.close().await;
    let reopened = LocalStore::open(&scratch.profile())
        .await
        .expect("the local database opens again");
    assert_eq!(
        held(&reopened, REFERENCE, "note/photo").await,
        Some(kept_as("kept")),
        "what the killed command wrote reached the file"
    );
    assert_eq!(units_in(&reopened, &world).await, ["reference.note.put"]);
    reopened.close().await;
}

/// The unit `unit` of the person `person` in the realm `home`: of `kind`,
/// labelled `label`, making `writes` and moving the states `marks` names.
fn a_unit(
    unit: Snowflake,
    person: &OpaqueId,
    kind: UnitKind,
    label: &str,
    writes: Vec<StagedWrite>,
    marks: Vec<(Snowflake, UnitState)>,
) -> UnitRecord {
    let world = MemoryDirectory::from_world(WORLD, &family()).expect("the world is valid");
    UnitRecord {
        unit,
        meta: UnitMeta {
            realm: world.realm_id("home").expect("home is a realm"),
            principal: person.clone(),
            kind,
            label: label.to_owned(),
            committed_at_ms: NOW_MS,
        },
        writes,
        blobs: Vec::new(),
        notes: Vec::new(),
        unpinned: Vec::new(),
        marks,
    }
}

/// The reference plugin's `key` written to `after`, or deleted, over a key
/// held to `expect`.
fn a_write(key: &str, expect: Expect, after: Option<String>) -> StagedWrite {
    StagedWrite {
        plugin: REFERENCE.to_owned(),
        key: key.to_owned(),
        expect,
        after,
    }
}

/// Commits `unit` to `store` as it is written, with room to spare.
async fn committed(store: &LocalStore, unit: &UnitRecord) {
    let quotas = Quotas {
        entries_per_plugin: 100,
        lowered: std::collections::BTreeMap::new(),
        blobs: 10,
        writes_per_unit: 10,
        notes_per_unit: 10,
    };
    store.commit(unit, &quotas).await.expect("the unit commits");
}

/// Commits `unit` to `store` held from the pruner, as a kernel commits one it
/// has not yet heard the journal close, with room to spare.
async fn committed_held(store: &LocalStore, unit: &UnitRecord) {
    let quotas = Quotas {
        entries_per_plugin: 100,
        lowered: std::collections::BTreeMap::new(),
        blobs: 10,
        writes_per_unit: 10,
        notes_per_unit: 10,
    };
    store
        .commit_held(unit, &quotas)
        .await
        .expect("the unit commits");
}

/// How the unit `unit` stands in `store`.
async fn state_of(store: &LocalStore, unit: Snowflake) -> Option<UnitState> {
    store
        .unit(unit)
        .await
        .expect("the store answers")
        .map(|detail| detail.row.state)
}

/// The brackets `entries` hold for the unit `unit`, by their result.
fn brackets_of(entries: &[Entry], unit: Snowflake) -> Vec<Outcome> {
    entries
        .iter()
        .filter(|entry| entry.record.work_unit == Some(unit))
        .filter(|entry| {
            [nc_journal::unit::OPENS, nc_journal::unit::CLOSES]
                .contains(&entry.record.action.as_str())
        })
        .map(|entry| entry.record.result)
        .collect()
}

/// The units a stopped process left, in every shape a stop can leave them.
struct Shapes {
    /// A step it kept: committed in the store and closed in the journal.
    kept: Snowflake,
    /// A step the store committed and the journal never closed.
    left: Snowflake,
    /// A unit the journal opened and the store holds nothing of.
    nothing: Snowflake,
    /// A unit the store holds reversed already, the journal never closed.
    reversed: Snowflake,
    /// The reversal that reversed it.
    its_reversal: Snowflake,
    /// A step kept, which `undo` undid.
    undone: Snowflake,
    /// An undo the store committed and the journal never closed.
    undo: Snowflake,
    /// A unit the realm `office` left open in the same journal.
    elsewhere: Snowflake,
}

/// The step `unit` of `person`'s, writing the reference plugin's `key` as a
/// note holding `after` over whatever it held.
fn a_step(unit: Snowflake, person: &OpaqueId, key: &str, after: &str) -> UnitRecord {
    a_unit(
        unit,
        person,
        UnitKind::Step,
        "reference.note.put",
        vec![a_write(key, Expect::Anything, Some(kept_as(after)))],
        Vec::new(),
    )
}

/// The reference plugin's `key` deleted over a note holding `held`: what
/// takes back a step that wrote it.
fn taken_back(key: &str, held: &str) -> Vec<StagedWrite> {
    vec![a_write(key, Expect::Value(Some(kept_as(held))), None)]
}

/// The history a process stopped in every shape leaves, and the units it
/// left in it, its store's half committed to `store`.
async fn left_in_every_shape(store: &LocalStore) -> (nc_journal::InMemory, Shapes) {
    let world = MemoryDirectory::from_world(WORLD, &family()).expect("the world is valid");
    // **The stopped process's own keeper**: whoever brackets a unit is the
    // journal's to name, and the world's `ana` is one no hatch is needed for.
    let ana = id_of(&world, "ana");
    let minter = |flake| {
        Minter::new(
            MachineFlake::from_value(flake).expect("a flake"),
            Still::new(),
        )
    };
    let mut stopped = nc_journal::Journal::new(
        nc_journal::InMemory::default(),
        world.realm_id("home").expect("home is a realm"),
        ana.clone(),
        minter(9),
    );
    let kept = stopped
        .open_unit("reference.note.put")
        .await
        .expect("opened");
    committed(store, &a_step(kept, &ana, "note/photo", "kept")).await;
    stopped
        .close_unit(kept, nc_journal::Closing::Committed)
        .await
        .expect("closed");
    let left = stopped
        .open_unit("reference.note.put")
        .await
        .expect("opened");
    committed(store, &a_step(left, &ana, "note/photo", "left behind")).await;
    let nothing = stopped
        .open_unit("reference.note.put")
        .await
        .expect("opened");
    let reversed = stopped
        .open_unit("reference.note.put")
        .await
        .expect("opened");
    committed(store, &a_step(reversed, &ana, "note/diary", "reversed")).await;
    let its_reversal = stopped.mint_unit().expect("a unit");
    committed(
        store,
        &a_unit(
            its_reversal,
            &ana,
            UnitKind::Undo { target: reversed },
            "kernel.revert",
            taken_back("note/diary", "reversed"),
            vec![(reversed, UnitState::Undone { by: its_reversal })],
        ),
    )
    .await;
    let (undone, undo) = an_undo_left_open(store, &mut stopped, &ana).await;
    let mut there = nc_journal::Journal::new(
        stopped.store().clone(),
        world.realm_id("office").expect("office is a realm"),
        ana,
        minter(10),
    );
    let elsewhere = there.open_unit("reference.note.put").await.expect("opened");
    let shapes = Shapes {
        kept,
        left,
        nothing,
        reversed,
        its_reversal,
        undone,
        undo,
        elsewhere,
    };
    (there.store().clone(), shapes)
}

/// A step `journal` kept, and an undo of it the journal never closed.
///
/// Both are `person`'s, the undo committed in the store as the step was, and
/// their store's half is committed to `store`.
async fn an_undo_left_open(
    store: &LocalStore,
    journal: &mut nc_journal::Journal<nc_journal::InMemory, Still>,
    person: &OpaqueId,
) -> (Snowflake, Snowflake) {
    let undone = journal
        .open_unit("reference.note.put")
        .await
        .expect("opened");
    committed(store, &a_step(undone, person, "note/recipe", "first")).await;
    journal
        .close_unit(undone, nc_journal::Closing::Committed)
        .await
        .expect("closed");
    let undo = journal.open_unit("kernel.undo").await.expect("opened");
    committed(
        store,
        &a_unit(
            undo,
            person,
            UnitKind::Undo { target: undone },
            "kernel.undo",
            taken_back("note/recipe", "first"),
            vec![(undone, UnitState::Undone { by: undo })],
        ),
    )
    .await;
    (undone, undo)
}

/// Holds `store` to holding nothing of what the journal never closed, once a
/// kernel opened over what `left` names.
async fn nothing_left_open_is_kept(store: &LocalStore, left: &Shapes) {
    assert_eq!(
        held(store, REFERENCE, "note/photo").await,
        Some(kept_as("kept")),
        "the step the journal never closed was reverted"
    );
    assert_eq!(held(store, REFERENCE, "note/diary").await, None);
    assert_eq!(
        held(store, REFERENCE, "note/recipe").await,
        Some(kept_as("first")),
        "the undo the journal never closed was reverted, and its step stands"
    );
    assert_eq!(state_of(store, left.kept).await, Some(UnitState::Committed));
    assert_eq!(
        state_of(store, left.undone).await,
        Some(UnitState::Committed),
        "the step the undo marked undone is committed again"
    );
    assert_eq!(state_of(store, left.nothing).await, None);
    assert_eq!(
        state_of(store, left.reversed).await,
        Some(UnitState::Undone {
            by: left.its_reversal
        }),
        "a unit reversed already is not reversed again"
    );
    for reverted in [left.left, left.undo] {
        let Some(UnitState::Undone { by }) = state_of(store, reverted).await else {
            panic!("{reverted} is reverted");
        };
        let reversal = store
            .unit(by)
            .await
            .expect("the store answers")
            .expect("the reversal is kept");
        assert_eq!(
            (reversal.row.meta.label.as_str(), reversal.row.meta.kind),
            ("kernel.revert", UnitKind::Undo { target: reverted }),
            "the kernel's own reversal, which no redo puts back"
        );
    }
}

/// A unit a stopped process left open in the journal is closed as aborted
/// when the kernel opens, and what the store kept of it is reverted first.
///
/// Every shape a stop can leave, written into one history and one store
/// before the kernel opens over them: a step kept, which stays; a step the
/// store committed and the journal never closed — the process stopped between
/// the two — which is reverted; a unit the store holds nothing of — stopped
/// while its command ran — and one the store holds reversed already — its
/// close refused, its reversal committed, its abort refused — which are only
/// closed; an undo the store committed and the journal never closed, whose
/// reversal commits its step again; and a unit another realm left open, which
/// is that realm's to recover and stays as it was.
#[tokio::test]
async fn a_unit_left_open_by_a_stopped_process_is_reverted_when_the_kernel_opens() {
    let store = in_memory().await;
    let (history, left) = left_in_every_shape(&store).await;
    let fixture = opened_over(store, history, "", false, GRANTED).await;
    nothing_left_open_is_kept(&fixture.store, &left).await;
    // The journal closes each as aborted, the newest first, and no other.
    let entries = fixture.entries().await;
    for unit in [left.left, left.nothing, left.reversed, left.undo] {
        assert_eq!(
            brackets_of(&entries, unit),
            [Outcome::UnitBegun, Outcome::UnitAborted],
            "{unit}"
        );
    }
    for unit in [left.kept, left.undone] {
        assert_eq!(
            brackets_of(&entries, unit),
            [Outcome::UnitBegun, Outcome::UnitCommitted],
            "{unit}"
        );
    }
    assert_eq!(
        brackets_of(&entries, left.elsewhere),
        [Outcome::UnitBegun],
        "another realm's unit is that realm's kernel's to recover"
    );
    let closes: Vec<Snowflake> = entries
        .iter()
        .filter(|entry| {
            entry.record.action == nc_journal::unit::CLOSES
                && entry.record.result == Outcome::UnitAborted
        })
        .filter_map(|entry| entry.record.work_unit)
        .collect();
    assert_eq!(closes, [left.undo, left.reversed, left.nothing, left.left]);
    // Each is logged, with the code a unit recovered is told by, saying
    // whether the store had kept it.
    let recovered = fixture.log.named("kernel.recovered");
    assert_eq!(
        recovered
            .iter()
            .map(|line| (
                line.text("unit"),
                line.fields.get("reverted").cloned(),
                line.text("code")
            ))
            .collect::<Vec<_>>(),
        [
            (left.undo, true),
            (left.reversed, false),
            (left.nothing, false),
            (left.left, true)
        ]
        .map(|(unit, reverted)| (
            Some(unit.to_crockford()),
            Some(Value::Bool(reverted)),
            Some(ErrorCode::KernelRecoveredUnit.code().to_owned())
        ))
    );
    assert_eq!(
        fixture.log.named("kernel.revert_failed"),
        [] as [nc_kernel::logging::CapturedLine; 0]
    );
    // And the chain the recovery wrote into verifies whole.
    let verified = fixture
        .as_ana("journal.verify", "{}")
        .await
        .expect("a guardian verifies the realm's journal");
    assert_eq!(
        field(&verified, "verified"),
        Value::Integer(i64::try_from(entries.len()).expect("fits")),
        "{verified}"
    );
}

/// The arguments of a request `POST`ed to `example.com` about `photo`.
///
/// With the compensation `compensate` names registered for it when one is
/// named, and the plugin's own `note.put` otherwise.
fn compensated_post(compensate: Option<(&str, &str)>) -> String {
    let mut fields = vec![
        ("note", Value::text("photo")),
        ("url", Value::text("https://example.com/")),
    ];
    if let Some((command, with)) = compensate {
        fields.push(("compensate", Value::text(command)));
        fields.push(("with", Value::text(with)));
    }
    Value::object(fields).to_canonical()
}

/// The arguments of a request `POST`ed to `example.com` about `photo`, with
/// no compensation.
fn uncompensated_post() -> String {
    Value::object([
        ("note", Value::text("photo")),
        ("url", Value::text("https://example.com/")),
    ])
    .to_canonical()
}

/// A note `note` holding `text`.
fn note_of(note: &str, text: &str) -> String {
    Value::object([("note", Value::text(note)), ("text", Value::text(text))]).to_canonical()
}

/// The field `name` of the canonical JSON object `answer`.
fn field(answer: &str, name: &str) -> Value {
    nc_canon::parse(answer)
        .unwrap_or_else(|refused| panic!("{answer} is not canonical JSON: {refused}"))
        .into_object()
        .and_then(|mut fields| fields.remove(name))
        .unwrap_or_else(|| panic!("{answer} holds no `{name}`"))
}

/// The command an entry's resource names, when it names one.
fn command_of(entry: &Entry) -> Option<String> {
    entry
        .record
        .resource
        .clone()
        .into_object()
        .and_then(|mut fields| fields.remove("command"))
        .and_then(Value::into_text)
}

/// The entries of `entries` that dispatched `command` and ended as `result`.
fn entries_of<'a>(entries: &'a [Entry], command: &str, result: Outcome) -> Vec<&'a Entry> {
    entries
        .iter()
        .filter(|entry| {
            command_of(entry).as_deref() == Some(command) && entry.record.result == result
        })
        .collect()
}

/// The failure an entry records, when it failed.
fn failure_of(entry: &Entry) -> Option<String> {
    entry
        .record
        .resource
        .clone()
        .into_object()
        .and_then(|mut fields| fields.remove("failure"))
        .and_then(Value::into_text)
}

/// What an undo or a redo answers of `step`: what it put back, and what was
/// beyond its reach.
fn reversed_as(
    step: nc_ids::Snowflake,
    writes: i64,
    blobs: i64,
    compensations: i64,
    beyond_reach: i64,
) -> String {
    Value::object([
        ("beyond_reach", Value::Integer(beyond_reach)),
        (
            "reverted",
            Value::object([
                ("blobs", Value::Integer(blobs)),
                ("compensations", Value::Integer(compensations)),
                ("writes", Value::Integer(writes)),
            ]),
        ),
        ("target", Value::text(step.to_crockford())),
    ])
    .to_canonical()
}

impl Fixture {
    /// Undoes `ana`'s newest step.
    async fn undo(&self) -> Result<String, Refusal> {
        self.as_ana("kernel.undo", "{}").await
    }

    /// Redoes the newest step `ana` undid.
    async fn redo(&self) -> Result<String, Refusal> {
        self.as_ana("kernel.redo", "{}").await
    }

    /// Every unit the store holds for `ana`, newest first, seeds included.
    async fn history(&self) -> Vec<UnitRow> {
        self.history_of("ana").await
    }

    /// Every unit the store holds for the user called `name`, newest first,
    /// seeds included.
    async fn history_of(&self, name: &str) -> Vec<UnitRow> {
        let realm = self.world.realm_id("home").expect("home is a realm");
        self.store
            .units(&realm, &id_of(&self.world, name), 100)
            .await
            .expect("the store answers")
    }

    /// `ana`'s newest step.
    async fn newest_step(&self) -> UnitRow {
        self.newest_step_of("ana").await
    }

    /// The newest step of the user called `name`.
    async fn newest_step_of(&self, name: &str) -> UnitRow {
        self.history_of(name)
            .await
            .into_iter()
            .find(|row| row.meta.kind == UnitKind::Step)
            .unwrap_or_else(|| panic!("{name} took a step"))
    }

    /// Where the unit `unit` stands.
    async fn state_of(&self, unit: nc_ids::Snowflake) -> UnitState {
        self.store
            .unit(unit)
            .await
            .expect("the store answers")
            .expect("the unit is held")
            .row
            .state
    }

    /// Whether the store holds the blob `hash` pinned.
    async fn pinned(&self, hash: &Hash) -> bool {
        self.store
            .blob(hash.as_str())
            .await
            .expect("the store answers")
            .is_some_and(|row| row.pinned)
    }
}

/// What both plugins hold as the note `photo`: `text`, each of them.
async fn both_hold(fixture: &Fixture, text: &str) {
    for plugin in [REFERENCE, THE_PEER] {
        assert_eq!(
            held(&fixture.store, plugin, "note/photo").await,
            Some(kept_as(text)),
            "`{plugin}` does not hold {text:?}"
        );
    }
}

/// Undo and redo, dispatched as any command is, reverse and restore one unit
/// that wrote in two plugins.
///
/// Both keys go back together and come back together, the step's state moves
/// with them in the store's own transaction, the journal's entry and the log
/// name the step and what of it was put back, and a step redone is one undo
/// reaches again.
#[tokio::test]
async fn undo_and_redo_through_dispatch_reverse_and_restore_a_unit_that_touched_two_plugins() {
    let fixture = opened(in_memory().await, "", true).await;
    fixture
        .as_ana("reference.pair.put", &photo("before"))
        .await
        .expect("written in both plugins");
    fixture
        .as_ana("reference.pair.put", &photo("after"))
        .await
        .expect("written in both plugins");
    let step = fixture.newest_step().await;
    let undone = fixture.undo().await.expect("undone");
    assert_eq!(undone, reversed_as(step.unit, 2, 0, 0, 0));
    both_hold(&fixture, "before").await;
    let undo = fixture.history().await.remove(0);
    assert_eq!(
        (undo.meta.kind, undo.meta.label.as_str()),
        (UnitKind::Undo { target: step.unit }, "kernel.undo")
    );
    assert_eq!(
        fixture.state_of(step.unit).await,
        UnitState::Undone { by: undo.unit }
    );
    let entries = fixture.entries().await;
    let entry = entries_of(&entries, "kernel.undo", Outcome::Allowed);
    assert_eq!(entry.len(), 1, "{entries:#?}");
    assert_eq!(
        entry[0].record.resource.to_canonical(),
        format!(
            r#"{{"command":"kernel.undo","entity":{{"id":"home","type":"Realm"}},"reverted":{{"blobs":0,"compensations":0,"writes":2}},"target":"{}"}}"#,
            step.unit.to_crockford()
        )
    );
    assert_eq!(entry[0].record.work_unit, Some(undo.unit));
    assert_eq!(
        entry[0].record.deciding_policy.as_deref(),
        Some("family.a_member_steps_through_their_own_history")
    );
    assert_eq!(
        fixture.logged("kernel.step_reversed", "direction"),
        ["undo"]
    );
    assert_eq!(
        fixture.logged("kernel.step_reversed", "unit"),
        [step.unit.to_crockford()]
    );
    assert_eq!(
        fixture.logged("kernel.step_reversed", "compensations"),
        ["0"]
    );
    assert_eq!(
        fixture.logged("kernel.step_reversed", "beyond_reach"),
        ["0"]
    );

    let redone = fixture.redo().await.expect("redone");
    assert_eq!(redone, reversed_as(step.unit, 2, 0, 0, 0));
    both_hold(&fixture, "after").await;
    let redo = fixture.history().await.remove(0);
    assert_eq!(redo.meta.kind, UnitKind::Redo { target: undo.unit });
    assert_eq!(fixture.state_of(step.unit).await, UnitState::Committed);
    assert_eq!(
        fixture.state_of(undo.unit).await,
        UnitState::Undone { by: redo.unit }
    );
    assert_eq!(
        fixture.logged("kernel.step_reversed", "direction"),
        ["undo", "redo"]
    );
    let entries = fixture.entries().await;
    assert_eq!(
        entries_of(&entries, "kernel.redo", Outcome::Allowed)[0]
            .record
            .work_unit,
        Some(redo.unit)
    );

    fixture.undo().await.expect("a step redone is undone again");
    both_hold(&fixture, "before").await;
    let verified = fixture
        .as_ana("journal.verify", "{}")
        .await
        .expect("the journal is read");
    assert!(verified.starts_with(r#"{"verified":"#), "{verified}");
}

/// An undo is refused when what the step changed has been changed since, and
/// changes nothing; a redo is refused when what the undo put back has.
#[tokio::test]
async fn undo_refuses_when_what_the_step_changed_has_changed_since() {
    let fixture = opened(in_memory().await, "", false).await;
    fixture
        .as_ana("reference.note.put", &photo("kept"))
        .await
        .expect("written");
    let step = fixture.newest_step().await;
    // **Another person's step changes the note**, which ana's history does not
    // hold: ana's newest step is still the one that wrote "kept".
    seeded_by(
        &fixture.store,
        "bruno",
        REFERENCE,
        "note/photo",
        &kept_as("changed"),
    )
    .await;
    let refused = fixture.undo().await;
    assert!(
        is(
            &refused,
            RefusalKind::Conflict,
            ErrorCode::KernelUndoConflict
        ),
        "{refused:?}"
    );
    assert_eq!(
        held(&fixture.store, REFERENCE, "note/photo").await,
        Some(kept_as("changed")),
        "an undo refused changed what it found"
    );
    assert_eq!(fixture.state_of(step.unit).await, UnitState::Committed);
    let entries = fixture.entries().await;
    let failed = entries_of(&entries, "kernel.undo", Outcome::Failed);
    assert_eq!(failed.len(), 1);
    assert_eq!(failure_of(failed[0]).as_deref(), Some("conflict"));
    assert_eq!(
        brackets(&entries)[2..],
        [Outcome::UnitBegun, Outcome::UnitAborted]
    );

    let fixture = opened(in_memory().await, "", false).await;
    fixture
        .as_ana("reference.note.put", &photo("written"))
        .await
        .expect("written");
    fixture.undo().await.expect("undone");
    assert_eq!(held(&fixture.store, REFERENCE, "note/photo").await, None);
    seeded_by(
        &fixture.store,
        "bruno",
        REFERENCE,
        "note/photo",
        &kept_as("since"),
    )
    .await;
    let refused = fixture.redo().await;
    assert!(
        is(
            &refused,
            RefusalKind::Conflict,
            ErrorCode::KernelUndoConflict
        ),
        "{refused:?}"
    );
    assert_eq!(
        held(&fixture.store, REFERENCE, "note/photo").await,
        Some(kept_as("since"))
    );
    // **Refused by the redo before anything is staged**, not by the commit
    // after: the commit's own check answers with the same code, and a redo that
    // reached it was journaled as allowed, having put a key back.
    let entries = fixture.entries().await;
    assert_eq!(
        entries_of(&entries, "kernel.redo", Outcome::Failed)
            .iter()
            .map(|entry| failure_of(entry))
            .collect::<Vec<_>>(),
        [Some("conflict".to_owned())]
    );
    assert!(
        entries_of(&entries, "kernel.redo", Outcome::Allowed).is_empty(),
        "a redo that put nothing back was journaled as allowed: {entries:#?}"
    );
}

/// An undo dispatches the compensation its step registered, and a redo of it
/// is then refused.
///
/// The compensation runs nested in the undo's own unit and as the person who
/// asked, what it writes is kept, and the undo says how many ran and what was
/// beyond its reach.
#[tokio::test]
async fn undo_dispatches_compensations_and_redo_then_refuses() {
    let fixture = opened(in_memory().await, "", false).await;
    let posted = fixture
        .as_ana("reference.compensated.post", &compensated_post(None))
        .await
        .expect("sent");
    assert_eq!(field(&posted, "compensation"), Value::text("registered"));
    let step = fixture.newest_step().await;
    let undone = fixture.undo().await.expect("undone, its compensation run");
    assert_eq!(undone, reversed_as(step.unit, 0, 0, 1, 1));
    assert_eq!(
        held(&fixture.store, REFERENCE, "note/photo").await,
        Some(kept_as("compensated")),
        "the compensation ran, and what it wrote is kept with the undo"
    );
    let undo = fixture.history().await.remove(0);
    let entries = fixture.entries().await;
    let compensation = entries_of(&entries, "reference.note.put", Outcome::Allowed);
    assert_eq!(compensation.len(), 1, "{entries:#?}");
    assert_eq!(
        compensation[0].record.work_unit,
        Some(undo.unit),
        "dispatched inside the undo's own unit"
    );
    assert_eq!(
        compensation[0].record.principal,
        id_of(&fixture.world, "ana")
    );
    assert_eq!(
        fixture.logged("kernel.step_reversed", "compensations"),
        ["1"]
    );
    assert_eq!(
        fixture.logged("kernel.step_reversed", "beyond_reach"),
        ["1"]
    );

    let refused = fixture.redo().await;
    assert!(
        is(
            &refused,
            RefusalKind::Denied,
            ErrorCode::KernelRedoCompensated
        ),
        "{refused:?}"
    );
    assert_eq!(
        fixture.state_of(step.unit).await,
        UnitState::Undone { by: undo.unit }
    );
}

/// An undo whose compensation fails is refused in the compensation's own case,
/// and nothing of it is kept: the step stays committed, and what the
/// compensation wrote before it failed is not kept either.
#[tokio::test]
async fn an_undo_whose_compensation_fails_keeps_nothing() {
    let fixture = opened(in_memory().await, "", false).await;
    fixture
        .as_ana("reference.note.put", &photo("kept"))
        .await
        .expect("written");
    let posted = fixture
        .as_ana(
            "reference.compensated.post",
            &compensated_post(Some(("reference.note.put-then-fail", &photo("half done")))),
        )
        .await
        .expect("sent");
    assert_eq!(field(&posted, "compensation"), Value::text("registered"));
    let step = fixture.newest_step().await;
    let refused = fixture.undo().await.expect_err("the compensation failed");
    assert_eq!(
        (refused.kind(), refused.code()),
        (RefusalKind::Invalid, ErrorCode::KernelCompensationFailed),
        "{refused}"
    );
    assert!(
        refused
            .sentence()
            .contains(&ErrorCode::PluginCommandRefused.to_string()),
        "{refused}"
    );
    assert_eq!(
        held(&fixture.store, REFERENCE, "note/photo").await,
        Some(kept_as("kept")),
        "what the failed compensation wrote was kept"
    );
    assert_eq!(fixture.state_of(step.unit).await, UnitState::Committed);
    assert_eq!(
        fixture.logged("kernel.step_reversed", "unit"),
        [] as [String; 0]
    );
}

/// A compensation that answers having let the undo's unit go fails the undo,
/// and no compensation after it is dispatched.
///
/// The step registered `unit.drop` twice, which writes a note in a unit it
/// begins and lets that unit go: the first one dispatched dooms the undo's
/// own unit, so a second would run for an undo that is not kept, and run
/// again when the undo is asked for again. The undo is refused with the
/// history's code, in the case of what doomed it, and the step stays
/// committed.
///
/// Found by a review: both were dispatched and journaled as allowed, and the
/// undo answered `[NC-KERN-0008]` rather than the history's own refusal.
#[tokio::test]
async fn a_compensation_that_lets_the_undos_unit_go_stops_the_ones_after_it() {
    let fixture = opened(in_memory().await, "", false).await;
    let args = Value::object([
        ("compensate", Value::text("reference.unit.drop")),
        ("note", Value::text("photo")),
        ("then", Value::text(photo("dropped second"))),
        ("url", Value::text("https://example.com/")),
        ("with", Value::text(photo("dropped first"))),
    ])
    .to_canonical();
    let posted = fixture
        .as_ana("reference.compensated.post", &args)
        .await
        .expect("sent");
    assert_eq!(field(&posted, "compensation"), Value::text("registered"));
    let step = fixture.newest_step().await;
    let refused = fixture
        .undo()
        .await
        .expect_err("the compensation let the undo's unit go");
    assert_eq!(
        (refused.kind(), refused.code()),
        (RefusalKind::Internal, ErrorCode::KernelCompensationFailed),
        "{refused}"
    );
    assert!(
        refused.sentence().contains("`reference.unit.drop`")
            && refused
                .sentence()
                .contains(&ErrorCode::KernelUnitAborted.to_string()),
        "{refused}"
    );
    let entries = fixture.entries().await;
    assert_eq!(
        entries_of(&entries, "reference.unit.drop", Outcome::Allowed).len(),
        1,
        "a compensation was dispatched after the undo's unit was let go: {entries:#?}"
    );
    let undo = entries_of(&entries, "kernel.undo", Outcome::Failed);
    assert_eq!(undo.len(), 1, "{entries:#?}");
    assert_eq!(failure_of(undo[0]).as_deref(), Some("compensation-failed"));
    assert_eq!(fixture.state_of(step.unit).await, UnitState::Committed);
    assert_eq!(held(&fixture.store, REFERENCE, "note/photo").await, None);
    assert_eq!(
        fixture.logged("kernel.step_reversed", "unit"),
        [] as [String; 0]
    );
}

/// A compensation that could never run is refused when a plugin registers
/// it, and the request it was for, sent with none registered, is then a step
/// undo does not pass.
///
/// Refused, each for its own reason: one that only reads, one that cannot be
/// undone, one that deletes, undo itself, another command of the kernel's own,
/// one that names no command, a name in no command's shape, and arguments its
/// command's schema refuses.
///
/// **And the reason a deletion is refused is the core's**: dispatched nested,
/// as a compensation would be, the peer's own deletion is denied by the rule
/// that asks a quorum's approval of every one — the rule the registration
/// reads, so a core that stops asking it fails here.
#[tokio::test]
async fn a_compensation_that_could_never_run_is_refused_when_it_is_registered() {
    let fixture = opened(in_memory().await, "", true).await;
    let note = r#"{"note":"photo"}"#;
    let refused_as = |case: &str, code: ErrorCode| format!("{case}: [{code}] ");
    let never = refused_as("invalid", ErrorCode::KernelCompensationRefused);
    for (compensate, with, answered, because) in [
        ("reference.note.get", note, never.clone(), "changes nothing"),
        (
            "reference.irreversible.do",
            note,
            never.clone(),
            "cannot be undone",
        ),
        ("reference.note.delete", note, never.clone(), "deletes"),
        ("kernel.undo", "{}", never.clone(), "top of a call tree"),
        ("limits.prune", "{}", never.clone(), "the kernel's own"),
        (
            "reference.nothing.here",
            note,
            refused_as("not-found", ErrorCode::KernelUnknownCommand),
            "no command",
        ),
        (
            "Not A Name",
            "{}",
            refused_as("invalid", ErrorCode::KernelMalformedName),
            "not a command's name",
        ),
        (
            "reference.note.put",
            note,
            refused_as("invalid", ErrorCode::ValidationArgumentsInvalid),
            "",
        ),
    ] {
        let posted = fixture
            .as_ana(
                "reference.compensated.post",
                &compensated_post(Some((compensate, with))),
            )
            .await
            .expect("sent");
        let compensation = field(&posted, "compensation")
            .into_text()
            .expect("the compensation's answer");
        assert!(
            compensation.starts_with(&answered) && compensation.contains(because),
            "`{compensate}`, refused for what {because:?} says: {compensation}"
        );
    }
    let refused = fixture.undo().await;
    assert!(
        is(&refused, RefusalKind::Denied, ErrorCode::KernelUndoBarrier),
        "{refused:?}"
    );

    let relayed = Value::object([
        ("args", Value::text(note)),
        ("name", Value::text("peer.note.delete")),
        ("note", Value::text("notice")),
    ])
    .to_canonical();
    let answered = fixture
        .as_ana("reference.relay.write", &relayed)
        .await
        .expect("the relay answers");
    let refused = field(&answered, "refused")
        .into_text()
        .expect("the nested deletion was refused");
    assert!(
        refused.starts_with(&format!("denied: [{}] ", ErrorCode::PolicyDenied)),
        "{refused}"
    );
    let entries = fixture.entries().await;
    assert_eq!(
        entries_of(&entries, "peer.note.delete", Outcome::Denied)
            .iter()
            .map(|entry| entry.record.deciding_policy.as_deref())
            .collect::<Vec<_>>(),
        [Some("core.destructive_needs_quorum")],
        "{entries:#?}"
    );
}

/// A compensation naming a command not its plugin's own is a dispatch put off
/// until the undo, and a plugin not granted `commands.dispatch` is refused it.
///
/// Without that grant the reference plugin can neither dispatch its peer's
/// command nor have an undo dispatch it as ana by naming it as a compensation:
/// the request it was for is then a barrier, and nothing of the peer's is
/// written. A command of the kernel's is no more its own; one of its own it
/// names as before, and an undo runs it.
///
/// Found by a review: `commands.begin` alone let a plugin have its peer's
/// command dispatched as the person, the plugin's own dispatch of the same
/// command being refused.
#[tokio::test]
async fn a_plugin_not_granted_dispatch_compensates_only_with_its_own_commands() {
    let fixture = opened_granting(in_memory().await, "", true, WITHOUT_DISPATCH).await;
    let denied = format!("denied: [{}] ", ErrorCode::PluginCapabilityDenied);
    let relayed = Value::object([
        ("args", Value::text(photo("relayed"))),
        ("name", Value::text("peer.note.put")),
        ("note", Value::text("notice")),
    ])
    .to_canonical();
    let answered = fixture
        .as_ana("reference.relay.write", &relayed)
        .await
        .expect("the relay answers");
    let refused = field(&answered, "refused")
        .into_text()
        .expect("the relay was refused");
    assert!(refused.starts_with(&denied), "{refused}");
    for (compensate, with) in [
        ("kernel.undo", "{}".to_owned()),
        ("peer.note.put", photo("planted")),
    ] {
        let posted = fixture
            .as_ana(
                "reference.compensated.post",
                &compensated_post(Some((compensate, &with))),
            )
            .await
            .expect("sent");
        let compensation = field(&posted, "compensation")
            .into_text()
            .expect("the compensation's answer");
        assert!(
            compensation.starts_with(&denied) && compensation.contains("`commands.dispatch`"),
            "`{compensate}`: {compensation}"
        );
    }
    let refused = fixture.undo().await;
    assert!(
        is(&refused, RefusalKind::Denied, ErrorCode::KernelUndoBarrier),
        "{refused:?}"
    );
    assert_eq!(held(&fixture.store, THE_PEER, "note/photo").await, None);
    let entries = fixture.entries().await;
    assert!(
        entries_of(&entries, "peer.note.put", Outcome::Allowed).is_empty(),
        "{entries:#?}"
    );

    let fixture = opened_granting(in_memory().await, "", true, WITHOUT_DISPATCH).await;
    let posted = fixture
        .as_ana("reference.compensated.post", &compensated_post(None))
        .await
        .expect("sent");
    assert_eq!(field(&posted, "compensation"), Value::text("registered"));
    fixture
        .undo()
        .await
        .expect("undone, its own compensation run");
    assert_eq!(
        held(&fixture.store, REFERENCE, "note/photo").await,
        Some(kept_as("compensated"))
    );
}

/// A compensation is dispatched only to the plugin that held its command when
/// it was registered: once that plugin is unloaded and another identity takes
/// its namespace and declares the same command, the undo dispatches nothing
/// and is refused — and the plugin that registered it, loaded again, is the
/// one the undo reaches.
///
/// **Written by a reviewer, who found a real bug with it**: bound to the
/// command's name alone, the undo dispatched the other plugin's command as
/// ana, in her undo's unit, and that compensation lifted the barrier the
/// step's request would have been.
#[tokio::test]
async fn a_compensation_never_reaches_a_plugin_that_took_its_command_since() {
    let fixture = opened_granting(in_memory().await, "", false, WITHOUT_DISPATCH).await;
    let posted = fixture
        .as_ana("reference.compensated.post", &compensated_post(None))
        .await
        .expect("sent");
    assert_eq!(field(&posted, "compensation"), Value::text("registered"));
    assert!(fixture.kernel.unload(fixture.slots[0]).await, "unloaded");
    let component = nc_kernel::component_for_tests("reference");
    let impostor = fixture
        .kernel
        .load(manifest("noctua:impostor", "reference", ""), &component)
        .await
        .expect("another plugin takes the namespace the first left");
    let refused = fixture.undo().await;
    assert!(
        is(
            &refused,
            RefusalKind::Denied,
            ErrorCode::KernelCompensationMoved
        ),
        "{refused:?}"
    );
    assert_eq!(
        held(&fixture.store, "noctua:impostor", "note/photo").await,
        None
    );
    let entries = fixture.entries().await;
    assert!(
        entries_of(&entries, "reference.note.put", Outcome::Allowed).is_empty(),
        "the impostor's command was dispatched as ana: {entries:#?}"
    );
    assert!(
        fixture.kernel.unload(impostor).await,
        "the impostor unloaded"
    );
    fixture
        .kernel
        .load(manifest(REFERENCE, "reference", ""), &component)
        .await
        .expect("the plugin that registered the compensation, loaded again");
    fixture
        .undo()
        .await
        .expect("undone, the compensation dispatched to the plugin that registered it");
    assert_eq!(
        held(&fixture.store, REFERENCE, "note/photo").await,
        Some(kept_as("compensated"))
    );
}

/// A compensation naming another plugin's command is dispatched by an undo
/// only while the plugin that registered it is loaded granted
/// `commands.dispatch`, as it had to be to register it: loaded again without
/// the grant, the undo dispatches nothing and is refused, and loaded again
/// with it, the undo runs it.
///
/// **Written by a reviewer (wave 5, PEP), and it found a real bug**: the undo
/// asked only that the command's holder was unchanged, so it dispatched the
/// peer's command as ana for a plugin no longer granted to — and the
/// compensation lifted the barrier the step's request would have been.
#[tokio::test]
async fn a_compensation_is_not_dispatched_for_a_plugin_no_longer_granted_dispatch() {
    let fixture = opened(in_memory().await, "", true).await;
    let posted = fixture
        .as_ana(
            "reference.compensated.post",
            &compensated_post(Some(("peer.note.put", &photo("planted")))),
        )
        .await
        .expect("sent");
    assert_eq!(field(&posted, "compensation"), Value::text("registered"));
    assert!(fixture.kernel.unload(fixture.slots[0]).await, "unloaded");
    let component = nc_kernel::component_for_tests("reference");
    let again = fixture
        .kernel
        .load(
            manifest_granting(REFERENCE, "reference", "", WITHOUT_DISPATCH),
            &component,
        )
        .await
        .expect("loaded again, not granted `commands.dispatch`");
    let refused = fixture.undo().await;
    assert!(
        is(
            &refused,
            RefusalKind::Denied,
            ErrorCode::KernelCompensationUngranted
        ),
        "{refused:?}"
    );
    assert_eq!(
        held(&fixture.store, THE_PEER, "note/photo").await,
        None,
        "the peer's command was dispatched as ana for a plugin not granted \
         `commands.dispatch`"
    );
    assert!(fixture.kernel.unload(again).await, "unloaded again");
    fixture
        .kernel
        .load(manifest(REFERENCE, "reference", ""), &component)
        .await
        .expect("loaded again, granted `commands.dispatch`");
    fixture
        .undo()
        .await
        .expect("undone, the compensation dispatched for the plugin granted to");
    assert_eq!(
        held(&fixture.store, THE_PEER, "note/photo").await,
        Some(kept_as("planted"))
    );
}

/// A request that changes something elsewhere, sent with no compensation, is a
/// barrier: undo refuses it as often as it is asked, and never reaches the
/// step before it. A command that cannot be undone is the same barrier.
#[tokio::test]
async fn an_uncompensated_post_is_a_barrier_undo_does_not_pass() {
    let fixture = opened(in_memory().await, "", false).await;
    fixture
        .as_ana("reference.note.put", &photo("kept"))
        .await
        .expect("written");
    let before = fixture.newest_step().await;
    fixture
        .as_ana("reference.effect.post", &uncompensated_post())
        .await
        .expect("sent");
    let barrier = fixture.newest_step().await;
    for _ in 0..2 {
        let refused = fixture.undo().await;
        assert!(
            is(&refused, RefusalKind::Denied, ErrorCode::KernelUndoBarrier),
            "{refused:?}"
        );
    }
    assert_eq!(
        held(&fixture.store, REFERENCE, "note/photo").await,
        Some(kept_as("kept")),
        "undo reached past the barrier"
    );
    for step in [before.unit, barrier.unit] {
        assert_eq!(fixture.state_of(step).await, UnitState::Committed);
    }
    let entries = fixture.entries().await;
    let failed = entries_of(&entries, "kernel.undo", Outcome::Failed);
    assert_eq!(
        failed
            .iter()
            .map(|entry| failure_of(entry))
            .collect::<Vec<_>>(),
        [Some("barrier".to_owned()), Some("barrier".to_owned())]
    );

    let fixture = opened(in_memory().await, "", false).await;
    let ana = fixture
        .world
        .principal(PrincipalKind::User, "ana")
        .expect("ana is a user of the world");
    let args = Value::object([("note", Value::text("photo"))]).to_canonical();
    let confirmation =
        fixture
            .kernel
            .confirmation_for_tests(&ana, "reference.irreversible.do", &args);
    fixture
        .kernel
        .dispatch(
            &Caller::new(ana).confirmed(confirmation),
            "reference.irreversible.do",
            &args,
        )
        .await
        .expect("done, confirmed");
    let refused = fixture.undo().await;
    assert!(
        is(&refused, RefusalKind::Denied, ErrorCode::KernelUndoBarrier),
        "{refused:?}"
    );
    assert_eq!(
        held(&fixture.store, REFERENCE, "done/photo")
            .await
            .as_deref(),
        Some("true")
    );
}

/// New work ends the way back.
///
/// A redo after an undo puts the step back, and after a new step there is
/// nothing to redo. Two steps undone are redone in the order they were undone
/// back, and with no history at all there is nothing to undo or redo.
#[tokio::test]
async fn new_work_clears_redo() {
    let fixture = opened(in_memory().await, "", false).await;
    for (refused, code) in [
        (fixture.undo().await, ErrorCode::KernelNothingToUndo),
        (fixture.redo().await, ErrorCode::KernelNothingToRedo),
    ] {
        assert!(is(&refused, RefusalKind::NotFound, code), "{refused:?}");
    }
    fixture
        .as_ana("reference.note.put", &photo("one"))
        .await
        .expect("written");
    fixture.undo().await.expect("undone");
    fixture
        .redo()
        .await
        .expect("redone, with no new work since");
    assert_eq!(
        held(&fixture.store, REFERENCE, "note/photo").await,
        Some(kept_as("one"))
    );
    let again = fixture.redo().await;
    assert!(
        is(
            &again,
            RefusalKind::NotFound,
            ErrorCode::KernelNothingToRedo
        ),
        "redone already: {again:?}"
    );
    fixture.undo().await.expect("undone again");
    assert_eq!(held(&fixture.store, REFERENCE, "note/photo").await, None);
    fixture
        .as_ana("reference.note.put", &photo("two"))
        .await
        .expect("new work");
    let after = fixture.redo().await;
    assert!(
        is(
            &after,
            RefusalKind::NotFound,
            ErrorCode::KernelNothingToRedo
        ),
        "new work ended the way back: {after:?}"
    );
    assert_eq!(
        held(&fixture.store, REFERENCE, "note/photo").await,
        Some(kept_as("two"))
    );

    let fixture = opened(in_memory().await, "", false).await;
    fixture
        .as_ana("reference.note.put", &note_of("photo", "a"))
        .await
        .expect("written");
    fixture
        .as_ana("reference.note.put", &note_of("notice", "b"))
        .await
        .expect("written");
    fixture.undo().await.expect("the notice undone");
    fixture.undo().await.expect("the photo undone");
    fixture.redo().await.expect("the photo redone");
    assert_eq!(
        (
            held(&fixture.store, REFERENCE, "note/photo").await,
            held(&fixture.store, REFERENCE, "note/notice").await
        ),
        (Some(kept_as("a")), None),
        "the step undone last is the one redone first"
    );
    fixture.redo().await.expect("the notice redone");
    assert_eq!(
        held(&fixture.store, REFERENCE, "note/notice").await,
        Some(kept_as("b"))
    );
}

/// A step undone and redone more often than the history's bound counts units
/// is still the step an undo reaches.
///
/// An undo looks through the person's newest steps, so the undos and redos
/// between them do not count against `undo_history`: after half that many
/// rounds the person holds more units than the bound, and the step is undone
/// once more.
///
/// Found by a review: the bound counted every unit, the newest of which were
/// then the rounds' own undos and redos, and the step was nothing to undo.
#[tokio::test]
async fn a_step_redone_more_often_than_the_bound_counts_units_is_still_undone() {
    let fixture = opened(in_memory().await, "", false).await;
    fixture
        .as_ana("reference.note.put", &photo("kept"))
        .await
        .expect("written");
    let step = fixture.newest_step().await;
    let bound = nc_catalog::KERNEL.undo_history;
    for round in 0..bound / 2 {
        fixture
            .undo()
            .await
            .unwrap_or_else(|refused| panic!("round {round}: {refused}"));
        fixture
            .redo()
            .await
            .unwrap_or_else(|refused| panic!("round {round}: {refused}"));
    }
    let realm = fixture.world.realm_id("home").expect("home is a realm");
    let units = fixture
        .store
        .units(
            &realm,
            &id_of(&fixture.world, "ana"),
            bound.saturating_mul(2),
        )
        .await
        .expect("the store answers");
    assert!(
        units.len() > usize::try_from(bound).expect("fits"),
        "{} unit(s), within the bound of {bound}: the rounds pushed nothing past it",
        units.len()
    );
    assert_eq!(fixture.state_of(step.unit).await, UnitState::Committed);
    let undone = fixture
        .undo()
        .await
        .expect("the step redone is still the newest to undo");
    assert_eq!(undone, reversed_as(step.unit, 1, 0, 0, 0));
    assert_eq!(held(&fixture.store, REFERENCE, "note/photo").await, None);
}

/// An undo authorizes again, now, what its step was allowed.
///
/// Once the photo is no longer ana's, writing it back is not hers either —
/// while undoing still is — and the denial is journaled with what decided it,
/// in the undo's own unit, and told to her without the policy.
#[tokio::test]
async fn undo_is_re_authorized_now() {
    let fixture = opened(in_memory().await, "", false).await;
    fixture
        .as_ana("reference.note.put", &photo("kept"))
        .await
        .expect("written: the photo is hers");
    let step = fixture.newest_step().await;
    fixture.shift.store(true, Ordering::SeqCst);
    let refused = fixture.undo().await.expect_err("denied now");
    assert_eq!(
        (refused.kind(), refused.code()),
        (RefusalKind::Denied, ErrorCode::PolicyDenied),
        "{refused}"
    );
    assert!(
        !refused.sentence().contains("kernel.no_permit_matched"),
        "the caller was told what decided: {refused}"
    );
    assert_eq!(
        held(&fixture.store, REFERENCE, "note/photo").await,
        Some(kept_as("kept"))
    );
    assert_eq!(fixture.state_of(step.unit).await, UnitState::Committed);
    let entries = fixture.entries().await;
    let denied = entries_of(&entries, "kernel.undo", Outcome::Denied);
    assert_eq!(denied.len(), 1, "{entries:#?}");
    assert_eq!(denied[0].record.action, "Write");
    assert_eq!(
        denied[0].record.resource.to_canonical(),
        r#"{"command":"kernel.undo","entity":{"id":"photo","type":"Content"}}"#
    );
    assert_eq!(
        denied[0].record.deciding_policy.as_deref(),
        Some("kernel.no_permit_matched")
    );
    let failed = entries_of(&entries, "kernel.undo", Outcome::Failed);
    assert_eq!(failure_of(failed[0]).as_deref(), Some("denied-now"));
    assert_eq!(
        denied[0].record.work_unit, failed[0].record.work_unit,
        "the denial is journaled in the undo's own unit"
    );
    assert_eq!(
        fixture.logged("kernel.dispatch_refused", "reason"),
        ["denied"]
    );

    fixture.shift.store(false, Ordering::SeqCst);
    fixture
        .undo()
        .await
        .expect("the photo hers again, so is undoing what she wrote");
    assert_eq!(held(&fixture.store, REFERENCE, "note/photo").await, None);

    // **And with what she holds now**: a delete her quorum approved is
    // undone only with an approval of the undo, the quorum the core asks of
    // every `Delete` being asked again.
    let fixture = opened(in_memory().await, "", false).await;
    let ana = fixture
        .world
        .principal(PrincipalKind::User, "ana")
        .expect("ana is a user of the world");
    fixture
        .as_ana("reference.note.put", &photo("kept"))
        .await
        .expect("written");
    let note = Value::object([("note", Value::text("photo"))]).to_canonical();
    let approval = fixture
        .kernel
        .approval_for_tests(&ana, "reference.note.delete", &note);
    fixture
        .kernel
        .dispatch(
            &Caller::new(ana.clone()).approved(approval),
            "reference.note.delete",
            &note,
        )
        .await
        .expect("deleted, a quorum approving");
    let refused = fixture.undo().await;
    assert!(
        refused
            .as_ref()
            .is_err_and(|refusal| refusal.kind() == RefusalKind::Denied),
        "{refused:?}"
    );
    let entries = fixture.entries().await;
    assert_eq!(
        entries_of(&entries, "kernel.undo", Outcome::Denied)[0]
            .record
            .deciding_policy
            .as_deref(),
        Some("core.destructive_needs_quorum")
    );
    assert_eq!(held(&fixture.store, REFERENCE, "note/photo").await, None);
    let approval = fixture.kernel.approval_for_tests(&ana, "kernel.undo", "{}");
    fixture
        .kernel
        .dispatch(&Caller::new(ana).approved(approval), "kernel.undo", "{}")
        .await
        .expect("undone, a quorum approving the undo");
    assert_eq!(
        held(&fixture.store, REFERENCE, "note/photo").await,
        Some(kept_as("kept"))
    );
}

/// A redo authorizes again, now, what the step it puts back was allowed.
///
/// Undone while the photo was ana's, the step is not redone once it is no
/// longer hers: nothing is put back, the step stays undone, and the denial is
/// journaled with what decided it. Once the photo is hers again, so is the
/// redo.
#[tokio::test]
async fn redo_is_re_authorized_now() {
    let fixture = opened(in_memory().await, "", false).await;
    fixture
        .as_ana("reference.note.put", &photo("kept"))
        .await
        .expect("written: the photo is hers");
    let step = fixture.newest_step().await;
    fixture
        .undo()
        .await
        .expect("undone while the photo is hers");
    let undo = fixture.history().await.remove(0);
    fixture.shift.store(true, Ordering::SeqCst);
    let refused = fixture.redo().await.expect_err("denied now");
    assert_eq!(
        (refused.kind(), refused.code()),
        (RefusalKind::Denied, ErrorCode::PolicyDenied),
        "{refused}"
    );
    assert_eq!(
        held(&fixture.store, REFERENCE, "note/photo").await,
        None,
        "a redo denied put the note back"
    );
    assert_eq!(
        fixture.state_of(step.unit).await,
        UnitState::Undone { by: undo.unit }
    );
    assert_eq!(fixture.state_of(undo.unit).await, UnitState::Committed);
    let entries = fixture.entries().await;
    let denied = entries_of(&entries, "kernel.redo", Outcome::Denied);
    assert_eq!(denied.len(), 1, "{entries:#?}");
    assert_eq!(
        denied[0].record.resource.to_canonical(),
        r#"{"command":"kernel.redo","entity":{"id":"photo","type":"Content"}}"#
    );
    assert_eq!(
        denied[0].record.deciding_policy.as_deref(),
        Some("kernel.no_permit_matched")
    );
    let failed = entries_of(&entries, "kernel.redo", Outcome::Failed);
    assert_eq!(failure_of(failed[0]).as_deref(), Some("denied-now"));

    fixture.shift.store(false, Ordering::SeqCst);
    fixture
        .redo()
        .await
        .expect("the photo hers again, so is redoing what she wrote");
    assert_eq!(
        held(&fixture.store, REFERENCE, "note/photo").await,
        Some(kept_as("kept"))
    );
}

/// An undo authorizes again what a command nested in its step was allowed, as
/// well as what the step's own command was.
///
/// The relay, a command about the note `notice`, writes the peer's photo
/// through a command of the peer's; once the photo is no longer ana's, the
/// undo is refused for the photo, and the peer still holds what was relayed.
#[tokio::test]
async fn undo_authorizes_again_what_a_nested_command_was_allowed() {
    let fixture = opened(in_memory().await, "", true).await;
    let relayed = Value::object([
        ("args", Value::text(photo("relayed"))),
        ("name", Value::text("peer.note.put")),
        ("note", Value::text("notice")),
    ])
    .to_canonical();
    let answered = fixture
        .as_ana("reference.relay.write", &relayed)
        .await
        .expect("relayed");
    assert_eq!(
        field(&answered, "answered"),
        Value::Integer(1),
        "{answered}"
    );
    assert_eq!(
        held(&fixture.store, THE_PEER, "note/photo").await,
        Some(kept_as("relayed"))
    );
    let step = fixture.newest_step().await;
    fixture.shift.store(true, Ordering::SeqCst);
    let refused = fixture.undo().await.expect_err("denied now");
    assert_eq!(
        (refused.kind(), refused.code()),
        (RefusalKind::Denied, ErrorCode::PolicyDenied),
        "{refused}"
    );
    assert_eq!(
        held(&fixture.store, THE_PEER, "note/photo").await,
        Some(kept_as("relayed")),
        "the nested write was undone on a note that is no longer ana's"
    );
    assert_eq!(fixture.state_of(step.unit).await, UnitState::Committed);
    let entries = fixture.entries().await;
    let denied = entries_of(&entries, "kernel.undo", Outcome::Denied);
    assert_eq!(
        denied
            .iter()
            .map(|entry| entry.record.resource.to_canonical())
            .collect::<Vec<_>>(),
        [r#"{"command":"kernel.undo","entity":{"id":"photo","type":"Content"}}"#],
        "{entries:#?}"
    );
}

/// A unit a plugin begins inside a command and lets go without committing, or
/// aborts, dooms the command's unit: nothing it wrote is kept, and the log
/// says which. One it commits is kept, shown as the label it was given.
#[tokio::test]
async fn a_dropped_unit_handle_aborts() {
    let fixture = opened(in_memory().await, "", false).await;
    for command in ["reference.unit.drop", "reference.unit.abort"] {
        let refused = fixture.as_ana(command, &photo("let go")).await;
        assert!(
            is(
                &refused,
                RefusalKind::Internal,
                ErrorCode::KernelUnitAborted
            ),
            "`{command}`: {refused:?}"
        );
        assert_eq!(
            held(&fixture.store, REFERENCE, "note/photo").await,
            None,
            "`{command}`: what the unit wrote was kept"
        );
    }
    assert_eq!(
        fixture.logged("kernel.unit_aborted", "cause"),
        ["dropped", "explicit"]
    );
    assert_eq!(fixture.kept().await, [] as [String; 0]);
    fixture
        .as_ana("reference.unit.commit", &photo("kept"))
        .await
        .expect("committed");
    assert_eq!(
        held(&fixture.store, REFERENCE, "note/photo").await,
        Some(kept_as("kept"))
    );
    assert_eq!(
        fixture.kept().await,
        ["Write a note"],
        "the step is shown as the label the plugin gave the unit it began"
    );
    assert_eq!(
        brackets(&fixture.entries().await),
        [
            Outcome::UnitBegun,
            Outcome::UnitAborted,
            Outcome::UnitBegun,
            Outcome::UnitAborted,
            Outcome::UnitBegun,
            Outcome::UnitCommitted
        ]
    );
}

/// Undo and redo nested in another command are refused, as a value the plugin
/// that asked reads, journaled with the kernel's own sentinel.
#[tokio::test]
async fn undo_and_redo_are_refused_from_inside_another_command() {
    let fixture = opened(in_memory().await, "", false).await;
    fixture
        .as_ana("reference.note.put", &photo("kept"))
        .await
        .expect("written");
    for history in ["kernel.undo", "kernel.redo"] {
        let relayed = Value::object([
            ("args", Value::text("{}")),
            ("name", Value::text(history)),
            ("note", Value::text("photo")),
        ])
        .to_canonical();
        let answered = fixture
            .as_ana("reference.relay.write", &relayed)
            .await
            .expect("the relay answers");
        let refused = field(&answered, "refused")
            .into_text()
            .expect("the relay was refused");
        assert!(
            refused.starts_with(&format!("denied: [{}] ", ErrorCode::KernelTopLevelOnly)),
            "`{history}`: {refused}"
        );
    }
    assert_eq!(
        held(&fixture.store, REFERENCE, "note/photo").await,
        Some(kept_as("kept")),
        "a nested undo undid something"
    );
    let entries = fixture.entries().await;
    assert_eq!(
        entries
            .iter()
            .filter(|entry| {
                entry.record.deciding_policy.as_deref() == Some("kernel.top_level_only")
            })
            .count(),
        2
    );
    assert_eq!(
        fixture.logged("kernel.dispatch_refused", "reason"),
        ["top_level_only", "top_level_only"]
    );
}

/// An undo takes the pin off the blob its step made and a redo puts it back,
/// and once the sweep has taken it, a redo is refused.
#[tokio::test]
async fn a_blob_undone_is_unpinned_pinned_again_by_redo_and_once_swept_refuses_it() {
    let fixture = opened(in_memory().await, "", false).await;
    fixture
        .as_ana("reference.blob.put", &blob_put("made", None))
        .await
        .expect("made");
    let made = Hash::of(b"made");
    let step = fixture.newest_step().await;
    assert!(fixture.pinned(&made).await);
    let undone = fixture.undo().await.expect("undone");
    assert_eq!(undone, reversed_as(step.unit, 0, 1, 0, 0));
    assert!(!fixture.pinned(&made).await, "the undone blob kept its pin");
    let redone = fixture.redo().await.expect("redone");
    assert_eq!(redone, reversed_as(step.unit, 0, 1, 0, 0));
    assert!(fixture.pinned(&made).await, "the redone blob is not pinned");
    fixture.undo().await.expect("undone again");
    fixture.clock.now.fetch_add(
        nc_catalog::KERNEL.blob_orphan_grace_ms + 1,
        Ordering::SeqCst,
    );
    let swept = fixture.kernel.sweep_blobs().await.expect("swept");
    assert_eq!(swept.forgotten, 1);
    let refused = fixture.redo().await;
    assert!(
        is(
            &refused,
            RefusalKind::NotFound,
            ErrorCode::KernelRedoBlobSwept
        ),
        "{refused:?}"
    );
    assert_eq!(
        fixture.state_of(step.unit).await,
        UnitState::Undone {
            by: fixture.history().await[0].unit
        }
    );
}

/// A blob another person's step still holds keeps its pin when the step that
/// first made it is undone, so the sweep leaves its bytes.
#[tokio::test]
async fn a_blob_another_step_still_holds_keeps_its_pin_when_the_first_is_undone() {
    let fixture = opened(in_memory().await, "", false).await;
    fixture
        .as_ana("reference.blob.put", &blob_put("shared", None))
        .await
        .expect("made");
    let shared = Hash::of(b"shared");
    // Bruno's step makes the same bytes again, noted as the kernel notes a
    // blob a committed unit already held.
    seeded_unit(&fixture.store, "bruno", |unit| {
        unit.blobs = vec![BlobCreated {
            hash: shared.to_string(),
            size: 6,
        }];
        unit.notes = vec![format!(
            r#"{{"hash":"{shared}","note":"blob_created","pinned_before":true}}"#
        )];
    })
    .await;
    let step = fixture.newest_step().await;
    let undone = fixture.undo().await.expect("undone");
    assert_eq!(undone, reversed_as(step.unit, 0, 0, 0, 0));
    assert!(fixture.pinned(&shared).await, "bruno's step still holds it");

    // **A pin the step did not put on is not its undo's to take off**, even
    // with no note left to say who put it on: here a step that kept no note.
    let fixture = opened(in_memory().await, "", false).await;
    let held = Hash::of(b"held");
    seeded_unit(&fixture.store, "bruno", |unit| {
        unit.blobs = vec![BlobCreated {
            hash: held.to_string(),
            size: 4,
        }];
    })
    .await;
    fixture
        .as_ana("reference.blob.put", &blob_put("held", None))
        .await
        .expect("made again");
    let undone = fixture.undo().await.expect("undone");
    assert_eq!(
        undone,
        reversed_as(fixture.newest_step().await.unit, 0, 0, 0, 0)
    );
    assert!(
        fixture.pinned(&held).await,
        "a pin put on before the step came off"
    );
}

/// A step redone after another person's undo holds its blob again, whoever
/// undid last, so the sweep leaves its bytes.
///
/// Ana's step made the bytes and carla's made them again, so carla's undo
/// left the pin to ana's step and ana's undo then took it off: carla's step
/// is the one that holds the blob once it is redone, though its own undo took
/// no pin off. And ana's redo, finding the blob pinned already, records no pin
/// of its own put back — which is what a reversal of it would take off.
///
/// Found by a review: a redo pinned again only what its own undo had taken
/// off, so carla's step was committed with its blob unpinned and, once the
/// grace had passed, the sweep deleted the bytes.
#[tokio::test]
async fn a_step_redone_after_another_persons_undo_holds_its_blob_again() {
    let fixture = opened(in_memory().await, "", false).await;
    fixture
        .as_ana("reference.blob.put", &blob_put("shared", None))
        .await
        .expect("ana made it");
    let ana = fixture.newest_step().await;
    fixture
        .as_(
            "carla",
            "reference.blob.put",
            &blob_put_on("recipe", "shared"),
        )
        .await
        .expect("carla made it again");
    let carla = fixture.newest_step_of("carla").await;
    let shared = Hash::of(b"shared");
    let undone = fixture
        .as_("carla", "kernel.undo", "{}")
        .await
        .expect("carla's step undone");
    assert_eq!(
        undone,
        reversed_as(carla.unit, 0, 0, 0, 0),
        "ana's step still held it"
    );
    let undone = fixture.undo().await.expect("ana's step undone");
    assert_eq!(undone, reversed_as(ana.unit, 0, 1, 0, 0));
    assert!(!fixture.pinned(&shared).await, "no committed step holds it");
    let redone = fixture
        .as_("carla", "kernel.redo", "{}")
        .await
        .expect("carla's step redone");
    assert_eq!(redone, reversed_as(carla.unit, 0, 1, 0, 0));
    assert_eq!(fixture.state_of(carla.unit).await, UnitState::Committed);
    assert!(
        fixture.pinned(&shared).await,
        "carla's redone step is committed and its blob is unpinned"
    );
    let redone = fixture.redo().await.expect("ana's step redone");
    assert_eq!(
        redone,
        reversed_as(ana.unit, 0, 0, 0, 0),
        "a blob pinned already is not one the redo pinned again"
    );
    fixture.clock.now.fetch_add(
        nc_catalog::KERNEL.blob_orphan_grace_ms + 1,
        Ordering::SeqCst,
    );
    let swept = fixture.kernel.sweep_blobs().await.expect("swept");
    assert_eq!(swept.forgotten, 0);
    assert_eq!(fixture.blobs.size(&shared).await, Ok(Some(6)));
}

/// A blob two people's steps made loses its pin once both are undone, so the
/// sweep takes its bytes.
///
/// Carla's step made the bytes ana's had made, and is noted as one a committed
/// unit held before it: ana's undo leaves the pin to carla's step, and carla's
/// undo — the last step holding it gone, and the row made by a step no longer
/// committed — takes it off.
///
/// Found by a review: an undo never took off the pin of a blob held before its
/// step, so a blob no committed unit held kept its pin for good.
#[tokio::test]
async fn a_blob_two_peoples_steps_made_loses_its_pin_once_both_are_undone() {
    let fixture = opened(in_memory().await, "", false).await;
    fixture
        .as_ana("reference.blob.put", &blob_put("shared", None))
        .await
        .expect("ana made it");
    let ana = fixture.newest_step().await;
    fixture
        .as_(
            "carla",
            "reference.blob.put",
            &blob_put_on("recipe", "shared"),
        )
        .await
        .expect("carla made it again");
    let carla = fixture.newest_step_of("carla").await;
    let shared = Hash::of(b"shared");
    let undone = fixture.undo().await.expect("ana's step undone");
    assert_eq!(
        undone,
        reversed_as(ana.unit, 0, 0, 0, 0),
        "carla's step still held it"
    );
    assert!(fixture.pinned(&shared).await, "carla's step holds it");
    let undone = fixture
        .as_("carla", "kernel.undo", "{}")
        .await
        .expect("carla's step undone");
    assert_eq!(undone, reversed_as(carla.unit, 0, 1, 0, 0));
    assert!(
        !fixture.pinned(&shared).await,
        "no committed unit holds the blob, and it kept its pin"
    );
    fixture.clock.now.fetch_add(
        nc_catalog::KERNEL.blob_orphan_grace_ms + 1,
        Ordering::SeqCst,
    );
    // **And its bytes unnamed as long**, on the blobs' own clock: bytes named
    // less than a grace ago may be another realm's unit's in flight.
    fixture.blobs.age(std::time::Duration::from_millis(
        nc_catalog::KERNEL.blob_orphan_grace_ms + 1,
    ));
    let swept = fixture.kernel.sweep_blobs().await.expect("swept");
    assert_eq!(swept.forgotten, 1);
    assert_eq!(fixture.blobs.size(&shared).await, Ok(None));
}

/// A blob an undo's compensation made keeps its pin when the step that first
/// made it is undone, and the sweep leaves its bytes.
///
/// The request's undo ran the compensation the request registered, which made
/// the bytes the step before it had made: that undo is never redone, so what
/// its compensation did stands, and it holds the blob as a step would.
///
/// Found by a review: only a step counted as holding a blob, so undoing the
/// first step took the pin off and the sweep took the bytes the undo kept.
#[tokio::test]
async fn a_blob_an_undos_compensation_made_keeps_its_pin_when_its_first_step_is_undone() {
    let fixture = opened(in_memory().await, "", false).await;
    fixture
        .as_ana("reference.blob.put", &blob_put("shared", None))
        .await
        .expect("made");
    let first = fixture.newest_step().await;
    let shared = Hash::of(b"shared");
    let posted = fixture
        .as_ana(
            "reference.compensated.post",
            &compensated_post(Some(("reference.blob.put", &blob_put("shared", None)))),
        )
        .await
        .expect("sent");
    assert_eq!(field(&posted, "compensation"), Value::text("registered"));
    let request = fixture.newest_step().await;
    fixture
        .undo()
        .await
        .expect("the request undone, its compensation run");
    let undo = fixture.history().await.remove(0);
    assert_eq!(
        undo.meta.kind,
        UnitKind::Undo {
            target: request.unit
        }
    );
    let undone = fixture.undo().await.expect("the first step undone");
    assert_eq!(
        undone,
        reversed_as(first.unit, 0, 0, 0, 0),
        "the first step's undo took off the pin of a blob the request's undo holds"
    );
    assert_eq!(fixture.state_of(undo.unit).await, UnitState::Committed);
    assert!(fixture.pinned(&shared).await, "the blob lost its pin");
    fixture.clock.now.fetch_add(
        nc_catalog::KERNEL.blob_orphan_grace_ms + 1,
        Ordering::SeqCst,
    );
    let swept = fixture.kernel.sweep_blobs().await.expect("swept");
    assert_eq!(swept.forgotten, 0);
    assert_eq!(fixture.blobs.size(&shared).await, Ok(Some(6)));
}

/// A redo is refused when the blob its step made has lost its row or its
/// bytes since the undo, whichever went: what it would pin again is not on
/// this machine.
#[tokio::test]
async fn a_redo_whose_blob_lost_its_row_or_its_bytes_is_refused() {
    for lost in ["bytes", "row"] {
        let fixture = opened(in_memory().await, "", false).await;
        fixture
            .as_ana("reference.blob.put", &blob_put("made", None))
            .await
            .expect("made");
        let made = Hash::of(b"made");
        fixture.undo().await.expect("undone");
        if lost == "bytes" {
            assert_eq!(fixture.blobs.remove(&made).await, Ok(true));
        } else {
            assert_eq!(fixture.store.forget_blob(made.as_str()).await, Ok(true));
        }
        let refused = fixture.redo().await;
        assert!(
            is(
                &refused,
                RefusalKind::NotFound,
                ErrorCode::KernelRedoBlobSwept
            ),
            "its {lost} gone: {refused:?}"
        );
    }
}

/// The compensations a step registered run the last first: the one registered
/// first is the last to write, and what it wrote is what is kept.
#[tokio::test]
async fn the_compensation_registered_last_runs_first() {
    let fixture = opened(in_memory().await, "", false).await;
    let args = Value::object([
        ("compensate", Value::text("reference.note.put")),
        ("note", Value::text("photo")),
        ("then", Value::text(photo("registered second"))),
        ("url", Value::text("https://example.com/")),
        ("with", Value::text(photo("registered first"))),
    ])
    .to_canonical();
    let posted = fixture
        .as_ana("reference.compensated.post", &args)
        .await
        .expect("sent");
    assert_eq!(field(&posted, "compensation"), Value::text("registered"));
    let undone = fixture.undo().await.expect("undone");
    assert_eq!(field(&undone, "beyond_reach"), Value::Integer(1));
    assert_eq!(
        held(&fixture.store, REFERENCE, "note/photo").await,
        Some(kept_as("registered first"))
    );
}

/// The arguments that write the note `photo`'s first `parts` numbered parts
/// over with one short text.
fn in_parts(parts: i64) -> String {
    Value::object([
        ("note", Value::text("photo")),
        ("parts", Value::Integer(parts)),
        ("text", Value::text("short")),
    ])
    .to_canonical()
}

/// A step is held, as it writes, to what its undo puts back: one that writes
/// over more than one unit could stage back is refused before it is kept, and
/// one within the bound is undone whole.
///
/// A verifier found a step that deleted twenty values of about a megabyte
/// kept — what it wrote alone counted against the unit's bytes — and its undo
/// refused with the bound's code at every attempt, so undo never reached past
/// it: the step a large accidental change is.
#[tokio::test]
async fn a_step_its_undo_could_not_stage_is_refused_and_one_it_can_is_undone_whole() {
    let fixture = opened(in_memory().await, "", false).await;
    // **A million bytes each**, within a store value's bound: sixteen of them
    // with their keys fit in one unit's bytes, and seventeen do not.
    let large = kept_as(&"v".repeat(999_989));
    assert_eq!(large.len(), 1_000_000);
    let part = |n: i64| format!("note/photo/{n}");
    for n in 0..17 {
        seeded_by(&fixture.store, "bruno", REFERENCE, &part(n), &large).await;
    }
    let refused = fixture
        .as_ana("reference.note.put-many", &in_parts(17))
        .await;
    assert!(
        is(
            &refused,
            RefusalKind::Unavailable,
            ErrorCode::PluginCommandRefused
        ),
        "{refused:?}"
    );
    let said = refused.as_ref().err().map_or("", Refusal::sentence);
    assert!(
        said.contains(&ErrorCode::PluginBoundReached.to_string()),
        "the plugin was refused for something else: {said}"
    );
    for n in 0..17 {
        assert_eq!(
            held(&fixture.store, REFERENCE, &part(n)).await.as_deref(),
            Some(large.as_str()),
            "a step refused kept `{}`",
            part(n)
        );
    }
    assert!(
        fixture.kept().await.is_empty(),
        "{:?}",
        fixture.kept().await
    );

    seeded_unit(&fixture.store, "bruno", |unit| {
        unit.writes = vec![StagedWrite {
            plugin: REFERENCE.to_owned(),
            key: part(16),
            expect: Expect::Anything,
            after: None,
        }];
    })
    .await;
    fixture
        .as_ana("reference.note.put-many", &in_parts(16))
        .await
        .expect("a step its undo can stage is kept");
    for n in 0..16 {
        assert_eq!(
            held(&fixture.store, REFERENCE, &part(n)).await,
            Some(kept_as("short"))
        );
    }
    fixture.undo().await.expect("the step is undone");
    for n in 0..16 {
        assert_eq!(
            held(&fixture.store, REFERENCE, &part(n)).await.as_deref(),
            Some(large.as_str()),
            "the undo did not put `{}` back",
            part(n)
        );
    }
}

/// A step the store keeps and the journal does not record as kept — one
/// nobody dispatched — is not undone.
#[tokio::test]
async fn a_step_the_journal_does_not_record_is_not_undone() {
    let fixture = opened(in_memory().await, "", false).await;
    seeded(&fixture.store, REFERENCE, "note/photo", &kept_as("seeded")).await;
    let refused = fixture.undo().await;
    assert!(
        is(
            &refused,
            RefusalKind::Internal,
            ErrorCode::KernelStepUnrecorded
        ),
        "{refused:?}"
    );
    assert_eq!(
        held(&fixture.store, REFERENCE, "note/photo").await,
        Some(kept_as("seeded"))
    );
}

/// Undo and redo reach the file: closed and opened again, it holds the note
/// as the last of them left it, and every unit's kind and state.
#[tokio::test]
async fn undo_and_redo_reach_the_file_and_survive_opening_it_again() {
    let scratch = Scratch::new("history");
    let store = LocalStore::open(&scratch.profile())
        .await
        .expect("the local database opens");
    let fixture = opened(store, "", false).await;
    for text in ["one", "two"] {
        fixture
            .as_ana("reference.note.put", &photo(text))
            .await
            .expect("written");
    }
    fixture.undo().await.expect("undone");
    fixture.redo().await.expect("redone");
    fixture.undo().await.expect("undone again");
    let Fixture {
        kernel,
        store,
        world,
        ..
    } = fixture;
    drop(kernel);
    store.close().await;
    let reopened = LocalStore::open(&scratch.profile())
        .await
        .expect("the local database opens again");
    assert_eq!(
        held(&reopened, REFERENCE, "note/photo").await,
        Some(kept_as("one"))
    );
    let realm = world.realm_id("home").expect("home is a realm");
    let history = reopened
        .units(&realm, &id_of(&world, "ana"), 10)
        .await
        .expect("the store answers");
    let (undo_again, redo, undo, two, one) = (
        &history[0],
        &history[1],
        &history[2],
        &history[3],
        &history[4],
    );
    assert_eq!(
        history
            .iter()
            .map(|row| row.meta.label.as_str())
            .collect::<Vec<_>>(),
        [
            "kernel.undo",
            "kernel.redo",
            "kernel.undo",
            "reference.note.put",
            "reference.note.put"
        ]
    );
    assert_eq!(undo_again.meta.kind, UnitKind::Undo { target: two.unit });
    assert_eq!(redo.meta.kind, UnitKind::Redo { target: undo.unit });
    assert_eq!(
        [
            undo_again.state,
            redo.state,
            undo.state,
            two.state,
            one.state
        ],
        [
            UnitState::Committed,
            UnitState::Committed,
            UnitState::Undone { by: redo.unit },
            UnitState::Undone {
                by: undo_again.unit
            },
            UnitState::Committed
        ]
    );
    reopened.close().await;
}

/// Fills the units table to one past its bound once `later` more units are
/// committed, with units whose identifiers follow `after` — newer than it, and
/// older than anything minted a millisecond later — so the pruner's oldest
/// batch holds `after` and none of those.
async fn filled_past_the_bound_after(store: &LocalStore, after: Snowflake, later: u64) {
    let limit = nc_catalog::TABLES
        .iter()
        .find(|table| table.name == "store_units")
        .expect("the catalog bounds the units");
    let most = limit.max_rows.expect("a bound");
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM main.store_units")
        .fetch_one(store.pool())
        .await
        .expect("the store counts");
    let seeds = most + 1 - u64::try_from(before).expect("a count") - later;
    let seeding = "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < ?1) \
                   INSERT INTO main.store_units (unit, realm, principal, kind, target, label, \
                   state, undone_by, committed_at) SELECT ?2 + i, 'r', 'p', 'step', NULL, 's', \
                   'committed', NULL, 0 FROM n";
    sqlx::query(seeding)
        .bind(i64::try_from(seeds).expect("fits"))
        .bind(i64::try_from(after.value()).expect("fits"))
        .execute(store.pool())
        .await
        .expect("the seed inserts");
}

/// A blob a step made keeps its pin once the pruner has taken that step —
/// beyond every undo, so what it made stays — when a later step that made the
/// same bytes again is undone.
///
/// **A reviewer found a real bug with it**: the undo of the later step asked
/// which units held the blob of the notes the store keeps, the pruner had
/// taken the first step's with it, and so the pin came off and the sweep
/// deleted the bytes a step no undo reaches had made.
#[tokio::test]
async fn a_blob_a_step_the_pruner_took_made_keeps_its_pin_when_a_later_step_is_undone() {
    let fixture = opened(in_memory().await, "", false).await;
    fixture
        .as_ana("reference.blob.put", &blob_put("the same bytes", None))
        .await
        .expect("made");
    let made = Hash::of(b"the same bytes");
    let first = fixture.newest_step().await;
    filled_past_the_bound_after(&fixture.store, first.unit, 1).await;
    fixture.clock.now.fetch_add(1_000, Ordering::SeqCst);
    fixture
        .as_ana("reference.blob.put", &blob_put("the same bytes", None))
        .await
        .expect("made again");
    let second = fixture.newest_step().await;
    assert!(second.unit > first.unit, "minted later");
    let pumped = fixture.kernel.pump().await;
    assert_eq!(pumped.prune_refused, None, "{pumped:?}");
    assert!(pumped.pruned > 0, "{pumped:?}");
    let held = |unit| {
        let store = fixture.store.clone();
        async move { store.unit(unit).await.expect("the store answers").is_some() }
    };
    assert!(!held(first.unit).await, "the pruner took the first step");
    assert!(held(second.unit).await, "and left the second");
    assert!(fixture.pinned(&made).await, "pinned before the undo");
    fixture.undo().await.expect("the second step is undone");
    assert!(
        fixture.pinned(&made).await,
        "the blob the first step made, beyond every undo since the pruner took it, lost its \
         pin when the second step was undone"
    );
    fixture.clock.now.fetch_add(
        nc_catalog::KERNEL.blob_orphan_grace_ms + 1,
        Ordering::SeqCst,
    );
    fixture.kernel.sweep_blobs().await.expect("swept");
    assert!(
        fixture
            .blobs
            .size(&made)
            .await
            .expect("the blobs answer")
            .is_some(),
        "the sweep deleted the bytes"
    );
}

/// A redo whose step the pruner took is nothing to redo, as the migration
/// reads a target that is gone.
///
/// **A reviewer found a real bug with it**: it was answered as a store holding
/// a row it did not write, `internal` with `[NC-DB-0007]`, every time it was
/// asked until new work came.
#[tokio::test]
async fn a_redo_whose_step_the_pruner_took_is_nothing_to_redo() {
    let fixture = opened(in_memory().await, "", false).await;
    fixture
        .as_ana("reference.note.put", &photo("kept"))
        .await
        .expect("written");
    let step = fixture.newest_step().await;
    filled_past_the_bound_after(&fixture.store, step.unit, 1).await;
    fixture.clock.now.fetch_add(1_000, Ordering::SeqCst);
    fixture.undo().await.expect("undone");
    let undo = fixture.history().await[0].unit;
    let pumped = fixture.kernel.pump().await;
    assert_eq!(pumped.prune_refused, None, "{pumped:?}");
    let held = |unit| {
        let store = fixture.store.clone();
        async move { store.unit(unit).await.expect("the store answers").is_some() }
    };
    assert!(!held(step.unit).await, "the pruner took the step");
    assert!(held(undo).await, "and left its undo");
    let redone = fixture.redo().await;
    assert!(
        is(
            &redone,
            RefusalKind::NotFound,
            ErrorCode::KernelNothingToRedo
        ),
        "{redone:?}"
    );
}

/// A step of `ana`'s the journal recorded as kept, committed to `store` with
/// what `shape` gives it: a journal over `history` opens and closes its unit
/// first, on a flake above the kernel's own, so it is ana's newest step.
async fn a_kept_step(
    store: &LocalStore,
    history: &mut nc_journal::InMemory,
    shape: impl FnOnce(&mut UnitRecord),
) -> Snowflake {
    let world = MemoryDirectory::from_world(WORLD, &family()).expect("the world is valid");
    let realm = world.realm_id("home").expect("home is a realm");
    let keeper = the_keeper();
    let mut journal = nc_journal::Journal::new(
        history.clone(),
        realm.clone(),
        keeper,
        Minter::new(MachineFlake::from_value(9).expect("a flake"), Still::new()),
    );
    let unit = journal
        .open_unit("reference.note.put")
        .await
        .expect("the journal opens it");
    journal
        .close_unit(unit, nc_journal::Closing::Committed)
        .await
        .expect("the journal keeps it");
    *history = journal.store().clone();
    let mut record = UnitRecord {
        unit,
        meta: UnitMeta {
            realm,
            principal: id_of(&world, "ana"),
            kind: UnitKind::Step,
            label: "a kept step".to_owned(),
            committed_at_ms: NOW_MS,
        },
        writes: Vec::new(),
        blobs: Vec::new(),
        notes: Vec::new(),
        unpinned: Vec::new(),
        marks: Vec::new(),
    };
    shape(&mut record);
    let quotas = Quotas {
        entries_per_plugin: 100,
        lowered: std::collections::BTreeMap::new(),
        blobs: 10,
        writes_per_unit: 10,
        notes_per_unit: 10_000,
    };
    store
        .commit(&record, &quotas)
        .await
        .expect("the step commits");
    unit
}

/// A step holding as many compensations as a unit may register is undone:
/// each is a dispatch of the undo's own tree, and the undo's is the first.
///
/// **A reviewer found a real bug with it**: a unit registered as many as a
/// tree may dispatch, and every undo of such a step was refused at the last
/// one, so undo never reached past it.
#[tokio::test]
async fn a_step_with_as_many_compensations_as_a_unit_may_register_is_undone() {
    let store = in_memory().await;
    let mut history = nc_journal::InMemory::default();
    let compensations = usize::try_from(nc_catalog::KERNEL.dispatches_per_tree - 1).expect("fits");
    let note = Value::object([
        (
            "args",
            Value::object([
                ("note", Value::text("photo")),
                ("text", Value::text("compensated")),
            ]),
        ),
        ("by", Value::text(REFERENCE)),
        ("command", Value::text("reference.note.put")),
        ("note", Value::text("compensation")),
        ("owner", Value::text(REFERENCE)),
    ])
    .to_canonical();
    let step = a_kept_step(&store, &mut history, |record| {
        record.notes = vec![note; compensations];
    })
    .await;
    let fixture = opened_over(store, history, "", false, GRANTED).await;
    assert_eq!(fixture.newest_step().await.unit, step, "ana's newest step");
    let undone = fixture.undo().await;
    assert!(undone.is_ok(), "{undone:?}");
    assert_eq!(
        field(&undone.expect("undone"), "reverted"),
        Value::object([
            ("blobs", Value::Integer(0)),
            (
                "compensations",
                Value::Integer(i64::try_from(compensations).expect("fits"))
            ),
            ("writes", Value::Integer(0)),
        ])
    );
}

/// An undo whose compensation makes the blob its step made is kept, and holds
/// that blob as a step does.
///
/// **A reviewer found a real bug with it**: the undo took the blob's pin off
/// before its compensation made the blob again, so its unit both pinned and
/// unpinned it, and the store refused it as a unit no unit can be — every
/// time it was asked, and undo reaches only the newest step.
#[tokio::test]
async fn an_undo_whose_compensation_makes_the_blob_its_step_made_is_kept() {
    const BYTES: &str = "compensated bytes";
    let store = in_memory().await;
    let mut history = nc_journal::InMemory::default();
    let made = Hash::of(BYTES.as_bytes());
    let step = a_kept_step(&store, &mut history, |record| {
        record.blobs = vec![BlobCreated {
            hash: made.to_string(),
            size: u64::try_from(BYTES.len()).expect("fits"),
        }];
        record.notes = vec![
            format!(r#"{{"hash":"{made}","note":"blob_created","pinned_before":false}}"#),
            Value::object([
                (
                    "args",
                    Value::object([("note", Value::text("photo")), ("text", Value::text(BYTES))]),
                ),
                ("by", Value::text(REFERENCE)),
                ("command", Value::text("reference.blob.put")),
                ("note", Value::text("compensation")),
                ("owner", Value::text(REFERENCE)),
            ])
            .to_canonical(),
        ];
    })
    .await;
    let fixture = opened_over(store.clone(), history, "", false, GRANTED).await;
    assert_eq!(fixture.newest_step().await.unit, step, "ana's newest step");
    let undone = fixture.undo().await;
    assert!(undone.is_ok(), "{undone:?}");
    assert!(
        fixture.pinned(&made).await,
        "the undo's compensation made the blob, and the undo holds it"
    );
    // **And says so, in its answer, its journal entry and its notes.** A
    // reviewer found it answering and journaling `reverted.blobs: 1` and
    // noting a `blob_unpinned` for the pin it kept on.
    let reverted = |answer: &str| {
        field(answer, "reverted")
            .into_object()
            .and_then(|mut fields| fields.remove("blobs"))
    };
    let undone = undone.expect("undone");
    assert_eq!(reverted(&undone), Some(Value::Integer(0)), "{undone}");
    let entries = fixture.entries().await;
    let entry = entries
        .iter()
        .find(|entry| command_of(entry).as_deref() == Some("kernel.undo"))
        .expect("the undo is journaled");
    assert_eq!(
        reverted(&entry.record.resource.to_canonical()),
        Some(Value::Integer(0)),
        "{}",
        entry.record.resource.to_canonical()
    );
    let Some(UnitState::Undone { by }) = state_of(&store, step).await else {
        panic!("the step is undone");
    };
    let notes = store
        .unit(by)
        .await
        .expect("the store answers")
        .expect("the undo is held")
        .notes;
    assert!(
        !notes.iter().any(|note| note.contains("blob_unpinned")),
        "{notes:#?}"
    );
}

/// A kernel of the realm `office` over `store`, its journal's store `history`
/// and `blobs`, opened as a daemon opens one over what a process before it
/// left, with no plugin loaded — or why it did not open: the realm a kernel of
/// `home` shares the file with.
async fn office_opening(
    store: LocalStore,
    history: nc_journal::InMemory,
    blobs: Arc<MemoryBlobs>,
    log: Arc<Captured>,
) -> Result<Kernel<Env>, nc_kernel::OpenError> {
    a_kernel_of("office", store, history, blobs, log, 0).await
}

/// A kernel of `realm` over `store`, `history` and `blobs`, logging to `log`,
/// its clock `later` milliseconds past [`NOW_MS`].
async fn a_kernel_of(
    realm: &str,
    store: LocalStore,
    history: nc_journal::InMemory,
    blobs: Arc<MemoryBlobs>,
    log: Arc<Captured>,
    later: u64,
) -> Result<Kernel<Env>, nc_kernel::OpenError> {
    let pack = family();
    let world = MemoryDirectory::from_world(WORLD, &pack).expect("the world is valid");
    let clock = Still::new();
    clock.now.fetch_add(later, Ordering::SeqCst);
    Kernel::open(KernelConfig {
        store,
        blobs,
        pdp: Ok(family()),
        journal: nc_journal::Journal::new(
            history,
            world.realm_id(realm).expect("a realm of the world"),
            the_keeper(),
            Minter::new(MachineFlake::from_value(5).expect("a flake"), clock.clone()),
        ),
        directory: Shifting {
            written: directory_over(WORLD, &pack),
            shifted: directory_over(WORLD, &pack),
            shift: Arc::new(AtomicBool::new(false)),
        },
        clock,
        log,
        transport: nc_kernel::net::Scripted::default(),
    })
    .await
}

/// The journal of the realm `office`'s process, under ana's keeping, on a
/// clock of its own: what a test commits that process's units against.
fn the_office_journal(clock: Still) -> nc_journal::Journal<nc_journal::InMemory, Still> {
    let world = MemoryDirectory::from_world(WORLD, &family()).expect("the world is valid");
    nc_journal::Journal::new(
        nc_journal::InMemory::default(),
        world.realm_id("office").expect("office is a realm"),
        id_of(&world, "ana"),
        Minter::new(MachineFlake::from_value(9).expect("a flake"), clock),
    )
}

/// A unit another realm's stopped process left committed in the store and
/// open in its journal is reverted when that realm's kernel opens, though a
/// kernel of `home` — sharing the file — pruned it in between.
///
/// **A reviewer found a real bug with it**: the pruner took the oldest units
/// of every realm, holding back only the ones the pumping kernel itself left,
/// so `home`'s pump took the record `office`'s recovery reverts from; the
/// recovery then closed the unit as aborted and its write stayed. The unit is
/// held from the pruner in the store now, whichever kernel prunes.
#[tokio::test]
async fn a_unit_another_realm_left_open_is_reverted_though_a_realm_pruned_meanwhile() {
    let store = in_memory().await;
    let world = MemoryDirectory::from_world(WORLD, &family()).expect("the world is valid");
    let ana = id_of(&world, "ana");
    let office = world.realm_id("office").expect("office is a realm");
    let mut stopped = the_office_journal(Still::new());
    // `office`'s process stopped between the store's commit and the journal's
    // close of this unit.
    let left = stopped
        .open_unit("reference.note.put")
        .await
        .expect("opened");
    let mut step = a_step(left, &ana, "note/photo", "left behind");
    step.meta.realm = office.clone();
    committed_held(&store, &step).await;
    filled_past_the_bound_after(&store, left, 0).await;
    let at_home = opened(store.clone(), "", false).await;
    let pumped = at_home.kernel.pump().await;
    assert_eq!(pumped.prune_refused, None, "{pumped:?}");
    assert!(pumped.pruned > 0, "{pumped:?}");
    drop(at_home);
    assert!(
        state_of(&store, left).await.is_some(),
        "home's pump took the record office's recovery reverts from"
    );
    let log = Arc::new(Captured::default());
    let at_the_office = office_opening(
        store.clone(),
        stopped.store().clone(),
        Arc::new(MemoryBlobs::default()),
        Arc::clone(&log),
    )
    .await
    .expect("the office's kernel opens");
    let entries = at_the_office
        .journal_for_tests()
        .await
        .expect("the journal reads back");
    assert_eq!(
        brackets_of(&entries, left),
        [Outcome::UnitBegun, Outcome::UnitAborted]
    );
    assert_eq!(
        store
            .get(&office, REFERENCE, "note/photo")
            .await
            .expect("the store answers")
            .map(|entry| entry.value),
        None,
        "a unit the journal closed as aborted kept its write"
    );
    assert_eq!(
        store.holders().await.expect("the store answers"),
        [],
        "the recovery let go of what it reverted"
    );
}

/// A realm's kernel opens over the undo its stopped process left committed in
/// the store and open in the journal, though a kernel of `home` — sharing the
/// file — pruned the step that undo undid in between.
///
/// **A reviewer found a real bug with it**: `home`'s pump took the step, the
/// oldest unit in the file; the undo's reversal marks it committed again, the
/// store refuses a mark of a unit it does not hold, and `office`'s kernel
/// refused to open, every time. An undo holds the step whose state it moves
/// now, as it holds itself.
#[tokio::test]
async fn a_realm_opens_over_its_undo_left_open_though_a_realm_pruned_meanwhile() {
    let store = in_memory().await;
    let world = MemoryDirectory::from_world(WORLD, &family()).expect("the world is valid");
    let ana = id_of(&world, "ana");
    let office = world.realm_id("office").expect("office is a realm");
    let clock = Still::new();
    let mut stopped = the_office_journal(clock.clone());
    let undone = stopped
        .open_unit("reference.note.put")
        .await
        .expect("opened");
    let mut step = a_step(undone, &ana, "note/recipe", "first");
    step.meta.realm = office.clone();
    committed_held(&store, &step).await;
    stopped
        .close_unit(undone, nc_journal::Closing::Committed)
        .await
        .expect("closed");
    store.release(undone).await.expect("its close is heard");
    filled_past_the_bound_after(&store, undone, 1).await;
    clock.now.fetch_add(1_000, Ordering::SeqCst);
    // `office`'s process stopped between the store's commit and the
    // journal's close of the undo.
    let undo = stopped.open_unit("kernel.undo").await.expect("opened");
    let mut undoing = a_unit(
        undo,
        &ana,
        UnitKind::Undo { target: undone },
        "kernel.undo",
        taken_back("note/recipe", "first"),
        vec![(undone, UnitState::Undone { by: undo })],
    );
    undoing.meta.realm = office.clone();
    committed_held(&store, &undoing).await;
    let at_home = opened(store.clone(), "", false).await;
    let pumped = at_home.kernel.pump().await;
    assert_eq!(pumped.prune_refused, None, "{pumped:?}");
    assert!(pumped.pruned > 0, "{pumped:?}");
    drop(at_home);
    assert!(
        state_of(&store, undone).await.is_some() && state_of(&store, undo).await.is_some(),
        "home's pump took the undo, or the step it undid"
    );
    for attempt in 0..2 {
        let log = Arc::new(Captured::default());
        let opening = office_opening(
            store.clone(),
            stopped.store().clone(),
            Arc::new(MemoryBlobs::default()),
            Arc::clone(&log),
        )
        .await;
        assert!(
            opening.is_ok(),
            "attempt {attempt}: office's kernel does not open: {:?}",
            opening.err().map(|error| error.to_string()),
        );
    }
    assert_eq!(
        state_of(&store, undone).await,
        Some(UnitState::Committed),
        "the recovery commits the step again"
    );
    assert_eq!(store.holders().await.expect("the store answers"), []);
}

/// A blob an undo another realm's stopped process left open took the pin off
/// is on this machine once that realm's kernel reverts the undo, though a
/// kernel of `home` — sharing the file and the blobs — swept a day later.
///
/// **A reviewer found a real bug with it**: the sweep forgot a row unpinned
/// for its grace whoever unpinned it, and the recovery's reversal of the undo
/// then pinned the blob again — inserting a pinned row for bytes that were
/// gone — and committed the step again as holding it.
#[tokio::test]
async fn a_blob_an_undo_left_open_took_the_pin_off_survives_another_realms_sweep() {
    let store = in_memory().await;
    let blobs = Arc::new(MemoryBlobs::default());
    let world = MemoryDirectory::from_world(WORLD, &family()).expect("the world is valid");
    let ana = id_of(&world, "ana");
    let office = world.realm_id("office").expect("office is a realm");
    let mut stopped = the_office_journal(Still::new());
    let bytes = b"office bytes";
    let writer = blobs.create().await.expect("a writer");
    let (writer, written) = blobs.write(writer, bytes.to_vec()).await;
    written.expect("written");
    let made = blobs.finish(writer).await.expect("finished").hash;
    let size = u64::try_from(bytes.len()).expect("fits");
    let undone = stopped
        .open_unit("reference.blob.put")
        .await
        .expect("opened");
    let mut step = a_unit(
        undone,
        &ana,
        UnitKind::Step,
        "reference.blob.put",
        Vec::new(),
        Vec::new(),
    );
    step.meta.realm = office.clone();
    step.blobs = vec![BlobCreated {
        hash: made.to_string(),
        size,
    }];
    step.notes = vec![format!(
        r#"{{"hash":"{made}","note":"blob_created","pinned_before":false}}"#
    )];
    committed_held(&store, &step).await;
    stopped
        .close_unit(undone, nc_journal::Closing::Committed)
        .await
        .expect("closed");
    store.release(undone).await.expect("its close is heard");
    let undo = stopped.open_unit("kernel.undo").await.expect("opened");
    let mut undoing = a_unit(
        undo,
        &ana,
        UnitKind::Undo { target: undone },
        "kernel.undo",
        Vec::new(),
        vec![(undone, UnitState::Undone { by: undo })],
    );
    undoing.meta.realm = office.clone();
    undoing.unpinned = vec![made.to_string()];
    undoing.notes = vec![format!(
        r#"{{"hash":"{made}","note":"blob_unpinned","size":{size}}}"#
    )];
    committed_held(&store, &undoing).await;
    let at_home = a_kernel_of(
        "home",
        store.clone(),
        nc_journal::InMemory::default(),
        Arc::clone(&blobs),
        Arc::new(Captured::default()),
        nc_catalog::KERNEL.blob_orphan_grace_ms + 1,
    )
    .await
    .expect("home's kernel opens");
    let swept = at_home.sweep_blobs().await.expect("swept");
    drop(at_home);
    assert_eq!(swept.forgotten, 0, "{swept:?}");
    let _at_the_office = office_opening(
        store.clone(),
        stopped.store().clone(),
        Arc::clone(&blobs),
        Arc::new(Captured::default()),
    )
    .await
    .expect("the office's kernel opens");
    assert_eq!(
        state_of(&store, undone).await,
        Some(UnitState::Committed),
        "the recovery commits the step again"
    );
    let row = store.blob(made.as_str()).await.expect("the store answers");
    assert!(row.as_ref().is_some_and(|row| row.pinned), "{row:?}");
    assert!(
        blobs.size(&made).await.expect("the blobs answer").is_some(),
        "the step committed again holds a blob whose bytes the sweep deleted"
    );
}

/// A unit the kernel kept holds nothing from the pruner once its close is
/// heard, and nor does an undo of it: the units table is the pruner's again.
#[tokio::test]
async fn a_unit_kept_holds_nothing_once_its_close_is_heard() {
    let fixture = opened(in_memory().await, "", false).await;
    fixture
        .as_ana("reference.note.put", &photo("kept"))
        .await
        .expect("written");
    assert_eq!(
        fixture.store.holders().await.expect("the store answers"),
        []
    );
    fixture.undo().await.expect("undone");
    assert_eq!(
        fixture.store.holders().await.expect("the store answers"),
        []
    );
}

/// A kernel opening lets go of the holds of every unit of its realm the
/// journal records as closed — what a process stopped between hearing a close
/// and letting go of its unit leaves — and keeps those of another realm.
#[tokio::test]
async fn a_kernel_opening_lets_go_of_what_its_realm_closed_and_nothing_else() {
    let store = in_memory().await;
    let world = MemoryDirectory::from_world(WORLD, &family()).expect("the world is valid");
    let ana = id_of(&world, "ana");
    let office = world.realm_id("office").expect("office is a realm");
    let mut journal = the_office_journal(Still::new());
    let closed = journal
        .open_unit("reference.note.put")
        .await
        .expect("opened");
    let mut step = a_step(closed, &ana, "note/photo", "kept");
    step.meta.realm = office.clone();
    committed_held(&store, &step).await;
    journal
        .close_unit(closed, nc_journal::Closing::Committed)
        .await
        .expect("closed");
    let mut elsewhere = a_step(
        journal.mint_unit().expect("a unit"),
        &ana,
        "note/recipe",
        "home's",
    );
    elsewhere.meta.realm = home();
    committed_held(&store, &elsewhere).await;
    let _at_the_office = office_opening(
        store.clone(),
        journal.store().clone(),
        Arc::new(MemoryBlobs::default()),
        Arc::new(Captured::default()),
    )
    .await
    .expect("the office's kernel opens");
    assert_eq!(
        store.holders().await.expect("the store answers"),
        [elsewhere.unit],
        "the office let go of its own closed unit, and only of it"
    );
    assert_eq!(state_of(&store, closed).await, Some(UnitState::Committed));
}

/// The journal in memory, which holds back the dispatch entry of
/// `reference.blob.put` while `holds` is set, saying when it has one: the
/// moment a command has made its blob and its unit is not yet committed.
#[derive(Debug, Clone)]
struct Holding {
    inner: nc_journal::InMemory,
    holds: Arc<AtomicBool>,
    reached: Arc<tokio::sync::Notify>,
    released: Arc<tokio::sync::Notify>,
}

impl nc_journal::Store for Holding {
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
        let held = self.holds.load(Ordering::SeqCst)
            && command_of(&entry).as_deref() == Some("reference.blob.put");
        let (reached, released) = (Arc::clone(&self.reached), Arc::clone(&self.released));
        async move {
            if held {
                reached.notify_one();
                released.notified().await;
            }
            self.inner.append(entry).await
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
        unit: Snowflake,
    ) -> impl Future<Output = Result<Option<Outcome>, nc_journal::Error>> + Send {
        self.inner.unit_closing(realm, unit)
    }

    fn unit_opened(
        &self,
        realm: &OpaqueId,
        unit: Snowflake,
    ) -> impl Future<Output = Result<bool, nc_journal::Error>> + Send {
        self.inner.unit_opened(realm, unit)
    }
}

/// The suite's kernel, journaling through [`Holding`].
#[derive(Debug)]
struct HeldEnv;

impl Environment for HeldEnv {
    type Store = LocalStore;
    type Blobs = Arc<MemoryBlobs>;
    type Journal = Holding;
    type Clock = Still;
    type Directory = Shifting;
    type Log = Arc<Captured>;
    type Transport = nc_kernel::net::Scripted;
}

/// Another realm's sweep, over the file and the bytes two realms share, leaves
/// the bytes a unit of the first has made and not yet committed, whose step is
/// then kept with its bytes.
///
/// **Written by a reviewer, who found a real bug with it**: each kernel's
/// sweep waits only for its own call trees, so `office`'s took the bytes of
/// `home`'s blob, in flight, for bytes no row names — and `home`'s step was
/// kept pinning a blob that was gone.
#[tokio::test]
async fn another_realms_sweep_leaves_the_bytes_a_unit_in_flight_made() {
    let store = in_memory().await;
    let blobs = Arc::new(MemoryBlobs::default());
    let pack = family();
    let world = MemoryDirectory::from_world(WORLD, &pack).expect("the world is valid");
    let holding = Holding {
        inner: nc_journal::InMemory::default(),
        holds: Arc::new(AtomicBool::new(true)),
        reached: Arc::new(tokio::sync::Notify::new()),
        released: Arc::new(tokio::sync::Notify::new()),
    };
    let clock = Still::new();
    let home = Kernel::<HeldEnv>::open(KernelConfig {
        store: store.clone(),
        blobs: Arc::clone(&blobs),
        pdp: Ok(family()),
        journal: nc_journal::Journal::new(
            holding.clone(),
            world.realm_id("home").expect("home is a realm"),
            the_keeper(),
            Minter::new(MachineFlake::from_value(3).expect("a flake"), clock.clone()),
        ),
        directory: Shifting {
            written: directory_over(WORLD, &pack),
            shifted: directory_over(WORLD, &pack),
            shift: Arc::new(AtomicBool::new(false)),
        },
        clock,
        log: Arc::new(Captured::default()),
        transport: nc_kernel::net::Scripted::default(),
    })
    .await
    .expect("home's kernel opens");
    home.load(
        manifest(REFERENCE, "reference", ""),
        &nc_kernel::component_for_tests("reference"),
    )
    .await
    .unwrap_or_else(|refused| panic!("the reference plugin loads: {refused}"));
    let office = a_kernel_of(
        "office",
        store.clone(),
        nc_journal::InMemory::default(),
        Arc::clone(&blobs),
        Arc::new(Captured::default()),
        0,
    )
    .await
    .expect("office's kernel opens");
    let ana = world
        .principal(PrincipalKind::User, "ana")
        .expect("ana is a user");
    let (caller, args) = (Caller::new(ana), blob_put("home's bytes", None));
    let (put, swept) = tokio::join!(home.dispatch(&caller, "reference.blob.put", &args), async {
        holding.reached.notified().await;
        let swept = office.sweep_blobs().await;
        holding.holds.store(false, Ordering::SeqCst);
        holding.released.notify_one();
        swept
    });
    assert_eq!(
        swept.expect("office sweeps"),
        nc_kernel::blobs::Swept::default(),
        "office's sweep took bytes a unit of home's had in flight"
    );
    let put = put.expect("home's step is kept");
    let hash = field(&put, "hash").into_text().expect("a hash");
    assert!(
        store
            .blob(&hash)
            .await
            .expect("the store answers")
            .is_some_and(|row| row.pinned),
        "home's step pins its blob"
    );
    assert_eq!(
        blobs
            .size(&Hash::parse(&hash).expect("a hash"))
            .await
            .expect("the blobs answer"),
        Some(12),
        "home's kept step pins a blob whose bytes office's sweep deleted"
    );
}

/// A realm's recovery reverts only its own units, never another realm's unit
/// of the same number in the file the two share: `office`, stopped with a unit
/// open that its own minter numbered as `home`'s kept one, opens again over
/// `home`'s note left as it was.
///
/// **Written by a reviewer, who found a real bug with it**: the recovery read
/// the unit back from the store by its number alone, and reverted `home`'s.
/// The journals of one machine share one minter (`Journal::sharing`), which
/// keeps their numbers apart; this is the kernel's own guard for two that do
/// not.
#[tokio::test]
async fn a_realms_recovery_leaves_another_realms_unit_of_the_same_number() {
    let store = in_memory().await;
    let at_home = opened(store.clone(), "", false).await;
    at_home
        .as_ana("reference.note.put", &photo("kept at home"))
        .await
        .expect("ana writes her note at home");
    let kept: Vec<Snowflake> = store
        .units(&home(), &id_of(&at_home.world, "ana"), 10)
        .await
        .expect("the store answers")
        .into_iter()
        .map(|row| row.unit)
        .collect();
    let world = MemoryDirectory::from_world(WORLD, &family()).expect("the world is valid");
    let mut stopped = nc_journal::Journal::new(
        nc_journal::InMemory::default(),
        world.realm_id("office").expect("office is a realm"),
        id_of(&world, "ana"),
        Minter::new(MachineFlake::from_value(3).expect("a flake"), Still::new()),
    );
    let left = stopped
        .open_unit("reference.note.put")
        .await
        .expect("opened");
    assert!(
        kept.contains(&left),
        "office's unit {left:?} is not numbered as home's {kept:?}, so this asks nothing"
    );
    let reopened = a_kernel_of(
        "office",
        store.clone(),
        stopped.store().clone(),
        Arc::new(MemoryBlobs::default()),
        Arc::new(Captured::default()),
        1,
    )
    .await;
    assert!(reopened.is_ok(), "office opens again");
    assert_eq!(
        held(&store, REFERENCE, "note/photo").await,
        Some(kept_as("kept at home")),
        "the recovery of `office` reverted the unit {left:?} of `home`"
    );
}

/// A confirmation and an approval issued for one plugin's command are
/// redeemed for no other plugin's command of the same name: one that took the
/// namespace the first left, after an unload and a load, is refused as never
/// confirmed and never approved — and what is issued for its own command once
/// it holds it is redeemed.
///
/// **Written by a reviewer, who found a real bug with it**: what the kernel
/// issued was bound to the command's name, and not to the plugin holding it,
/// so ana's confirmation of `noctua:reference`'s `reference.irreversible.do`
/// ran `noctua:impostor`'s — the bound a compensation is held to
/// (`a_compensation_never_reaches_a_plugin_that_took_its_command_since`).
#[tokio::test]
async fn a_confirmation_is_not_redeemed_for_a_plugin_that_took_its_command_since() {
    let fixture = opened(in_memory().await, "", false).await;
    let ana = fixture
        .world
        .principal(PrincipalKind::User, "ana")
        .expect("ana is a user of the world");
    let note = Value::object([("note", Value::text("photo"))]).to_canonical();
    let confirmation =
        fixture
            .kernel
            .confirmation_for_tests(&ana, "reference.irreversible.do", &note);
    let approval = fixture
        .kernel
        .approval_for_tests(&ana, "reference.note.delete", &note);
    assert!(fixture.kernel.unload(fixture.slots[0]).await, "unloaded");
    let component = nc_kernel::component_for_tests("reference");
    fixture
        .kernel
        .load(manifest("noctua:impostor", "reference", ""), &component)
        .await
        .expect("another plugin takes the namespace the first left");
    let confirmed = fixture
        .kernel
        .dispatch(
            &Caller::new(ana.clone()).confirmed(confirmation),
            "reference.irreversible.do",
            &note,
        )
        .await;
    let ran = held(&fixture.store, "noctua:impostor", "done/photo").await;
    assert!(
        is(
            &confirmed,
            RefusalKind::Denied,
            ErrorCode::KernelUnconfirmed
        ) && ran.is_none(),
        "the confirmation ana was issued for noctua:reference's command was redeemed for \
         noctua:impostor's: answered {confirmed:?}, the impostor's done/photo holds {ran:?}"
    );
    let approved = fixture
        .kernel
        .dispatch(
            &Caller::new(ana.clone()).approved(approval),
            "reference.note.delete",
            &note,
        )
        .await;
    assert!(
        approved
            .as_ref()
            .is_err_and(|refusal| refusal.kind() == RefusalKind::Denied),
        "the approval issued for noctua:reference's deletion approved noctua:impostor's: \
         {approved:?}"
    );
    let entries = fixture.entries().await;
    assert_eq!(
        entries_of(&entries, "reference.note.delete", Outcome::Denied)[0]
            .record
            .deciding_policy
            .as_deref(),
        Some("core.destructive_needs_quorum"),
        "the deletion was decided with no quorum met"
    );
    let confirmation =
        fixture
            .kernel
            .confirmation_for_tests(&ana, "reference.irreversible.do", &note);
    fixture
        .kernel
        .dispatch(
            &Caller::new(ana).confirmed(confirmation),
            "reference.irreversible.do",
            &note,
        )
        .await
        .expect("a confirmation issued for the command the impostor holds is redeemed for it");
    assert_eq!(
        held(&fixture.store, "noctua:impostor", "done/photo")
            .await
            .as_deref(),
        Some("true")
    );
}

/// A compensation a pack's rule holds to a quorum on some resources and not
/// on others is registered, and decided when the undo dispatches it: on the
/// private `diary` the family pack asks a quorum of every `Share`, so each
/// undo of its step is refused; on `photo`, which the realm reads, the same
/// command runs.
///
/// **What the registration refuses is what the core's rules make impossible
/// on every resource** — a command that cannot be undone, one that deletes.
/// A reviewer found the documents saying it refused every compensation that
/// needs a quorum's approval, and this one passing it: a rule that reads the
/// resource is decided by the undo, with what holds then, and a step whose
/// compensation it refuses is one no undo reaches past while it holds.
#[tokio::test]
async fn a_compensation_a_packs_rule_holds_to_a_quorum_is_registered_and_decided_at_the_undo() {
    let fixture = opened(in_memory().await, "", false).await;
    let posted = fixture
        .as_ana(
            "reference.compensated.post",
            &compensated_post(Some((
                "reference.note.share",
                &note_of("diary", "shared back"),
            ))),
        )
        .await
        .expect("sent");
    assert_eq!(field(&posted, "compensation"), Value::text("registered"));
    for attempt in 1..=2 {
        let refused = fixture.undo().await;
        assert!(
            refused
                .as_ref()
                .is_err_and(|refusal| refusal.code() == ErrorCode::KernelCompensationFailed),
            "undo {attempt}: {refused:?}"
        );
    }
    let entries = fixture.entries().await;
    let deciding: Vec<Option<&str>> = entries_of(&entries, "reference.note.share", Outcome::Denied)
        .iter()
        .map(|entry| entry.record.deciding_policy.as_deref())
        .collect();
    assert_eq!(
        deciding,
        [Some("family.giving_out_encrypted_content_needs_quorum"); 2],
        "each undo dispatched the share, and the pack's rule refused it"
    );
    let fixture = opened(in_memory().await, "", false).await;
    let posted = fixture
        .as_ana(
            "reference.compensated.post",
            &compensated_post(Some((
                "reference.note.share",
                &note_of("photo", "shared back"),
            ))),
        )
        .await
        .expect("sent");
    assert_eq!(field(&posted, "compensation"), Value::text("registered"));
    fixture
        .undo()
        .await
        .expect("a share of what the realm reads needs no quorum, so the undo runs it");
    assert_eq!(
        held(&fixture.store, REFERENCE, "note/photo").await,
        Some(kept_as("shared back"))
    );
}
