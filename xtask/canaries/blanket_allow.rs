//! Not compiled: this file is outside every crate's `src/`, and `cargo xtask
//! check` asserts that the blanket-allow gate reports it.
//!
//! **One gate, not two.** The §15.1 walk reads `crates/`, `plugins/` and the
//! migrations and never opens `xtask/`, so a hand-written error code here is
//! one that walk never reads. The §15.1 canary is
//! `crates/nc-canon/canaries/a_code_written_by_hand.rs`, which is where it has
//! to live to be read.
//!
//! If this gate stops reporting this file, it has stopped reading the tree —
//! which is the failure a numeric floor cannot see: a filter above a counter
//! leaves the number where it was. Nothing may compile it either: a canary
//! cargo builds ships the defect, and `check` refuses any `#[path]` or
//! `include!` that reaches in here.
#![allow(clippy::all)]
