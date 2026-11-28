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
//!   asks to see it, in the theme the system prefers ([`theme`]), its webview
//!   loading the document [`page`] builds — markup written as `rsx!`, its
//!   root stamped with the theme, linking the design system's sheets and the
//!   interface's sheet of layout utilities ([`sheet`]) — through the
//!   `noctua` custom protocol, which answers a closed list of paths from
//!   bytes this build embeds ([`assets`]).
//! - **The daemon's state** ([`client`], [`mirror`]): followed by a watch on a
//!   connection of its own, read strictly, and drawn — the window shown and
//!   hidden as the state says, a show waiting for the document to have
//!   loaded, and the footer ([`footer`]) drawn from it, its modeline, its
//!   info line and its prompt line; and what the window asks, a command or a
//!   note, sent on the other connection.
//! - **The keys** ([`keys`], [`commands`], [`drawer`]): the Leader asked of
//!   the system where this window is its listener, tapped or held as the
//!   window hears it, every key a physical key run by the binding in force,
//!   and the leader menu, the command list and a command's parameters drawn
//!   in the drawer — each press, entry and row a command run as the person.
//! - **The drawer and the sidebar** ([`drawer`], [`sidebar`]): the drawer
//!   below the footer, with its controls rail, the page in focus and its tabs
//!   rail, and the sidebar beside the workspace area, listing what is open in
//!   it — both drawn from the state, every control, tab and entry a command
//!   run as the person.
//! - **The surfaces a plugin shows** ([`surface`]): the tree the kernel kept
//!   for each, read strictly as the kernel's closed set of nodes and drawn by
//!   the design system's components, every text escaped and every control
//!   found by its node, never by the command it runs.
//! - **The seats** ([`seat`]): the system's own seat is the engine's, and
//!   what its pointer is over is said to the daemon as that seat's hover;
//!   every other seat the state holds is written onto the page as the design
//!   system's seat attributes, which its sheets style as the native states.
//!   The window is sized and placed as the state asks, never smaller than its
//!   least.
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
//! webview loading what this build did not write, a file of the machine's,
//! or an address off it: everything its custom protocol answers is bytes
//! compiled in, by a closed list of paths no request can step out of; the
//! document's content security policy lets it load a script, a sheet, an
//! image or a font from that protocol alone and connect, frame or submit
//! nowhere ([`page::POLICY`]), so no script in the page can fetch an address
//! or send what it holds off the machine; it navigates nowhere else, opens no
//! window and starts no download; it keeps nothing of its own under the
//! person's directories (`nc_platform::webview::Kept`); and its message
//! channel is read for a closed set of messages. A page asking the
//! window to run what it pleases: a control's word is run only when it is
//! one of the footer's closed set of commands, none of which takes an
//! argument, or one of the drawer's rail, whose arguments the window takes
//! from the state it holds, a key, an entry or a row only as what a binding
//! in force, the `w` list or the window's own list offers, a tab or an entry
//! of the sidebar only as a page the drawer or the window's workspace area
//! holds, and the prompt's text is said only as `prompt.set`'s, cut to its
//! bound — so the page can do nothing the person could not do from the CLI.
//! Markup drawn as something it is not: every text the footer shows is
//! escaped where it is rendered, and the page is handed markup only as a
//! script's string literal.
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

pub mod assets;
pub mod client;
pub mod commands;
pub mod drawer;
pub mod footer;
pub mod host;
pub mod keys;
pub mod mirror;
pub mod page;
pub mod seat;
pub mod sheet;
pub mod sidebar;
pub mod surface;
pub mod text;
pub mod theme;
