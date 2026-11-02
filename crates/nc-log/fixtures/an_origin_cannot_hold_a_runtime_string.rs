//! This must NOT compile. See `a_string_cannot_be_a_log_value.rs`.
//!
//! **The other gate beside the wall**, and the one on *every* line the log
//! emits. `Origin`'s two fields were `pub &'static str` — the only bare static
//! strings left in the crate — and `§15.4`'s own table lists "service and
//! version" among what the central log contains.
//!
//! A `static OnceLock<String>` yields a genuine `&'static str` from run-time
//! input with **no leak spelling anywhere**, which is the exact attack
//! `a_leaked_string_cannot_reach_a_literal.rs` names in its own header: the
//! fixture defended `Literal` against it and left `Origin` open beside it.
//! Measured, before the fields became `Literal`:
//!
//! ```text
//! {"_msg":"db.migrated","level":"info","service":"tenant: Ana <ana@example.com>",…}
//! ```
//!
//! **`service` is the field this proves, and `version` is the same type.** A
//! reviewer is owed the difference: `Origin` declares both as `Literal`, so the
//! refusal below is the field's *type* refusing a `&'static str` and it is the
//! same type on the line above. Putting the run-time string in *both* fields
//! was tried and is **wrong**: with no `literal!` left in the file the refusal
//! stops needing `nc-log` at all, and `the_wall_holds_and_the_proof_actually_ran`'s
//! empty-crate control says so, which is the control working.
//!
//! **`version` is named by `an_origin_version_cannot_hold_a_runtime_string.rs`
//! now.** This header used to say that closing the gap was "a second fixture,
//! and that is a number four documents state", and the gap was real: measured
//! on 2026-09-08, `pub version: Literal` demoted to `&'static str` with its
//! four construction sites followed left criterion 2's own test at `1 passed`.
//! Closing it took the second fixture *and* a stub whose `Origin` keeps no
//! guarantee on either field — the stub used to keep `version: Literal`, which
//! is what made the new fixture refuse against the hollow crate too.
//!
//! EXPECT: expected `Literal`, found `&str`

use std::sync::OnceLock;

static HELD: OnceLock<String> = OnceLock::new();

fn main() {
    let service = std::env::args().nth(1).unwrap_or_default();
    let held: &'static str = HELD.get_or_init(|| service).as_str();
    let _ = nc_log::Log::open(
        nc_log::Origin {
            service: held,
            version: nc_log::literal!("9.9.9"),
        },
        "http://127.0.0.1:9428/insert",
    );
}
