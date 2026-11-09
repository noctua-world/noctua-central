//! Which install a binary is: the manifest beside its canonicalized
//! executable, judged and read, and every name it derives (`NOCTUA.md §17.1`).
//!
//! **Kernel-free, beside the parser**, so a binary that holds no kernel reads
//! its install as one that does: the manifest beside the file that runs,
//! judged and refused with the `CHAN` area's codes, and the names it derives
//! are [`layout`]'s for the user that directory is private to.
//!
//! **Canonicalized first**, so a binary reached through a link reads the
//! manifest beside the file that runs, never one beside the link — a link is
//! whatever its owner points it at. **Judged before it is read**: the
//! directory and the manifest must be this user's alone, by
//! `nc_platform::user_in` and `nc_platform::judge`, because the manifest names
//! every directory the profile is kept in, so one another account could write
//! would choose where this user's device seed, world and token are read from.
//! **And no fallback**: a binary with no manifest beside it refuses with
//! `NC-CHAN-0006`, because a channel built in would make every binary a build
//! leaves behind a live client of the developer's own channel.
//!
//! # What this does not answer
//!
//! A process running as the same user, which can rewrite the manifest or
//! replace the executable it sits beside — judging the directory this user's
//! alone stops other accounts only.

use std::path::{Path, PathBuf};

use nc_catalog::ErrorCode;
use nc_platform::{Judged, Kind, User};

use crate::{ChannelError, Install, Layout, MANIFEST, layout};

/// An install found beside an executable: what its manifest says, who runs
/// it, and every name that derives.
#[derive(Debug, Clone)]
pub struct Found {
    /// What the manifest says.
    pub install: Install,
    /// The user the install's directory is private to, which runs it.
    pub user: User,
    /// Every name the install gives the system, for that user.
    pub layout: Layout,
}

/// Why no install was found beside an executable.
#[derive(Debug)]
pub enum Refused {
    /// No manifest is beside the executable.
    NoManifest {
        /// Where it was looked for.
        path: PathBuf,
    },
    /// The manifest, or the directory holding it, is not this user's alone.
    NotPrivate(nc_platform::Refusal),
    /// The manifest says something this build does not read.
    Manifest(ChannelError),
    /// The filesystem did not answer for the executable, its directory or the
    /// manifest.
    Unavailable {
        /// What was being done, naming the path.
        doing: String,
        /// What the system answered.
        error: std::io::Error,
    },
}

impl Refused {
    /// The catalog code this maps to, which is what a log is queried by.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::NoManifest { .. } => ErrorCode::ChannelNoManifest,
            Self::NotPrivate(_) => ErrorCode::ChannelManifestNotPrivate,
            Self::Manifest(refused) => refused.code(),
            Self::Unavailable { .. } => ErrorCode::ChannelManifestUnavailable,
        }
    }

    /// `doing`, which the filesystem answered with `error`.
    fn unavailable(doing: String, error: std::io::Error) -> Self {
        Self::Unavailable { doing, error }
    }

    /// The manifest, or the directory holding it, judged and not taken.
    fn judged(judged: Judged) -> Self {
        match judged {
            Judged::Refused(refused) => Self::NotPrivate(refused),
            Judged::Failed(failed) => Self::unavailable(
                failed.doing().to_owned(),
                std::io::Error::new(failed.error().kind(), failed.error().to_string()),
            ),
        }
    }
}

impl core::fmt::Display for Refused {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            // The manifest's own refusal says its code itself.
            Self::Manifest(refused) => write!(formatter, "{refused}"),
            Self::NoManifest { path } => write!(
                formatter,
                "[{}] no install manifest is at `{}`, so this binary names no channel and \
                 does not run; install it with the installer that writes one",
                self.code().code(),
                path.display()
            ),
            Self::NotPrivate(refused) => write!(
                formatter,
                "[{}] {refused}, so the install manifest was not read: another account \
                 could have written it",
                self.code().code()
            ),
            Self::Unavailable { doing, error } => {
                write!(formatter, "[{}] {doing}: {error}", self.code().code())
            }
        }
    }
}

impl std::error::Error for Refused {}

/// The install of the process running now.
///
/// # Errors
///
/// [`Refused::Unavailable`] when the system cannot say which executable this
/// is, and every refusal [`beside`] answers with.
pub fn of_this_process() -> Result<Found, Refused> {
    let executable = std::env::current_exe().map_err(|error| {
        Refused::unavailable(
            "the system could not say which executable this is".to_owned(),
            error,
        )
    })?;
    beside(&executable)
}

/// The install whose manifest is beside `executable`, once both are judged.
///
/// # Errors
///
/// [`Refused::NoManifest`] for no manifest there, [`Refused::NotPrivate`] for
/// one, or a directory holding it, that is not this user's alone,
/// [`Refused::Manifest`] for one [`Install::parse`] refuses, and
/// [`Refused::Unavailable`] when a path could not be read.
pub fn beside(executable: &Path) -> Result<Found, Refused> {
    let canonical = std::fs::canonicalize(executable).map_err(|error| {
        Refused::unavailable(
            format!("`{}` could not be canonicalized", executable.display()),
            error,
        )
    })?;
    let directory = canonical.parent().ok_or_else(|| {
        Refused::unavailable(
            format!("`{}` has no directory", canonical.display()),
            std::io::Error::from(std::io::ErrorKind::NotFound),
        )
    })?;
    let user = nc_platform::user_in(directory).map_err(Refused::judged)?;
    let manifest = directory.join(MANIFEST);
    match nc_platform::judge(&manifest, Kind::File, &user) {
        Ok(()) => {}
        Err(Judged::Failed(failed)) if failed.error().kind() == std::io::ErrorKind::NotFound => {
            return Err(Refused::NoManifest { path: manifest });
        }
        Err(judged) => return Err(Refused::judged(judged)),
    }
    let text = std::fs::read_to_string(&manifest).map_err(|error| {
        Refused::unavailable(format!("`{}` could not be read", manifest.display()), error)
    })?;
    let install = Install::parse(&text).map_err(Refused::Manifest)?;
    let layout = layout(&install, &user.to_string());
    Ok(Found {
        install,
        user,
        layout,
    })
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use nc_catalog::ErrorCode;

    /// A scratch directory for one test, removed however the test leaves.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = nc_platform::testing::make_private(&self.0);
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl Scratch {
        /// A private directory of its own, holding a stand-in executable.
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir()
                .join(format!("nc-channel-found-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            nc_platform::create_private_directory(&root).expect("a scratch directory");
            nc_platform::create_private(&root.join("noctua")).expect("a stand-in binary");
            Self(root)
        }

        fn executable(&self) -> PathBuf {
            self.0.join("noctua")
        }

        /// A manifest beside the executable, private as an installer writes one.
        fn manifest(&self, text: &str) {
            use std::io::Write as _;
            let mut file =
                nc_platform::create_private(&self.0.join(crate::MANIFEST)).expect("a manifest");
            file.write_all(text.as_bytes()).expect("written");
        }

        /// A manifest under this scratch's own bases, relocated by a
        /// namespace, as an installer writes one.
        fn a_good_manifest(&self) -> String {
            let base = |name: &str| self.0.join(name);
            crate::Install {
                channel: crate::Channel::Staging,
                bases: crate::Bases::new(
                    base("data"),
                    base("config"),
                    base("cache"),
                    base("state"),
                )
                .expect("the scratch's bases are plain absolute paths"),
                namespace: Some(crate::Namespace::parse("t1").expect("a namespace")),
                log_endpoint: None,
            }
            .to_canonical()
        }
    }

    fn refused(executable: &Path) -> super::Refused {
        super::beside(executable).expect_err("the install is refused")
    }

    /// The manifest beside the executable is the install, and every name
    /// derives from it for the user its directory is private to.
    #[test]
    fn the_manifest_beside_the_executable_is_the_install() {
        let scratch = Scratch::new("found");
        scratch.manifest(&scratch.a_good_manifest());
        let found = super::beside(&scratch.executable()).expect("the install is read");
        assert_eq!(found.install.channel, crate::Channel::Staging);
        assert_eq!(
            found.layout,
            crate::layout(&found.install, &found.user.to_string())
        );
        assert!(
            found.layout.data.starts_with(&scratch.0),
            "a relocated install's data is under its own base"
        );
    }

    /// A binary reached through a link reads the manifest beside the file that
    /// runs: the link is followed to it before anything is judged.
    #[test]
    fn a_binary_reached_through_a_link_reads_the_manifest_beside_the_file_that_runs() {
        let scratch = Scratch::new("runs");
        scratch.manifest(&scratch.a_good_manifest());
        let elsewhere = Scratch::new("elsewhere");
        let through = elsewhere.0.join("through");
        nc_platform::testing::link_directory(&scratch.0, &through).expect("a link");
        let found = super::beside(&through.join("noctua"))
            .expect("the install beside the file that runs is read");
        let direct = super::beside(&scratch.executable()).expect("the install is read");
        assert_eq!(found.install, direct.install);
        assert_eq!(found.layout, direct.layout);
        assert!(found.layout.data.starts_with(&scratch.0));
    }

    /// A binary with no manifest beside it refuses, naming where it looked,
    /// with its code in what it says.
    #[test]
    fn no_manifest_is_refused_and_no_channel_is_guessed() {
        let scratch = Scratch::new("missing");
        let refusal = refused(&scratch.executable());
        assert_eq!(refusal.code(), ErrorCode::ChannelNoManifest, "{refusal}");
        let said = refusal.to_string();
        assert!(said.contains(crate::MANIFEST), "{said}");
        assert!(
            said.starts_with(&format!("[{}] ", refusal.code().code())),
            "{said}"
        );
    }

    /// A manifest another account can reach is not read.
    #[test]
    fn a_manifest_another_user_can_reach_is_refused() {
        let scratch = Scratch::new("reachable");
        scratch.manifest(&scratch.a_good_manifest());
        nc_platform::testing::open_to_another_user(&scratch.0.join(crate::MANIFEST))
            .expect("the manifest is opened to another user");
        let refusal = refused(&scratch.executable());
        assert_eq!(
            refusal.code(),
            ErrorCode::ChannelManifestNotPrivate,
            "{refusal}"
        );
        assert!(
            refusal
                .to_string()
                .starts_with(&format!("[{}] ", refusal.code().code())),
            "{refusal}"
        );
    }

    /// A manifest in a directory another account can reach is not read, and
    /// the directory is left holding what it held, as it was.
    #[test]
    fn a_manifest_in_a_directory_another_user_can_reach_is_refused() {
        let scratch = Scratch::new("open");
        scratch.manifest(&scratch.a_good_manifest());
        let opened = nc_platform::testing::open_to_another_user(&scratch.0)
            .expect("the install's directory is opened to another user");
        let refusal = refused(&scratch.executable());
        assert_eq!(
            refusal.code(),
            ErrorCode::ChannelManifestNotPrivate,
            "{refusal}"
        );
        assert!(refusal.to_string().contains(&opened), "{opened}: {refusal}");
        let mut held: Vec<String> = std::fs::read_dir(&scratch.0)
            .expect("the directory is listed")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        held.sort();
        assert_eq!(
            held,
            [crate::MANIFEST, "noctua"],
            "something was made in a directory that was refused"
        );
        assert!(
            !nc_platform::testing::is_private(&scratch.0),
            "the directory is left as it was"
        );
    }

    /// A manifest [`crate::Install::parse`] refuses is refused with its code.
    #[test]
    fn a_manifest_the_channel_refuses_is_refused_with_its_code() {
        let scratch = Scratch::new("malformed");
        scratch.manifest("{\"format\":1}");
        let refusal = refused(&scratch.executable());
        assert_eq!(
            refusal.code(),
            ErrorCode::ChannelManifestUnreadable,
            "{refusal}"
        );
        assert!(
            refusal
                .to_string()
                .starts_with(&format!("[{}] ", refusal.code().code())),
            "{refusal}"
        );
    }

    /// An executable the filesystem cannot find is the filesystem's answer,
    /// not a missing manifest: nothing was beside it to look for.
    #[test]
    fn an_executable_the_filesystem_cannot_find_is_its_answer() {
        let scratch = Scratch::new("gone");
        scratch.manifest(&scratch.a_good_manifest());
        let refusal = refused(&scratch.0.join("not-here"));
        assert_eq!(
            refusal.code(),
            ErrorCode::ChannelManifestUnavailable,
            "{refusal}"
        );
        assert!(
            refusal
                .to_string()
                .starts_with(&format!("[{}] ", refusal.code().code())),
            "{refusal}"
        );
    }
}
