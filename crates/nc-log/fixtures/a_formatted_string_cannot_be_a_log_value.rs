//! This must NOT compile. See `a_string_cannot_be_a_log_value.rs`.
//!
//! Pre-formatting is the obvious way round a type wall, and it is refused twice:
//! here by the trait, and at a real call site by the source gate in
//! `xtask/src/wall.rs`. `format!` produces a `String`, and `String` is not a
//! `LogValue`.
//!
//! EXPECT: the trait bound `String: LogValue` is not satisfied

fn accepts<T: nc_log::LogValue>(_: T) {}

fn main() {
    accepts(format!("{}", std::env::args().count()));
}
