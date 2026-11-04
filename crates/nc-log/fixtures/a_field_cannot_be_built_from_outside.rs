//! This must NOT compile. See `a_string_cannot_be_a_log_value.rs`.
//!
//! **A door beside the wall.** If `Field`'s variants were ordinary tuple
//! variants, an outside crate could write `Field::Id(email)` and hand the
//! result to anything taking a `Field` — no macro, no `LogValue`, no gate.
//! Every variant is `#[non_exhaustive]`, which makes construction *and*
//! matching from outside a compile error.
//!
//! `Id` holds a `String`, so it is the variant any text at all would travel
//! in, and this file builds it from outside the crate.
//!
//! EXPECT: tuple variant `Id` is private
//!
//! **The sentence names `Id`**, a variant of `nc_log`'s own `Field`, which
//! this file cannot declare. An expectation of `cannot be constructed` would
//! name nothing: that is a span label any private or `#[non_exhaustive]` type
//! provokes, and the fixture rule in `xtask/src/wall.rs` refuses an
//! expectation with no backticked identifier, because it leaves nothing to
//! hold the fixture to.
//!
//! What this file holds is the attribute's effect, seen from outside the
//! crate. That every variant carries the attribute is held in the source, by
//! `every_variant_of_a_field_is_non_exhaustive` in
//! `crates/nc-log/src/value.rs`, which reads the lines above each variant of
//! `Field`.

fn main() {
    let email = std::env::args().nth(1).unwrap_or_default();
    let _ = nc_log::Field::Id(email);
}
