//! Every message of the protocol, as the one canonical object each is, and
//! read back only as this code writes it.
//!
//! **A message is an object whose `type` says what it is, holding exactly the
//! fields that type holds**: a field the protocol does not read, one missing or
//! of another type, and a type not read where it arrived, are each refused
//! rather than ignored, so two builds that disagree about a message disagree
//! loudly. The handshake's own messages name the protocol first, and a version
//! other than [`PROTOCOL`] is refused as that, before anything else of the
//! message is read.

use std::collections::BTreeMap;

use nc_canon::Value;
use nc_catalog::{ErrorCode, Locale};
use nc_channel::{Channel, Checkout};

use crate::{IpcError, PROTOCOL};

/// A malformed message, said as `what`.
fn malformed(what: impl Into<String>) -> IpcError {
    IpcError::Malformed(what.into())
}

/// A message's fields, taken one at a time: what is left once its type has
/// taken what it reads is a field the protocol does not read.
struct Fields {
    /// The message's type.
    kind: String,
    /// The fields not taken yet.
    left: BTreeMap<String, Value>,
}

impl Fields {
    /// The fields of `value`, which must be an object with a textual type.
    fn of(value: Value) -> Result<Self, IpcError> {
        let mut left = value
            .into_object()
            .ok_or_else(|| malformed("a message that is not one object"))?;
        let kind = left
            .remove("type")
            .and_then(Value::into_text)
            .ok_or_else(|| malformed("a message with no type"))?;
        Ok(Self { kind, left })
    }

    /// The field `key`, which must be there.
    fn value(&mut self, key: &str) -> Result<Value, IpcError> {
        self.left
            .remove(key)
            .ok_or_else(|| malformed(format!("a `{}` message with no `{key}`", self.kind)))
    }

    /// The text field `key`.
    fn text(&mut self, key: &str) -> Result<String, IpcError> {
        let kind = self.kind.clone();
        self.value(key)?
            .into_text()
            .ok_or_else(|| malformed(format!("a `{kind}` message whose `{key}` is not text")))
    }

    /// The text field `key`, or nothing where it is absent.
    fn text_if_there(&mut self, key: &str) -> Result<Option<String>, IpcError> {
        if self.left.contains_key(key) {
            self.text(key).map(Some)
        } else {
            Ok(None)
        }
    }

    /// The whole-number field `key`.
    fn integer(&mut self, key: &str) -> Result<i64, IpcError> {
        match self.value(key)? {
            Value::Integer(number) => Ok(number),
            _ => Err(malformed(format!(
                "a `{}` message whose `{key}` is not a whole number",
                self.kind
            ))),
        }
    }

    /// The field `key`, a count: a whole number that is not below zero.
    fn count(&mut self, key: &str) -> Result<u64, IpcError> {
        let kind = self.kind.clone();
        u64::try_from(self.integer(key)?)
            .map_err(|_| malformed(format!("a `{kind}` message whose `{key}` is below zero")))
    }

    /// The count `key`, or nothing where it is absent.
    fn count_if_there(&mut self, key: &str) -> Result<Option<u64>, IpcError> {
        if self.left.contains_key(key) {
            self.count(key).map(Some)
        } else {
            Ok(None)
        }
    }

    /// The object field `key`, or nothing where it is absent.
    fn object_if_there(&mut self, key: &str) -> Result<Option<Value>, IpcError> {
        if self.left.contains_key(key) {
            self.object(key).map(Some)
        } else {
            Ok(None)
        }
    }

    /// The object field `key`.
    fn object(&mut self, key: &str) -> Result<Value, IpcError> {
        match self.value(key)? {
            value @ Value::Object(_) => Ok(value),
            _ => Err(malformed(format!(
                "a `{}` message whose `{key}` is not an object",
                self.kind
            ))),
        }
    }

    /// The field `key`, 32 bytes as 64 lower-case hexadecimal digits.
    fn bytes32(&mut self, key: &str) -> Result<[u8; 32], IpcError> {
        let kind = self.kind.clone();
        hex_of(&self.text(key)?)
            .ok_or_else(|| malformed(format!("a `{kind}` message whose `{key}` is not 32 bytes")))
    }

    /// The protocol the message names, held to this build's before anything
    /// else of it is read.
    fn protocol(&mut self) -> Result<(), IpcError> {
        let theirs = self.integer("protocol")?;
        if theirs == i64::from(PROTOCOL) {
            Ok(())
        } else {
            Err(IpcError::Protocol { theirs })
        }
    }

    /// Nothing is left that the type did not read.
    fn done(self) -> Result<(), IpcError> {
        match self.left.keys().next() {
            None => Ok(()),
            Some(key) => Err(malformed(format!(
                "a `{}` message with a field the protocol does not read, `{key}`",
                self.kind
            ))),
        }
    }
}

/// `N` bytes written as lower-case hexadecimal digits, or nothing.
fn hex_of<const N: usize>(text: &str) -> Option<[u8; N]> {
    let bytes = nc_canon::from_hex(text).ok()?;
    <[u8; N]>::try_from(bytes.as_slice()).ok()
}

/// An object of `kind` holding `fields`.
fn message(kind: &str, fields: Vec<(&str, Value)>) -> Value {
    let mut pairs = vec![("type", Value::text(kind))];
    pairs.extend(fields);
    Value::object(pairs)
}

/// The fields of a message where `expected` is the type the protocol reads:
/// a goodbye there is the other end turning this one away, and anything
/// else is refused.
fn expecting(value: Value, expected: &str) -> Result<Fields, IpcError> {
    let fields = Fields::of(value)?;
    if fields.kind == expected {
        return Ok(fields);
    }
    if fields.kind == "goodbye" {
        return Err(IpcError::TurnedAway(goodbye_of(fields)?));
    }
    Err(malformed(format!(
        "a `{}` message where a `{expected}` is read",
        fields.kind
    )))
}

/// The code a goodbye's fields carry.
fn goodbye_of(mut fields: Fields) -> Result<ErrorCode, IpcError> {
    let code = fields.text("code")?;
    fields.done()?;
    ErrorCode::from_code(&code).ok_or_else(|| {
        malformed(format!(
            "a goodbye with a code this build does not know, `{code}`"
        ))
    })
}

/// What the daemon says first: the protocol it speaks, and its nonce.
pub(crate) fn challenge(nonce: &[u8; 32]) -> Value {
    message(
        "challenge",
        vec![
            ("protocol", Value::Integer(i64::from(PROTOCOL))),
            ("nonce", Value::text(nc_canon::to_hex(nonce))),
        ],
    )
}

/// The daemon's nonce, from its challenge.
pub(crate) fn read_challenge(value: Value) -> Result<[u8; 32], IpcError> {
    let mut fields = expecting(value, "challenge")?;
    fields.protocol()?;
    let nonce = fields.bytes32("nonce")?;
    fields.done()?;
    Ok(nonce)
}

/// What the client answers: the protocol it speaks, its nonce and its proof.
pub(crate) fn hello(nonce: &[u8; 32], proof: &blake3::Hash) -> Value {
    message(
        "hello",
        vec![
            ("protocol", Value::Integer(i64::from(PROTOCOL))),
            ("nonce", Value::text(nc_canon::to_hex(nonce))),
            ("proof", Value::text(proof.to_hex().as_str())),
        ],
    )
}

/// The client's nonce and proof, from its hello.
pub(crate) fn read_hello(value: Value) -> Result<([u8; 32], blake3::Hash), IpcError> {
    let mut fields = expecting(value, "hello")?;
    fields.protocol()?;
    let nonce = fields.bytes32("nonce")?;
    let proof = blake3::Hash::from_bytes(fields.bytes32("proof")?);
    fields.done()?;
    Ok((nonce, proof))
}

/// What the daemon answers a proven client with: its own proof, and what it
/// is.
pub(crate) fn welcomed(proof: &blake3::Hash, welcome: &Welcome) -> Value {
    let mut fields = vec![
        ("proof", Value::text(proof.to_hex().as_str())),
        ("pid", Value::Integer(i64::from(welcome.pid))),
        ("instance", Value::text(nc_canon::to_hex(&welcome.instance))),
        ("channel", Value::text(welcome.channel.word())),
        ("state", Value::text(welcome.state.word())),
    ];
    if let Channel::Development(checkout) = welcome.channel {
        fields.push(("checkout", Value::text(checkout.to_string())));
    }
    message("welcome", fields)
}

/// The daemon's proof and what it says it is, from its welcome.
pub(crate) fn read_welcomed(value: Value) -> Result<(blake3::Hash, Welcome), IpcError> {
    let mut fields = expecting(value, "welcome")?;
    let proof = blake3::Hash::from_bytes(fields.bytes32("proof")?);
    let pid = u32::try_from(fields.integer("pid")?)
        .ok()
        .filter(|pid| *pid > 0)
        .ok_or_else(|| malformed("a welcome whose `pid` is no process"))?;
    let instance = hex_of(&fields.text("instance")?)
        .ok_or_else(|| malformed("a welcome whose `instance` is not 16 bytes"))?;
    let word = fields.text("channel")?;
    let checkout = fields.text_if_there("checkout")?;
    let channel = match (word.as_str(), checkout) {
        ("production", None) => Channel::Production,
        ("staging", None) => Channel::Staging,
        ("development", Some(checkout)) => {
            Channel::Development(Checkout::parse(&checkout).map_err(|error| {
                malformed(format!("a welcome whose checkout is not one: {error}"))
            })?)
        }
        _ => {
            return Err(malformed(format!(
                "a welcome naming the channel `{word}` with a checkout where it has none, or \
                 none where it has one"
            )));
        }
    };
    let state = State::of(&fields.text("state")?)
        .ok_or_else(|| malformed("a welcome naming a state this build does not know"))?;
    fields.done()?;
    Ok((
        proof,
        Welcome {
            pid,
            instance,
            channel,
            state,
        },
    ))
}

/// What a side says before it closes a connection for a reason it can name.
pub(crate) fn goodbye(code: ErrorCode) -> Value {
    message("goodbye", vec![("code", Value::text(code.code()))])
}

/// What a side says when it has said nothing for a heartbeat.
pub(crate) fn ping() -> Value {
    message("ping", Vec::new())
}

/// A frame that arrived once the handshake was over, as the connection reads
/// it: a ping, a goodbye, or a message for its caller.
pub(crate) enum Heard {
    /// A ping, which says the peer is there and nothing else.
    Ping,
    /// The peer is closing the connection, saying why.
    Goodbye(ErrorCode),
    /// A request or an answer, for the caller to read.
    Message(Value),
}

/// What `value` is, once the handshake is over.
pub(crate) fn heard(value: Value) -> Result<Heard, IpcError> {
    let kind = match &value {
        Value::Object(fields) => fields.get("type").and_then(|kind| match kind {
            Value::Text(text) => Some(text.as_str()),
            _ => None,
        }),
        _ => None,
    };
    match kind {
        Some("ping") => Fields::of(value)?.done().map(|()| Heard::Ping),
        Some("goodbye") => goodbye_of(Fields::of(value)?).map(Heard::Goodbye),
        _ => Ok(Heard::Message(value)),
    }
}

/// What the daemon is, as it says so at the end of a handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Welcome {
    /// Its process, which a client holds to the one the system says is at the
    /// other end and to the PID record.
    pub pid: u32,
    /// Its instance, drawn at each start, so a client tells one run from the
    /// next.
    pub instance: [u8; 16],
    /// The channel it serves.
    pub channel: Channel,
    /// Where its start is.
    pub state: State,
}

/// Where a daemon's start is, as its welcome and its status say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Reading its profile, before it has a kernel.
    Starting,
    /// Behind its mark, waiting for the clock to pass it (`NOCTUA.md §16.2`).
    WaitingForClock,
    /// Opening its kernel, which recovers what a stopped process left.
    Recovering,
    /// Serving its kernel.
    Running,
    /// Refusing new dispatches, and stopping.
    Stopping,
}

impl State {
    /// Every state, in the order a start reaches them.
    pub const ALL: [Self; 5] = [
        Self::Starting,
        Self::WaitingForClock,
        Self::Recovering,
        Self::Running,
        Self::Stopping,
    ];

    /// The word the protocol writes it as.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::WaitingForClock => "waiting-for-clock",
            Self::Recovering => "recovering",
            Self::Running => "running",
            Self::Stopping => "stopping",
        }
    }

    /// The state `word` names, or nothing.
    fn of(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.word() == word)
    }
}

/// A count as the wire writes one: a whole number, the largest JSON's
/// integer holds where the count is past it.
///
/// **So a count past `i64::MAX` does not read back as itself**, and each
/// count the protocol holds says why that loses nothing: a watch's wait is
/// capped by the daemon at `ipc_watch_wait_ms` whatever it says, and a
/// revision is the daemon's own count, which turns to zero after `i64::MAX`
/// and so never passes it (`Watched` in `apps/daemon/src/server.rs`, held by
/// `a_revision_turns_to_zero_after_the_wires_largest_integer` there). A
/// client names as seen only a revision a daemon answered.
fn counted(count: u64) -> Value {
    Value::Integer(i64::try_from(count).unwrap_or(i64::MAX))
}

/// What an interface says at the rate it draws, as one object: where a page
/// is scrolled to, what a prompt holds, what a pointer is over.
///
/// **Bounded where it is made and where it is read, by one rule**
/// ([`Note::new`]): the object's canonical text is at most
/// `ipc_note_max_bytes`, so no client sends one the daemon would refuse and
/// no daemon reads one a client could not have made. What the object holds is
/// its reader's to judge — the protocol reads it as a note and as nothing
/// else: a note is journaled nowhere, dispatches nothing and confirms
/// nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note(Value);

impl Note {
    /// The note `detail` is.
    ///
    /// # Errors
    ///
    /// [`IpcError::Malformed`] for a detail that is not one object, or whose
    /// canonical text is longer than `ipc_note_max_bytes`.
    pub fn new(detail: Value) -> Result<Self, IpcError> {
        if !matches!(detail, Value::Object(_)) {
            return Err(malformed("a note whose `detail` is not an object"));
        }
        let length = detail.to_canonical().len();
        let most = nc_catalog::DAEMON.ipc_note_max_bytes;
        if u64::try_from(length).unwrap_or(u64::MAX) > most {
            return Err(malformed(format!(
                "a note of {length} bytes, and `ipc_note_max_bytes` is {most}"
            )));
        }
        Ok(Self(detail))
    }

    /// What the note says: one object.
    #[must_use]
    pub const fn detail(&self) -> &Value {
        &self.0
    }
}

/// The graphical environment a client announces: where a window opened for
/// its person would be shown.
///
/// **A closed list of names, each value bounded and plain**
/// ([`Graphical::new`]), made and read by one rule, so the daemon holds no
/// environment a client of this build could not have announced, and no other
/// variable of a client's environment reaches it by this message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Graphical(BTreeMap<&'static str, String>);

impl Graphical {
    /// Every variable an announcement may name: the X display and its
    /// authority file, the Wayland display, and the runtime directory a
    /// Wayland display's name is resolved in.
    pub const NAMES: [&'static str; 4] = [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XAUTHORITY",
        "XDG_RUNTIME_DIR",
    ];

    /// The variables that name a display: an announcement holds one at least.
    pub const DISPLAYS: [&'static str; 2] = ["DISPLAY", "WAYLAND_DISPLAY"];

    /// The variables that hold a path, which is absolute.
    pub const PATHS: [&'static str; 2] = ["XAUTHORITY", "XDG_RUNTIME_DIR"];

    /// The environment `pairs` announce.
    ///
    /// **In order, the first refusal the answer**, for each pair: its name
    /// is one of [`Graphical::NAMES`], written once; its value is not empty,
    /// is at most `ipc_display_value_max_bytes` long, and holds no control
    /// character — a NUL among them, which no environment can hold; and the
    /// value of a name in [`Graphical::PATHS`] begins with `/`. Then the
    /// whole names a display: one of [`Graphical::DISPLAYS`] at least.
    ///
    /// # Errors
    ///
    /// [`IpcError::Malformed`], saying which rule and which name.
    pub fn new<Name, Text>(pairs: impl IntoIterator<Item = (Name, Text)>) -> Result<Self, IpcError>
    where
        Name: AsRef<str>,
        Text: Into<String>,
    {
        let most = nc_catalog::DAEMON.ipc_display_value_max_bytes;
        let mut held = BTreeMap::new();
        for (name, value) in pairs {
            let written = name.as_ref();
            let Some(name) = Self::NAMES.into_iter().find(|name| *name == written) else {
                return Err(malformed(format!(
                    "an environment naming `{}`, which is none of {}",
                    written.escape_debug(),
                    Self::NAMES.join(", ")
                )));
            };
            let value: String = value.into();
            if value.is_empty() {
                return Err(malformed(format!("an environment whose `{name}` is empty")));
            }
            if u64::try_from(value.len()).unwrap_or(u64::MAX) > most {
                return Err(malformed(format!(
                    "an environment whose `{name}` is {} bytes, and \
                     `ipc_display_value_max_bytes` is {most}",
                    value.len()
                )));
            }
            if value.chars().any(char::is_control) {
                return Err(malformed(format!(
                    "an environment whose `{name}` holds a control character"
                )));
            }
            if Self::PATHS.contains(&name) && !value.starts_with('/') {
                return Err(malformed(format!(
                    "an environment whose `{name}` is not an absolute path"
                )));
            }
            if held.insert(name, value).is_some() {
                return Err(malformed(format!("an environment naming `{name}` twice")));
            }
        }
        if !Self::DISPLAYS.iter().any(|name| held.contains_key(name)) {
            return Err(malformed(format!(
                "an environment naming no display, neither {}",
                Self::DISPLAYS.join(" nor ")
            )));
        }
        Ok(Self(held))
    }

    /// The value announced for `name`, or nothing.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str)
    }

    /// Every variable announced with its value, in the order of their names.
    pub fn pairs(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().map(|(name, value)| (*name, value.as_str()))
    }

    /// The object the wire writes it as: each name with its value.
    fn as_object(&self) -> Value {
        Value::object(self.pairs().map(|(name, value)| (name, Value::text(value))))
    }

    /// The environment the object `value` announces.
    fn of_object(value: Value) -> Result<Self, IpcError> {
        let fields = value
            .into_object()
            .ok_or_else(|| malformed("a `display` message whose `environment` is not an object"))?;
        let mut pairs = Vec::new();
        for (name, value) in fields {
            let Some(value) = value.into_text() else {
                return Err(malformed(format!(
                    "an environment whose `{}` is not text",
                    name.escape_debug()
                )));
            };
            pairs.push((name, value));
        }
        Self::new(pairs)
    }
}

/// What a client asks the daemon, one at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Run a command with these arguments, as the connection's principal: the
    /// profile's person, or the agent of the session the connection opened.
    Dispatch {
        /// The command's registered name.
        command: String,
        /// Its arguments: one object.
        args: Value,
    },
    /// Every command the registry holds, each one's texts in `locale`.
    ///
    /// **One locale, named by the client**: the texts of every command in
    /// every locale are twice the answer for a reader who shows one, and the
    /// frame is what bounds it. The wire writes the locale's BCP 47 tag, and
    /// a tag this build ships no locale for is a malformed request.
    Registry {
        /// The locale the texts are resolved in.
        locale: Locale,
    },
    /// The commands the connection's principal may dispatch now, each one's
    /// texts in `locale`, as one registry entry each: a session's tools.
    Tools {
        /// The locale the texts are resolved in, named as a registry
        /// request names it.
        locale: Locale,
    },
    /// Act, from its answer on, as the agent `agent` names: one the profile's
    /// person made.
    ///
    /// **Read strictly**: the identifier is the text `to_crockford` writes
    /// for it and no other — not in lower case, not with a digit Crockford
    /// reads as another, not with a leading zero — so one agent has one
    /// spelling on the wire, and the message holds that field alone.
    Session {
        /// The agent, as the profile's minter gave it.
        agent: nc_ids::Snowflake,
    },
    /// What the daemon is and where its start is.
    Status,
    /// Stop.
    Stop,
    /// Answer once the state the daemon lets a client watch is at another
    /// revision than `seen`, or once the wait is over.
    ///
    /// **The long-poll, and still one request with one answer**: the daemon
    /// sends nothing nobody asked for, so a client that also asks other
    /// things watches on a second connection. A watch that names no revision
    /// has seen nothing and is answered at once.
    Watch {
        /// The revision of the state the client holds, or nothing where it
        /// holds none.
        seen: Option<u64>,
        /// How long the client will wait, in milliseconds; the daemon waits
        /// `ipc_watch_wait_ms` at most, whatever this says.
        wait_ms: u64,
    },
    /// What an interface says at the rate it draws, which is journaled
    /// nowhere.
    Note(Note),
    /// The graphical environment a window for the person would be shown in.
    Display(Graphical),
    /// The connection is the desktop interface the daemon started: `proof`
    /// is the secret it was handed, proven for this connection
    /// ([`crate::Secret::proof`]).
    AttachDesktop {
        /// The proof, 32 bytes as 64 lower-case hexadecimal digits.
        proof: [u8; 32],
    },
    /// The person confirmed the call the kernel holds as `pending`.
    ///
    /// **A serial, never a command or its arguments**: what is confirmed is
    /// the call the kernel holds, which no request names otherwise.
    Confirm {
        /// The serial the kernel gave the held call.
        pending: u64,
    },
    /// The person declined the call the kernel holds as `pending`.
    Decline {
        /// The serial the kernel gave the held call.
        pending: u64,
    },
}

/// The locale a request's `locale` field names, by its exact tag.
///
/// **One reading for every request that names one**, so the registry and the
/// tools are asked in the same spelling, and a tag this build ships no locale
/// for — `pt-br` among them — is malformed.
fn locale_of(fields: &mut Fields) -> Result<Locale, IpcError> {
    let tag = fields.text("locale")?;
    Locale::ALL
        .iter()
        .copied()
        .find(|locale| locale.tag() == tag)
        .ok_or_else(|| {
            malformed(format!(
                "a `{}` message naming `{}`, a locale this build does not ship",
                fields.kind,
                tag.escape_debug()
            ))
        })
}

/// The agent a session request names, read back only as it is written.
fn agent_of(fields: &mut Fields) -> Result<nc_ids::Snowflake, IpcError> {
    let text = fields.text("agent")?;
    nc_ids::Snowflake::from_crockford(&text)
        .ok()
        .filter(|agent| agent.to_crockford() == text)
        .ok_or_else(|| {
            malformed(format!(
                "a `session` message naming `{}`, which is not an identifier as one is written",
                text.escape_debug()
            ))
        })
}

impl Request {
    /// The message this request is.
    pub(crate) fn to_value(&self) -> Value {
        match self {
            Self::Dispatch { command, args } => message(
                "dispatch",
                vec![
                    ("command", Value::text(command.as_str())),
                    ("args", args.clone()),
                ],
            ),
            Self::Registry { locale } => {
                message("registry", vec![("locale", Value::text(locale.tag()))])
            }
            Self::Tools { locale } => message("tools", vec![("locale", Value::text(locale.tag()))]),
            Self::Session { agent } => message(
                "session",
                vec![("agent", Value::text(agent.to_crockford()))],
            ),
            Self::Status => message("status", Vec::new()),
            Self::Stop => message("stop", Vec::new()),
            Self::Watch { seen, wait_ms } => {
                let mut fields = vec![("wait_ms", counted(*wait_ms))];
                if let Some(seen) = seen {
                    fields.push(("seen", counted(*seen)));
                }
                message("watch", fields)
            }
            Self::Note(note) => message("note", vec![("detail", note.detail().clone())]),
            Self::Display(environment) => {
                message("display", vec![("environment", environment.as_object())])
            }
            Self::AttachDesktop { proof } => message(
                "attach",
                vec![("proof", Value::text(nc_canon::to_hex(proof)))],
            ),
            Self::Confirm { pending } => message("confirm", vec![("pending", counted(*pending))]),
            Self::Decline { pending } => message("decline", vec![("pending", counted(*pending))]),
        }
    }

    /// The request `value` is.
    pub(crate) fn from_value(value: Value) -> Result<Self, IpcError> {
        let mut fields = Fields::of(value)?;
        let request = match fields.kind.as_str() {
            "dispatch" => Self::Dispatch {
                command: fields.text("command")?,
                args: fields.object("args")?,
            },
            "registry" => Self::Registry {
                locale: locale_of(&mut fields)?,
            },
            "tools" => Self::Tools {
                locale: locale_of(&mut fields)?,
            },
            "session" => Self::Session {
                agent: agent_of(&mut fields)?,
            },
            "status" => Self::Status,
            "stop" => Self::Stop,
            "watch" => Self::Watch {
                seen: fields.count_if_there("seen")?,
                wait_ms: fields.count("wait_ms")?,
            },
            "note" => Self::Note(Note::new(fields.value("detail")?)?),
            "display" => Self::Display(Graphical::of_object(fields.value("environment")?)?),
            "attach" => Self::AttachDesktop {
                proof: fields.bytes32("proof")?,
            },
            "confirm" => Self::Confirm {
                pending: fields.count("pending")?,
            },
            "decline" => Self::Decline {
                pending: fields.count("pending")?,
            },
            other => {
                return Err(malformed(format!(
                    "a `{other}` message where a request is read"
                )));
            }
        };
        fields.done()?;
        Ok(request)
    }
}

/// How a dispatch was refused, by the kind every surface tells it by.
///
/// **The kernel's words**, one for one: `nc_kernel::RefusalKind::word` writes
/// them, and the daemon that hands a refusal on holds the two lists to each
/// other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusalKind {
    /// Not allowed.
    Denied,
    /// Not there.
    NotFound,
    /// Not what was expected.
    Invalid,
    /// Written against something that changed.
    Conflict,
    /// Not reachable right now.
    Unavailable,
    /// The daemon's own failure, or a plugin's.
    Internal,
}

impl RefusalKind {
    /// Every kind.
    pub const ALL: [Self; 6] = [
        Self::Denied,
        Self::NotFound,
        Self::Invalid,
        Self::Conflict,
        Self::Unavailable,
        Self::Internal,
    ];

    /// The word the protocol writes it as.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Denied => "denied",
            Self::NotFound => "not-found",
            Self::Invalid => "invalid",
            Self::Conflict => "conflict",
            Self::Unavailable => "unavailable",
            Self::Internal => "internal",
        }
    }

    /// The kind `word` names, or nothing.
    #[must_use]
    pub fn of(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.word() == word)
    }
}

/// A dispatch refused: its code, its kind, and the sentence that says why,
/// in English, which a surface shows beside the code's own message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// What a log is queried by.
    pub code: ErrorCode,
    /// The kind a surface tells it by.
    pub kind: RefusalKind,
    /// Why, in English.
    pub sentence: String,
}

/// What the daemon answers a request with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// What a dispatch answered.
    Result(Value),
    /// A dispatch refused.
    Refused(Refusal),
    /// The commands the registry holds, one object each.
    Registry(Vec<Value>),
    /// The commands the connection's principal may dispatch now, one object
    /// each, as the registry's are.
    Tools(Vec<Value>),
    /// The session asked for opened: from now on, the connection is its
    /// agent's.
    Session,
    /// The daemon's status, as one object.
    Status(Value),
    /// The daemon is stopping.
    Stopping,
    /// What a watch is answered: the revision the watched state is at, and
    /// the state itself where that is not the revision the watch had seen.
    ///
    /// **No state is the wait being over with nothing changed**: the revision
    /// is then the one the watch named.
    Changed {
        /// The revision the state is at.
        revision: u64,
        /// The state at that revision, as one object, or nothing where the
        /// client holds it already.
        state: Option<Value>,
    },
    /// The note was taken.
    Noted,
    /// The environment announced is the one the daemon holds.
    Displayed,
    /// The connection is attached as the desktop interface the daemon
    /// started.
    Attached,
    /// The held call was answered as asked.
    Settled,
}

impl Answer {
    /// The message this answer is.
    pub(crate) fn to_value(&self) -> Value {
        match self {
            Self::Result(payload) => message("result", vec![("payload", payload.clone())]),
            Self::Refused(refusal) => message(
                "refused",
                vec![
                    ("code", Value::text(refusal.code.code())),
                    ("kind", Value::text(refusal.kind.word())),
                    ("sentence", Value::text(refusal.sentence.as_str())),
                ],
            ),
            Self::Registry(commands) => message(
                "registry",
                vec![("commands", Value::Array(commands.clone()))],
            ),
            Self::Tools(commands) => {
                message("tools", vec![("commands", Value::Array(commands.clone()))])
            }
            Self::Session => message("session", Vec::new()),
            Self::Status(status) => message("status", vec![("status", status.clone())]),
            Self::Stopping => message("stopping", Vec::new()),
            Self::Changed { revision, state } => {
                let mut fields = vec![("revision", counted(*revision))];
                if let Some(state) = state {
                    fields.push(("state", state.clone()));
                }
                message("changed", fields)
            }
            Self::Noted => message("noted", Vec::new()),
            Self::Displayed => message("displayed", Vec::new()),
            Self::Attached => message("attached", Vec::new()),
            Self::Settled => message("settled", Vec::new()),
        }
    }

    /// The answer `value` is.
    pub(crate) fn from_value(value: Value) -> Result<Self, IpcError> {
        let mut fields = Fields::of(value)?;
        let answer = match fields.kind.as_str() {
            "result" => Self::Result(fields.value("payload")?),
            "refused" => {
                let code = fields.text("code")?;
                let kind = fields.text("kind")?;
                Self::Refused(Refusal {
                    code: ErrorCode::from_code(&code).ok_or_else(|| {
                        malformed(format!(
                            "a refusal with a code this build does not know, `{code}`"
                        ))
                    })?,
                    kind: RefusalKind::of(&kind).ok_or_else(|| {
                        malformed(format!(
                            "a refusal of a kind this build does not know, `{kind}`"
                        ))
                    })?,
                    sentence: fields.text("sentence")?,
                })
            }
            "registry" => Self::Registry(
                fields
                    .value("commands")?
                    .into_array()
                    .ok_or_else(|| malformed("a registry whose commands are not a list"))?,
            ),
            "tools" => Self::Tools(
                fields
                    .value("commands")?
                    .into_array()
                    .ok_or_else(|| malformed("a list of tools whose commands are not a list"))?,
            ),
            "session" => Self::Session,
            "status" => Self::Status(fields.object("status")?),
            "stopping" => Self::Stopping,
            "changed" => Self::Changed {
                revision: fields.count("revision")?,
                state: fields.object_if_there("state")?,
            },
            "noted" => Self::Noted,
            "displayed" => Self::Displayed,
            "attached" => Self::Attached,
            "settled" => Self::Settled,
            other => {
                return Err(malformed(format!(
                    "a `{other}` message where an answer is read"
                )));
            }
        };
        fields.done()?;
        Ok(answer)
    }
}

#[cfg(test)]
mod tests {
    use nc_canon::Value;
    use nc_catalog::Locale;

    use super::{Answer, Graphical, Note, Request, message};
    use crate::IpcError;

    /// What a registry request written as `fields` is read back as.
    fn read(fields: Vec<(&str, Value)>) -> Result<Request, IpcError> {
        Request::from_value(message("registry", fields))
    }

    /// That `read` is refused as malformed, in a sentence holding `says`.
    fn refused_saying<T: core::fmt::Debug>(read: Result<T, IpcError>, says: &str, what: &str) {
        match read {
            Err(refused @ IpcError::Malformed(_)) => {
                assert!(refused.to_string().contains(says), "{what}: {refused}");
            }
            other => panic!("{what} was read as {other:?}"),
        }
    }

    /// An identifier as the profile's minter gives one: twelve characters,
    /// as every one minted before about 2034 is written.
    fn minted() -> nc_ids::Snowflake {
        let flake = nc_ids::MachineFlake::from_value(1).expect("a flake");
        nc_ids::Minter::new(flake, nc_ids::SystemClock)
            .mint()
            .expect("minted")
    }

    /// A session request names one agent, by the one spelling a minted
    /// identifier has.
    ///
    /// **Read back only as `to_crockford` writes it**: the twelve characters
    /// it writes are read back as the same agent, and the same value written
    /// in lower case, with a leading zero — thirteen characters that do not
    /// write back as read — eleven or fourteen characters, or not as text at
    /// all is refused as malformed, and so is a session request holding any
    /// field beside its agent, or none.
    #[test]
    fn a_session_request_names_one_agent_by_the_one_spelling_a_minted_identifier_has() {
        let agent = minted();
        let written = agent.to_crockford();
        assert_eq!(written.len(), nc_ids::base32::MIN_WIDTH, "{written}");
        let request = Request::Session { agent };
        assert_eq!(
            request.to_value().to_canonical(),
            format!("{{\"agent\":\"{written}\",\"type\":\"session\"}}")
        );
        assert_eq!(
            Request::from_value(request.to_value()).expect("read back"),
            request
        );
        let session = |fields: Vec<(&str, Value)>| Request::from_value(message("session", fields));
        let not_as_written = "which is not an identifier as one is written";
        for (text, what) in [
            (written.to_lowercase(), "in lower case"),
            (format!("0{written}"), "with a leading zero"),
            (written[1..].to_owned(), "eleven characters"),
            (format!("{written}00"), "fourteen characters"),
            (String::new(), "empty"),
        ] {
            refused_saying(
                session(vec![("agent", Value::text(text.as_str()))]),
                not_as_written,
                what,
            );
        }
        refused_saying(
            session(vec![("agent", Value::Integer(1))]),
            "a `session` message whose `agent` is not text",
            "an integer",
        );
        refused_saying(
            session(Vec::new()),
            "a `session` message with no `agent`",
            "no agent",
        );
        for field in ["name", "confirmed"] {
            refused_saying(
                session(vec![
                    ("agent", Value::text(written.as_str())),
                    (field, Value::Bool(true)),
                ]),
                &format!("a field the protocol does not read, `{field}`"),
                field,
            );
        }
        // The answer names nothing: the session opened, or a refusal.
        assert_eq!(
            Answer::Session.to_value().to_canonical(),
            "{\"type\":\"session\"}"
        );
        assert_eq!(
            Answer::from_value(Answer::Session.to_value()).expect("read back"),
            Answer::Session
        );
        refused_saying(
            Answer::from_value(message("session", vec![("agent", Value::text(written))])),
            "a field the protocol does not read, `agent`",
            "a session answer naming an agent",
        );
    }

    /// A tools request names its locale as a registry request does, and its
    /// answer is a list as the registry's is.
    ///
    /// **One reading of a locale for both**: each locale this build ships is
    /// read back as written, and `pt-br`, none, or an integer is refused.
    #[test]
    fn a_tools_request_names_its_locale_as_a_registry_request_does() {
        for &locale in Locale::ALL {
            let request = Request::Tools { locale };
            assert_eq!(
                request.to_value().to_canonical(),
                format!("{{\"locale\":\"{}\",\"type\":\"tools\"}}", locale.tag())
            );
            assert_eq!(
                Request::from_value(request.to_value()).expect("read back"),
                request
            );
        }
        let tools = |fields: Vec<(&str, Value)>| Request::from_value(message("tools", fields));
        for (fields, says) in [
            (
                vec![("locale", Value::text("pt-br"))],
                "a `tools` message naming `pt-br`, a locale this build does not ship",
            ),
            (Vec::new(), "a `tools` message with no `locale`"),
            (
                vec![("locale", Value::Integer(1))],
                "a `tools` message whose `locale` is not text",
            ),
        ] {
            refused_saying(tools(fields.clone()), says, &format!("{fields:?}"));
        }
        let answer = Answer::Tools(vec![Value::object([("name", Value::text("naming.read"))])]);
        assert_eq!(
            answer.to_value().to_canonical(),
            "{\"commands\":[{\"name\":\"naming.read\"}],\"type\":\"tools\"}"
        );
        assert_eq!(
            Answer::from_value(answer.to_value()).expect("read back"),
            answer
        );
        refused_saying(
            Answer::from_value(message("tools", vec![("commands", Value::Integer(1))])),
            "a list of tools whose commands are not a list",
            "tools that are not a list",
        );
    }

    /// No request the wire reads carries a confirmation.
    ///
    /// **Nothing a client writes beside a dispatch is read**: a dispatch
    /// holding a `confirmation`, a `confirmed_by_human` or an `approval` is
    /// refused as malformed, whatever it holds, so no client presents one; and
    /// every request is one of these, none of which holds anything a
    /// confirmation could be — a watch, a note, an announced environment, an
    /// attach, a confirm and a decline each refuse the same three fields
    /// beside their own, and a note's detail is read by nothing that
    /// dispatches. **A confirm names a call the kernel holds, by its serial
    /// alone**: it carries no command and no arguments, and refuses either
    /// beside its serial, so no request makes the kernel run a command it was
    /// not already holding for the person.
    #[test]
    fn no_request_the_wire_reads_carries_a_confirmation() {
        for request in [
            Request::Dispatch {
                command: "naming.read".to_owned(),
                args: Value::object::<&str>([]),
            },
            Request::Registry { locale: Locale::En },
            Request::Tools { locale: Locale::En },
            Request::Session { agent: minted() },
            Request::Status,
            Request::Stop,
            Request::Watch {
                seen: Some(7),
                wait_ms: 1,
            },
            Request::Note(a_note()),
            Request::Display(a_display()),
            Request::AttachDesktop { proof: [7; 32] },
            Request::Confirm { pending: 3 },
            Request::Decline { pending: 3 },
        ] {
            // Exhaustive and with no wildcard: a request added here is read
            // for what it carries before it compiles.
            let carries = match &request {
                Request::Dispatch { .. } => "a command and its arguments",
                Request::Registry { .. } | Request::Tools { .. } => "a locale",
                Request::Session { .. } => "an agent",
                Request::Status | Request::Stop => "nothing",
                Request::Watch { .. } => "a revision seen and a wait",
                Request::Note(_) => "what an interface draws, which nothing dispatches",
                Request::Display(_) => "four variables of a graphical session",
                Request::AttachDesktop { .. } => {
                    "a proof of the desktop's secret for this connection"
                }
                Request::Confirm { .. } | Request::Decline { .. } => {
                    "the serial of a call the kernel holds, never a command"
                }
            };
            let read = Request::from_value(request.to_value()).expect("read back");
            assert_eq!(read, request, "{carries}");
        }
        for field in ["confirmation", "confirmed_by_human", "approval"] {
            refused_saying(
                Request::from_value(message(
                    "dispatch",
                    vec![
                        ("command", Value::text("naming.read")),
                        ("args", Value::object::<&str>([])),
                        (field, Value::Bool(true)),
                    ],
                )),
                &format!("a field the protocol does not read, `{field}`"),
                field,
            );
            for request in [
                Request::Watch {
                    seen: None,
                    wait_ms: 0,
                },
                Request::Note(a_note()),
                Request::Display(a_display()),
                Request::AttachDesktop { proof: [7; 32] },
                Request::Confirm { pending: 3 },
                Request::Decline { pending: 3 },
            ] {
                let mut written = request.to_value().into_object().expect("an object");
                written.insert(field.to_owned(), Value::Bool(true));
                refused_saying(
                    Request::from_value(Value::Object(written)),
                    &format!("a field the protocol does not read, `{field}`"),
                    &format!("{request:?} with `{field}`"),
                );
            }
        }
    }

    /// An attach carries one proof, and a confirm and a decline one serial,
    /// each written one way and read back only so; their answers hold
    /// nothing.
    ///
    /// **Strict**: a proof that is not 32 bytes as 64 lower-case hexadecimal
    /// digits, a serial below zero or not a whole number, and a field beside
    /// either — a `command` or `args` beside a serial among them — are
    /// refused, as is an answer holding anything.
    #[test]
    fn an_attach_carries_one_proof_and_a_confirm_one_serial_each_read_back_strictly() {
        let proof = [0xab_u8; 32];
        for (request, written) in [
            (
                Request::AttachDesktop { proof },
                format!(r#"{{"proof":"{}","type":"attach"}}"#, "ab".repeat(32)),
            ),
            (
                Request::Confirm { pending: 41 },
                r#"{"pending":41,"type":"confirm"}"#.to_owned(),
            ),
            (
                Request::Decline { pending: 0 },
                r#"{"pending":0,"type":"decline"}"#.to_owned(),
            ),
        ] {
            assert_eq!(request.to_value().to_canonical(), written);
            assert_eq!(
                Request::from_value(request.to_value()).expect("read back"),
                request
            );
        }
        let attach = |fields: Vec<(&str, Value)>| Request::from_value(message("attach", fields));
        for (fields, says) in [
            (Vec::new(), "a `attach` message with no `proof`"),
            (
                vec![("proof", Value::text("AB".repeat(32)))],
                "a `attach` message whose `proof` is not 32 bytes",
            ),
            (
                vec![("proof", Value::text("ab".repeat(31)))],
                "a `attach` message whose `proof` is not 32 bytes",
            ),
            (
                vec![("proof", Value::Integer(1))],
                "a `attach` message whose `proof` is not text",
            ),
            (
                vec![
                    ("proof", Value::text("ab".repeat(32))),
                    ("pid", Value::Integer(1)),
                ],
                "a field the protocol does not read, `pid`",
            ),
        ] {
            refused_saying(attach(fields.clone()), says, &format!("{fields:?}"));
        }
        for kind in ["confirm", "decline"] {
            let settle = |fields: Vec<(&str, Value)>| Request::from_value(message(kind, fields));
            for (fields, says) in [
                (Vec::new(), format!("a `{kind}` message with no `pending`")),
                (
                    vec![("pending", Value::Integer(-1))],
                    format!("a `{kind}` message whose `pending` is below zero"),
                ),
                (
                    vec![("pending", Value::text("1"))],
                    format!("a `{kind}` message whose `pending` is not a whole number"),
                ),
                (
                    vec![
                        ("pending", Value::Integer(1)),
                        ("command", Value::text("reference.irreversible.do")),
                    ],
                    "a field the protocol does not read, `command`".to_owned(),
                ),
                (
                    vec![
                        ("pending", Value::Integer(1)),
                        ("args", Value::object::<&str>([])),
                    ],
                    "a field the protocol does not read, `args`".to_owned(),
                ),
            ] {
                refused_saying(settle(fields.clone()), &says, &format!("{kind} {fields:?}"));
            }
        }
        for (answer, written) in [
            (Answer::Attached, r#"{"type":"attached"}"#),
            (Answer::Settled, r#"{"type":"settled"}"#),
        ] {
            assert_eq!(answer.to_value().to_canonical(), written);
            assert_eq!(
                Answer::from_value(answer.to_value()).expect("read back"),
                answer
            );
            let kind = if answer == Answer::Attached {
                "attached"
            } else {
                "settled"
            };
            refused_saying(
                Answer::from_value(message(kind, vec![("pending", Value::Integer(1))])),
                "a field the protocol does not read, `pending`",
                kind,
            );
        }
    }

    /// A note, as an interface writes one.
    fn a_note() -> Note {
        Note::new(Value::object([("scroll", Value::Integer(120))])).expect("a note")
    }

    /// An environment naming an X display and its authority file.
    fn a_display() -> Graphical {
        Graphical::new([("DISPLAY", ":0"), ("XAUTHORITY", "/run/user/1000/xauth")])
            .expect("an environment")
    }

    /// A watch names the revision it has seen, or none, and how long it
    /// waits, and its answer the revision the state is at.
    ///
    /// **Each written one way and read back only so**: a watch that has seen
    /// nothing writes no `seen`, and one whose `seen` or `wait_ms` is below
    /// zero, not a whole number or missing is refused; an answer holds the
    /// state as one object or none at all, and a revision below zero, a state
    /// that is not an object and a field beside the two are refused.
    #[test]
    fn a_watch_names_the_revision_seen_and_its_answer_the_one_the_state_is_at() {
        for (request, written) in [
            (
                Request::Watch {
                    seen: None,
                    wait_ms: 0,
                },
                r#"{"type":"watch","wait_ms":0}"#,
            ),
            (
                Request::Watch {
                    seen: Some(0),
                    wait_ms: 8000,
                },
                r#"{"seen":0,"type":"watch","wait_ms":8000}"#,
            ),
            (
                Request::Watch {
                    seen: Some(41),
                    wait_ms: 250,
                },
                r#"{"seen":41,"type":"watch","wait_ms":250}"#,
            ),
        ] {
            assert_eq!(request.to_value().to_canonical(), written);
            assert_eq!(
                Request::from_value(request.to_value()).expect("read back"),
                request
            );
        }
        let watch = |fields: Vec<(&str, Value)>| Request::from_value(message("watch", fields));
        for (fields, says) in [
            (Vec::new(), "a `watch` message with no `wait_ms`"),
            (
                vec![("wait_ms", Value::Integer(-1))],
                "a `watch` message whose `wait_ms` is below zero",
            ),
            (
                vec![("wait_ms", Value::text("1"))],
                "a `watch` message whose `wait_ms` is not a whole number",
            ),
            (
                vec![("wait_ms", Value::Integer(1)), ("seen", Value::Integer(-1))],
                "a `watch` message whose `seen` is below zero",
            ),
            (
                vec![("wait_ms", Value::Integer(1)), ("seen", Value::Null)],
                "a `watch` message whose `seen` is not a whole number",
            ),
            (
                vec![("wait_ms", Value::Integer(1)), ("since", Value::Integer(1))],
                "a field the protocol does not read, `since`",
            ),
        ] {
            refused_saying(watch(fields.clone()), says, &format!("{fields:?}"));
        }
        let state = Value::object([("windows", Value::Array(Vec::new()))]);
        for (answer, written) in [
            (
                Answer::Changed {
                    revision: 42,
                    state: Some(state.clone()),
                },
                r#"{"revision":42,"state":{"windows":[]},"type":"changed"}"#,
            ),
            (
                Answer::Changed {
                    revision: 41,
                    state: None,
                },
                r#"{"revision":41,"type":"changed"}"#,
            ),
        ] {
            assert_eq!(answer.to_value().to_canonical(), written);
            assert_eq!(
                Answer::from_value(answer.to_value()).expect("read back"),
                answer
            );
        }
        let changed = |fields: Vec<(&str, Value)>| Answer::from_value(message("changed", fields));
        for (fields, says) in [
            (Vec::new(), "a `changed` message with no `revision`"),
            (
                vec![("revision", Value::Integer(-1))],
                "a `changed` message whose `revision` is below zero",
            ),
            (
                vec![("revision", Value::Integer(1)), ("state", Value::Null)],
                "a `changed` message whose `state` is not an object",
            ),
            (
                vec![("revision", Value::Integer(1)), ("shell", state)],
                "a field the protocol does not read, `shell`",
            ),
        ] {
            refused_saying(changed(fields.clone()), says, &format!("{fields:?}"));
        }
    }

    /// A count past what JSON's integer holds is written as the largest it
    /// holds, never as a number below zero.
    #[test]
    fn a_count_past_the_wires_integer_is_written_as_the_largest_it_holds() {
        assert_eq!(super::counted(0), Value::Integer(0));
        assert_eq!(super::counted(41), Value::Integer(41));
        assert_eq!(super::counted(u64::MAX), Value::Integer(i64::MAX));
    }

    /// A note is one object within its bound, made and read by one rule.
    ///
    /// **`ipc_note_max_bytes` of canonical text, exactly**: a note at the
    /// bound is made, written and read back, and one a byte past it is
    /// refused as it is made and as it is read off the wire; a detail that is
    /// not an object, none at all, and a field beside it are refused; and its
    /// answer holds nothing.
    #[test]
    fn a_note_is_one_object_within_its_bound_made_and_read_by_one_rule() {
        let most = usize::try_from(nc_catalog::DAEMON.ipc_note_max_bytes).expect("a length");
        // `{"draft":""}` is twelve bytes around the text.
        let of = |length: usize| Value::object([("draft", Value::text("a".repeat(length - 12)))]);
        assert_eq!(of(most).to_canonical().len(), most);
        let note = Note::new(of(most)).expect("a note at its bound");
        assert_eq!(note.detail(), &of(most));
        let request = Request::Note(note);
        assert_eq!(
            Request::from_value(request.to_value()).expect("read back"),
            request
        );
        assert_eq!(
            Request::Note(a_note()).to_value().to_canonical(),
            r#"{"detail":{"scroll":120},"type":"note"}"#
        );
        let past = format!(
            "a note of {} bytes, and `ipc_note_max_bytes` is {most}",
            most + 1
        );
        refused_saying(Note::new(of(most + 1)), &past, "made past its bound");
        refused_saying(
            Request::from_value(message("note", vec![("detail", of(most + 1))])),
            &past,
            "read past its bound",
        );
        for detail in [
            Value::Array(Vec::new()),
            Value::text("{}"),
            Value::Null,
            Value::Integer(1),
        ] {
            refused_saying(
                Note::new(detail.clone()),
                "a note whose `detail` is not an object",
                &format!("{detail:?}"),
            );
            refused_saying(
                Request::from_value(message("note", vec![("detail", detail.clone())])),
                "a note whose `detail` is not an object",
                &format!("{detail:?} on the wire"),
            );
        }
        refused_saying(
            Request::from_value(message("note", Vec::new())),
            "a `note` message with no `detail`",
            "no detail",
        );
        assert_eq!(
            Answer::Noted.to_value().to_canonical(),
            r#"{"type":"noted"}"#
        );
        assert_eq!(
            Answer::from_value(Answer::Noted.to_value()).expect("read back"),
            Answer::Noted
        );
        refused_saying(
            Answer::from_value(message("noted", vec![("detail", of(13))])),
            "a field the protocol does not read, `detail`",
            "a noted answer holding a detail",
        );
    }

    /// An announced environment names a display, by a closed list of names,
    /// each value bounded and plain.
    ///
    /// **One rule where it is made and where it is read**: every name of the
    /// list is announced and read back; a name outside it, a value empty,
    /// past `ipc_display_value_max_bytes` by one byte, holding a NUL or
    /// another control character, a path that is not absolute, a name
    /// written twice and an environment naming no display are each refused,
    /// from the constructor and off the wire alike.
    #[test]
    fn an_announced_environment_names_a_display_by_a_closed_list_bounded_and_plain() {
        let all = [
            ("DISPLAY", ":1"),
            ("WAYLAND_DISPLAY", "wayland-1"),
            ("XAUTHORITY", "/home/a/.Xauthority"),
            ("XDG_RUNTIME_DIR", "/run/user/1000"),
        ];
        assert_eq!(all.map(|(name, _)| name), Graphical::NAMES);
        let environment = Graphical::new(all).expect("every name announced");
        assert_eq!(environment.pairs().collect::<Vec<_>>(), all);
        assert_eq!(environment.get("WAYLAND_DISPLAY"), Some("wayland-1"));
        assert_eq!(environment.get("PATH"), None);
        let request = Request::Display(environment);
        assert_eq!(
            request.to_value().to_canonical(),
            "{\"environment\":{\"DISPLAY\":\":1\",\"WAYLAND_DISPLAY\":\"wayland-1\",\
             \"XAUTHORITY\":\"/home/a/.Xauthority\",\"XDG_RUNTIME_DIR\":\"/run/user/1000\"},\
             \"type\":\"display\"}"
        );
        assert_eq!(
            Request::from_value(request.to_value()).expect("read back"),
            request
        );
        for alone in Graphical::DISPLAYS {
            let one = Graphical::new([(alone, "a")]).expect("one display is an environment");
            assert_eq!(one.pairs().collect::<Vec<_>>(), [(alone, "a")]);
        }
        let most =
            usize::try_from(nc_catalog::DAEMON.ipc_display_value_max_bytes).expect("a length");
        Graphical::new([("DISPLAY", "a".repeat(most))]).expect("a value at its bound");
        let long = "a".repeat(most + 1);
        let too_long = format!(
            "an environment whose `DISPLAY` is {} bytes, and `ipc_display_value_max_bytes` is \
             {most}",
            most + 1
        );
        for (pairs, says) in [
            (
                vec![("DISPLAY", ":0"), ("LD_PRELOAD", "/tmp/a.so")],
                "an environment naming `LD_PRELOAD`, which is none of DISPLAY, WAYLAND_DISPLAY, \
                 XAUTHORITY, XDG_RUNTIME_DIR",
            ),
            (
                vec![("display", ":0")],
                "an environment naming `display`, which is none of",
            ),
            (
                vec![("DISPLAY", "")],
                "an environment whose `DISPLAY` is empty",
            ),
            (vec![("DISPLAY", long.as_str())], too_long.as_str()),
            (
                vec![("WAYLAND_DISPLAY", "wayland-0\0")],
                "an environment whose `WAYLAND_DISPLAY` holds a control character",
            ),
            (
                vec![("DISPLAY", ":0\n")],
                "an environment whose `DISPLAY` holds a control character",
            ),
            (
                vec![("DISPLAY", ":0"), ("XAUTHORITY", ".Xauthority")],
                "an environment whose `XAUTHORITY` is not an absolute path",
            ),
            (
                vec![("WAYLAND_DISPLAY", "wayland-0"), ("XDG_RUNTIME_DIR", "run")],
                "an environment whose `XDG_RUNTIME_DIR` is not an absolute path",
            ),
            (
                vec![("XAUTHORITY", "/a"), ("XDG_RUNTIME_DIR", "/run")],
                "an environment naming no display, neither DISPLAY nor WAYLAND_DISPLAY",
            ),
            (
                Vec::new(),
                "an environment naming no display, neither DISPLAY nor WAYLAND_DISPLAY",
            ),
            (
                vec![("DISPLAY", ":0"), ("DISPLAY", ":1")],
                "an environment naming `DISPLAY` twice",
            ),
        ] {
            refused_saying(
                Graphical::new(pairs.clone()),
                says,
                &format!("{pairs:?} made"),
            );
            // An object names a key once, so the wire cannot write the last.
            if pairs.len() == 2 && pairs[0].0 == pairs[1].0 {
                continue;
            }
            let written = Value::object(
                pairs
                    .iter()
                    .map(|(name, value)| (*name, Value::text(*value))),
            );
            refused_saying(
                Request::from_value(message("display", vec![("environment", written)])),
                says,
                &format!("{pairs:?} read"),
            );
        }
    }

    /// A display message holds one object of texts, and its answer nothing.
    ///
    /// A value that is not text, an environment that is not an object or is
    /// missing, and an answer holding anything are each refused.
    #[test]
    fn a_display_message_holds_one_object_of_texts_and_its_answer_nothing() {
        let display = |environment: Value| {
            Request::from_value(message("display", vec![("environment", environment)]))
        };
        refused_saying(
            display(Value::object([("DISPLAY", Value::Integer(0))])),
            "an environment whose `DISPLAY` is not text",
            "a value that is not text",
        );
        refused_saying(
            display(Value::text(":0")),
            "a `display` message whose `environment` is not an object",
            "an environment that is not an object",
        );
        refused_saying(
            Request::from_value(message("display", Vec::new())),
            "a `display` message with no `environment`",
            "no environment",
        );
        assert_eq!(
            Answer::Displayed.to_value().to_canonical(),
            r#"{"type":"displayed"}"#
        );
        assert_eq!(
            Answer::from_value(Answer::Displayed.to_value()).expect("read back"),
            Answer::Displayed
        );
        refused_saying(
            Answer::from_value(message("displayed", vec![("DISPLAY", Value::text(":0"))])),
            "a field the protocol does not read, `DISPLAY`",
            "a displayed answer holding a variable",
        );
    }

    /// A registry request names its locale, read back as written, and one
    /// naming no locale this build ships — or none at all — is refused as
    /// malformed.
    ///
    /// **By its exact tag**: BCP 47 compares tags without case, and the
    /// protocol reads a message back only as this code writes it, so `pt-br`
    /// is a peer that does not speak this build's protocol, not Portuguese.
    #[test]
    fn a_registry_request_names_its_locale_and_one_naming_no_locale_this_build_has_is_refused() {
        for &locale in Locale::ALL {
            let request = Request::Registry { locale };
            assert_eq!(
                Request::from_value(request.to_value()).expect("read back"),
                request
            );
            assert_eq!(
                request.to_value().to_canonical(),
                format!("{{\"locale\":\"{}\",\"type\":\"registry\"}}", locale.tag())
            );
        }
        for (fields, says) in [
            (Vec::new(), "a `registry` message with no `locale`"),
            (
                vec![("locale", Value::text("pt-br"))],
                "naming `pt-br`, a locale this build does not ship",
            ),
            (
                vec![("locale", Value::text("fr\u{202e}"))],
                "naming `fr\\u{202e}`, a locale this build does not ship",
            ),
            (
                vec![("locale", Value::Integer(1))],
                "a `registry` message whose `locale` is not text",
            ),
            (
                vec![("locale", Value::text("en")), ("title", Value::text("en"))],
                "a field the protocol does not read, `title`",
            ),
        ] {
            match read(fields.clone()) {
                Err(refused @ IpcError::Malformed(_)) => {
                    assert!(refused.to_string().contains(says), "{fields:?}: {refused}");
                }
                other => panic!("{fields:?} was read as {other:?}"),
            }
        }
    }
}
