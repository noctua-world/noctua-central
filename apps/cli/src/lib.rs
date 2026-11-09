//! The command line a channel is reached through, `noctua` — `noctua-staging`,
//! and `noctua-dev-<checkout>` for a development checkout (`NOCTUA.md §20.2`,
//! `§17.1`).
//!
//! # What a run does, in order
//!
//! - **The install** is read beside the canonicalized executable, as every
//!   binary of an install reads it (`nc_channel::found`); none, or one
//!   refused, ends the run before anything else is read (exit 12).
//! - **The command line** ([`args`]): the options before the command, a verb
//!   — one undotted word — or a command's dotted name, and the words after it,
//!   read against that command's schema into one object and held to the
//!   schema, as the kernel will hold it.
//! - **The daemon** ([`client`]): its endpoint reached and its other end read
//!   off the system, the token read only once it is judged private, the
//!   handshake that proves both ends, and the welcome held to the process at
//!   the other end and to the install's channel — before anything is sent.
//!   Its registry is the one list of commands, which help prints from
//!   ([`help`]) and a command line is read against.
//! - **The outcome** ([`render`], [`exit`]): the answer on standard output and
//!   a refusal on standard error, or one canonical JSON document under
//!   `--json`, and an exit code from the one table.
//!
//! [`run()`] is all of it but reading the install, so a suite drives the CLI
//! in-process against an install of its own; `noctua`'s `main` only reads its
//! arguments, its install and its environment, and prints.
//!
//! # What this answers (`§2.4`)
//!
//! Another account standing in for the daemon, reading the token, or being
//! shown as the daemon's: the endpoint's other end is read off the system, the
//! token is read only from a file and a directory judged this user's alone,
//! nothing is sent to what has not proven it holds the token, and the proven
//! welcome must name the process the system shows and this install's channel.
//! A terminal acted on by what the daemon says: every text from it is escaped.
//! A command a person did not confirm: the CLI confirms nothing, so the kernel
//! refuses every irreversible command it is asked (`NC-KERN-0006`), which ends
//! the run in an exit code of its own.
//!
//! # What it does not
//!
//! A process running as the same user, which can read the token and answer
//! as the daemon, or ask the daemon anything this CLI can — authorization
//! re-decides every dispatch (`§14`); the PID record, which no part of the CLI
//! reads yet, so the welcome's process is held to the system's word alone; and
//! process control — `start`, `stop`, `restart` and `status` — which these
//! modules do not hold.

#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
// Every doctest of this crate forbids `unsafe` as the crate does: a
// doctest is a crate of its own, outside the root's `forbid`.
#![doc(test(attr(forbid(unsafe_code))))]

pub mod args;
pub mod client;
pub mod exit;
pub mod help;
pub mod render;
pub mod run;

pub use render::{Env, Outcome};
pub use run::run;

/// How a run ends when no install was found beside the executable: exit 12,
/// with the manifest's refusal, rendered as the command line asks.
///
/// **The one outcome a run without an install has**: no command line is read
/// past its options and no daemon is asked, because which daemon, and where,
/// is the install's to say.
#[must_use]
pub fn unfound(
    argv: &[std::ffi::OsString],
    refused: &nc_channel::found::Refused,
    env: &Env,
) -> Outcome {
    let failure = render::Failure {
        code: refused.code(),
        exit: exit::Exit::NoInstall,
        detail: render::without_code(&refused.to_string(), refused.code()),
    };
    render::failed(run::asks_for_json(argv), env.locale(), &failure)
}
