//! `noctua-desktop`, the desktop interface of a channel.
//!
//! **Thin**: it reads its arguments, hands them to `nc_desktop`, and writes
//! what comes back through the one printing door below. Run with no argument
//! it is the interface for the daemon of the install beside it
//! (`nc_desktop::host::run`): a window kept hidden until the daemon's state
//! shows it, which runs until it is ended. `noctua-desktop --probe` asks
//! whether this system can host the interface (`nc_desktop::host::probe`) —
//! a window opened hidden and undecorated, its document loaded, the window
//! closed — and answers exit code 0 when all of it held and 1 when any did
//! not, saying which; it needs no install and no daemon. Any other
//! invocation is exit code 2; a refusal of the install or of the window is
//! exit code 1, with its sentence.
//!
//! **A window of the system's own kind, never a console's**: on Windows the
//! binary is built for the windows subsystem, so starting it opens no
//! console beside the window; what it says goes to the standard error its
//! starter handed it, and nowhere where none was.
//!
//! **Which invocation is answered, and with which exit code, is decided by
//! functions this file's own tests call**, so a weakening of either is judged
//! without running the binary.

#![windows_subsystem = "windows"]
#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]

use core::time::Duration;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use nc_desktop::host::{Probed, Unopened};

/// The exit code of an invocation this build does not answer.
const UNANSWERED: u8 = 2;

/// The exit code of a refusal, and of a probe that saw something not hold.
const REFUSED: u8 = 1;

/// How long a probe waits for its document to say it loaded: long enough
/// for a webview's first start on a machine that has never made one, which
/// the slowest system measured took seconds to do.
const PROBE_WITHIN: Duration = Duration::from_secs(60);

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
    /// No argument: the interface, for the install beside the executable.
    Run,
    /// `--probe`: whether this system can host the interface.
    Probe,
}

/// What `arguments` ask of this build, or nothing it answers.
fn asked(arguments: &[OsString]) -> Option<Asked> {
    match arguments {
        [] => Some(Asked::Run),
        [probe] if probe == "--probe" => Some(Asked::Probe),
        _ => None,
    }
}

/// The exit code and the line `arguments` are answered with; `executable` is
/// asked which binary this is only for the interface, which reads the install
/// beside it.
fn answer(
    arguments: &[OsString],
    executable: impl FnOnce() -> std::io::Result<PathBuf>,
) -> (u8, String) {
    match asked(arguments) {
        None => (
            UNANSWERED,
            "noctua-desktop: the invocations this build answers are `noctua-desktop`, which \
             runs the interface for the install beside it, and `noctua-desktop --probe`, which \
             asks whether this system can host it"
                .to_owned(),
        ),
        Some(Asked::Probe) => probed(nc_desktop::host::probe(PROBE_WITHIN)),
        Some(Asked::Run) => {
            let executable = match executable() {
                Ok(executable) => executable,
                Err(error) => {
                    return (
                        REFUSED,
                        format!(
                            "noctua-desktop: the system could not say which executable this \
                             is: {error}"
                        ),
                    );
                }
            };
            let found = match nc_channel::found::beside(&executable) {
                Ok(found) => found,
                Err(refused) => return (REFUSED, format!("noctua-desktop: {refused}")),
            };
            match nc_desktop::host::run(found) {
                Ok(()) => (0, "noctua-desktop: the interface ended".to_owned()),
                Err(unopened) => (REFUSED, format!("noctua-desktop: {unopened}")),
            }
        }
    }
}

/// The exit code and the line a probe is answered with: 0 when the window
/// opened hidden and undecorated, its document loaded and it stayed hidden;
/// 1 naming each that did not, or why no window was opened.
fn probed(probe: Result<Probed, Unopened>) -> (u8, String) {
    let probed = match probe {
        Ok(probed) => probed,
        Err(unopened) => return (REFUSED, format!("noctua-desktop: {unopened}")),
    };
    let mut wrong = Vec::new();
    if !probed.hidden_when_opened {
        wrong.push("the window was shown once it was opened");
    }
    if !probed.undecorated {
        wrong.push("the window draws the system's decorations");
    }
    match probed.hidden_when_loaded {
        None => wrong.push("its document did not say it loaded within the wait"),
        Some(false) => wrong.push("the window was shown once its document had loaded"),
        Some(true) => {}
    }
    if wrong.is_empty() {
        (
            0,
            "noctua-desktop: a window opened hidden and undecorated, its document loaded \
             with the window still hidden, and the window was closed"
                .to_owned(),
        )
    } else {
        (
            REFUSED,
            format!(
                "noctua-desktop: this system cannot host the interface as it is built: {}",
                wrong.join("; ")
            ),
        )
    }
}

/// The one place this binary prints: a line to its standard error.
#[expect(
    clippy::disallowed_methods,
    reason = "the desktop's one printing door: a probe's caller, and whoever started the \
              interface, read why it ended from its standard error"
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

    use nc_desktop::host::Probed;

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
                std::env::temp_dir().join(format!("nc-desktop-main-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            nc_platform::create_private_directory(&root).expect("a scratch directory");
            nc_platform::create_private(&root.join("noctua-desktop")).expect("a stand-in binary");
            Self(root)
        }

        fn executable(&self) -> PathBuf {
            self.0.join("noctua-desktop")
        }
    }

    fn words(arguments: &[&str]) -> Vec<OsString> {
        arguments.iter().map(OsString::from).collect()
    }

    /// The executable an invocation that reads no install must never ask
    /// for.
    fn never() -> std::io::Result<PathBuf> {
        panic!("an invocation that reads no install asked which executable this is")
    }

    /// No argument runs the interface, `--probe` probes, and any other
    /// invocation is not answered.
    #[test]
    fn the_interface_and_the_probe_are_answered_and_nothing_else_is() {
        assert_eq!(super::asked(&[]), Some(super::Asked::Run));
        assert_eq!(
            super::asked(&words(&["--probe"])),
            Some(super::Asked::Probe)
        );
        for unanswered in [
            &["--probe", "--probe"][..],
            &["--probe", "x"],
            &["--hidden"],
            &["-probe"],
            &["probe"],
            &["--Probe"],
            &[""],
        ] {
            assert_eq!(super::asked(&words(unanswered)), None, "{unanswered:?}");
        }
    }

    /// An invocation this build does not answer is exit code 2, names the
    /// two it does, and asks nothing of the system.
    #[test]
    fn an_invocation_this_build_does_not_answer_is_exit_code_2() {
        let (code, line) = super::answer(&words(&["--hidden"]), never);
        assert_eq!(code, 2, "{line}");
        assert!(
            line.contains("`noctua-desktop`, which runs the interface"),
            "{line}"
        );
        assert!(line.contains("`noctua-desktop --probe`"), "{line}");
    }

    /// The interface outside an install is exit code 1 in the shared
    /// reader's words, and one the system cannot name the executable of is
    /// exit code 1 too.
    #[test]
    fn the_interface_outside_an_install_is_exit_code_1() {
        let scratch = Scratch::new("outside");
        let executable = scratch.executable();
        let (code, line) = super::answer(&[], || Ok(executable.clone()));
        let refused = nc_channel::found::beside(&executable).expect_err("no manifest is there");
        assert_eq!(code, 1, "{line}");
        assert_eq!(line, format!("noctua-desktop: {refused}"));
        let (code, line) = super::answer(&[], || {
            Err(std::io::Error::from(std::io::ErrorKind::NotFound))
        });
        assert_eq!(code, 1, "{line}");
        assert!(line.contains("could not say which executable"), "{line}");
    }

    /// A probe is exit code 0 only when the window opened hidden and
    /// undecorated and stayed hidden once its document loaded; each of
    /// those that did not hold is exit code 1, named.
    #[test]
    fn a_probe_passes_only_when_every_part_held() {
        let held = Probed {
            hidden_when_opened: true,
            undecorated: true,
            hidden_when_loaded: Some(true),
        };
        let (code, line) = super::probed(Ok(held));
        assert_eq!(code, 0, "{line}");
        assert!(line.contains("the window was closed"), "{line}");
        for (probed, said) in [
            (
                Probed {
                    hidden_when_opened: false,
                    ..held
                },
                "shown once it was opened",
            ),
            (
                Probed {
                    undecorated: false,
                    ..held
                },
                "draws the system's decorations",
            ),
            (
                Probed {
                    hidden_when_loaded: None,
                    ..held
                },
                "did not say it loaded",
            ),
            (
                Probed {
                    hidden_when_loaded: Some(false),
                    ..held
                },
                "shown once its document had loaded",
            ),
        ] {
            let (code, line) = super::probed(Ok(probed));
            assert_eq!(code, 1, "{line}");
            assert!(line.contains(said), "{said}: {line}");
        }
        let (code, line) = super::probed(Err(nc_desktop::host::Unopened::Thread(
            std::io::Error::from(std::io::ErrorKind::OutOfMemory),
        )));
        assert_eq!(code, 1, "{line}");
        assert!(line.contains("was not started"), "{line}");
    }
}
