//! Not compiled: this file is outside every crate's `src/`, and `cargo xtask
//! check` asserts that the blanket-allow gate reports it.
//!
//! **One gate, not two.** This said two, counting the §15.1 walk as the second
//! — and that walk's scope is `crates/` and `migrations/`, so it never reads
//! `xtask/` at all. Measured: deleting the hand-written code that used to sit
//! below changed no number and `check passed`. The §15.1 canary is
//! `crates/nc-canon/canaries/a_code_written_by_hand.rs`, which is where it has
//! to live to be read.
//!
//! If this gate stops reporting this file, it has stopped reading the tree —
//! which is the failure five numeric floors could not see. Nothing may compile
//! it either: a canary cargo builds ships the defect, and `check` refuses any
//! `#[path]` or `include!` that reaches in here.
#![allow(clippy::all)]
