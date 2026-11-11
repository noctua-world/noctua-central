//! A daemon's run, from its install to its stop (`NOCTUA.md §11.1`, `§17.1`).
//!
//! # The start, in order
//!
//! 1. **The install** beside the canonicalized executable — and, for a start a
//!    restorer asked for (`--restored`, [`restore`]), **the `stopped` mark**:
//!    while the profile carries it the daemon does not start, and ends well.
//! 2. **The profile**, its directories made private or judged, and **the
//!    instance lock** in its state directory, taken at once or not at all:
//!    one daemon keeps one profile, and a second refuses (`NC-DMN-0008`). The
//!    lock is the state directory's, never the data directory's, which each
//!    open of the local database holds for as long as it opens.
//! 3. **The device and the world**, read and vouched for, **the central log**
//!    the install names — its standard error alone when it names none — and
//!    the session it was started in left where it can be
//!    (`nc_platform::leave_the_session`), only once every refusal a start
//!    reads its profile for is behind it. A daemon that does not lead its
//!    process group starts a session of its own, so no terminal it was started
//!    from signals it; one that leads its group — started by hand as a job of a
//!    shell, or spawned as a group's leader — cannot, and stays in the session
//!    it was started in, where that terminal's hangup reaches it.
//! 4. **The PID record** — the process as the system says it is, and the
//!    instance drawn now — **a token drawn fresh**, each written whole and
//!    private in the runtime directory, and **the endpoint bound**. From here
//!    the status is answered.
//! 5. **The local database**, and the newest unit it keeps; then
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
//! A stop asked through the endpoint refuses every new dispatch, begins no
//! pump and no sweep, and waits `stop_drain_ms` at most for the dispatches
//! running, for a pump or a sweep it finds inside a run, and for every
//! connection to write its last answer — the stop's own among them — and
//! close, giving up on — and counting — the dispatches and runs that still run
//! then; then **settles the mark** to the exact high-water mark, so a clean
//! restart has nothing to wait for, closes the local database and removes the
//! PID record. **Until then the endpoint answers**, each connection told the
//! daemon is stopping, so a stop asked again — a restart's, a reinstall's —
//! waits for it as the first did and signals nothing; then the endpoint is let
//! go — the socket with the listener — the token removed, and the instance lock
//! let go. A stop asked while the start is under way waits for the connections
//! alike.
//! There is no signal handler: a daemon killed leaves what a crash leaves,
//! which the next start reads past.
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
use crate::{Refused, device, mark, pidfile, plugins, profile};

/// How a run that stopped when it was asked ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stopped {
    /// How long it ran.
    pub ran: Duration,
    /// How many dispatches, pumps and sweeps were still running when
    /// `stop_drain_ms` ran out.
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
    serving(&nc_channel::found::beside(executable).map_err(Refused::Install)?)
}

/// How a start a restorer asked for ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restored {
    /// The profile carries the `stopped` mark, so the daemon stays down.
    StaysDown,
    /// Another daemon of the channel holds its profile, so it is up already.
    AlreadyUp,
    /// The daemon ran, and stopped when it was asked to.
    Ran(Stopped),
}

/// Runs the daemon of the install beside `executable` as every restorer
/// starts it, `--restored`: not at all while its profile carries the
/// `stopped` mark, and not twice.
///
/// **The mark is read before anything else of the profile**, so a person's
/// explicit stop holds whichever restorer starts the daemon next — a service
/// manager, a logon task, the interface's countdown — and no restorer has to
/// know of it (`NOCTUA.md §17.5`). **A daemon already up is not a failure**:
/// a restorer that starts one beside it leaves it as it is and ends well.
///
/// # Errors
///
/// [`Refused`], as [`serve`] refuses, and [`Refused::NotPrivate`] for a state
/// directory or a mark another account can reach — never one of them read as
/// a stop asked for.
pub fn restore(executable: &Path) -> Result<Restored, Refused> {
    let found = nc_channel::found::beside(executable).map_err(Refused::Install)?;
    if stopped_on_purpose(&found.layout, &found.user)? {
        return Ok(Restored::StaysDown);
    }
    match serving(&found) {
        Ok(stopped) => Ok(Restored::Ran(stopped)),
        Err(Refused::Held { .. }) => Ok(Restored::AlreadyUp),
        Err(refused) => Err(refused),
    }
}

/// Whether the profile of `layout` carries the `stopped` mark: its state
/// directory and the mark judged this user's alone, and no mark where either
/// is not there yet.
///
/// # Errors
///
/// [`Refused::NotPrivate`] for a state directory or a mark another account
/// can reach, and [`Refused::Unavailable`] when either could not be read.
pub fn stopped_on_purpose(
    layout: &nc_channel::Layout,
    user: &nc_platform::User,
) -> Result<bool, Refused> {
    for (path, kind) in [
        (&layout.state, nc_platform::Kind::Directory),
        (&layout.stopped, nc_platform::Kind::File),
    ] {
        match nc_platform::judge(path, kind, user) {
            Ok(()) => {}
            Err(nc_platform::Judged::Failed(absent))
                if absent.error().kind() == std::io::ErrorKind::NotFound =>
            {
                return Ok(false);
            }
            Err(judged) => return Err(Refused::judged(judged)),
        }
    }
    Ok(true)
}

/// Runs the daemon of `found`'s install until it is asked to stop.
fn serving(found: &nc_channel::found::Found) -> Result<Stopped, Refused> {
    let began = std::time::Instant::now();
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
        found,
        held: &held,
        instance,
        device,
        world,
        pdp,
        log: Arc::clone(&log),
    };
    let ran = runtime.block_on(run(started, Arc::clone(&server)));
    // A blocking task still waiting for the clock is not waited for past the
    // drain: the run is over.
    runtime.shutdown_timeout(Duration::from_millis(nc_catalog::DAEMON.stop_drain_ms));
    // A stop removed both already; a start refused once they were written
    // did not. The lock is still held, so neither is another start's.
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
    found: &'a nc_channel::found::Found,
    held: &'a nc_platform::Held,
    /// The instance the start drew, which its PID record names.
    instance: [u8; 16],
    device: device::Device,
    world: nc_authz::LocalWorld,
    pdp: nc_authz::Pdp,
    log: environment::Sink,
}

/// The run on the runtime: the endpoint bound and served, the kernel opened
/// beside it, and the stop; answers how many dispatches, pumps and sweeps the
/// stop gave up on.
async fn run(started: Started<'_>, server: Arc<Server>) -> Result<u32, Refused> {
    let (found, instance) = (started.found, started.instance);
    let layout = &found.layout;
    let endpoint = nc_platform::ipc::Endpoint::new(
        layout.runtime.clone(),
        layout.socket.clone(),
        layout.pipe.clone(),
    );
    let listener =
        nc_ipc::listen(&endpoint, &found.user, started.held).map_err(Refused::Endpoint)?;
    let accepting = tokio::spawn(Arc::clone(&server).accepting(listener));
    let opened = tokio::select! {
        opened = opened(started, &server) => opened,
        () = server.stopping() => Ok(None),
    };
    let (kernel, store, minter) = match opened {
        Ok(Some(opened)) => opened,
        Ok(None) => {
            let deadline = the_drain_from_now();
            let abandoned = server.wound_down(Vec::new(), deadline).await;
            ended(found, instance, &server, accepting, deadline).await;
            return Ok(abandoned);
        }
        Err(refused) => {
            accepting.abort();
            return Err(refused);
        }
    };
    let pumping = tokio::spawn({
        let (kernel, done, log) = (Arc::clone(&kernel), server.schedule(), server.log());
        let server = Arc::clone(&server);
        async move { crate::schedule::pumping(kernel, done, log, server.stopping()).await }
    });
    let sweeping = tokio::spawn({
        let (kernel, done) = (Arc::clone(&kernel), server.schedule());
        let server = Arc::clone(&server);
        async move { crate::schedule::sweeping(kernel, done, server.stopping()).await }
    });
    server.stopping().await;
    // The dispatches running, the pump and the sweep each inside a run or
    // between two, and every connection's last answer, waited for within one
    // bound: a run is call trees as a dispatch is, and the store closes under
    // none the stop did not give up on.
    let deadline = the_drain_from_now();
    let abandoned = server.wound_down(vec![pumping, sweeping], deadline).await;
    // The exact high-water mark, so a clean restart waits for nothing. A
    // settle that failed leaves the ceiling the last raise wrote, which no
    // identifier was handed out past.
    let _ = mark::settled(&minter);
    drop(kernel);
    store.close().await;
    ended(found, instance, &server, accepting, deadline).await;
    Ok(abandoned)
}

/// The end of a stopped run at its endpoint, in the order a stop asked again
/// reads it.
///
/// **The PID record removed while the endpoint still answers**, so a stop asked
/// before the endpoint goes is answered that the daemon is stopping, and one
/// asked after finds no record naming it, and neither signals it; then the
/// endpoint let go, `deadline` at most, and the token beside it removed.
async fn ended(
    found: &nc_channel::found::Found,
    instance: [u8; 16],
    server: &Server,
    accepting: tokio::task::JoinHandle<()>,
    deadline: tokio::time::Instant,
) {
    let _ = pidfile::remove_if_ours(&found.layout.pid_file, instance, &found.user);
    server.let_go(accepting, deadline).await;
    let _ = std::fs::remove_file(&found.layout.token);
}

/// When a drain that begins now ends: `stop_drain_ms` from now.
fn the_drain_from_now() -> tokio::time::Instant {
    tokio::time::Instant::now() + Duration::from_millis(nc_catalog::DAEMON.stop_drain_ms)
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
    /// **Held while the central log named is one a start refuses**, so a
    /// start that read past the lock would refuse at the log, before it left
    /// the session this suite runs in or bound anything. No start here is let
    /// past that fence: one that got through would be a daemon in this suite's
    /// own process.
    #[test]
    fn a_start_refuses_outside_an_install_and_on_a_profile_another_holds() {
        let installed =
            crate::testing::Installed::new(&std::env::current_exe().expect("this test binary"));
        let stand_in = installed.layout.data.join("noctua-daemon");
        nc_platform::create_private(&stand_in).expect("a binary with no manifest beside it");
        let refused = super::serve(&stand_in).expect_err("no install there");
        assert_eq!(refused.code(), ErrorCode::ChannelNoManifest, "{refused}");
        installed.with_a_central_log_refused();
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

    /// A start a restorer asks for ends well at once while the profile
    /// carries the `stopped` mark, and beside a daemon already up; a mark
    /// another account can reach is refused rather than read as a stop; and
    /// with neither it is a start like any other.
    ///
    /// **Held behind a central log a start refuses**, as the test above is,
    /// so no start here gets past that fence into this suite's own process.
    #[test]
    fn a_restored_start_stays_down_on_the_mark_and_beside_a_daemon_up() {
        let installed =
            crate::testing::Installed::new(&std::env::current_exe().expect("this test binary"));
        installed.with_a_central_log_refused();
        let layout = &installed.layout;
        let refused = super::restore(&installed.executable).expect_err("a start, fenced");
        assert_eq!(refused.code(), ErrorCode::DaemonNoCentralLog, "{refused}");
        drop(nc_platform::create_private(&layout.stopped).expect("the mark"));
        assert!(matches!(
            super::restore(&installed.executable),
            Ok(super::Restored::StaysDown)
        ));
        assert!(
            !layout.pid_file.exists() && !layout.token.exists(),
            "a start that stayed down wrote its run"
        );
        nc_platform::testing::open_to_another_user(&layout.stopped).expect("opened");
        let refused = super::restore(&installed.executable).expect_err("a mark not private");
        assert_eq!(
            refused.code(),
            ErrorCode::DaemonProfileNotPrivate,
            "{refused}"
        );
        assert!(
            layout.stopped.is_file(),
            "a refused mark was not left as it was"
        );
        nc_platform::testing::make_private(&layout.stopped).expect("private again");
        std::fs::remove_file(&layout.stopped).expect("the mark is removed");
        let lock = layout
            .instance_lock
            .file_name()
            .expect("a name")
            .to_string_lossy()
            .into_owned();
        let held = nc_platform::try_hold(&layout.state, &lock, &installed.user)
            .expect("asked")
            .expect("nothing held it");
        assert!(matches!(
            super::restore(&installed.executable),
            Ok(super::Restored::AlreadyUp)
        ));
        drop(held);
    }

    /// What tells a child of this test binary to serve an install.
    ///
    /// The install beside the path it holds, as the daemon's binary serves the
    /// one beside itself.
    const SERVES: &str = "NC_DAEMON_RUN_SERVES";

    /// A clock that never moves: it reads the one millisecond it was made at,
    /// whenever it is asked.
    struct At(u64);

    /// Read as a minter reads any clock, its wait returning at once, which is
    /// all a minter that mints once asks.
    impl nc_ids::Clock for At {
        /// The millisecond it was made at.
        fn now_ms(&self) -> u64 {
            self.0
        }

        /// Nothing: it never moves.
        fn wait_past(&self, _millis: u64) {}
    }

    /// A unit `ahead_ms` ahead of the clock, committed in the store of the
    /// profile of `layout` alone, as a reversal's is: no journal entry names it.
    async fn committed_ahead_by(layout: &nc_channel::Layout, ahead_ms: u64) -> nc_ids::Snowflake {
        let (store, _) = crate::environment::store(layout).await.expect("opened");
        let at = crate::server::now_ms() + ahead_ms;
        let flake = nc_ids::MachineFlake::from_value(3).expect("a flake");
        let unit = nc_ids::Minter::new(flake, At(at)).mint().expect("a unit");
        let record = nc_local::UnitRecord {
            unit,
            meta: nc_local::UnitMeta {
                realm: nc_ids::OpaqueId::of_device_key(&[7; 32]),
                principal: nc_ids::OpaqueId::of_device_key(&[8; 32]),
                kind: nc_local::UnitKind::Undo { target: unit },
                label: "a reversal".to_owned(),
                committed_at_ms: at,
            },
            writes: Vec::new(),
            blobs: Vec::new(),
            notes: Vec::new(),
            unpinned: Vec::new(),
            marks: Vec::new(),
        };
        let quotas = nc_local::Quotas {
            entries_per_plugin: 1,
            lowered: std::collections::BTreeMap::new(),
            blobs: 1,
            writes_per_unit: 1,
            notes_per_unit: 1,
        };
        store.commit(&record, &quotas).await.expect("committed");
        store.close().await;
        unit
    }

    /// `request`, answered on `client` within the suites' patience, or the test
    /// fails.
    async fn asked(client: &mut nc_ipc::Client, request: nc_ipc::Request) -> nc_ipc::Answer {
        tokio::time::timeout(crate::testing::PATIENCE, client.ask(&request))
            .await
            .expect("answered in time")
            .expect("answered")
    }

    /// A run resumes above the newest unit its profile keeps, and settles its mark.
    ///
    /// The daemon's own run, in a child of this test binary told to serve a
    /// scratch install whose store holds a unit further ahead of the clock than
    /// the mark's lease and whose mark is not there: the start waits for the
    /// clock and logs that it moved back, the unit a dispatch then opens is
    /// above the one kept, a stop asked again while the first drains is
    /// answered that the daemon is stopping, and the daemon ends well and
    /// leaves the mark at exactly that unit's millisecond. **Found by a
    /// review**: nothing held the run's own wiring of the mark — the newest
    /// unit handed to the minter, the sink it logs to, the minter the kernel
    /// shares and the settle at a stop — and a stop asked again found nothing
    /// answering once a stop began, and signalled the daemon.
    #[test]
    fn a_run_resumes_above_the_newest_unit_kept_and_a_clean_stop_settles_its_mark() {
        if let Some(executable) = std::env::var_os(SERVES) {
            super::serve(std::path::Path::new(&executable)).expect("the daemon ran");
            return;
        }
        let this = std::env::current_exe().expect("this test binary");
        let installed = crate::testing::Installed::new(&this);
        let layout = &installed.layout;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        let kept = runtime.block_on(committed_ahead_by(
            layout,
            nc_catalog::DAEMON.mark_lease_ms + 1_500,
        ));
        assert!(!layout.mark.exists(), "an install made a mark");
        let mut daemon = installed.spawn_as(
            &this,
            &[
                "--exact",
                "run::tests::a_run_resumes_above_the_newest_unit_kept_and_a_clean_stop_settles_its_mark",
                "--nocapture",
                "--test-threads=1",
            ],
            &[(SERVES, installed.executable.as_os_str())],
        );
        runtime.block_on(async {
            let (mut client, _) = daemon.connected().await;
            let running = format!("\"state\":\"{}\"", nc_ipc::State::Running.word());
            let deadline = tokio::time::Instant::now() + crate::testing::PATIENCE;
            loop {
                match asked(&mut client, nc_ipc::Request::Status).await {
                    nc_ipc::Answer::Status(status) if status.to_canonical().contains(&running) => {
                        break;
                    }
                    _ => assert!(tokio::time::Instant::now() < deadline, "it never ran"),
                }
                tokio::time::sleep(core::time::Duration::from_millis(20)).await;
            }
            let dispatch = nc_ipc::Request::Dispatch {
                command: "kernel.undo".to_owned(),
                args: nc_canon::Value::object(Vec::<(&str, nc_canon::Value)>::new()),
            };
            drop(asked(&mut client, dispatch).await);
            // A connection that has asked nothing yet, whose first request the
            // drain waits to answer: what holds the stop open below.
            let (mut holding, _) = daemon.connected().await;
            assert_eq!(
                asked(&mut client, nc_ipc::Request::Stop).await,
                nc_ipc::Answer::Stopping
            );
            let (mut again, welcome) = daemon.connected().await;
            assert_eq!(
                (
                    welcome.state,
                    asked(&mut again, nc_ipc::Request::Stop).await
                ),
                (nc_ipc::State::Stopping, nc_ipc::Answer::Stopping),
                "a stop asked again while the daemon drained was not answered that it is \
                 stopping"
            );
            match asked(&mut holding, nc_ipc::Request::Status).await {
                nc_ipc::Answer::Status(status) => assert!(
                    status.to_canonical().contains("\"state\":\"stopping\""),
                    "{status:?}"
                ),
                other => panic!("the held connection was answered {other:?}"),
            }
            drop((holding, again));
            assert_eq!(daemon.ended().await, Some(0), "the daemon did not end well");
        });
        let said = installed.standard_error();
        let backwards = said
            .lines()
            .filter(|line| line.contains("\"_msg\":\"ids.clock_backwards\""))
            .count();
        assert_eq!(
            backwards, 1,
            "the wait for the clock was not logged once: {said}"
        );
        let minted = runtime.block_on(async {
            let (store, _) = crate::environment::store(layout).await.expect("opened");
            let minted = store.journal().newest_unit().await.expect("asked");
            store.close().await;
            minted.expect("the dispatch opened a unit")
        });
        assert!(
            minted > kept,
            "a unit was minted at or below one the profile kept"
        );
        let (_, read) =
            crate::mark::MarkFile::open(&layout.mark, &installed.user).expect("the mark reads");
        assert_eq!(
            read,
            Some(minted.millis_since_epoch() + nc_ids::EPOCH_MS),
            "a clean stop did not settle the mark at the last unit minted"
        );
    }

    /// Two installs a suite makes never share a namespace or a root, and
    /// no other process's install spells one; each is initialized as the
    /// installer initializes one and names a central log nothing answers at,
    /// is rewritten to name none or one a start refuses, and is gone with its
    /// guard — the ground every suite that starts a daemon stands on
    /// (`crate::testing`).
    #[test]
    fn an_install_a_suite_makes_is_its_own_and_gone_with_its_guard() {
        let binary = std::env::current_exe().expect("this test binary");
        let one = crate::testing::Installed::new(&binary);
        let two = crate::testing::Installed::new(&binary);
        assert_ne!(one.install.namespace, two.install.namespace);
        assert_ne!(one.layout.state, two.layout.state);
        let mut made = Vec::new();
        for installed in [&one, &two] {
            let namespace = installed
                .install
                .namespace
                .as_ref()
                .expect("a suite's install is relocated")
                .as_str()
                .to_owned();
            let (pid, count) = namespace
                .strip_prefix('t')
                .and_then(|rest| rest.split_once('n'))
                .unwrap_or_else(|| panic!("`{namespace}` holds no process and no count"));
            assert_eq!(u32::from_str_radix(pid, 16).ok(), Some(std::process::id()));
            made.push(count.parse::<u32>().expect("a count"));
        }
        assert_ne!(made[0], made[1]);
        assert_ne!(
            crate::testing::namespace_of(0x1a, 10),
            crate::testing::namespace_of(0x1a1, 0),
            "two processes' installs spell one namespace"
        );
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
        let named = || {
            let manifest = std::fs::read_to_string(&two.layout.manifest).expect("a manifest");
            nc_channel::Install::parse(&manifest)
                .expect("it reads")
                .log_endpoint
        };
        two.without_a_central_log();
        assert_eq!(named(), None);
        two.with_a_central_log_refused();
        assert_eq!(named().as_deref(), Some(crate::testing::OFF_THIS_MACHINE));
    }
}
