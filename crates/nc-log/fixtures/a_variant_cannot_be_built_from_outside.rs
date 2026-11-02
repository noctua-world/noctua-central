//! This must NOT compile. See `a_string_cannot_be_a_log_value.rs`.
//!
//! `Variant`'s field was `pub`, so `Variant(format!("…{email}").leak())` was an
//! "enum state" as far as every type in this crate was concerned. Making it
//! private was the fix, and nothing held it: restoring `pub` left the suite
//! green.
//!
//! EXPECT: you might have meant to use the `of` associated function
//!
//! **It declared `cannot initialize a tuple struct which contains private
//! fields`, which names nothing** — any private tuple struct anywhere gives that
//! sentence. rustc's own help names the constructor that *is* public, and `of`
//! is `nc_log`'s: a fixture declaring one would be caught by the rule that a
//! fixture may not define what it claims the wall refuses.
//!
//! **It named nothing in backticks**, so the fixture rule had nothing to hold
//! this file to and any private tuple struct anywhere satisfied it. `Variant`
//! is `nc_log`'s, and this file cannot declare one.

fn main() {
    let email = std::env::args().nth(1).unwrap_or_default();
    let _ = nc_log::Variant(email.leak());
}
