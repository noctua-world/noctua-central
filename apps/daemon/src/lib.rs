//! The daemon a channel runs: what names it, the profile it keeps, the device
//! it mints identifiers on, and the mark that keeps those identifiers unique
//! across a restart (`NOCTUA.md §11.1`, `§16.2`, `§17.1`).
//!
//! # What a start reads, in order
//!
//! - **The install** ([`install`]): the manifest beside the canonicalized
//!   executable, judged this user's alone and read by
//!   `nc_channel::Install::parse`. There is no channel built in to fall back to:
//!   a binary with no manifest refuses before it reads anything else.
//! - **The profile** ([`profile`]): every directory the install's layout
//!   names, created private one level at a time or judged so, and the world
//!   file the daemon decides as — its realm and its person (`§5.13`).
//! - **The device** ([`device`]): the seed the installer drew and the flake
//!   derived from it. The daemon reads them and never creates them.
//! - **The mark** ([`mark`]): the ceiling no identifier is minted past before
//!   it is durable, which a start resumes above and waits behind.
//!
//! `noctua-daemon --initialize` is the installer's entry into the same
//! modules: it prepares the profile, creates the device when there is none
//! and copies a world in. Every refusal is a [`Refused`], which carries the
//! catalog code a log is queried by.
//!
//! # What this answers (`§2.4`)
//!
//! Another account on this machine renaming the install, reading the device's
//! seed, or rewriting the world or the mark: every one of them is a path
//! judged this user's alone before it is read, and every one this crate makes
//! is created private in one call. An identifier handed out twice across a
//! crash or a clock moved back: none is handed out past a ceiling that is not
//! yet durable, and a start resumes above the ceiling and the journal's newest
//! unit, waiting for the clock rather than minting behind it.
//!
//! # What it does not
//!
//! A process running as the same user, which can rewrite the manifest, the
//! world or the mark, or delete the mark so a start resumes from the journal
//! alone; and a disk that answers a sync before the bytes are on it.

#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
// Every doctest of this crate forbids `unsafe` as the crate does: a
// doctest is a crate of its own, outside the root's `forbid`.
#![doc(test(attr(forbid(unsafe_code))))]

pub mod device;
pub mod install;
pub mod mark;
pub mod profile;

use std::path::{Path, PathBuf};

use nc_catalog::ErrorCode;

/// Why the daemon did not start, or `--initialize` did not finish.
///
/// **One type for every module**, so a caller maps every refusal to its code
/// in one place, and each variant to the code its own failure is queried by.
#[derive(Debug)]
pub enum Refused {
    /// No install manifest is beside the executable.
    NoManifest {
        /// Where it was looked for.
        path: PathBuf,
    },
    /// The install manifest is not this user's alone.
    ManifestNotPrivate(nc_platform::Refusal),
    /// The install manifest says something this build does not read.
    Manifest(nc_channel::ChannelError),
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
}

impl Refused {
    /// The catalog code this maps to, which is what a log is queried by.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::NoManifest { .. } => ErrorCode::ChannelNoManifest,
            Self::ManifestNotPrivate(_) => ErrorCode::ChannelManifestNotPrivate,
            Self::Manifest(refused) => refused.code(),
            Self::NotPrivate(_) => ErrorCode::DaemonProfileNotPrivate,
            Self::Unavailable { .. } => ErrorCode::DaemonProfileUnavailable,
            Self::NoDevice { .. } => ErrorCode::DaemonNoDevice,
            Self::DeviceUnreadable { .. } => ErrorCode::DaemonDeviceUnreadable,
            Self::NoWorld { .. } => ErrorCode::DaemonNoWorld,
            Self::WorldUnreadable { .. } => ErrorCode::DaemonWorldUnreadable,
            Self::MarkUnreadable { .. } => ErrorCode::DaemonMarkUnreadable,
            Self::Mark(refused) => refused.code(),
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
        write!(formatter, "[{}] ", self.code().code())?;
        match self {
            Self::NoManifest { path } => write!(
                formatter,
                "no install manifest is at `{}`, so this binary names no channel and does \
                 not run; install it with the installer that writes one",
                path.display()
            ),
            Self::ManifestNotPrivate(refused) => write!(
                formatter,
                "{refused}, so the install manifest was not read: another account could \
                 have written it"
            ),
            Self::Manifest(refused) => write!(formatter, "{refused}"),
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
    let found = install::beside(executable)?;
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
