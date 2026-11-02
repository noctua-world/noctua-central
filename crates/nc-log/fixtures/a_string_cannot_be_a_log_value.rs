//! This must NOT compile, and `cargo xtask check` says so if it does.
//!
//! `NOCTUA.md §15.4` and acceptance criterion 2: **a dynamic string cannot
//! compile into a log event**, proven by a compile-fail test rather than by a
//! comment.
//!
//! **One shape per fixture, deliberately.** They were one file, and three of the
//! four shapes reported a trait error while the fourth — a borrowed `&str` —
//! reported none at all: a lifetime error comes out of borrow checking, and
//! borrow checking does not run when type checking has already failed. The
//! shape the wall most depends on was the one being masked by its neighbours.
//!
//! EXPECT: the trait bound `String: LogValue` is not satisfied

fn accepts<T: nc_log::LogValue>(_: T) {}

fn main() {
    // The one that matters: a `String` built from something a user typed.
    let name: String = std::env::args().nth(1).unwrap_or_default();
    accepts(name);
}
