//! This must NOT compile. See `a_string_cannot_be_a_log_value.rs`.
//!
//! **The doors beside the wall.** `Field`'s variants were ordinary tuple
//! variants, so an outside crate could write `Field::Id(email)` and hand the
//! result to anything taking a `Field` — no macro, no `LogValue`, no gate. Every
//! variant is now `#[non_exhaustive]`, which makes construction *and* matching
//! from outside a compile error.
//!
//! Nothing tested it: removing the attribute from `Field::Id` left the whole
//! suite green, and it is the only thing holding that variant shut.
//!
//! EXPECT: tuple variant `Id` is private
//!
//! **It declared `cannot be constructed`, which names nothing.** That sentence
//! is a span label any private or `#[non_exhaustive]` type provokes, and a
//! fixture whose declaration names no identifier is one nothing can hold to a
//! subject — a reviewer wrote a five-line forgery for it and the gate counted it
//! as proof. `Id` is a variant of `nc_log`'s own `Field`, which this file cannot
//! declare.
//!
//! **It named nothing in backticks**, which turned the fixture rule off for
//! this file: `cannot be constructed` is a sentence any private or
//! `#[non_exhaustive]` type provokes, and there was no identifier to hold the
//! fixture to. `Id` is a variant of `nc_log`'s own `Field`, which this file
//! cannot declare.

fn main() {
    let email = std::env::args().nth(1).unwrap_or_default();
    let _ = nc_log::Field::Id(email);
}
