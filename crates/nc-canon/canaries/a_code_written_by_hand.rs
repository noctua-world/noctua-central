//! Not compiled: outside this crate's `src/`, so cargo never sees it, and the
//! §15.1 walk does — it reads `crates/`, `plugins/` and the migrations, never
//! `xtask/`, so this canary lives here and the blanket-allow one in `xtask/`.
//!
//! `cargo xtask check` asserts the walk reports this file. If it stops, the walk
//! has stopped reading the tree, which is the failure a numeric floor cannot
//! see: a filter above a counter leaves the number where it was.

/// A code in no registry, written out as text.
pub const CANARY: &str = "NC-KERN-9999";
