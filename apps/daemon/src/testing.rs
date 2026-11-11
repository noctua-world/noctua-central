//! An install a suite runs a real daemon in, under a scratch root of its own:
//! **for tests**, behind `test-support`.
//!
//! **Never a real install's names.** Every install made here is relocated by a
//! namespace drawn from this process and a count, the two kept apart so no
//! other process's install spells it, under a scratch root the guard removes,
//! so its directories, its socket and its pipe — and every other name the
//! system sees — are its own and no developer's (`AGENTS.md`, the names'
//! security answer). Its manifest names a central log on this machine that
//! nothing answers at, so the daemon's lines reach its standard error, which
//! is a file in the profile's log directory.
//!
//! **The daemon holds none of its spawner's handles**: it is started with
//! its standard input and output null and its standard error that file, after
//! this process's own standard handles are kept from every child
//! (`nc_platform::process::keep_standard_handles_from_children`), so a daemon
//! a suite leaves behind never holds the output the suite's runner reads.
//!
//! **Every wait is bounded**, so a daemon that never answers fails the suite
//! rather than hanging it, and the guards stop what a suite started — the
//! daemon killed and waited for, the scratch removed — however it leaves.

#![cfg(any(test, feature = "test-support"))]

use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::{AtomicU32, Ordering};

use core::time::Duration;

/// How long a suite waits for a daemon to answer, to stop or to end.
pub const PATIENCE: Duration = Duration::from_secs(120);

/// The central log every scratch install names: on this machine, at port
/// zero, which nothing answers at — so every line goes to standard error.
pub const NOBODY_LISTENS: &str = "http://127.0.0.1:0/insert/jsonline";

/// A central log off this machine, which `nc_log` refuses and so a start
/// refuses: the fence a test of a start's refusals puts after the one it is
/// about.
pub const OFF_THIS_MACHINE: &str = "http://example.com:9428/insert/jsonline";

/// How many installs this process has made, so two never share a namespace.
static MADE: AtomicU32 = AtomicU32::new(0);

/// A scratch root of its own, removed however the suite leaves.
#[derive(Debug)]
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = nc_platform::testing::make_private(&self.0);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Scratch {
    /// A private directory under the temporary one, named for `namespace`.
    ///
    /// **Short**, because a socket's path is bounded at 103 bytes on macOS.
    fn new(namespace: &str) -> Self {
        let root = std::env::temp_dir().join(format!("nc-dmn-{namespace}"));
        let _ = nc_platform::testing::make_private(&root);
        let _ = std::fs::remove_dir_all(&root);
        nc_platform::create_private_directory(&root).expect("a scratch root");
        Self(root)
    }
}

/// A daemon's install under a scratch root: its binary copied where an
/// installer puts one, its manifest, its device and its world.
#[derive(Debug)]
pub struct Installed {
    /// The scratch root, which goes with the install.
    root: Scratch,
    /// What the manifest says.
    pub install: nc_channel::Install,
    /// The user the install runs as.
    pub user: nc_platform::User,
    /// Every name the install gives the system.
    pub layout: nc_channel::Layout,
    /// The daemon's executable, beside the manifest.
    pub executable: PathBuf,
}

/// The grant a suite's world names for `ana` in `home`.
///
/// **`g.by_policy`, the membership the pack confers, and not the kernel
/// suites' `g.current`**: those suites decide on a clock standing at
/// 2026-08-26, inside `g.current`'s window, and a daemon decides on the
/// system's, which leaves that window on 2027-01-01. This grant has begun and
/// has no end, which the core allows a grant the pack confers alone.
pub const GRANT: &str = "g.by_policy";

/// The epoch a suite's world gives `home`: the one the kernel's suites give it.
pub const EPOCH: i64 = 7;

/// A world file as an installer writes one.
///
/// It names the realm `home`, the person `ana`, the grant she holds there
/// ([`GRANT`]) and the realm's epoch ([`EPOCH`]), and holds the entities every
/// policy case is about.
#[must_use]
pub fn a_world() -> String {
    let mut fields = std::collections::BTreeMap::new();
    fields.insert("realm".to_owned(), nc_canon::Value::text("home"));
    fields.insert("person".to_owned(), nc_canon::Value::text("ana"));
    fields.insert("grant".to_owned(), nc_canon::Value::text(GRANT));
    fields.insert("epoch".to_owned(), nc_canon::Value::Integer(EPOCH));
    fields.insert(
        "world".to_owned(),
        nc_canon::parse(include_str!("../../../policy/world.json"))
            .unwrap_or(nc_canon::Value::Null),
    );
    nc_canon::Value::Object(fields).to_canonical()
}

impl Installed {
    /// A staging install of `binary` under a scratch root of its own,
    /// initialized as the installer initializes one.
    ///
    /// # Panics
    ///
    /// When the scratch, the copy, the manifest or the profile could not be
    /// made: a suite cannot run without them.
    #[must_use]
    pub fn new(binary: &Path) -> Self {
        install_at(binary)
    }

    /// Rewrites the install's manifest to name no central log, so the
    /// daemon writes its lines to its standard error alone.
    ///
    /// # Panics
    ///
    /// When the manifest could not be rewritten.
    pub fn without_a_central_log(&self) {
        self.rewritten(None);
    }

    /// Rewrites the install's manifest to name a central log off this
    /// machine, [`OFF_THIS_MACHINE`], which a start refuses: a fence a test of
    /// a start's refusals puts after the one it is about.
    ///
    /// # Panics
    ///
    /// When the manifest could not be rewritten.
    pub fn with_a_central_log_refused(&self) {
        self.rewritten(Some(OFF_THIS_MACHINE));
    }

    /// The install's manifest, written again naming `log_endpoint`.
    fn rewritten(&self, log_endpoint: Option<&str>) {
        let mut install = self.install.clone();
        install.log_endpoint = log_endpoint.map(str::to_owned);
        std::fs::remove_file(&self.layout.manifest).expect("the manifest is removed");
        written(&self.layout.manifest, &install.to_canonical());
    }

    /// Installs the plugin `manifest` describes, with `component`'s bytes, as
    /// `name` in the profile's plugins directory.
    ///
    /// # Panics
    ///
    /// When its directory or a file of it could not be made.
    pub fn install_plugin(&self, name: &str, manifest: &str, component: &[u8]) {
        let directory = self.layout.plugins.join(name);
        nc_platform::create_private_directory(&directory).expect("a plugin's directory");
        written(&directory.join(crate::plugins::MANIFEST), manifest);
        let mut file = nc_platform::create_private(&directory.join(crate::plugins::COMPONENT))
            .expect("a component");
        std::io::Write::write_all(&mut file, component).expect("written");
    }

    /// The daemon of this install, started as its spawner starts it.
    ///
    /// # Panics
    ///
    /// When it could not be started.
    #[must_use]
    pub fn spawn(&self) -> Running {
        Running {
            child: start(&self.executable, &self.layout.daemon_err),
            installed: Reached {
                layout: self.layout.clone(),
                user: self.user.clone(),
            },
        }
    }

    /// `program` started with `arguments` and `environment`, as its spawner
    /// starts the daemon of this install: a child of a test binary told to
    /// serve it, which a unit test the mutation runner judges starts.
    ///
    /// # Panics
    ///
    /// When it could not be started.
    #[must_use]
    pub fn spawn_as(
        &self,
        program: &Path,
        arguments: &[&str],
        environment: &[(&str, &std::ffi::OsStr)],
    ) -> Running {
        Running {
            child: start_with(program, arguments, environment, &self.layout.daemon_err),
            installed: Reached {
                layout: self.layout.clone(),
                user: self.user.clone(),
            },
        }
    }

    /// The scratch root the install is under, removed with it.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root.0
    }

    /// What the daemon wrote to its standard error, every run of it.
    #[must_use]
    pub fn standard_error(&self) -> String {
        std::fs::read_to_string(&self.layout.daemon_err).unwrap_or_default()
    }

    /// The daemon `pid` of this install, reached through its endpoint, once
    /// its record names it: for a daemon some other process started.
    ///
    /// # Panics
    ///
    /// When it does not answer within [`PATIENCE`].
    pub async fn reach(&self, pid: u32) -> (nc_ipc::Client, nc_ipc::Welcome) {
        Reached {
            layout: self.layout.clone(),
            user: self.user.clone(),
        }
        .connected(pid, &mut || {})
        .await
    }
}

/// A staging install of `binary` under a scratch root of its own, initialized
/// as the installer initializes one: what [`Installed::new`] hands out.
fn install_at(binary: &Path) -> Installed {
    let made = MADE.fetch_add(1, Ordering::SeqCst);
    let namespace = namespace_of(std::process::id(), made);
    let root = Scratch::new(&namespace);
    let base = |name: &str| {
        let base = root.0.join(name);
        nc_platform::create_private_directory(&base).expect("a base");
        base
    };
    let install = nc_channel::Install {
        channel: nc_channel::Channel::Staging,
        bases: nc_channel::Bases::new(base("d"), base("c"), base("k"), base("s"))
            .expect("the scratch's bases are plain absolute paths"),
        namespace: Some(nc_channel::Namespace::parse(&namespace).expect("a namespace")),
        log_endpoint: Some(NOBODY_LISTENS.to_owned()),
    };
    let user = nc_platform::user_in(&root.0).expect("the scratch is private");
    let layout = nc_channel::layout(&install, &user.to_string());
    nc_platform::create_private_directory(&layout.state).expect("the state directory");
    nc_platform::create_private_directory(&layout.install).expect("the install directory");
    let executable = layout
        .install
        .join(binary.file_name().expect("a binary's name"));
    // Linked where the system allows it — a debug daemon is hundreds of
    // megabytes, and a suite installs one per test — and copied where it
    // does not, across volumes; either way a file of the install's own.
    if std::fs::hard_link(binary, &executable).is_err() {
        std::fs::copy(binary, &executable)
            .expect("the binary is copied where an installer puts it");
    }
    written(&layout.manifest, &install.to_canonical());
    let world = root.0.join("world.json");
    written(&world, &a_world());
    crate::initialize(&executable, Some(&world)).expect("the profile is initialized");
    Installed {
        root,
        install,
        user,
        layout,
        executable,
    }
}

/// The namespace of the install the process `pid` made `made`-th.
///
/// **The two numbers kept apart by a letter no hexadecimal digit is**: run
/// together, the process `0x1a`'s eleventh install and the process `0x1a1`'s
/// first are one namespace, and the second scratch root made removes the
/// first's while its daemon runs.
pub(crate) fn namespace_of(pid: u32, made: u32) -> String {
    format!("t{pid:x}n{made}")
}

/// `text`, written to a new private file at `path`.
fn written(path: &Path, text: &str) {
    let mut file = nc_platform::create_private(path).expect("a private file");
    std::io::Write::write_all(&mut file, text.as_bytes()).expect("written");
}

/// `executable` started with no argument, its standard input and output null
/// and its standard error appended to `standard_error`, holding none of this
/// process's handles: the daemon as every suite starts it.
///
/// # Panics
///
/// When it could not be started.
#[must_use]
pub fn start(executable: &Path, standard_error: &Path) -> Child {
    start_with(executable, &[], &[], standard_error)
}

/// `program` started with `arguments` and `environment`, as [`start`] starts
/// the daemon's binary.
#[expect(
    clippy::disallowed_methods,
    reason = "the daemon's binary started as its spawner starts it: its standard handles \
              null or a file in its profile, never this process's"
)]
fn start_with(
    program: &Path,
    arguments: &[&str],
    environment: &[(&str, &std::ffi::OsStr)],
    standard_error: &Path,
) -> Child {
    nc_platform::process::keep_standard_handles_from_children()
        .expect("this process's standard handles are kept from its children");
    let error = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(standard_error)
        .expect("the daemon's standard error");
    std::process::Command::new(program)
        .args(arguments)
        .envs(environment.iter().copied())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(error)
        .spawn()
        .expect("the daemon starts")
}

/// A daemon a suite started, killed and waited for however the suite leaves.
#[derive(Debug)]
pub struct Running {
    child: Child,
    installed: Reached,
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Running {
    /// The daemon's process id.
    #[must_use]
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// The daemon's endpoint.
    #[must_use]
    pub fn endpoint(&self) -> nc_platform::ipc::Endpoint {
        self.installed.endpoint()
    }

    /// The user the daemon runs as.
    #[must_use]
    pub fn user(&self) -> nc_platform::User {
        self.installed.user.clone()
    }

    /// A connection to the daemon, through its endpoint and its handshake.
    ///
    /// # Panics
    ///
    /// When it does not answer within [`PATIENCE`], or ends first.
    pub async fn connected(&mut self) -> (nc_ipc::Client, nc_ipc::Welcome) {
        let pid = self.child.id();
        let child = &mut self.child;
        self.installed
            .connected(pid, &mut || {
                let ended = child.try_wait().expect("the daemon is asked");
                assert!(
                    ended.is_none(),
                    "the daemon ended as {ended:?} before it answered"
                );
            })
            .await
    }

    /// Waits for the daemon to end, [`PATIENCE`] at most, and answers its
    /// exit code.
    ///
    /// # Panics
    ///
    /// When it has not ended by then.
    pub async fn ended(&mut self) -> Option<i32> {
        let deadline = tokio::time::Instant::now() + PATIENCE;
        loop {
            if let Some(status) = self.child.try_wait().expect("the daemon is asked") {
                return status.code();
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "the daemon did not end within {PATIENCE:?}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

/// What a client needs to reach an install's daemon.
#[derive(Debug)]
struct Reached {
    layout: nc_channel::Layout,
    user: nc_platform::User,
}

impl Reached {
    /// The daemon's endpoint.
    fn endpoint(&self) -> nc_platform::ipc::Endpoint {
        nc_platform::ipc::Endpoint::new(
            self.layout.runtime.clone(),
            self.layout.socket.clone(),
            self.layout.pipe.clone(),
        )
    }

    /// The daemon `pid`, once its record names it and the token beside it is
    /// written, through its endpoint and its handshake; `alive` is asked
    /// between tries, and panics when the daemon is gone.
    async fn connected(
        &self,
        pid: u32,
        alive: &mut dyn FnMut(),
    ) -> (nc_ipc::Client, nc_ipc::Welcome) {
        let endpoint = self.endpoint();
        let deadline = tokio::time::Instant::now() + PATIENCE;
        loop {
            // The record is written after the token, so one naming `pid`
            // says the token beside it is that start's.
            let named = crate::pidfile::read(&self.layout.pid_file, &self.user)
                .is_ok_and(|record| record.pid == pid);
            let token = std::fs::read_to_string(&self.layout.token)
                .ok()
                .and_then(|text| nc_ipc::Token::from_text(&text));
            if named
                && let Some(token) = token
                && let Ok((stream, peer)) = nc_platform::ipc::connect(&endpoint, &self.user).await
            {
                let (client, welcome) =
                    tokio::time::timeout(PATIENCE, nc_ipc::connect(stream, &token))
                        .await
                        .expect("the handshake ends")
                        .expect("the daemon proves itself");
                assert_eq!(peer.pid, pid, "the endpoint is another process's");
                return (client, welcome);
            }
            alive();
            assert!(
                tokio::time::Instant::now() < deadline,
                "the daemon did not answer within {PATIENCE:?}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}
