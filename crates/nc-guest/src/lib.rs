//! The guest half of the kernel's world, generated once for every plugin.
//!
//! A plugin depends on this crate and calls `nc_guest::generated::export!`; none
//! of them runs `wit_bindgen::generate!` itself. The world is read from
//! `crates/nc-kernel/wit/noctua-kernel.wit` — the file the host's bindings are
//! generated from — so the two halves of every call are compiled from one text.
//! **That file is the reference a plugin's author reads**, not a page here: the
//! generated module is hidden from rustdoc, for the reason written on it, so
//! nothing in it is rendered and nothing here links to it.
//!
//! **A crate of its own, because of `forbid(unsafe_code)`.** `generate!` emits
//! `export!` as a `macro_rules!` inside its own output, and its expansion writes
//! `#[unsafe(export_name)]`, `#[unsafe(link_section)]` and `unsafe extern "C"`
//! functions. Invoked in the crate that generated it, that expansion is local
//! code and the workspace's `forbid` refuses it — measured in a scratch crate
//! carrying this workspace's lints: ten `unsafe_code` errors building for
//! `wasm32`. Invoked from another crate, it is an external macro's expansion, and
//! rustc does not report `unsafe_code` there; so every plugin keeps `[lints]
//! workspace = true`, `forbid` included, and so does this crate, whose own
//! `generate!` is an external macro here too.
//!
//! **What `forbid` then means is written down, not implied:** no `unsafe` is
//! written in this repository. The expansion is `wit-bindgen`'s, as its runtime
//! is, and the `unsafe` in it is theirs to answer for the way wasmtime's
//! internals are. Never judge the lints with `WIT_BINDGEN_DEBUG` set: it
//! includes the generated source as a local file, and every one of those
//! expansions then reads as this crate's own.
//!
//! It builds for the host target too — `wit-bindgen` emits stubs there — which
//! is what lets a plugin's behaviour be tested on the host, and the generated
//! doc-test run there.
//!
//! What this answers (`NOCTUA.md §2.4`): a plugin's own code declares no import
//! the world does not have, because the only bindings it is given are these.
//! What it does not answer, and who does: a dependency that brings bindings of
//! its own — `wasip2` embeds a WASI world beside this one, and the encoder
//! merges the two — is refused when `cargo xtask` encodes the plugin, which
//! reads the component back and holds every import to the world
//! (`xtask/src/plugins.rs`); an `extern` block written in the plugin is refused
//! by its own `forbid(unsafe_code)`, and one a dependency writes, which no world
//! describes, by the encoder itself; and what a granted import may do is the
//! kernel's.
#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
// Every doctest of this crate forbids `unsafe` as the crate does: a
// doctest is a crate of its own, outside the root's `forbid`.
#![doc(test(attr(forbid(unsafe_code))))]

/// Everything `wit_bindgen::generate!` produced for the world `plugin`.
///
/// **Hidden from rustdoc, which is also why `missing_docs` is not put down
/// here.** What the generator writes carries the documentation the WIT has —
/// which a plugin's author reads — and no more: its namespace modules and
/// export helpers carry none. Allowing `missing_docs` for the whole module is
/// the blanket `gates::no_source_puts_the_lints_down` refuses; `missing_docs`
/// does not ask a hidden item for a doc comment, and nothing here is meant to be
/// read as a rendered page.
#[doc(hidden)]
#[expect(
    clippy::same_length_and_capacity,
    reason = "generate! rebuilds each list the canonical ABI hands over with \
              `Vec::from_raw_parts(ptr, len, len)`: the host allocated exactly `len` through \
              `cabi_realloc`, so length equal to capacity is the contract, not a mistake"
)]
pub mod generated {
    wit_bindgen::generate!({
        path: "../nc-kernel/wit",
        world: "plugin",
        pub_export_macro: true,
        default_bindings_module: "nc_guest::generated",
    });
}
