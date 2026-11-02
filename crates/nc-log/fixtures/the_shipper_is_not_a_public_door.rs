//! This must NOT compile. See `a_string_cannot_be_a_log_value.rs`.
//!
//! **The gate beside the wall.** `Shipper::send` takes a raw `String`, and it
//! was `pub` along with `Shipper`, `start`, `dropped` and `DEFAULT_ENDPOINT` —
//! so three lines from a consumer crate put arbitrary run-time text into the
//! central log store past every wall, and an embedded newline forged a second
//! record in it:
//!
//! ```ignore
//! let shipper = nc_log::Shipper::start(nc_log::ship::DEFAULT_ENDPOINT)?;
//! shipper.send(format!("{{\"email\":\"{email}\"}}"));
//! ```
//!
//! The fixtures beside it, a sealed `Event`, `#[non_exhaustive]` on every `Field`
//! variant and a const-evaluation door all guard `Log::record`. This guarded
//! nothing, and reverting it left `cargo xtask check` green — the fifth time
//! this crate has closed a door and left it unheld.
//!
//! **And it named a path that could not fail.** This tested `nc_log::Shipper`,
//! which resolves through the crate root's `use ship::Shipper;` — a *private*
//! import, so that path is `E0603` whatever `ship.rs` declares. The fixture was
//! baselined where it cannot report anything, and `ship` is a public module, so
//! the path a consumer actually has is `nc_log::ship::Shipper`. Measured on
//! 2026-09-08 with `Shipper`, `start` and `send` each made `pub` — three tokens:
//!
//! ```text
//! ok    11 fixture(s) refuse to compile, for the stated reason they declare
//! ok    1185 test(s)
//! check passed, with 9 warn line(s)
//! ```
//!
//! — while a consumer crate outside the workspace built and put
//! `{"_msg":"forged","email":"ana.silva@example.com"}` into a listening store.
//! The paragraph above already reached for the public path in prose while the
//! code below tested the private one.
//!
//! EXPECT: struct `Shipper` is private

fn main() {
    let email = std::env::args().nth(1).unwrap_or_default();
    let shipper =
        nc_log::ship::Shipper::start("http://127.0.0.1:9428/insert").expect("an endpoint");
    shipper.send(email);
}
