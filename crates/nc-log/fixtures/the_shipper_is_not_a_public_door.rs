//! This must NOT compile. See `a_string_cannot_be_a_log_value.rs`.
//!
//! **The gate beside the wall.** `Shipper::send` takes a raw `String`. Were it
//! `pub` along with `Shipper`, `start`, `dropped` and `DEFAULT_ENDPOINT`, a
//! consumer crate could put arbitrary run-time text into the central log store
//! past every wall, and an embedded newline would forge a second record in
//! it:
//!
//! ```ignore
//! let shipper = nc_log::ship::Shipper::start(nc_log::ship::DEFAULT_ENDPOINT)?;
//! shipper.send(format!("{{\"email\":\"{email}\"}}"));
//! ```
//!
//! The fixtures beside it, a sealed `Event`, `#[non_exhaustive]` on every `Field`
//! variant and a const-evaluation door all guard `Log::record`. This one guards
//! the shipper itself: `Shipper`, its `start` and its `send` are `pub(crate)`.
//!
//! **The path a consumer has, not the one the crate root re-imports.**
//! `nc_log::Shipper` resolves through the crate root's `use ship::Shipper;` — a
//! *private* import, so that path is `E0603` whatever `ship.rs` declares. A
//! fixture naming it would go on refusing with the door wide open, and the
//! empty-crate control would not object, because the hollow crate re-exports
//! its `Shipper` at the root and compiles. `ship` is a public module, so the
//! path a consumer actually has is `nc_log::ship::Shipper`, and that is the
//! path this file names; against a crate of the same name whose
//! `ship::Shipper` is public, it compiles.
//!
//! With `pub` added to `Shipper`, `start` and `send` and to nothing else, a
//! consumer crate outside the workspace builds against the real crate and
//! writes a record of its own choosing into a listening store, which then
//! holds:
//!
//! ```text
//! {"_msg":"forged","email":"ana.silva@example.com"}
//! ```
//!
//! EXPECT: struct `Shipper` is private

fn main() {
    let email = std::env::args().nth(1).unwrap_or_default();
    let shipper =
        nc_log::ship::Shipper::start("http://127.0.0.1:9428/insert").expect("an endpoint");
    shipper.send(email);
}
