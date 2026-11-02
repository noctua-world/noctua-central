//! This must NOT compile. See `a_string_cannot_be_a_log_value.rs`.
//!
//! **The largest hole the wall ever had.** `Event` was an open trait and
//! `Field`'s variants were public, so a crate outside `nc-log` could write its
//! own event and hand the log anything at all:
//!
//! ```ignore
//! impl Event for Mine {
//!     fn fields(&self) -> Vec<(&'static str, Field)> {
//!         vec![("who", Field::Id(user_email))]
//!     }
//! }
//! ```
//!
//! No leak. No macro. Nothing any source gate looks for — the wall was moved to
//! `Literal` and the door beside it was left open. A reviewer built exactly this
//! and got `{"_msg":"user.did_something","who":"victim@example.com"}` out of the
//! real crate, with no `unsafe`.
//!
//! `Event` now has a private supertrait, and that takes nothing away: every
//! event struct is generated **into** `nc-log` from `catalog/log-events.toml`,
//! so an event nobody declared does not exist (`§15.2`).
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
