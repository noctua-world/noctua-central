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
//! **Read back only as this code writes it**, judged this user's alone first,
//! and **a process numbered zero or below is refused as a record**, never read
//! as one: to a signal, zero is the caller's own process group and `-1` every
//! process the user owns (`NC-DMN-0013`).

use std::path::{Path, PathBuf};

use nc_canon::Value;
use nc_platform::{Judged, Kind, User};

use crate::Refused;

/// The keys a record holds, with `boot` where the system names one.
const KEYS: [&str; 5] = ["boot", "executable", "instance", "pid", "started"];

/// Which process the running daemon is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// Its process id: never zero.
    pub pid: u32,
    /// The boot it runs in, where the system names one.
    pub boot: Option<String>,
    /// When it started, in the system's own spelling.
    pub started: String,
    /// The executable it runs, canonicalized.
    pub executable: PathBuf,
    /// The instance it drew at this start, which its welcome names too.
    pub instance: [u8; 16],
}

impl Record {
    /// The record of `identity`, a process that drew `instance`.
    #[must_use]
    pub fn of(identity: nc_platform::process::Identity, instance: [u8; 16]) -> Self {
        Self {
            pid: identity.pid,
            boot: identity.boot,
            started: identity.started,
            executable: identity.executable,
            instance,
        }
    }

    /// The record as its file holds it: one canonical object.
    ///
    /// # Errors
    ///
    /// The executable's path, when it is not UTF-8 and so has no spelling the
    /// file can hold.
    pub fn to_canonical(&self) -> Result<String, String> {
        let executable = self.executable.to_str().ok_or_else(|| {
            format!(
                "the executable `{}` is not UTF-8, so no record can name it",
                self.executable.display()
            )
        })?;
        let mut fields = vec![
            ("executable", Value::text(executable)),
            ("instance", Value::text(nc_canon::to_hex(&self.instance))),
            ("pid", Value::Integer(i64::from(self.pid))),
            ("started", Value::text(self.started.as_str())),
        ];
        if let Some(boot) = &self.boot {
            fields.push(("boot", Value::text(boot.as_str())));
        }
        Ok(Value::object(fields).to_canonical())
    }

    /// The record `text` says, read only as [`Record::to_canonical`] writes
    /// one.
    ///
    /// # Errors
    ///
    /// What is wrong with it: not canonical JSON, a key it does not hold or
    /// one missing, a value of another kind, and a process id that is zero,
    /// below zero or past what a process id can be.
    pub fn parse(text: &str) -> Result<Self, String> {
        let value = nc_canon::parse(text).map_err(|error| format!("it is not JSON: {error}"))?;
        if value.to_canonical() != text {
            return Err("it is not written canonically, as the daemon writes it".to_owned());
        }
        let mut fields = value.into_object().ok_or("it is not one object")?;
        if let Some(key) = fields.keys().find(|key| !KEYS.contains(&key.as_str())) {
            return Err(format!("it holds the key {key:?}, which it does not read"));
        }
        let mut text_of = |key: &str| match fields.remove(key) {
            None => Ok(None),
            Some(value) => value
                .into_text()
                .map(Some)
                .ok_or_else(|| format!("its {key:?} is not text")),
        };
        let boot = text_of("boot")?;
        let started = text_of("started")?.ok_or("it names no start")?;
        let executable = PathBuf::from(text_of("executable")?.ok_or("it names no executable")?);
        let instance = text_of("instance")?
            .and_then(|hex| nc_canon::from_hex(&hex).ok())
            .and_then(|bytes| <[u8; 16]>::try_from(bytes.as_slice()).ok())
            .ok_or("its instance is not 16 bytes")?;
        let Some(Value::Integer(pid)) = fields.remove("pid") else {
            return Err("its pid is not a whole number".to_owned());
        };
        let pid = process_of(pid)?;
        Ok(Self {
            pid,
            boot,
            started,
            executable,
            instance,
        })
    }
}

/// The process `pid` names, refused where it names none.
///
/// **Zero and below are not processes**: a signal to zero is one to the
/// caller's own process group, and to `-1` one to every process the user
/// owns, so a record holding either is refused here and never handed on.
fn process_of(pid: i64) -> Result<u32, String> {
    if pid <= 0 {
        return Err(format!("its pid is {pid}, which names no process"));
    }
    u32::try_from(pid).map_err(|_| format!("its pid is {pid}, past what a process id can be"))
}

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
    nc_platform::judge(path, Kind::File, user).map_err(Refused::judged)?;
    let text = std::fs::read_to_string(path).map_err(|error| {
        Refused::unavailable(format!("`{}` could not be read", path.display()), error)
    })?;
    Record::parse(&text).map_err(|why| Refused::PidRecord {
        path: path.to_owned(),
        why,
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

    /// A record reads back as the one written, with a boot and without one.
    #[test]
    fn a_record_reads_back_as_written() {
        for record in [a_record(Some("6c1f2a")), a_record(None)] {
            let text = record.to_canonical().expect("written");
            assert_eq!(Record::parse(&text), Ok(record));
        }
    }

    /// A record naming no process — zero, below zero, or past what an id can
    /// be — is refused as a record.
    #[test]
    fn a_record_naming_no_process_is_refused() {
        let text = a_record(None).to_canonical().expect("written");
        for pid in ["0", "-1", "-4242", "4294967296"] {
            let planted = text.replace("\"pid\":4242", &format!("\"pid\":{pid}"));
            assert_ne!(planted, text);
            let refused = Record::parse(&planted).expect_err("no process");
            assert!(refused.contains(pid), "{pid}: {refused}");
        }
        let one = text.replace("\"pid\":4242", "\"pid\":1");
        assert_eq!(Record::parse(&one).map(|record| record.pid), Ok(1));
    }

    /// Each way a record is not one this code writes is refused.
    #[test]
    fn a_record_not_as_written_is_refused() {
        let text = a_record(Some("6c1f2a")).to_canonical().expect("written");
        for (planted, says) in [
            (format!(" {text}"), "canonically"),
            ("[]".to_owned(), "one object"),
            (text.replacen('{', "{\"also\":1,", 1), "\"also\""),
            (
                text.replace("\"pid\":4242", "\"pid\":\"4242\""),
                "whole number",
            ),
            (
                text.replace(&nc_canon::to_hex(&[7; 16]), &nc_canon::to_hex(&[7; 15])),
                "16 bytes",
            ),
            (text.replace(",\"started\":\"987654\"", ""), "no start"),
            (
                text.replace("\"boot\":\"6c1f2a\"", "\"boot\":1"),
                "not text",
            ),
        ] {
            assert_ne!(planted, text, "{says}: nothing was planted");
            let refused = Record::parse(&planted).expect_err("refused");
            assert!(refused.contains(says), "{says}: {refused}");
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
