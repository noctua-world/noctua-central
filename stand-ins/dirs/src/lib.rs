//! A stand-in for `dirs` 6, holding the one function `wry` calls, put in its
//! place by the root manifest's `[patch.crates-io]`: the crate itself takes
//! `dirs-sys`, which takes `option-ext`, MPL-2.0, and this tree links no
//! file-level copyleft (`deny.toml`'s `[licenses]`).
//!
//! `wry` asks it for the download directory its download handler falls back to
//! — in its `WebKitGTK` half and its `WKWebView` half — and is answered
//! with nothing, so such a download would fall back to the working directory.
//! The interface starts no download, and a release of `wry` that calls another
//! function of `dirs` does not compile against this one.

#![forbid(unsafe_code)]
#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
// Every doctest of this crate forbids `unsafe` as the crate does: a doctest is
// a crate of its own, outside the root's `forbid`.
#![doc(test(attr(forbid(unsafe_code))))]

use std::path::PathBuf;

/// The directory a download is saved in: none, so `wry` falls back to the
/// working directory.
#[must_use]
pub const fn download_dir() -> Option<PathBuf> {
    None
}
