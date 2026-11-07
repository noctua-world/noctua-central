//! This must NOT compile. See `a_string_cannot_be_a_log_value.rs`.
//!
//! With `Variant`'s field `pub`, `Variant(format!("…{email}").leak())` would
//! be an "enum state" as far as every type in this crate is concerned. The
//! field is private and the public constructor, `Variant::of`, reads a
//! `Source` const, so a variant's name cannot be built from run-time text.
//!
//! EXPECT: you might have meant to use the `of` associated function
//!
//! **The sentence names `of`**: it is rustc's help, pointing at the
//! constructor that *is* public. `of` is `nc_log`'s, so a fixture declaring
//! its own would be refused by the rule that a fixture may not define what it
//! claims the wall refuses. The error beside it, `cannot initialize a tuple
//! struct which contains private fields`, names nothing in backticks — any
//! private tuple struct anywhere gives that sentence — and the fixture rule
//! refuses an expectation with no identifier to hold the file to. `Variant` is
//! `nc_log`'s too, and this file cannot declare one.
//!

fn main() {
    let email = std::env::args().nth(1).unwrap_or_default();
    let _ = nc_log::Variant(email.leak());
}
