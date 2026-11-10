//! Every name an install gives the system, from its manifest and one function.
//!
//! `NOCTUA.md §17.1`: production, staging and development run on one computer at
//! once, and two development checkouts beside them, and nothing one of them does
//! may touch another. So every name the operating system sees — the directories,
//! the socket and the pipe, the units, agents, tasks and mutexes, the CLI's name,
//! the application's identities, the update feed, the default Leader, a checkout's
//! stack — is derived by [`layout`], from an [`Install`] and the user it runs as,
//! and from nowhere else. A second place that spells a name is a second rule, and
//! two rules agree until the day one of them is edited.
//!
//! # An install names itself
//!
//! Every binary reads [`MANIFEST`] beside its own canonicalized executable, and an
//! [`Install`] is what that file says: the channel, the checkout a development
//! install was built from, the base directories resolved when the installer ran,
//! an optional namespace and the log store's endpoint. **The environment is read
//! when the manifest is written, never by a running binary**, so a daemon a
//! service manager starts — which never sees the shell that installed it — names
//! exactly what the installer named. **There is no channel built in to fall back
//! to**: a binary with no manifest refuses to run, because a default would make
//! every binary a build leaves behind a live client of the developer's own
//! channel. Reading the file, and judging it this user's alone, is the binary's;
//! what it says is judged here, by [`Install::parse`], which refuses with an
//! `NC-CHAN` code and writes nothing.
//!
//! # One leaf, in the shape each system wants
//!
//! Each install has one leaf — `noctua`, `noctua-staging` or
//! `noctua-dev-<checkout>`, and `-<namespace>` after it for an install a test
//! relocates — and every name is that leaf written the way its system spells
//! one. With `L` the leaf, `U` the user and `A` the application identity:
//!
//! | Name | Spelled |
//! |---|---|
//! | the profile's directories | `<base>/L`, for each of the four bases |
//! | the install directory | `<state base>/L/bin`, holding [`MANIFEST`] |
//! | the runtime directory | `<state base>/L/run`, on every system |
//! | the socket | `s` in the runtime directory, short because macOS bounds a socket's path |
//! | the CLI, the Run-key value, the URL scheme | `L` |
//! | the daemon's executable | `L-daemon` |
//! | the Windows pipe and mutex | `\\.\pipe\L.U` and `Local\L.U` |
//! | the systemd unit, the scheduled task | `L.U.service` and `\L.U` |
//! | the application identity `A` | `io.github.noctua_world.` and `L` with `_` for `-` |
//! | the launchd label | `A.daemon.U` |
//! | the desktop entry, the tray, the credential store | `A.desktop`, `A.tray`, `A.credentials` |
//! | the update feed and its signing key | `L-updates` and `L-release`, production and staging only |
//! | the stack's compose project and ports | the checkout's, a development install only |
//!
//! **`A` is the organization's GitHub home read as a domain** — the project owns
//! no domain, and `io.github.<owner>` is the form freedesktop gives a project
//! hosted there, with the owner's `-` written `_` because an identity's element
//! may not hold one. **The user is in every name the whole machine shares but
//! a checkout's stack** — pipes, mutexes, units, labels and tasks — so one
//! person's daemon is never another's; the stack's compose project and ports
//! are the checkout's, whoever works in it. **The namespace is in every name
//! but the default Leader**, which is the channel's, so a test's install never
//! reaches a real one.
//!
//! # Why no two leaves meet
//!
//! A leaf's parts are joined by `-`, and a [`Namespace`] holds none, so a
//! relocated leaf has one part more than its channel's; it holds a digit, which
//! no word of a leaf does, so `noctua-<namespace>` is never `noctua-staging` and
//! `L-<namespace>` never `L-daemon`. The user is joined by `.`, which no leaf
//! holds, and written with ASCII letters, digits and `-` alone, every other
//! character spelled out (`_` and its code point in hex, then `_`), so no user
//! can be read as another's name or end one.
//! `crates/nc-channel/tests/the_names_are_disjoint.rs` derives every channel's
//! names for two users, as a real install and relocated under two namespaces,
//! all under one root so that the namespace alone keeps a relocated install's
//! paths apart, and compares them case-folded, as a pipe name and a
//! case-insensitive file system compare them: one user's installs share no
//! name, two users' installs share no pipe, mutex, unit, label or task, and a
//! relocated install shares none with a real one, whoever each runs as.
//!
//! # What a relocated install leaves out
//!
//! An install with a namespace is a test's, so it has **no update feed and no
//! stack**: a test's daemon never updates itself from a real channel's feed, and
//! nothing it derives names a checkout's real compose project.
//!
//! # What this crate does not do
//!
//! It reads no environment and opens no file. Its one call that touches the disk
//! is [`Checkout::of`], which canonicalizes the checkout's path. Which base
//! directories a system has is `nc-platform`'s, asked by an installer; this crate
//! is handed them.

#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
// Every doctest of this crate forbids `unsafe` as the crate does: a
// doctest is a crate of its own, outside the root's `forbid`.
#![doc(test(attr(forbid(unsafe_code))))]

use core::fmt::{self, Write as _};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use nc_canon::Value;
use nc_catalog::ErrorCode;

/// The file an install names itself in, beside each of its executables.
pub const MANIFEST: &str = "install.json";

/// The one manifest format this build reads and writes.
///
/// A manifest of another format is refused rather than read as this one: an
/// installer from a later build may mean something else by the same keys.
pub const FORMAT: i64 = 1;

/// The most characters a [`Namespace`] holds.
///
/// It is in every name, a profile directory's among them, and the socket under
/// that directory is bounded by what macOS lets a socket's path hold.
pub const NAMESPACE_MAX: usize = 16;

/// The first port of the block a checkout's stack takes its ports from.
///
/// Below every system's ephemeral range — Linux starts at 32768, macOS and
/// Windows at 49152 — so no outgoing connection is handed one, and far from
/// the 5432 and the 9428 a stack started before this block still holds.
pub const PORTS_FROM: u16 = 20_000;

/// How many pairs of ports the block holds: the last of them ends at
/// 32765, which is `PORTS_FROM + 2 · 6382 + 1`, inside 20000–32767 and
/// below every system's ephemeral range.
pub const PORT_PAIRS: u16 = 6_383;

/// The domain every application identity sits under; see the crate's header.
const APPLICATION: &str = "io.github.noctua_world";

/// The keys a manifest holds, in the order canonical form writes them.
const KEYS: [&str; 6] = [
    "bases",
    "channel",
    "checkout",
    "format",
    "log_endpoint",
    "namespace",
];

/// The keys the manifest's `bases` holds, in the order canonical form writes
/// them.
const BASE_KEYS: [&str; 4] = ["cache", "config", "data", "state"];

/// The most characters of a manifest's own text a refusal quotes.
const QUOTED_AT_MOST: usize = 64;

/// One of the three channels an install belongs to (`NOCTUA.md §17.1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channel {
    /// What a person runs every day, installed from its releases.
    Production,
    /// The release candidates, installed from their releases beside production.
    Staging,
    /// A build of one checkout of this repository, installed from it.
    Development(Checkout),
}

impl Channel {
    /// The word a manifest writes this channel as.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::Staging => "staging",
            Self::Development(_) => "development",
        }
    }

    /// The channel's default Leader (`NOCTUA.md §12.8`).
    ///
    /// **One per channel, and the checkout plays no part**: two development
    /// checkouts share the development chord until one is configured, and the
    /// configuration refuses the collision (Stage 16) — a default derived from
    /// the checkout would be a chord nobody chose.
    #[must_use]
    pub const fn leader(self) -> Leader {
        match self {
            Self::Production => Leader {
                modifiers: &[Modifier::Alt],
            },
            Self::Staging => Leader {
                modifiers: &[Modifier::Control, Modifier::Alt],
            },
            Self::Development(_) => Leader {
                modifiers: &[Modifier::Alt, Modifier::Super],
            },
        }
    }
}

/// Which checkout of this repository a development install was built from.
///
/// **The FNV-1a digest, 32 bits of it, of the checkout's canonicalized path,
/// written as eight lower-case hexadecimal digits.** The same number names the
/// checkout's compose project, so it is frozen: a digest that moved would rename
/// every checkout's stack and strand the volumes holding its database — this
/// crate's suite and `cargo xtask`'s `the_project_name_does_not_move_under_a_checkout`
/// hold the vectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Checkout {
    /// The digest; [`fmt::Display`] writes it as the eight digits.
    digest: u32,
}

impl Checkout {
    /// The checkout whose root is `root`.
    ///
    /// **Canonicalized first, so a linked path and its target are one checkout
    /// rather than two**, and the path as given when it cannot be — a checkout
    /// that is not there names the same stack every time it is asked about.
    #[must_use]
    pub fn of(root: &Path) -> Self {
        let path = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        Self {
            digest: digest_of(&path.to_string_lossy()),
        }
    }

    /// A checkout as a manifest writes it.
    ///
    /// # Errors
    ///
    /// [`ChannelError::Checkout`] unless `text` is exactly eight lower-case
    /// hexadecimal digits: an upper-case digit, a sign or a ninth digit would be
    /// a second spelling of one checkout, and every name it derives a second
    /// name.
    pub fn parse(text: &str) -> Result<Self, ChannelError> {
        let digits = text.len() == 8
            && text
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        if !digits {
            return Err(ChannelError::Checkout(format!(
                "`{}` is not eight lower-case hexadecimal digits",
                quoted(text)
            )));
        }
        u32::from_str_radix(text, 16)
            .map(|digest| Self { digest })
            .map_err(|error| {
                ChannelError::Checkout(format!("`{}` is not a digest: {error}", quoted(text)))
            })
    }

    /// The local development stack this checkout names: its compose project and
    /// its ports.
    ///
    /// **The checkout's and not an install's**, because `cargo xtask dev` brings
    /// a stack up for the tree it runs in, with nothing installed; [`layout`]
    /// hands a development install the same one. `cargo xtask dev` names its
    /// project from it and binds these ports, unless the variable it reads for
    /// either moves it.
    #[must_use]
    pub fn stack(self) -> Stack {
        let postgres = PORTS_FROM + 2 * pair_of(self.digest);
        Stack {
            project: format!("noctua-central-{self}"),
            postgres,
            logs: postgres + 1,
        }
    }
}

impl fmt::Display for Checkout {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // **`{:08x}`, not `{:x}`**: the short form drops the leading zero for
        // one checkout path in sixteen, renaming its project and stranding its
        // volumes, while every digest stays what it was.
        write!(formatter, "{:08x}", self.digest)
    }
}

/// FNV-1a, 32 bits of it, over the checkout's path.
///
/// **Written here rather than taken from a crate**, and not because a
/// dependency would be heavy: what this number names is a Docker volume holding
/// a developer's database, so it has to mean the same thing in five years. A
/// hasher whose output is documented as unstable between releases — which is
/// every `std` one — would rename the project on a toolchain upgrade and orphan
/// the data. This is eight lines and it is frozen by
/// `the_project_name_does_not_move_under_a_checkout`.
fn digest_of(text: &str) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in text.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// Which pair of the block a digest takes.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the remainder is below `PORT_PAIRS`, a `u16`, so it fits one"
)]
fn pair_of(digest: u32) -> u16 {
    (digest % u32::from(PORT_PAIRS)) as u16
}

/// A namespace an install is relocated under, so that a test's never meets a
/// real one.
///
/// **One to [`NAMESPACE_MAX`] lower-case ASCII letters and digits, at least
/// one of them a digit.** No separator of any kind, so it cannot reach into the
/// next part of a name or out of a directory; lower case, because a pipe and a
/// case-insensitive file system read `T1` as `t1`; and a digit, because no word
/// a leaf is made of holds one — the crate's header says what that keeps apart.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Namespace(String);

impl Namespace {
    /// A namespace as a manifest writes it.
    ///
    /// # Errors
    ///
    /// [`ChannelError::Namespace`] for text outside the alphabet or the length
    /// above, and for text holding no digit.
    pub fn parse(text: &str) -> Result<Self, ChannelError> {
        let alphabet = !text.is_empty()
            && text.len() <= NAMESPACE_MAX
            && text
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit());
        if !alphabet {
            return Err(ChannelError::Namespace(format!(
                "`{}` is not one to {NAMESPACE_MAX} lower-case letters and digits",
                quoted(text)
            )));
        }
        if !text.bytes().any(|byte| byte.is_ascii_digit()) {
            return Err(ChannelError::Namespace(format!(
                "`{text}` holds no digit, so it could spell a word a name is already \
                 made of"
            )));
        }
        Ok(Self(text.to_owned()))
    }

    /// The namespace's text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The four base directories an install was given, resolved when it was
/// installed.
///
/// **Each a plain absolute path**: written as its own components spell it —
/// no `.`, no `..`, no doubled or trailing separator, no control character and
/// no other character that ends a line — and valid UTF-8, because the manifest
/// is text. A path that climbs or loops names one directory two ways, and every
/// comparison of two installs' names is a comparison of their text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bases {
    /// What the profile keeps: its database, its blobs, its plugins, its device.
    data: PathBuf,
    /// What the profile is configured with: its world.
    config: PathBuf,
    /// What the profile can lose.
    cache: PathBuf,
    /// What the profile's running keeps between runs, and its executables.
    state: PathBuf,
}

impl Bases {
    /// The four bases, each judged plain and absolute.
    ///
    /// # Errors
    ///
    /// [`ChannelError::Base`] naming the first base that is not.
    pub fn new(
        data: PathBuf,
        config: PathBuf,
        cache: PathBuf,
        state: PathBuf,
    ) -> Result<Self, ChannelError> {
        for (key, path) in [
            ("data", &data),
            ("config", &config),
            ("cache", &cache),
            ("state", &state),
        ] {
            plain(key, path)?;
        }
        Ok(Self {
            data,
            config,
            cache,
            state,
        })
    }

    /// The base of what the profile keeps.
    #[must_use]
    pub fn data(&self) -> &Path {
        &self.data
    }

    /// The base of what the profile is configured with.
    #[must_use]
    pub fn config(&self) -> &Path {
        &self.config
    }

    /// The base of what the profile can lose.
    #[must_use]
    pub fn cache(&self) -> &Path {
        &self.cache
    }

    /// The base of what the profile's running keeps, and its executables.
    #[must_use]
    pub fn state(&self) -> &Path {
        &self.state
    }
}

/// Whether `path` is a plain absolute path, for the base named `key`.
fn plain(key: &str, path: &Path) -> Result<(), ChannelError> {
    let Some(text) = path.to_str() else {
        return Err(ChannelError::Base(format!(
            "the {key} base `{}` is not UTF-8, and a manifest is text",
            quoted(&path.to_string_lossy())
        )));
    };
    // **Cc, and the two line breaks Cc leaves out**: U+2028 and U+2029 end a
    // line wherever text is read by Unicode's rules, and a base is written into
    // every path the profile has. Not `nc_canon::is_unsafe_in_a_message`,
    // which refuses `\`, Windows' separator, and `"`, which a Unix path holds.
    if text
        .chars()
        .any(|character| character.is_control() || matches!(character, '\u{2028}' | '\u{2029}'))
    {
        return Err(ChannelError::Base(format!(
            "the {key} base `{}` holds a control character or a line break",
            quoted(text)
        )));
    }
    if !path.is_absolute() {
        return Err(ChannelError::Base(format!(
            "the {key} base `{}` is not an absolute path",
            quoted(text)
        )));
    }
    // **Its own components, joined again, must spell it.** That one comparison
    // refuses a `.`, a doubled separator and a trailing one, which `components`
    // drops; a `..` it keeps, so it is refused by name.
    let spelled: PathBuf = path.components().collect();
    if spelled.as_os_str() != path.as_os_str()
        || path
            .components()
            .any(|component| component == Component::ParentDir)
    {
        return Err(ChannelError::Base(format!(
            "the {key} base `{}` is not written as its own components spell it",
            quoted(text)
        )));
    }
    Ok(())
}

/// What an install's manifest says (`NOCTUA.md §17.1`, "An install names
/// itself").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Install {
    /// The channel, and a development install's checkout.
    pub channel: Channel,
    /// The base directories, resolved when it was installed.
    pub bases: Bases,
    /// The namespace a test relocated it under, and `None` for a real install.
    pub namespace: Option<Namespace>,
    /// Where its daemon ships its log, judged by `nc-log` when the daemon opens
    /// it, which is the one reader of an endpoint; `None` writes to stderr.
    pub log_endpoint: Option<String>,
}

impl Install {
    /// The install a manifest's text describes.
    ///
    /// Any JSON `nc-canon` reads is read, so a manifest a person reformatted is
    /// still one; what [`Install::to_canonical`] writes is the form an installer
    /// leaves.
    ///
    /// # Errors
    ///
    /// - [`ChannelError::Unreadable`]: not a JSON object `nc-canon` reads; a key
    ///   this build does not read, at the top or under `bases`; a key missing; a
    ///   value of the wrong type, `null` included; or a `format` other than
    ///   [`FORMAT`].
    /// - [`ChannelError::Channel`]: a channel that is not `production`,
    ///   `staging` or `development`.
    /// - [`ChannelError::Checkout`]: a development install naming no checkout,
    ///   another channel naming one, or a checkout [`Checkout::parse`] refuses.
    /// - [`ChannelError::Base`]: a base [`Bases::new`] refuses.
    /// - [`ChannelError::Namespace`]: a namespace [`Namespace::parse`] refuses.
    pub fn parse(text: &str) -> Result<Self, ChannelError> {
        // **Carried as it comes**: of the text, `nc-canon`'s refusal quotes a
        // key written twice and nothing else, escaped and cut at as many
        // characters as `quoted` cuts at.
        let value = nc_canon::parse(text)
            .map_err(|error| unreadable(&format!("it is not JSON this build reads: {error}")))?;
        let mut fields = value
            .into_object()
            .ok_or_else(|| unreadable("it is not a JSON object"))?;
        only_known(&fields, &KEYS, "")?;

        let format = integer(&mut fields, "format")?;
        if format != FORMAT {
            return Err(unreadable(&format!(
                "its format is {format}, and this build reads format {FORMAT}"
            )));
        }

        let word = required_text(&mut fields, "channel")?;
        let checkout = optional_text(&mut fields, "checkout")?;
        let channel = match (word.as_str(), checkout) {
            ("development", Some(checkout)) => Channel::Development(Checkout::parse(&checkout)?),
            ("development", None) => {
                return Err(ChannelError::Checkout(
                    "a development install names the checkout it was built from, and this \
                     one names none"
                        .to_owned(),
                ));
            }
            ("production" | "staging", Some(_)) => {
                return Err(ChannelError::Checkout(format!(
                    "a checkout is a development install's, and this one is {word}"
                )));
            }
            ("production", None) => Channel::Production,
            ("staging", None) => Channel::Staging,
            (other, _) => {
                return Err(ChannelError::Channel(format!(
                    "`{}` is not production, staging or development",
                    quoted(other)
                )));
            }
        };

        let mut bases = fields
            .remove("bases")
            .ok_or_else(|| unreadable("it has no `bases`"))?
            .into_object()
            .ok_or_else(|| unreadable("its `bases` is not an object"))?;
        only_known(&bases, &BASE_KEYS, "bases.")?;
        let mut base = |key: &str| {
            required_text(&mut bases, key)
                .map(PathBuf::from)
                .map_err(|refusal| match refusal {
                    ChannelError::Unreadable(what) => {
                        ChannelError::Unreadable(format!("{what}, under `bases`"))
                    }
                    other => other,
                })
        };
        let bases = Bases::new(
            base("data")?,
            base("config")?,
            base("cache")?,
            base("state")?,
        )?;

        let namespace = optional_text(&mut fields, "namespace")?
            .map(|text| Namespace::parse(&text))
            .transpose()?;
        let log_endpoint = optional_text(&mut fields, "log_endpoint")?;
        Ok(Self {
            channel,
            bases,
            namespace,
            log_endpoint,
        })
    }

    /// The manifest an installer writes for this install: canonical JSON, keys
    /// in order, nothing absent written as `null`.
    #[must_use]
    pub fn to_canonical(&self) -> String {
        let text = |path: &Path| Value::text(path.to_string_lossy());
        let mut fields = vec![
            ("format", Value::Integer(FORMAT)),
            ("channel", Value::text(self.channel.word())),
            (
                "bases",
                Value::object([
                    ("data", text(&self.bases.data)),
                    ("config", text(&self.bases.config)),
                    ("cache", text(&self.bases.cache)),
                    ("state", text(&self.bases.state)),
                ]),
            ),
        ];
        if let Channel::Development(checkout) = self.channel {
            fields.push(("checkout", Value::text(checkout.to_string())));
        }
        if let Some(namespace) = &self.namespace {
            fields.push(("namespace", Value::text(namespace.as_str())));
        }
        if let Some(endpoint) = &self.log_endpoint {
            fields.push(("log_endpoint", Value::text(endpoint.as_str())));
        }
        Value::object(fields).to_canonical()
    }
}

/// A refusal of everything but the keys `known`, under `prefix`.
fn only_known(
    fields: &BTreeMap<String, Value>,
    known: &[&str],
    prefix: &str,
) -> Result<(), ChannelError> {
    match fields.keys().find(|key| !known.contains(&key.as_str())) {
        Some(key) => Err(unreadable(&format!(
            "it holds `{prefix}{}`, a key this build does not read",
            quoted(key)
        ))),
        None => Ok(()),
    }
}

/// The integer under `key`, which must be there.
fn integer(fields: &mut BTreeMap<String, Value>, key: &str) -> Result<i64, ChannelError> {
    match fields.remove(key) {
        Some(Value::Integer(number)) => Ok(number),
        Some(_) => Err(unreadable(&format!("its `{key}` is not a whole number"))),
        None => Err(unreadable(&format!("it has no `{key}`"))),
    }
}

/// The text under `key`, which must be there.
fn required_text(fields: &mut BTreeMap<String, Value>, key: &str) -> Result<String, ChannelError> {
    optional_text(fields, key)?.ok_or_else(|| unreadable(&format!("it has no `{key}`")))
}

/// The text under `key`, if the key is there; anything else under it is refused.
fn optional_text(
    fields: &mut BTreeMap<String, Value>,
    key: &str,
) -> Result<Option<String>, ChannelError> {
    match fields.remove(key) {
        Some(value) => value
            .into_text()
            .map(Some)
            .ok_or_else(|| unreadable(&format!("its `{key}` is not text"))),
        None => Ok(None),
    }
}

/// A refusal of the manifest as a whole, saying what was wrong with it.
fn unreadable(what: &str) -> ChannelError {
    ChannelError::Unreadable(what.to_owned())
}

/// A manifest's own text, as a refusal may quote it: no character that would
/// end a line or a field, and no more than [`QUOTED_AT_MOST`] characters.
fn quoted(text: &str) -> String {
    let mut out: String = text
        .chars()
        .take(QUOTED_AT_MOST)
        .map(nc_canon::shown)
        .collect();
    if text.chars().nth(QUOTED_AT_MOST).is_some() {
        out.push('…');
    }
    out
}

/// Why a manifest was refused.
///
/// **Every variant is permanent**: the manifest is the installer's, so reading
/// it again reads the same bytes, and what fixes it is installing again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelError {
    /// Not a manifest this build reads at all; the text says what was wrong.
    Unreadable(String),
    /// A channel this build does not know.
    Channel(String),
    /// A checkout missing, misplaced or malformed.
    Checkout(String),
    /// A base directory that is not a plain absolute path.
    Base(String),
    /// A namespace outside its alphabet.
    Namespace(String),
}

impl ChannelError {
    /// The catalog code this maps to, which is what a log is queried by.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::Unreadable(_) => ErrorCode::ChannelManifestUnreadable,
            Self::Channel(_) => ErrorCode::ChannelUnknown,
            Self::Checkout(_) => ErrorCode::ChannelCheckoutMalformed,
            Self::Base(_) => ErrorCode::ChannelBaseMalformed,
            Self::Namespace(_) => ErrorCode::ChannelNamespaceMalformed,
        }
    }
}

impl fmt::Display for ChannelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (Self::Unreadable(what)
        | Self::Channel(what)
        | Self::Checkout(what)
        | Self::Base(what)
        | Self::Namespace(what)) = self;
        write!(
            formatter,
            "[{}] the install manifest was refused: {what}. Install again with the \
             installer that wrote it",
            self.code().code()
        )
    }
}

impl std::error::Error for ChannelError {}

/// A key held down for the Leader; the chord always ends with Space.
///
/// **A physical key, never a character**, as every chord is (`NOCTUA.md
/// §12.8`): the same key is Super on Linux, Win on Windows and Command on a Mac.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Modifier {
    /// Ctrl, or Control on a Mac.
    Control,
    /// Alt, or Option on a Mac.
    Alt,
    /// Super, Win on Windows, or Command on a Mac.
    Super,
}

impl Modifier {
    /// How `keyboard` labels this key.
    #[must_use]
    pub const fn written_for(self, keyboard: Keyboard) -> &'static str {
        match (self, keyboard) {
            (Self::Control, Keyboard::Mac) => "Control",
            (Self::Control, Keyboard::Linux | Keyboard::Windows) => "Ctrl",
            (Self::Alt, Keyboard::Mac) => "Option",
            (Self::Alt, Keyboard::Linux | Keyboard::Windows) => "Alt",
            (Self::Super, Keyboard::Mac) => "Command",
            (Self::Super, Keyboard::Windows) => "Win",
            (Self::Super, Keyboard::Linux) => "Super",
        }
    }
}

/// Whose labels a chord is written in.
///
/// **Named by the caller, never asked of the system this runs on**, so the
/// three spellings are one table every build can test (`NOCTUA.md §2.7`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyboard {
    /// A Linux keyboard.
    Linux,
    /// A Windows keyboard.
    Windows,
    /// A Mac keyboard.
    Mac,
}

/// A channel's default Leader: the keys held, then Space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Leader {
    /// The keys held, in the order `NOCTUA.md §12.8` writes them.
    modifiers: &'static [Modifier],
}

impl Leader {
    /// The keys held with Space.
    #[must_use]
    pub const fn modifiers(self) -> &'static [Modifier] {
        self.modifiers
    }

    /// The chord as `keyboard` labels it: `Ctrl+Alt+Space`, `Option+Space`.
    #[must_use]
    pub fn written_for(self, keyboard: Keyboard) -> String {
        let mut keys: Vec<&str> = self
            .modifiers
            .iter()
            .map(|modifier| modifier.written_for(keyboard))
            .collect();
        keys.push("Space");
        keys.join("+")
    }
}

/// Where a published channel's updates come from, and the key that signs them
/// (`NOCTUA.md §17.6`, `§17.7`).
///
/// Names, not addresses: what each resolves to — a URL, a pinned key — is the
/// updater's, which is Stage 19's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Update {
    /// The feed the channel follows.
    pub feed: String,
    /// The key its releases are signed with.
    pub signing_key: String,
}

/// A checkout's local development stack: its compose project and its ports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stack {
    /// The Docker Compose project, `noctua-central-<checkout>`.
    ///
    /// **Frozen**: what `cargo xtask dev` named this checkout's stack before
    /// this crate existed, so the volumes holding its database keep their name.
    pub project: String,
    /// PostgreSQL's port: `PORTS_FROM + 2 · (digest mod PORT_PAIRS)`.
    pub postgres: u16,
    /// The log store's port: PostgreSQL's, plus one.
    pub logs: u16,
}

/// Every name an install gives the system (`NOCTUA.md §17.1`).
///
/// **Exhaustive on purpose**: `crates/nc-channel/tests/the_names_are_disjoint.rs`
/// destructures it without `..`, so a field added here fails to compile there
/// until it is held apart from every other install's — a name the test did not
/// see is a name two installs may share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// Where the executables and [`MANIFEST`] are: `<state>/bin`.
    pub install: PathBuf,
    /// The profile's data directory: `<data base>/<leaf>`.
    pub data: PathBuf,
    /// The profile's configuration directory: `<config base>/<leaf>`.
    pub config: PathBuf,
    /// The profile's cache directory: `<cache base>/<leaf>`.
    pub cache: PathBuf,
    /// The profile's state directory: `<state base>/<leaf>`.
    pub state: PathBuf,
    /// The log directory: `<state>/log`.
    pub log: PathBuf,
    /// The runtime directory, `<state>/run` on every system: private, made by
    /// the daemon, and never a system's shared temporary one.
    pub runtime: PathBuf,
    /// The manifest: `<install>/install.json`.
    pub manifest: PathBuf,
    /// The plugins installed in the profile: `<data>/plugins`.
    pub plugins: PathBuf,
    /// The bytes of the blobs the profile keeps: `<data>/blobs`.
    pub blobs: PathBuf,
    /// The device's secret seed: `<data>/device.key`.
    pub device_key: PathBuf,
    /// The device's public key and flake: `<data>/device.json`.
    pub device: PathBuf,
    /// The realm and person the profile decides as: `<config>/world.json`.
    pub world: PathBuf,
    /// The identifiers' persisted high-water mark: `<state>/ids.mark`.
    pub mark: PathBuf,
    /// The explicit-stop mark every restorer honours: `<state>/stopped`, in
    /// the persistent state directory and never a temporary one.
    pub stopped: PathBuf,
    /// The lock one daemon of the profile holds: `<state>/instance.lock`.
    pub instance_lock: PathBuf,
    /// The PID record: `<runtime>/daemon.pid`.
    pub pid_file: PathBuf,
    /// The token the IPC handshake is keyed by: `<runtime>/token`.
    pub token: PathBuf,
    /// The IPC socket on Linux and macOS: `<runtime>/s`.
    pub socket: PathBuf,
    /// A daemon started detached writes its standard error here, `<log>/daemon.err`.
    pub daemon_err: PathBuf,
    /// The CLI's name on `PATH`, without a system's executable suffix.
    pub cli: String,
    /// The daemon's executable, without a system's executable suffix.
    pub daemon: String,
    /// The IPC named pipe on Windows.
    pub pipe: String,
    /// The Windows mutex.
    pub mutex: String,
    /// The systemd user unit.
    pub systemd_unit: String,
    /// The launchd agent's label.
    pub launchd_label: String,
    /// The scheduled task's path, in the root folder.
    pub task: String,
    /// The value under the user's Run key.
    pub run_key: String,
    /// The application identity.
    pub app_id: String,
    /// The desktop entry's file name.
    pub desktop_entry: String,
    /// The tray icon's identity.
    pub tray_id: String,
    /// The URL scheme the interface handles.
    pub url_scheme: String,
    /// The service the credential store files the profile's secrets under.
    pub credential_service: String,
    /// The default Leader, which is the channel's.
    pub leader: Leader,
    /// The update feed and its key: production and staging only.
    pub update: Option<Update>,
    /// The checkout's stack: a development install only.
    pub stack: Option<Stack>,
}

/// Every name `install` gives the system, for `user` — **the one function**
/// (`NOCTUA.md §17.1`).
///
/// `user` is how the system names whoever the install runs as — the uid on
/// Unix, the SID on Windows, as `nc_platform::User` writes it — and is carried
/// by every name the whole machine shares but the stack's, which is the
/// checkout's.
#[must_use]
pub fn layout(install: &Install, user: &str) -> Layout {
    let leaf = leaf(install.channel, install.namespace.as_ref());
    let user = spelled(user);
    let data = install.bases.data.join(&leaf);
    let config = install.bases.config.join(&leaf);
    let cache = install.bases.cache.join(&leaf);
    let state = install.bases.state.join(&leaf);
    let install_directory = state.join("bin");
    let log = state.join("log");
    let runtime = state.join("run");
    let app_id = format!("{APPLICATION}.{}", leaf.replace('-', "_"));
    let relocated = install.namespace.is_some();
    Layout {
        manifest: install_directory.join(MANIFEST),
        plugins: data.join("plugins"),
        blobs: data.join("blobs"),
        device_key: data.join("device.key"),
        device: data.join("device.json"),
        world: config.join("world.json"),
        mark: state.join("ids.mark"),
        stopped: state.join("stopped"),
        instance_lock: state.join("instance.lock"),
        pid_file: runtime.join("daemon.pid"),
        token: runtime.join("token"),
        socket: runtime.join("s"),
        daemon_err: log.join("daemon.err"),
        cli: leaf.clone(),
        daemon: format!("{leaf}-daemon"),
        pipe: format!(r"\\.\pipe\{leaf}.{user}"),
        mutex: format!(r"Local\{leaf}.{user}"),
        systemd_unit: format!("{leaf}.{user}.service"),
        launchd_label: format!("{app_id}.daemon.{user}"),
        task: format!(r"\{leaf}.{user}"),
        run_key: leaf.clone(),
        desktop_entry: format!("{app_id}.desktop"),
        tray_id: format!("{app_id}.tray"),
        credential_service: format!("{app_id}.credentials"),
        url_scheme: leaf.clone(),
        app_id,
        leader: install.channel.leader(),
        update: match install.channel {
            Channel::Production | Channel::Staging if !relocated => Some(Update {
                feed: format!("{leaf}-updates"),
                signing_key: format!("{leaf}-release"),
            }),
            _ => None,
        },
        stack: match install.channel {
            Channel::Development(checkout) if !relocated => Some(checkout.stack()),
            _ => None,
        },
        install: install_directory,
        data,
        config,
        cache,
        state,
        log,
        runtime,
    }
}

/// The leaf every name of an install is written from.
fn leaf(channel: Channel, namespace: Option<&Namespace>) -> String {
    let mut leaf = match channel {
        Channel::Production => "noctua".to_owned(),
        Channel::Staging => "noctua-staging".to_owned(),
        Channel::Development(checkout) => format!("noctua-dev-{checkout}"),
    };
    if let Some(namespace) = namespace {
        leaf.push('-');
        leaf.push_str(namespace.as_str());
    }
    leaf
}

/// A user as a name carries one: ASCII letters, digits and `-` as they are,
/// every other character as `_`, its code point in hex and `_` again.
///
/// **Injective, and never holding a separator**: `_` itself is spelled out, so
/// no two users are written alike, and no user can hold the `.` that ends it,
/// the `\` that would open a folder or the `/` that would climb out of one.
/// Nobody is no user: the empty text is `_`, which no spelling of a character
/// is.
fn spelled(user: &str) -> String {
    let mut out = String::with_capacity(user.len());
    for character in user.chars() {
        if character.is_ascii_alphanumeric() || character == '-' {
            out.push(character);
        } else {
            // Writing to a `String` cannot fail.
            let _ = write!(out, "_{:x}_", u32::from(character));
        }
    }
    if out.is_empty() {
        out.push('_');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{digest_of, spelled};

    /// The digest is the one `cargo xtask dev` named every stack with.
    ///
    /// **Frozen.** If either number changes, every existing checkout's stack is
    /// renamed and its volumes are stranded under the old name.
    #[test]
    fn the_digest_is_the_one_every_stack_was_named_with() {
        assert_eq!(digest_of("/home/dev/noctua-central"), 0x5e24_507c);
        assert_eq!(digest_of(""), 0x811c_9dc5);
    }

    /// A user is written with no separator a name could be split at, and no two
    /// users alike.
    #[test]
    fn a_user_is_spelled_without_a_separator_and_never_like_another() {
        assert_eq!(spelled("1000"), "1000");
        assert_eq!(spelled("S-1-5-21-1-2-3-1001"), "S-1-5-21-1-2-3-1001");
        assert_eq!(spelled("a.b"), "a_2e_b");
        // **Nobody is no user**: a name ending at its separator would read as
        // a name with its user cut off.
        assert_eq!(spelled(""), "_");
        // **The `_` on each side of a code point is what keeps two users
        // apart**, so the users compared include pairs that meet without
        // either one: `1.` and `\u{12e}` are both written `12e_` without the
        // opening `_`, and `a.b` and `a\u{2eb}` are both written `a_2eb`
        // without the closing one.
        let users = [
            "a.b", "a_2e_b", "a/b", r"a\b", "a_b", "", "_", "é", "1.", "\u{12e}", "a\u{2eb}",
        ];
        let written: Vec<String> = users.iter().map(|user| spelled(user)).collect();
        for (at, one) in written.iter().enumerate() {
            assert!(
                one.chars()
                    .all(|character| character.is_ascii_alphanumeric()
                        || character == '-'
                        || character == '_'),
                "{:?} was written {one:?}",
                users[at]
            );
            for (other_at, other) in written.iter().enumerate() {
                if at != other_at {
                    assert_ne!(one, other, "{:?} and {:?}", users[at], users[other_at]);
                }
            }
        }
    }
}
