//! This must NOT compile, and `cargo xtask check` says so if it does.
//!
//! `NOCTUA.md §15.4` and acceptance criterion 2: **a dynamic string cannot
//! compile into a log event**, proven by a compile-fail test rather than by a
//! comment.
//!
//! **One shape per fixture, deliberately.** Borrow checking does not run once
//! type checking has failed, so a shape refused by a later stage than its
//! neighbours would be masked by them, and a sentence two shapes share would
//! not say which of them produced it. Each file holds one shape, and the only
//! thing in it that can fail is the thing it is named for.
//!
//! EXPECT: the trait bound `String: LogValue` is not satisfied

fn accepts<T: nc_log::LogValue>(_: T) {}

fn main() {
    // The one that matters: a `String` built from something a user typed.
    let name: String = std::env::args().nth(1).unwrap_or_default();
    accepts(name);
}
