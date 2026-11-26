//! `noctua-desktop`, the desktop interface of a channel.
//!
//! **Thin**: it reads its arguments, hands them to `nc_desktop`, and writes
//! what comes back through the one printing door below. Run with no argument
//! it is the interface for the daemon of the install beside it
//! (`nc_desktop::host::run`): a window kept hidden until the daemon's state
//! shows it, which runs until it is ended. `noctua-desktop --probe` asks
//! whether this system can host the interface (`nc_desktop::host::probe`) —
//! a window opened in the light and one in the dark, each hidden and
//! undecorated, kept hidden though asked to show until its document loaded,
//! its first frame painting the background its settled page paints — the
//! dark one the design system's dark surface, the two differing — its page
//! the least size when asked for less, and its footer and its drawer doing
//! what they are for, each closed — and answers exit
//! code 0 when all of it held and 1 when any did not, saying which; it needs
//! no install and no daemon. Any other invocation is exit code 2; a refusal
//! of the install or of the window is exit code 1, with its sentence.
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

use nc_desktop::host::{Probed, Seen, Unopened};

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

/// What is wrong with the window a probe opened in `mode`, by what it `seen`.
fn wrong_with(mode: &str, seen: &Seen, wrong: &mut Vec<String>) {
    if !seen.hidden_when_opened {
        wrong.push(format!(
            "the window in the {mode} was shown once it was opened"
        ));
    }
    if !seen.undecorated {
        wrong.push(format!(
            "the window in the {mode} draws the system's decorations"
        ));
    }
    match seen.hidden_when_loaded {
        None => wrong.push(format!(
            "the document in the {mode} did not say it loaded within the wait"
        )),
        Some(false) => wrong.push(format!(
            "the window in the {mode} was shown before its document had loaded"
        )),
        Some(true) => {}
    }
    match (seen.first_frame, seen.settled) {
        (Some(first), Some(settled)) if first == settled => {}
        (Some(first), Some(settled)) => wrong.push(format!(
            "the first frame in the {mode} painted {first:?} and the settled page {settled:?}"
        )),
        _ => wrong.push(format!(
            "what the page in the {mode} painted was not read within the wait"
        )),
    }
    let least = (
        nc_catalog::KERNEL.shell_window_min_width,
        nc_catalog::KERNEL.shell_window_min_height,
    );
    if seen.least != Some(least) {
        wrong.push(format!(
            "the page in the {mode}, asked to be smaller than {least:?}, was {:?}",
            seen.least
        ));
    }
    let Some(footer) = seen.footer else {
        wrong.push(format!(
            "the footer in the {mode} was not exercised within the wait"
        ));
        return;
    };
    for (held, what) in [
        (
            footer.enhanced,
            "the design system's script did not enhance the prompt",
        ),
        (footer.drafted, "typing into the prompt said no draft"),
        (
            footer.acted,
            "activating the info line's chevron said no action",
        ),
        (
            footer.kept,
            "the footer drawn again did not show the new markup with the focus and the \
             prompt's block kept",
        ),
        (
            footer.written,
            "a draft written into the prompt did not reach it",
        ),
        (
            footer.rails,
            "the design system's scripts did not take the drawer and its tabs rail",
        ),
        (
            footer.capped,
            "the drawer, holding more than fits, did not stand below the footer inside the \
             window with its content scrolling",
        ),
        (footer.tabbed, "pressing a tab said no page"),
        (
            footer.collapsed,
            "pressing the drawer's collapse said no command",
        ),
    ] {
        if !held {
            wrong.push(format!("in the {mode}, {what}"));
        }
    }
}

/// The exit code and the line a probe is answered with: 0 when each window
/// opened hidden and undecorated, stayed hidden until its document loaded,
/// painted at its first frame what its settled page paints — the dark one
/// the design system's dark surface, the light one another colour — kept its
/// page the least size when asked for less, and its footer and its drawer did
/// what they are for when exercised: its prompt enhanced by the design
/// system's script, typing and a control said, the footer drawn again keeping
/// its focus, a draft written, the drawer drawn and taken by the design
/// system's scripts, standing below the footer inside the window with its
/// content scrolling, and a tab and a rail's control said; 1 naming each that
/// did not, or why no window was opened.
fn probed(probe: Result<Probed, Unopened>) -> (u8, String) {
    let probed = match probe {
        Ok(probed) => probed,
        Err(unopened) => return (REFUSED, format!("noctua-desktop: {unopened}")),
    };
    let mut wrong = Vec::new();
    wrong_with("light", &probed.light, &mut wrong);
    wrong_with("dark", &probed.dark, &mut wrong);
    let dark = probed
        .dark
        .first_frame
        .map(|[red, green, blue, alpha]| ([red, green, blue], alpha));
    match (dark, probed.dark_backdrop) {
        (Some((painted, u8::MAX)), Some(backdrop)) if painted == backdrop => {}
        (_, None) => {
            wrong
                .push("the design system gives no dark surface for the default palette".to_owned());
        }
        (painted, Some(backdrop)) => wrong.push(format!(
            "the first frame in the dark painted {painted:?}, not the dark surface {backdrop:?}"
        )),
    }
    if probed.light.first_frame.is_some() && probed.light.first_frame == probed.dark.first_frame {
        wrong.push("the first frame painted the same in the light and in the dark".to_owned());
    }
    if wrong.is_empty() {
        (
            0,
            "noctua-desktop: a window opened in the light and one in the dark, each hidden and \
             undecorated and kept hidden until its document loaded, each first frame painting \
             what its settled page paints and the dark one the dark surface, each page the \
             least size when asked for less, each footer and drawer doing what they are for, \
             and each window was closed"
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

    use nc_desktop::host::{Exercised, Probed, Seen};

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

    /// What a window that held everything was seen to do, painting `colour`.
    fn held(colour: [u8; 4]) -> Seen {
        Seen {
            hidden_when_opened: true,
            undecorated: true,
            hidden_when_loaded: Some(true),
            first_frame: Some(colour),
            settled: Some(colour),
            least: Some((
                nc_catalog::KERNEL.shell_window_min_width,
                nc_catalog::KERNEL.shell_window_min_height,
            )),
            footer: Some(Exercised {
                enhanced: true,
                drafted: true,
                acted: true,
                kept: true,
                written: true,
                rails: true,
                capped: true,
                tabbed: true,
                collapsed: true,
            }),
        }
    }

    /// The colour the dark surface is, in the probes below.
    const DARK: [u8; 4] = [10, 20, 30, 255];

    /// A probe in which everything held, painting [`DARK`] in the dark.
    fn passing() -> Probed {
        Probed {
            light: held([250, 250, 250, 255]),
            dark: held(DARK),
            dark_backdrop: Some([10, 20, 30]),
        }
    }

    /// A probe is exit code 0 only when each window opened hidden and
    /// undecorated, stayed hidden until its document loaded, painted at its
    /// first frame what its settled page paints, and kept its page the least
    /// size; each of those that did not hold, in either mode, is exit code
    /// 1, named.
    #[test]
    fn a_probe_passes_only_when_every_part_held() {
        let passing = passing();
        let (code, line) = super::probed(Ok(passing));
        assert_eq!(code, 0, "{line}");
        assert!(
            line.contains("each footer and drawer doing what they are for"),
            "{line}"
        );
        assert!(line.contains("each window was closed"), "{line}");
        let changed = |mode: &str, change: &dyn Fn(&mut Seen)| {
            let mut probed = passing;
            change(if mode == "light" {
                &mut probed.light
            } else {
                &mut probed.dark
            });
            probed
        };
        for mode in ["light", "dark"] {
            for (probed, said) in [
                (
                    changed(mode, &|seen| seen.hidden_when_opened = false),
                    "shown once it was opened",
                ),
                (
                    changed(mode, &|seen| seen.undecorated = false),
                    "draws the system's decorations",
                ),
                (
                    changed(mode, &|seen| seen.hidden_when_loaded = None),
                    "did not say it loaded",
                ),
                (
                    changed(mode, &|seen| seen.hidden_when_loaded = Some(false)),
                    "shown before its document had loaded",
                ),
                (
                    changed(mode, &|seen| seen.first_frame = Some([1, 2, 3, 255])),
                    "the settled page",
                ),
                (
                    changed(mode, &|seen| seen.settled = None),
                    "was not read within the wait",
                ),
                (
                    changed(mode, &|seen| seen.least = Some((160, 120))),
                    "asked to be smaller",
                ),
                (
                    changed(mode, &|seen| seen.least = None),
                    "asked to be smaller",
                ),
            ] {
                let (code, line) = super::probed(Ok(probed));
                assert_eq!(code, 1, "{mode}, {said}: {line}");
                assert!(
                    line.contains(&format!("in the {mode}")) && line.contains(said),
                    "{mode}, {said}: {line}"
                );
            }
        }
    }

    /// A probe is exit code 1, naming what did not hold, when the footer of
    /// either window was not exercised, or when any part of its exercise —
    /// the footer's or the drawer's — did not do what it is for.
    #[test]
    fn a_probe_passes_only_when_each_footer_did_what_it_is_for() {
        let passing = passing();
        for mode in ["light", "dark"] {
            for (part, said) in [
                (None, "the footer in the"),
                (Some(0), "did not enhance the prompt"),
                (Some(1), "said no draft"),
                (Some(2), "said no action"),
                (Some(3), "drawn again"),
                (Some(4), "did not reach it"),
                (Some(5), "did not take the drawer"),
                (Some(6), "content scrolling"),
                (Some(7), "said no page"),
                (Some(8), "collapse said no command"),
            ] {
                let mut probed = passing;
                let seen = if mode == "light" {
                    &mut probed.light
                } else {
                    &mut probed.dark
                };
                match part {
                    None => seen.footer = None,
                    Some(part) => {
                        let footer = seen.footer.as_mut().expect("exercised");
                        *[
                            &mut footer.enhanced,
                            &mut footer.drafted,
                            &mut footer.acted,
                            &mut footer.kept,
                            &mut footer.written,
                            &mut footer.rails,
                            &mut footer.capped,
                            &mut footer.tabbed,
                            &mut footer.collapsed,
                        ]
                        .into_iter()
                        .nth(part)
                        .expect("a part") = false;
                    }
                }
                let (code, line) = super::probed(Ok(probed));
                assert_eq!(code, 1, "{mode}, {said}: {line}");
                assert!(
                    line.contains(&format!("in the {mode}")) && line.contains(said),
                    "{mode}, {said}: {line}"
                );
            }
        }
    }

    /// A probe is exit code 1 when the first frame in the dark painted
    /// anything but the dark surface, opaque, when the design system gives
    /// none, or when the light and the dark painted the same.
    #[test]
    fn a_probe_passes_only_when_the_dark_is_the_dark_surface() {
        let passing = passing();
        for (probed, said) in [
            (
                Probed {
                    dark: held([10, 20, 31, 255]),
                    ..passing
                },
                "not the dark surface",
            ),
            (
                Probed {
                    dark: held([10, 20, 30, 254]),
                    ..passing
                },
                "not the dark surface",
            ),
            (
                Probed {
                    dark_backdrop: None,
                    ..passing
                },
                "no dark surface",
            ),
            (
                Probed {
                    light: held(DARK),
                    ..passing
                },
                "the same in the light and in the dark",
            ),
        ] {
            let (code, line) = super::probed(Ok(probed));
            assert_eq!(code, 1, "{said}: {line}");
            assert!(line.contains(said), "{said}: {line}");
        }
        let (code, line) = super::probed(Err(nc_desktop::host::Unopened::Thread(
            std::io::Error::from(std::io::ErrorKind::OutOfMemory),
        )));
        assert_eq!(code, 1, "{line}");
        assert!(line.contains("was not started"), "{line}");
    }
}
