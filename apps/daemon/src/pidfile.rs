//! The PID record: which process the running daemon is, as the system says it
//! is, written where a client and a stop read it (`NOCTUA.md §17.1`).
//!
//! **A process, not a number.** The record holds the daemon's process id with
//! the boot it runs in, when it started and the executable it runs, read off
//! the system ([`nc_platform::process::this_process`]), and the instance it
//! drew at this start: a process id is handed out again once its process is
//! gone, so a record naming the id alone names whichever process holds it
//! next. A reader holds a live process to every field before it believes the
//! record is that process.
//!
//! **Written whole or not at all**: into a file beside it, created private,
//! synced, and renamed over the record, so a reader never meets half of one.
//! A file an interrupted write left beside it is removed only once it is
//! judged this user's alone. The same holds for the channel's token, which
//! [`replace_private`] writes the same way.
//!
//! **One format, the install's**: [`Record`] is `nc_channel::record`'s, the
//! reader the CLI holds a process to before it signals one, so the daemon and
//! the CLI read one file one way. It is read back only as it is written,
//! judged this user's alone first, and **a process numbered zero or below is
//! refused as a record**, never read as one: to a signal, zero is the caller's
//! own process group and `-1` every process the user owns (`NC-DMN-0013`).

use std::path::{Path, PathBuf};

use nc_platform::{Judged, Kind, User};

pub use nc_channel::record::Record;

use crate::Refused;

/// Writes `record` at `path`, whole, private, and in place of what was there.
///
/// # Errors
///
/// [`Refused::Unavailable`] when it could not be written, and
/// [`Refused::NotPrivate`] for a file left beside it that is not this user's
/// alone.
pub fn write(path: &Path, record: &Record, user: &User) -> Result<(), Refused> {
    let text = record.to_canonical().map_err(|why| {
        Refused::unavailable(
            format!("`{}` could not be written", path.display()),
            std::io::Error::new(std::io::ErrorKind::InvalidData, why),
        )
    })?;
    replace_private(path, &text, user)
}

/// The record at `path`, judged this user's alone and read.
///
/// # Errors
///
/// [`Refused::NotPrivate`] for one that is not this user's alone,
/// [`Refused::PidRecord`] for one that is not as this code writes it, and
/// [`Refused::Unavailable`] when it could not be read — a missing record
/// among them, with [`std::io::ErrorKind::NotFound`].
pub fn read(path: &Path, user: &User) -> Result<Record, Refused> {
    nc_channel::record::read(path, user).map_err(|unread| match unread {
        nc_channel::record::Unread::Missing => Refused::unavailable(
            format!("`{}` could not be read", path.display()),
            std::io::Error::from(std::io::ErrorKind::NotFound),
        ),
        nc_channel::record::Unread::NotPrivate(refused) => Refused::NotPrivate(refused),
        nc_channel::record::Unread::Malformed(why) => Refused::PidRecord {
            path: path.to_owned(),
            why,
        },
        nc_channel::record::Unread::Unavailable { doing, error } => {
            Refused::unavailable(doing, error)
        }
    })
}

/// Removes the record at `path` when it names `instance`, and leaves one
/// another start wrote — or anything else — as it was.
///
/// # Errors
///
/// [`Refused::Unavailable`] when it could not be removed.
pub fn remove_if_ours(path: &Path, instance: [u8; 16], user: &User) -> Result<(), Refused> {
    match read(path, user) {
        Ok(record) if record.instance == instance => std::fs::remove_file(path).map_err(|error| {
            Refused::unavailable(format!("`{}` could not be removed", path.display()), error)
        }),
        _ => Ok(()),
    }
}

/// Writes `text` at `path` through a private file beside it, synced and
/// renamed over it, and syncs the directory, so `path` holds all of it or
/// what it held before.
///
/// **A file an interrupted write left beside it** is removed once it is
/// judged this user's alone; one another account can reach is refused, left
/// as it was.
///
/// # Errors
///
/// [`Refused::NotPrivate`] for such a file, and [`Refused::Unavailable`] when
/// the file could not be made, written, renamed or synced.
pub fn replace_private(path: &Path, text: &str, user: &User) -> Result<(), Refused> {
    use std::io::Write as _;
    let beside = beside(path);
    let failed = |doing: &str, error| {
        Refused::unavailable(
            format!("`{}` could not be {doing}", beside.display()),
            error,
        )
    };
    match nc_platform::judge(&beside, Kind::File, user) {
        Ok(()) => std::fs::remove_file(&beside).map_err(|error| failed("removed", error))?,
        Err(Judged::Failed(absent)) if absent.error().kind() == std::io::ErrorKind::NotFound => {}
        Err(left) => return Err(Refused::judged(left)),
    }
    let mut file = nc_platform::create_private(&beside).map_err(|error| failed("made", error))?;
    file.write_all(text.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|error| failed("written", error))?;
    drop(file);
    std::fs::rename(&beside, path).map_err(|error| failed("renamed", error))?;
    if let Some(directory) = path.parent() {
        nc_platform::sync_directory(directory).map_err(|error| {
            Refused::unavailable(
                format!("`{}` could not be synced", directory.display()),
                error,
            )
        })?;
    }
    Ok(())
}

/// Where a file is written before it is renamed over `path`.
fn beside(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_owned();
    name.push(".new");
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use nc_catalog::ErrorCode;

    use super::Record;

    /// A scratch directory for one test, removed however the test leaves.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = nc_platform::testing::make_private(&self.0);
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir()
                .join(format!("nc-daemon-pidfile-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            nc_platform::create_private_directory(&root).expect("a scratch directory");
            Self(root)
        }

        fn user(&self) -> nc_platform::User {
            nc_platform::user_in(&self.0).expect("the scratch is private")
        }
    }

    fn a_record(boot: Option<&str>) -> Record {
        Record {
            pid: 4242,
            boot: boot.map(str::to_owned),
            started: "987654".to_owned(),
            executable: PathBuf::from("/opt/noctua/bin/noctua-daemon"),
            instance: [7; 16],
        }
    }

    /// A record written is private and whole, read back as written, and
    /// removed only when it names the instance asking.
    #[test]
    fn a_record_written_is_private_and_removed_only_by_its_instance() {
        let scratch = Scratch::new("written");
        let user = scratch.user();
        let path = scratch.0.join("daemon.pid");
        let record = a_record(Some("6c1f2a"));
        super::write(&path, &record, &user).expect("written");
        assert!(nc_platform::testing::is_private(&path));
        assert_eq!(super::read(&path, &user).expect("read"), record);
        super::remove_if_ours(&path, [8; 16], &user).expect("asked");
        assert!(path.is_file(), "another instance's record was removed");
        super::remove_if_ours(&path, record.instance, &user).expect("removed");
        assert!(!path.exists(), "the instance's own record was left");
    }

    /// A record another account can reach is refused, left as it was.
    #[test]
    fn a_record_another_account_can_reach_is_refused() {
        let scratch = Scratch::new("reachable");
        let user = scratch.user();
        let path = scratch.0.join("daemon.pid");
        super::write(&path, &a_record(None), &user).expect("written");
        nc_platform::testing::open_to_another_user(&path).expect("opened");
        let refused = super::read(&path, &user).expect_err("reachable");
        assert_eq!(
            refused.code(),
            ErrorCode::DaemonProfileNotPrivate,
            "{refused}"
        );
        super::remove_if_ours(&path, [7; 16], &user).expect("asked");
        assert!(path.is_file(), "a record that was refused was removed");
    }

    /// A malformed record is refused with the record's own code.
    #[test]
    fn a_malformed_record_is_refused_with_its_code() {
        let scratch = Scratch::new("malformed");
        let user = scratch.user();
        let path = scratch.0.join("daemon.pid");
        super::replace_private(&path, "{\"pid\":0}", &user).expect("written");
        let refused = super::read(&path, &user).expect_err("malformed");
        assert_eq!(
            refused.code(),
            ErrorCode::DaemonPidRecordUnreadable,
            "{refused}"
        );
    }

    /// What a write left beside the file is removed once judged, and one
    /// another account can reach is refused with the file as it was.
    #[test]
    fn a_file_left_beside_is_removed_or_refused() {
        let scratch = Scratch::new("beside");
        let user = scratch.user();
        let path = scratch.0.join("token");
        let left = super::beside(&path);
        nc_platform::create_private(&left).expect("left by an interrupted write");
        super::replace_private(&path, "one", &user).expect("replaced");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "one");
        assert!(!left.exists());
        nc_platform::create_private(&left).expect("left again");
        nc_platform::testing::open_to_another_user(&left).expect("opened");
        let refused = super::replace_private(&path, "two", &user).expect_err("reachable");
        assert_eq!(
            refused.code(),
            ErrorCode::DaemonProfileNotPrivate,
            "{refused}"
        );
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "one");
    }
}
