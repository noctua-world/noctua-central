//! The launcher: `noctua-dev` where a symbolic link needs a privilege a user
//! may not hold (`nc_platform::Reach::Launcher`, Windows) is a copy of the CLI
//! with a pointer beside it, and **a CLI that finds no install beside itself,
//! but finds its pointer, runs the CLI the pointer names** (`NOCTUA.md §17.1`).
//!
//! **Every argument unchanged, standard input, output and error inherited,
//! and that CLI's exit code its own**, so whoever ran `noctua-dev` sees one
//! run. The CLI named reads its own install beside itself, judged as every
//! binary's is, so the launcher decides nothing a channel's install decides.
//!
//! **Compiled and tested on every system**, though only Windows installs one:
//! `apps/cli/tests/the_launcher_passes_everything_through.rs` runs a launcher
//! on Linux and macOS as on Windows.
//!
//! # What this answers (`§2.4`)
//!
//! Another account choosing what a launcher runs: its pointer is read only
//! once its directory is judged written by this user alone and the pointer
//! itself this user's alone, no further than its bound, and the CLI it names is
//! a plain absolute path. A pointer refused runs nothing, and ends in the code
//! a CLI without an install ends in.
//!
//! # What this does not answer
//!
//! A process running as this user, which can rewrite the pointer as an
//! installer would; and the CLI named, which judges its own install.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use nc_catalog::ErrorCode;
use nc_channel::checkouts::{Pointer, pointer_beside, read_bounded};
use nc_channel::found::Refused;

use crate::exit::Exit;
use crate::render::{self, Env, Failure, Outcome};

/// How a CLI with no install beside it ends.
#[derive(Debug)]
pub enum Through {
    /// It was a launcher, and the CLI its pointer names ran and ended with
    /// this code, which is the launcher's.
    Ran(i32),
    /// What it says: a CLI with no install, or a launcher refused.
    Said(Outcome),
}

/// How a CLI that found no install beside itself — `refused` — ends, run
/// with `argv` in `env`.
///
/// **As the launcher** a pointer beside it makes it, or as a CLI with no
/// install.
#[must_use]
pub fn through(refused: &Refused, argv: &[OsString], env: &Env) -> Through {
    if let Refused::NoManifest { .. } = refused
        && let Some(executable) = running()
        && let Some(launched) = launched_from(&executable, argv)
    {
        return match launched {
            Ok(code) => Through::Ran(code),
            Err(failure) => Through::Said(render::failed(
                crate::run::asks_for_json(argv),
                env.locale(),
                &failure,
            )),
        };
    }
    Through::Said(crate::unfound(argv, refused, env))
}

/// The executable running now, canonicalized.
fn running() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    std::fs::canonicalize(&executable).ok()
}

/// What the launcher at `executable` comes to, run with `argv`: nothing when
/// no pointer is beside it; the code the CLI it names ended with; or why
/// nothing was run.
///
/// # Errors
///
/// [`Failure`] for a pointer refused, or for a CLI it names that did not
/// start.
#[must_use]
pub fn launched_from(executable: &Path, argv: &[OsString]) -> Option<Result<i32, Failure>> {
    let pointer = pointer_beside(executable);
    if let Err(error) = std::fs::symlink_metadata(&pointer)
        && error.kind() == std::io::ErrorKind::NotFound
    {
        return None;
    }
    let refused = |detail: String| Failure {
        code: ErrorCode::CliLauncherRefused,
        exit: Exit::NoInstall,
        detail,
    };
    Some(
        pointed(executable, &pointer)
            .map_err(refused)
            .and_then(|cli| {
                ran(&cli, argv)
                    .map_err(|error| refused(format!("`{}` did not start: {error}", cli.display())))
            }),
    )
}

/// The CLI the pointer at `pointer` names, beside the launcher `executable`,
/// once its directory and the pointer are judged.
fn pointed(executable: &Path, pointer: &Path) -> Result<PathBuf, String> {
    let user = nc_platform::this_user()
        .map_err(|error| format!("the user this runs as could not be asked: {error}"))?;
    let bin = executable
        .parent()
        .ok_or_else(|| format!("`{}` has no directory", executable.display()))?;
    nc_platform::judge_written_by_user_alone(bin, &user)
        .map_err(|judged| format!("{judged}, so the pointer in it was not read"))?;
    let text = read_bounded(pointer, &user)?
        .ok_or_else(|| format!("`{}` went before it was read", pointer.display()))?;
    Pointer::parse(&text)
        .map(|read| read.cli)
        .map_err(|what| format!("`{}` was refused: {what}", pointer.display()))
}

/// The CLI `cli` run with `argv`, its standard input, output and error this
/// process's, and the code it ended with.
#[expect(
    clippy::disallowed_methods,
    reason = "the launcher runs the CLI its pointer names in its place, handing it this \
              process's standard input, output and error, so what it prints is the run's"
)]
fn ran(cli: &Path, argv: &[OsString]) -> std::io::Result<i32> {
    std::process::Command::new(cli)
        .args(argv)
        .status()
        .map(|status| passed_on(status.code()))
}

/// The code a launcher ends with when the CLI it ran ended with `code`: that
/// code, or the CLI's own failure where the system names none — a process
/// ended by a signal.
#[must_use]
pub fn passed_on(code: Option<i32>) -> i32 {
    code.unwrap_or_else(|| i32::from(Exit::Failed.code()))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::PathBuf;

    use nc_catalog::ErrorCode;

    use crate::exit::Exit;

    /// A scratch directory for one test, removed however the test leaves.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = nc_platform::testing::make_private(&self.0);
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl Scratch {
        fn new(tag: &str) -> Self {
            let root = std::env::temp_dir()
                .join(format!("nc-launch-pointer-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            nc_platform::create_private_directory(&root).expect("a scratch directory");
            Self(root)
        }

        fn file(&self, name: &str, text: &str) -> PathBuf {
            use std::io::Write as _;
            let path = self.0.join(name);
            nc_platform::create_private(&path)
                .and_then(|mut file| file.write_all(text.as_bytes()))
                .expect("a file");
            path
        }
    }

    /// A launcher ends with the code the CLI it ran ended with, and with the
    /// failure of its own where the system names none.
    #[test]
    fn a_launcher_ends_with_the_code_its_cli_ended_with() {
        for code in [0, 2, 13, 255, -1_073_741_510] {
            assert_eq!(super::passed_on(Some(code)), code);
        }
        assert_eq!(super::passed_on(None), i32::from(Exit::Failed.code()));
    }

    /// A CLI with no pointer beside it is no launcher; one with a pointer that
    /// is not this user's alone, or does not read, runs nothing and says why.
    #[test]
    fn no_pointer_is_no_launcher_and_a_refused_one_runs_nothing() {
        let scratch = Scratch::new("refused");
        let launcher = scratch.0.join("noctua-dev-t1");
        let argv: Vec<OsString> = vec![OsString::from("help")];
        assert!(super::launched_from(&launcher, &argv).is_none());
        let pointer = nc_channel::checkouts::pointer_beside(&launcher);
        for (text, said) in [
            (
                "{\"cli\":\"relative\",\"format\":1}".to_owned(),
                "not an absolute path",
            ),
            ("{\"cli\":1,\"format\":1}".to_owned(), "not text"),
            ("not json".to_owned(), "not JSON"),
        ] {
            let _ = std::fs::remove_file(&pointer);
            scratch.file(
                pointer
                    .file_name()
                    .and_then(|name| name.to_str())
                    .expect("a name"),
                &text,
            );
            let refused = super::launched_from(&launcher, &argv)
                .expect("a pointer is there")
                .expect_err("refused");
            assert_eq!(refused.code, ErrorCode::CliLauncherRefused);
            assert_eq!(refused.exit, Exit::NoInstall);
            assert!(refused.detail.contains(said), "{}", refused.detail);
        }
        let opened = nc_platform::testing::open_to_another_user(&pointer).expect("opened");
        let refused = super::launched_from(&launcher, &argv)
            .expect("a pointer is there")
            .expect_err("refused");
        assert!(refused.detail.contains(&opened), "{}", refused.detail);
        nc_platform::testing::make_private(&pointer).expect("private again");
        nc_platform::testing::let_another_user_write(&scratch.0).expect("written by another");
        let refused = super::launched_from(&launcher, &argv)
            .expect("a pointer is there")
            .expect_err("refused");
        assert!(
            refused.detail.contains("so the pointer in it was not read"),
            "{}",
            refused.detail
        );
    }
}
