//! This must NOT compile. See `an_origin_cannot_hold_a_runtime_string.rs`.
//!
//! **`version` by name**, which is what that fixture's own header says no
//! fixture here covers. `Origin` declares both fields as `Literal` and the
//! fixture beside this one proves the refusal on `service` only — so
//! `pub version: Literal` could go back to `&'static str` with criterion 2's
//! own test at `1 passed`, and `version` is on every line the log emits.
//!
//! The run-time string goes in `version` and `service` keeps its `literal!`,
//! so the refusal still needs `nc-log`: the empty-crate control rebuilds this
//! against a hollow `Origin` whose fields carry no type at all, where it
//! compiles.
//!
//! EXPECT: expected `Literal`, found `&str`

use std::sync::OnceLock;

static HELD: OnceLock<String> = OnceLock::new();

fn main() {
    let version = std::env::args().nth(1).unwrap_or_default();
    let held: &'static str = HELD.get_or_init(|| version).as_str();
    let _ = nc_log::Log::open(
        nc_log::Origin {
            service: nc_log::literal!("noctua"),
            version: held,
        },
        "http://127.0.0.1:9428/insert",
    );
}
