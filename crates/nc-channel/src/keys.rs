//! A person's choice of keys: the channel's own keybinding configuration,
//! read privately and judged by the Leader's grammar against the catalog's
//! bindings and every other channel's Leader (`NOCTUA.md §12.8`, `§17.1`).
//!
//! # The file
//!
//! [`FILE`] in the profile's configuration directory,
//! [`Layout::keybindings`]: canonical JSON as `nc-canon` reads it, at most
//! [`BYTES_MAX`] bytes, holding `format`, which is [`FORMAT`] and required,
//! and three keys more, each optional — `leader`, a chord as the grammar
//! writes one; `chord`, the word of a modifier; and `bindings`, each command's
//! name beside the bindings it is pressed with, which replace the catalog's for
//! that command, `[]` unbinding it:
//!
//! ```text
//! {"bindings":{"kernel.redo":[],"kernel.undo":["Leader+KeyZ"]},"chord":"Shift","format":1,"leader":"Control+Super+Space"}
//! ```
//!
//! A file that is not this user's alone, larger than its bound, or not one
//! this build reads — a key it does not read, a value of the wrong type,
//! another format, a binding or a chord outside the grammar, a name no command
//! can have — is refused whole, and the channel's defaults stand.
//!
//! # Two parts, judged apart
//!
//! The Leader part is `leader` and `chord`, and the bindings part is
//! `bindings`. A part that breaks a rule of `§12.8` is refused alone, with
//! its code, and its defaults stand while the other part is in force: a Leader
//! without a modifier, one holding its own chord modifier or one that meets
//! another channel's refuses the Leader part, and a key of one table bound
//! twice refuses the bindings part. Whether a Leader part is in force never
//! depends on bindings, so another channel's is judged without its catalog.
//!
//! # The other channels, read and never written
//!
//! A configured Leader is judged against the other channels' (`§17.1`),
//! found from this install's own bases and namespace alone: production and
//! staging by the names [`layout`] gives them, and each development checkout
//! by a directory of the state base whose name is a checkout's leaf under
//! this namespace and no other, so a test's install never reads a real one's,
//! nor the other way round. A channel counts only while its install manifest
//! is there, so an uninstalled checkout binds nothing; the state base is the
//! one listed because the manifest is kept there, so no installed checkout is
//! missed for a configuration directory it does not have.
//!
//! The channels are settled in one order — production, staging, then the
//! checkouts by digest — by [`nc_schema::keys::settle`]: a configured Leader
//! is judged against the Leaders settled before it and the defaults after
//! it, so production reads nobody's file, a channel reads only the files of
//! those before it, and every channel settles the same prefix from the same
//! files. A file before this one that its own reader would refuse is its
//! channel's defaults, as that reader takes it; one this build cannot judge —
//! of a later format, not read because the disk did not answer, of a channel
//! whose manifest the disk did not answer for, or beside more than
//! [`CHANNELS_MAX`] checkouts — refuses this channel's configured Leader
//! rather than guess. **Nothing of another channel is written, locked or
//! removed**: its directory is listed, its manifest asked whether it is there,
//! and its file judged and read.
//!
//! # What this answers (`§2.4`)
//!
//! Another account choosing this user's keys: every file and its directory
//! are judged this user's alone before they are read, a link however it
//! points included. A file growing without bound: it is read no further than
//! one byte past [`BYTES_MAX`], and one longer than [`BYTES_MAX`] is refused;
//! a choice handed to [`Configuration::parse`] is held to the same bound, so
//! no writer leaves a file its reader refuses for its size. A
//! text from the file forging what a refusal says: each piece a refusal quotes
//! as it was written — a key, a word, a chord or a binding the grammar cannot
//! read — is escaped and cut at 64 characters, as the manifest's are, and a
//! command's name is repeated whole only once it is one a command can have, at
//! most [`nc_schema::NAME_MAX`] characters of lower-case letters, digits, `-`
//! and `.`. A binding reaching what the command list could not: it names one
//! key of a closed vocabulary and one command's name, and carries no argument.
//!
//! # What this does not answer
//!
//! A process running as this user, which can write the file as a person's
//! settings would; installs whose bases differ from this one's, which it
//! cannot find; and the moment between a read here and the binding of a key,
//! which is the interface's.

use core::fmt;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use nc_canon::Value;
use nc_catalog::ErrorCode;
use nc_platform::{Judged, Kind, User};
use nc_schema::keys::{
    self, Binding, Chord, Claim, KeyBoundTwice, Keyboard, Leader, LeaderPart, LeaderRule, Modifier,
    Tables, Verdict, Warning,
};

use crate::found::Found;
use crate::{Channel, Checkout, Install, Layout, Namespace, layout, quoted};

/// The file a person's keybinding configuration is, in the profile's
/// configuration directory.
pub const FILE: &str = "keybindings.json";

/// The one format of the file this build reads.
///
/// **A key added to the vocabulary is a new format**: a reader older than it
/// would refuse a binding it cannot name, and another channel's reader must
/// know that it cannot judge the file rather than read it as its own.
pub const FORMAT: i64 = 1;

/// The most bytes the file may hold.
///
/// One byte more is read, only to tell a file past it. Far above any person's
/// choice — a binding is about twenty bytes — and a bound on what one read
/// costs, whoever wrote the file.
pub const BYTES_MAX: u64 = 16_384;

/// The most development checkouts a configured Leader is judged beside.
///
/// More is a state base nobody keeps by hand, and judging past it would read
/// without bound; it refuses this channel's configured Leader.
pub const CHANNELS_MAX: usize = 64;

/// The keys the file holds besides `format`, in the order canonical form
/// writes them.
const FIELDS: [&str; 3] = ["bindings", "chord", "leader"];

/// A person's keybinding configuration, as the file holds it.
///
/// **Sealed**: one is made by [`Configuration::parse`], so every one held is
/// one the grammar read within [`BYTES_MAX`], and
/// [`Configuration::to_canonical`] writes it back within that bound too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Configuration {
    /// The chord configured as the Leader, judged a Leader when it is weighed.
    leader: Option<Chord>,
    /// The modifier configured as the chord modifier.
    chord: Option<Modifier>,
    /// Each command's bindings, by its name, in the order canonical form
    /// writes them.
    bindings: Option<Vec<(String, Vec<Binding>)>>,
}

/// Why a file was not read as a configuration.
enum Unparsed {
    /// It is not one this build reads; the text says what is wrong.
    Unreadable(String),
    /// It names a format later than this build's, which a later build may read.
    Later(i64),
}

impl Configuration {
    /// The configuration `text` holds.
    ///
    /// # Errors
    ///
    /// [`KeysRefused::Unreadable`] for text that is not a configuration of
    /// [`FORMAT`] as the crate's header describes it, saying what is wrong —
    /// text longer than [`BYTES_MAX`] among it, as the file would be.
    pub fn parse(text: &str) -> Result<Self, KeysRefused> {
        parsed(text).map_err(|unparsed| match unparsed {
            Unparsed::Unreadable(what) => KeysRefused::Unreadable(what),
            Unparsed::Later(format) => KeysRefused::Unreadable(format!(
                "its format is {format}, and this build reads format {FORMAT}"
            )),
        })
    }

    /// The file a writer leaves for this configuration: canonical JSON, keys in
    /// order, nothing absent written.
    #[must_use]
    pub fn to_canonical(&self) -> String {
        let mut fields = vec![("format", Value::Integer(FORMAT))];
        if let Some(leader) = self.leader {
            fields.push(("leader", Value::text(leader.written())));
        }
        if let Some(chord) = self.chord {
            fields.push(("chord", Value::text(chord.word())));
        }
        if let Some(bindings) = &self.bindings {
            fields.push((
                "bindings",
                Value::object(bindings.iter().map(|(command, bound)| {
                    (
                        command.clone(),
                        Value::Array(
                            bound
                                .iter()
                                .map(|binding| Value::text(binding.written()))
                                .collect(),
                        ),
                    )
                })),
            ));
        }
        Value::object(fields).to_canonical()
    }

    /// The chord configured as the Leader, if one is.
    #[must_use]
    pub const fn leader(&self) -> Option<Chord> {
        self.leader
    }

    /// The modifier configured as the chord modifier, if one is.
    #[must_use]
    pub const fn chord(&self) -> Option<Modifier> {
        self.chord
    }

    /// Each command's configured bindings, if the file names any.
    #[must_use]
    pub fn bindings(&self) -> Option<&[(String, Vec<Binding>)]> {
        self.bindings.as_deref()
    }

    /// The Leader part this configures over `default`.
    ///
    /// The chord and the modifier it names, the default's standing for the one
    /// it leaves out, and nothing when it names neither.
    fn leader_part(&self, default: LeaderPart) -> Option<(Chord, Modifier)> {
        match (self.leader, self.chord) {
            (None, None) => None,
            (leader, chord) => Some((
                leader.unwrap_or(default.leader.chord()),
                chord.unwrap_or(default.chord),
            )),
        }
    }
}

/// The configuration `text` holds, or why it holds none.
///
/// **The format first**, before any key is refused: a later format may hold
/// keys this build does not read, and another channel's reader must tell a
/// file it cannot judge from one it can.
fn parsed(text: &str) -> Result<Configuration, Unparsed> {
    let unreadable = |what: String| Unparsed::Unreadable(what);
    // **The file's bound, whoever hands the text**, so a writer is held to
    // what the reader reads. The canonical form written back is never longer
    // than the text: every name, word, chord and binding the grammar keeps is
    // ASCII that form writes as it is read, and it adds no space.
    bounded(text.len()).map_err(unreadable)?;
    // `nc-canon` quotes a key written twice escaped and cut, and nothing else
    // of the text.
    let value = nc_canon::parse(text)
        .map_err(|error| unreadable(format!("it is not JSON this build reads: {error}")))?;
    let mut fields = value
        .into_object()
        .ok_or_else(|| unreadable("it is not a JSON object".to_owned()))?;
    match fields.remove("format") {
        Some(Value::Integer(format)) if format > FORMAT => return Err(Unparsed::Later(format)),
        Some(Value::Integer(FORMAT)) => {}
        Some(Value::Integer(format)) => {
            return Err(unreadable(format!(
                "its format is {format}, and this build reads format {FORMAT}"
            )));
        }
        Some(_) => return Err(unreadable("its `format` is not a whole number".to_owned())),
        None => return Err(unreadable("it has no `format`".to_owned())),
    }
    if let Some(key) = fields.keys().find(|key| !FIELDS.contains(&key.as_str())) {
        return Err(unreadable(format!(
            "it holds `{}`, a key this build does not read",
            quoted(key)
        )));
    }
    let text_of = |key: &str, value: Value| {
        value
            .into_text()
            .ok_or_else(|| unreadable(format!("its `{key}` is not text")))
    };
    let leader = match fields.remove("leader") {
        None => None,
        Some(value) => Some(
            Chord::parse(&text_of("leader", value)?).map_err(|malformed| {
                unreadable(format!("its `leader` is no chord: {malformed}"))
            })?,
        ),
    };
    let chord = match fields.remove("chord") {
        None => None,
        Some(value) => {
            let word = text_of("chord", value)?;
            Some(Modifier::of_word(&word).ok_or_else(|| {
                unreadable(format!(
                    "its `chord` is `{}`, which is none of `Control`, `Alt`, `Shift` and `Super`",
                    quoted(&word)
                ))
            })?)
        }
    };
    let bindings = match fields.remove("bindings") {
        None => None,
        Some(value) => Some(
            value
                .into_object()
                .ok_or_else(|| unreadable("its `bindings` is not an object".to_owned()))?
                .into_iter()
                .map(|(command, bound)| bindings_of(command, bound))
                .collect::<Result<Vec<_>, String>>()
                .map_err(unreadable)?,
        ),
    };
    Ok(Configuration {
        leader,
        chord,
        bindings,
    })
}

/// The bindings `bound` names for `command`, each read by the grammar.
fn bindings_of(command: String, bound: Value) -> Result<(String, Vec<Binding>), String> {
    if !nc_schema::command_name_is_well_formed(&command) {
        return Err(format!(
            "its `bindings` names `{}`, which no command can be named",
            quoted(&command)
        ));
    }
    let list = bound
        .into_array()
        .ok_or_else(|| format!("the bindings of `{command}` are not a list"))?;
    let bindings = list
        .into_iter()
        .map(|binding| {
            let text = binding
                .into_text()
                .ok_or_else(|| format!("a binding of `{command}` is not text"))?;
            Binding::parse(&text)
                .map_err(|malformed| format!("a binding of `{command}` is none: {malformed}"))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok((command, bindings))
}

/// Which part of a configuration a refusal leaves to its defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// Both: the file was not read as a configuration at all.
    Whole,
    /// The Leader and the chord modifier.
    Leader,
    /// The bindings.
    Bindings,
}

/// Why a configuration, or one part of it, is not in force.
///
/// Every text a variant holds is one a refusal may show: each piece of the
/// file it quotes as it was written is escaped and cut at 64 characters, and a
/// command's name is repeated only once it is one a command can have.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeysRefused {
    /// The file, or the directory holding it, is not this user's alone; the
    /// text names the path and what it is.
    NotPrivate(String),
    /// The filesystem did not answer for the file or its directory; the text
    /// says what was being done and what it answered.
    Unavailable(String),
    /// The file is not a configuration this build reads; the text says what
    /// is wrong with it.
    Unreadable(String),
    /// The configured Leader part breaks `rule`: alone, or against `by`.
    Leader {
        /// The chord configured as the Leader.
        leader: Chord,
        /// The chord modifier it is held with.
        chord: Modifier,
        /// The rule it breaks.
        rule: LeaderRule,
        /// The channel whose Leader it meets, if the rule is one between two.
        by: Option<Channel>,
    },
    /// The configured bindings bind a key of one table twice, rule 4.
    KeyBoundTwice(Vec<KeyBoundTwice>),
    /// Another channel's configuration, which the configured Leader is judged
    /// against, could not be judged; the text says whose, and why.
    Unjudged(String),
}

impl KeysRefused {
    /// The catalog code this maps to, which is what a log is queried by.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::NotPrivate(_) => ErrorCode::ChannelKeybindingsNotPrivate,
            Self::Unavailable(_) => ErrorCode::ChannelKeybindingsUnavailable,
            Self::Unreadable(_) => ErrorCode::ChannelKeybindingsUnreadable,
            Self::Leader { rule, .. } => match rule {
                LeaderRule::HoldsTheChordModifier => ErrorCode::ChannelLeaderHoldsTheChordModifier,
                LeaderRule::WithoutAModifier => ErrorCode::ChannelLeaderWithoutAModifier,
                LeaderRule::HeldWithTheChordIsAnothers => {
                    ErrorCode::ChannelLeaderHeldWithTheChordIsAnothers
                }
                LeaderRule::SharedWithAnother => ErrorCode::ChannelLeaderOfAnotherChannel,
            },
            Self::KeyBoundTwice(_) => ErrorCode::ChannelKeyBoundTwice,
            Self::Unjudged(_) => ErrorCode::ChannelKeybindingsUnjudged,
        }
    }

    /// Which part this leaves to its defaults.
    #[must_use]
    pub const fn part(&self) -> Part {
        match self {
            Self::NotPrivate(_) | Self::Unavailable(_) | Self::Unreadable(_) => Part::Whole,
            Self::Leader { .. } | Self::Unjudged(_) => Part::Leader,
            Self::KeyBoundTwice(_) => Part::Bindings,
        }
    }
}

impl fmt::Display for KeysRefused {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "[{}] ", self.code().code())?;
        match self {
            Self::NotPrivate(what) => write!(
                formatter,
                "{what}, so the keybinding configuration was not read: another account could \
                 have written it"
            )?,
            Self::Unavailable(what) => write!(
                formatter,
                "{what}, so the keybinding configuration was not read"
            )?,
            Self::Unreadable(what) => write!(
                formatter,
                "the keybinding configuration is not one this build reads: {what}"
            )?,
            Self::Leader {
                leader,
                chord,
                rule,
                by,
            } => {
                write!(
                    formatter,
                    "the configured Leader `{leader}`, with `{}` as its chord modifier, is \
                     refused: {rule}",
                    chord.word()
                )?;
                if let Some(other) = by {
                    write!(formatter, "; it meets {}'s", named(*other))?;
                }
            }
            Self::KeyBoundTwice(twice) => {
                formatter.write_str("the configured bindings are refused: ")?;
                for (at, one) in twice.iter().enumerate() {
                    if at > 0 {
                        formatter.write_str("; ")?;
                    }
                    write!(formatter, "{one}")?;
                }
            }
            Self::Unjudged(why) => write!(
                formatter,
                "the configured Leader is refused, because {why}, so it could not be judged \
                 against that channel's"
            )?,
        }
        formatter.write_str(match self.part() {
            Part::Whole => ", and the channel's defaults stand",
            Part::Leader => ", and the channel's default Leader stands",
            Part::Bindings => ", and the catalog's bindings stand",
        })
    }
}

impl std::error::Error for KeysRefused {}

/// What a choice of keys in force is warned of, and kept: [`Warning`] beside
/// the Leader part it was raised under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeysWarning {
    /// What the judge warned of.
    pub warning: Warning,
    /// The Leader and chord modifier in force when it was raised.
    pub part: LeaderPart,
}

impl KeysWarning {
    /// The catalog code this maps to.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self.warning {
            Warning::TheSystemUses { .. } => ErrorCode::ChannelKeyTheSystemUses,
            Warning::Unpressable { .. } => ErrorCode::ChannelKeyUnpressable,
        }
    }
}

impl fmt::Display for KeysWarning {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "[{}] ", self.code().code())?;
        match self.warning {
            Warning::TheSystemUses { binding, shortcut } => {
                write!(
                    formatter,
                    "{} uses {} ({})",
                    shortcut.system,
                    shortcut.chord.written_for(shortcut.keyboard),
                    shortcut.source
                )?;
                match binding {
                    None => formatter.write_str(", which is this channel's Leader")?,
                    Some(binding) => write!(
                        formatter,
                        ", which `{binding}` is pressed with as {}",
                        binding.written_for(self.part, shortcut.keyboard)
                    )?,
                }
                formatter.write_str("; it is kept, and the system may take those keys first")
            }
            Warning::Unpressable { binding } => write!(
                formatter,
                "`{binding}` is the Leader's own key, `{}`, held with the Leader, which no hand \
                 presses while holding it; it is kept",
                self.part.leader.key().written_for(Keyboard::Linux)
            ),
        }
    }
}

/// The keys a channel's interface is reached and driven by, as its
/// configuration and the catalog leave them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effective {
    /// The channel they are this install's.
    pub channel: Channel,
    /// The file read, [`Layout::keybindings`].
    pub file: PathBuf,
    /// The Leader in force.
    pub leader: Leader,
    /// The chord modifier in force.
    pub chord: Modifier,
    /// Every binding in force, beside the command it runs.
    pub tables: Tables,
    /// Whether the Leader part in force is the configured one.
    pub leader_configured: bool,
    /// Whether the bindings in force are the configured ones over the
    /// catalog's.
    pub bindings_configured: bool,
    /// Every refusal, in the order it was met: of the file, then of its Leader
    /// part, then of its bindings part.
    pub refused: Vec<KeysRefused>,
    /// Every warning the keys in force raise.
    pub warnings: Vec<KeysWarning>,
}

impl Effective {
    /// The Leader and chord modifier in force.
    #[must_use]
    pub const fn part(&self) -> LeaderPart {
        LeaderPart {
            leader: self.leader,
            chord: self.chord,
        }
    }
}

/// The keys in force for `found`'s install: its configuration read and
/// judged over the catalog this build carries.
///
/// **It never fails**: a file that is refused is reported in
/// [`Effective::refused`] with the defaults in its place, because a broken
/// choice of keys is no reason to refuse a start.
#[must_use]
pub fn effective(found: &Found) -> Effective {
    let read = read(&found.layout.config, &found.layout.keybindings, &found.user);
    let (own, refused) = match read {
        Ok(None) => (None, None),
        Ok(Some(text)) => match Configuration::parse(&text) {
            Ok(configuration) => (Some(configuration), None),
            Err(refused) => (None, Some(refused)),
        },
        Err(unread) => (None, Some(unread.refused())),
    };
    let mut effective = judged(found, own.as_ref(), &catalog_bindings());
    if let Some(refused) = refused {
        effective.refused.insert(0, refused);
    }
    effective
}

/// The keys `own` would put in force for `found`'s install over `catalog`.
///
/// Judged as [`effective`] judges the file: the call a writer makes before it
/// writes one. `own` is within [`BYTES_MAX`], as every [`Configuration`] is,
/// so what this accepts [`Configuration::to_canonical`] writes as a file
/// [`effective`] reads.
#[must_use]
pub fn judged(
    found: &Found,
    own: Option<&Configuration>,
    catalog: &[(String, Vec<Binding>)],
) -> Effective {
    let catalog: Vec<(&str, Vec<Binding>)> = catalog
        .iter()
        .map(|(command, bound)| (command.as_str(), bound.clone()))
        .collect();
    let channel = found.install.channel;
    let default = default_of(channel);
    let mut refused = Vec::new();
    let (part, leader_configured) = match own.and_then(|own| own.leader_part(default)) {
        None => (default, false),
        Some((leader, chord)) => match settled(found, leader, chord) {
            Ok(part) => (part, true),
            Err(refusal) => {
                refused.push(refusal);
                (default, false)
            }
        },
    };
    // The catalog's own tables: the compiler refuses a catalog binding a key
    // twice (`duplicate-keybinding`), by this same judge, so its defaults
    // always make tables.
    let defaults = || keys::tables(&catalog, &[]).unwrap_or_default();
    let (tables, bindings_configured) = match own.and_then(Configuration::bindings) {
        None => (defaults(), false),
        Some(chosen) => {
            let chosen: Vec<(&str, Vec<Binding>)> = chosen
                .iter()
                .map(|(command, bound)| (command.as_str(), bound.clone()))
                .collect();
            match keys::tables(&catalog, &chosen) {
                Ok(tables) => (tables, true),
                Err(twice) => {
                    refused.push(KeysRefused::KeyBoundTwice(twice));
                    (defaults(), false)
                }
            }
        }
    };
    let warnings = keys::warnings(part, &tables)
        .into_iter()
        .map(|warning| KeysWarning { warning, part })
        .collect();
    Effective {
        channel,
        file: found.layout.keybindings.clone(),
        leader: part.leader,
        chord: part.chord,
        tables,
        leader_configured,
        bindings_configured,
        refused,
        warnings,
    }
}

/// The bindings the catalog this build carries declares, each command's
/// read by the grammar.
///
/// Every one is a binding: the compiler reads each with [`Binding::parse`]
/// and refuses the catalog otherwise (`malformed-keybinding`), so none is
/// left out here.
#[must_use]
pub fn catalog_bindings() -> Vec<(String, Vec<Binding>)> {
    nc_catalog::COMMANDS
        .iter()
        .map(|command| {
            (
                command.name.to_owned(),
                command
                    .keybindings
                    .iter()
                    .filter_map(|text| Binding::parse(text).ok())
                    .collect(),
            )
        })
        .collect()
}

/// A channel's default Leader part: its Leader and its chord modifier.
const fn default_of(channel: Channel) -> LeaderPart {
    LeaderPart {
        leader: channel.leader(),
        chord: channel.chord_modifier(),
    }
}

/// How `channel` is named in what a refusal says.
fn named(channel: Channel) -> String {
    match channel {
        Channel::Production => "production".to_owned(),
        Channel::Staging => "staging".to_owned(),
        Channel::Development(checkout) => format!("the development checkout {checkout}"),
    }
}

/// The Leader part `leader` and `chord` make, once it keeps every rule
/// against the other channels of `found`'s install.
fn settled(found: &Found, leader: Chord, chord: Modifier) -> Result<LeaderPart, KeysRefused> {
    let refused = |rule: LeaderRule, by: Option<Channel>| KeysRefused::Leader {
        leader,
        chord,
        rule,
        by,
    };
    let part = LeaderPart {
        leader: Leader::of(leader).map_err(|rule| refused(rule, None))?,
        chord,
    };
    // **The rule that needs no other channel first**, so a part it refuses
    // is refused for that, whatever the other channels' files say.
    keys::intra(part).map_err(|rule| refused(rule, None))?;
    let own = found.install.channel;
    let mut claims = vec![Claim {
        rank: own,
        default: default_of(own),
        configured: Some(part),
    }];
    for (channel, layout) in siblings(&found.install, &found.user)? {
        if channel == own {
            continue;
        }
        // **Only the files before this channel are read**: what this one
        // settles to depends on what those settled to and on the defaults
        // after it, never on what a later channel configured.
        let configured = if channel < own {
            predecessor(
                channel,
                read(&layout.config, &layout.keybindings, &found.user),
            )?
        } else {
            None
        };
        claims.push(Claim {
            rank: channel,
            default: default_of(channel),
            configured,
        });
    }
    match keys::settle(&claims).first().map(|settled| settled.verdict) {
        Some(Verdict::Refused { rule, by }) => Err(refused(rule, by)),
        _ => Ok(part),
    }
}

/// Every channel installed beside `install`, under its bases and its
/// namespace, with the names each derives, in the order they are settled.
fn siblings(install: &Install, user: &User) -> Result<Vec<(Channel, Layout)>, KeysRefused> {
    let checkouts = checkouts_under(install.bases.state(), install.namespace.as_ref())?;
    let mut counted = Vec::new();
    for channel in [Channel::Production, Channel::Staging]
        .into_iter()
        .chain(checkouts.into_iter().map(Channel::Development))
    {
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
        let asked = std::fs::symlink_metadata(&names.manifest).map(drop);
        if installed(asked, channel, &names.manifest)? {
            counted.push((channel, names));
        }
    }
    Ok(counted)
}

/// Whether `channel` is installed, by what asking for its manifest answered.
///
/// An install whose manifest is gone binds nothing. **Any other answer
/// refuses the configured Leader**, naming where the manifest was asked for,
/// rather than guess whether that channel binds a Leader of its own.
fn installed(
    asked: std::io::Result<()>,
    channel: Channel,
    manifest: &Path,
) -> Result<bool, KeysRefused> {
    let Err(error) = asked else {
        return Ok(true);
    };
    match error.kind() {
        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory => Ok(false),
        _ => Err(KeysRefused::Unjudged(format!(
            "whether {} is installed could not be read at `{}`: {error}",
            named(channel),
            manifest.display()
        ))),
    }
}

/// Every checkout whose profile directory sits under the state base `base`
/// for `namespace`, by digest.
fn checkouts_under(
    base: &Path,
    namespace: Option<&Namespace>,
) -> Result<Vec<Checkout>, KeysRefused> {
    let unlisted = |error: &std::io::Error| {
        KeysRefused::Unjudged(format!(
            "the profiles under `{}` could not be listed: {error}",
            base.display()
        ))
    };
    let listing = std::fs::read_dir(base); // allow-nondeterminism: this `read_dir` is sorted by digest below, which is the order the checkouts are settled in
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
        if found.len() > CHANNELS_MAX {
            return Err(KeysRefused::Unjudged(format!(
                "more than {CHANNELS_MAX} development checkouts keep a profile under `{}`",
                base.display()
            )));
        }
    }
    found.sort();
    Ok(found)
}

/// The checkout whose profile directory under `namespace` is named `name`.
///
/// The inverse of the leaf [`layout`] writes for a development install, and
/// nothing for any other name.
fn checkout_named(name: &str, namespace: Option<&Namespace>) -> Option<Checkout> {
    let rest = name.strip_prefix("noctua-dev-")?;
    let digits = match namespace {
        None => rest,
        Some(namespace) => rest.strip_suffix(namespace.as_str())?.strip_suffix('-')?,
    };
    Checkout::parse(digits).ok()
}

/// The Leader part `channel`, settled before this one, configures: as its own
/// reader takes the file whose reading `answered`.
///
/// **What that reader would refuse is its defaults**, so it is `None` here:
/// no file, one that is not private, larger than its bound or not one this
/// format reads, and a Leader holding no modifier. **What this build cannot
/// judge refuses this channel's configured Leader instead**: a later format,
/// and a file the disk did not answer for.
fn predecessor(
    channel: Channel,
    answered: Result<Option<String>, Unread>,
) -> Result<Option<LeaderPart>, KeysRefused> {
    let text = match answered {
        Ok(Some(text)) => text,
        Ok(None) | Err(Unread::NotPrivate(_) | Unread::Unreadable(_)) => return Ok(None),
        Err(Unread::Unavailable(what)) => {
            return Err(KeysRefused::Unjudged(format!(
                "{}'s keybinding configuration was not read: {what}",
                named(channel)
            )));
        }
    };
    match parsed(&text) {
        Ok(configuration) => {
            Ok(configuration
                .leader_part(default_of(channel))
                .and_then(|(leader, chord)| {
                    Leader::of(leader)
                        .ok()
                        .map(|leader| LeaderPart { leader, chord })
                }))
        }
        Err(Unparsed::Later(format)) => Err(KeysRefused::Unjudged(format!(
            "{}'s keybinding configuration is of format {format}, and this build reads format \
             {FORMAT}",
            named(channel)
        ))),
        Err(Unparsed::Unreadable(_)) => Ok(None),
    }
}

/// Why a file was not read.
enum Unread {
    /// It, or its directory, is not this user's alone.
    NotPrivate(String),
    /// The filesystem did not answer.
    Unavailable(String),
    /// It is larger than its bound, or not text.
    Unreadable(String),
}

impl Unread {
    /// The refusal of a channel's own file this is.
    fn refused(self) -> KeysRefused {
        match self {
            Self::NotPrivate(what) => KeysRefused::NotPrivate(what),
            Self::Unavailable(what) => KeysRefused::Unavailable(what),
            Self::Unreadable(what) => KeysRefused::Unreadable(what),
        }
    }
}

/// The text of `file` in `directory`, once both are judged this user's alone,
/// and nothing when either is not there.
fn read(directory: &Path, file: &Path, user: &User) -> Result<Option<String>, Unread> {
    for (path, kind) in [(directory, Kind::Directory), (file, Kind::File)] {
        match nc_platform::judge(path, kind, user) {
            Ok(()) => {}
            Err(Judged::Failed(failed))
                if failed.error().kind() == std::io::ErrorKind::NotFound =>
            {
                return Ok(None);
            }
            Err(Judged::Refused(refused)) => return Err(Unread::NotPrivate(refused.to_string())),
            Err(Judged::Failed(failed)) => return Err(Unread::Unavailable(failed.to_string())),
        }
    }
    let mut bytes = Vec::new();
    std::fs::File::open(file)
        .and_then(|opened| opened.take(BYTES_MAX + 1).read_to_end(&mut bytes))
        .map_err(|error| {
            Unread::Unavailable(format!("`{}` could not be read: {error}", file.display()))
        })?;
    // Before the text is decoded: past the bound, the last byte read may cut a
    // character in two.
    bounded(bytes.len()).map_err(Unread::Unreadable)?;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| Unread::Unreadable("it is not UTF-8".to_owned()))
}

/// Whether `bytes` are within [`BYTES_MAX`], or what a refusal says of them.
///
/// The one bound of the file read and of a choice handed to
/// [`Configuration::parse`].
fn bounded(bytes: usize) -> Result<(), String> {
    if u64::try_from(bytes).map_or(true, |bytes| bytes > BYTES_MAX) {
        return Err(format!("it holds more than {BYTES_MAX} bytes"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;
    use std::path::Path;

    use nc_catalog::ErrorCode;
    use nc_schema::keys::{Chord, Leader, LeaderPart, Modifier};

    use super::{
        BYTES_MAX, Configuration, KeysRefused, Part, Unread, checkout_named, installed, predecessor,
    };
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
        ] {
            assert_eq!(checkout_named(&name, space), None, "{name}");
        }
    }

    /// A configuration reads back as the text it writes, and canonical text
    /// as itself.
    #[test]
    fn a_configuration_reads_back_as_the_text_it_writes() {
        for text in [
            r#"{"format":1}"#,
            r#"{"chord":"Control","format":1}"#,
            r#"{"format":1,"leader":"Control+Super+Space"}"#,
            r#"{"bindings":{"kernel.redo":[],"kernel.undo":["Leader KeyU","Leader+Chord+KeyZ"]},"chord":"Shift","format":1,"leader":"Alt+F12"}"#,
        ] {
            let read = Configuration::parse(text).expect("a configuration");
            assert_eq!(read.to_canonical(), text);
            assert_eq!(
                Configuration::parse(&read.to_canonical()).expect("read back"),
                read
            );
        }
        let spaced = "{ \"leader\" : \"Alt+Space\",\n  \"format\" : 1 }";
        assert_eq!(
            Configuration::parse(spaced)
                .expect("whitespace is JSON's")
                .to_canonical(),
            r#"{"format":1,"leader":"Alt+Space"}"#
        );
    }

    /// A later format is told from every other refusal, so another channel's
    /// reader knows it cannot judge it.
    #[test]
    fn a_later_format_is_told_from_a_file_that_is_wrong() {
        assert!(matches!(
            super::parsed(r#"{"format":2,"anything":"a later build reads"}"#),
            Err(super::Unparsed::Later(2))
        ));
        for wrong in [
            r#"{"format":0}"#,
            r#"{"format":-1}"#,
            r#"{"format":"1"}"#,
            r#"{"leader":"Alt+Space"}"#,
            r#"{"format":1,"anything":true}"#,
        ] {
            assert!(
                matches!(super::parsed(wrong), Err(super::Unparsed::Unreadable(_))),
                "{wrong}"
            );
        }
        let refused = Configuration::parse(r#"{"format":2}"#).expect_err("a later format");
        assert!(matches!(&refused, KeysRefused::Unreadable(what) if what.contains("format 1")));
    }

    /// A configuration is read up to the file's bound and no further.
    ///
    /// Text at [`BYTES_MAX`] is read, and writes back shorter than itself,
    /// its escapes and spaces dropped; one byte more is refused, whatever it
    /// holds.
    #[test]
    fn a_configuration_is_read_up_to_the_files_bound_and_no_further() {
        let bound = usize::try_from(BYTES_MAX).expect("a bound that fits");
        let entries: Vec<String> = (0..600)
            .map(|at| format!(r#""plugin.command-number-{at:04}":[]"#))
            .collect();
        let past = format!(r#"{{"bindings":{{{}}},"format":1}}"#, entries.join(","));
        assert!(past.len() > bound);
        assert_eq!(
            Configuration::parse(&past),
            Err(KeysRefused::Unreadable(format!(
                "it holds more than {BYTES_MAX} bytes"
            )))
        );
        let held = entries.len() / 2;
        let opened = format!(
            r#"{{"bindings":{{"kernel\u002eundo":["Leader\u002bKeyZ"],{}}},"format":1"#,
            entries[..held].join(",")
        );
        let at_the_bound = format!("{opened}{}}}", " ".repeat(bound - opened.len() - 1));
        assert_eq!(at_the_bound.len(), bound);
        let read = Configuration::parse(&at_the_bound).expect("a configuration at its bound");
        assert_eq!(read.bindings().map(<[_]>::len), Some(held + 1));
        assert!(read.to_canonical().len() < at_the_bound.len());
        assert!(
            read.to_canonical()
                .contains(r#""kernel.undo":["Leader+KeyZ"]"#)
        );
        assert_eq!(
            Configuration::parse(&format!("{at_the_bound} ")),
            Err(KeysRefused::Unreadable(format!(
                "it holds more than {BYTES_MAX} bytes"
            )))
        );
    }

    /// A file the disk did not answer for is a refusal of its own code.
    ///
    /// Each answer reading the channel's own file gives is refused with the
    /// code a log is queried by, saying what was answered, and it leaves
    /// both parts to their defaults.
    #[test]
    fn a_file_the_disk_did_not_answer_for_is_refused_with_its_own_code() {
        let what = "`/scratch/keybindings.json` could not be read: the disk did not answer";
        for (unread, refused, code) in [
            (
                Unread::Unavailable(what.to_owned()),
                KeysRefused::Unavailable(what.to_owned()),
                ErrorCode::ChannelKeybindingsUnavailable,
            ),
            (
                Unread::NotPrivate(what.to_owned()),
                KeysRefused::NotPrivate(what.to_owned()),
                ErrorCode::ChannelKeybindingsNotPrivate,
            ),
            (
                Unread::Unreadable(what.to_owned()),
                KeysRefused::Unreadable(what.to_owned()),
                ErrorCode::ChannelKeybindingsUnreadable,
            ),
        ] {
            let said = unread.refused();
            assert_eq!(said, refused);
            assert_eq!(said.code(), code);
            assert_eq!(said.part(), Part::Whole);
            assert!(
                said.to_string().starts_with(&format!("[{}] ", code.code())),
                "{said}"
            );
            assert!(said.to_string().contains(what), "{said}");
        }
    }

    /// A channel is installed while its manifest is there and not while
    /// nothing is.
    ///
    /// Any other answer from the disk refuses the configured Leader, naming
    /// the channel and where its manifest was asked for.
    #[test]
    fn a_manifest_the_disk_did_not_answer_for_refuses_the_configured_leader() {
        let manifest = Path::new("/scratch/s/noctua-dev-0123abcd-k7/bin/manifest");
        let checkout = Channel::Development(Checkout::parse("0123abcd").expect("eight digits"));
        assert_eq!(installed(Ok(()), checkout, manifest), Ok(true));
        for gone in [ErrorKind::NotFound, ErrorKind::NotADirectory] {
            assert_eq!(installed(Err(gone.into()), checkout, manifest), Ok(false));
        }
        for unanswered in [
            ErrorKind::PermissionDenied.into(),
            ErrorKind::InvalidFilename.into(),
            std::io::Error::other("the disk did not answer"),
        ] {
            let refused =
                installed(Err(unanswered), checkout, manifest).expect_err("not known either way");
            assert_eq!(refused.code(), ErrorCode::ChannelKeybindingsUnjudged);
            assert_eq!(refused.part(), Part::Leader);
            let said = refused.to_string();
            assert!(
                said.contains("whether the development checkout 0123abcd is installed")
                    && said.contains(&manifest.display().to_string()),
                "{said}"
            );
        }
    }

    /// A predecessor's file the disk did not answer for refuses this
    /// channel's configured Leader, naming whose file it is.
    ///
    /// What that channel's own reader refuses — no file, one another account
    /// can reach, one not read as a configuration — is its defaults instead,
    /// and a file it reads is the Leader part it configures.
    #[test]
    fn a_predecessors_file_the_disk_did_not_answer_for_refuses_the_configured_leader() {
        let what =
            "`/scratch/c/noctua-k7/keybindings.json` could not be read: the disk did not answer";
        let refused = predecessor(
            Channel::Production,
            Err(Unread::Unavailable(what.to_owned())),
        )
        .expect_err("not judged");
        assert_eq!(refused.code(), ErrorCode::ChannelKeybindingsUnjudged);
        assert_eq!(refused.part(), Part::Leader);
        assert!(
            refused.to_string().contains(&format!(
                "production's keybinding configuration was not read: {what}"
            )),
            "{refused}"
        );
        for defaults in [
            Ok(None),
            Err(Unread::NotPrivate(what.to_owned())),
            Err(Unread::Unreadable(what.to_owned())),
            Ok(Some(r#"{"format":1,"anything":true}"#.to_owned())),
            Ok(Some(r#"{"format":1,"leader":"Space"}"#.to_owned())),
        ] {
            assert_eq!(predecessor(Channel::Production, defaults), Ok(None));
        }
        let chord = Chord::parse("Control+Super+Space").expect("a chord");
        assert_eq!(
            predecessor(
                Channel::Production,
                Ok(Some(
                    r#"{"format":1,"leader":"Control+Super+Space"}"#.to_owned()
                ))
            ),
            Ok(Some(LeaderPart {
                leader: Leader::of(chord).expect("a Leader"),
                chord: Modifier::Shift,
            }))
        );
    }
}
