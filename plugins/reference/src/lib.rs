//! The reference plugin: the component the kernel's suites load.
//!
//! **Two halves, and only one of them is WebAssembly.** [`behaviour`] is what
//! the plugin does, in plain Rust with no binding to the world, and the host
//! target tests it like any other crate. `adapter` is the world's side — the
//! `Guest` implementation and the one `export!` — and exists only when this
//! crate is built for `wasm32`, because a host `cdylib` holding the exports
//! does not link: their names carry `:`, `/`, `@` and `#`, and the linker's
//! version script refuses them. Compiling the whole adapter out rather than the
//! `export!` line alone is what keeps the host build free of the dead code the
//! adapter would otherwise be there.
//!
//! No `unsafe` is written here, and `[lints] workspace = true` keeps
//! `forbid(unsafe_code)`: the `export!` this plugin calls is `nc-guest`'s macro,
//! so its expansion is an external one — `crates/nc-guest/src/lib.rs` says what
//! that does and does not cover.
//!
//! What this answers (`NOCTUA.md §2.4`): nothing of the host is reachable from
//! here but through the world, because the world is all `nc-guest` generates —
//! and its probe reaches every function of it, which is what lets the kernel's
//! suite show each one refused. What it does not answer: it is a test subject,
//! and a plugin that behaves is no evidence about one that does not.
#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
// Every doctest of this crate forbids `unsafe` as the crate does: a
// doctest is a crate of its own, outside the root's `forbid`.
#![doc(test(attr(forbid(unsafe_code))))]

pub mod behaviour;

mod adapter;
