//! The daemon a channel runs: what names it, the profile it keeps, the device
//! it mints identifiers on, the mark that keeps those identifiers unique
//! across a restart, and the kernel it serves at its endpoint (`NOCTUA.md
//! §11.1`, `§16.2`, `§17.1`).
//!
//! # What a start reads, in order
//!
//! - **The install** (`nc_channel::found`, the reader every binary of an
//!   install shares): the manifest beside the canonicalized executable, judged
//!   this user's alone and read by `nc_channel::Install::parse`. There is no
//!   channel built in to fall back to: a binary with no manifest refuses before
//!   it reads anything else, with the code the CLI beside it would refuse with.
//! - **The profile** ([`profile`]): its data, plugins, configuration, cache and
//!   state directories and the log and runtime ones under the state one,
//!   created private one level at a time or judged so, and the world file the
//!   daemon decides as — its realm and its person (`§5.13`).
//! - **The device** ([`device`]): the seed the installer drew and the flake
//!   derived from it. The daemon reads them and never creates them.
//! - **The mark** ([`mark`]): the ceiling no identifier is minted past before
//!   it is durable, which a start resumes above — with the newest unit the
//!   local journal holds — and waits behind.
//!
//! # What a run serves
//!
//! [`run`] holds the order: the instance lock, the central log
//! ([`environment`]), the PID record and a fresh token ([`pidfile`]), the
//! endpoint ([`server`]), the kernel over the local database, the plugins
//! installed in the profile ([`plugins`]), and the pump and the sweep on the
//! catalog's schedule ([`schedule`]) — until a stop drains the dispatches
//! running, waits for every connection's last answer, and settles the mark.
//!
//! `noctua-daemon` with no argument is the daemon; `noctua-daemon
//! --initialize` is the installer's entry into the same modules: it prepares
//! the profile, creates the device when there is none and copies a world in.
//! Every refusal is a [`Refused`], which carries the catalog code a log is
//! queried by.
//!
//! # What this answers (`§2.4`)
//!
//! Another account on this machine renaming the install, reading the device's
//! seed, rewriting the world or the mark, installing a plugin, or reaching the
//! daemon: every one of them is a path judged this user's alone before it is
//! read, every one this crate makes is created private in one call, and the
//! endpoint is `nc_platform::ipc`'s, proven by `nc_ipc`'s handshake before a
//! request is read. An identifier handed out twice across a crash or a clock
//! moved back: none is handed out past a ceiling that is not yet durable, and
//! a start resumes above the ceiling and the newest unit its local journal
//! holds, waiting for the clock rather than minting behind it. Two daemons over
//! one profile: the second refuses at once. A network reaching the daemon: it
//! binds nothing a network can reach, which a gate holds.
//!
//! # What it does not
//!
//! A process running as the same user, which can rewrite the manifest, the
//! world or the mark, delete the mark so a start resumes from the journal
//! alone, read the token and speak for anybody, or install a plugin — every
//! dispatch is the kernel's to decide, whoever asked; and a disk that answers a
//! sync before the bytes are on it. A daemon killed leaves its PID record, its
//! token and its socket for the next start to replace.

#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
// Every doctest of this crate forbids `unsafe` as the crate does: a
// doctest is a crate of its own, outside the root's `forbid`.
#![doc(test(attr(forbid(unsafe_code))))]

pub mod device;
pub mod environment;
pub mod mark;
pub mod pidfile;
pub mod plugins;
pub mod profile;
pub mod run;
pub mod schedule;
pub mod server;
pub mod testing;

use std::path::{Path, PathBuf};

use nc_catalog::ErrorCode;

/// Why the daemon did not start, or `--initialize` did not finish.
///
/// **One type for every module**, so a caller maps every refusal to its code
/// in one place, and each variant to the code its own failure is queried by.
#[derive(Debug)]
pub enum Refused {
    /// No install was found beside the executable: no manifest, one another
    /// account can reach, one this build does not read, or a path the
    /// filesystem did not answer for — each by its own `CHAN` code.
    Install(nc_channel::found::Refused),
    /// A directory or a file of the profile is not this user's alone.
    NotPrivate(nc_platform::Refusal),
    /// The filesystem could not answer for a path of the install or the
    /// profile.
    Unavailable {
        /// What was being done, naming the path.
        doing: String,
        /// What the system answered.
        error: std::io::Error,
    },
    /// The profile holds no device: its seed or its record is missing.
    NoDevice {
        /// The file that is missing.
        path: PathBuf,
    },
    /// The device's seed or its record is not one this build wrote.
    DeviceUnreadable {
        /// The file refused.
        path: PathBuf,
        /// What is wrong with it.
        why: String,
    },
    /// The profile holds no world to decide as.
    NoWorld {
        /// Where it was looked for.
        path: PathBuf,
    },
    /// The world file is not one this build reads.
    WorldUnreadable {
        /// The file refused.
        path: PathBuf,
        /// What is wrong with it.
        why: String,
    },
    /// Neither slot of the identifiers' mark is whole.
    MarkUnreadable {
        /// The mark file.
        path: PathBuf,
    },
    /// The mark is one the minter refuses to resume at: further ahead of the
    /// clock than any real skew.
    Mark(nc_ids::MintError),
    /// Another daemon of the channel holds its profile.
    Held {
        /// The state directory it holds.
        path: PathBuf,
    },
    /// The install names a central log the daemon cannot speak to.
    NoCentralLog {
        /// Why `nc_log` refuses it.
        why: String,
    },
    /// The kernel's engine refused its configuration.
    Engine(String),
    /// The local database did not open, by its own refusal.
    Store(nc_local::LocalError),
    /// The local journal could not say which unit is its newest.
    Journal(nc_journal::Error),
    /// The blobs' directory was refused, by its own refusal.
    Blobs(nc_kernel::blobs::BlobError),
    /// A unit a stopped process left open was not recovered, so the kernel did
    /// not open.
    Unrecovered(nc_kernel::Refusal),
    /// The endpoint was not bound, by its own refusal.
    Endpoint(nc_ipc::IpcError),
    /// The PID record is not one this build wrote.
    PidRecord {
        /// The record refused.
        path: PathBuf,
        /// What is wrong with it.
        why: String,
    },
}

impl Refused {
    /// The catalog code this maps to, which is what a log is queried by.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::Install(refused) => refused.code(),
            Self::NotPrivate(_) => ErrorCode::DaemonProfileNotPrivate,
            Self::Unavailable { .. } => ErrorCode::DaemonProfileUnavailable,
            Self::NoDevice { .. } => ErrorCode::DaemonNoDevice,
            Self::DeviceUnreadable { .. } => ErrorCode::DaemonDeviceUnreadable,
            Self::NoWorld { .. } => ErrorCode::DaemonNoWorld,
            Self::WorldUnreadable { .. } => ErrorCode::DaemonWorldUnreadable,
            Self::MarkUnreadable { .. } => ErrorCode::DaemonMarkUnreadable,
            Self::Mark(refused) => refused.code(),
            Self::Held { .. } => ErrorCode::DaemonInstanceHeld,
            Self::NoCentralLog { .. } => ErrorCode::DaemonNoCentralLog,
            Self::Engine(_) => ErrorCode::DaemonEngineRefused,
            Self::Store(refused) => refused.code(),
            Self::Journal(refused) => refused.code(),
            Self::Blobs(refused) => refused.code(),
            Self::Unrecovered(refusal) => refusal.code(),
            Self::Endpoint(refused) => refused.code(),
            Self::PidRecord { .. } => ErrorCode::DaemonPidRecordUnreadable,
        }
    }

    /// `doing`, which the filesystem answered with `error`.
    fn unavailable(doing: String, error: std::io::Error) -> Self {
        Self::Unavailable { doing, error }
    }

    /// A path judged and not taken: a refusal, or the filesystem's failure.
    fn judged(judged: nc_platform::Judged) -> Self {
        match judged {
            nc_platform::Judged::Refused(refused) => Self::NotPrivate(refused),
            nc_platform::Judged::Failed(failed) => {
                Self::unavailable(failed.doing().to_owned(), clone_of(failed.error()))
            }
        }
    }
}

/// An error the filesystem answered, as a value this crate keeps.
///
/// `std::io::Error` is not `Clone`, and the kind and the sentence are what a
/// refusal shows.
fn clone_of(error: &std::io::Error) -> std::io::Error {
    std::io::Error::new(error.kind(), error.to_string())
}

impl core::fmt::Display for Refused {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The shared reader's refusal says its code itself.
        if let Self::Install(refused) = self {
            return write!(formatter, "{refused}");
        }
        write!(formatter, "[{}] ", self.code().code())?;
        match self {
            Self::Install(_) => Ok(()),
            Self::NotPrivate(refused) => write!(
                formatter,
                "{refused}, so the profile was not used; it is left as it was"
            ),
            Self::Unavailable { doing, error } => write!(formatter, "{doing}: {error}"),
            Self::NoDevice { path } => write!(
                formatter,
                "`{}` is missing: the profile holds no device, and the daemon never makes \
                 one; the installer's `--initialize` does",
                path.display()
            ),
            Self::DeviceUnreadable { path, why } => write!(
                formatter,
                "`{}` was refused as the device's: {why}",
                path.display()
            ),
            Self::NoWorld { path } => write!(
                formatter,
                "`{}` is missing: the profile names no realm and no person to decide as, \
                 and the daemon does not start without one",
                path.display()
            ),
            Self::WorldUnreadable { path, why } => write!(
                formatter,
                "`{}` was refused as the profile's world: {why}",
                path.display()
            ),
            Self::MarkUnreadable { path } => write!(
                formatter,
                "neither slot of `{}` is whole, so the daemon cannot know which identifiers \
                 it handed out, and does not start rather than repeat one",
                path.display()
            ),
            Self::Mark(refused) => write!(formatter, "{refused}"),
            Self::Held { path } => write!(
                formatter,
                "another daemon of this channel holds `{}`, so this one did not start: one \
                 daemon keeps one profile",
                path.display()
            ),
            Self::NoCentralLog { why } => write!(
                formatter,
                "{why}, so the daemon did not start rather than send its lines somewhere the \
                 install did not name; install it with the installer, which names one on \
                 this machine or none"
            ),
            Self::Engine(why) => write!(formatter, "the kernel did not open: {why}"),
            Self::Store(refused) => write!(formatter, "the local database did not open: {refused}"),
            Self::Journal(refused) => write!(
                formatter,
                "the local journal could not say which unit is its newest: {refused}"
            ),
            Self::Blobs(refused) => write!(formatter, "the blobs' directory: {refused}"),
            Self::Unrecovered(refusal) => write!(
                formatter,
                "the kernel did not open, because a unit a stopped process left open was not \
                 recovered: {refusal}"
            ),
            Self::Endpoint(refused) => write!(formatter, "the endpoint was not bound: {refused}"),
            Self::PidRecord { path, why } => write!(
                formatter,
                "`{}` was refused as the daemon's PID record: {why}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for Refused {}

/// What `--initialize` made or found.
#[derive(Debug)]
pub struct Initialized {
    /// Every name the install gives the system.
    pub layout: nc_channel::Layout,
    /// The device the profile holds now.
    pub device: device::Device,
    /// Whether a world was copied in.
    pub world: bool,
}

/// The installer's entry: the profile beside `executable` prepared, its
/// device created when it has none, and `world` copied in when one is handed.
///
/// # Errors
///
/// [`Refused`], each one the install, the profile or the device answers
/// with.
pub fn initialize(executable: &Path, world: Option<&Path>) -> Result<Initialized, Refused> {
    let found = nc_channel::found::beside(executable).map_err(Refused::Install)?;
    profile::prepare(&found.layout, &found.user)?;
    let device = device::Device::initialize(&found.layout, &found.user)?;
    if let Some(from) = world {
        profile::copy_world(from, &found.layout, &found.user)?;
    }
    Ok(Initialized {
        layout: found.layout,
        device,
        world: world.is_some(),
    })
}
