//! This must NOT compile. See `a_string_cannot_be_a_log_value.rs`.
//!
//! **The door that needs no leak.** With `Event` an open trait and `Field`'s
//! variants public, a crate outside `nc-log` could write its own event and
//! hand the log anything at all:
//!
//! ```ignore
//! impl Event for Mine {
//!     fn fields(&self) -> Vec<(&'static str, Field)> {
//!         vec![("who", Field::Id(user_email))]
//!     }
//! }
//! ```
//!
//! No leak, no macro, no `unsafe`, nothing any source gate looks for. With both
//! doors open, that impl in a consumer crate puts
//! `{"_msg":"user.did_something","who":"victim@example.com"}` on a line the
//! real crate ships to the store.
//!
//! `Event` has a private supertrait, and that takes nothing away: every event
//! struct is generated **into** `nc-log` from `catalog/log-events.toml`, so an
//! event nobody declared does not exist (`§15.2`).
//!
//! EXPECT: the trait bound `Mine: nc_log::sealed::Sealed` is not satisfied

struct Mine;

impl nc_log::Event for Mine {
    fn name(&self) -> &'static str {
        "mine.happened"
    }
    fn level(&self) -> nc_catalog::Level {
        nc_catalog::Level::Info
    }
    fn fields(&self) -> Vec<(&'static str, nc_log::Field)> {
        Vec::new()
    }
}

fn main() {
    let _ = Mine;
}
