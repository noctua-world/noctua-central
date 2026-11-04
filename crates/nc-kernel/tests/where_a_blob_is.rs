//! Blobs: where a blob is is said honestly — on this machine, or not on it, and
//! never on another machine a kernel with no peer cannot see — a blob is read
//! back a bounded chunk at a time, and the blobs a unit made go with it when it
//! is let go (`NOCTUA.md §13.1`, `§13.4`, acceptance criterion 9, and the blob
//! half of criterion 3).
//!
//! **The real things.** The reference plugin's commands run through the
//! decision point into the real component, over the local database held in
//! memory and a blob directory on disk ([`DirectoryBlobs`]), so what a test
//! reads back is a row the database keeps and a file the directory holds. A
//! second store over the same directory plants what a stopped process would
//! have left — bytes no row names, a writing never finished — which the kernel
//! never wrote. The database's commits can be held at a gate, which is how a
//! sweep is shown to wait for the call tree that is still running.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use nc_authz::{EntityRef, MemoryDirectory, Pdp, PrincipalKind};
use nc_canon::Value;
use nc_catalog::{ErrorCode, KERNEL};
use nc_ids::{MachineFlake, Minter, OpaqueId};
use nc_journal::{Entry, Outcome};
use nc_kernel::backend::StoreBackend;
use nc_kernel::blobs::{BlobStore as _, DirectoryBlobs, Hash, Swept};
use nc_kernel::logging::Captured;
use nc_kernel::manifest::PluginManifest;
use nc_kernel::{Caller, Environment, Kernel, KernelConfig, Refusal, RefusalKind};
use nc_local::{LocalError, LocalStore, Quotas, UnitRecord};

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

/// The local database held in memory, whose commits a test can hold at a
/// gate until it lets them go — announcing that one is waiting.
#[derive(Debug, Clone)]
struct Gated {
    inner: LocalStore,
    /// Whether a commit is refused, as a database that cannot be written.
    refuses: Arc<AtomicBool>,
    /// Whether asking for a blob's row is refused, as a database that cannot
    /// be read.
    refuses_rows: Arc<AtomicBool>,
    holds: Arc<AtomicBool>,
    reached: Arc<tokio::sync::Notify>,
    released: Arc<tokio::sync::Notify>,
}

impl Gated {
    /// What its gate does now: whether a commit is held at it, whether one is
    /// refused, and how a test is told and lets it go.
    fn gate(
        &self,
    ) -> (
        bool,
        bool,
        Arc<tokio::sync::Notify>,
        Arc<tokio::sync::Notify>,
    ) {
        (
            self.holds.load(Ordering::SeqCst),
            self.refuses.load(Ordering::SeqCst),
            Arc::clone(&self.reached),
            Arc::clone(&self.released),
        )
    }
}

/// `commit`, refused or held at the gate as `gate` says.
async fn at_the_gate(
    (holds, refuses, reached, released): (
        bool,
        bool,
        Arc<tokio::sync::Notify>,
        Arc<tokio::sync::Notify>,
    ),
    commit: impl Future<Output = Result<nc_local::Committed, nc_local::CommitError>>,
) -> Result<nc_local::Committed, nc_local::CommitError> {
    if refuses {
        return Err(nc_local::CommitError::Store(LocalError::Unavailable(
            "the database refuses the commit, as the test made it".to_owned(),
        )));
    }
    if holds {
        reached.notify_one();
        released.notified().await;
    }
    commit.await
}

impl StoreBackend for Gated {
    fn get(
        &self,
        realm: &nc_ids::OpaqueId,
        plugin: &str,
        key: &str,
    ) -> impl Future<Output = Result<Option<nc_local::Entry>, LocalError>> + Send {
        self.inner.get(realm, plugin, key)
    }

    fn list(
        &self,
        realm: &nc_ids::OpaqueId,
        plugin: &str,
        prefix: &str,
        limit: u32,
        cursor: Option<&str>,
    ) -> impl Future<Output = Result<nc_local::Page, LocalError>> + Send {
        self.inner.list(realm, plugin, prefix, limit, cursor)
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
        at_the_gate(self.gate(), self.inner.commit(unit, quotas))
    }

    fn commit_held(
        &self,
        unit: &UnitRecord,
        quotas: &Quotas,
    ) -> impl Future<Output = Result<nc_local::Committed, nc_local::CommitError>> + Send {
        at_the_gate(self.gate(), self.inner.commit_held(unit, quotas))
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
        let refuses = self.refuses_rows.load(Ordering::SeqCst);
        let row = self.inner.blob(hash);
        async move {
            if refuses {
                return Err(LocalError::Unavailable(
                    "the database refuses the row, as the test made it".to_owned(),
                ));
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

/// A kernel over the [`Gated`] database, a blob directory on disk, the journal
/// in memory, the [`Manual`] clock and the world's directory.
#[derive(Debug)]
struct Env;

impl Environment for Env {
    type Store = Gated;
    type Blobs = DirectoryBlobs;
    type Journal = nc_journal::InMemory;
    type Clock = Manual;
    type Directory = MemoryDirectory;
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
            std::env::temp_dir().join(format!("nc-kernel-blobs-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir(&root).expect("a scratch directory");
        Self(root)
    }
}

/// A kernel with the reference plugin loaded, and everything a test reads
/// beside it — the scratch last, so it outlives the kernel that writes in it.
struct Fixture {
    kernel: Kernel<Env>,
    store: Gated,
    clock: Manual,
    world: MemoryDirectory,
    log: Arc<Captured>,
    blobs: PathBuf,
    _scratch: Scratch,
}

/// The identity the reference plugin is loaded under.
const REFERENCE: &str = "noctua:reference";

/// The family pack, as a kernel carries it.
fn family() -> Pdp {
    Pdp::embedded("family").expect("the family pack is carried")
}

/// A kernel whose blobs are kept in a scratch called `name`, with the
/// reference plugin loaded granted the store, both halves of the blobs and
/// the dispatch.
async fn opened(name: &str) -> Fixture {
    let scratch = Scratch::new(name);
    let blobs = scratch.0.join("blobs");
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
    let store = Gated {
        inner: LocalStore::in_memory_for_tests()
            .await
            .expect("the local database opens in memory"),
        refuses: Arc::new(AtomicBool::new(false)),
        refuses_rows: Arc::new(AtomicBool::new(false)),
        holds: Arc::new(AtomicBool::new(false)),
        reached: Arc::new(tokio::sync::Notify::new()),
        released: Arc::new(tokio::sync::Notify::new()),
    };
    let clock = Manual {
        now: Arc::new(AtomicU64::new(NOW_MS)),
    };
    let log = Arc::new(Captured::default());
    let keeper = OpaqueId::new("kernel").expect("a keeper"); // allow-logging: OpaqueId::new of a literal this suite writes
    let kernel = Kernel::open(KernelConfig {
        store: store.clone(),
        blobs: DirectoryBlobs::open(&blobs).expect("the blob directory opens"),
        pdp: Ok(pack),
        journal: nc_journal::Journal::new(
            nc_journal::InMemory::default(),
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
    let manifest = PluginManifest::parse(&format!(
        "id = \"{REFERENCE}\"\nnamespace = \"reference\"\ncapabilities = [\"store\", \
         \"blobs.read\", \"blobs.write\", \"commands.dispatch\"]\n"
    ))
    .expect("a manifest the kernel reads");
    kernel
        .load(manifest, &nc_kernel::component_for_tests("reference"))
        .await
        .unwrap_or_else(|refused| panic!("the reference plugin loads: {refused}"));
    Fixture {
        kernel,
        store,
        clock,
        world,
        log,
        blobs,
        _scratch: scratch,
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

    /// Makes a blob of `text` written `times` over, kept, and answers its hash.
    async fn put(&self, text: &str, times: u32) -> String {
        let answered = self
            .as_ana("reference.blob.put", &put_args(text, times, None, false))
            .await
            .unwrap_or_else(|refused| panic!("the blob is made: {refused}"));
        let hash = text_of(&answered, "hash");
        assert_eq!(
            hash,
            Hash::of(
                text.repeat(usize::try_from(times).expect("fits"))
                    .as_bytes()
            )
            .as_str(),
            "the hash the kernel answered is not the BLAKE3 of what was written"
        );
        hash
    }

    /// Where `hash` is, as the plugin is told.
    async fn located(&self, hash: &str) -> Result<String, Refusal> {
        self.as_ana("reference.blob.locate", &hash_args(hash, None))
            .await
            .map(|answered| text_of(&answered, "availability"))
    }

    /// Whether the blob directory holds a file under `hash`'s name.
    fn holds_bytes(&self, hash: &str) -> bool {
        std::fs::symlink_metadata(self.blobs.join(hash)).is_ok()
    }

    /// The database's row for `hash`.
    async fn row(&self, hash: &str) -> Option<nc_local::BlobRow> {
        self.store
            .inner
            .blob(hash)
            .await
            .expect("the store answers")
    }

    /// Names every file in the blob directory a grace and a second ago: bytes
    /// a sweep may take once no row names them.
    fn aged(&self) {
        let long_ago = std::time::SystemTime::now()
            - Duration::from_millis(KERNEL.blob_orphan_grace_ms)
            - Duration::from_secs(1);
        for entry in std::fs::read_dir(&self.blobs).expect("listed") {
            let path = entry.expect("an entry").path();
            if path.is_file() {
                std::fs::File::open(&path)
                    .and_then(|file| file.set_modified(long_ago))
                    .expect("this test's own file is aged");
            }
        }
    }

    /// A second store over the kernel's blob directory: what another process
    /// of the same user could write there.
    fn beside(&self) -> DirectoryBlobs {
        DirectoryBlobs::open(&self.blobs).expect("the blob directory opens again")
    }

    /// Every entry of the kernel's journal.
    async fn entries(&self) -> Vec<Entry> {
        self.kernel
            .journal_for_tests()
            .await
            .expect("the journal reads back")
    }
}

/// `blob.put`'s arguments: `text` written `times` over, then ending as `then`
/// says, and asking where it is when `located`.
fn put_args(text: &str, times: u32, then: Option<&str>, located: bool) -> String {
    let mut fields = vec![
        ("note", Value::text("photo")),
        ("text", Value::text(text)),
        ("times", Value::Integer(i64::from(times))),
    ];
    if let Some(then) = then {
        fields.push(("then", Value::text(then)));
    }
    if located {
        fields.push(("located", Value::Bool(true)));
    }
    Value::object(fields).to_canonical()
}

/// The arguments of a command about the blob `hash`, each read asking `max`.
fn hash_args(hash: &str, max: Option<u32>) -> String {
    let mut fields = vec![("note", Value::text("photo")), ("hash", Value::text(hash))];
    if let Some(max) = max {
        fields.push(("max", Value::Integer(i64::from(max))));
    }
    Value::object(fields).to_canonical()
}

/// The field `name` of an answer.
fn field(answered: &str, name: &str) -> Value {
    nc_canon::parse(answered)
        .ok()
        .and_then(Value::into_object)
        .and_then(|mut fields| fields.remove(name))
        .unwrap_or_else(|| panic!("`{answered}` has no `{name}`"))
}

/// The text field `name` of an answer.
fn text_of(answered: &str, name: &str) -> String {
    field(answered, name)
        .into_text()
        .unwrap_or_else(|| panic!("`{name}` of `{answered}` is not text"))
}

/// Whether `refusal` is of `kind`, and its sentence carries `code` — the
/// plugin's own refusal carries the kernel's, which it was told.
fn told(refusal: &Refusal, kind: RefusalKind, code: ErrorCode) -> bool {
    refusal.kind() == kind && refusal.sentence().contains(&format!("[{code}]"))
}

/// Where a blob is is `local` or `unavailable`, and never `remote`: a blob
/// this machine holds, made by the command's own unit or kept by a committed
/// one, is local; one it never held, one whose row stays while its bytes are
/// gone, bytes no row names and a blob no unit holds any more are not.
///
/// **No peer exists in this build**, so no answer can be `remote` honestly:
/// every answer below is collected, and both words this build may say are
/// said, and the third is not.
#[tokio::test]
async fn locate_answers_local_or_unavailable_and_never_remote() {
    let fixture = opened("locate").await;
    let mut said = std::collections::BTreeSet::new();
    let made_here = fixture
        .as_ana(
            "reference.blob.put",
            &put_args("asked at once", 1, None, true),
        )
        .await
        .expect("the blob is made and asked about");
    assert_eq!(
        text_of(&made_here, "availability"),
        "local",
        "a blob the command's own unit made is on this machine for that command"
    );
    said.insert(text_of(&made_here, "availability"));
    let kept = text_of(&made_here, "hash");
    for (hash, expected, what) in [
        (kept.clone(), "local", "a blob a committed unit keeps"),
        (
            Hash::of(b"never written").to_string(),
            "unavailable",
            "a blob nothing ever wrote",
        ),
    ] {
        let answered = fixture.located(&hash).await.expect("located");
        assert_eq!(answered, expected, "{what}");
        said.insert(answered);
    }

    // A row whose bytes are gone is not a blob this machine can read.
    let gone = fixture.put("its bytes go", 2).await;
    std::fs::remove_file(fixture.blobs.join(&gone)).expect("the bytes are removed");
    assert!(fixture.row(&gone).await.is_some_and(|row| row.pinned));
    // Bytes no row names are what the sweep deletes.
    let orphan = fixture.beside();
    let planted = orphan.create().await.expect("a writing");
    let (planted, wrote) = orphan.write(planted, b"no row names me".to_vec()).await;
    wrote.expect("written");
    let unnamed = orphan
        .finish(planted)
        .await
        .expect("finished")
        .hash
        .to_string();
    // A blob no unit holds any more is on its way out.
    let unpinned = fixture.put("no unit holds it", 1).await;
    assert!(
        fixture
            .store
            .inner
            .set_pinned(&unpinned, false, NOW_MS)
            .await
            .expect("unpinned")
    );
    for (hash, what) in [
        (gone, "a row whose bytes are gone"),
        (unnamed, "bytes no row names"),
        (unpinned, "a blob no unit holds"),
    ] {
        let answered = fixture.located(&hash).await.expect("located");
        assert_eq!(answered, "unavailable", "{what}");
        said.insert(answered);
    }
    assert_eq!(
        said.into_iter().collect::<Vec<_>>(),
        ["local", "unavailable"],
        "this build says a blob is here or is not, and never that a peer has it"
    );
}

/// Opening a blob this machine does not hold is refused at once.
///
/// It is `unavailable` with `[NC-BLOB-0002]`: no peer is asked, so nothing is
/// waited on.
///
/// **Measured against the host call's deadline**, which a kernel that waited
/// on a fetch would reach: the refusal comes back in a small part of it.
#[tokio::test]
async fn opening_a_blob_not_on_this_machine_is_unavailable_at_once() {
    let fixture = opened("not-here").await;
    let gone = fixture.put("here, then not", 1).await;
    std::fs::remove_file(fixture.blobs.join(&gone)).expect("the bytes are removed");
    for (hash, what) in [
        (
            Hash::of(b"on no machine").to_string(),
            "a blob never written",
        ),
        (gone, "a row whose bytes are gone"),
    ] {
        let started = Instant::now();
        let refused = fixture
            .as_ana("reference.blob.get", &hash_args(&hash, None))
            .await
            .expect_err("a blob not on this machine is not opened");
        let took = started.elapsed();
        assert!(
            told(&refused, RefusalKind::Unavailable, ErrorCode::BlobNotLocal),
            "{what}: {refused}"
        );
        assert!(
            refused
                .sentence()
                .contains(&format!("`{hash}` is not on this machine")),
            "{what}: {refused}"
        );
        assert!(
            took.saturating_mul(10) < Duration::from_millis(KERNEL.host_call_deadline_ms),
            "{what}: the refusal took {took:?}, which is no longer at once against a host \
             call's {} ms",
            KERNEL.host_call_deadline_ms
        );
    }
}

/// A blob is read back a chunk at a time.
///
/// No chunk is past `blob_read_chunk_bytes`, whatever the plugin asks for,
/// and the bytes read are the bytes written, which the hash names.
#[tokio::test]
async fn a_blob_streams_in_bounded_chunks() {
    let fixture = opened("chunks").await;
    let piece = "0123456789abcdef".repeat(256);
    let chunk = usize::try_from(KERNEL.blob_read_chunk_bytes).expect("fits");
    // Two whole chunks and half of another.
    let times = u32::try_from(chunk * 5 / 2 / piece.len()).expect("fits");
    let hash = fixture.put(&piece, times).await;
    let written = piece.repeat(usize::try_from(times).expect("fits"));
    assert_eq!(
        std::fs::read(fixture.blobs.join(&hash)).expect("the bytes are there"),
        written.as_bytes(),
        "the directory holds what was written under the hash's name"
    );
    let size = written.len();
    for (max, step) in [(None, chunk), (Some(300_000_u32), 300_000_usize)] {
        let answered = fixture
            .as_ana("reference.blob.get", &hash_args(&hash, max))
            .await
            .expect("the blob is read back");
        let expected: Vec<Value> = (0..size)
            .step_by(step)
            .map(|at| Value::Integer(i64::try_from(step.min(size - at)).expect("fits")))
            .collect();
        assert_eq!(
            field(&answered, "chunks"),
            Value::Array(expected),
            "asking for {max:?}: every read answered at most {step} bytes"
        );
        assert_eq!(
            field(&answered, "size"),
            Value::Integer(i64::try_from(size).expect("fits"))
        );
        assert_eq!(
            field(&answered, "text"),
            Value::Null,
            "too large to answer whole"
        );
    }
    // And the bytes a small blob reads back are its own.
    let small = fixture.put("ab", 3).await;
    let answered = fixture
        .as_ana("reference.blob.get", &hash_args(&small, Some(4)))
        .await
        .expect("read back");
    assert_eq!(
        (text_of(&answered, "text"), field(&answered, "chunks")),
        (
            "ababab".to_owned(),
            Value::Array(vec![Value::Integer(4), Value::Integer(2)])
        )
    );
}

/// Text that is not a hash is refused `invalid` with `[NC-BLOB-0001]` before
/// anything is read, and what was named is repeated cut to a bound.
#[tokio::test]
async fn a_malformed_hash_is_refused_before_anything_is_read() {
    let fixture = opened("malformed").await;
    let long = "f".repeat(4_000);
    for hash in [
        "",
        "not a hash",
        &"A".repeat(64),
        &"0".repeat(63),
        &"0".repeat(65),
        "../../etc/passwd",
        &long,
    ] {
        for command in ["reference.blob.locate", "reference.blob.get"] {
            let refused = fixture
                .as_ana(command, &hash_args(hash, None))
                .await
                .expect_err("a malformed hash is refused");
            assert!(
                told(&refused, RefusalKind::Invalid, ErrorCode::BlobMalformedHash),
                "`{command}` of {hash:?}: {refused}"
            );
            assert!(
                refused.sentence().len() < 600,
                "`{command}` repeated {} characters of what it was handed",
                refused.sentence().len()
            );
        }
    }
}

/// A blob made in a unit that is let go is gone.
///
/// Its command refused or its plugin trapped, no row names it, the directory
/// holds no bytes under its name, and the plugin is told it is not on this
/// machine.
#[tokio::test]
async fn a_blob_created_in_an_aborted_unit_is_gone() {
    for (then, kind, code) in [
        (
            "fail",
            RefusalKind::Invalid,
            ErrorCode::PluginCommandRefused,
        ),
        ("trap", RefusalKind::Internal, ErrorCode::PluginTrapped),
    ] {
        let fixture = opened(&format!("aborted-{then}")).await;
        let refused = fixture
            .as_ana(
                "reference.blob.put",
                &put_args("never kept", 3, Some(then), false),
            )
            .await
            .expect_err("the command did not end well");
        assert_eq!((refused.kind(), refused.code()), (kind, code), "{refused}");
        let hash = Hash::of(b"never keptnever keptnever kept").to_string();
        assert!(!fixture.holds_bytes(&hash), "`{then}`: the bytes were kept");
        assert_eq!(fixture.row(&hash).await, None, "`{then}`: a row was kept");
        assert_eq!(
            fixture.located(&hash).await.expect("located"),
            "unavailable",
            "`{then}`"
        );
        let opened_and_closed: Vec<Outcome> = fixture
            .entries()
            .await
            .iter()
            .filter(|entry| {
                [nc_journal::unit::OPENS, nc_journal::unit::CLOSES]
                    .contains(&entry.record.action.as_str())
            })
            .map(|entry| entry.record.result)
            .collect();
        assert_eq!(
            opened_and_closed,
            [Outcome::UnitBegun, Outcome::UnitAborted],
            "`{then}`"
        );
        assert_eq!(
            fixture
                .log
                .named("kernel.unit_aborted")
                .iter()
                .filter_map(|line| line.text("cause"))
                .collect::<Vec<_>>(),
            ["failed"],
            "`{then}`"
        );
    }
}

/// A blob made in a unit the database refused to commit is gone with it.
#[tokio::test]
async fn a_blob_made_in_a_unit_the_database_refused_to_commit_is_gone() {
    let fixture = opened("refused-commit").await;
    fixture.store.refuses.store(true, Ordering::SeqCst);
    let refused = fixture
        .as_ana(
            "reference.blob.put",
            &put_args("not committed", 1, None, false),
        )
        .await
        .expect_err("the database refused the commit");
    fixture.store.refuses.store(false, Ordering::SeqCst);
    assert_eq!(refused.kind(), RefusalKind::Unavailable, "{refused}");
    let hash = Hash::of(b"not committed").to_string();
    assert!(
        !fixture.holds_bytes(&hash),
        "the bytes outlived a refused commit"
    );
    assert_eq!(fixture.row(&hash).await, None);
    assert_eq!(
        fixture.located(&hash).await.expect("located"),
        "unavailable"
    );
}

/// A finish whose row the database refused to read names no bytes: the
/// writer's `finish` is refused, its unit is let go, and nothing is left under
/// the hash for the unit or the sweep to find.
///
/// **The row is read first**: a `finish` that named the bytes before it read
/// their row, and recorded them in its unit only once both had answered, would
/// leave a refused read's file under the hash that no row names and the unit
/// let go knows nothing of, while the plugin is told the blob is not on this
/// machine. Read first, a refusal lets the writing go unnamed.
#[tokio::test]
async fn a_finish_whose_row_the_database_refused_names_no_bytes() {
    let fixture = opened("refused-row").await;
    fixture.store.refuses_rows.store(true, Ordering::SeqCst);
    let refused = fixture
        .as_ana(
            "reference.blob.put",
            &put_args("its row was refused", 1, None, false),
        )
        .await
        .expect_err("the database refused the row");
    fixture.store.refuses_rows.store(false, Ordering::SeqCst);
    assert!(
        told(&refused, RefusalKind::Unavailable, ErrorCode::DbUnreachable),
        "{refused}"
    );
    assert!(
        refused.sentence().contains("the database refuses the row"),
        "{refused}"
    );
    let hash = Hash::of(b"its row was refused").to_string();
    assert!(
        !fixture.holds_bytes(&hash),
        "the bytes a finish named before it read their row outlived the unit let go"
    );
    assert_eq!(fixture.row(&hash).await, None);
    assert_eq!(
        fixture.located(&hash).await.expect("located"),
        "unavailable"
    );
    assert_eq!(
        fixture.kernel.sweep_blobs().await.expect("swept"),
        Swept::default(),
        "the unit let go left something behind for the sweep"
    );
}

/// A blob this machine held before a unit made it again survives the unit
/// being let go: a blob a committed unit keeps stays pinned and readable, and
/// bytes no row named stay for the sweep, which is theirs to delete.
#[tokio::test]
async fn a_blob_that_already_existed_survives_the_abort() {
    let fixture = opened("survives").await;
    let kept = fixture.put("kept before", 2).await;
    let refused = fixture
        .as_ana(
            "reference.blob.put",
            &put_args("kept before", 2, Some("fail"), false),
        )
        .await
        .expect_err("the command refused");
    assert!(refused.sentence().contains(&kept), "{refused}");
    assert!(
        fixture.holds_bytes(&kept),
        "a committed unit's bytes were deleted"
    );
    assert!(fixture.row(&kept).await.is_some_and(|row| row.pinned));
    assert_eq!(fixture.located(&kept).await.expect("located"), "local");
    let answered = fixture
        .as_ana("reference.blob.get", &hash_args(&kept, None))
        .await
        .expect("still read back");
    assert_eq!(text_of(&answered, "text"), "kept beforekept before");

    // Bytes on disk that no row named are left by the unit that made them
    // again, and the sweep deletes them.
    let beside = fixture.beside();
    let planted = beside.create().await.expect("a writing");
    let (planted, wrote) = beside.write(planted, b"left by a stop".to_vec()).await;
    wrote.expect("written");
    let left = beside
        .finish(planted)
        .await
        .expect("finished")
        .hash
        .to_string();
    fixture
        .as_ana(
            "reference.blob.put",
            &put_args("left by a stop", 1, Some("fail"), false),
        )
        .await
        .expect_err("the command refused");
    assert!(
        fixture.holds_bytes(&left),
        "bytes the unit did not make were deleted"
    );
    assert_eq!(fixture.row(&left).await, None);
    fixture.aged();
    let swept = fixture.kernel.sweep_blobs().await.expect("swept");
    assert_eq!(
        swept,
        Swept {
            forgotten: 0,
            orphans: 1,
            unfinished: 0
        }
    );
    assert!(!fixture.holds_bytes(&left));
    assert!(
        fixture.holds_bytes(&kept),
        "the sweep took a pinned blob's bytes"
    );
}

/// A sweep deletes what no unit has held past its grace, and what no row names.
///
/// That is a blob unpinned for longer than `blob_orphan_grace_ms`, row and
/// bytes, the bytes no row names and the writings no writer holds; it keeps a
/// pinned blob, one unpinned exactly its grace before the sweep, and a file
/// that is neither a blob nor a writing.
///
/// **The two unpinned blobs are a millisecond apart, one on each side of the
/// grace.** The sweep runs a grace and a millisecond after `old` was unpinned,
/// which is past it, and exactly a grace after `recent` was, which is not yet.
/// A sweep that forgets anything before the whole grace has passed takes
/// `recent` too, and a redo of the unit that made it would find its bytes
/// gone.
#[tokio::test]
async fn a_sweep_deletes_only_what_no_unit_held_past_its_grace_and_what_no_row_names() {
    let fixture = opened("sweep").await;
    let pinned = fixture.put("pinned", 1).await;
    let old = fixture.put("unpinned long ago", 1).await;
    let recent = fixture.put("unpinned a grace ago", 1).await;
    for (hash, unpinned_at) in [(&old, NOW_MS), (&recent, NOW_MS + 1)] {
        assert!(
            fixture
                .store
                .inner
                .set_pinned(hash, false, unpinned_at)
                .await
                .expect("unpinned")
        );
    }
    fixture.clock.advance(KERNEL.blob_orphan_grace_ms + 1);
    let beside = fixture.beside();
    let planted = beside.create().await.expect("a writing");
    let (planted, wrote) = beside.write(planted, b"no row".to_vec()).await;
    wrote.expect("written");
    let orphan = beside
        .finish(planted)
        .await
        .expect("finished")
        .hash
        .to_string();
    // A writing a stopped process never finished: its writer is never
    // dropped, so nothing deletes the file but the sweep.
    let (stopped, wrote) = beside
        .write(beside.create().await.expect("a writing"), b"half".to_vec())
        .await;
    wrote.expect("written");
    std::mem::forget(stopped);
    std::fs::write(fixture.blobs.join("notes-of-someone-else"), b"").expect("planted");
    let before = std::fs::read_dir(&fixture.blobs).expect("listed").count();
    fixture.aged();

    let swept = fixture.kernel.sweep_blobs().await.expect("swept");
    assert_eq!(
        swept,
        Swept {
            forgotten: 1,
            orphans: 1,
            unfinished: 1
        }
    );
    assert_eq!(fixture.row(&old).await, None, "the old blob's row stays");
    assert!(!fixture.holds_bytes(&old), "the old blob's bytes stay");
    assert!(!fixture.holds_bytes(&orphan), "the orphan stays");
    assert!(fixture.holds_bytes(&pinned) && fixture.row(&pinned).await.is_some());
    assert!(
        fixture.holds_bytes(&recent) && fixture.row(&recent).await.is_some_and(|row| !row.pinned),
        "a blob unpinned exactly its grace before the sweep was swept: its grace had not \
         yet passed"
    );
    assert!(fixture.blobs.join("notes-of-someone-else").exists());
    assert_eq!(
        std::fs::read_dir(&fixture.blobs).expect("listed").count(),
        before - 3
    );
    assert_eq!(
        fixture.kernel.sweep_blobs().await.expect("swept again"),
        Swept::default(),
        "a second sweep finds nothing more"
    );
}

/// A directory under a blob's or a writing's name is not one, and stops no
/// sweep.
///
/// **No unlink deletes a directory**, so a sweep that took one for bytes no row
/// names, for a writing no writer holds or for the bytes of a row past its
/// grace would be refused at it, and every sweep after it refused at the same
/// entry: the writings a stopped process left, which a sweep deletes last,
/// would never be deleted. Measured on a sweep that lists directories, one
/// directory under a hash's name would make two sweeps in a row refuse at it
/// with `[NC-BLOB-0004]` ("Is a directory"), and the unfinished writing beside
/// it stay.
#[tokio::test]
async fn a_directory_under_a_blobs_name_is_not_a_blob_and_stops_no_sweep() {
    let fixture = opened("directories").await;
    let unnamed = Hash::of(b"a directory").to_string();
    let writing = ".writing-0-0-0";
    // The bytes of a row past its grace, become a directory.
    let became = fixture.put("its bytes became a directory", 1).await;
    std::fs::remove_file(fixture.blobs.join(&became)).expect("the bytes are removed");
    for planted in [unnamed.as_str(), writing, became.as_str()] {
        std::fs::create_dir(fixture.blobs.join(planted)).expect("planted");
    }
    assert!(
        fixture
            .store
            .inner
            .set_pinned(&became, false, NOW_MS)
            .await
            .expect("unpinned")
    );
    fixture.clock.advance(KERNEL.blob_orphan_grace_ms + 1);
    // And what a sweep is for, beside them.
    let beside = fixture.beside();
    let planted = beside.create().await.expect("a writing");
    let (planted, wrote) = beside.write(planted, b"no row names me".to_vec()).await;
    wrote.expect("written");
    let orphan = beside
        .finish(planted)
        .await
        .expect("finished")
        .hash
        .to_string();
    let (stopped, wrote) = beside
        .write(beside.create().await.expect("a writing"), b"half".to_vec())
        .await;
    wrote.expect("written");
    std::mem::forget(stopped);
    fixture.aged();

    let swept = fixture.kernel.sweep_blobs().await.expect("swept");
    assert_eq!(
        swept,
        Swept {
            forgotten: 1,
            orphans: 1,
            unfinished: 1
        }
    );
    assert_eq!(
        fixture.kernel.sweep_blobs().await.expect("swept again"),
        Swept::default(),
        "a second sweep stopped where the first did, or found more"
    );
    assert!(!fixture.holds_bytes(&orphan), "the orphan stays");
    assert_eq!(
        fixture.row(&became).await,
        None,
        "the row past its grace stays"
    );
    // **And the store, asked to delete one, holds no bytes there.** A sweep
    // lists no directory, so it never asks; this is what is left for one that
    // takes a hash's name after a sweep listed it. This asks the store itself,
    // so its refusing such a name is decided here.
    assert_eq!(
        fixture
            .beside()
            .remove(&Hash::parse(&unnamed).expect("a hash"))
            .await,
        Ok(false),
        "a directory under a hash's name was deleted as its bytes, or refused"
    );
    for planted in [unnamed.as_str(), writing, became.as_str()] {
        assert!(
            fixture.blobs.join(planted).is_dir(),
            "`{planted}` is not the store's to delete"
        );
    }
    assert_eq!(
        std::fs::read_dir(&fixture.blobs).expect("listed").count(),
        3,
        "a writing no writer holds, or bytes no row names, stayed"
    );
}

/// A sweep waits for the call tree that is running: the blob a unit has made
/// and not yet committed is bytes no row names, which a sweep in the middle of
/// the tree would take for an orphan.
///
/// **Held at the commit**, the one moment a unit's bytes are named by no row
/// and the tree has finished every command: the sweep is asked for then, does
/// not answer while the commit is held, and finds nothing to delete once it is
/// let go.
///
/// **The command races its own arrival at the commit**, so one that answers
/// before it gets there fails this test rather than leaving it waiting for a
/// commit that never comes: a store whose directory was never made refuses
/// the blob at once, and this test then hung until something killed it.
#[tokio::test]
async fn a_sweep_waits_for_the_call_tree_that_is_running() {
    let fixture = opened("waits").await;
    fixture.store.holds.store(true, Ordering::SeqCst);
    let args = put_args("made while a sweep waits", 1, None, false);
    let dispatched = fixture.as_ana("reference.blob.put", &args);
    tokio::pin!(dispatched);
    tokio::select! {
        answered = &mut dispatched => {
            panic!("the command answered before its unit reached the commit: {answered:?}")
        }
        () = fixture.store.reached.notified() => {}
    }
    fixture.store.holds.store(false, Ordering::SeqCst);
    let sweep = fixture.kernel.sweep_blobs();
    tokio::pin!(sweep);
    assert!(
        tokio::time::timeout(Duration::from_millis(200), &mut sweep)
            .await
            .is_err(),
        "the sweep ran while a call tree held a unit it had not committed"
    );
    fixture.store.released.notify_one();
    let (dispatched, swept) = tokio::join!(dispatched, sweep);
    let hash = text_of(&dispatched.expect("the blob is made"), "hash");
    assert_eq!(swept.expect("swept"), Swept::default());
    assert!(
        fixture.holds_bytes(&hash),
        "the sweep took the unit's bytes"
    );
    assert!(fixture.row(&hash).await.is_some_and(|row| row.pinned));
    assert_eq!(fixture.located(&hash).await.expect("located"), "local");
}

/// An announcement reaches no peer and says so.
///
/// It is `unavailable` with `[NC-BLOB-0006]`, and the rest of the interface
/// answers the probe as it is built to: a writer made, written and finished,
/// a blob not here refused.
#[tokio::test]
async fn an_announcement_reaches_no_peer_and_says_so() {
    let fixture = opened("announce").await;
    let probed = fixture
        .as_ana(
            "reference.probe",
            &Value::object([("note", Value::text("photo"))]).to_canonical(),
        )
        .await
        .expect("the probe answers");
    let outcome = |function: &str| text_of(&probed, function);
    assert!(
        outcome("blobs#announce")
            .starts_with(&format!("unavailable: [{}] ", ErrorCode::BlobNoPeers)),
        "{probed}"
    );
    assert!(
        outcome("blobs#open").starts_with(&format!("unavailable: [{}] ", ErrorCode::BlobNotLocal)),
        "{probed}"
    );
    for function in [
        "blobs#locate",
        "blobs#create",
        "blobs#[method]writer.write",
        "blobs#[method]writer.finish",
    ] {
        assert_eq!(outcome(function), "answered", "`{function}`: {probed}");
    }
    for function in [
        "blobs#[method]reader.size",
        "blobs#[method]reader.read",
        "blobs#[method]reader.seek",
    ] {
        assert_eq!(
            outcome(function),
            "unreachable",
            "`{function}`: the probe opens a blob no machine holds"
        );
    }
    // The probe's finished writer made the blob of nothing, kept with the
    // probe's unit.
    let nothing = Hash::of(b"").to_string();
    assert!(fixture.holds_bytes(&nothing));
    assert!(fixture.row(&nothing).await.is_some_and(|row| row.pinned));
}

/// A directory another user could reach is not one a kernel keeps blobs in.
///
/// **One body for every system**: `nc_platform::testing` opens the directory
/// to another user as this system does — a group bit on Unix, an entry for a
/// group on Windows — and answers the words the refusal names that by.
#[test]
fn a_blob_directory_another_user_can_reach_is_refused() {
    use nc_platform::testing;

    let scratch = Scratch::new("refused");
    let blobs: &Path = &scratch.0.join("blobs");
    std::fs::create_dir(blobs).expect("made");
    testing::make_private(blobs).expect("private");
    let said = testing::open_to_another_user(blobs).expect("opened up");
    let refused = DirectoryBlobs::open(blobs).expect_err("refused");
    assert_eq!(refused.code(), ErrorCode::DbLocalOpenRefused, "{refused}");
    assert!(refused.to_string().contains(&said), "{said}: {refused}");
}
