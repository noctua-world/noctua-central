//! This must NOT compile. See `a_string_cannot_be_a_log_value.rs`.
//!
//! **The door beside the macro.** A macro expands at the caller's site, so the
//! constructor `literal!` calls must be reachable from there, and what the wall
//! guarantees at that door is that a `Literal` can be built from a *const* and
//! from nothing else: `Literal::of` reads `Source::TEXT`, an associated const.
//! A public constructor taking a `&'static str` would accept `email.leak()`
//! from any crate, with no macro, no `unsafe`, and no source gate to see it.
//!
//! This file calls such a constructor, `from_macro`, with run-time text, and
//! `nc-log` declares none. Against a crate of the same name whose `Literal` has
//! a public `from_macro(&'static str)`, it compiles — which is what makes it a
//! proof rather than a coincidence. Expecting rustc's const-evaluation refusal,
//! `cannot call non-const method` in a constant, would not be one: that refusal
//! is the language's, and its sentence comes back whether or not `nc-log` is
//! behind the build.
//!
//! EXPECT: no associated function or constant named `from_macro` found for struct `Literal`

fn main() {
    let email = std::env::args().nth(1).unwrap_or_default();
    let leaked: &'static str = email.leak();
    let _ = nc_log::Literal::from_macro(leaked);
}
