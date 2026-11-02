# A canary for the citation gates

Not read by anything but `cargo xtask check`, which asserts that both citation
gates report the two dangling references below. If either stops, that gate has
stopped reading the tree — the failure a numeric floor could not see, because a
filter placed above a counter leaves the number where it was.

The path: `crates/nc-nowhere/src/lib.rs`

The section: `NOCTUA.md §99.99`
