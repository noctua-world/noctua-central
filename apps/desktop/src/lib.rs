//! The desktop interface, `noctua-desktop` — `noctua-staging-desktop`, and
//! `noctua-dev-<checkout>-desktop` for a development checkout (`NOCTUA.md
//! §12`, `§17.1`): a window and its webview, drawing the interface's state
//! the daemon holds, and changing it only by commands and notes.
//!
//! # What a run does, in order
//!
//! - **The install** is read beside the canonicalized executable, as every
//!   binary of an install reads it (`nc_channel::found`), and names the
//!   daemon this window is a view of.
//! - **The window** ([`host`]): built hidden and undecorated before anybody
//!   asks to see it, its webview handed the document [`page`] builds —
//!   markup written as `rsx!`, with the interface's sheet of layout
//!   utilities ([`sheet`]) inlined.
//! - **The daemon's state** ([`client`], [`mirror`]): followed by a watch on a
//!   connection of its own, read strictly, and drawn — the window shown and
//!   hidden as the state says; and what the window asks, a command or a note,
//!   sent on the other connection.
//!
//! **The interface is a view** (`§12.1`): it holds no authoritative state, so
//! killing it loses nothing, and a window opened again draws what the daemon
//! held. **`--probe`** asks whether this system can host the interface at all,
//! with no daemon ([`host::probe`]).
//!
//! # What this answers (`§2.4`)
//!
//! Another account standing in for the daemon, or being shown as the
//! daemon's: both connections are made through `nc_ipc::client`, which reads
//! the endpoint's other end off the system, reads the token only once it is
//! judged this user's alone, and sends nothing to what has not proven it holds
//! it. A state of another shape drawn as one it is not: the mirror reads every
//! field strictly, and keeps the state it held over one it cannot read. The
//! webview loading what this build did not write: it is handed one document,
//! which links nothing, and its message channel is read for one word.
//!
//! # What it does not
//!
//! A process running as the same user, which can read the token and answer
//! as the daemon, drive the interface through the CLI as the person can, or
//! inject into this process; and, in this build, the confirmation of a held
//! call, which is not a command and which this window does not yet offer
//! (`§11.1`).

#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
// Every doctest of this crate forbids `unsafe` as the crate does: a
// doctest is a crate of its own, outside the root's `forbid`.
#![doc(test(attr(forbid(unsafe_code))))]

pub mod client;
pub mod host;
pub mod mirror;
pub mod page;
pub mod sheet;
