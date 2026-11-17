//! The development checkouts installed beside an install, what each records
//! of itself, and the launcher's pointer (`NOCTUA.md §17.1`).
//!
//! # One listing
//!
//! A checkout is found from an install's own bases and namespace alone: a
//! directory of the state base whose name is a checkout's leaf under this
//! namespace and no other, so a test's install never lists a real one's, nor
//! the other way round; and it counts only while its install manifest is
//! there, so a profile an uninstall left lists nothing. **The Leader
//! settlement ([`crate::keys`]) and the CLI's `checkouts` verb read this one
//! listing**, bounded alike: past [`PROFILES_MAX`] profiles, or
//! [`CHANNELS_MAX`] installed checkouts, it refuses rather than read without
//! bound.
//!
//! # What a checkout records
//!
//! [`RECORD`], beside the manifest (`Layout::checkout_record`): canonical
//! JSON of at most [`RECORD_BYTES_MAX`] bytes holding `format`, which is
//! [`RECORD_FORMAT`]; the checkout's `directory`, the canonical path its
//! digest is of; the `link`, where `noctua-dev` lives in the user's bin
//! directory; and when it was `installed`, in milliseconds since the Unix
//! epoch. **The manifest stays format 1**: an older checkout's binaries refuse
//! a manifest of another format, their own and their siblings', so what this
//! needs is a second file, which they never read. A checkout installed before
//! the file existed has none, and is listed without its directory; its
//! manifest's last write stands for when it was installed.
//!
//! # The launcher's pointer
//!
//! Where a name in the bin directory reaches its executable through a launcher
//! (`nc_platform::Reach::Launcher`), the launcher reads [`Pointer`] in the file
//! [`pointer_beside`] names: canonical JSON holding `format`, which is
//! [`POINTER_FORMAT`], and the `cli` it runs.
//!
//! # What this answers (`§2.4`)
//!
//! Another account choosing what is listed, or what a choice runs: a record
//! and a pointer are read only once they are judged this user's alone, and no
//! further than one byte past their bound; every path either holds is a plain
//! absolute path, as a manifest's bases are; and what a link points at is
//! taken as a checkout only when it is exactly the CLI [`layout`] names for a
//! checkout listed here ([`pointed_at`]). Nothing here writes: the installer
//! and the verb do.
//!
//! # What this does not answer
//!
//! A process running as this user, which can write a record or a pointer as an
//! installer would; and installs whose bases differ from this one's, which it
//! cannot find.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use nc_canon::Value;
use nc_platform::{Judged, Kind, User};

use crate::{Channel, Checkout, Install, Layout, Namespace, layout, plain, quoted};

/// The file a development checkout records itself in, beside its manifest.
pub const RECORD: &str = "checkout.json";

/// The one format of [`RECORD`] this build reads and writes.
pub const RECORD_FORMAT: i64 = 1;

/// The one format of a launcher's [`Pointer`] this build reads and writes.
pub const POINTER_FORMAT: i64 = 1;

/// The most bytes a record or a pointer may hold.
///
/// One byte more is read, only to tell a file past it. Two paths and a number
/// are far below it, and it bounds what one read costs, whoever wrote the file.
pub const RECORD_BYTES_MAX: u64 = 16_384;

/// The most installed development checkouts a listing holds.
///
/// More is a state base nobody keeps by hand, and reading past it would read
/// without bound: a configured Leader is refused, and the `checkouts` verb
/// lists nothing. **Installed ones alone are counted**: a profile an uninstall
/// left behind is no checkout, and every checkout ever installed leaves one.
pub const CHANNELS_MAX: usize = 64;

/// The most development profiles the state base is listed for.
///
/// Each profile is asked whether its checkout's manifest is there, so this
/// bounds what a listing asks, profiles left by an uninstall among them; it is
/// far past [`CHANNELS_MAX`] because only an installed checkout is counted
/// against that. Past it, the listing refuses.
pub const PROFILES_MAX: usize = 1_024;

/// The keys a record holds, in the order canonical form writes them.
const RECORD_KEYS: [&str; 4] = ["directory", "format", "installed", "link"];

/// The keys a pointer holds, in the order canonical form writes them.
const POINTER_KEYS: [&str; 2] = ["cli", "format"];

/// What a development checkout records of itself, beside its manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// The checkout's directory: the canonical path its digest is of.
    pub directory: PathBuf,
    /// Where `noctua-dev` lives, in the user's bin directory, its system's
    /// executable suffix included.
    pub link: PathBuf,
    /// When it was installed, in milliseconds since the Unix epoch.
    pub installed: i64,
}

impl Record {
    /// The record a file's text describes.
    ///
    /// # Errors
    ///
    /// What is wrong with it: not a JSON object `nc-canon` reads, a key this
    /// build does not read or one missing, a value of the wrong type, a
    /// `format` other than [`RECORD_FORMAT`], a path that is not plain and
    /// absolute, or a time before the epoch.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut fields = object(text, &RECORD_KEYS)?;
        formatted(&mut fields, RECORD_FORMAT)?;
        let installed = match fields.remove("installed") {
            Some(Value::Integer(at)) if at >= 0 => at,
            _ => return Err("its `installed` is not a time since the epoch".to_owned()),
        };
        Ok(Self {
            directory: path(&mut fields, "directory")?,
            link: path(&mut fields, "link")?,
            installed,
        })
    }

    /// The text an installer writes for this record: canonical JSON.
    #[must_use]
    pub fn to_canonical(&self) -> String {
        Value::object([
            ("directory", Value::text(self.directory.to_string_lossy())),
            ("format", Value::Integer(RECORD_FORMAT)),
            ("installed", Value::Integer(self.installed)),
            ("link", Value::text(self.link.to_string_lossy())),
        ])
        .to_canonical()
    }
}

/// What a launcher runs: the CLI of the development checkout installed or
/// chosen last.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pointer {
    /// That CLI's executable, a plain absolute path.
    pub cli: PathBuf,
}

impl Pointer {
    /// The pointer a file's text describes.
    ///
    /// # Errors
    ///
    /// [`Record::parse`]'s refusals, said of the pointer's own keys.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut fields = object(text, &POINTER_KEYS)?;
        formatted(&mut fields, POINTER_FORMAT)?;
        Ok(Self {
            cli: path(&mut fields, "cli")?,
        })
    }

    /// The text an installer writes for this pointer: canonical JSON.
    #[must_use]
    pub fn to_canonical(&self) -> String {
        Value::object([
            ("cli", Value::text(self.cli.to_string_lossy())),
            ("format", Value::Integer(POINTER_FORMAT)),
        ])
        .to_canonical()
    }
}

/// Where the launcher at `launcher` reads its [`Pointer`]: beside it, its
/// name without the system's executable suffix, with .launch.json after it.
#[must_use]
pub fn pointer_beside(launcher: &Path) -> PathBuf {
    let stem = launcher.file_stem().unwrap_or_default().to_string_lossy();
    launcher.with_file_name(format!("{stem}.launch.json"))
}

/// The JSON object `text` holds, refused when it holds a key `known` does not
/// name.
fn object(text: &str, known: &[&str]) -> Result<std::collections::BTreeMap<String, Value>, String> {
    let value = nc_canon::parse(text).map_err(|error| format!("it is not JSON: {error}"))?;
    let fields = value
        .into_object()
        .ok_or_else(|| "it is not a JSON object".to_owned())?;
    if let Some(key) = fields.keys().find(|key| !known.contains(&key.as_str())) {
        return Err(format!(
            "it holds `{}`, a key this build does not read",
            quoted(key)
        ));
    }
    Ok(fields)
}

/// Whether `fields` say they are of `format`.
fn formatted(
    fields: &mut std::collections::BTreeMap<String, Value>,
    format: i64,
) -> Result<(), String> {
    match fields.remove("format") {
        Some(Value::Integer(said)) if said == format => Ok(()),
        Some(Value::Integer(said)) => Err(format!(
            "its format is {said}, and this build reads format {format}"
        )),
        _ => Err("it names no format".to_owned()),
    }
}

/// The plain absolute path under `key`.
fn path(
    fields: &mut std::collections::BTreeMap<String, Value>,
    key: &str,
) -> Result<PathBuf, String> {
    let text = fields
        .remove(key)
        .and_then(Value::into_text)
        .ok_or_else(|| format!("its `{key}` is not text"))?;
    let path = PathBuf::from(text);
    // **The rule a manifest's bases are held to**, said of this key: "the
    // link base" reads "the link".
    plain(key, &path).map_err(|refused| match refused {
        crate::ChannelError::Base(what) => what.replacen(" base ", " ", 1),
        other => other.to_string(),
    })?;
    Ok(path)
}

/// The CLI's executable of the install laid out as `layout`, its system's
/// executable suffix included: where an installer copies it, and what a link
/// to the checkout points at.
#[must_use]
pub fn cli_of(layout: &Layout) -> PathBuf {
    layout
        .install
        .join(format!("{}{}", layout.cli, nc_platform::EXECUTABLE_SUFFIX))
}

/// What a checkout records of itself, as it was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recorded {
    /// Its record.
    Read(Record),
    /// No record: a checkout installed before the file existed.
    Absent,
    /// A record that was not read, and why.
    Refused(String),
}

/// One development checkout installed beside an install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    /// The checkout.
    pub checkout: Checkout,
    /// Every name its install gives the system.
    pub layout: Layout,
    /// Its CLI's executable, as [`cli_of`] names it.
    pub cli: PathBuf,
    /// What it records of itself.
    pub recorded: Recorded,
    /// When it was installed, in milliseconds since the Unix epoch.
    ///
    /// Its record's time, or its manifest's last write where it has no record,
    /// and nothing where neither says.
    pub installed: Option<i64>,
}

/// Why the installed checkouts were not listed: what was not read, naming
/// where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unlisted(pub String);

impl core::fmt::Display for Unlisted {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl std::error::Error for Unlisted {}

/// Every development checkout installed beside `install`, under its bases and
/// its namespace, by digest, with what each records of itself — for `user`.
///
/// # Errors
///
/// [`Unlisted`] when the state base could not be listed, a manifest's
/// presence could not be asked, or a bound was passed.
pub fn listed(install: &Install, user: &User) -> Result<Vec<Listed>, Unlisted> {
    Ok(installed_checkouts(install, user)?
        .into_iter()
        .map(|(checkout, layout, written)| {
            let recorded = layout
                .checkout_record
                .as_deref()
                .map_or(Recorded::Absent, |record| read_record(record, user));
            let installed = match &recorded {
                Recorded::Read(record) => Some(record.installed),
                Recorded::Absent | Recorded::Refused(_) => written,
            };
            Listed {
                cli: cli_of(&layout),
                checkout,
                layout,
                recorded,
                installed,
            }
        })
        .collect())
}

/// The checkout of `listed` whose CLI is exactly `target`, if one is.
///
/// **Exactly the path [`cli_of`] names**, so a link or a pointer is taken as
/// pointing at a checkout only when it points where an installer points one.
#[must_use]
pub fn pointed_at(target: &Path, listed: &[Listed]) -> Option<Checkout> {
    listed
        .iter()
        .find(|one| one.cli == target)
        .map(|one| one.checkout)
}

/// The record at `path`, judged this user's alone and read no further than its
/// bound.
fn read_record(path: &Path, user: &User) -> Recorded {
    match read_bounded(path, user) {
        Ok(Some(text)) => Record::parse(&text).map_or_else(
            |what| Recorded::Refused(format!("`{}` was refused: {what}", path.display())),
            Recorded::Read,
        ),
        Ok(None) => Recorded::Absent,
        Err(what) => Recorded::Refused(what),
    }
}

/// The text of the file `path`, once judged this user's alone, read no
/// further than one byte past [`RECORD_BYTES_MAX`]; nothing when it is not
/// there.
///
/// # Errors
///
/// What was wrong: a file not this user's alone, one the disk did not answer
/// for, one past its bound, or one that is not UTF-8.
pub fn read_bounded(path: &Path, user: &User) -> Result<Option<String>, String> {
    match nc_platform::judge(path, Kind::File, user) {
        Ok(()) => {}
        Err(Judged::Failed(failed)) if failed.error().kind() == std::io::ErrorKind::NotFound => {
            return Ok(None);
        }
        Err(judged) => return Err(format!("{judged}, so it was not read")),
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|opened| opened.take(RECORD_BYTES_MAX + 1).read_to_end(&mut bytes))
        .map_err(|error| format!("`{}` could not be read: {error}", path.display()))?;
    if u64::try_from(bytes.len()).map_or(true, |read| read > RECORD_BYTES_MAX) {
        return Err(format!(
            "`{}` holds more than {RECORD_BYTES_MAX} bytes",
            path.display()
        ));
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| format!("`{}` is not UTF-8", path.display()))
}

/// Every development checkout installed beside `install`, under its bases and
/// its namespace, by digest: each with its layout for `user`, and its
/// manifest's last write in milliseconds since the epoch where the system
/// says.
///
/// **The one listing** the Leader settlement and the `checkouts` verb read.
pub(crate) fn installed_checkouts(
    install: &Install,
    user: &User,
) -> Result<Vec<(Checkout, Layout, Option<i64>)>, Unlisted> {
    let base = install.bases.state();
    let mut counted = Vec::new();
    for checkout in checkouts_under(base, install.namespace.as_ref())? {
        let channel = Channel::Development(checkout);
        let names = layout(
            &Install {
                channel,
                bases: install.bases.clone(),
                namespace: install.namespace.clone(),
                log_endpoint: None,
            },
            &user.to_string(),
        );
        // **Asked whether it is there, and nothing more**: a manifest's
        // contents are its own binaries' to judge.
        let asked = std::fs::symlink_metadata(&names.manifest);
        let written = asked
            .as_ref()
            .ok()
            .and_then(|found| found.modified().ok())
            .and_then(|at| at.duration_since(UNIX_EPOCH).ok())
            .and_then(|since| i64::try_from(since.as_millis()).ok());
        if installed(asked.map(drop), &named(channel), &names.manifest)? {
            counted.push((checkout, names, written));
            // **Counted once its manifest answered**: a profile an uninstall
            // left is listed above and binds nothing, so it is not counted.
            if counted.len() > CHANNELS_MAX {
                return Err(Unlisted(format!(
                    "more than {CHANNELS_MAX} development checkouts are installed under `{}`",
                    base.display()
                )));
            }
        }
    }
    Ok(counted)
}

/// How `channel` is named in what a refusal says.
pub(crate) fn named(channel: Channel) -> String {
    match channel {
        Channel::Production => "production".to_owned(),
        Channel::Staging => "staging".to_owned(),
        Channel::Development(checkout) => format!("the development checkout {checkout}"),
    }
}

/// Whether the channel `named` is installed, by what asking for its manifest
/// answered.
///
/// An install whose manifest is gone counts as none. **Any other answer is a
/// refusal**, naming where the manifest was asked for, rather than a guess
/// whether that channel is installed.
pub(crate) fn installed(
    asked: std::io::Result<()>,
    named: &str,
    manifest: &Path,
) -> Result<bool, Unlisted> {
    let Err(error) = asked else {
        return Ok(true);
    };
    match error.kind() {
        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory => Ok(false),
        _ => Err(Unlisted(format!(
            "whether {named} is installed could not be read at `{}`: {error}",
            manifest.display()
        ))),
    }
}

/// Every checkout whose profile directory sits under the state base `base`
/// for `namespace`, by digest.
fn checkouts_under(base: &Path, namespace: Option<&Namespace>) -> Result<Vec<Checkout>, Unlisted> {
    let unlisted = |error: &std::io::Error| {
        Unlisted(format!(
            "the profiles under `{}` could not be listed: {error}",
            base.display()
        ))
    };
    let listing = std::fs::read_dir(base); // allow-nondeterminism: this `read_dir` is sorted by digest below, which is the order the checkouts are settled and listed in
    let listing = match listing {
        Ok(listing) => listing,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(unlisted(&error)),
    };
    let mut found = Vec::new();
    for entry in listing {
        let entry = entry.map_err(|error| unlisted(&error))?;
        let name = entry.file_name();
        let Some(checkout) = name
            .to_str()
            .and_then(|name| checkout_named(name, namespace))
        else {
            continue;
        };
        found.push(checkout);
        if found.len() > PROFILES_MAX {
            return Err(Unlisted(format!(
                "more than {PROFILES_MAX} development profiles sit under `{}`",
                base.display()
            )));
        }
    }
    found.sort();
    Ok(found)
}

/// The checkout whose leaf under `namespace` is `name`: its profile
/// directory's name, and its CLI's without a system's executable suffix.
///
/// The inverse of the leaf [`layout`] writes for a development install, and
/// nothing for any other name.
#[must_use]
pub fn checkout_named(name: &str, namespace: Option<&Namespace>) -> Option<Checkout> {
    let rest = name.strip_prefix("noctua-dev-")?;
    let digits = match namespace {
        None => rest,
        Some(namespace) => rest.strip_suffix(namespace.as_str())?.strip_suffix('-')?,
    };
    Checkout::parse(digits).ok()
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;
    use std::path::{Path, PathBuf};

    use super::{Pointer, Record, checkout_named, installed, pointer_beside};
    use crate::{Channel, Checkout, Namespace, leaf};

    fn namespace(text: &str) -> Namespace {
        Namespace::parse(text).expect("a namespace")
    }

    /// A checkout's profile directory is read back as the checkout that
    /// names it, under its namespace and under none.
    #[test]
    fn a_profile_directory_is_read_back_as_the_channel_that_names_it() {
        for digits in ["0123abcd", "00000000", "ffffffff"] {
            let checkout = Checkout::parse(digits).expect("eight digits");
            for space in [None, Some(namespace("t1")), Some(namespace("a2b3"))] {
                let name = leaf(Channel::Development(checkout), space.as_ref());
                assert_eq!(
                    checkout_named(&name, space.as_ref()),
                    Some(checkout),
                    "{name}"
                );
            }
        }
    }

    /// No name a checkout's leaf is not, under this namespace, is a sibling.
    ///
    /// Production's and staging's leaves, another namespace's, none's, and
    /// anything spelled near one.
    #[test]
    fn a_name_no_channel_writes_is_no_sibling() {
        let checkout = Checkout::parse("0123abcd").expect("eight digits");
        let t1 = namespace("t1");
        let t2 = namespace("t2");
        for (name, space) in [
            (leaf(Channel::Production, None), None),
            (leaf(Channel::Staging, None), None),
            (leaf(Channel::Production, Some(&t1)), Some(&t1)),
            (leaf(Channel::Staging, Some(&t1)), Some(&t1)),
            // **Another namespace's, and a real one's**, each way round.
            (leaf(Channel::Development(checkout), Some(&t2)), Some(&t1)),
            (leaf(Channel::Development(checkout), Some(&t1)), None),
            (leaf(Channel::Development(checkout), None), Some(&t1)),
            ("noctua-devx".to_owned(), None),
            ("noctua-dev-".to_owned(), None),
            ("noctua-dev-ZZ".to_owned(), None),
            ("noctua-dev-0123ABCD".to_owned(), None),
            ("noctua-dev-0123abcd0".to_owned(), None),
            ("noctua-dev-0123abcd-".to_owned(), None),
            ("noctua-dev-0123abcd--t1".to_owned(), Some(&t1)),
            ("noctua-dev-0123abcdt1".to_owned(), Some(&t1)),
            ("noctua-dev-0123abcd-t1x".to_owned(), Some(&t1)),
            ("Noctua-dev-0123abcd".to_owned(), None),
            // **The chosen checkout's name is no checkout's.**
            ("noctua-dev".to_owned(), None),
            ("noctua-dev-t1".to_owned(), Some(&t1)),
        ] {
            assert_eq!(checkout_named(&name, space), None, "{name}");
        }
    }

    /// A channel counts only while its manifest is there, and a manifest the
    /// disk did not answer for is a refusal naming where it was asked for.
    #[test]
    fn a_channel_counts_only_while_its_manifest_is_there() {
        let manifest = Path::new("/scratch/s/noctua-dev-0123abcd/bin/install.json");
        let named = "the development checkout 0123abcd";
        assert_eq!(installed(Ok(()), named, manifest), Ok(true));
        for gone in [ErrorKind::NotFound, ErrorKind::NotADirectory] {
            assert_eq!(installed(Err(gone.into()), named, manifest), Ok(false));
        }
        for unanswered in [
            ErrorKind::PermissionDenied.into(),
            ErrorKind::InvalidFilename.into(),
            std::io::Error::other("the disk did not answer"),
        ] {
            let refused = installed(Err(unanswered), named, manifest).expect_err("not known");
            assert!(
                refused
                    .0
                    .contains("whether the development checkout 0123abcd is installed")
                    && refused.0.contains(&manifest.display().to_string()),
                "{refused}"
            );
        }
    }

    /// Absolute on every system, and nothing is made under it.
    fn somewhere(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("nowhere")
            .join(name)
    }

    /// A record reads back as the text it writes, and one this build does not
    /// write is refused, saying why.
    #[test]
    fn a_record_reads_back_as_the_text_it_writes_and_another_is_refused() {
        let record = Record {
            directory: somewhere("checkout"),
            link: somewhere("noctua-dev"),
            installed: 1_763_380_000_000,
        };
        let text = record.to_canonical();
        assert_eq!(Record::parse(&text), Ok(record.clone()));
        assert!(text.starts_with("{\"directory\":"), "{text}");
        let written = |directory: &str, format: &str, installed: &str, more: &str| {
            format!(
                "{{\"directory\":{directory},\"format\":{format},\"installed\":{installed},\
                 \"link\":{}{more}}}",
                nc_canon::Value::text(somewhere("l").to_string_lossy()).to_canonical()
            )
        };
        let directory = nc_canon::Value::text(somewhere("d").to_string_lossy()).to_canonical();
        assert!(Record::parse(&written(&directory, "1", "0", "")).is_ok());
        for (text, said) in [
            (written(&directory, "2", "0", ""), "format is 2"),
            (written(&directory, "\"1\"", "0", ""), "names no format"),
            (written(&directory, "1", "-1", ""), "not a time"),
            (written(&directory, "1", "0", ",\"more\":1"), "`more`"),
            (
                written("\"relative\"", "1", "0", ""),
                "the directory `relative` is not",
            ),
            (written("1", "1", "0", ""), "not text"),
            ("[]".to_owned(), "not a JSON object"),
            ("{".to_owned(), "not JSON"),
        ] {
            let refused = Record::parse(&text).expect_err("refused");
            assert!(refused.contains(said), "{text}: {refused}");
        }
    }

    /// A launcher's pointer reads back as the text it writes, sits beside the
    /// launcher, and names a plain absolute path.
    #[test]
    fn a_pointer_reads_back_and_sits_beside_its_launcher() {
        let pointer = Pointer {
            cli: somewhere("noctua-dev-0123abcd.exe"),
        };
        assert_eq!(Pointer::parse(&pointer.to_canonical()), Ok(pointer));
        assert!(Pointer::parse("{\"cli\":\"relative\",\"format\":1}").is_err());
        assert!(Pointer::parse("{\"cli\":\"/a\",\"format\":2}").is_err());
        let launcher = somewhere("bin").join("noctua-dev.exe");
        assert_eq!(
            pointer_beside(&launcher),
            somewhere("bin").join("noctua-dev.launch.json")
        );
        assert_eq!(
            pointer_beside(&somewhere("bin").join("noctua-dev-t1")),
            somewhere("bin").join("noctua-dev-t1.launch.json")
        );
    }

    /// A scratch directory for one test, removed however the test leaves.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = nc_platform::testing::make_private(&self.0);
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl Scratch {
        fn new(tag: &str) -> Self {
            let root = std::env::temp_dir().join(format!("nc-listed-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            nc_platform::create_private_directory(&root).expect("a scratch directory");
            Self(root)
        }

        /// The development install of `digest` under this scratch's bases.
        fn install(&self, digest: &str) -> crate::Install {
            let base = |name: &str| self.0.join(name);
            crate::Install {
                channel: Channel::Development(Checkout::parse(digest).expect("a digest")),
                bases: crate::Bases::new(base("d"), base("c"), base("k"), base("s"))
                    .expect("plain"),
                namespace: Some(namespace("tl1")),
                log_endpoint: None,
            }
        }

        /// `text` written privately at `path`, its directories made private.
        fn written(path: &Path, text: &str) {
            use std::io::Write as _;
            let mut missing = Vec::new();
            let mut at = path.parent().expect("a parent");
            while !at.exists() {
                missing.push(at.to_path_buf());
                at = at.parent().expect("a parent");
            }
            for directory in missing.iter().rev() {
                nc_platform::create_private_directory(directory).expect("a directory");
            }
            nc_platform::create_private(path)
                .and_then(|mut file| file.write_all(text.as_bytes()))
                .expect("written");
        }
    }

    /// A listing holds every installed checkout by digest, with what it
    /// records.
    ///
    /// A record's time, an unrecorded one dated by its manifest, a record that
    /// is not read said why — and a profile with no manifest is no checkout.
    #[test]
    fn a_listing_holds_every_installed_checkout_with_what_it_records() {
        let scratch = Scratch::new("listed");
        let user = nc_platform::user_in(&scratch.0).expect("private");
        let [recorded, unrecorded, unread, gone] =
            ["0000000c", "0000000a", "0000000b", "0000000d"].map(|digest| scratch.install(digest));
        for install in [&recorded, &unrecorded, &unread] {
            let layout = crate::layout(install, "");
            Scratch::written(&layout.manifest, &install.to_canonical());
        }
        let gone_layout = crate::layout(&gone, "");
        nc_platform::create_private_directory(&gone_layout.state).expect("a profile left");
        let record = Record {
            directory: scratch.0.join("checkout"),
            link: scratch.0.join("bin").join("noctua-dev-tl1"),
            installed: 1_000,
        };
        let record_of = |install: &crate::Install| {
            crate::layout(install, "")
                .checkout_record
                .expect("a development install's")
        };
        Scratch::written(&record_of(&recorded), &record.to_canonical());
        Scratch::written(
            &record_of(&unread),
            &"x".repeat(usize::try_from(super::RECORD_BYTES_MAX).expect("small") + 1),
        );
        let listed = super::listed(&recorded, &user).expect("listed");
        let digests: Vec<String> = listed.iter().map(|one| one.checkout.to_string()).collect();
        assert_eq!(
            digests,
            ["0000000a", "0000000b", "0000000c"],
            "by digest, installed alone"
        );
        assert_eq!(listed[2].recorded, super::Recorded::Read(record));
        assert_eq!(
            listed[2].installed,
            Some(1_000),
            "a record's time is when it was installed"
        );
        assert_eq!(listed[0].recorded, super::Recorded::Absent);
        assert!(
            listed[0].installed.is_some_and(|at| at > 1_000),
            "an unrecorded checkout is dated by its manifest"
        );
        assert!(
            matches!(&listed[1].recorded, super::Recorded::Refused(why) if why.contains("more than")),
            "{:?}",
            listed[1].recorded
        );
        assert_eq!(listed[1].cli, super::cli_of(&crate::layout(&unread, "")));
        assert_eq!(
            super::pointed_at(&listed[1].cli, &listed),
            Some(listed[1].checkout)
        );
        assert_eq!(
            super::pointed_at(&listed[1].cli.with_file_name("elsewhere"), &listed),
            None,
            "a link points at a checkout only at exactly its CLI"
        );
        // **A record another account can reach is not read.**
        nc_platform::testing::open_to_another_user(&record_of(&recorded)).expect("opened");
        let opened = super::listed(&recorded, &user).expect("listed");
        assert!(
            matches!(&opened[2].recorded, super::Recorded::Refused(why) if why.contains("was not read")),
            "{:?}",
            opened[2].recorded
        );
    }
}
