//! `noctua-dev`: the one name a development install writes outside Noctua's
//! own directories, in the user's bin directory, reaching the CLI of the
//! development checkout installed or chosen last (`NOCTUA.md §17.1`).
//!
//! **How it reaches that CLI is the system's** (`nc_platform::REACH`), and
//! both ways are compiled and tested on every system:
//!
//! - **A symbolic link**, on Unix. It is re-pointed by a new link made under a
//!   temporary name beside it and renamed over it, so it names the old CLI or
//!   the new one at every moment, never neither. The CLI canonicalizes its own
//!   path before it reads its manifest, so it finds its install through the
//!   link.
//! - **A launcher**, on Windows, where a symbolic link needs a privilege a
//!   user may not hold: a copy of a CLI, and a private pointer beside it
//!   (`nc_channel::checkouts::Pointer`) naming the CLI it runs, which a
//!   re-pointing rewrites the same way. The copy is made when it is missing,
//!   and kept otherwise: a running program cannot be replaced there, and the
//!   launcher runs whatever the pointer names, so any build's copy does.
//!
//! # What this answers (`§2.4`)
//!
//! A name in the user's bin directory is run by whatever the person's `PATH`
//! finds, so **nothing this did not make is ever replaced or removed**: an
//! existing name is taken as this install's only when it is a link — or a
//! launcher whose pointer is this user's alone — to exactly the CLI
//! `nc_channel::layout` names for a development checkout of this install's
//! bases and namespace; anything else, a regular file or a link elsewhere
//! included, is refused, named, and left as it was. **And the directory must
//! be this user's**, and no other account may write in it
//! (`nc_platform::judge_written_by_user_alone`): one another account could add
//! to would let it choose what the person runs. A directory that is missing
//! is made, each level private.
//!
//! # What this does not answer
//!
//! A process running as this user, which can replace the name, the launcher
//! or its pointer between a judgement and a run; and what is earlier on the
//! person's `PATH` than the bin directory, which is theirs to order.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use nc_channel::checkouts::{Pointer, checkout_named, cli_of, pointer_beside};
use nc_channel::{Channel, Install, layout};
use nc_platform::{Judged, Kind, Reach, User};

/// Why `noctua-dev` was not pointed, or not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unpointed {
    /// The bin directory, or what stands at the name, was not this install's
    /// to change: refused, named, and left as it was.
    Refused(String),
    /// The filesystem did not answer, naming what was being done.
    Failed(String),
}

impl Unpointed {
    /// What was refused, or what failed.
    #[must_use]
    pub fn detail(&self) -> &str {
        let (Self::Refused(detail) | Self::Failed(detail)) = self;
        detail
    }
}

impl core::fmt::Display for Unpointed {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "{}", self.detail())
    }
}

impl std::error::Error for Unpointed {}

/// `doing` `path`, which the system answered with `error`.
fn failed(doing: &str, path: &Path, error: &impl core::fmt::Display) -> Unpointed {
    Unpointed::Failed(format!(
        "`{}` could not be {doing}: {error}",
        path.display()
    ))
}

/// What stands at `link` refused, saying what it is.
fn refused_at(link: &Path, what: &str) -> Unpointed {
    Unpointed::Refused(format!(
        "`{}` {what}, which this install did not make, so it was left as it was and \
         `noctua-dev` was not pointed: move it away, then install or choose again",
        link.display()
    ))
}

/// Where `noctua-dev` lives in the bin directory `bin`, for the development
/// install `install`: its name, as `nc_channel::layout` gives it, with this
/// system's executable suffix; nothing for an install of another channel.
#[must_use]
pub fn link_in(bin: &Path, install: &Install) -> Option<PathBuf> {
    let name = layout(install, "").chosen_cli?;
    Some(bin.join(format!("{name}{}", nc_platform::EXECUTABLE_SUFFIX)))
}

/// Whether `target` is exactly the CLI `nc_channel::layout` names for a
/// development checkout of `install`'s bases and namespace, installed or not.
///
/// **Read back from its own name**: the checkout its file name is the leaf
/// of, under this namespace, and then the whole path held to the one that
/// checkout's layout names.
#[must_use]
pub fn is_a_checkouts_cli(target: &Path, install: &Install) -> bool {
    let Some(stem) = target
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix(nc_platform::EXECUTABLE_SUFFIX))
    else {
        return false;
    };
    let Some(checkout) = checkout_named(stem, install.namespace.as_ref()) else {
        return false;
    };
    let of_it = Install {
        channel: Channel::Development(checkout),
        bases: install.bases.clone(),
        namespace: install.namespace.clone(),
        log_endpoint: None,
    };
    cli_of(&layout(&of_it, "")) == target
}

/// What the name at `link` points at now, as `reach` makes one, for the
/// user `user`: nothing when nothing is there.
///
/// # Errors
///
/// [`Unpointed::Refused`] for a name that is not a link, or a launcher whose
/// pointer is missing, not this user's alone or does not read; and
/// [`Unpointed::Failed`] when the disk did not answer.
pub fn pointed(link: &Path, reach: Reach, user: &User) -> Result<Option<PathBuf>, Unpointed> {
    match reach {
        Reach::Link => match std::fs::symlink_metadata(link) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(failed("read", link, &error)),
            Ok(found) if found.file_type().is_symlink() => std::fs::read_link(link)
                .map(Some)
                .map_err(|error| failed("read", link, &error)),
            Ok(_) => Err(refused_at(link, "is not a symbolic link")),
        },
        Reach::Launcher => {
            let pointer = pointer_beside(link);
            let launcher = std::fs::symlink_metadata(link);
            match nc_channel::checkouts::read_bounded(&pointer, user) {
                Err(what) => Err(refused_at(&pointer, &format!("was not read: {what}"))),
                Ok(None) => match launcher {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                    Err(error) => Err(failed("read", link, &error)),
                    Ok(_) => Err(refused_at(link, "is there with no pointer beside it")),
                },
                Ok(Some(text)) => {
                    let pointer = Pointer::parse(&text).map_err(|what| {
                        refused_at(&pointer, &format!("does not read as a pointer: {what}"))
                    })?;
                    match launcher {
                        Ok(found) if found.file_type().is_file() => Ok(Some(pointer.cli)),
                        Ok(_) => Err(refused_at(link, "is not a regular file")),
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                            Ok(Some(pointer.cli))
                        }
                        Err(error) => Err(failed("read", link, &error)),
                    }
                }
            }
        }
    }
}

/// The directory `bin`, made private where it or a parent is missing, then
/// judged this user's with no other account able to write in it.
///
/// # Errors
///
/// [`Unpointed::Refused`] for a directory another account owns or may write
/// in, a link or a file; [`Unpointed::Failed`] for one that could not be made
/// or read.
pub fn bin_judged(bin: &Path, user: &User) -> Result<(), Unpointed> {
    let mut missing = Vec::new();
    let mut at = bin;
    while std::fs::symlink_metadata(at).is_err() {
        missing.push(at);
        at = at.parent().ok_or_else(|| {
            Unpointed::Failed(format!("`{}` has no parent that is there", bin.display()))
        })?;
    }
    for directory in missing.into_iter().rev() {
        nc_platform::create_private_directory(directory)
            .map_err(|error| failed("made", directory, &error))?;
    }
    nc_platform::judge_written_by_user_alone(bin, user).map_err(|judged| match judged {
        Judged::Refused(refused) => Unpointed::Refused(format!(
            "{refused}, so nothing was written in it and `noctua-dev` was not pointed: a \
             person's `PATH` runs what is there"
        )),
        Judged::Failed(failure) => Unpointed::Failed(failure.to_string()),
    })
}

/// Whether the name at `link` may be pointed anew by `install`: nothing
/// there, or a link or a launcher to a checkout's CLI of its bases and
/// namespace, once its directory is judged.
///
/// # Errors
///
/// What [`bin_judged`] and [`pointed`] answer, and [`Unpointed::Refused`] for
/// a name pointing anywhere else.
pub fn may_point(
    link: &Path,
    install: &Install,
    reach: Reach,
    user: &User,
) -> Result<(), Unpointed> {
    let bin = link
        .parent()
        .ok_or_else(|| Unpointed::Failed(format!("`{}` has no directory", link.display())))?;
    bin_judged(bin, user)?;
    match pointed(link, reach, user)? {
        None => Ok(()),
        Some(target) if is_a_checkouts_cli(&target, install) => Ok(()),
        Some(target) => Err(refused_at(
            link,
            &format!(
                "points at `{}`, which is no development checkout's CLI here",
                target.display()
            ),
        )),
    }
}

/// `noctua-dev` at `link` pointed at the CLI `cli`, as `reach` makes one; a
/// launcher missing is copied from `launcher`.
///
/// **Judged first, as [`may_point`] judges it**, and written through a new
/// file beside it, synced and renamed over the old one.
///
/// # Errors
///
/// What [`may_point`] answers, and [`Unpointed::Failed`] for a file that
/// could not be written, synced or renamed.
pub fn point(
    link: &Path,
    cli: &Path,
    install: &Install,
    reach: Reach,
    launcher: &Path,
    user: &User,
) -> Result<(), Unpointed> {
    may_point(link, install, reach, user)?;
    match reach {
        Reach::Link => {
            let beside = beside(link);
            cleared(&beside, true, user)?;
            nc_platform::link(cli, &beside).map_err(|error| failed("made", &beside, &error))?;
            renamed(&beside, link)
        }
        Reach::Launcher => {
            if std::fs::symlink_metadata(link).is_err() {
                let bytes =
                    std::fs::read(launcher).map_err(|error| failed("read", launcher, &error))?;
                written(link, &bytes, true, user)?;
            }
            let pointer = Pointer {
                cli: cli.to_path_buf(),
            };
            written(
                &pointer_beside(link),
                pointer.to_canonical().as_bytes(),
                false,
                user,
            )
        }
    }
}

/// `noctua-dev` at `link` removed, as `reach` made it, once it is judged this
/// install's to remove.
///
/// # Errors
///
/// What [`may_point`] answers, and [`Unpointed::Failed`] for a file that could
/// not be removed — on Windows, a launcher that runs now, whose pointer is
/// removed all the same.
pub fn removed(link: &Path, install: &Install, reach: Reach, user: &User) -> Result<(), Unpointed> {
    may_point(link, install, reach, user)?;
    if reach == Reach::Launcher {
        gone(&pointer_beside(link))?;
    }
    gone(link)
}

/// The file at `path` removed, when it is there.
fn gone(path: &Path) -> Result<(), Unpointed> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(failed("removed", path, &error)),
    }
}

/// The temporary name beside `path` a new file or link is made under.
fn beside(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_owned();
    name.push(".new");
    path.with_file_name(name)
}

/// What an interrupted write left at `beside` removed: a link, where `link`
/// says one is what it left, or a file this user's alone.
fn cleared(beside: &Path, link: bool, user: &User) -> Result<(), Unpointed> {
    let left = match std::fs::symlink_metadata(beside) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(failed("read", beside, &error)),
        Ok(found) => found,
    };
    if link {
        if !left.file_type().is_symlink() {
            return Err(refused_at(beside, "is not a symbolic link"));
        }
    } else {
        nc_platform::judge(beside, Kind::File, user).map_err(|judged| match judged {
            Judged::Refused(refused) => refused_at(beside, &format!("was refused: {refused}")),
            Judged::Failed(failure) => Unpointed::Failed(failure.to_string()),
        })?;
    }
    std::fs::remove_file(beside).map_err(|error| failed("removed", beside, &error))
}

/// `bytes` written at `path` through a private file beside it, runnable
/// when `executable`, synced and renamed over it.
fn written(path: &Path, bytes: &[u8], executable: bool, user: &User) -> Result<(), Unpointed> {
    let beside = beside(path);
    cleared(&beside, false, user)?;
    let mut file = if executable {
        nc_platform::create_private_executable(&beside)
    } else {
        nc_platform::create_private(&beside)
    }
    .map_err(|error| failed("made", &beside, &error))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| failed("written", &beside, &error))?;
    drop(file);
    renamed(&beside, path)
}

/// `from` renamed over `to`, and their directory synced.
fn renamed(from: &Path, to: &Path) -> Result<(), Unpointed> {
    std::fs::rename(from, to).map_err(|error| failed("renamed over", to, &error))?;
    if let Some(directory) = to.parent() {
        nc_platform::sync_directory(directory)
            .map_err(|error| failed("synced", directory, &error))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use nc_channel::checkouts::{cli_of, pointer_beside};
    use nc_channel::{Bases, Channel, Checkout, Install, Namespace, layout};
    use nc_platform::Reach;

    use super::Unpointed;

    /// A scratch home for one test, removed however the test leaves.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = nc_platform::testing::make_private(&self.0);
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl Scratch {
        fn new(tag: &str) -> Self {
            let root = std::env::temp_dir().join(format!("nc-chosen-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            nc_platform::create_private_directory(&root).expect("a scratch directory");
            Self(root)
        }

        /// The development install of `digest` relocated under `namespace`,
        /// its bases under this scratch.
        fn install(&self, digest: &str, namespace: &str) -> Install {
            let base = |name: &str| self.0.join("home").join(name);
            Install {
                channel: Channel::Development(Checkout::parse(digest).expect("a digest")),
                bases: Bases::new(base("d"), base("c"), base("k"), base("s")).expect("plain"),
                namespace: Some(Namespace::parse(namespace).expect("a namespace")),
                log_endpoint: None,
            }
        }

        /// A file holding `text`, made private.
        fn file(&self, name: &str, text: &str) -> PathBuf {
            use std::io::Write as _;
            let path = self.0.join(name);
            nc_platform::create_private(&path)
                .and_then(|mut file| file.write_all(text.as_bytes()))
                .expect("a file");
            path
        }
    }

    fn user(scratch: &Scratch) -> nc_platform::User {
        nc_platform::user_in(&scratch.0).expect("private")
    }

    /// The ways a name reaches its CLI that this system can make: a launcher
    /// on every system, and a link where links are how it reaches one.
    fn reaches() -> Vec<Reach> {
        let mut reaches = vec![Reach::Launcher];
        if nc_platform::REACH == Reach::Link {
            reaches.push(Reach::Link);
        }
        reaches
    }

    /// `noctua-dev` is made pointing at a checkout's CLI, pointed again at
    /// another's, and removed — as a link and as a launcher, whose copy is made
    /// once and whose pointer is rewritten.
    #[test]
    fn noctua_dev_is_made_pointed_again_and_removed() {
        for reach in reaches() {
            let scratch = Scratch::new(&format!("made-{reach:?}"));
            let user = user(&scratch);
            let one = scratch.install("0000000a", "tc1");
            let other = scratch.install("0000000b", "tc1");
            let (cli, next) = (cli_of(&layout(&one, "")), cli_of(&layout(&other, "")));
            let launcher = scratch.file("launcher", "the CLI's bytes");
            let bin = scratch.0.join("home").join(".local").join("bin");
            let link = super::link_in(&bin, &one).expect("a development install's");
            assert_eq!(
                link.file_name().and_then(|name| name.to_str()),
                Some(format!("noctua-dev-tc1{}", nc_platform::EXECUTABLE_SUFFIX).as_str())
            );
            assert_eq!(super::link_in(&bin, &other), Some(link.clone()), "one name");
            super::point(&link, &cli, &one, reach, &launcher, &user).expect("pointed");
            nc_platform::judge_written_by_user_alone(&bin, &user).expect("made private");
            assert_eq!(super::pointed(&link, reach, &user), Ok(Some(cli.clone())));
            super::point(&link, &next, &one, reach, &launcher, &user).expect("pointed again");
            assert_eq!(super::pointed(&link, reach, &user), Ok(Some(next.clone())));
            if reach == Reach::Launcher {
                assert_eq!(
                    std::fs::read_to_string(&link).expect("the launcher"),
                    "the CLI's bytes"
                );
                nc_platform::judge(&pointer_beside(&link), nc_platform::Kind::File, &user)
                    .expect("its pointer is private");
            } else {
                assert_eq!(std::fs::read_link(&link).expect("a link"), next);
            }
            assert!(
                !bin.join(format!(
                    "{}.new",
                    link.file_name().and_then(|n| n.to_str()).expect("a name")
                ))
                .exists()
            );
            super::removed(&link, &other, reach, &user).expect("removed");
            assert_eq!(super::pointed(&link, reach, &user), Ok(None));
            assert!(!pointer_beside(&link).exists());
        }
    }

    /// A name this install did not make is refused and left as it was.
    ///
    /// A regular file, a link elsewhere, a launcher with no pointer, and one
    /// pointing at another install's CLI.
    #[test]
    fn a_name_this_install_did_not_make_is_refused_and_left_as_it_was() {
        for reach in reaches() {
            let scratch = Scratch::new(&format!("theirs-{reach:?}"));
            let user = user(&scratch);
            let ours = scratch.install("0000000a", "tc2");
            let elsewhere = scratch.install("0000000a", "tc3");
            let cli = cli_of(&layout(&ours, ""));
            let launcher = scratch.file("launcher", "bytes");
            let bin = scratch.0.join("bin");
            nc_platform::create_private_directory(&bin).expect("a bin directory");
            let link = super::link_in(&bin, &ours).expect("a name");
            let held = |what: &str| {
                let refused =
                    super::point(&link, &cli, &ours, reach, &launcher, &user).expect_err("refused");
                assert!(
                    matches!(&refused, Unpointed::Refused(said) if said.contains(&link.display().to_string()) || said.contains("pointer")),
                    "{what}: {refused}"
                );
                refused
            };
            std::fs::write(&link, "a person's own script").expect("a regular file");
            if reach == Reach::Link {
                held("a regular file");
            } else {
                held("a launcher with no pointer");
            }
            assert_eq!(
                std::fs::read_to_string(&link).expect("left"),
                "a person's own script"
            );
            std::fs::remove_file(&link).expect("removed");
            let theirs = cli_of(&layout(&elsewhere, ""));
            match reach {
                Reach::Link => {
                    nc_platform::link(&theirs, &link).expect("a link elsewhere");
                }
                Reach::Launcher => {
                    std::fs::write(&link, "bytes").expect("a launcher");
                    let pointer = nc_channel::checkouts::Pointer {
                        cli: theirs.clone(),
                    };
                    std::fs::write(pointer_beside(&link), pointer.to_canonical())
                        .expect("a pointer");
                    nc_platform::testing::make_private(&pointer_beside(&link)).expect("private");
                }
            }
            let refused = held("another namespace's CLI");
            assert!(
                refused.to_string().contains(&theirs.display().to_string()),
                "{refused}"
            );
            assert_eq!(
                super::pointed(&link, reach, &user),
                Ok(Some(theirs)),
                "it was changed"
            );
        }
    }

    /// A bin directory another account may write in is refused, and nothing
    /// is written in it.
    #[test]
    fn a_bin_directory_another_account_may_write_in_is_refused() {
        let scratch = Scratch::new("writable");
        let user = user(&scratch);
        let install = scratch.install("0000000a", "tc4");
        let bin = scratch.0.join("bin");
        nc_platform::create_private_directory(&bin).expect("a bin directory");
        let said = nc_platform::testing::let_another_user_write(&bin).expect("opened");
        let link = super::link_in(&bin, &install).expect("a name");
        let launcher = scratch.file("launcher", "bytes");
        for reach in reaches() {
            let refused = super::point(
                &link,
                &cli_of(&layout(&install, "")),
                &install,
                reach,
                &launcher,
                &user,
            )
            .expect_err("refused");
            assert!(
                matches!(&refused, Unpointed::Refused(detail) if detail.contains(&said)),
                "{refused}"
            );
        }
        assert_eq!(
            std::fs::read_dir(&bin).expect("listed").count(),
            0,
            "written in"
        );
    }

    /// A target is a checkout's CLI only when it is exactly the path the
    /// layout names for a checkout of this install's bases and namespace.
    #[test]
    fn a_target_is_a_checkouts_cli_only_where_the_layout_names_one() {
        let scratch = Scratch::new("named");
        let install = scratch.install("0000000a", "tc5");
        let another = scratch.install("0000000b", "tc5");
        assert!(super::is_a_checkouts_cli(
            &cli_of(&layout(&another, "")),
            &install
        ));
        let real = Install {
            namespace: None,
            ..install.clone()
        };
        let moved = Install {
            bases: Bases::new(
                scratch.0.join("x"),
                scratch.0.join("y"),
                scratch.0.join("z"),
                scratch.0.join("w"),
            )
            .expect("plain"),
            ..install.clone()
        };
        for (target, what) in [
            (
                cli_of(&layout(&real, "")),
                "a real install's, from a test's",
            ),
            (cli_of(&layout(&moved, "")), "another bases'"),
            (
                cli_of(&layout(&install, "")).with_file_name("noctua-dev-0000000a-tc5-daemon"),
                "the daemon",
            ),
            (Path::new("/usr/bin/sh").to_path_buf(), "a system's program"),
        ] {
            assert!(!super::is_a_checkouts_cli(&target, &install), "{what}");
        }
    }
}
