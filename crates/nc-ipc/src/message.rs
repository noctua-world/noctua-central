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

    use super::{Answer, Request, message};
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
    /// confirmation could be.
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
        ] {
            // Exhaustive and with no wildcard: a request added here is read
            // for what it carries before it compiles.
            let carries = match &request {
                Request::Dispatch { .. } => "a command and its arguments",
                Request::Registry { .. } | Request::Tools { .. } => "a locale",
                Request::Session { .. } => "an agent",
                Request::Status | Request::Stop => "nothing",
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
        }
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
