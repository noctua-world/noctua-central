//! This must NOT compile. See `a_string_cannot_be_a_log_value.rs`.
//!
//! **The attack that walked straight through the wall.** `LogValue` used to be
//! implemented for `&'static str` on the reasoning that a borrowed `&'a str`
//! cannot coerce to one. True, and beside the point: `String::leak` — stable
//! since Rust 1.72 — manufactures a real `&'static str` from anything at all,
//! and the leaked value satisfied the trait exactly as a literal did. A source
//! gate knew `Box::leak` and not `.leak()`, so
//!
//! ```ignore
//! let table: &'static str = format!("user={email}").leak();
//! log.record(&events::PruneWithinLimit { table, .. });
//! ```
//!
//! passed every gate and put an email address in a store that stays active in
//! production.
//!
//! Chasing method names is a race that keeps being lost — `Box::leak`,
//! `String::leak`, `Vec::leak`, `into_boxed_str`, `Box::into_raw`. So the type
//! carries it instead: `Literal`'s field is private and `literal!` matches
//! `$text:literal`, which is a **token** rule. A leaked value is not a literal
//! token, so this is not a type error to argue about — it is not that macro rule
//! at all.
//!
//! EXPECT: no rules expected `leaked`

fn main() {
    let email = std::env::args().nth(1).unwrap_or_default();
    let leaked: &'static str = format!("user={email}").leak();
    let _ = nc_log::literal!(leaked);
}
