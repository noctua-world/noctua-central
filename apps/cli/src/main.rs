//! `noctua`, the command line a channel is reached through.
//!
//! **Thin**: it reads its arguments, the install beside its executable and
//! the environment's language, hands them to `nc_cli`, and writes the outcome
//! through the one printing door below — standard output, then standard
//! error — exiting with the outcome's code. What a run does and which code it
//! ends in is the library's; what is written, and the code when it cannot be,
//! is decided by [`written`], which this file's own tests call.

#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]

use std::ffi::OsString;
use std::io::Write;
use std::process::ExitCode;

use nc_cli::{Env, Outcome};

/// Reads the invocation, runs it, and exits with its outcome's code.
fn main() -> ExitCode {
    let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
    let env = Env::of(|name| std::env::var(name).ok());
    let outcome = match nc_channel::found::of_this_process() {
        Ok(found) => nc_cli::run(&arguments, &found, &env),
        Err(refused) => nc_cli::unfound(&arguments, &refused, &env),
    };
    ExitCode::from(say(&outcome))
}

/// The one place this binary prints: the outcome, to the process's own
/// standard output and standard error.
#[expect(
    clippy::disallowed_methods,
    reason = "the CLI's one printing door: what a run answered is written to the streams the \
              person or the program that ran it reads"
)]
fn say(outcome: &Outcome) -> u8 {
    written(
        outcome,
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
    )
}

/// Writes `outcome` to `stdout` and `stderr`, and answers the code the
/// process exits with: the outcome's, or 1 when what it answered could not be
/// written whole — said on `stderr` when that still takes it.
fn written(outcome: &Outcome, stdout: &mut impl Write, stderr: &mut impl Write) -> u8 {
    let out = stdout
        .write_all(outcome.stdout.as_bytes())
        .and_then(|()| stdout.flush());
    let err = stderr
        .write_all(outcome.stderr.as_bytes())
        .and_then(|()| stderr.flush());
    match out.and(err) {
        Ok(()) => outcome.code(),
        Err(error) => {
            // A line nobody can read is no reason to hide the code that
            // says the answer was cut short.
            let _ = writeln!(
                stderr,
                "error[{}]: {error}",
                nc_catalog::ErrorCode::CliOutputFailed.code()
            );
            nc_cli::exit::Exit::Failed.code()
        }
    }
}

#[cfg(test)]
mod tests {
    use nc_cli::Outcome;
    use nc_cli::exit::Exit;

    /// A writer that takes nothing.
    struct Closed;

    impl std::io::Write for Closed {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn outcome(exit: Exit) -> Outcome {
        Outcome {
            exit,
            stdout: "answer\n".to_owned(),
            stderr: "said\n".to_owned(),
        }
    }

    /// What a run answered is written whole, each stream its own, and the
    /// process exits with the outcome's code.
    #[test]
    fn an_outcome_is_written_and_its_code_is_the_exit_code() {
        for exit in Exit::ALL {
            let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
            let code = super::written(&outcome(exit), &mut stdout, &mut stderr);
            assert_eq!(code, exit.code());
            assert_eq!(stdout, b"answer\n");
            assert_eq!(stderr, b"said\n");
        }
    }

    /// An answer that cannot be written whole is exit 1, whatever the run came to.
    ///
    /// It is said on standard error, with the CLI's code, where that still
    /// takes it.
    #[test]
    fn an_answer_that_cannot_be_written_is_exit_1() {
        let mut stderr = Vec::new();
        let code = super::written(&outcome(Exit::Success), &mut Closed, &mut stderr);
        assert_eq!(code, 1);
        let said = String::from_utf8(stderr).expect("text");
        let code = nc_catalog::ErrorCode::CliOutputFailed.code();
        assert!(said.contains(&format!("[{code}]")), "{said}");
        let code = super::written(&outcome(Exit::Denied), &mut Vec::new(), &mut Closed);
        assert_eq!(code, 1);
    }
}
