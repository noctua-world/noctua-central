//! What the daemon's kernel runs on (`NOCTUA.md §11.1`): the local database
//! and its journal, the blobs on disk, the system's clock, the world the
//! profile decides as, the central log, and the transport a plugin's request
//! is sent with.
//!
//! **Opened in the order a start needs**: the local database first, so the
//! newest unit its journal holds is read before the mark is waited on; then,
//! once the minter is behind its ceiling, the journal over the same file and
//! the kernel, which recovers what a stopped process left before it opens.
//!
//! **The central log is the manifest's**, and the daemon writes its own events
//! to the same sink the kernel writes to. A store that does not answer is
//! `nc_log`'s to handle — its lines go to the daemon's standard error — but an
//! install naming no store at all, or one `nc_log` refuses, is refused here
//! (`NC-DMN-0011`): a daemon whose refusals reach nobody is not started.

use std::sync::{Arc, Mutex};

use nc_ids::{Minter, Snowflake, SystemClock};
use nc_kernel::{Kernel, KernelConfig};
use nc_local::{LocalJournal, LocalStore};

use crate::Refused;

/// The environment a daemon opens its kernel in.
#[derive(Debug)]
pub struct Daemon;

impl nc_kernel::Environment for Daemon {
    type Store = LocalStore;
    type Blobs = nc_kernel::blobs::DirectoryBlobs;
    type Journal = LocalJournal;
    type Clock = SystemClock;
    type Directory = nc_authz::MemoryDirectory;
    type Log = Sink;
    type Transport = nc_kernel::net::UreqTransport;
}

/// The central log, shared by the kernel and the daemon's own events.
pub type Sink = Arc<Mutex<nc_log::Log>>;

/// The version every line the daemon logs carries: this build's.
struct Version;

impl nc_log::Source for Version {
    const TEXT: &'static str = env!("CARGO_PKG_VERSION");
}

/// Who the daemon's lines say they come from.
#[must_use]
pub fn origin() -> nc_log::Origin {
    nc_log::Origin {
        service: nc_log::literal!("noctua-daemon"),
        version: nc_log::Literal::of::<Version>(),
    }
}

/// The central log `install` names.
///
/// # Errors
///
/// [`Refused::NoCentralLog`] for an install naming none, and for one naming a
/// store `nc_log` refuses to speak to.
pub fn log(install: &nc_channel::Install) -> Result<Sink, Refused> {
    let endpoint = install
        .log_endpoint
        .as_deref()
        .ok_or_else(|| Refused::NoCentralLog {
            why: "the install names no central log".to_owned(),
        })?;
    let opened = nc_log::Log::open(origin(), endpoint).map_err(|why| Refused::NoCentralLog {
        why: format!("the install's central log is refused: {why}"),
    })?;
    Ok(Arc::new(Mutex::new(opened)))
}

/// The local database in the profile's data directory, and the newest unit
/// its journal holds, which a start resumes its identifiers above.
///
/// # Errors
///
/// [`Refused::Store`] when the database did not open, and
/// [`Refused::Journal`] when its journal could not say its newest unit.
pub async fn store(
    layout: &nc_channel::Layout,
) -> Result<(LocalStore, Option<Snowflake>), Refused> {
    let store = LocalStore::open(&layout.data)
        .await
        .map_err(Refused::Store)?;
    match store.journal().newest_unit().await {
        Ok(newest) => Ok((store, newest)),
        Err(refused) => {
            store.close().await;
            Err(Refused::Journal(refused))
        }
    }
}

/// What a kernel is opened over, besides the store.
#[derive(Debug)]
pub struct Opening {
    /// The world the profile decides as.
    pub world: nc_authz::LocalWorld,
    /// The decision point of the pack the world's realm selects.
    pub pdp: nc_authz::Pdp,
    /// The journal's keeper: the device's.
    pub keeper: nc_ids::OpaqueId,
    /// The one minter every unit of the profile is minted by, behind its mark.
    pub minter: Arc<Mutex<Minter<SystemClock>>>,
    /// The central log.
    pub log: Sink,
}

/// The kernel over `store`, opened with what `opening` holds: the journal
/// over the same file, sharing the minter, and the blobs in the profile.
///
/// **Recovery runs before it answers** (`Kernel::open`): what a stopped
/// process left committed in the store and open in the journal is reverted
/// first, or the kernel does not open.
///
/// # Errors
///
/// [`Refused::Blobs`] for a blobs' directory that is refused,
/// [`Refused::Engine`] for an engine wasmtime refuses, and
/// [`Refused::Unrecovered`] for a unit left open that was not recovered.
pub async fn kernel(
    layout: &nc_channel::Layout,
    store: LocalStore,
    opening: Opening,
) -> Result<Kernel<Daemon>, Refused> {
    let Opening {
        world,
        pdp,
        keeper,
        minter,
        log,
    } = opening;
    let blobs = nc_kernel::blobs::DirectoryBlobs::open(&layout.blobs).map_err(Refused::Blobs)?;
    let realm = world.realm().clone();
    let journal = nc_journal::Journal::sharing(store.journal(), realm, keeper, minter);
    let config = KernelConfig::<Daemon> {
        store,
        blobs,
        pdp: Ok(pdp),
        journal,
        directory: world.into_directory(),
        clock: SystemClock,
        log,
        transport: nc_kernel::net::UreqTransport::new(),
    };
    Kernel::open(config).await.map_err(|refused| match refused {
        nc_kernel::OpenError::Engine(engine) => Refused::Engine(engine.to_string()),
        nc_kernel::OpenError::Unrecovered(refusal) => Refused::Unrecovered(refusal),
    })
}

#[cfg(test)]
mod tests {
    use nc_catalog::ErrorCode;

    fn an_install(log_endpoint: Option<&str>) -> nc_channel::Install {
        nc_channel::Install {
            channel: nc_channel::Channel::Staging,
            bases: nc_channel::Bases::new(
                "/nowhere/data".into(),
                "/nowhere/config".into(),
                "/nowhere/cache".into(),
                "/nowhere/state".into(),
            )
            .expect("plain bases"),
            namespace: None,
            log_endpoint: log_endpoint.map(str::to_owned),
        }
    }

    /// An install naming no central log is refused, and so is one naming a
    /// store off this machine; one on this machine opens, whether or not
    /// anything answers there.
    #[test]
    fn an_install_with_no_central_log_it_can_speak_to_is_refused() {
        for (endpoint, says) in [
            (None, "names no central log"),
            (Some("http://example.com:9428/insert"), "is refused"),
        ] {
            let refused = super::log(&an_install(endpoint)).expect_err("refused");
            assert_eq!(refused.code(), ErrorCode::DaemonNoCentralLog, "{refused}");
            assert!(refused.to_string().contains(says), "{refused}");
        }
        super::log(&an_install(Some("http://127.0.0.1:0/insert")))
            .expect("a store on this machine, answering or not");
    }

    /// A scratch directory for one test, removed however the test leaves.
    struct Scratch(std::path::PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = nc_platform::testing::make_private(&self.0);
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "nc-daemon-environment-{}-{name}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&root);
            nc_platform::create_private_directory(&root).expect("a scratch directory");
            Self(root)
        }
    }

    /// The local database a start opens is the profile's, and the newest unit
    /// it reads is the newest its journal holds — none in a new profile.
    #[tokio::test]
    async fn a_start_reads_the_newest_unit_its_journal_holds() {
        use nc_journal::Store as _;

        let scratch = Scratch::new("store");
        let base = |name: &str| {
            let base = scratch.0.join(name);
            nc_platform::create_private_directory(&base).expect("a base");
            base
        };
        let install = nc_channel::Install {
            channel: nc_channel::Channel::Staging,
            bases: nc_channel::Bases::new(base("d"), base("c"), base("k"), base("s"))
                .expect("plain bases"),
            namespace: Some(nc_channel::Namespace::parse("e1").expect("a namespace")),
            log_endpoint: None,
        };
        let layout = nc_channel::layout(&install, "1000");
        nc_platform::create_private_directory(&layout.data).expect("the data directory");
        let (store, newest) = super::store(&layout).await.expect("opened");
        assert_eq!(newest, None, "a new profile's journal holds a unit");
        // Minted and derived, as the daemon's own are: nothing here is built
        // out of text.
        let flake = nc_ids::MachineFlake::from_value(3).expect("a flake");
        let unit = nc_ids::Minter::new(flake, nc_ids::SystemClock)
            .mint()
            .expect("a unit");
        let someone = nc_ids::OpaqueId::of_device_key(&[7; 32]);
        let mut record = nc_journal::Record::new(
            someone.clone(),
            nc_journal::unit::OPENS,
            nc_canon::Value::object([("kind", nc_canon::Value::text("realm"))]),
            nc_journal::Outcome::UnitBegun,
            1_767_225_600_000,
        );
        record.realm = someone;
        record.work_unit = Some(unit);
        let mut journal = store.journal();
        journal
            .append(nc_journal::Entry::chain(0, None, record))
            .await
            .expect("appended");
        store.close().await;
        let (store, newest) = super::store(&layout).await.expect("opened again");
        assert_eq!(newest, Some(unit));
        store.close().await;
    }

    /// The daemon's lines say they come from the daemon, at this build's
    /// version.
    #[test]
    fn the_daemons_lines_name_the_daemon_and_this_build() {
        let origin = super::origin();
        assert_eq!(origin.service.get(), "noctua-daemon");
        assert_eq!(origin.version.get(), env!("CARGO_PKG_VERSION"));
    }
}
