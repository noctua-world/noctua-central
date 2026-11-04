//! This must NOT compile. See `a_string_cannot_be_a_log_value.rs`.
//!
//! **The other gate beside the wall**, and the one on *every* line the log
//! emits. `§15.4`'s own table lists "service and version" among what the
//! central log contains, and `Origin` declares both as `Literal`. Typed as
//! bare `pub &'static str` fields, they would have no wall in front of them.
//!
//! A `static OnceLock<String>` yields a genuine `&'static str` from run-time
//! input with **no leak spelling anywhere**, so a source gate chasing `.leak()`
//! never sees it — run-time text as a `&'static str`, which is what
//! `a_leaked_string_cannot_reach_a_literal.rs` keeps away from `Literal`. With
//! `service` typed `&'static str`, a consumer built like this file puts a
//! tenant's address in that field of every line it records:
//!
//! ```text
//! {"_msg":"db.migrated","level":"info","service":"tenant: Ana <ana@example.com>",…}
//! ```
//!
//! **`service` is the field this proves, and `version` is the field
//! `an_origin_version_cannot_hold_a_runtime_string.rs` proves.** The refusal
//! below is the field's *type* refusing a `&'static str`, and `version` has the
//! same type. Each fixture puts the run-time string in its own field and a
//! `literal!` in the other, so each refusal is about one field: the
//! empty-crate control rebuilds this against a hollow `Origin` that removes
//! only `service`'s guarantee and keeps `version: Literal`. This file compiles
//! there; a file under this name that attacked `version` instead would not,
//! and the control reports that as failing the same way with no `nc-log`
//! behind it.
//!
//! Putting the run-time string in *both* fields is wrong for the same reason:
//! the hollow `version` refuses it too, so the sentence comes back whether or
//! not `nc-log` guards `service`, and the control says this file proves
//! nothing — which is the control working.
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
