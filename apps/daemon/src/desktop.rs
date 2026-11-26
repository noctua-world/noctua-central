//! The desktop interface the daemon starts, and how it proves it is that one
//! (`NOCTUA.md §11.1`, `§12.10`).
//!
//! **The daemon starts the desktop, detached and hidden** ([`Desktop`]): the
//! executable its install names beside the CLI and the daemon
//! (`nc_channel::Layout::desktop`), with no argument — the desktop starts with
//! its window hidden — its standard output null, its standard error
//! `<log>/desktop.err`, and its standard input a pipe holding a [`Secret`]
//! drawn for that one process, written whole and then closed
//! (`nc_platform::process::spawn_detached`). The daemon keeps the secret and
//! the process it started, and nothing else of it: the secret is never written
//! to disk, logged or sent. A desktop that runs on is not started again; one
//! that ended — killed, or closed — is started again the next time the daemon
//! is asked to ([`Desktop::wanted`]): when its kernel begins to serve, when a
//! person's connection announces a graphical environment, and before a press of
//! the Leader is dispatched, so a person who closed or killed it opens it again
//! by the Leader, and never through a loop of starts.
//!
//! **Where a window could be shown, and nowhere else** ([`environment_for`]):
//! on macOS and Windows the daemon runs inside the person's session, so the
//! desktop is started as the daemon is; on Linux the daemon is a user service
//! that may run with no display in its environment (§12.8), so the desktop is
//! started with the graphical environment a person's connection announced last
//! set over the daemon's own — or with the daemon's own where it names a
//! display — and not at all where neither names one: it is started once one is
//! announced.
//!
//! **A connection attaches as that desktop by proving both halves**
//! ([`Desktop::attaches`]): the secret, proven over the connection's own
//! handshake ([`nc_ipc::Secret::proves`]), so a proof is good on the connection
//! it was made for alone; and the process at the other end, as the system says
//! it, which must be the one the daemon started. A process that only speaks the
//! protocol has neither; one the daemon did not start has no secret; and one
//! that read the secret somewhere is not the process it was handed to. The
//! secret alone was not enough, nor the process alone: the executable's path
//! is the user's to replace, and a process of the user's can start the real
//! binary as its own child.
//!
//! # What this does not answer
//!
//! **A process running as the same user that tampers with the desktop
//! itself** (`AGENTS.md`, What is deliberately not claimed): it can read the
//! secret out of the desktop's memory or drive it — attach a debugger to it,
//! inject a library, send it synthetic input — replace its executable or the
//! service's definition before the daemon starts it, or announce a display of
//! its own. On a desktop operating system that is not a boundary that exists
//! between two processes of one user (`NOCTUA.md §2.4`); what the daemon holds
//! the desktop to keeps every process that only speaks the protocol — the CLI,
//! an MCP client, an agent, a plugin, a dispatched command — from confirming
//! anything.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use nc_ipc::{Drawn, Graphical, Secret};
use nc_platform::clipboard::System;

/// The desktop interface a daemon starts, and the one it started last.
#[derive(Debug)]
pub struct Desktop {
    /// Its executable.
    executable: PathBuf,
    /// What it is started with.
    arguments: Vec<String>,
    /// Where its standard error goes.
    standard_error: PathBuf,
    /// The one started last, while it may still run.
    started: Mutex<Option<Started>>,
}

/// One desktop the daemon started: its process, and the secret it was handed.
#[derive(Debug)]
struct Started {
    /// The process, reaped once it ends or is let go.
    process: nc_platform::process::Detached,
    /// The secret it was handed down its standard input.
    secret: Secret,
}

/// What came of asking for a desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wanted {
    /// One the daemon started runs, as this process.
    Running(u32),
    /// One was started now, as this process.
    Started(u32),
    /// None runs, and no window could be shown where the daemon is: nothing
    /// names a display.
    NoDisplay,
    /// None runs, and it could not be started — its executable is not
    /// installed, or the system refused.
    NotStarted,
}

impl Desktop {
    /// The desktop of the install `layout` lays out: its executable beside
    /// the daemon's, under the name the layout gives it, started with no
    /// argument.
    #[must_use]
    pub fn of(layout: &nc_channel::Layout) -> Self {
        Self::at(
            layout.install.join(format!(
                "{}{}",
                layout.desktop,
                nc_platform::EXECUTABLE_SUFFIX
            )),
            Vec::new(),
            layout.desktop_err.clone(),
        )
    }

    /// A desktop started as `executable` with `arguments`, its standard error
    /// `standard_error`. **For tests only**: a suite starts a stand-in of its
    /// own where the install's desktop is not built.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    #[must_use]
    pub fn for_tests(executable: PathBuf, arguments: Vec<String>, standard_error: PathBuf) -> Self {
        Self::at(executable, arguments, standard_error)
    }

    /// The desktop at `executable`.
    const fn at(executable: PathBuf, arguments: Vec<String>, standard_error: PathBuf) -> Self {
        Self {
            executable,
            arguments,
            standard_error,
            started: Mutex::new(None),
        }
    }

    /// A desktop, started where none the daemon started runs and a window
    /// could be shown, in the graphical environment `announced` — the one a
    /// person's connection announced last, if one did — over the daemon's
    /// own, which `own` reads.
    pub fn wanted(
        &self,
        announced: Option<&Graphical>,
        own: &dyn Fn(&str) -> Option<OsString>,
    ) -> Wanted {
        let mut started = self.started.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(running) = started.as_mut()
            && matches!(running.process.ended(), Ok(None))
        {
            return Wanted::Running(running.process.pid());
        }
        *started = None;
        if !self.executable.is_file() {
            return Wanted::NotStarted;
        }
        let Some(environment) = environment_for(nc_platform::clipboard::SYSTEM, announced, own)
        else {
            return Wanted::NoDisplay;
        };
        let Ok(secret) = Secret::fresh() else {
            return Wanted::NotStarted;
        };
        let Ok(standard_error) = self.standard_error() else {
            return Wanted::NotStarted;
        };
        let pairs: Vec<(&str, &str)> = environment
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        let arguments: Vec<&str> = self.arguments.iter().map(String::as_str).collect();
        let handed = nc_platform::process::Handed {
            input: Some(secret.as_bytes()),
            environment: &pairs,
        };
        match nc_platform::process::spawn_detached(
            &self.executable,
            &arguments,
            standard_error,
            handed,
        ) {
            Ok(process) => {
                let pid = process.pid();
                *started = Some(Started { process, secret });
                Wanted::Started(pid)
            }
            Err(_) => Wanted::NotStarted,
        }
    }

    /// The file the desktop's standard error goes to: made new and private
    /// for each desktop started, as the CLI makes the daemon's, in a log
    /// directory made private where it is not there.
    fn standard_error(&self) -> std::io::Result<std::fs::File> {
        if let Some(log) = self.standard_error.parent()
            && !log.is_dir()
        {
            nc_platform::create_private_directory(log)?;
        }
        match std::fs::remove_file(&self.standard_error) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
        nc_platform::create_private(&self.standard_error)
    }

    /// Whether the connection whose other end is the process `peer`, and
    /// whose handshake drew `drawn`, is the desktop the daemon started, by
    /// `proof`: the process the daemon started last, still running, and a
    /// proof of the secret it was handed, made for this connection.
    ///
    /// **Both, or the connection is no desktop's**: the secret proven by
    /// another process, the right process proving another secret, or either
    /// once the desktop ended, attaches nothing.
    pub fn attaches(&self, peer: Option<u32>, proof: &[u8; 32], drawn: &Drawn) -> bool {
        let mut started = self.started.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(running) = started.as_mut() else {
            return false;
        };
        let alive = matches!(running.process.ended(), Ok(None));
        alive && peer == Some(running.process.pid()) && running.secret.proves(proof, drawn)
    }
}

/// The variables a desktop is started with over the daemon's own on `system`,
/// or nothing where no window could be shown.
///
/// **On Linux, a display named**: the environment `announced` when a person's
/// connection announced one — its variables set over the daemon's — or none
/// set where the daemon's own, which `own` reads, names a display; and nothing
/// where neither names one. **On macOS and Windows, none set**: the daemon runs
/// inside the person's session there, and its children with it.
pub fn environment_for(
    system: System,
    announced: Option<&Graphical>,
    own: &dyn Fn(&str) -> Option<OsString>,
) -> Option<Vec<(String, String)>> {
    match system {
        System::MacOs | System::Windows => Some(Vec::new()),
        System::Linux => {
            if let Some(announced) = announced {
                return Some(
                    announced
                        .pairs()
                        .map(|(name, value)| (name.to_owned(), value.to_owned()))
                        .collect(),
                );
            }
            Graphical::DISPLAYS
                .iter()
                .any(|name| own(name).is_some_and(|value| !value.is_empty()))
                .then(Vec::new)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::Path;

    use core::time::Duration;

    use nc_catalog::ErrorCode;
    use nc_ipc::{Answer, Graphical, Request};
    use nc_platform::clipboard::System;

    use super::environment_for;

    /// What tells a child of this test binary to serve an install, with this
    /// test binary as the desktop it starts: the install beside the path it
    /// holds.
    const SERVES: &str = "NC_DAEMON_DESKTOP_SERVES";

    /// The word a child of this test binary started as the desktop finds
    /// among its arguments.
    const STANDS_IN: &str = "nc-daemon-desktop-stand-in";

    /// The test this binary runs as each of its children.
    const THIS_TEST: &str = "desktop::tests::a_desktop_the_daemon_started_attaches_and_answers_what_waits_and_nothing_else_does";

    /// The file the stand-in writes in the install's state directory once it
    /// attached.
    const ATTACHED: &str = "stand-in.attached";

    /// How many held calls the stand-in answers before it ends.
    const ANSWERS: u32 = 4;

    /// A plugin manifest granting the reference plugin what its commands use.
    const GRANTED: &str = "id = \"noctua:reference\"\nnamespace = \"reference\"\ncapabilities = \
                           [\"context\", \"store\", \"blobs.read\", \"blobs.write\", \
                           \"commands.dispatch\", \"commands.begin\", \"events.publish\", \
                           \"ui\", \"timers\"]\n";

    /// The arguments a child of this test binary runs this test alone with.
    fn this_test_alone() -> Vec<String> {
        ["--exact", THIS_TEST, "--nocapture", "--test-threads=1"]
            .map(str::to_owned)
            .to_vec()
    }

    /// The desktop the daemon starts: what it was handed down its standard
    /// input read to its end, a connection to the daemon attached by it, and
    /// each call held for a confirmation answered as it comes — the first and
    /// the third confirmed, the second and the fourth declined — then the end.
    fn stand_in() {
        use std::io::Read as _;
        let mut bytes = Vec::new();
        std::io::stdin()
            .read_to_end(&mut bytes)
            .expect("the secret is read");
        let secret = nc_ipc::Secret::from_bytes(
            <[u8; 32]>::try_from(bytes.as_slice()).expect("32 bytes were handed"),
        );
        let executable = std::env::var_os(SERVES).expect("the install served");
        let found = nc_channel::found::beside(Path::new(&executable)).expect("the install reads");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        runtime.block_on(async {
            let mut connection = nc_ipc::client::connect(&found)
                .await
                .expect("the daemon is reached");
            connection.attach(&secret).await.expect("attached");
            nc_platform::create_private(&found.layout.state.join(ATTACHED))
                .expect("the attach is told");
            let (mut seen, mut answered) = (None, 0);
            let deadline = tokio::time::Instant::now() + crate::testing::PATIENCE;
            while answered < ANSWERS && tokio::time::Instant::now() < deadline {
                let changed = connection
                    .watch(seen, Duration::from_secs(5))
                    .await
                    .expect("watched");
                seen = Some(changed.revision);
                let held = changed
                    .state
                    .and_then(nc_canon::Value::into_object)
                    .and_then(|mut state| state.remove("confirmations"))
                    .and_then(nc_canon::Value::into_array)
                    .and_then(|held| held.into_iter().next());
                if let Some(nc_canon::Value::Integer(serial)) = held {
                    let serial = u64::try_from(serial).expect("a serial");
                    connection
                        .settle(serial, answered % 2 == 0)
                        .await
                        .expect("answered");
                    answered += 1;
                }
            }
        });
    }

    /// Once the stand-in said it attached, within the suites' patience.
    fn attached_within(installed: &crate::testing::Installed) {
        let attached = installed.layout.state.join(ATTACHED);
        let deadline = std::time::Instant::now() + crate::testing::PATIENCE;
        while !attached.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "the desktop never attached: {}",
                std::fs::read_to_string(&installed.layout.desktop_err).unwrap_or_default()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// The answer `request` is given on `client`.
    async fn asked(client: &mut nc_ipc::Client, request: Request) -> Answer {
        client.ask(&request).await.expect("answered")
    }

    /// MCP switched on from the CLI of `found`'s install, in `english`, while
    /// the stand-in answers: the third call held is confirmed and the fourth
    /// declined, and the switch off between them is never held.
    fn mcp_waits_for_the_desktop(found: &nc_channel::found::Found, english: &nc_cli::Env) {
        let words = |words: &[&str]| {
            words
                .iter()
                .map(std::ffi::OsString::from)
                .collect::<Vec<_>>()
        };
        let mcp = || {
            let read = nc_cli::run(&words(&["--json", "settings.mcp.read"]), found, english);
            assert_eq!(read.code(), 0, "{read:?}");
            read.stdout
        };
        let switched_on = nc_cli::run(&words(&["settings.mcp.set", "--on"]), found, english);
        assert_eq!(switched_on.code(), 0, "{switched_on:?}");
        assert!(mcp().contains(r#""on":true"#), "confirmed, MCP is on");
        let switched_off = nc_cli::run(&words(&["settings.mcp.set", "--on=false"]), found, english);
        assert_eq!(switched_off.code(), 0, "{switched_off:?}");
        let refused_on = nc_cli::run(&words(&["settings.mcp.set", "--on"]), found, english);
        assert_eq!(refused_on.code(), 15, "{refused_on:?}");
        assert!(
            refused_on
                .stderr
                .contains(ErrorCode::KernelMcpDeclined.code()),
            "{}",
            refused_on.stderr
        );
        assert!(mcp().contains(r#""on":false"#), "declined, MCP stays off");
    }

    /// A desktop the daemon started attaches, and answers what waits for the
    /// person; nothing else does.
    ///
    /// **Over a daemon a child of this test binary serves**, its desktop
    /// another child — this test binary again, handed its secret down its
    /// standard input. Once an environment naming a display is announced the
    /// daemon starts it, and it attaches by that secret, from that process.
    /// Then an irreversible command from the CLI waits for it and runs once it
    /// confirms, ending in 0; the next it declines ends in 15 with
    /// `NC-KERN-0030`. **MCP switched on from the CLI waits for it the same
    /// way** (`NOCTUA.md §12.15`): confirmed, it ends in 0 and MCP reads on;
    /// switched off, it is never held and ends in 0 at once; switched on
    /// again and declined, it ends in 15 with `NC-KERN-0034` and MCP reads
    /// off. A person's connection that did not attach answering a
    /// held call is refused with `NC-DMN-0018`, and one attaching with a proof
    /// of no secret is refused with `NC-DMN-0017` and closed.
    #[test]
    fn a_desktop_the_daemon_started_attaches_and_answers_what_waits_and_nothing_else_does() {
        if std::env::args().any(|argument| argument == STANDS_IN) {
            stand_in();
            return;
        }
        let this = std::env::current_exe().expect("this test binary");
        if let Some(executable) = std::env::var_os(SERVES) {
            let found = nc_channel::found::beside(Path::new(&executable)).expect("the install");
            let mut arguments = this_test_alone();
            arguments.extend(["--skip".to_owned(), STANDS_IN.to_owned()]);
            let desktop = super::Desktop::for_tests(this, arguments, found.layout.desktop_err);
            crate::run::serve_with(Path::new(&executable), desktop).expect("the daemon ran");
            return;
        }
        let installed = crate::testing::Installed::new(&this);
        installed.without_a_central_log();
        installed.install_plugin(
            "reference",
            GRANTED,
            &crate::testing::entries_of("reference"),
            &nc_kernel::component_for_tests("reference"),
        );
        let alone = this_test_alone();
        let alone: Vec<&str> = alone.iter().map(String::as_str).collect();
        let mut daemon =
            installed.spawn_as(&this, &alone, &[(SERVES, installed.executable.as_os_str())]);
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        let mut person = runtime.block_on(async {
            let (mut person, _) = daemon.connected().await;
            let running = "\"state\":\"running\"";
            let deadline = tokio::time::Instant::now() + crate::testing::PATIENCE;
            loop {
                match asked(&mut person, Request::Status).await {
                    Answer::Status(status) if status.to_canonical().contains(running) => break,
                    _ => assert!(tokio::time::Instant::now() < deadline, "it never ran"),
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            let display = Graphical::new([("DISPLAY", ":99")]).expect("an environment");
            assert_eq!(
                asked(&mut person, Request::Display(display)).await,
                Answer::Displayed
            );
            person
        });
        attached_within(&installed);
        let found = nc_channel::found::beside(&installed.executable).expect("the install reads");
        let line = |note: &str| {
            ["reference.irreversible.do", "--note", note]
                .map(std::ffi::OsString::from)
                .to_vec()
        };
        let english = nc_cli::Env::of(|name| (name == "LANG").then(|| "en_US.UTF-8".to_owned()));
        let confirmed = nc_cli::run(&line("photo"), &found, &english);
        assert_eq!(confirmed.code(), 0, "{confirmed:?}");
        let declined = nc_cli::run(&line("diary"), &found, &english);
        assert_eq!(declined.code(), 15, "{declined:?}");
        assert!(
            declined.stderr.contains(ErrorCode::KernelDeclined.code()),
            "{}",
            declined.stderr
        );
        mcp_waits_for_the_desktop(&found, &english);
        runtime.block_on(async {
            match asked(&mut person, Request::Confirm { pending: 1 }).await {
                Answer::Refused(refusal) => {
                    assert_eq!(refusal.code, ErrorCode::DaemonNotAttached, "{refusal:?}");
                }
                other => panic!("a connection not attached answered a held call: {other:?}"),
            }
            let (mut impostor, _) = daemon.connected().await;
            match asked(&mut impostor, Request::AttachDesktop { proof: [0; 32] }).await {
                Answer::Refused(refusal) => {
                    assert_eq!(refusal.code, ErrorCode::DaemonAttachRefused, "{refusal:?}");
                }
                other => panic!("a proof of no secret attached: {other:?}"),
            }
            assert!(
                impostor.ask(&Request::Status).await.is_err(),
                "a refused attach closes its connection"
            );
            assert_eq!(asked(&mut person, Request::Stop).await, Answer::Stopping);
            assert_eq!(
                daemon.ended().await,
                Some(0),
                "{}",
                installed.standard_error()
            );
        });
    }

    /// A scratch directory of this test's, removed however the test leaves.
    struct Scratch(std::path::PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl Scratch {
        /// A private directory of its own, under the temporary one.
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("nc-dsk-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            nc_platform::create_private_directory(&root).expect("a scratch");
            Self(root)
        }
    }

    /// The word a child of this test binary started as a desktop that only
    /// waits finds among its arguments.
    const WAITS: &str = "nc-daemon-desktop-waits";

    /// A desktop that is not there is not started, and nothing of it is
    /// made; one started runs once,
    /// and attaches a connection only by its own process proving its own
    /// secret for that connection.
    ///
    /// **Both halves, each held alone**: the child this test binary starts as
    /// the desktop — it reads its input and waits for the file its last
    /// argument names — is started once and asked for again as running, the
    /// same process; a proof of its secret over a connection's nonces attaches
    /// that connection from its process, and not from another process, from no
    /// process the system named, over another connection's nonces, nor as a
    /// proof of another secret; and once it ended, nothing attaches.
    #[test]
    fn a_desktop_attaches_by_its_own_process_proving_its_own_secret() {
        if std::env::args().any(|argument| argument == WAITS) {
            use std::io::Read as _;
            let mut secret = Vec::new();
            let _ = std::io::stdin().read_to_end(&mut secret);
            let ends = std::env::args().next_back().expect("the file it waits for");
            let deadline = std::time::Instant::now() + crate::testing::PATIENCE;
            while !Path::new(&ends).exists() && std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            return;
        }
        let scratch = Scratch::new();
        let scratch = &scratch.0;
        let x = |name: &str| (name == "DISPLAY").then(|| OsString::from(":0"));
        let missing = super::Desktop::for_tests(
            scratch.join("not-there"),
            Vec::new(),
            scratch.join("missing.err"),
        );
        assert_eq!(missing.wanted(None, &x), super::Wanted::NotStarted);
        assert!(
            !scratch.join("missing.err").exists(),
            "nothing of a desktop not installed is made, its standard error included"
        );
        let desktop = super::Desktop::for_tests(
            std::env::current_exe().expect("this test binary"),
            [
                "--exact",
                "desktop::tests::a_desktop_attaches_by_its_own_process_proving_its_own_secret",
                "--nocapture",
                "--test-threads=1",
                "--skip",
                WAITS,
                "--skip",
            ]
            .map(str::to_owned)
            .into_iter()
            .chain([scratch.join("ends").to_string_lossy().into_owned()])
            .collect(),
            scratch.join("desktop.err"),
        );
        let super::Wanted::Started(pid) = desktop.wanted(None, &x) else {
            panic!("the desktop was not started");
        };
        assert_eq!(desktop.wanted(None, &x), super::Wanted::Running(pid));
        let drawn = nc_ipc::Drawn::new([1; 32], [2; 32]);
        let proof = desktop
            .started
            .lock()
            .expect("nothing panics holding it")
            .as_ref()
            .expect("started")
            .secret
            .proof(&drawn);
        assert!(desktop.attaches(Some(pid), &proof, &drawn));
        assert!(
            !desktop.attaches(Some(pid + 1), &proof, &drawn),
            "another process"
        );
        assert!(!desktop.attaches(None, &proof, &drawn), "no process named");
        let another = nc_ipc::Drawn::new([1; 32], [3; 32]);
        assert!(
            !desktop.attaches(Some(pid), &proof, &another),
            "another connection's nonces"
        );
        let stranger = nc_ipc::Secret::fresh().expect("a secret").proof(&drawn);
        assert!(
            !desktop.attaches(Some(pid), &stranger, &drawn),
            "another secret"
        );
        drop(nc_platform::create_private(&scratch.join("ends")).expect("the end is told"));
        let deadline = std::time::Instant::now() + crate::testing::PATIENCE;
        while desktop.attaches(Some(pid), &proof, &drawn) {
            assert!(std::time::Instant::now() < deadline, "it never ended");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// A desktop is started where a window could be shown, with the
    /// environment announced last set over the daemon's, on every system.
    ///
    /// **Linux names a display or starts nothing**: the environment announced
    /// is set whole, whatever the daemon's names; with none announced, the
    /// daemon's own is enough where it names an X or a Wayland display, and
    /// one naming neither — or naming one empty — starts nothing. macOS and
    /// Windows start one as the daemon was started, announced or not.
    #[test]
    fn a_desktop_is_started_where_a_window_could_be_shown_in_the_environment_announced() {
        let announced = Graphical::new([
            ("WAYLAND_DISPLAY", "wayland-1"),
            ("XDG_RUNTIME_DIR", "/run/u"),
        ])
        .expect("an environment");
        let none = |_: &str| None;
        let x = |name: &str| (name == "DISPLAY").then(|| OsString::from(":0"));
        let wayland = |name: &str| (name == "WAYLAND_DISPLAY").then(|| OsString::from("w-0"));
        let empty = |name: &str| (name == "DISPLAY").then(OsString::new);
        let other = |name: &str| (name == "XAUTHORITY").then(|| OsString::from("/a"));
        let set = Some(vec![
            ("WAYLAND_DISPLAY".to_owned(), "wayland-1".to_owned()),
            ("XDG_RUNTIME_DIR".to_owned(), "/run/u".to_owned()),
        ]);
        for own in [&none as &dyn Fn(&str) -> Option<OsString>, &x, &other] {
            assert_eq!(
                environment_for(System::Linux, Some(&announced), own),
                set,
                "what was announced is set whole"
            );
        }
        assert_eq!(environment_for(System::Linux, None, &x), Some(Vec::new()));
        assert_eq!(
            environment_for(System::Linux, None, &wayland),
            Some(Vec::new())
        );
        for own in [&none as &dyn Fn(&str) -> Option<OsString>, &empty, &other] {
            assert_eq!(
                environment_for(System::Linux, None, own),
                None,
                "no display named, no desktop started"
            );
        }
        for system in [System::MacOs, System::Windows] {
            for announced in [None, Some(&announced)] {
                assert_eq!(
                    environment_for(system, announced, &none),
                    Some(Vec::new()),
                    "{system:?}"
                );
            }
        }
    }
}
