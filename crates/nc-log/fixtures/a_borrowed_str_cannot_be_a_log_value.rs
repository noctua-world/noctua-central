//! This must NOT compile. See `a_string_cannot_be_a_log_value.rs`.
//!
//! **No string type is a `LogValue`**, and this is the shape that used to be the
//! whole argument. The wall was once `LogValue for &'static str`, on the
//! reasoning that a `&str` borrowed from a local cannot satisfy that bound. It
//! cannot — and `String::leak` manufactures a real `&'static str` out of
//! anything, so the reasoning held and the wall did not. `Literal` replaced it,
//! and this fixture now proves the narrower thing it always proved: a borrowed
//! `&str` is not a log value. No lint and no review is involved.
//!
//! **One fixture per file, and the reason is not the one this used to give.**
//! It said "it fails during borrow checking rather than type checking", and it
//! does not: compiled alone against the real crates, rustc answers
//! `error[E0277]: the trait bound `&str: LogValue` is not satisfied` — E0277 is
//! trait selection, which is type checking, and the `EXPECT` line below says so
//! itself.
//!
//! The rule still holds for the reason that applies to every fixture here: two
//! type errors in one crate can stop the compiler before it reaches the second,
//! so a file that must fail for a *stated* reason has to be the only thing that
//! can fail in it.
//!
//! EXPECT: the trait bound `&str: LogValue` is not satisfied

fn accepts<T: nc_log::LogValue>(_: T) {}

fn main() {
    let owned = std::env::var("HOME").unwrap_or_default();
    let borrowed: &str = &owned;
    accepts(borrowed);
}
