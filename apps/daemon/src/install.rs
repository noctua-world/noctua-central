//! Which install this binary is: the manifest beside its canonicalized
//! executable, judged and read, and every name it derives (`NOCTUA.md §17.1`).
//!
//! **Canonicalized first**, so a binary reached through a link reads the
//! manifest beside the file that runs, never one beside the link — a link is
//! whatever its owner points it at. **Judged before it is read**: the manifest
//! names every directory the profile is kept in, so one another account could
//! write would choose where this user's device seed and world are read from.
//! **And no fallback**: a binary with no manifest beside it refuses, with
//! `NC-CHAN-0006`, because a channel built in would make every binary a build
//! leaves behind a live client of the developer's own channel.

use std::path::Path;

use nc_channel::{Install, Layout};
use nc_platform::{Judged, Kind, User};

use crate::Refused;

/// An install found beside an executable: what its manifest says, who runs
/// it, and every name that derives.
#[derive(Debug)]
pub struct Found {
    /// What the manifest says.
    pub install: Install,
    /// The user the install's directory is private to, which runs it.
    pub user: User,
    /// Every name the install gives the system, for that user.
    pub layout: Layout,
}

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
/// [`Refused::NoManifest`] for no manifest there, [`Refused::ManifestNotPrivate`]
/// for one, or a directory holding it, that is not this user's alone,
/// [`Refused::Manifest`] for one `nc_channel::Install::parse` refuses, and
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
    let user = nc_platform::user_in(directory).map_err(manifest_judged)?;
    let manifest = directory.join(nc_channel::MANIFEST);
    match nc_platform::judge(&manifest, Kind::File, &user) {
        Ok(()) => {}
        Err(Judged::Failed(failed)) if failed.error().kind() == std::io::ErrorKind::NotFound => {
            return Err(Refused::NoManifest { path: manifest });
        }
        Err(judged) => return Err(manifest_judged(judged)),
    }
    let text = std::fs::read_to_string(&manifest).map_err(|error| {
        Refused::unavailable(format!("`{}` could not be read", manifest.display()), error)
    })?;
    let install = Install::parse(&text).map_err(Refused::Manifest)?;
    let layout = nc_channel::layout(&install, &user.to_string());
    Ok(Found {
        install,
        user,
        layout,
    })
}

/// The manifest, or the directory holding it, judged and not taken.
fn manifest_judged(judged: Judged) -> Refused {
    match judged {
        Judged::Refused(refused) => Refused::ManifestNotPrivate(refused),
        failed @ Judged::Failed(_) => Refused::judged(failed),
    }
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
                .join(format!("nc-daemon-install-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            nc_platform::create_private_directory(&root).expect("a scratch directory");
            nc_platform::create_private(&root.join("noctua-daemon")).expect("a stand-in binary");
            Self(root)
        }

        fn executable(&self) -> PathBuf {
            self.0.join("noctua-daemon")
        }

        /// A manifest beside the executable, private as an installer writes one.
        fn manifest(&self, text: &str) {
            use std::io::Write as _;
            let mut file = nc_platform::create_private(&self.0.join(nc_channel::MANIFEST))
                .expect("a manifest");
            file.write_all(text.as_bytes()).expect("written");
        }

        /// A manifest under this scratch's own bases, relocated by a
        /// namespace, as an installer writes one.
        fn a_good_manifest(&self) -> String {
            let base = |name: &str| self.0.join(name);
            nc_channel::Install {
                channel: nc_channel::Channel::Staging,
                bases: nc_channel::Bases::new(
                    base("data"),
                    base("config"),
                    base("cache"),
                    base("state"),
                )
                .expect("the scratch's bases are plain absolute paths"),
                namespace: Some(nc_channel::Namespace::parse("t1").expect("a namespace")),
                log_endpoint: None,
            }
            .to_canonical()
        }
    }

    fn refused(executable: &Path) -> crate::Refused {
        super::beside(executable).expect_err("the install is refused")
    }

    /// The manifest beside the executable is the install, and every name
    /// derives from it for the user its directory is private to.
    #[test]
    fn the_manifest_beside_the_executable_is_the_install() {
        let scratch = Scratch::new("found");
        scratch.manifest(&scratch.a_good_manifest());
        let found = super::beside(&scratch.executable()).expect("the install is read");
        assert_eq!(found.install.channel, nc_channel::Channel::Staging);
        assert_eq!(
            found.layout,
            nc_channel::layout(&found.install, &found.user.to_string())
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
        let found = super::beside(&through.join("noctua-daemon"))
            .expect("the install beside the file that runs is read");
        let direct = super::beside(&scratch.executable()).expect("the install is read");
        assert_eq!(found.install, direct.install);
        assert_eq!(found.layout, direct.layout);
        assert!(found.layout.data.starts_with(&scratch.0));
    }

    /// A binary with no manifest beside it refuses, naming where it looked.
    #[test]
    fn no_manifest_is_refused_and_no_channel_is_guessed() {
        let scratch = Scratch::new("missing");
        let refusal = refused(&scratch.executable());
        assert_eq!(refusal.code(), ErrorCode::ChannelNoManifest, "{refusal}");
        assert!(
            refusal.to_string().contains(nc_channel::MANIFEST),
            "{refusal}"
        );
    }

    /// A manifest another account can reach is not read.
    #[test]
    fn a_manifest_another_user_can_reach_is_refused() {
        let scratch = Scratch::new("reachable");
        scratch.manifest(&scratch.a_good_manifest());
        nc_platform::testing::open_to_another_user(&scratch.0.join(nc_channel::MANIFEST))
            .expect("the manifest is opened to another user");
        let refusal = refused(&scratch.executable());
        assert_eq!(
            refusal.code(),
            ErrorCode::ChannelManifestNotPrivate,
            "{refusal}"
        );
    }

    /// A manifest in a directory another account can reach is not read, and
    /// nothing is made in the directory to find out who runs the binary.
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
            [nc_channel::MANIFEST, "noctua-daemon"],
            "something was made in a directory that was refused"
        );
        assert!(
            !nc_platform::testing::is_private(&scratch.0),
            "the directory is left as it was"
        );
    }

    /// A manifest `nc_channel::Install::parse` refuses is refused with its code.
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
    }
}
