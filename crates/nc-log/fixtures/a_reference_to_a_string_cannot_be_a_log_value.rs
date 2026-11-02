//! This must NOT compile. See `a_string_cannot_be_a_log_value.rs`.
//!
//! `&String` derefs to `&str`, so this is the shape that would slip through if
//! `LogValue` were written on `AsRef<str>` or on anything that auto-derefs. It
//! is not: the trait is implemented on concrete types, and no string type is
//! among them.
//!
//! EXPECT: the trait bound `&String: LogValue` is not satisfied

fn accepts<T: nc_log::LogValue>(_: T) {}

fn main() {
    let owned = String::from("x");
    accepts(&owned);
}
