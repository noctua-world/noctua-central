//! `noctua-daemon`, the daemon a channel runs, and the binary an installer
//! runs to prepare its profile.
//!
//! **Thin**: it reads its arguments, hands them to `nc_daemon`, and writes what
//! comes back through the one printing door below. Run with no argument it is
//! the daemon (`nc_daemon::run::serve`), which answers exit code 0 once it
//! stopped when it was asked to. `noctua-daemon --restored` is how every
//! restorer starts it (`nc_daemon::run::restore`): exit code 0 at once while
//! the profile carries the `stopped` mark or another daemon of the channel
//! runs, and the daemon otherwise. It answers `noctua-daemon --initialize`, the
//! installer's entry, with `--world <file>` to copy a world in, and refuses
//! any other invocation with exit code 2; a refusal — of the install, the
//! profile or anything else a start reads — is exit code 1, with its code and
//! its sentence.
//!
//! **Which invocation is answered, and with which exit code, is decided by
//! functions this file's own tests call**, so a weakening of either is judged
//! without running the binary; `main` only hands them the arguments and the
//! executable and prints what they answer.

#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

/// The exit code of an invocation this build does not answer.
const UNANSWERED: u8 = 2;

/// The exit code of a refusal: of the profile, or of the system asked which
/// executable this is.
const REFUSED: u8 = 1;

/// Reads the invocation, runs it, and answers with its exit code.
fn main() -> ExitCode {
    let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
    let (code, line) = answer(&arguments, std::env::current_exe);
    say(&line);
    ExitCode::from(code)
}

/// An invocation this build answers.
#[derive(Debug, PartialEq, Eq)]
enum Asked {
    /// No argument: run the daemon until it is asked to stop.
    Serve,
    /// `--restored`: run it as a restorer starts it, honouring the `stopped`
    /// mark.
    Restored,
    /// `--initialize`, and the world file named by `--world` when it follows.
    Initialize {
        /// The world file to copy into the profile, when one is named.
        world: Option<PathBuf>,
    },
}

/// What `arguments` ask of this build, or nothing it answers.
fn asked(arguments: &[OsString]) -> Option<Asked> {
    match arguments {
        [] => Some(Asked::Serve),
        [restored] if restored == "--restored" => Some(Asked::Restored),
        [initialize] if initialize == "--initialize" => Some(Asked::Initialize { world: None }),
        [initialize, named, world] if initialize == "--initialize" && named == "--world" => {
            Some(Asked::Initialize {
                world: Some(PathBuf::from(world)),
            })
        }
        _ => None,
    }
}

/// The exit code and the line `arguments` are answered with; `executable` is
/// asked which binary this is only for an invocation this build answers.
fn answer(
    arguments: &[OsString],
    executable: impl FnOnce() -> std::io::Result<PathBuf>,
) -> (u8, String) {
    let Some(asked) = asked(arguments) else {
        return (
            UNANSWERED,
            "noctua-daemon: the invocations this build answers are `noctua-daemon`, which \
             runs the daemon, `noctua-daemon --restored`, which runs it as a restorer \
             does, and `noctua-daemon --initialize [--world <file>]`"
                .to_owned(),
        );
    };
    let executable = match executable() {
        Ok(executable) => executable,
        Err(error) => {
            return (
                REFUSED,
                format!(
                    "noctua-daemon: the system could not say which executable this is: {error}"
                ),
            );
        }
    };
    let world = match asked {
        Asked::Serve => {
            return match nc_daemon::run::serve(&executable) {
                Ok(stopped) => (0, ran(stopped)),
                Err(refused) => (REFUSED, format!("noctua-daemon: {refused}")),
            };
        }
        Asked::Restored => {
            return match nc_daemon::run::restore(&executable) {
                Ok(nc_daemon::run::Restored::StaysDown) => (
                    0,
                    "noctua-daemon: the profile carries the `stopped` mark, so the daemon \
                     stays down until `start` removes it"
                        .to_owned(),
                ),
                Ok(nc_daemon::run::Restored::AlreadyUp) => (
                    0,
                    "noctua-daemon: another daemon of this channel runs, so this one did not \
                     start"
                        .to_owned(),
                ),
                Ok(nc_daemon::run::Restored::Ran(stopped)) => (0, ran(stopped)),
                Err(refused) => (REFUSED, format!("noctua-daemon: {refused}")),
            };
        }
        Asked::Initialize { world } => world,
    };
    match nc_daemon::initialize(&executable, world.as_deref()) {
        Ok(initialized) => (
            0,
            format!(
                "noctua-daemon: the profile under `{}` is ready, and its device's flake is {}",
                initialized.layout.state.display(),
                initialized.device.flake()
            ),
        ),
        Err(refused) => (REFUSED, format!("noctua-daemon: {refused}")),
    }
}

/// What a run that stopped when it was asked to says.
fn ran(stopped: nc_daemon::run::Stopped) -> String {
    format!(
        "noctua-daemon: stopped when it was asked to, after {} ms, giving up on {} \
         dispatch(es), pump(s) or sweep(s) still running",
        stopped.ran.as_millis(),
        stopped.abandoned
    )
}

/// The one place this binary prints: a line to its standard error.
#[expect(
    clippy::disallowed_methods,
    reason = "the daemon's one printing door: an installer reads what `--initialize` did, and \
              why it refused, from its standard error"
)]
fn say(line: &str) {
    use std::io::Write as _;
    // A line nobody can read is no reason to fail what it reports on.
    let _ = writeln!(std::io::stderr().lock(), "{line}");
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::PathBuf;

    /// A scratch directory for one test, removed however the test leaves.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = nc_platform::testing::make_private(&self.0);
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl Scratch {
        /// A private directory of its own, holding a stand-in executable.
        fn new(name: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("nc-daemon-main-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            nc_platform::create_private_directory(&root).expect("a scratch directory");
            nc_platform::create_private(&root.join("noctua-daemon")).expect("a stand-in binary");
            Self(root)
        }

        fn executable(&self) -> PathBuf {
            self.0.join("noctua-daemon")
        }
    }

    fn words(arguments: &[&str]) -> Vec<OsString> {
        arguments.iter().map(OsString::from).collect()
    }

    /// The executable an invocation this build does not answer must never ask
    /// for.
    fn never() -> std::io::Result<PathBuf> {
        panic!("an invocation this build does not answer asked which executable this is")
    }

    /// No argument runs the daemon, `--initialize` is answered alone and
    /// with `--world <file>`, and any other invocation is not.
    #[test]
    fn the_daemon_and_initialize_are_answered_and_nothing_else_is() {
        assert_eq!(super::asked(&[]), Some(super::Asked::Serve));
        assert_eq!(
            super::asked(&words(&["--restored"])),
            Some(super::Asked::Restored)
        );
        assert_eq!(
            super::asked(&words(&["--initialize"])),
            Some(super::Asked::Initialize { world: None })
        );
        assert_eq!(
            super::asked(&words(&["--initialize", "--world", "w.json"])),
            Some(super::Asked::Initialize {
                world: Some(PathBuf::from("w.json"))
            })
        );
        for unanswered in [
            &["--serve"][..],
            &["--initialise"],
            &["--initialize", "--world"],
            &["--initialize", "--globe", "w.json"],
            &["--world", "w.json", "--initialize"],
            &["--initialize", "--initialize"],
            &["--restored", "--restored"],
            &["--restored", "--initialize"],
            &["--restore"],
        ] {
            assert_eq!(super::asked(&words(unanswered)), None, "{unanswered:?}");
        }
    }

    /// An invocation this build does not answer is exit code 2, names the one
    /// it does, and asks nothing of the system.
    #[test]
    fn an_invocation_this_build_does_not_answer_is_exit_code_2() {
        let (code, line) = super::answer(&words(&["--serve"]), never);
        assert_eq!(code, 2, "{line}");
        assert!(line.contains("--initialize [--world <file>]"), "{line}");
        assert!(
            line.contains("`noctua-daemon`, which runs the daemon"),
            "{line}"
        );
        assert!(line.contains("`noctua-daemon --restored`"), "{line}");
    }

    /// A start a restorer asks for, outside an install, is exit code 1 in
    /// the shared reader's words, as the daemon's own start is.
    #[test]
    fn a_restored_start_outside_an_install_is_exit_code_1() {
        let scratch = Scratch::new("restored");
        let executable = scratch.executable();
        let (code, line) = super::answer(&words(&["--restored"]), || Ok(executable.clone()));
        assert_eq!(code, 1, "{line}");
        let refused = nc_channel::found::beside(&executable).expect_err("no manifest is there");
        assert_eq!(line, format!("noctua-daemon: {refused}"));
    }

    /// The daemon run outside an install is exit code 1 in the shared reader's
    /// words: the channel's code once, and the manifest it looked for.
    #[test]
    fn the_daemon_outside_an_install_is_exit_code_1() {
        let scratch = Scratch::new("serve");
        let executable = scratch.executable();
        let (code, line) = super::answer(&[], || Ok(executable.clone()));
        assert_eq!(code, 1, "{line}");
        // The reader's sentence, and not a second code of the daemon's before it.
        let refused = nc_channel::found::beside(&executable).expect_err("no manifest is there");
        assert_eq!(line, format!("noctua-daemon: {refused}"));
        let code = nc_catalog::ErrorCode::ChannelNoManifest.code();
        assert_eq!(line.matches(&format!("[{code}]")).count(), 1, "{line}");
        let manifest = std::fs::canonicalize(&executable)
            .expect("the stand-in is there")
            .with_file_name(nc_channel::MANIFEST);
        assert!(line.contains(&manifest.display().to_string()), "{line}");
    }

    /// A refusal is exit code 1 with its code — outside an install, and when
    /// the system cannot say which executable this is.
    #[test]
    fn a_refusal_is_exit_code_1_with_its_code() {
        let scratch = Scratch::new("refused");
        let executable = scratch.executable();
        let (code, line) = super::answer(&words(&["--initialize"]), || Ok(executable));
        assert_eq!(code, 1, "{line}");
        let code = nc_catalog::ErrorCode::ChannelNoManifest.code();
        assert!(line.contains(&format!("[{code}]")), "{line}");
        let (code, line) = super::answer(&words(&["--initialize"]), || {
            Err(std::io::Error::from(std::io::ErrorKind::NotFound))
        });
        assert_eq!(code, 1, "{line}");
    }

    /// An initialized profile is exit code 0, beside a manifest as an
    /// installer writes one.
    #[test]
    fn an_initialized_profile_is_exit_code_0() {
        use std::io::Write as _;
        let scratch = Scratch::new("initialized");
        let base = |name: &str| {
            let base = scratch.0.join(name);
            nc_platform::create_private_directory(&base).expect("a base");
            base
        };
        let install = nc_channel::Install {
            channel: nc_channel::Channel::Staging,
            bases: nc_channel::Bases::new(
                base("data"),
                base("config"),
                base("cache"),
                base("state"),
            )
            .expect("plain bases"),
            namespace: Some(nc_channel::Namespace::parse("t3").expect("a namespace")),
            log_endpoint: None,
        };
        let mut manifest =
            nc_platform::create_private(&scratch.0.join(nc_channel::MANIFEST)).expect("a manifest");
        manifest
            .write_all(install.to_canonical().as_bytes())
            .expect("written");
        drop(manifest);
        let executable = scratch.executable();
        let (code, line) = super::answer(&words(&["--initialize"]), || Ok(executable));
        assert_eq!(code, 0, "{line}");
        let user = nc_platform::user_in(&scratch.0).expect("the scratch is private");
        let layout = nc_channel::layout(&install, &user.to_string());
        assert!(layout.device_key.is_file(), "no seed was drawn: {line}");
    }
}
