//! A stand-in, empty, for `kuchikiki`, put in its place by the root
//! manifest's `[patch.crates-io]`: `wry` takes it on Android alone, which no
//! build of this tree is for, and it brings `cssparser` and `selectors`,
//! MPL-2.0, into the tree `cargo deny` judges for every target (`deny.toml`'s
//! `[licenses]`). A build for Android would fail here, naming this crate,
//! rather than link a copyleft parser unseen.

#![forbid(unsafe_code)]
#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
// Every doctest of this crate forbids `unsafe` as the crate does: a doctest is
// a crate of its own, outside the root's `forbid`.
#![doc(test(attr(forbid(unsafe_code))))]
