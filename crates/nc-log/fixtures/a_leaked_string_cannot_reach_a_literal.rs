//! This must NOT compile. See `a_string_cannot_be_a_log_value.rs`.
//!
//! **The attack the second wall was built for**, and for four rounds this file
//! did not test it. It expected `cannot call non-const method
//! `String::leak::<'_>` in constants` — which is *rustc's* const-evaluation
//! rule, not the wall's. A leaked `String` **is** `&'static str`, so `LogValue`
//! never refuses it; the language does. Two reviewers and two stub shapes agreed:
//! the sentence comes back whether or not `nc-log` is behind the build, so the
//! empty-crate control could only ever report that this file proves nothing.
//!
//! What the wall actually guarantees at this door is that a `Literal` can be
//! built from a *const* and from nothing else. `from_macro` is the constructor
//! `literal!` expands to; it is `#[doc(hidden)]` and not public, so a caller
//! reaching for it with runtime text is refused by `nc-log` and by nothing else.
//! Against a crate of the same name with the guarantee removed, this compiles —
//! which is what makes it a proof rather than a coincidence.
//!
//! EXPECT: no associated function or constant named `from_macro` found for struct `Literal`

fn main() {
    let email = std::env::args().nth(1).unwrap_or_default();
    let leaked: &'static str = email.leak();
    let _ = nc_log::Literal::from_macro(leaked);
}
