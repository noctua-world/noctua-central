//! `noctua-daemon`, the binary an installer runs to prepare a profile.
//!
//! **Thin**: it reads its arguments, hands them to `nc_daemon`, and writes what
//! comes back through the one printing door below. It answers
//! `noctua-daemon --initialize`, the installer's entry, with `--world <file>`
//! to copy a world in, and refuses any other invocation with exit code 2; a
//! refusal of the profile is exit code 1, with its code and its sentence.

#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]

use std::path::PathBuf;
use std::process::ExitCode;

/// Reads the invocation, runs it, and answers with its exit code.
fn main() -> ExitCode {
    let arguments: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let world = match arguments.as_slice() {
        [initialize] if initialize == "--initialize" => None,
        [initialize, named, world] if initialize == "--initialize" && named == "--world" => {
            Some(PathBuf::from(world))
        }
        _ => {
            say("noctua-daemon: the one invocation this build answers is \
                 `noctua-daemon --initialize [--world <file>]`");
            return ExitCode::from(2);
        }
    };
    let executable = match std::env::current_exe() {
        Ok(executable) => executable,
        Err(error) => {
            say(&format!(
                "noctua-daemon: the system could not say which executable this is: {error}"
            ));
            return ExitCode::FAILURE;
        }
    };
    match nc_daemon::initialize(&executable, world.as_deref()) {
        Ok(initialized) => {
            say(&format!(
                "noctua-daemon: the profile under `{}` is ready, and its device's flake is {}",
                initialized.layout.state.display(),
                initialized.device.flake()
            ));
            ExitCode::SUCCESS
        }
        Err(refused) => {
            say(&format!("noctua-daemon: {refused}"));
            ExitCode::FAILURE
        }
    }
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
