//! This must NOT compile. See `a_string_cannot_be_a_log_value.rs`.
//!
//! **No string type is a `LogValue`**, borrowed or owned. A bound of
//! `&'static str` would refuse this borrowed `&str` and still let run-time
//! text through: `String::leak` manufactures a real `&'static str` out of
//! anything, so a leaked value would satisfy it exactly as a literal does.
//! Text from the source reaches a log line as a `Literal` instead
//! (`a_leaked_string_cannot_be_a_literal.rs`), and this fixture proves the
//! narrower thing: a borrowed `&str` is not a log value. No lint and no review
//! is involved.
//!
//! **The refusal is a type error.** Compiled alone against the real crates,
//! this file gets an unsatisfied trait bound from rustc, E0277, and E0277 is
//! trait selection, which is type checking, and the `EXPECT` line below says so
//! itself. Borrow checking, which does not run once type checking has failed,
//! plays no part in this refusal.
//!
//! **One fixture per file**, for the reason that applies to every fixture
//! here: one failure can stop rustc before it reaches another, so a file that
//! must fail for a *stated* reason has to be the only thing that can fail in
//! it.
//!
//! EXPECT: the trait bound `&str: LogValue` is not satisfied

fn accepts<T: nc_log::LogValue>(_: T) {}

fn main() {
    let owned = std::env::var("HOME").unwrap_or_default();
    let borrowed: &str = &owned;
    accepts(borrowed);
}
