//! The PID record: which process an install's running daemon is, as the system
//! says it is, in the file [`crate::Layout::pid_file`] names (`NOCTUA.md
//! §17.1`).
//!
//! **Kernel-free, beside the manifest's reader**, so the daemon that writes
//! the record and the CLI that reads it before it signals anything read one
//! format, written once.
//!
//! **A process, not a number.** The record holds the daemon's process id with
//! the boot it runs in, when it started and the executable it runs, read off
//! the system (`nc_platform::process::this_process`), and the instance it drew
//! at its start: a process id is handed out again once its process is gone, so
//! a record naming the id alone names whichever process holds it next. A
//! reader holds a live process to every field — `nc_platform::process::Verified`
//! — before it believes the record is that process.
//!
//! **Read back only as this code writes it**, judged this user's alone first,
//! and **a process numbered zero or below is refused as a record**, never read
//! as one: to a signal, zero is the caller's own process group and `-1` every
//! process the user owns.

use std::path::{Path, PathBuf};

use nc_canon::Value;
use nc_platform::{Judged, Kind, User};

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

    /// The process the record names, as a verification is asked about it.
    #[must_use]
    pub fn identity(&self) -> nc_platform::process::Identity {
        nc_platform::process::Identity {
            pid: self.pid,
            boot: self.boot.clone(),
            started: self.started.clone(),
            executable: self.executable.clone(),
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

/// Why no record was read.
#[derive(Debug)]
pub enum Unread {
    /// No record is there: no daemon wrote one, or the last stopped and
    /// removed it.
    Missing,
    /// The record is not this user's alone, so it was not read.
    NotPrivate(nc_platform::Refusal),
    /// The record is not one this build writes; the text says why.
    Malformed(String),
    /// The filesystem did not answer.
    Unavailable {
        /// What was being done, naming the path.
        doing: String,
        /// What the system answered.
        error: std::io::Error,
    },
}

impl core::fmt::Display for Unread {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Missing => write!(formatter, "no PID record is there"),
            Self::NotPrivate(refused) => write!(
                formatter,
                "{refused}, so the PID record was not read: another account could have written \
                 it"
            ),
            Self::Malformed(why) => write!(formatter, "the PID record was refused: {why}"),
            Self::Unavailable { doing, error } => write!(formatter, "{doing}: {error}"),
        }
    }
}

/// The record at `path`, judged this user's alone and read.
///
/// # Errors
///
/// [`Unread::Missing`] for no record there, [`Unread::NotPrivate`] for one
/// another account can reach, [`Unread::Malformed`] for one this code does not
/// write, and [`Unread::Unavailable`] when it could not be read.
pub fn read(path: &Path, user: &User) -> Result<Record, Unread> {
    match nc_platform::judge(path, Kind::File, user) {
        Ok(()) => {}
        Err(Judged::Refused(refused)) => return Err(Unread::NotPrivate(refused)),
        Err(Judged::Failed(failed)) if failed.error().kind() == std::io::ErrorKind::NotFound => {
            return Err(Unread::Missing);
        }
        Err(Judged::Failed(failed)) => {
            return Err(Unread::Unavailable {
                doing: failed.doing().to_owned(),
                error: std::io::Error::new(failed.error().kind(), failed.error().to_string()),
            });
        }
    }
    let text = std::fs::read_to_string(path).map_err(|error| Unread::Unavailable {
        doing: format!("`{}` could not be read", path.display()),
        error,
    })?;
    Record::parse(&text).map_err(Unread::Malformed)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{Record, Unread};

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
                .join(format!("nc-channel-record-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            nc_platform::create_private_directory(&root).expect("a scratch directory");
            Self(root)
        }

        fn user(&self) -> nc_platform::User {
            nc_platform::user_in(&self.0).expect("the scratch is private")
        }

        /// `text` written private at `name` in the scratch.
        fn written(&self, name: &str, text: &str) -> PathBuf {
            use std::io::Write as _;
            let path = self.0.join(name);
            nc_platform::create_private(&path)
                .expect("a private file")
                .write_all(text.as_bytes())
                .expect("written");
            path
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

    /// A record reads back as the one written, with a boot and without one,
    /// and names the process it was made of.
    #[test]
    fn a_record_reads_back_as_written() {
        for record in [a_record(Some("6c1f2a")), a_record(None)] {
            let text = record.to_canonical().expect("written");
            assert_eq!(Record::parse(&text), Ok(record.clone()));
            assert_eq!(Record::of(record.identity(), record.instance), record);
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

    /// A record is read once judged: one missing, one another account can
    /// reach and one malformed are each refused by their own kind, the
    /// reachable one left as it was.
    #[test]
    fn a_record_is_read_once_judged_and_refused_by_kind() {
        let scratch = Scratch::new("read");
        let user = scratch.user();
        let record = a_record(Some("6c1f2a"));
        let path = scratch.written("daemon.pid", &record.to_canonical().expect("written"));
        assert_eq!(super::read(&path, &user).expect("read"), record);
        assert!(matches!(
            super::read(&scratch.0.join("none"), &user),
            Err(Unread::Missing)
        ));
        let malformed = scratch.written("malformed.pid", "{\"pid\":0}");
        assert!(matches!(
            super::read(&malformed, &user),
            Err(Unread::Malformed(_))
        ));
        nc_platform::testing::open_to_another_user(&path).expect("opened");
        assert!(matches!(
            super::read(&path, &user),
            Err(Unread::NotPrivate(_))
        ));
        assert!(path.is_file(), "a record refused was not left as it was");
    }
}
