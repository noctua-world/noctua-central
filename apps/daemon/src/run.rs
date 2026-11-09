//! A daemon's run, from its install to its stop (`NOCTUA.md §11.1`, `§17.1`).
//!
//! # The start, in order
//!
//! 1. **The install** beside the canonicalized executable.
//! 2. **The profile**, its directories made private or judged, and **the
//!    instance lock** in its state directory, taken at once or not at all:
//!    one daemon keeps one profile, and a second refuses (`NC-DMN-0008`). The
//!    lock is the state directory's, never the data directory's, which each
//!    open of the local database holds for as long as it opens.
//! 3. **The device and the world**, read and vouched for, **the central log**
//!    the install names, and a session of the daemon's own
//!    (`nc_platform::leave_the_session`), so no terminal it was started from
//!    signals it — left only once every refusal a start reads its profile for
//!    is behind it.
//! 4. **The PID record** — the process as the system says it is, and the
//!    instance drawn now — **a token drawn fresh**, each written whole and
//!    private in the runtime directory, and **the endpoint bound**. From here
//!    the status is answered.
//! 5. **The local database**, and the newest unit its journal holds; then
//!    **the mark**, waited on until the clock passes it and the newest unit
//!    (state `waiting-for-clock`).
//! 6. **The kernel** over the local database, its journal sharing the one
//!    minter, which recovers what a stopped process left before it opens
//!    (state `recovering`).
//! 7. **The plugins** installed in the profile, each loaded or refused alone.
//! 8. **Running**: dispatches are served, and the pump and the sweep run on
//!    their schedule.
//!
//! # The stop
//!
//! A stop asked through the endpoint refuses every new dispatch, stops
//! accepting, and waits `stop_drain_ms` at most for the dispatches running;
//! then **settles the mark** to the exact high-water mark, so a clean restart
//! has nothing to wait for, closes the local database, removes the PID record
//! and the token it wrote — the socket goes with the listener — and lets the
//! instance lock go. There is no signal handler: a daemon killed leaves what a
//! crash leaves, which the next start reads past.
//!
//! **Its standard handles are its spawner's to make null**, and the daemon
//! writes to none but its standard error, which the central log's shipper
//! writes the lines it could not deliver to.

use std::path::Path;
use std::sync::{Arc, Mutex};

use core::time::Duration;

use nc_kernel::logging::LogSink;

use crate::environment::{self, Opening};
use crate::server::{Named, Server};
use crate::{Refused, device, install, mark, pidfile, plugins, profile};

/// How a run that stopped when it was asked ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stopped {
    /// How long it ran.
    pub ran: Duration,
    /// How many dispatches were still running when `stop_drain_ms` ran out.
    pub abandoned: u32,
}

/// Runs the daemon of the install beside `executable` until it is asked to
/// stop.
///
/// # Errors
///
/// [`Refused`], with the code of the first thing the start could not vouch
/// for: the install, the profile, the instance lock, the device, the world,
/// the central log, the endpoint, the local database, the mark, the blobs or
/// the kernel.
pub fn serve(executable: &Path) -> Result<Stopped, Refused> {
    let began = std::time::Instant::now();
    let found = install::beside(executable)?;
    let (layout, user) = (&found.layout, &found.user);
    profile::prepare(layout, user)?;
    let lock_name = layout
        .instance_lock
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    let held = nc_platform::try_hold(&layout.state, &lock_name.to_string_lossy(), user)
        .map_err(Refused::judged)?
        .ok_or_else(|| Refused::Held {
            path: layout.state.clone(),
        })?;
    let device = device::Device::read(layout, user)?;
    let (world, pdp) = profile::world_and_pack(layout, user)?;
    let log = environment::log(&found.install)?;
    nc_platform::leave_the_session().map_err(|error| {
        Refused::unavailable(
            "the daemon could not leave the session it was started in".to_owned(),
            error,
        )
    })?;
    let identity = nc_platform::process::this_process().map_err(|error| {
        Refused::unavailable(
            "the system could not say which process this is".to_owned(),
            error,
        )
    })?;
    let mut instance = [0_u8; 16];
    nc_platform::random(&mut instance).map_err(|error| {
        Refused::unavailable("the daemon's instance could not be drawn".to_owned(), error)
    })?;
    let token = nc_ipc::Token::fresh().map_err(|error| {
        Refused::unavailable("the channel's token could not be drawn".to_owned(), error)
    })?;
    // Built before anything of the run is written, so a start that cannot
    // have one leaves no token and no record behind.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            Refused::unavailable("the daemon's runtime could not be built".to_owned(), error)
        })?;
    // The token first and the record last, so a record naming this process
    // says the token beside it is this start's.
    pidfile::replace_private(&layout.token, &token.to_text(), user)?;
    let record = pidfile::Record::of(identity, instance);
    pidfile::write(&layout.pid_file, &record, user)?;
    let named = Named {
        pid: record.pid,
        instance,
        channel: found.install.channel,
        realm: world.realm().as_str().to_owned(),
        person: world.person().clone(),
        since: crate::server::now_ms(),
    };
    let server = Arc::new(Server::new(named, token, Arc::clone(&log)));
    let started = Started {
        found: &found,
        held: &held,
        device,
        world,
        pdp,
        log: Arc::clone(&log),
    };
    let ran = runtime.block_on(run(started, Arc::clone(&server)));
    // A blocking task still waiting for the clock is not waited for past the
    // drain: the run is over.
    runtime.shutdown_timeout(Duration::from_millis(nc_catalog::DAEMON.stop_drain_ms));
    let _ = pidfile::remove_if_ours(&layout.pid_file, instance, user);
    let _ = std::fs::remove_file(&layout.token);
    drop(held);
    let abandoned = ran?;
    let stopped = Stopped {
        ran: began.elapsed(),
        abandoned,
    };
    log.record(&nc_log::events::DaemonStopped {
        ran: stopped.ran,
        abandoned,
    });
    Ok(stopped)
}

/// What the start read before it bound the endpoint.
struct Started<'a> {
    found: &'a install::Found,
    held: &'a nc_platform::Held,
    device: device::Device,
    world: nc_authz::LocalWorld,
    pdp: nc_authz::Pdp,
    log: environment::Sink,
}

/// The run on the runtime: the endpoint bound and served, the kernel opened
/// beside it, and the stop; answers how many dispatches the stop gave up on.
async fn run(started: Started<'_>, server: Arc<Server>) -> Result<u32, Refused> {
    let layout = &started.found.layout;
    let endpoint = nc_platform::ipc::Endpoint::new(
        layout.runtime.clone(),
        layout.socket.clone(),
        layout.pipe.clone(),
    );
    let listener =
        nc_ipc::listen(&endpoint, &started.found.user, started.held).map_err(Refused::Endpoint)?;
    let accepting = tokio::spawn(Arc::clone(&server).accepting(listener));
    let opened = tokio::select! {
        opened = opened(started, &server) => opened,
        () = server.stopping() => Ok(None),
    };
    let (kernel, store, minter) = match opened {
        Ok(Some(opened)) => opened,
        Ok(None) => {
            accepting.abort();
            return Ok(0);
        }
        Err(refused) => {
            accepting.abort();
            return Err(refused);
        }
    };
    let schedule = server.schedule();
    let pumping = tokio::spawn(crate::schedule::pumping(
        Arc::clone(&kernel),
        Arc::clone(&schedule),
        server.log(),
    ));
    let sweeping = tokio::spawn(crate::schedule::sweeping(Arc::clone(&kernel), schedule));
    server.stopping().await;
    accepting.abort();
    pumping.abort();
    sweeping.abort();
    let abandoned = server
        .drained(Duration::from_millis(nc_catalog::DAEMON.stop_drain_ms))
        .await;
    // The exact high-water mark, so a clean restart waits for nothing. A
    // settle that failed leaves the ceiling the last raise wrote, which no
    // identifier was handed out past.
    let _ = mark::settled(&minter);
    drop(kernel);
    store.close().await;
    Ok(abandoned)
}

/// The kernel open and its plugins loaded, with what a stop settles and
/// closes; nothing when a stop came first.
type Opened = (
    Arc<nc_kernel::Kernel<environment::Daemon>>,
    nc_local::LocalStore,
    Arc<Mutex<nc_ids::Minter<nc_ids::SystemClock>>>,
);

/// Opens the local database, waits on the mark, opens the kernel and loads
/// the plugins, moving the server's state on at each step.
async fn opened(started: Started<'_>, server: &Arc<Server>) -> Result<Option<Opened>, Refused> {
    let Started {
        found,
        device,
        world,
        pdp,
        log,
        ..
    } = started;
    let layout = &found.layout;
    let (store, newest) = environment::store(layout).await?;
    let (file, read) = mark::MarkFile::open(&layout.mark, &found.user)?;
    let flake = device.flake();
    let waiting = Arc::clone(server);
    let marking = Arc::clone(&log);
    let minter = tokio::task::spawn_blocking(move || {
        mark::minter(
            file,
            read,
            newest,
            flake,
            &nc_ids::SystemClock,
            &marking,
            &mut |behind| {
                waiting.waiting_for_the_clock(behind);
                std::thread::sleep(behind.min(Duration::from_millis(100)));
            },
        )
    })
    .await
    .map_err(|error| {
        Refused::unavailable(
            "the wait on the mark did not finish".to_owned(),
            std::io::Error::other(error.to_string()),
        )
    })??;
    let minter = Arc::new(Mutex::new(minter));
    server.reach(nc_ipc::State::Recovering);
    let opening = Opening {
        world,
        pdp,
        keeper: device.keeper(),
        minter: Arc::clone(&minter),
        log: Arc::clone(&log),
    };
    let kernel = Arc::new(environment::kernel(layout, store.clone(), opening).await?);
    let loaded = plugins::load(&kernel, &layout.plugins, &found.user, &log).await;
    let refused = loaded
        .iter()
        .filter(|plugin| plugin.refused.is_some())
        .count();
    log.record(&nc_log::events::DaemonStarted {
        pid: std::process::id(),
        loaded: u32::try_from(loaded.len() - refused).unwrap_or(u32::MAX),
        refused: u32::try_from(refused).unwrap_or(u32::MAX),
    });
    server.serving(Arc::clone(&kernel), loaded);
    Ok(Some((kernel, store, minter)))
}

#[cfg(test)]
mod tests {
    use nc_catalog::ErrorCode;

    /// A start refuses before anything of its run is made: outside an
    /// install with the channel's code, and on a profile another daemon holds
    /// with the profile's — and only by what its profile says, never by
    /// starting.
    ///
    /// **Held while no central log is named**, so a start that read past the
    /// lock would refuse at the log, before it left the session this suite
    /// runs in or bound anything. No start here is let past that fence: one
    /// that got through would be a daemon in this suite's own process.
    #[test]
    fn a_start_refuses_outside_an_install_and_on_a_profile_another_holds() {
        let installed =
            crate::testing::Installed::new(&std::env::current_exe().expect("this test binary"));
        let stand_in = installed.layout.data.join("noctua-daemon");
        nc_platform::create_private(&stand_in).expect("a binary with no manifest beside it");
        let refused = super::serve(&stand_in).expect_err("no install there");
        assert_eq!(refused.code(), ErrorCode::ChannelNoManifest, "{refused}");
        installed.without_a_central_log();
        let lock = installed
            .layout
            .instance_lock
            .file_name()
            .expect("a name")
            .to_string_lossy()
            .into_owned();
        let held = nc_platform::try_hold(&installed.layout.state, &lock, &installed.user)
            .expect("asked")
            .expect("nothing held it");
        let refused = super::serve(&installed.executable).expect_err("another holds it");
        assert_eq!(refused.code(), ErrorCode::DaemonInstanceHeld, "{refused}");
        assert!(
            !installed.layout.pid_file.exists() && !installed.layout.token.exists(),
            "a refused start wrote its run"
        );
        drop(held);
    }

    /// Two installs a suite makes never share a namespace or a root, each is
    /// initialized as the installer initializes one and names a central log
    /// nothing answers at, and each is gone with its guard — the ground every
    /// suite that starts a daemon stands on (`crate::testing`).
    #[test]
    fn an_install_a_suite_makes_is_its_own_and_gone_with_its_guard() {
        let binary = std::env::current_exe().expect("this test binary");
        let one = crate::testing::Installed::new(&binary);
        let two = crate::testing::Installed::new(&binary);
        assert_ne!(one.install.namespace, two.install.namespace);
        assert_ne!(one.layout.state, two.layout.state);
        for installed in [&one, &two] {
            assert!(installed.executable.is_file());
            assert!(installed.layout.device_key.is_file(), "no device was drawn");
            assert!(installed.layout.world.is_file(), "no world was copied");
            let manifest = std::fs::read_to_string(&installed.layout.manifest).expect("a manifest");
            let read = nc_channel::Install::parse(&manifest).expect("it reads");
            assert_eq!(
                read.log_endpoint.as_deref(),
                Some(crate::testing::NOBODY_LISTENS)
            );
        }
        let root = one.root().to_path_buf();
        drop(one);
        assert!(!root.exists(), "an install's scratch outlived it");
        two.without_a_central_log();
        let manifest = std::fs::read_to_string(&two.layout.manifest).expect("a manifest");
        assert_eq!(
            nc_channel::Install::parse(&manifest)
                .expect("it reads")
                .log_endpoint,
            None
        );
    }
}
