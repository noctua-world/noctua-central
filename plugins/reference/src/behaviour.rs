//! What the reference plugin does, free of the world, so the host can test it.
//!
//! **A table the kernel's suites need, and nothing more.** `activate` declares
//! notes a person reads, writes and deletes in the plugin's own store, each
//! selected by its `note` argument as a Cedar `Content`, one of them written
//! over as many numbered parts as it is told — and the ways a
//! command can go wrong after it has written: refusing, trapping, writing from
//! a read — and a read that traps having written nothing at all. Beside them:
//! the probe, which calls every function of the world
//! through [`Imports`] and answers what each did; a count of this instance's
//! executions, which is how a suite shows a command never ran; a command that
//! cannot be undone; two relays that dispatch another command from inside this
//! one, one that changes state and one that only reads; an echo that answers
//! whatever text it is given, payload or not; the abuses — a trap, a loop that
//! spins, one that burns fuel, a memory that grows without end and a dispatch
//! into this plugin while it runs — and a note written before each of the
//! first four; two commands that write a note here and in the [`PEER`], one of
//! them making the peer trap after it wrote; three over the blobs — one
//! that writes a text some number of times over into a new blob, and then
//! refuses or stops as it is told, one that reads a blob back whole a chunk at
//! a time, and one that asks where a blob is; and two over the network — a
//! read that fetches whatever URL, method, headers and body it is given, a
//! command that changes state and sends a `POST` unless told another method,
//! and one that sends its `POST` with a compensation registered for it; and
//! three that write a note inside a unit they begin, and then commit it, abort
//! it, or let it go; and the rest of the world — events published inside a
//! unit or outside one, timers set by a change or by a read and cancelled, a
//! surface shown, who the call runs for read, and lines written to the
//! plugin's own log. A name it does not declare is not found. The table grows
//! with the kernel's suites.
//!
//! **Its callbacks say what they were called with.** [`on_timer`] and
//! [`on_event`] each write a line to the plugin's log naming what woke them
//! and who they run for, and then do what the tag or the data tells them to —
//! dispatch a note, group two in a unit, set or cancel a timer, publish, or
//! trap — so a kernel suite reads what a callback did from the log and the
//! journal.
//!
//! **How it activates is read from its own store**, so one component serves
//! every case a kernel suite needs without a build per case. `activate` reads
//! the key [`ACTIVATION`]: absent, or the store not granted, it declares its
//! table under [`NAMESPACE`] as written; present, it is an [`Activation`] — the
//! namespace to declare under and the [`Variant`] that breaks the first command
//! in one chosen way, which is how the kernel's suite shows each refusal of
//! `NOCTUA.md §11.7` against a real component. A value it cannot read is a
//! refusal to activate.
//!
//! **The probe is the kernel's coverage test's other half.** [`PROBED`] is every
//! function the world imports, written once here, and [`probe`] calls each of
//! them — a resource's methods on a handle its constructor handed out, or not
//! at all when the constructor was refused — and answers a canonical JSON
//! object from each function to what it answered.
//! `crates/nc-kernel/tests/capabilities.rs` holds `PROBED` to the world and to
//! `catalog/capabilities.toml`, and runs the probe granted nothing, so every
//! function a capability grants must answer it `denied` or, behind a refused
//! constructor, not be reached at all.
//!
//! The declarations are held here to the same judge a kernel holds a plugin's
//! command to at `activate` (`NOCTUA.md §11.7`), so a descriptor this file
//! gets wrong is a failing host test rather than a refused activation.

use std::sync::atomic::{AtomicU64, Ordering};

/// The namespace every command and topic of this plugin starts with.
///
/// **The manifest's, written twice and held together.** `plugin.toml` is what
/// the kernel reads, and a command outside its namespace is refused at
/// activation; a test below reads the manifest and compares.
pub const NAMESPACE: &str = "reference";

/// The key of the plugin's own store that says how it activates.
pub const ACTIVATION: &str = "activation";

/// The namespace the pair commands dispatch into: this same component, loaded
/// a second time as the kernel's suites load it.
pub const PEER: &str = "peer";

/// Every function of the world the probe calls, as `interface#function`, in
/// the order it calls them.
///
/// The function's name is the world's parser's: a resource's method is
/// `[method]<resource>.<name>`, and the listing in `store` is
/// written `list`.
pub const PROBED: [&str; 30] = [
    "log#write",
    "context#principal",
    "context#actor-chain",
    "context#confirmed-by-human",
    "store#get",
    "store#put",
    "store#delete",
    "store#list",
    "blobs#locate",
    "blobs#open",
    "blobs#[method]reader.size",
    "blobs#[method]reader.read",
    "blobs#[method]reader.seek",
    "blobs#create",
    "blobs#[method]writer.write",
    "blobs#[method]writer.finish",
    "blobs#announce",
    "commands#dispatch",
    "commands#begin",
    "commands#[method]unit.compensate-with",
    "commands#[method]unit.commit",
    "commands#[method]unit.abort",
    "events#publish",
    "ui#open-surface",
    "ui#close-surface",
    "ui#render",
    "net#fetch",
    "timers#after",
    "timers#every",
    "timers#cancel",
];

/// Whether a command can be undone, and whether a human must confirm it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reversibility {
    /// Changes nothing.
    Read,
    /// Changes state; undo reverses what the kernel can reverse.
    Write,
    /// Cannot be reversed: requires a human's confirmation.
    Irreversible,
}

/// One command, as `activate` declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// Its name, which starts with [`NAMESPACE`] and a dot.
    pub name: String,
    /// The label a person sees.
    pub title: &'static str,
    /// The JSON Schema of its arguments, in the kernel's closed subset.
    pub args_schema: &'static str,
    /// Whether it mutates state.
    pub mutating: bool,
    /// The Cedar action the decision point evaluates.
    pub action: &'static str,
    /// How the kernel extracts the resource from the arguments.
    pub resource_selector: &'static str,
    /// Read, write or irreversible.
    pub reversibility: Reversibility,
}

/// A refusal, as the world's `error` carries one: its case and its sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// Not granted.
    Denied(String),
    /// Not there.
    NotFound(String),
    /// Not what was expected.
    Invalid(String),
    /// Written against a stale version.
    Conflict(String),
    /// There, and not reachable right now.
    Unavailable(String),
    /// The host's own failure.
    Internal(String),
}

impl Refused {
    /// The world's word for its case, and its sentence.
    #[must_use]
    pub fn parts(&self) -> (&str, &str) {
        match self {
            Self::Denied(why) => ("denied", why),
            Self::NotFound(why) => ("not-found", why),
            Self::Invalid(why) => ("invalid", why),
            Self::Conflict(why) => ("conflict", why),
            Self::Unavailable(why) => ("unavailable", why),
            Self::Internal(why) => ("internal", why),
        }
    }
}

/// What one function of the world answered the probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// It answered, without an error.
    Answered,
    /// It answered with one of the world's errors.
    Refused(Refused),
    /// It was not called: it is a method of a resource, and the function that
    /// hands the resource out refused.
    Unreachable,
}

impl Outcome {
    /// How the probe writes it: `answered`, `unreachable`, or the error's case,
    /// a colon and its sentence.
    #[must_use]
    pub fn written(&self) -> String {
        match self {
            Self::Answered => "answered".to_owned(),
            Self::Unreachable => "unreachable".to_owned(),
            Self::Refused(refused) => {
                let (case, why) = refused.parts();
                format!("{case}: {why}")
            }
        }
    }
}

/// What a key of the plugin's store holds, as `store.get` answers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    /// Its value, as canonical JSON.
    pub value: String,
    /// The revision a write names to replace it.
    pub rev: String,
}

/// A request a command sends: as the world's `net.request` carries one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetch {
    /// The method's word, as the command was told it.
    pub method: String,
    /// Where.
    pub url: String,
    /// Every header, in the order told.
    pub headers: Vec<(String, String)>,
    /// The body, if there is one.
    pub body: Option<Vec<u8>>,
}

/// What a host answered a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    /// Its status.
    pub status: u16,
    /// Every header, in the order the host sent them.
    pub headers: Vec<(String, String)>,
    /// Its whole body.
    pub body: Vec<u8>,
}

/// A blob read back whole: its size, how many bytes each read answered, and
/// the bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blob {
    /// What the reader said its size was.
    pub size: u64,
    /// How many bytes each read answered, in order, the last one that
    /// answered none left out.
    pub chunks: Vec<u64>,
    /// Every byte read.
    pub bytes: Vec<u8>,
}

/// The world's imports, as the probe and the commands call them.
pub trait Imports {
    /// Calls `function` — one of [`PROBED`] — with arguments of the probe's
    /// own, and says what it answered.
    fn call(&mut self, function: &str) -> Outcome;

    /// Reads `key` with `store.get`.
    ///
    /// # Errors
    ///
    /// The store's refusal, as the world's `error`, when it
    /// refuses.
    fn get(&mut self, key: &str) -> Result<Option<Stored>, Refused>;

    /// Writes `value` to `key` with `store.put`, answering the revision
    /// written.
    ///
    /// # Errors
    ///
    /// The store's refusal, as the world's `error`, when it
    /// refuses.
    fn put(&mut self, key: &str, value: &str, expected: Option<&str>) -> Result<String, Refused>;

    /// Deletes `key` with `store.delete`.
    ///
    /// # Errors
    ///
    /// The store's refusal, as the world's `error`, when it
    /// refuses.
    fn delete(&mut self, key: &str, expected: Option<&str>) -> Result<(), Refused>;

    /// Dispatches `name` with `args` through `commands.dispatch`, answering
    /// the command's result.
    ///
    /// # Errors
    ///
    /// The kernel's refusal or the command's, as the world's `error`, when
    /// either refuses.
    fn dispatch(&mut self, name: &str, args: &str) -> Result<String, Refused>;

    /// Asks where the blob `hash` is with `blobs.locate`, answering the
    /// world's word for it: `local`, `remote` or `unavailable`.
    ///
    /// # Errors
    ///
    /// The kernel's refusal, as the world's `error`, when it
    /// refuses.
    fn locate(&mut self, hash: &str) -> Result<String, Refused>;

    /// Writes `piece` `times` over into a new blob, and answers its hash.
    ///
    /// That is `blobs.create`, a `write` of it each time, then `finish`.
    ///
    /// # Errors
    ///
    /// The kernel's refusal of any of those calls, as the world's `error`,
    /// when one refuses.
    fn put_blob(&mut self, piece: &[u8], times: u32) -> Result<String, Refused>;

    /// Reads the blob `hash` whole: `blobs.open`, its `size`, then a `read`
    /// asking for `max` bytes until one answers none.
    ///
    /// # Errors
    ///
    /// The kernel's refusal of any of those calls, as the world's `error`,
    /// when one refuses.
    fn get_blob(&mut self, hash: &str, max: u32) -> Result<Blob, Refused>;

    /// Sends `request` with `net.fetch`, answering what the host answered.
    ///
    /// # Errors
    ///
    /// The kernel's refusal, as the world's `error`, when it
    /// refuses.
    fn fetch(&mut self, request: &Fetch) -> Result<Fetched, Refused>;

    /// Begins a unit labelled `label` with `commands.begin`, answering where
    /// the plugin holds it.
    ///
    /// # Errors
    ///
    /// The kernel's refusal, as the world's `error`, when it
    /// refuses.
    fn begin(&mut self, label: &str) -> Result<usize, Refused>;

    /// Registers `name` with `args` as the compensation of the unit at `unit`.
    ///
    /// # Errors
    ///
    /// The kernel's refusal of `unit.compensate-with`, as the world's
    /// `error`, when it refuses, and [`Refused::Invalid`] when no unit is
    /// held there.
    fn compensate_with(&mut self, unit: usize, name: &str, args: &str) -> Result<(), Refused>;

    /// Commits the unit held at `unit`, with `unit.commit`.
    ///
    /// # Errors
    ///
    /// The kernel's refusal, as the world's `error`, when it refuses, and
    /// [`Refused::Invalid`] when no unit is held there.
    fn commit(&mut self, unit: usize) -> Result<(), Refused>;

    /// Aborts the unit held at `unit`, with `unit.abort`, and lets it go.
    fn abort(&mut self, unit: usize);

    /// Lets the unit held at `unit` go without committing it: its handle
    /// dropped.
    fn release(&mut self, unit: usize);

    /// Writes `text` to the plugin's own log at `level`, a word of the
    /// world's `level`: `trace`, `debug`, `info`, `warn` or `error`.
    fn log(&mut self, level: &str, text: &str);

    /// Publishes `data` under `topic` with `events.publish`.
    ///
    /// # Errors
    ///
    /// The kernel's refusal, as the world's `error`, when it
    /// refuses.
    fn publish(&mut self, topic: &str, data: &str) -> Result<(), Refused>;

    /// Sets a timer of `ms` tagged `tag`, answering its identifier.
    ///
    /// With `timers.every` when `every`, and with `timers.after`
    /// otherwise.
    ///
    /// # Errors
    ///
    /// The kernel's refusal, as the world's `error`, when it
    /// refuses.
    fn set_timer(&mut self, ms: u64, every: bool, tag: &str) -> Result<u64, Refused>;

    /// Cancels the timer `timer` with `timers.cancel`.
    ///
    /// # Errors
    ///
    /// The kernel's refusal, as the world's `error`, when it
    /// refuses.
    fn cancel(&mut self, timer: u64) -> Result<(), Refused>;

    /// Opens a surface of `kind` titled `title`, answering its identifier.
    ///
    /// With `ui.open-surface`, `kind` being one of `panel`, `document`,
    /// `overlay` or `node-3d`.
    ///
    /// # Errors
    ///
    /// The kernel's refusal, as the world's `error`, when it
    /// refuses.
    fn open_surface(&mut self, kind: &str, title: &str) -> Result<String, Refused>;

    /// Closes the surface `surface` with `ui.close-surface`.
    ///
    /// # Errors
    ///
    /// The kernel's refusal, as the world's `error`, when it
    /// refuses.
    fn close_surface(&mut self, surface: &str) -> Result<(), Refused>;

    /// Renders `tree` into the surface `surface` with `ui.render`.
    ///
    /// # Errors
    ///
    /// The kernel's refusal, as the world's `error`, when it
    /// refuses.
    fn render(&mut self, surface: &str, tree: &str) -> Result<(), Refused>;

    /// Who the call runs for, its actor chain and whether a person confirmed
    /// it, as `context` answers each.
    ///
    /// # Errors
    ///
    /// The kernel's refusal of any of the three, as the world's `error`, when
    /// one refuses.
    fn context(&mut self) -> Result<Context, Refused>;
}

/// What `context` answered: who the call runs for, on whose behalf, and
/// whether a person confirmed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    /// The principal, as its opaque identifier.
    pub principal: String,
    /// On whose behalf, outermost first.
    pub actor_chain: Vec<String>,
    /// Whether a person confirmed the call.
    pub confirmed: bool,
}

/// The arguments of a command about a note and nothing else.
const A_NOTE: &str = r#"{"type":"object","properties":{"note":{"type":"string","x-entity":"Content","maxLength":128}},"required":["note"],"additionalProperties":false}"#;

/// The arguments of a command that writes a note: the note, and its text.
const A_NOTE_AND_TEXT: &str = r#"{"type":"object","properties":{"note":{"type":"string","x-entity":"Content","maxLength":128},"text":{"type":"string","maxLength":4096}},"required":["note","text"],"additionalProperties":false}"#;

/// The arguments of a note written in parts: the note, its text, and how
/// many numbered parts of it the text is written to.
const A_NOTE_IN_PARTS: &str = r#"{"type":"object","properties":{"note":{"type":"string","x-entity":"Content","maxLength":128},"text":{"type":"string","maxLength":4096},"parts":{"type":"integer","minimum":1,"maximum":10000}},"required":["note","text","parts"],"additionalProperties":false}"#;

/// The arguments of the count: a note to be authorized on, a whole number it
/// is held to and a flag it may be given — every kind of value a schema can
/// refuse.
const COUNTED: &str = r#"{"type":"object","properties":{"note":{"type":"string","x-entity":"Content","maxLength":128},"at_most":{"type":"integer","minimum":0,"maximum":1000000},"verbose":{"type":"boolean"}},"required":["note","at_most"],"additionalProperties":false}"#;

/// The arguments of a relay: the note it is authorized on, the command it
/// dispatches with its arguments as text, how many times, and whether it goes
/// on past a refusal.
const RELAYED: &str = r#"{"type":"object","properties":{"note":{"type":"string","x-entity":"Content","maxLength":128},"name":{"type":"string","maxLength":96},"args":{"type":"string","maxLength":65536},"times":{"type":"integer","minimum":1,"maximum":2048},"through":{"type":"boolean"}},"required":["note","name","args"],"additionalProperties":false}"#;

/// The arguments of a note written before the command stops as `abuse` says.
const A_NOTE_THEN_ABUSE: &str = r#"{"type":"object","properties":{"note":{"type":"string","x-entity":"Content","maxLength":128},"text":{"type":"string","maxLength":4096},"abuse":{"type":"string","enum":["burn","grow","spin","trap"]}},"required":["note","text","abuse"],"additionalProperties":false}"#;

/// The arguments of a blob made of `text` written `times` over, which then
/// asks where it is when `located`, and refuses or stops as `then` says.
const A_BLOB_PUT: &str = r#"{"type":"object","properties":{"note":{"type":"string","x-entity":"Content","maxLength":128},"text":{"type":"string","maxLength":4096},"times":{"type":"integer","minimum":0,"maximum":1000000},"located":{"type":"boolean"},"then":{"type":"string","enum":["burn","fail","grow","spin","trap"]}},"required":["note","text"],"additionalProperties":false}"#;

/// The arguments of a blob asked about by its hash — any text, so a suite can
/// hand the kernel one that is not a hash — and how much each read asks for.
const A_BLOB: &str = r#"{"type":"object","properties":{"note":{"type":"string","x-entity":"Content","maxLength":128},"hash":{"type":"string","maxLength":4096},"max":{"type":"integer","minimum":0,"maximum":4294967295}},"required":["note","hash"],"additionalProperties":false}"#;

/// The arguments of a request a command sends.
///
/// The note it is authorized on and the URL, and, when they say, the method's
/// word — any text, so a suite can hand the kernel one it refuses — the
/// headers, and a body as text or as `fill` bytes of `x`, written that many
/// times.
const A_FETCH: &str = r#"{"type":"object","properties":{"note":{"type":"string","x-entity":"Content","maxLength":128},"url":{"type":"string","maxLength":8192},"method":{"type":"string","maxLength":16},"headers":{"type":"array","maxItems":128,"items":{"type":"object","properties":{"name":{"type":"string","maxLength":1024},"value":{"type":"string","maxLength":8192}},"required":["name","value"],"additionalProperties":false}},"body":{"type":"string","maxLength":65536},"fill":{"type":"integer","minimum":0,"maximum":20000000}},"required":["note","url"],"additionalProperties":false}"#;

/// The arguments of a request sent with what sets it right.
///
/// The note and the URL, and the command and arguments registered as its
/// compensation, the plugin's own `note.put` when they are not given — and,
/// when `then` is given, the same command registered again with those
/// arguments after it.
const A_COMPENSATED_FETCH: &str = r#"{"type":"object","properties":{"note":{"type":"string","x-entity":"Content","maxLength":128},"url":{"type":"string","maxLength":8192},"compensate":{"type":"string","maxLength":96},"with":{"type":"string","maxLength":65536},"then":{"type":"string","maxLength":65536}},"required":["note","url"],"additionalProperties":false}"#;

/// The arguments of an event published about a note.
///
/// Its topic, its data as text — or, when `fill` is given, `{"fill":"x…"}` of
/// that many `x` — how many times, and whether the command then
/// refuses.
const A_PUBLISH: &str = r#"{"type":"object","properties":{"note":{"type":"string","x-entity":"Content","maxLength":128},"topic":{"type":"string","maxLength":256},"data":{"type":"string","maxLength":65536},"fill":{"type":"integer","minimum":0,"maximum":2000000},"times":{"type":"integer","minimum":1,"maximum":2048},"then":{"type":"string","enum":["fail"]}},"required":["note","topic"],"additionalProperties":false}"#;

/// The arguments of a timer set about a note: how long, its tag, whether it
/// repeats, how many to set, and whether the command then refuses.
const A_TIMER: &str = r#"{"type":"object","properties":{"note":{"type":"string","x-entity":"Content","maxLength":128},"ms":{"type":"integer","minimum":0,"maximum":9007199254740991},"tag":{"type":"string","maxLength":1024},"every":{"type":"boolean"},"times":{"type":"integer","minimum":1,"maximum":128},"then":{"type":"string","enum":["fail"]}},"required":["note","ms","tag"],"additionalProperties":false}"#;

/// The arguments of a timer cancelled: the note it is authorized on, the
/// timer's identifier, and whether the command then refuses.
const A_CANCEL: &str = r#"{"type":"object","properties":{"note":{"type":"string","x-entity":"Content","maxLength":128},"timer":{"type":"integer","minimum":0,"maximum":9007199254740991},"then":{"type":"string","enum":["fail"]}},"required":["note","timer"],"additionalProperties":false}"#;

/// The arguments of a surface shown: its title and kind, or a surface opened
/// already, the tree rendered into it, how many to open, and whether it is
/// then closed.
const A_SURFACE: &str = r#"{"type":"object","properties":{"note":{"type":"string","x-entity":"Content","maxLength":128},"title":{"type":"string","maxLength":1024},"kind":{"type":"string","enum":["document","node-3d","overlay","panel"]},"surface":{"type":"string","maxLength":128},"tree":{"type":"string","maxLength":65536},"times":{"type":"integer","minimum":1,"maximum":64},"close":{"type":"boolean"}},"required":["note"],"additionalProperties":false}"#;

/// The arguments of lines written to the plugin's log: the text, how many
/// times, and at what level.
const A_LOG: &str = r#"{"type":"object","properties":{"note":{"type":"string","x-entity":"Content","maxLength":128},"text":{"type":"string","maxLength":65536},"times":{"type":"integer","minimum":1,"maximum":1000},"level":{"type":"string","enum":["debug","error","info","trace","warn"]}},"required":["note","text"],"additionalProperties":false}"#;

/// The same arguments with a keyword outside the closed subset in them.
const A_NOTE_WITH_A_PATTERN: &str = r#"{"type":"object","properties":{"note":{"type":"string","x-entity":"Content","maxLength":128,"pattern":"^[a-z]+$"}},"required":["note"],"additionalProperties":false}"#;

/// What is wrong, on purpose, with the first command an activation declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    /// Nothing: every command as written.
    Normal,
    /// It declares no action.
    MissingAction,
    /// It declares no resource selector.
    MissingSelector,
    /// It declares no name.
    EmptyName,
    /// It declares no title.
    EmptyTitle,
    /// It declares no argument schema.
    EmptySchema,
    /// Its argument schema uses a keyword outside the closed subset.
    UnknownKeyword,
    /// Its selector names an argument its schema does not declare.
    BadSelector,
    /// It is declared a read and mutating.
    ReadThatMutates,
    /// Its name is outside the namespace the others are declared under.
    OutsideNamespace,
    /// Its name is in a namespace the kernel keeps for its own commands.
    ReservedNamespace,
    /// It names an action the decision point's schema does not declare.
    UnknownAction,
    /// `activate` traps instead of answering.
    Trap,
}

impl Variant {
    /// Every variant, in the order declared.
    pub const ALL: [Self; 13] = [
        Self::Normal,
        Self::MissingAction,
        Self::MissingSelector,
        Self::EmptyName,
        Self::EmptyTitle,
        Self::EmptySchema,
        Self::UnknownKeyword,
        Self::BadSelector,
        Self::ReadThatMutates,
        Self::OutsideNamespace,
        Self::ReservedNamespace,
        Self::UnknownAction,
        Self::Trap,
    ];

    /// The word an activation writes it as.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::MissingAction => "missing-action",
            Self::MissingSelector => "missing-selector",
            Self::EmptyName => "empty-name",
            Self::EmptyTitle => "empty-title",
            Self::EmptySchema => "empty-schema",
            Self::UnknownKeyword => "unknown-keyword",
            Self::BadSelector => "bad-selector",
            Self::ReadThatMutates => "read-that-mutates",
            Self::OutsideNamespace => "outside-namespace",
            Self::ReservedNamespace => "reserved-namespace",
            Self::UnknownAction => "unknown-action",
            Self::Trap => "trap",
        }
    }

    /// The variant `word` names, if it names one.
    #[must_use]
    pub fn named(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|variant| variant.word() == word)
    }
}

/// How the plugin activates.
///
/// The namespace it declares its commands under, what, if anything, is wrong
/// with the first of them, and the topics it subscribes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activation {
    /// The namespace every command is declared under.
    pub namespace: String,
    /// What is wrong with the first command.
    pub variant: Variant,
    /// The topic patterns it subscribes to, which its manifest's `[events]
    /// subscribe` must declare.
    pub subscribe: Vec<String>,
}

impl Default for Activation {
    /// The activation with nothing in the store: [`NAMESPACE`], as written,
    /// subscribing to nothing.
    fn default() -> Self {
        Self {
            namespace: NAMESPACE.to_owned(),
            variant: Variant::Normal,
            subscribe: Vec::new(),
        }
    }
}

impl Activation {
    /// The activation a store holds, as canonical JSON.
    ///
    /// Written `{"namespace":"…","subscribe":["…"],"variant":"…"}`, the
    /// subscriptions left out when there are none.
    ///
    /// # Errors
    ///
    /// [`Refused::Invalid`] for a value that is not that object, with nothing
    /// else in it, a variant [`Variant::named`] knows and every subscription a
    /// text.
    pub fn read(stored: &str) -> Result<Self, Refused> {
        let invalid = |why: &str| {
            Refused::Invalid(format!(
                "the store's `{ACTIVATION}` is not an activation the reference plugin reads: \
                 {why}"
            ))
        };
        let mut fields = nc_canon::parse(stored)
            .map_err(|error| invalid(&error.to_string()))?
            .into_object()
            .ok_or_else(|| invalid("it is not an object"))?;
        let mut text = |name: &str| {
            fields
                .remove(name)
                .and_then(nc_canon::Value::into_text)
                .ok_or_else(|| invalid(&format!("`{name}` is not text")))
        };
        let (namespace, variant) = (text("namespace")?, text("variant")?);
        let subscribe = match fields.remove("subscribe") {
            None => Vec::new(),
            Some(listed) => listed
                .into_array()
                .ok_or_else(|| invalid("`subscribe` is not a list"))?
                .into_iter()
                .map(|topic| {
                    topic
                        .into_text()
                        .ok_or_else(|| invalid("`subscribe` holds what is not a topic"))
                })
                .collect::<Result<_, _>>()?,
        };
        if !fields.is_empty() {
            return Err(invalid(
                "it holds a field beside `namespace`, `subscribe` and `variant`",
            ));
        }
        let variant = Variant::named(&variant)
            .ok_or_else(|| invalid(&format!("no variant is `{variant}`")))?;
        Ok(Self {
            namespace,
            variant,
            subscribe,
        })
    }

    /// The activation as a store holds it, for [`Activation::read`].
    #[must_use]
    pub fn written(&self) -> String {
        let mut fields = vec![
            ("namespace", nc_canon::Value::text(self.namespace.clone())),
            (
                "variant",
                nc_canon::Value::text(self.variant.word().to_owned()),
            ),
        ];
        if !self.subscribe.is_empty() {
            fields.push((
                "subscribe",
                nc_canon::Value::Array(
                    self.subscribe
                        .iter()
                        .map(|topic| nc_canon::Value::text(topic.clone()))
                        .collect(),
                ),
            ));
        }
        nc_canon::Value::object(fields).to_canonical()
    }

    /// Whether `activate` traps rather than answering.
    #[must_use]
    pub fn traps(&self) -> bool {
        self.variant == Variant::Trap
    }
}

/// The commands this plugin declares when it is activated with nothing in its
/// store.
#[must_use]
pub fn commands() -> Vec<Command> {
    commands_for(&Activation::default())
}

/// The commands this plugin declares when it is activated as `activation` says.
///
/// Its table, under the activation's namespace, with the first command broken
/// as its variant says.
#[must_use]
pub fn commands_for(activation: &Activation) -> Vec<Command> {
    let mut table = table(&activation.namespace);
    if let Some(first) = table.first_mut() {
        broken(first, activation);
    }
    table
}

/// `command` with what `activation`'s variant says is wrong with it.
fn broken(command: &mut Command, activation: &Activation) {
    match activation.variant {
        Variant::Normal | Variant::Trap => {}
        Variant::MissingAction => command.action = "",
        Variant::MissingSelector => command.resource_selector = "",
        Variant::EmptyName => command.name = String::new(),
        Variant::EmptyTitle => command.title = "",
        Variant::EmptySchema => command.args_schema = "",
        Variant::UnknownKeyword => command.args_schema = A_NOTE_WITH_A_PATTERN,
        Variant::BadSelector => command.resource_selector = "argument:nothing",
        Variant::ReadThatMutates => command.mutating = true,
        Variant::OutsideNamespace => {
            command.name = format!("{}-elsewhere.note.get", activation.namespace);
        }
        Variant::ReservedNamespace => "kernel.note.get".clone_into(&mut command.name),
        Variant::UnknownAction => command.action = "Rename",
    }
}

/// One command of the table under `namespace`: mutating unless it is a read,
/// and acting on the note its arguments name.
fn row(
    namespace: &str,
    name: &str,
    title: &'static str,
    args_schema: &'static str,
    action: &'static str,
    reversibility: Reversibility,
) -> Command {
    Command {
        name: format!("{namespace}.{name}"),
        title,
        args_schema,
        mutating: reversibility != Reversibility::Read,
        action,
        resource_selector: "argument:note",
        reversibility,
    }
}

/// The table, every command as written, under `namespace`.
fn table(namespace: &str) -> Vec<Command> {
    let command = |name: &str, title, args_schema, action, reversibility| {
        row(namespace, name, title, args_schema, action, reversibility)
    };
    let mut table: Vec<Command> = vec![
        command(
            "note.get",
            "Read a note",
            A_NOTE,
            "Read",
            Reversibility::Read,
        ),
        // **Mutating, so the only refusal it can meet is a capability's.** A
        // read is refused every write by the kernel's frame rules, which would
        // stand between the probe and the functions it exists to reach.
        command(
            "probe",
            "Call every function of the world",
            A_NOTE,
            "Write",
            Reversibility::Write,
        ),
        command(
            "note.put",
            "Write a note",
            A_NOTE_AND_TEXT,
            "Write",
            Reversibility::Write,
        ),
        command(
            "note.put-then-fail",
            "Write a note, then refuse",
            A_NOTE_AND_TEXT,
            "Write",
            Reversibility::Write,
        ),
        command(
            "note.put-then-trap",
            "Write a note, then trap",
            A_NOTE_AND_TEXT,
            "Write",
            Reversibility::Write,
        ),
        command(
            "note.delete",
            "Delete a note",
            A_NOTE,
            "Delete",
            Reversibility::Write,
        ),
        // **A write decided as `Share`**, which the family pack holds to a
        // quorum on content that is not shared already: a compensation naming
        // it is registered, and decided when the undo dispatches it.
        command(
            "note.share",
            "Share a note",
            A_NOTE_AND_TEXT,
            "Share",
            Reversibility::Write,
        ),
        // **A read that writes anyway**, so a suite can show the kernel refuse
        // a write in a frame that may only read.
        command(
            "note.scribble",
            "Read a note, and try to write one",
            A_NOTE,
            "Read",
            Reversibility::Read,
        ),
        command(
            "executions.count",
            "Count the commands this instance has run",
            COUNTED,
            "Read",
            Reversibility::Read,
        ),
        command(
            "irreversible.do",
            "Do something that cannot be undone",
            A_NOTE,
            "Write",
            Reversibility::Irreversible,
        ),
        command(
            "relay.write",
            "Dispatch a command, changing state",
            RELAYED,
            "Write",
            Reversibility::Write,
        ),
        command(
            "relay.read",
            "Dispatch a command, only reading",
            RELAYED,
            "Read",
            Reversibility::Read,
        ),
        command(
            "echo",
            "Answer with the text given",
            A_NOTE_AND_TEXT,
            "Read",
            Reversibility::Read,
        ),
    ];
    table.extend(abuses(namespace));
    table.extend(blobs(namespace));
    table.extend(fetches(namespace));
    table
}

/// The commands over the rest of the world under `namespace`.
///
/// Two that publish an event — one changing state, inside a unit, and one
/// that only reads — two over timers, one that shows a surface, one that
/// reads who the call runs for, and one that writes to the plugin's own log.
fn around(namespace: &str) -> Vec<Command> {
    let command = |name: &str, title, args_schema, action, reversibility| {
        row(namespace, name, title, args_schema, action, reversibility)
    };
    vec![
        command(
            "event.publish",
            "Publish an event, changing state",
            A_PUBLISH,
            "Write",
            Reversibility::Write,
        ),
        command(
            "event.announce",
            "Publish an event, only reading",
            A_PUBLISH,
            "Read",
            Reversibility::Read,
        ),
        command(
            "timer.set",
            "Set a timer",
            A_TIMER,
            "Write",
            Reversibility::Write,
        ),
        command(
            "timer.remind",
            "Set a timer, only reading",
            A_TIMER,
            "Read",
            Reversibility::Read,
        ),
        command(
            "timer.cancel",
            "Cancel a timer",
            A_CANCEL,
            "Write",
            Reversibility::Write,
        ),
        command(
            "ui.show",
            "Show a surface",
            A_SURFACE,
            "Read",
            Reversibility::Read,
        ),
        command(
            "context.read",
            "Read who the call runs for",
            A_NOTE,
            "Read",
            Reversibility::Read,
        ),
        command(
            "log.write",
            "Write lines to the plugin's own log",
            A_LOG,
            "Read",
            Reversibility::Read,
        ),
    ]
}

/// The network commands under `namespace`, the units, and the rest of the
/// world.
///
/// A read that fetches as it is told, two commands that change state
/// elsewhere — one registering what sets it right — the three that write a
/// note in a unit they begin, and those of [`around`].
fn fetches(namespace: &str) -> Vec<Command> {
    let command = |name: &str, title, args_schema, action, reversibility| {
        row(namespace, name, title, args_schema, action, reversibility)
    };
    let mut fetches = vec![
        // **A read, so a suite can show what a read may and may not send**:
        // a `GET` unless told another method, and anything else it is told.
        command(
            "abuse.fetch",
            "Fetch a URL as told",
            A_FETCH,
            "Read",
            Reversibility::Read,
        ),
        command(
            "effect.post",
            "Send a request that changes something elsewhere",
            A_FETCH,
            "Write",
            Reversibility::Write,
        ),
        command(
            "compensated.post",
            "Send a request, and register what sets it right",
            A_COMPENSATED_FETCH,
            "Write",
            Reversibility::Write,
        ),
        command(
            "unit.commit",
            "Write a note in a unit, and commit it",
            A_NOTE_AND_TEXT,
            "Write",
            Reversibility::Write,
        ),
        command(
            "unit.abort",
            "Write a note in a unit, and abort it",
            A_NOTE_AND_TEXT,
            "Write",
            Reversibility::Write,
        ),
        command(
            "unit.drop",
            "Write a note in a unit, and let it go",
            A_NOTE_AND_TEXT,
            "Write",
            Reversibility::Write,
        ),
    ];
    fetches.extend(around(namespace));
    fetches
}

/// The blob commands under `namespace`: one that makes a blob, one that reads
/// one back, and one that asks where one is.
fn blobs(namespace: &str) -> Vec<Command> {
    let command = |name: &str, title, args_schema, action, reversibility| {
        row(namespace, name, title, args_schema, action, reversibility)
    };
    vec![
        command(
            "blob.put",
            "Write a blob, then stop as told",
            A_BLOB_PUT,
            "Write",
            Reversibility::Write,
        ),
        command(
            "blob.get",
            "Read a blob back whole",
            A_BLOB,
            "Read",
            Reversibility::Read,
        ),
        command(
            "blob.locate",
            "Ask where a blob is",
            A_BLOB,
            "Read",
            Reversibility::Read,
        ),
    ]
}

/// The rest of the table under `namespace`: the abuses — a trap, and those
/// that never return — the note written before one of them, the note written
/// in parts, and the pairs that write in the peer too.
fn abuses(namespace: &str) -> Vec<Command> {
    let command = |name: &str, title, args_schema, action, reversibility| {
        row(namespace, name, title, args_schema, action, reversibility)
    };
    vec![
        command(
            "abuse.trap",
            "Trap at once",
            A_NOTE,
            "Read",
            Reversibility::Read,
        ),
        command(
            "abuse.spin",
            "Run for ever, computing nothing",
            A_NOTE,
            "Read",
            Reversibility::Read,
        ),
        command(
            "abuse.burn",
            "Compute for ever",
            A_NOTE,
            "Read",
            Reversibility::Read,
        ),
        command(
            "abuse.grow",
            "Grow its memory until it is stopped",
            A_NOTE,
            "Read",
            Reversibility::Read,
        ),
        command(
            "abuse.reenter",
            "Dispatch into its own plugin while it runs",
            A_NOTE,
            "Read",
            Reversibility::Read,
        ),
        command(
            "note.put-then-abuse",
            "Write a note, then stop as told",
            A_NOTE_THEN_ABUSE,
            "Write",
            Reversibility::Write,
        ),
        // **Many keys in one step**, so a suite can show what the step's
        // undo will stage held to the unit's bound when the step writes.
        command(
            "note.put-many",
            "Write a note's numbered parts",
            A_NOTE_IN_PARTS,
            "Write",
            Reversibility::Write,
        ),
        command(
            "pair.put",
            "Write a note here and in the peer",
            A_NOTE_AND_TEXT,
            "Write",
            Reversibility::Write,
        ),
        command(
            "pair.put-then-trap",
            "Write a note here and in the peer, which then traps",
            A_NOTE_AND_TEXT,
            "Write",
            Reversibility::Write,
        ),
    ]
}

/// How a command of this plugin stops once it is told to: as the `abuse.*`
/// command of the same word does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Abuse {
    /// It panics, which a component answers by trapping.
    Trap,
    /// It runs for ever and computes nothing: a loop only the epoch stops.
    Spin,
    /// It computes for ever: a loop that burns its fuel as fast as it runs.
    Burn,
    /// It grows its memory without end, until its bound refuses the growth.
    Grow,
}

impl Abuse {
    /// Every abuse, in the order declared.
    pub const ALL: [Self; 4] = [Self::Trap, Self::Spin, Self::Burn, Self::Grow];

    /// The word an argument names it by.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Trap => "trap",
            Self::Spin => "spin",
            Self::Burn => "burn",
            Self::Grow => "grow",
        }
    }

    /// The abuse `word` names, if it names one.
    #[must_use]
    pub fn named(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|abuse| abuse.word() == word)
    }

    /// Stops the command `name` as this abuse says, which never returns.
    fn run(self, name: &str) -> ! {
        match self {
            Self::Trap => panic!("`{name}` trapped, as it was told to"),
            Self::Spin => spin(),
            Self::Burn => burn(),
            Self::Grow => grow(),
        }
    }
}

/// Runs for ever and computes nothing.
fn spin() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

/// Computes for ever, each step a step of a generator whose next value
/// depends on the last, so nothing can compute it ahead.
fn burn() -> ! {
    let mut state: u64 = 1;
    loop {
        state = core::hint::black_box(
            state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407),
        );
    }
}

/// Grows its memory a mebibyte at a time, holding every one of them.
fn grow() -> ! {
    let mut held: Vec<Vec<u8>> = Vec::new();
    loop {
        held.push(core::hint::black_box(vec![1; 1 << 20]));
    }
}

/// Calls every function of the world through `imports`, and answers what each
/// did as one canonical JSON object, keyed by [`PROBED`].
pub fn probe(imports: &mut impl Imports) -> String {
    nc_canon::Value::object(PROBED.iter().map(|function| {
        (
            *function,
            nc_canon::Value::text(imports.call(function).written()),
        )
    }))
    .to_canonical()
}

/// How many commands this instance has run, the one running included.
///
/// **A static, so a new instance starts again at nothing**: after a plugin is
/// killed and instantiated again, the count is that instance's, and before
/// any command has run it is zero — which is how a suite shows that a refused
/// dispatch never reached the plugin.
static EXECUTIONS: AtomicU64 = AtomicU64::new(0);

/// Runs the command `name` with `args`, reaching the world through `imports`.
///
/// # Errors
///
/// Returns [`Refused::NotFound`] for a name this plugin does not declare,
/// [`Refused::Invalid`] for arguments it cannot read and for
/// `note.put-then-fail` once it has written, and whatever the world refused a
/// command's call with.
///
/// # Panics
///
/// `note.put-then-trap` panics once it has written, `abuse.trap` before it
/// reaches the world at all, and `note.put-then-abuse` once it has written
/// when it is told to trap, which a component answers by trapping. The other
/// abuses never return at all, and only a bound the host holds them to stops
/// them.
pub fn execute(name: &str, args: &str, imports: &mut impl Imports) -> Result<String, Refused> {
    EXECUTIONS.fetch_add(1, Ordering::Relaxed);
    // The command after the namespace, whichever namespace it was declared
    // under: the kernel routes a plugin nothing but its own.
    let command = name.split_once('.').map_or("", |(_, command)| command);
    match command {
        "probe" => Ok(probe(imports)),
        "note.get" => {
            let note = Arguments::read(args)?.text("note")?;
            Ok(imports
                .get(&format!("note/{note}"))?
                .map_or_else(|| "null".to_owned(), |stored| stored.value))
        }
        "note.put" | "note.share" => noted(args, imports),
        "note.put-many" => in_parts(args, imports),
        "note.put-then-fail" => {
            noted(args, imports)?;
            Err(Refused::Invalid(format!(
                "`{name}` wrote its note and then refused, as it is written to"
            )))
        }
        "note.put-then-trap" => {
            noted(args, imports)?;
            panic!("`{name}` wrote its note and then trapped, as it is written to")
        }
        "note.delete" => {
            let note = Arguments::read(args)?.text("note")?;
            imports.delete(&format!("note/{note}"), None)?;
            Ok(answer([("deleted", nc_canon::Value::Bool(true))]))
        }
        "note.scribble" => {
            let note = Arguments::read(args)?.text("note")?;
            Ok(
                match imports.put(&format!("scribble/{note}"), "true", None) {
                    Ok(revision) => answer([("written", nc_canon::Value::text(revision))]),
                    Err(refused) => answer([(
                        "refused",
                        nc_canon::Value::text(Outcome::Refused(refused).written()),
                    )]),
                },
            )
        }
        "executions.count" => {
            let executions = EXECUTIONS.load(Ordering::Relaxed);
            Ok(answer([(
                "executions",
                nc_canon::Value::Integer(i64::try_from(executions).unwrap_or(i64::MAX)),
            )]))
        }
        "irreversible.do" => {
            let key = format!("done/{}", Arguments::read(args)?.text("note")?);
            let held = imports.get(&key)?;
            imports.put(&key, "true", held.as_ref().map(|held| held.rev.as_str()))?;
            Ok(answer([("done", nc_canon::Value::Bool(true))]))
        }
        "relay.write" | "relay.read" => relayed(args, imports),
        "echo" => Arguments::read(args)?.text("text"),
        "abuse.trap" => panic!("`{name}` trapped at once, as it is written to"),
        _ => abused(command, name, args, imports),
    }
}

/// Runs the commands that abuse the kernel or pair with the peer, and refuses
/// a name this plugin does not declare as not found.
fn abused(
    command: &str,
    name: &str,
    args: &str,
    imports: &mut impl Imports,
) -> Result<String, Refused> {
    match command {
        "abuse.spin" => spin(),
        "abuse.burn" => burn(),
        "abuse.grow" => grow(),
        "abuse.reenter" => reentered(name, args, imports),
        "note.put-then-abuse" => {
            let how = Arguments::read(args)?.text("abuse")?;
            let abuse = Abuse::named(&how).ok_or_else(|| {
                Refused::Invalid(format!("`{how}` is not a way `{name}` knows to stop"))
            })?;
            noted(args, imports)?;
            abuse.run(name)
        }
        "pair.put" => paired(args, "note.put", imports),
        "pair.put-then-trap" => paired(args, "note.put-then-trap", imports),
        "blob.put" => blob_put(name, args, imports),
        "blob.get" => {
            let mut arguments = Arguments::read(args)?;
            let hash = arguments.text("hash")?;
            let max = match arguments.whole("max") {
                Some(max) => u32::try_from(max)
                    .map_err(|_| Refused::Invalid(format!("`{max}` is not a read's size")))?,
                None => u32::MAX,
            };
            let blob = imports.get_blob(&hash, max)?;
            Ok(answer([
                (
                    "size",
                    nc_canon::Value::Integer(i64::try_from(blob.size).unwrap_or(i64::MAX)),
                ),
                (
                    "chunks",
                    nc_canon::Value::Array(
                        blob.chunks
                            .iter()
                            .map(|chunk| {
                                nc_canon::Value::Integer(i64::try_from(*chunk).unwrap_or(i64::MAX))
                            })
                            .collect(),
                    ),
                ),
                ("text", shown(blob.bytes)),
            ]))
        }
        "blob.locate" => {
            let hash = Arguments::read(args)?.text("hash")?;
            Ok(answer([(
                "availability",
                nc_canon::Value::text(imports.locate(&hash)?),
            )]))
        }
        "abuse.fetch" => fetched(args, "GET", imports),
        "effect.post" => fetched(args, "POST", imports),
        "compensated.post" => compensated(name, args, imports),
        "unit.commit" | "unit.abort" | "unit.drop" => in_a_unit(command, args, imports),
        _ => around_the_world(command, name, args, imports),
    }
}

/// Runs the commands over events, timers, surfaces, the context and the
/// log, and refuses a name this plugin does not declare as not found.
fn around_the_world(
    command: &str,
    name: &str,
    args: &str,
    imports: &mut impl Imports,
) -> Result<String, Refused> {
    match command {
        "event.publish" | "event.announce" => published(name, args, imports),
        "timer.set" | "timer.remind" => timed(name, args, imports),
        "timer.cancel" => {
            let mut arguments = Arguments::read(args)?;
            let timer = arguments.whole("timer").unwrap_or(0);
            let timer = u64::try_from(timer)
                .map_err(|_| Refused::Invalid(format!("`{timer}` is not a timer")))?;
            let cancelled = said(imports.cancel(timer));
            if arguments.maybe_text("then").is_some() {
                return Err(Refused::Invalid(format!(
                    "`{name}` cancelled the timer {timer} and then refused, as it was told to"
                )));
            }
            Ok(answer([("cancelled", cancelled)]))
        }
        "ui.show" => shown_in_a_surface(args, imports),
        "context.read" => Ok(match imports.context() {
            Ok(context) => answer([
                (
                    "actor_chain",
                    nc_canon::Value::Array(
                        context
                            .actor_chain
                            .into_iter()
                            .map(nc_canon::Value::text)
                            .collect(),
                    ),
                ),
                ("confirmed", nc_canon::Value::Bool(context.confirmed)),
                ("principal", nc_canon::Value::text(context.principal)),
            ]),
            Err(refused) => answer([(
                "refused",
                nc_canon::Value::text(Outcome::Refused(refused).written()),
            )]),
        }),
        "log.write" => {
            let mut arguments = Arguments::read(args)?;
            let text = arguments.text("text")?;
            let level = arguments
                .maybe_text("level")
                .unwrap_or_else(|| "info".to_owned());
            let times = arguments.whole("times").unwrap_or(1);
            for at in 0..times {
                imports.log(&level, &text.replace("{n}", &at.to_string()));
            }
            Ok(answer([("written", nc_canon::Value::Integer(times))]))
        }
        _ => Err(Refused::NotFound(format!(
            "`{name}` is not a command of the reference plugin"
        ))),
    }
}

/// `answered`, or what was refused as the world writes it, as text.
fn said<T>(answered: Result<T, Refused>) -> nc_canon::Value {
    nc_canon::Value::text(match answered {
        Ok(_) => Outcome::Answered.written(),
        Err(refused) => Outcome::Refused(refused).written(),
    })
}

/// Publishes the event the arguments say as many times as they say, and
/// answers how many were published and the refusal that stopped it — then
/// refuses itself when told to, having published.
fn published(name: &str, args: &str, imports: &mut impl Imports) -> Result<String, Refused> {
    let mut arguments = Arguments::read(args)?;
    let topic = arguments.text("topic")?;
    let data =
        match (arguments.whole("fill"), arguments.maybe_text("data")) {
            (Some(fill), _) => answer([(
                "fill",
                nc_canon::Value::text("x".repeat(usize::try_from(fill).map_err(|_| {
                    Refused::Invalid(format!("`{fill}` is not a number of bytes"))
                })?)),
            )]),
            (None, Some(data)) => data,
            (None, None) => "{}".to_owned(),
        };
    let times = arguments.whole("times").unwrap_or(1);
    let fails = arguments.maybe_text("then").is_some();
    let (mut published, mut refused) = (0_i64, nc_canon::Value::Null);
    for _ in 0..times {
        match imports.publish(&topic, &data) {
            Ok(()) => published += 1,
            Err(refusal) => {
                refused = nc_canon::Value::text(Outcome::Refused(refusal).written());
                break;
            }
        }
    }
    if fails {
        return Err(Refused::Invalid(format!(
            "`{name}` published {published} event(s) and then refused, as it was told to"
        )));
    }
    Ok(answer([
        ("published", nc_canon::Value::Integer(published)),
        ("refused", refused),
    ]))
}

/// Sets the timers the arguments say, and answers the identifiers set and
/// the refusal that stopped it — then refuses itself when told to, having set
/// them.
fn timed(name: &str, args: &str, imports: &mut impl Imports) -> Result<String, Refused> {
    let mut arguments = Arguments::read(args)?;
    let tag = arguments.text("tag")?;
    let ms = arguments.whole("ms").unwrap_or(0);
    let ms = u64::try_from(ms).map_err(|_| Refused::Invalid(format!("`{ms}` is no while")))?;
    let every = arguments.flag("every");
    let count = arguments.whole("times").unwrap_or(1);
    let fails = arguments.maybe_text("then").is_some();
    let (mut set, mut refused) = (Vec::new(), nc_canon::Value::Null);
    for _ in 0..count {
        match imports.set_timer(ms, every, &tag) {
            Ok(id) => set.push(nc_canon::Value::Integer(
                i64::try_from(id).unwrap_or(i64::MAX),
            )),
            Err(refusal) => {
                refused = nc_canon::Value::text(Outcome::Refused(refusal).written());
                break;
            }
        }
    }
    if fails {
        return Err(Refused::Invalid(format!(
            "`{name}` set {} timer(s) and then refused, as it was told to",
            set.len()
        )));
    }
    Ok(answer([
        ("refused", refused),
        ("timers", nc_canon::Value::Array(set)),
    ]))
}

/// Opens the surfaces the arguments say — or takes the one they name —
/// renders their tree into the last, and closes it when told to, answering
/// what each call answered.
fn shown_in_a_surface(args: &str, imports: &mut impl Imports) -> Result<String, Refused> {
    let mut arguments = Arguments::read(args)?;
    let title = arguments
        .maybe_text("title")
        .unwrap_or_else(|| "Notes".to_owned());
    let kind = arguments
        .maybe_text("kind")
        .unwrap_or_else(|| "panel".to_owned());
    let tree = arguments.maybe_text("tree");
    let then_close = arguments.flag("close");
    let times = arguments.whole("times").unwrap_or(1);
    let (surface, opened) = if let Some(surface) = arguments.maybe_text("surface") {
        (Some(surface), nc_canon::Value::Null)
    } else {
        let (mut surface, mut opened) = (None, said(Ok::<(), Refused>(())));
        for _ in 0..times {
            match imports.open_surface(&kind, &title) {
                Ok(id) => surface = Some(id),
                Err(refusal) => {
                    opened = said(Err::<(), Refused>(refusal));
                    break;
                }
            }
        }
        (surface, opened)
    };
    let rendered = match (&surface, tree) {
        (Some(surface), Some(tree)) => said(imports.render(surface, &tree)),
        _ => nc_canon::Value::Null,
    };
    let closed = match (&surface, then_close) {
        (Some(surface), true) => said(imports.close_surface(surface)),
        _ => nc_canon::Value::Null,
    };
    Ok(answer([
        ("closed", closed),
        ("opened", opened),
        ("rendered", rendered),
        (
            "surface",
            surface.map_or(nc_canon::Value::Null, nc_canon::Value::text),
        ),
    ]))
}

/// What a callback says it runs for: the principal and whether a person
/// confirmed it, or the refusal it was told.
fn running_for(imports: &mut impl Imports) -> String {
    match imports.context() {
        Ok(context) => format!("for {}, confirmed {}", context.principal, context.confirmed),
        Err(refused) => format!(
            "for nobody it was told: {}",
            Outcome::Refused(refused).written()
        ),
    }
}

/// What the plugin does when a timer it set fires with `tag`, under the
/// namespace `namespace`.
///
/// It writes `timer <tag> fired for <principal>, confirmed <bool>` to its log,
/// and then does what the tag says: `put:<into>:<note>` dispatches the
/// `note.put` of the namespace `into` for the note, writing `fired`;
/// `group:<into>:<note>` begins a unit, dispatches `note.put` and
/// `note.put-many` of `into` in it, and commits it; `drop:<into>:<note>` begins
/// one, dispatches `note.put` of `into`, and lets it go;
/// `trapped:<into>:<note>` begins one, dispatches `note.put` and `blob.put` of
/// `into` in it, and panics with it open; `fetch:<method>` begins one, sends a
/// `<method>` of `https://example.com/fired` in it and commits it — which a
/// callback is refused for anything but a `GET` or a `HEAD`, whatever unit it
/// holds; `again:<ms>` sets a
/// timer of `ms` tagged `log`, and `cancel:<id>` cancels the timer `id`, each
/// from a callback that holds no unit; `publish` publishes
/// `<namespace>.fired`; `trap` panics; anything else does nothing more. What
/// each dispatch answered is a line of the log too. **The namespace is
/// another plugin's**, because a callback runs in the plugin it calls, and a
/// command of that plugin dispatched from it is a dispatch into itself.
///
/// # Panics
///
/// When the tag is `trap` or `trapped:<into>:<note>`, which a component
/// answers by trapping.
pub fn on_timer(namespace: &str, tag: &str, imports: &mut impl Imports) {
    let running = running_for(imports);
    imports.log("info", &format!("timer {tag} fired {running}"));
    let note = |text: &str, note: &str| {
        answer([
            ("note", nc_canon::Value::text(note)),
            ("text", nc_canon::Value::text(text)),
        ])
    };
    let mut parts = tag.splitn(3, ':');
    let (what, into, which) = (parts.next(), parts.next(), parts.next());
    let put = format!("{}.note.put", into.unwrap_or(namespace));
    match (what, which) {
        (Some("put"), Some(which)) => {
            let answered = imports.dispatch(&put, &note("fired", which));
            imports.log("info", &format!("put {}", said(answered).to_canonical()));
        }
        (Some("group"), Some(which)) => {
            let unit = imports.begin("Fired in a group");
            let first = imports.dispatch(&put, &note("grouped", which));
            let many = answer([
                ("note", nc_canon::Value::text(which)),
                ("parts", nc_canon::Value::Integer(2)),
                ("text", nc_canon::Value::text("grouped")),
            ]);
            let second = imports.dispatch(&format!("{put}-many"), &many);
            let committed = unit.and_then(|unit| imports.commit(unit));
            imports.log(
                "info",
                &format!(
                    "group {} {} {}",
                    said(first).to_canonical(),
                    said(second).to_canonical(),
                    said(committed).to_canonical()
                ),
            );
        }
        (Some("fetch"), _) => fetched_in_a_unit(into.unwrap_or("GET"), imports),
        (Some("again"), _) => {
            let ms = into.and_then(|ms| ms.parse::<u64>().ok()).unwrap_or(1);
            let set = imports.set_timer(ms, false, "log");
            imports.log("info", &format!("again {}", said(set).to_canonical()));
        }
        (Some("cancel"), _) => {
            let id = into.and_then(|id| id.parse::<u64>().ok()).unwrap_or(0);
            let cancelled = imports.cancel(id);
            imports.log(
                "info",
                &format!("cancel {}", said(cancelled).to_canonical()),
            );
        }
        (Some("drop"), Some(which)) => {
            let unit = imports.begin("Fired and let go");
            let answered = imports.dispatch(&put, &note("dropped", which));
            if let Ok(unit) = unit {
                imports.release(unit);
            }
            imports.log("info", &format!("drop {}", said(answered).to_canonical()));
        }
        (Some("trapped"), Some(which)) => {
            let unit = imports.begin("Fired and trapped");
            let put_answered = imports.dispatch(&put, &note("trapped", which));
            let blob = answer([
                ("note", nc_canon::Value::text(which)),
                ("text", nc_canon::Value::text("trapped")),
            ]);
            let blob_answered =
                imports.dispatch(&format!("{}.blob.put", into.unwrap_or(namespace)), &blob);
            imports.log(
                "info",
                &format!(
                    "trapped {} {} {}",
                    said(unit).to_canonical(),
                    said(put_answered).to_canonical(),
                    said(blob_answered).to_canonical()
                ),
            );
            panic!("the timer `{tag}` trapped in the unit it began, as it was told to")
        }
        _ if tag == "publish" => {
            let topic = format!("{namespace}.fired");
            let published = imports.publish(&topic, &answer([("tag", nc_canon::Value::text(tag))]));
            imports.log(
                "info",
                &format!("publish {}", said(published).to_canonical()),
            );
        }
        _ if tag == "trap" => panic!("the timer `{tag}` trapped, as it was told to"),
        _ => {}
    }
}

/// Begins a unit, sends a `method` of `https://example.com/fired` in it and
/// commits it, writing to the log what the request and the commit answered.
fn fetched_in_a_unit(method: &str, imports: &mut impl Imports) {
    let unit = imports.begin("Fired and fetched");
    let request = Fetch {
        method: method.to_owned(),
        url: "https://example.com/fired".to_owned(),
        headers: Vec::new(),
        body: None,
    };
    let fetched = imports.fetch(&request);
    let committed = unit.and_then(|unit| imports.commit(unit));
    imports.log(
        "info",
        &format!(
            "fetch {} {}",
            said(fetched).to_canonical(),
            said(committed).to_canonical()
        ),
    );
}

/// What the plugin does when it hears `data` under `topic`, under the
/// namespace `namespace`.
///
/// It writes `heard <topic> <data> for <principal>, confirmed <bool>` to its
/// log, and then does what the data's `then` says: `write` dispatches the
/// `note.put` of the publisher's namespace — the topic's first word — for the
/// note `photo`, which a callback that only reads is refused; `defer` sets a
/// timer of a millisecond tagged `put:<publisher>:photo`, which puts that
/// dispatch off until the timer fires; `relay` publishes
/// `<namespace>.relayed`; `fetch` sends a `GET` of `https://example.org/heard`,
/// which a plugin not granted `net` is refused; `trap` panics; anything else
/// does nothing more. What it was answered is a line of the log too.
///
/// # Panics
///
/// When the data says `"then":"trap"`, which a component answers by
/// trapping.
pub fn on_event(namespace: &str, topic: &str, data: &str, imports: &mut impl Imports) {
    let running = running_for(imports);
    imports.log("info", &format!("heard {topic} {data} {running}"));
    let then = nc_canon::parse(data)
        .ok()
        .and_then(nc_canon::Value::into_object)
        .and_then(|mut fields| fields.remove("then"))
        .and_then(nc_canon::Value::into_text);
    match then.as_deref() {
        Some("write") => {
            let args = answer([
                ("note", nc_canon::Value::text("photo")),
                ("text", nc_canon::Value::text("heard")),
            ]);
            let publisher = topic.split('.').next().unwrap_or(topic);
            let answered = imports.dispatch(&format!("{publisher}.note.put"), &args);
            imports.log("info", &format!("write {}", said(answered).to_canonical()));
        }
        Some("defer") => {
            let publisher = topic.split('.').next().unwrap_or(topic);
            let set = imports.set_timer(1, false, &format!("put:{publisher}:photo"));
            imports.log("info", &format!("defer {}", said(set).to_canonical()));
        }
        Some("relay") => {
            let relayed = answer([("from", nc_canon::Value::text(topic))]);
            let published = imports.publish(&format!("{namespace}.relayed"), &relayed);
            imports.log("info", &format!("relay {}", said(published).to_canonical()));
        }
        Some("fetch") => {
            let request = Fetch {
                method: "GET".to_owned(),
                url: "https://example.org/heard".to_owned(),
                headers: Vec::new(),
                body: None,
            };
            let fetched = imports.fetch(&request);
            imports.log("info", &format!("fetch {}", said(fetched).to_canonical()));
        }
        Some("trap") => panic!("hearing `{topic}` trapped, as it was told to"),
        _ => {}
    }
}

/// How many bytes of a blob read back an answer repeats as text: more would
/// make an answer past what a payload may be.
const SHOWN: usize = 4096;

/// A blob's bytes as the text they are, when they are text and short enough
/// to answer, and `null` otherwise.
fn shown(bytes: Vec<u8>) -> nc_canon::Value {
    if bytes.len() > SHOWN {
        return nc_canon::Value::Null;
    }
    String::from_utf8(bytes).map_or(nc_canon::Value::Null, nc_canon::Value::text)
}

/// Sends the request the arguments say — `method` unless they name one — and
/// answers what came back, or what the kernel refused it with.
///
/// The answer is the status, how many headers and body bytes there were, the
/// `location` when there was one, and the body as text when it is short
/// enough to repeat; a refusal is answered as `{"refused":"<case>: <text>"}`
/// rather than carried, so a suite reads the kernel's own sentence.
fn fetched(args: &str, method: &str, imports: &mut impl Imports) -> Result<String, Refused> {
    let mut arguments = Arguments::read(args)?;
    let url = arguments.text("url")?;
    let method = arguments
        .maybe_text("method")
        .unwrap_or_else(|| method.to_owned());
    let headers = arguments.headers()?;
    let body = match (arguments.maybe_text("body"), arguments.whole("fill")) {
        (Some(text), _) => Some(text.into_bytes()),
        (None, Some(fill)) => Some(vec![
            b'x';
            usize::try_from(fill).map_err(|_| {
                Refused::Invalid(format!("`{fill}` is not a number of bytes"))
            })?
        ]),
        (None, None) => None,
    };
    let request = Fetch {
        method,
        url,
        headers,
        body,
    };
    Ok(match imports.fetch(&request) {
        Ok(fetched) => {
            let location = fetched
                .headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("location"))
                .map_or(nc_canon::Value::Null, |(_, value)| {
                    nc_canon::Value::text(value.clone())
                });
            let count = |n: usize| nc_canon::Value::Integer(i64::try_from(n).unwrap_or(i64::MAX));
            answer([
                ("bytes", count(fetched.body.len())),
                ("headers", count(fetched.headers.len())),
                ("location", location),
                (
                    "status",
                    nc_canon::Value::Integer(i64::from(fetched.status)),
                ),
                ("text", shown(fetched.body)),
            ])
        }
        Err(refused) => answer([(
            "refused",
            nc_canon::Value::text(Outcome::Refused(refused).written()),
        )]),
    })
}

/// Begins a unit, registers the compensation the arguments name — or this
/// plugin's own `note.put` writing `compensated` to the note, and the same
/// command again with `then` when it is given — sends the `POST` they say,
/// and commits the unit.
///
/// It answers whether the compensation was registered — the kernel's refusal,
/// as the world writes it, when it was not — and what the request was
/// answered, and commits either way: the request is sent.
fn compensated(name: &str, args: &str, imports: &mut impl Imports) -> Result<String, Refused> {
    let mut arguments = Arguments::read(args)?;
    let note = arguments.text("note")?;
    let namespace = name.split('.').next().unwrap_or(name);
    let compensation = arguments
        .maybe_text("compensate")
        .unwrap_or_else(|| format!("{namespace}.note.put"));
    let with = arguments.maybe_text("with").unwrap_or_else(|| {
        answer([
            ("note", nc_canon::Value::text(note)),
            ("text", nc_canon::Value::text("compensated")),
        ])
    });
    let then = arguments.maybe_text("then");
    let unit = imports.begin("Send a request, and what sets it right")?;
    let mut registered = imports.compensate_with(unit, &compensation, &with);
    if let (Ok(()), Some(then)) = (&registered, then) {
        registered = imports.compensate_with(unit, &compensation, &then);
    }
    let registered = match registered {
        Ok(()) => "registered".to_owned(),
        Err(refused) => Outcome::Refused(refused).written(),
    };
    let sent = fetched(args, "POST", imports)?;
    imports.commit(unit)?;
    Ok(answer([
        ("compensation", nc_canon::Value::text(registered)),
        ("sent", nc_canon::Value::text(sent)),
    ]))
}

/// Begins a unit, writes the note the arguments say in it, and ends the unit
/// as `command` says, answering the note's revision.
///
/// `unit.commit` commits it, `unit.abort` aborts it, and `unit.drop` lets it
/// go.
fn in_a_unit(command: &str, args: &str, imports: &mut impl Imports) -> Result<String, Refused> {
    let unit = imports.begin("Write a note")?;
    let written = noted(args, imports)?;
    match command {
        "unit.commit" => imports.commit(unit)?,
        "unit.abort" => imports.abort(unit),
        _ => imports.release(unit),
    }
    Ok(written)
}

/// How `blob.put` ends once it has made its blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Then {
    /// It refuses, as `note.put-then-fail` does.
    Fail,
    /// It stops as the abuse of that word does.
    Abuse(Abuse),
}

/// Makes the blob the arguments say — their text written as many times over
/// as they say, once when they say nothing — answers its hash, and where it
/// is when told to ask, and then refuses or stops when told to, an end it was
/// told that no command knows being refused before anything is made.
fn blob_put(name: &str, args: &str, imports: &mut impl Imports) -> Result<String, Refused> {
    let mut arguments = Arguments::read(args)?;
    let text = arguments.text("text")?;
    let times = match arguments.whole("times") {
        Some(times) => u32::try_from(times)
            .map_err(|_| Refused::Invalid(format!("`{times}` is not a number of writes")))?,
        None => 1,
    };
    let then = match arguments.maybe_text("then") {
        None => None,
        Some(word) if word == "fail" => Some(Then::Fail),
        Some(word) => Some(Then::Abuse(Abuse::named(&word).ok_or_else(|| {
            Refused::Invalid(format!("`{word}` is not a way `{name}` knows to end"))
        })?)),
    };
    let located = arguments.flag("located");
    let hash = imports.put_blob(text.as_bytes(), times)?;
    match then {
        None if located => Ok(answer([
            (
                "availability",
                nc_canon::Value::text(imports.locate(&hash)?),
            ),
            ("hash", nc_canon::Value::text(hash)),
        ])),
        None => Ok(answer([("hash", nc_canon::Value::text(hash))])),
        Some(Then::Fail) => Err(Refused::Invalid(format!(
            "`{name}` made the blob `{hash}` and then refused, as it was told to"
        ))),
        Some(Then::Abuse(abuse)) => abuse.run(name),
    }
}

/// Dispatches into this plugin while it runs — the count of its own
/// namespace's executions — and answers what it was told.
fn reentered(name: &str, args: &str, imports: &mut impl Imports) -> Result<String, Refused> {
    let note = Arguments::read(args)?.text("note")?;
    let namespace = name
        .split_once('.')
        .map_or(name, |(namespace, _)| namespace);
    let counted = answer([
        ("at_most", nc_canon::Value::Integer(1)),
        ("note", nc_canon::Value::text(note)),
    ]);
    Ok(
        match imports.dispatch(&format!("{namespace}.executions.count"), &counted) {
            Ok(said) => answer([("answered", nc_canon::Value::text(said))]),
            Err(refused) => answer([(
                "refused",
                nc_canon::Value::text(Outcome::Refused(refused).written()),
            )]),
        },
    )
}

/// Writes the note here, then dispatches `peer_command` of the [`PEER`].
///
/// It dispatches with the same note and text, and answers what each said —
/// the peer's refusal included, which it carries no further.
fn paired(args: &str, peer_command: &str, imports: &mut impl Imports) -> Result<String, Refused> {
    let here = noted(args, imports)?;
    let mut arguments = Arguments::read(args)?;
    let relayed = answer([
        ("note", nc_canon::Value::text(arguments.text("note")?)),
        ("text", nc_canon::Value::text(arguments.text("text")?)),
    ]);
    let peer = match imports.dispatch(&format!("{PEER}.{peer_command}"), &relayed) {
        Ok(said) => said,
        Err(refused) => Outcome::Refused(refused).written(),
    };
    Ok(answer([
        ("here", nc_canon::Value::text(here)),
        ("peer", nc_canon::Value::text(peer)),
    ]))
}

/// A command's answer: a canonical JSON object of `fields`.
fn answer<const N: usize>(fields: [(&str, nc_canon::Value); N]) -> String {
    nc_canon::Value::object(fields).to_canonical()
}

/// Writes the note the arguments name with the text they give, over whatever
/// revision it holds, and answers that revision.
fn noted(args: &str, imports: &mut impl Imports) -> Result<String, Refused> {
    let mut arguments = Arguments::read(args)?;
    let key = format!("note/{}", arguments.text("note")?);
    let value = answer([("text", nc_canon::Value::text(arguments.text("text")?))]);
    let held = imports.get(&key)?;
    let revision = imports.put(&key, &value, held.as_ref().map(|held| held.rev.as_str()))?;
    Ok(answer([("rev", nc_canon::Value::text(revision))]))
}

/// Writes the note's text to each of its numbered parts, `note/<note>/0`
/// on, over what each holds, and answers how many it wrote: at the first
/// write refused it stops, and answers the refusal.
fn in_parts(args: &str, imports: &mut impl Imports) -> Result<String, Refused> {
    let mut arguments = Arguments::read(args)?;
    let note = arguments.text("note")?;
    let value = answer([("text", nc_canon::Value::text(arguments.text("text")?))]);
    let parts = arguments.whole("parts").unwrap_or(1);
    for part in 0..parts {
        let key = format!("note/{note}/{part}");
        let held = imports.get(&key)?;
        imports.put(&key, &value, held.as_ref().map(|held| held.rev.as_str()))?;
    }
    Ok(answer([("written", nc_canon::Value::Integer(parts))]))
}

/// Dispatches the command the arguments name, with the arguments they give.
///
/// As many times as they say or once, stopping at the first refusal unless
/// told to go on `through` them — a plugin that asks again whatever it is
/// told, which is how a suite shows the kernel stop one — and answers how
/// many were answered, the last answer and the last refusal.
fn relayed(args: &str, imports: &mut impl Imports) -> Result<String, Refused> {
    let mut arguments = Arguments::read(args)?;
    let (name, relayed) = (arguments.text("name")?, arguments.text("args")?);
    let times = arguments.whole("times").unwrap_or(1);
    let through = arguments.flag("through");
    let (mut answered, mut last, mut refused) =
        (0_i64, nc_canon::Value::Null, nc_canon::Value::Null);
    for _ in 0..times {
        match imports.dispatch(&name, &relayed) {
            Ok(said) => {
                answered += 1;
                last = nc_canon::Value::text(said);
            }
            Err(refusal) => {
                refused = nc_canon::Value::text(Outcome::Refused(refusal).written());
                if !through {
                    break;
                }
            }
        }
    }
    Ok(answer([
        ("answered", nc_canon::Value::Integer(answered)),
        ("last", last),
        ("refused", refused),
    ]))
}

/// A command's arguments, read as the object the kernel validated.
struct Arguments {
    fields: std::collections::BTreeMap<String, nc_canon::Value>,
}

impl Arguments {
    /// The arguments `args` are.
    fn read(args: &str) -> Result<Self, Refused> {
        let fields = nc_canon::parse(args)
            .map_err(|error| Refused::Invalid(format!("the arguments are not JSON: {error}")))?
            .into_object()
            .ok_or_else(|| Refused::Invalid("the arguments are not an object".to_owned()))?;
        Ok(Self { fields })
    }

    /// The text `name` holds.
    fn text(&mut self, name: &str) -> Result<String, Refused> {
        self.fields
            .remove(name)
            .and_then(nc_canon::Value::into_text)
            .ok_or_else(|| Refused::Invalid(format!("`{name}` is not text")))
    }

    /// The text `name` holds, when it holds some.
    fn maybe_text(&mut self, name: &str) -> Option<String> {
        self.fields
            .remove(name)
            .and_then(nc_canon::Value::into_text)
    }

    /// The whole number `name` holds, when it holds one.
    fn whole(&mut self, name: &str) -> Option<i64> {
        match self.fields.remove(name) {
            Some(nc_canon::Value::Integer(number)) => Some(number),
            _ => None,
        }
    }

    /// Whether `name` holds `true`.
    fn flag(&mut self, name: &str) -> bool {
        matches!(self.fields.remove(name), Some(nc_canon::Value::Bool(true)))
    }

    /// The headers `headers` holds, each `{"name":…,"value":…}`, in order:
    /// none when it holds nothing.
    fn headers(&mut self) -> Result<Vec<(String, String)>, Refused> {
        let Some(listed) = self.fields.remove("headers") else {
            return Ok(Vec::new());
        };
        let invalid = || Refused::Invalid("`headers` is not a list of names and values".to_owned());
        listed
            .into_array()
            .ok_or_else(invalid)?
            .into_iter()
            .map(|header| {
                let mut fields = header.into_object().ok_or_else(invalid)?;
                let mut text = |name: &str| {
                    fields
                        .remove(name)
                        .and_then(nc_canon::Value::into_text)
                        .ok_or_else(invalid)
                };
                Ok((text("name")?, text("value")?))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        ACTIVATION, Abuse, Activation, Context, Fetch, Fetched, Imports, NAMESPACE, Outcome, PEER,
        PROBED, Refused, Reversibility, Stored, Variant, commands, commands_for, execute, on_event,
        on_timer, probe,
    };

    /// A world that answers each function with what `answer` says.
    ///
    /// It keeps a store in memory, and remembers what it was asked and what
    /// was dispatched.
    struct Scripted {
        asked: Vec<String>,
        answer: fn(&str) -> Outcome,
        store: BTreeMap<String, Stored>,
        written: u32,
        dispatched: Vec<(String, String)>,
        refusing: Option<Refused>,
        /// What every dispatch is refused with, from the first.
        refusing_dispatch: Option<Refused>,
        /// The blobs, by a name the script gives each: its bytes.
        blobs: BTreeMap<String, Vec<u8>>,
        /// Every request sent, in order.
        fetched: Vec<Fetch>,
        /// What every request is answered, when the script says.
        fetch_answer: Option<Result<Fetched, Refused>>,
        /// Every call on a unit, as its function and where the unit is held.
        units: Vec<(&'static str, usize)>,
        /// What every compensation registered is refused with, when the
        /// script says.
        refusing_compensation: Option<Refused>,
        /// Every line written to the log, as its level and its text.
        logged: Vec<(String, String)>,
        /// Every call over events, timers and surfaces, as the function and
        /// what it was handed.
        world: Vec<(&'static str, String)>,
        /// How many of those calls are answered before every later one is
        /// refused as `unavailable`.
        world_answers: usize,
        /// What `context` answers.
        context: Result<Context, Refused>,
    }

    impl Scripted {
        /// A world answering every function with `answer`, holding nothing.
        fn answering(answer: fn(&str) -> Outcome) -> Self {
            Self {
                asked: Vec::new(),
                answer,
                store: BTreeMap::new(),
                written: 0,
                dispatched: Vec::new(),
                refusing: None,
                refusing_dispatch: None,
                blobs: BTreeMap::new(),
                fetched: Vec::new(),
                fetch_answer: None,
                units: Vec::new(),
                refusing_compensation: None,
                logged: Vec::new(),
                world: Vec::new(),
                world_answers: usize::MAX,
                context: Ok(Context {
                    principal: "ana-id".to_owned(),
                    actor_chain: Vec::new(),
                    confirmed: false,
                }),
            }
        }

        /// Records a call over events, timers or surfaces, refused once the
        /// script's answers are spent.
        fn worldly(&mut self, function: &'static str, handed: String) -> Result<(), Refused> {
            self.world.push((function, handed));
            if self.world.len() > self.world_answers {
                return Err(Refused::Unavailable(format!("{function} refused")));
            }
            Ok(())
        }
    }

    impl Imports for Scripted {
        fn call(&mut self, function: &str) -> Outcome {
            self.asked.push(function.to_owned());
            (self.answer)(function)
        }

        fn get(&mut self, key: &str) -> Result<Option<Stored>, Refused> {
            Ok(self.store.get(key).cloned())
        }

        fn put(
            &mut self,
            key: &str,
            value: &str,
            expected: Option<&str>,
        ) -> Result<String, Refused> {
            if let Some(refused) = self.refusing.clone() {
                return Err(refused);
            }
            if self.store.get(key).map(|held| held.rev.as_str()) != expected {
                return Err(Refused::Conflict(format!("{key} holds another revision")));
            }
            self.written += 1;
            let rev = format!("r{}", self.written);
            self.store.insert(
                key.to_owned(),
                Stored {
                    value: value.to_owned(),
                    rev: rev.clone(),
                },
            );
            Ok(rev)
        }

        fn delete(&mut self, key: &str, _expected: Option<&str>) -> Result<(), Refused> {
            self.store.remove(key);
            Ok(())
        }

        fn dispatch(&mut self, name: &str, args: &str) -> Result<String, Refused> {
            self.dispatched.push((name.to_owned(), args.to_owned()));
            if let Some(refused) = &self.refusing_dispatch {
                return Err(refused.clone());
            }
            match &self.refusing {
                Some(refused) if self.dispatched.len() > 1 => Err(refused.clone()),
                _ => Ok(r#"{"done":true}"#.to_owned()),
            }
        }

        fn locate(&mut self, hash: &str) -> Result<String, Refused> {
            Ok(if self.blobs.contains_key(hash) {
                "local"
            } else {
                "unavailable"
            }
            .to_owned())
        }

        fn put_blob(&mut self, piece: &[u8], times: u32) -> Result<String, Refused> {
            if let Some(refused) = self.refusing.clone() {
                return Err(refused);
            }
            let bytes: Vec<u8> = (0..times).flat_map(|_| piece.iter().copied()).collect();
            let name = format!("blob{}", self.blobs.len());
            self.blobs.insert(name.clone(), bytes);
            Ok(name)
        }

        fn get_blob(&mut self, hash: &str, max: u32) -> Result<super::Blob, Refused> {
            let bytes = self
                .blobs
                .get(hash)
                .cloned()
                .ok_or_else(|| Refused::Unavailable(format!("no {hash}")))?;
            let step = usize::try_from(max.clamp(1, 3)).expect("fits");
            Ok(super::Blob {
                size: u64::try_from(bytes.len()).expect("fits"),
                chunks: bytes
                    .chunks(step)
                    .map(|chunk| u64::try_from(chunk.len()).expect("fits"))
                    .collect(),
                bytes,
            })
        }

        fn fetch(&mut self, request: &Fetch) -> Result<Fetched, Refused> {
            self.fetched.push(request.clone());
            self.fetch_answer.clone().unwrap_or_else(|| {
                Ok(Fetched {
                    status: 200,
                    headers: Vec::new(),
                    body: Vec::new(),
                })
            })
        }

        fn begin(&mut self, _label: &str) -> Result<usize, Refused> {
            let unit = self
                .units
                .iter()
                .filter(|(called, _)| *called == "begin")
                .count();
            self.units.push(("begin", unit));
            Ok(unit)
        }

        fn compensate_with(&mut self, unit: usize, name: &str, args: &str) -> Result<(), Refused> {
            self.units.push(("compensate-with", unit));
            self.dispatched.push((name.to_owned(), args.to_owned()));
            self.refusing_compensation.clone().map_or(Ok(()), Err)
        }

        fn commit(&mut self, unit: usize) -> Result<(), Refused> {
            self.units.push(("commit", unit));
            Ok(())
        }

        fn abort(&mut self, unit: usize) {
            self.units.push(("abort", unit));
        }

        fn release(&mut self, unit: usize) {
            self.units.push(("release", unit));
        }

        fn log(&mut self, level: &str, text: &str) {
            self.logged.push((level.to_owned(), text.to_owned()));
        }

        fn publish(&mut self, topic: &str, data: &str) -> Result<(), Refused> {
            self.worldly("publish", format!("{topic} {data}"))
        }

        fn set_timer(&mut self, ms: u64, every: bool, tag: &str) -> Result<u64, Refused> {
            self.worldly("set", format!("{ms} {every} {tag}"))?;
            Ok(u64::try_from(self.world.len()).expect("fits"))
        }

        fn cancel(&mut self, timer: u64) -> Result<(), Refused> {
            self.worldly("cancel", timer.to_string())
        }

        fn open_surface(&mut self, kind: &str, title: &str) -> Result<String, Refused> {
            self.worldly("open", format!("{kind} {title}"))?;
            Ok(format!("surface-{}", self.world.len()))
        }

        fn close_surface(&mut self, surface: &str) -> Result<(), Refused> {
            self.worldly("close", surface.to_owned())
        }

        fn render(&mut self, surface: &str, tree: &str) -> Result<(), Refused> {
            self.worldly("render", format!("{surface} {tree}"))
        }

        fn context(&mut self) -> Result<Context, Refused> {
            self.context.clone()
        }
    }

    /// Every command the plugin declares as written, in the order declared.
    const THE_TABLE: [&str; 39] = [
        "reference.note.get",
        "reference.probe",
        "reference.note.put",
        "reference.note.put-then-fail",
        "reference.note.put-then-trap",
        "reference.note.delete",
        "reference.note.share",
        "reference.note.scribble",
        "reference.executions.count",
        "reference.irreversible.do",
        "reference.relay.write",
        "reference.relay.read",
        "reference.echo",
        "reference.abuse.trap",
        "reference.abuse.spin",
        "reference.abuse.burn",
        "reference.abuse.grow",
        "reference.abuse.reenter",
        "reference.note.put-then-abuse",
        "reference.note.put-many",
        "reference.pair.put",
        "reference.pair.put-then-trap",
        "reference.blob.put",
        "reference.blob.get",
        "reference.blob.locate",
        "reference.abuse.fetch",
        "reference.effect.post",
        "reference.compensated.post",
        "reference.unit.commit",
        "reference.unit.abort",
        "reference.unit.drop",
        "reference.event.publish",
        "reference.event.announce",
        "reference.timer.set",
        "reference.timer.remind",
        "reference.timer.cancel",
        "reference.ui.show",
        "reference.context.read",
        "reference.log.write",
    ];

    #[test]
    fn the_plugin_declares_its_table_each_command_mutating_exactly_when_it_is_not_a_read() {
        let declared = commands();
        let names: Vec<&str> = declared
            .iter()
            .map(|command| command.name.as_str())
            .collect();
        assert_eq!(names, THE_TABLE);
        for command in &declared {
            assert_eq!(
                command.mutating,
                command.reversibility != Reversibility::Read,
                "`{}`",
                command.name
            );
            assert_eq!(
                command.resource_selector, "argument:note",
                "`{}`",
                command.name
            );
        }
        let of = |name: &str| {
            declared
                .iter()
                .find(|command| command.name == format!("reference.{name}"))
                .map(|command| (command.action, command.reversibility))
        };
        assert_eq!(of("note.get"), Some(("Read", Reversibility::Read)));
        assert_eq!(of("probe"), Some(("Write", Reversibility::Write)));
        assert_eq!(of("note.delete"), Some(("Delete", Reversibility::Write)));
        assert_eq!(of("note.share"), Some(("Share", Reversibility::Write)));
        assert_eq!(
            of("irreversible.do"),
            Some(("Write", Reversibility::Irreversible))
        );
        assert_eq!(of("relay.read"), Some(("Read", Reversibility::Read)));
        for abuse in ["abuse.spin", "abuse.burn", "abuse.grow", "abuse.reenter"] {
            assert_eq!(of(abuse), Some(("Read", Reversibility::Read)), "`{abuse}`");
        }
        for reading in ["blob.get", "blob.locate", "abuse.fetch", "timer.remind"] {
            assert_eq!(
                of(reading),
                Some(("Read", Reversibility::Read)),
                "`{reading}`"
            );
        }
        for writing in [
            "note.put-many",
            "note.put-then-abuse",
            "pair.put",
            "pair.put-then-trap",
            "blob.put",
            "effect.post",
            "compensated.post",
            "unit.commit",
            "unit.abort",
            "unit.drop",
        ] {
            assert_eq!(
                of(writing),
                Some(("Write", Reversibility::Write)),
                "`{writing}`"
            );
        }
    }

    /// A note written in parts is its text in every numbered part, each
    /// written over what it held, and a write refused stops the rest.
    #[test]
    fn a_note_in_parts_is_written_to_every_part_over_what_each_held() {
        let mut world = Scripted::answering(|_| Outcome::Answered);
        world
            .put("note/photo/1", "{}", None)
            .expect("a part held before");
        let written = execute(
            "reference.note.put-many",
            r#"{"note":"photo","parts":3,"text":"short"}"#,
            &mut world,
        );
        assert_eq!(written, Ok(r#"{"written":3}"#.to_owned()));
        let parts: Vec<(&str, &str)> = world
            .store
            .iter()
            .map(|(key, held)| (key.as_str(), held.value.as_str()))
            .collect();
        assert_eq!(
            parts,
            [
                ("note/photo/0", r#"{"text":"short"}"#),
                ("note/photo/1", r#"{"text":"short"}"#),
                ("note/photo/2", r#"{"text":"short"}"#),
            ]
        );
        let mut refusing = Scripted::answering(|_| Outcome::Answered);
        refusing.refusing = Some(Refused::Unavailable("full".to_owned()));
        assert_eq!(
            execute(
                "reference.note.put-many",
                r#"{"note":"photo","parts":3,"text":"short"}"#,
                &mut refusing
            ),
            Err(Refused::Unavailable("full".to_owned()))
        );
        assert!(refusing.store.is_empty(), "{:?}", refusing.store);
    }

    /// A note written in a unit is written inside it, and the unit is then
    /// committed, aborted or let go as the command says.
    #[test]
    fn a_note_in_a_unit_is_written_in_it_and_the_unit_ended_as_the_command_says() {
        for (command, ended) in [
            ("unit.commit", "commit"),
            ("unit.abort", "abort"),
            ("unit.drop", "release"),
        ] {
            let mut world = Scripted::answering(|_| Outcome::Answered);
            let written = execute(
                &format!("reference.{command}"),
                r#"{"note":"photo","text":"in a unit"}"#,
                &mut world,
            );
            assert_eq!(written, Ok(r#"{"rev":"r1"}"#.to_owned()), "`{command}`");
            assert_eq!(world.units, [("begin", 0), (ended, 0)], "`{command}`");
            assert_eq!(
                world
                    .store
                    .get("note/photo")
                    .map(|held| held.value.as_str()),
                Some(r#"{"text":"in a unit"}"#),
                "`{command}`"
            );
        }
        let mut refusing = Scripted::answering(|_| Outcome::Answered);
        refusing.refusing = Some(Refused::Unavailable("full".to_owned()));
        assert_eq!(
            execute(
                "reference.unit.commit",
                r#"{"note":"photo","text":"x"}"#,
                &mut refusing
            ),
            Err(Refused::Unavailable("full".to_owned())),
            "a write refused ends the command before the unit is ended"
        );
        assert_eq!(refusing.units, [("begin", 0)]);
    }

    /// A request sent with its compensation registers what sets it right,
    /// sends the `POST`, and commits.
    ///
    /// What it registers is this plugin's own `note.put` unless it is told
    /// another, then a second when it is told `then`, and a compensation
    /// refused is answered rather than carried, nothing registered after it.
    #[test]
    fn a_compensated_request_registers_what_sets_it_right_sends_and_commits() {
        let mut world = Scripted::answering(|_| Outcome::Answered);
        let sent = execute(
            "peer.compensated.post",
            r#"{"note":"photo","url":"https://example.com/"}"#,
            &mut world,
        )
        .expect("answered");
        assert_eq!(
            sent,
            nc_canon::Value::object([
                ("compensation", nc_canon::Value::text("registered")),
                (
                    "sent",
                    nc_canon::Value::text(
                        r#"{"bytes":0,"headers":0,"location":null,"status":200,"text":""}"#
                    )
                ),
            ])
            .to_canonical()
        );
        assert_eq!(
            world.dispatched,
            [(
                "peer.note.put".to_owned(),
                r#"{"note":"photo","text":"compensated"}"#.to_owned()
            )]
        );
        assert_eq!(world.fetched[0].method, "POST");
        assert_eq!(
            world.units,
            [("begin", 0), ("compensate-with", 0), ("commit", 0)]
        );
        world.refusing_compensation = Some(Refused::Invalid("changes nothing".to_owned()));
        let told = execute(
            "reference.compensated.post",
            r#"{"compensate":"reference.note.get","note":"photo","url":"https://example.com/","with":"{}"}"#,
            &mut world,
        )
        .expect("answered");
        assert!(
            told.starts_with(r#"{"compensation":"invalid: changes nothing""#),
            "{told}"
        );
        assert_eq!(
            world.dispatched[1],
            ("reference.note.get".to_owned(), "{}".to_owned())
        );
        let told_twice = r#"{"note":"photo","then":"{\"note\":\"photo\",\"text\":\"twice\"}","url":"https://example.com/"}"#;
        execute("reference.compensated.post", told_twice, &mut world).expect("answered");
        assert_eq!(
            world.dispatched.len(),
            3,
            "a compensation refused is the last one registered"
        );
        world.refusing_compensation = None;
        world.units.clear();
        execute("reference.compensated.post", told_twice, &mut world).expect("answered");
        assert_eq!(
            world.dispatched[3..],
            [
                (
                    "reference.note.put".to_owned(),
                    r#"{"note":"photo","text":"compensated"}"#.to_owned()
                ),
                (
                    "reference.note.put".to_owned(),
                    r#"{"note":"photo","text":"twice"}"#.to_owned()
                ),
            ]
        );
        assert_eq!(
            world.units,
            [
                ("begin", 0),
                ("compensate-with", 0),
                ("compensate-with", 0),
                ("commit", 0)
            ]
        );
    }

    /// The abuses a note may be told to stop with are the ones its schema
    /// allows, both ways, each read back from its own word.
    #[test]
    fn the_abuses_an_argument_names_are_the_ones_its_schema_allows() {
        let declared = commands()
            .into_iter()
            .find(|command| command.name == "reference.note.put-then-abuse")
            .expect("the command is declared");
        let field = |value: nc_canon::Value, name: &str| {
            value
                .into_object()
                .and_then(|mut fields| fields.remove(name))
                .unwrap_or_else(|| panic!("the schema holds `{name}`"))
        };
        let schema = nc_canon::parse(declared.args_schema).expect("the schema is JSON");
        let abuse = field(field(schema, "properties"), "abuse");
        let mut allowed: Vec<String> = field(abuse, "enum")
            .into_array()
            .expect("the abuse is one of a list")
            .into_iter()
            .filter_map(nc_canon::Value::into_text)
            .collect();
        allowed.sort_unstable();
        let mut words: Vec<String> = Abuse::ALL
            .iter()
            .map(|abuse| abuse.word().to_owned())
            .collect();
        words.sort_unstable();
        assert_eq!(allowed, words);
        for abuse in Abuse::ALL {
            assert_eq!(Abuse::named(abuse.word()), Some(abuse));
        }
        assert_eq!(Abuse::named("sleep"), None);
    }

    /// A note told to trap traps once it has written, and one told an abuse
    /// no command knows is refused before it writes anything.
    #[test]
    fn a_note_told_to_trap_traps_once_it_has_written_and_an_unknown_abuse_writes_nothing() {
        let mut world = Scripted::answering(|_| Outcome::Answered);
        let unknown = r#"{"abuse":"sleep","note":"photo","text":"a day out"}"#;
        assert!(matches!(
            execute("reference.note.put-then-abuse", unknown, &mut world),
            Err(Refused::Invalid(why)) if why.contains("`sleep`")
        ));
        assert!(
            world.store.is_empty(),
            "an unknown abuse wrote first: {:?}",
            world.store
        );
        let trap = r#"{"abuse":"trap","note":"photo","text":"a day out"}"#;
        let trapped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            execute("reference.note.put-then-abuse", trap, &mut world)
        }));
        assert!(trapped.is_err(), "it answered: {trapped:?}");
        assert!(
            world.store.contains_key("note/photo"),
            "it trapped before it wrote"
        );
    }

    /// A blob is made of its text written as often as told, its hash answered,
    /// and read back whole a chunk at a time; where one is, is the world's word.
    #[test]
    fn a_blob_is_made_as_told_and_read_back_whole_a_chunk_at_a_time() {
        let mut world = Scripted::answering(|_| Outcome::Answered);
        assert_eq!(
            execute(
                "reference.blob.put",
                r#"{"note":"photo","text":"ab","times":4}"#,
                &mut world
            ),
            Ok(r#"{"hash":"blob0"}"#.to_owned())
        );
        assert_eq!(world.blobs["blob0"], b"abababab");
        assert_eq!(
            execute(
                "reference.blob.get",
                r#"{"hash":"blob0","max":3,"note":"photo"}"#,
                &mut world
            ),
            Ok(r#"{"chunks":[3,3,2],"size":8,"text":"abababab"}"#.to_owned())
        );
        assert_eq!(
            execute(
                "reference.blob.put",
                r#"{"note":"photo","text":"once"}"#,
                &mut world
            ),
            Ok(r#"{"hash":"blob1"}"#.to_owned()),
            "written once when nothing says how often"
        );
        assert_eq!(world.blobs["blob1"], b"once");
        assert_eq!(
            execute(
                "reference.blob.put",
                r#"{"located":true,"note":"photo","text":"asked","times":0}"#,
                &mut world
            ),
            Ok(r#"{"availability":"local","hash":"blob2"}"#.to_owned()),
            "made and then asked where it is, in the one command"
        );
        assert_eq!(world.blobs["blob2"], b"", "written no times over");
        assert_eq!(
            execute(
                "reference.blob.locate",
                r#"{"hash":"blob1","note":"photo"}"#,
                &mut world
            ),
            Ok(r#"{"availability":"local"}"#.to_owned())
        );
        assert_eq!(
            execute(
                "reference.blob.locate",
                r#"{"hash":"elsewhere","note":"photo"}"#,
                &mut world
            ),
            Ok(r#"{"availability":"unavailable"}"#.to_owned())
        );
        assert_eq!(
            execute(
                "reference.blob.get",
                r#"{"hash":"elsewhere","note":"photo"}"#,
                &mut world
            ),
            Err(Refused::Unavailable("no elsewhere".to_owned())),
            "a refusal is the command's"
        );
        world
            .blobs
            .insert("large".to_owned(), vec![b'x'; super::SHOWN + 1]);
        assert!(
            execute(
                "reference.blob.get",
                r#"{"hash":"large","note":"photo"}"#,
                &mut world
            )
            .is_ok_and(|answered| answered.ends_with(r#""text":null}"#)),
            "bytes past what an answer repeats are not repeated"
        );
        world.blobs.insert("binary".to_owned(), vec![0xff, 0xfe]);
        assert!(
            execute(
                "reference.blob.get",
                r#"{"hash":"binary","note":"photo"}"#,
                &mut world
            )
            .is_ok_and(|answered| answered.ends_with(r#""text":null}"#)),
            "bytes that are not text are not repeated as text"
        );
    }

    /// A fetch sends what it is told, and answers what came back.
    ///
    /// A `GET` unless told otherwise, a `POST` from `effect.post`, and a
    /// refusal answered as a value.
    #[test]
    fn a_fetch_sends_what_it_is_told_and_answers_what_came_back() {
        let mut world = Scripted::answering(|_| Outcome::Answered);
        assert_eq!(
            execute(
                "reference.abuse.fetch",
                r#"{"note":"photo","url":"https://example.com/"}"#,
                &mut world
            ),
            Ok(r#"{"bytes":0,"headers":0,"location":null,"status":200,"text":""}"#.to_owned())
        );
        assert_eq!(
            world.fetched,
            [Fetch {
                method: "GET".to_owned(),
                url: "https://example.com/".to_owned(),
                headers: Vec::new(),
                body: None,
            }]
        );
        world.fetch_answer = Some(Ok(Fetched {
            status: 301,
            headers: vec![
                ("x-one".to_owned(), "1".to_owned()),
                ("Location".to_owned(), "https://example.org/".to_owned()),
            ],
            body: b"moved".to_vec(),
        }));
        let told = r#"{"body":"hi","headers":[{"name":"Accept","value":"*/*"}],"method":"TRACE","note":"photo","url":"https://example.com/a"}"#;
        assert_eq!(
            execute("reference.abuse.fetch", told, &mut world),
            Ok(
                r#"{"bytes":5,"headers":2,"location":"https://example.org/","status":301,"text":"moved"}"#
                    .to_owned()
            )
        );
        assert_eq!(
            world.fetched[1],
            Fetch {
                method: "TRACE".to_owned(),
                url: "https://example.com/a".to_owned(),
                headers: vec![("Accept".to_owned(), "*/*".to_owned())],
                body: Some(b"hi".to_vec()),
            }
        );
        let posted = execute(
            "reference.effect.post",
            r#"{"fill":3,"note":"photo","url":"https://example.com/"}"#,
            &mut world,
        );
        assert!(posted.is_ok(), "{posted:?}");
        assert_eq!(world.fetched[2].method, "POST");
        assert_eq!(world.fetched[2].body.as_deref(), Some(&b"xxx"[..]));
        world.fetch_answer = Some(Err(Refused::Denied("not declared".to_owned())));
        assert_eq!(
            execute(
                "reference.abuse.fetch",
                r#"{"note":"photo","url":"https://example.org/"}"#,
                &mut world
            ),
            Ok(r#"{"refused":"denied: not declared"}"#.to_owned()),
            "a refusal is answered, not carried"
        );
        assert!(matches!(
            execute(
                "reference.abuse.fetch",
                r#"{"headers":[{"name":"a"}],"note":"photo","url":"https://example.com/"}"#,
                &mut world
            ),
            Err(Refused::Invalid(why)) if why.contains("`headers`")
        ));
    }

    /// A blob told how to end ends so once it is made.
    ///
    /// Told to fail it refuses, told to trap it traps, and told an end no
    /// command knows it is refused before it makes anything.
    #[test]
    fn a_blob_told_how_to_end_ends_so_once_it_is_made_and_an_unknown_end_makes_nothing() {
        let mut world = Scripted::answering(|_| Outcome::Answered);
        assert!(matches!(
            execute(
                "reference.blob.put",
                r#"{"note":"photo","text":"a","then":"sleep"}"#,
                &mut world
            ),
            Err(Refused::Invalid(why)) if why.contains("`sleep`")
        ));
        assert!(
            world.blobs.is_empty(),
            "an unknown end made a blob first: {:?}",
            world.blobs
        );
        assert!(matches!(
            execute(
                "reference.blob.put",
                r#"{"note":"photo","text":"a","then":"fail"}"#,
                &mut world
            ),
            Err(Refused::Invalid(why)) if why.contains("made the blob `blob0` and then refused")
        ));
        let trapped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            execute(
                "reference.blob.put",
                r#"{"note":"photo","text":"a","then":"trap"}"#,
                &mut world,
            )
        }));
        assert!(trapped.is_err(), "it answered: {trapped:?}");
        assert_eq!(world.blobs.len(), 2, "each made its blob before it ended");
        world.refusing = Some(Refused::Denied("no".to_owned()));
        assert_eq!(
            execute(
                "reference.blob.put",
                r#"{"note":"photo","text":"a","then":"fail"}"#,
                &mut world
            ),
            Err(Refused::Denied("no".to_owned())),
            "a blob refused is the refusal, before the one it is told to make"
        );
    }

    /// The ends a blob may be told are the ones its schema allows, both ways.
    #[test]
    fn the_ends_a_blob_may_be_told_are_the_ones_its_schema_allows() {
        let declared = commands()
            .into_iter()
            .find(|command| command.name == "reference.blob.put")
            .expect("the command is declared");
        let field = |value: nc_canon::Value, name: &str| {
            value
                .into_object()
                .and_then(|mut fields| fields.remove(name))
                .unwrap_or_else(|| panic!("the schema holds `{name}`"))
        };
        let schema = nc_canon::parse(declared.args_schema).expect("the schema is JSON");
        let then = field(field(schema, "properties"), "then");
        let mut allowed: Vec<String> = field(then, "enum")
            .into_array()
            .expect("the end is one of a list")
            .into_iter()
            .filter_map(nc_canon::Value::into_text)
            .collect();
        allowed.sort_unstable();
        let mut words: Vec<String> = Abuse::ALL
            .iter()
            .map(|abuse| abuse.word().to_owned())
            .chain(["fail".to_owned()])
            .collect();
        words.sort_unstable();
        assert_eq!(allowed, words);
    }

    /// A dispatch into its own plugin names its own namespace, whichever it
    /// was declared under, and answers what it was told.
    #[test]
    fn a_dispatch_into_its_own_plugin_names_its_own_namespace_and_answers_what_it_was_told() {
        let mut world = Scripted::answering(|_| Outcome::Answered);
        assert_eq!(
            execute("peer.abuse.reenter", r#"{"note":"photo"}"#, &mut world),
            Ok(r#"{"answered":"{\"done\":true}"}"#.to_owned())
        );
        assert_eq!(
            world.dispatched,
            [(
                "peer.executions.count".to_owned(),
                r#"{"at_most":1,"note":"photo"}"#.to_owned()
            )]
        );
        world.refusing_dispatch = Some(Refused::Denied("running already".to_owned()));
        assert_eq!(
            execute("reference.abuse.reenter", r#"{"note":"photo"}"#, &mut world),
            Ok(r#"{"refused":"denied: running already"}"#.to_owned())
        );
        assert_eq!(world.dispatched[1].0, "reference.executions.count");
    }

    /// A pair writes its note here, then dispatches the peer's command with
    /// the same note and text, and answers what each said.
    #[test]
    fn a_pair_writes_here_then_dispatches_the_peer_and_answers_both() {
        let mut world = Scripted::answering(|_| Outcome::Answered);
        let note = r#"{"note":"photo","text":"a day out"}"#;
        assert_eq!(
            execute("reference.pair.put", note, &mut world),
            Ok(r#"{"here":"{\"rev\":\"r1\"}","peer":"{\"done\":true}"}"#.to_owned())
        );
        assert!(
            world.store.contains_key("note/photo"),
            "it wrote nothing here"
        );
        assert_eq!(
            world.dispatched,
            [(format!("{PEER}.note.put"), note.to_owned())]
        );
        world.dispatched.clear();
        world.refusing_dispatch = Some(Refused::Internal("it trapped".to_owned()));
        assert_eq!(
            execute("reference.pair.put-then-trap", note, &mut world),
            Ok(r#"{"here":"{\"rev\":\"r2\"}","peer":"internal: it trapped"}"#.to_owned()),
            "the peer's refusal is answered, not carried further"
        );
        assert_eq!(world.dispatched[0].0, format!("{PEER}.note.put-then-trap"));
    }

    #[test]
    fn every_command_passes_the_judge_a_kernel_holds_it_to() {
        for command in commands() {
            let judged = nc_schema::descriptor::judge(&nc_schema::descriptor::Declared {
                name: &command.name,
                title: command.title,
                args_schema: command.args_schema,
                action: command.action,
                resource_selector: command.resource_selector,
                mutating: command.mutating,
                reversibility: match command.reversibility {
                    Reversibility::Read => nc_schema::descriptor::Reversibility::Read,
                    Reversibility::Write => nc_schema::descriptor::Reversibility::Write,
                    Reversibility::Irreversible => {
                        nc_schema::descriptor::Reversibility::Irreversible
                    }
                },
            })
            .unwrap_or_else(|problems| panic!("`{}` {problems:?}", command.name));
            assert_eq!(
                judged.selector.entity_type(),
                "Content",
                "`{}`",
                command.name
            );
        }
    }

    #[test]
    fn every_command_is_in_the_namespace_the_manifest_declares() {
        let manifest: toml::Table =
            toml::from_str(include_str!("../plugin.toml")).expect("plugin.toml is TOML");
        assert_eq!(
            manifest.get("namespace").and_then(toml::Value::as_str),
            Some(NAMESPACE),
            "plugin.toml and `NAMESPACE` name two namespaces"
        );
        for command in commands() {
            assert!(
                command.name.starts_with(&format!("{NAMESPACE}.")),
                "`{}` is outside the namespace",
                command.name
            );
        }
    }

    #[test]
    fn the_probe_calls_every_function_once_and_writes_what_each_answered() {
        let mut world = Scripted::answering(|function| match function {
            "log#write" => Outcome::Answered,
            "blobs#[method]reader.size" => Outcome::Unreachable,
            _ => Outcome::Refused(Refused::Denied(format!("no {function}"))),
        });
        let answered = probe(&mut world);
        assert_eq!(
            world.asked, PROBED,
            "the probe calls each function once, in order"
        );
        let read = nc_canon::parse(&answered)
            .expect("the probe answers canonical JSON")
            .into_object()
            .expect("an object");
        assert_eq!(read.len(), PROBED.len());
        let written = |key: &str| read.get(key).cloned().and_then(nc_canon::Value::into_text);
        assert_eq!(written("log#write").as_deref(), Some("answered"));
        assert_eq!(
            written("blobs#[method]reader.size").as_deref(),
            Some("unreachable")
        );
        assert_eq!(
            written("net#fetch").as_deref(),
            Some("denied: no net#fetch")
        );
    }

    /// Each row of [`PROBED`] names an interface and a function, and no row
    /// is written twice.
    ///
    /// How many rows there are is the world's to say, and
    /// `crates/nc-kernel/tests/capabilities.rs` holds the list to the world;
    /// the array's length here is only what this file wrote.
    #[test]
    fn every_probed_function_is_named_once_by_interface_and_function() {
        let distinct: std::collections::BTreeSet<&str> = PROBED.into_iter().collect();
        assert_eq!(distinct.len(), PROBED.len());
        for function in PROBED {
            let (interface, name) = function.split_once('#').expect("interface#function");
            assert!(
                !interface.is_empty() && !name.is_empty(),
                "`{function}` names an interface and a function"
            );
        }
    }

    #[test]
    fn every_case_of_a_refusal_is_written_as_the_worlds_word() {
        for (refused, word) in [
            (Refused::Denied("a".to_owned()), "denied: a"),
            (Refused::NotFound("a".to_owned()), "not-found: a"),
            (Refused::Invalid("a".to_owned()), "invalid: a"),
            (Refused::Conflict("a".to_owned()), "conflict: a"),
            (Refused::Unavailable("a".to_owned()), "unavailable: a"),
            (Refused::Internal("a".to_owned()), "internal: a"),
        ] {
            assert_eq!(Outcome::Refused(refused).written(), word);
        }
    }

    #[test]
    fn the_probe_runs_under_any_namespace_and_a_name_not_declared_is_not_found() {
        let mut world = Scripted::answering(|_| Outcome::Answered);
        assert!(
            execute("reference.probe", "{}", &mut world)
                .is_ok_and(|answered| answered.contains("\"timers#cancel\":\"answered\""))
        );
        assert!(matches!(
            execute("reference.note.rename", "{}", &mut world),
            Err(Refused::NotFound(why)) if why.contains("`reference.note.rename`")
        ));
        assert_eq!(
            world.asked.len(),
            PROBED.len(),
            "only the probe reached the world"
        );
        assert!(
            execute("peer.probe", "{}", &mut world).is_ok(),
            "the probe runs under whichever namespace it was declared"
        );
        assert!(matches!(
            execute("probe", "{}", &mut world),
            Err(Refused::NotFound(_))
        ));
    }

    #[test]
    fn a_note_is_written_over_the_revision_it_holds_read_back_and_deleted() {
        let mut world = Scripted::answering(|_| Outcome::Answered);
        let note = r#"{"note":"photo","text":"a day out"}"#;
        assert_eq!(
            execute("reference.note.get", r#"{"note":"photo"}"#, &mut world),
            Ok("null".to_owned()),
            "nothing written yet"
        );
        assert_eq!(
            execute("reference.note.put", note, &mut world),
            Ok(r#"{"rev":"r1"}"#.to_owned())
        );
        assert_eq!(
            execute("reference.note.put", note, &mut world),
            Ok(r#"{"rev":"r2"}"#.to_owned()),
            "the second write names the revision the first left"
        );
        assert_eq!(
            execute("reference.note.get", r#"{"note":"photo"}"#, &mut world),
            Ok(r#"{"text":"a day out"}"#.to_owned())
        );
        assert_eq!(
            execute("reference.note.delete", r#"{"note":"photo"}"#, &mut world),
            Ok(r#"{"deleted":true}"#.to_owned())
        );
        assert!(world.store.is_empty(), "{:?}", world.store);
        assert!(matches!(
            execute("reference.note.put", r#"{"note":"photo"}"#, &mut world),
            Err(Refused::Invalid(why)) if why.contains("`text`")
        ));
        assert!(matches!(
            execute("reference.note.get", "[]", &mut world),
            Err(Refused::Invalid(_))
        ));
        assert!(matches!(
            execute("reference.note.get", "{", &mut world),
            Err(Refused::Invalid(_))
        ));
    }

    #[test]
    fn a_note_written_and_then_refused_is_written_first() {
        let mut world = Scripted::answering(|_| Outcome::Answered);
        let note = r#"{"note":"photo","text":"a day out"}"#;
        assert!(matches!(
            execute("reference.note.put-then-fail", note, &mut world),
            Err(Refused::Invalid(why)) if why.contains("refused, as it is written to")
        ));
        assert!(world.store.contains_key("note/photo"), "it wrote first");
        world.refusing = Some(Refused::Denied("no".to_owned()));
        assert_eq!(
            execute("reference.note.put-then-fail", note, &mut world),
            Err(Refused::Denied("no".to_owned())),
            "a write refused is the refusal, before the one it is written to make"
        );
    }

    #[test]
    #[should_panic(expected = "wrote its note and then trapped")]
    fn a_note_written_and_then_trapped_panics_once_it_has_written() {
        let mut world = Scripted::answering(|_| Outcome::Answered);
        let _ = execute(
            "reference.note.put-then-trap",
            r#"{"note":"photo","text":"a day out"}"#,
            &mut world,
        );
    }

    /// A trap traps before it reaches the world, so nothing it ran wrote.
    #[test]
    fn a_trap_panics_before_it_reaches_the_world() {
        let mut world = Scripted::answering(|_| Outcome::Answered);
        let trapped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            execute("reference.abuse.trap", r#"{"note":"photo"}"#, &mut world)
        }));
        assert!(trapped.is_err(), "it answered: {trapped:?}");
        assert!(
            world.store.is_empty() && world.asked.is_empty() && world.dispatched.is_empty(),
            "it reached the world before it trapped"
        );
    }

    #[test]
    fn a_scribble_answers_what_the_write_it_tried_was_told() {
        let mut world = Scripted::answering(|_| Outcome::Answered);
        assert_eq!(
            execute("reference.note.scribble", r#"{"note":"photo"}"#, &mut world),
            Ok(r#"{"written":"r1"}"#.to_owned())
        );
        world.refusing = Some(Refused::Denied("only reading".to_owned()));
        assert_eq!(
            execute("reference.note.scribble", r#"{"note":"photo"}"#, &mut world),
            Ok(r#"{"refused":"denied: only reading"}"#.to_owned())
        );
    }

    #[test]
    fn the_count_rises_with_every_command_this_instance_runs() {
        let mut world = Scripted::answering(|_| Outcome::Answered);
        let count = |world: &mut Scripted| {
            let answered = execute(
                "reference.executions.count",
                r#"{"at_most":1,"note":"photo"}"#,
                world,
            )
            .expect("counted");
            nc_canon::parse(&answered)
                .expect("canonical")
                .into_object()
                .and_then(|mut fields| fields.remove("executions"))
                .and_then(|executions| match executions {
                    nc_canon::Value::Integer(executions) => Some(executions),
                    _ => None,
                })
                .expect("a count")
        };
        let first = count(&mut world);
        assert!(first >= 1, "the count includes itself");
        let _ = execute("reference.note.rename", "{}", &mut world);
        assert!(
            count(&mut world) >= first + 2,
            "a command not found still ran, and the count saw it"
        );
    }

    #[test]
    fn what_cannot_be_undone_is_written_over_what_it_left() {
        let mut world = Scripted::answering(|_| Outcome::Answered);
        for _ in 0..2 {
            assert_eq!(
                execute(
                    "reference.irreversible.do",
                    r#"{"note":"photo"}"#,
                    &mut world
                ),
                Ok(r#"{"done":true}"#.to_owned())
            );
        }
        assert_eq!(
            world.store.get("done/photo").map(|held| held.rev.as_str()),
            Some("r2")
        );
    }

    #[test]
    fn a_relay_dispatches_as_often_as_it_is_told_and_stops_at_a_refusal() {
        let mut world = Scripted::answering(|_| Outcome::Answered);
        let relayed = r#"{"args":"{}","name":"peer.note.get","note":"photo"}"#;
        assert_eq!(
            execute("reference.relay.write", relayed, &mut world),
            Ok(r#"{"answered":1,"last":"{\"done\":true}","refused":null}"#.to_owned())
        );
        assert_eq!(
            world.dispatched,
            [("peer.note.get".to_owned(), "{}".to_owned())]
        );
        world.dispatched.clear();
        world.refusing = Some(Refused::Denied("too many".to_owned()));
        let three = r#"{"args":"{}","name":"peer.note.get","note":"photo","times":3}"#;
        assert_eq!(
            execute("reference.relay.read", three, &mut world),
            Ok(
                r#"{"answered":1,"last":"{\"done\":true}","refused":"denied: too many"}"#
                    .to_owned()
            )
        );
        assert_eq!(world.dispatched.len(), 2, "it stopped at the first refusal");
        world.dispatched.clear();
        let through =
            r#"{"args":"{}","name":"peer.note.get","note":"photo","through":true,"times":3}"#;
        assert_eq!(
            execute("reference.relay.read", through, &mut world),
            Ok(
                r#"{"answered":1,"last":"{\"done\":true}","refused":"denied: too many"}"#
                    .to_owned()
            )
        );
        assert_eq!(
            world.dispatched.len(),
            3,
            "told to go on through its refusals, it asked every time it was told to"
        );
    }

    #[test]
    fn an_echo_answers_its_text_whatever_it_is() {
        let mut world = Scripted::answering(|_| Outcome::Answered);
        assert_eq!(
            execute(
                "reference.echo",
                r#"{"note":"photo","text":"not json"}"#,
                &mut world
            ),
            Ok("not json".to_owned())
        );
    }

    /// The judge's verdict on a command, in the judge's own types.
    fn judged(
        command: &super::Command,
    ) -> Result<nc_schema::descriptor::Judged, Vec<nc_schema::descriptor::DescriptorProblem>> {
        nc_schema::descriptor::judge(&nc_schema::descriptor::Declared {
            name: &command.name,
            title: command.title,
            args_schema: command.args_schema,
            action: command.action,
            resource_selector: command.resource_selector,
            mutating: command.mutating,
            reversibility: match command.reversibility {
                Reversibility::Read => nc_schema::descriptor::Reversibility::Read,
                Reversibility::Write => nc_schema::descriptor::Reversibility::Write,
                Reversibility::Irreversible => nc_schema::descriptor::Reversibility::Irreversible,
            },
        })
    }

    #[test]
    fn an_empty_store_activates_the_table_as_written_under_the_plugins_namespace() {
        assert_eq!(
            Activation::default(),
            Activation {
                namespace: NAMESPACE.to_owned(),
                variant: Variant::Normal,
                subscribe: Vec::new(),
            }
        );
        assert_eq!(commands(), commands_for(&Activation::default()));
        let peer = commands_for(&Activation {
            namespace: "peer".to_owned(),
            variant: Variant::Normal,
            subscribe: Vec::new(),
        });
        let names: Vec<String> = peer.iter().map(|command| command.name.clone()).collect();
        let renamed: Vec<String> = commands()
            .iter()
            .map(|command| command.name.replacen(NAMESPACE, "peer", 1))
            .collect();
        assert_eq!(names, renamed);
        assert_eq!(names[..2], ["peer.note.get", "peer.probe"]);
        assert_eq!(ACTIVATION, "activation");
    }

    #[test]
    fn every_variant_is_written_and_read_back_as_itself() {
        for variant in Variant::ALL {
            let activation = Activation {
                namespace: "peer".to_owned(),
                variant,
                subscribe: Vec::new(),
            };
            let written = activation.written();
            assert_eq!(
                nc_canon::parse(&written).map(|value| value.to_canonical()),
                Ok(written.clone()),
                "the store holds canonical text"
            );
            assert_eq!(Activation::read(&written), Ok(activation.clone()));
            assert_eq!(Variant::named(variant.word()), Some(variant));
            assert_eq!(activation.traps(), variant == Variant::Trap);
        }
        assert_eq!(Variant::named("sideways"), None);
    }

    #[test]
    fn an_activation_subscribes_to_what_it_names_and_reads_it_back() {
        let activation = Activation {
            namespace: PEER.to_owned(),
            variant: Variant::Normal,
            subscribe: vec!["reference.*".to_owned(), "reference.noted".to_owned()],
        };
        let written = activation.written();
        assert_eq!(
            written,
            r#"{"namespace":"peer","subscribe":["reference.*","reference.noted"],"variant":"normal"}"#
        );
        assert_eq!(Activation::read(&written), Ok(activation));
        assert_eq!(
            Activation::default().written(),
            r#"{"namespace":"reference","variant":"normal"}"#,
            "no subscriptions, no field"
        );
        for (stored, says) in [
            (
                r#"{"namespace":"peer","subscribe":"a.b","variant":"normal"}"#,
                "is not a list",
            ),
            (
                r#"{"namespace":"peer","subscribe":[1],"variant":"normal"}"#,
                "not a topic",
            ),
        ] {
            assert!(
                matches!(Activation::read(stored), Err(Refused::Invalid(ref why)) if why.contains(says)),
                "{stored}: {:?}",
                Activation::read(stored)
            );
        }
    }

    #[test]
    fn an_event_is_published_as_often_as_told_and_stops_at_the_first_refusal() {
        let mut world = Scripted::answering(|_| Outcome::Answered);
        let answered = execute(
            "reference.event.publish",
            r#"{"data":"{\"a\":1}","note":"photo","times":3,"topic":"reference.noted"}"#,
            &mut world,
        );
        assert_eq!(answered.as_deref(), Ok(r#"{"published":3,"refused":null}"#));
        assert_eq!(world.world.len(), 3);
        assert_eq!(
            world.world[0],
            ("publish", r#"reference.noted {"a":1}"#.to_owned())
        );
        let mut refusing = Scripted::answering(|_| Outcome::Answered);
        refusing.world_answers = 1;
        let answered = execute(
            "reference.event.announce",
            r#"{"note":"photo","times":5,"topic":"reference.noted"}"#,
            &mut refusing,
        );
        assert_eq!(
            answered.as_deref(),
            Ok(r#"{"published":1,"refused":"unavailable: publish refused"}"#)
        );
        assert_eq!(refusing.world.len(), 2, "it stops at the first refusal");
        assert_eq!(
            refusing.world[0].1, "reference.noted {}",
            "no data is an empty object"
        );
        let mut filled = Scripted::answering(|_| Outcome::Answered);
        let refused = execute(
            "reference.event.publish",
            r#"{"fill":3,"note":"photo","then":"fail","topic":"reference.noted"}"#,
            &mut filled,
        );
        assert!(
            matches!(refused, Err(Refused::Invalid(ref why)) if why.contains("published 1")),
            "{refused:?}"
        );
        assert_eq!(filled.world[0].1, r#"reference.noted {"fill":"xxx"}"#);
    }

    #[test]
    fn a_timer_is_set_and_cancelled_as_told_and_stops_at_the_first_refusal() {
        let mut world = Scripted::answering(|_| Outcome::Answered);
        let answered = execute(
            "reference.timer.set",
            r#"{"every":true,"ms":1000,"note":"photo","tag":"put:photo","times":2}"#,
            &mut world,
        );
        assert_eq!(
            answered.as_deref(),
            Ok(r#"{"refused":null,"timers":[1,2]}"#)
        );
        assert_eq!(world.world[0], ("set", "1000 true put:photo".to_owned()));
        let cancelled = execute(
            "reference.timer.cancel",
            r#"{"note":"photo","timer":7}"#,
            &mut world,
        );
        assert_eq!(cancelled.as_deref(), Ok(r#"{"cancelled":"answered"}"#));
        assert_eq!(world.world[2], ("cancel", "7".to_owned()));
        let mut refusing = Scripted::answering(|_| Outcome::Answered);
        refusing.world_answers = 0;
        let answered = execute(
            "reference.timer.set",
            r#"{"ms":5,"note":"photo","tag":"t","times":3}"#,
            &mut refusing,
        );
        assert_eq!(
            answered.as_deref(),
            Ok(r#"{"refused":"unavailable: set refused","timers":[]}"#)
        );
        assert_eq!(refusing.world.len(), 1);
        assert_eq!(refusing.world[0].1, "5 false t", "a timer that waits once");
        let refused = execute(
            "reference.timer.set",
            r#"{"ms":5,"note":"photo","tag":"t","then":"fail"}"#,
            &mut Scripted::answering(|_| Outcome::Answered),
        );
        assert!(
            matches!(refused, Err(Refused::Invalid(ref why)) if why.contains("set 1")),
            "{refused:?}"
        );
        let failed = execute(
            "reference.timer.cancel",
            r#"{"note":"photo","then":"fail","timer":4}"#,
            &mut world,
        );
        assert!(
            matches!(failed, Err(Refused::Invalid(ref why)) if why.contains("cancelled the timer 4")),
            "{failed:?}"
        );
        assert_eq!(
            world.world.last(),
            Some(&("cancel", "4".to_owned())),
            "cancelled first"
        );
        let refused = execute(
            "reference.timer.cancel",
            r#"{"note":"photo","timer":3}"#,
            &mut refusing,
        );
        assert_eq!(
            refused.as_deref(),
            Ok(r#"{"cancelled":"unavailable: cancel refused"}"#)
        );
    }

    #[test]
    fn a_surface_is_opened_rendered_and_closed_as_told() {
        let mut world = Scripted::answering(|_| Outcome::Answered);
        let answered = execute(
            "reference.ui.show",
            r#"{"close":true,"kind":"overlay","note":"photo","title":"Notes","tree":"{\"node\":\"divider\"}"}"#,
            &mut world,
        );
        assert_eq!(
            answered.as_deref(),
            Ok(
                r#"{"closed":"answered","opened":"answered","rendered":"answered","surface":"surface-1"}"#
            )
        );
        assert_eq!(
            world.world,
            [
                ("open", "overlay Notes".to_owned()),
                ("render", r#"surface-1 {"node":"divider"}"#.to_owned()),
                ("close", "surface-1".to_owned()),
            ]
        );
        let mut named = Scripted::answering(|_| Outcome::Answered);
        let answered = execute(
            "reference.ui.show",
            r#"{"note":"photo","surface":"surface-9","tree":"{}"}"#,
            &mut named,
        );
        assert_eq!(
            answered.as_deref(),
            Ok(r#"{"closed":null,"opened":null,"rendered":"answered","surface":"surface-9"}"#)
        );
        let mut refusing = Scripted::answering(|_| Outcome::Answered);
        refusing.world_answers = 2;
        let answered = execute(
            "reference.ui.show",
            r#"{"note":"photo","times":5}"#,
            &mut refusing,
        );
        assert_eq!(
            answered.as_deref(),
            Ok(
                r#"{"closed":null,"opened":"unavailable: open refused","rendered":null,"surface":"surface-2"}"#
            )
        );
        assert_eq!(refusing.world.len(), 3, "it stops at the first refusal");
        assert_eq!(
            refusing.world[0].1, "panel Notes",
            "a panel titled `Notes` unless told"
        );
    }

    #[test]
    fn the_context_is_answered_as_it_was_told_or_as_its_refusal() {
        let mut world = Scripted::answering(|_| Outcome::Answered);
        world.context = Ok(Context {
            principal: "a-person".to_owned(),
            actor_chain: vec!["an-agent".to_owned()],
            confirmed: true,
        });
        assert_eq!(
            execute("reference.context.read", r#"{"note":"photo"}"#, &mut world).as_deref(),
            Ok(r#"{"actor_chain":["an-agent"],"confirmed":true,"principal":"a-person"}"#)
        );
        world.context = Err(Refused::Denied("no".to_owned()));
        assert_eq!(
            execute("reference.context.read", r#"{"note":"photo"}"#, &mut world).as_deref(),
            Ok(r#"{"refused":"denied: no"}"#)
        );
    }

    #[test]
    fn a_line_is_written_to_the_log_as_often_as_told_and_numbered() {
        let mut world = Scripted::answering(|_| Outcome::Answered);
        let answered = execute(
            "reference.log.write",
            r#"{"level":"warn","note":"photo","text":"line {n}","times":3}"#,
            &mut world,
        );
        assert_eq!(answered.as_deref(), Ok(r#"{"written":3}"#));
        assert_eq!(
            world.logged,
            [
                ("warn".to_owned(), "line 0".to_owned()),
                ("warn".to_owned(), "line 1".to_owned()),
                ("warn".to_owned(), "line 2".to_owned()),
            ]
        );
        execute(
            "reference.log.write",
            r#"{"note":"photo","text":"once"}"#,
            &mut world,
        )
        .expect("written");
        assert_eq!(world.logged[3], ("info".to_owned(), "once".to_owned()));
    }

    #[test]
    fn a_timer_firing_says_who_it_runs_for_and_does_what_its_tag_says() {
        let mut world = Scripted::answering(|_| Outcome::Answered);
        on_timer("reference", "put:peer:photo", &mut world);
        assert_eq!(
            world.logged[0].1,
            "timer put:peer:photo fired for ana-id, confirmed false"
        );
        assert_eq!(
            world.dispatched,
            [(
                "peer.note.put".to_owned(),
                r#"{"note":"photo","text":"fired"}"#.to_owned()
            )]
        );
        assert_eq!(world.logged[1].1, r#"put "answered""#);
        let mut grouped = Scripted::answering(|_| Outcome::Answered);
        on_timer("reference", "group:peer:photo", &mut grouped);
        assert_eq!(
            grouped
                .dispatched
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            ["peer.note.put", "peer.note.put-many"]
        );
        assert_eq!(grouped.units, [("begin", 0), ("commit", 0)]);
        let mut dropped = Scripted::answering(|_| Outcome::Answered);
        on_timer("reference", "drop:peer:photo", &mut dropped);
        assert_eq!(dropped.units, [("begin", 0), ("release", 0)]);
        assert_eq!(dropped.dispatched[0].0, "peer.note.put");
        let mut publishing = Scripted::answering(|_| Outcome::Answered);
        on_timer("reference", "publish", &mut publishing);
        assert_eq!(
            publishing.world,
            [("publish", r#"reference.fired {"tag":"publish"}"#.to_owned())]
        );
        let mut nothing = Scripted::answering(|_| Outcome::Answered);
        nothing.context = Err(Refused::Denied("not granted".to_owned()));
        on_timer("reference", "log", &mut nothing);
        assert_eq!(
            nothing.logged,
            [(
                "info".to_owned(),
                "timer log fired for nobody it was told: denied: not granted".to_owned()
            )]
        );
        assert!(nothing.dispatched.is_empty() && nothing.world.is_empty());
    }

    #[test]
    fn a_timer_firing_sets_and_cancels_timers_as_its_tag_says() {
        let mut again = Scripted::answering(|_| Outcome::Answered);
        on_timer("reference", "again:250", &mut again);
        assert_eq!(again.world, [("set", "250 false log".to_owned())]);
        assert_eq!(again.logged[1].1, r#"again "answered""#);
        let mut cancelling = Scripted::answering(|_| Outcome::Answered);
        cancelling.world_answers = 0;
        on_timer("reference", "cancel:9", &mut cancelling);
        assert_eq!(cancelling.world, [("cancel", "9".to_owned())]);
        assert_eq!(
            cancelling.logged[1].1,
            r#"cancel "unavailable: cancel refused""#
        );
    }

    #[test]
    #[should_panic(expected = "the timer `trap` trapped")]
    fn a_timer_told_to_trap_panics() {
        on_timer(
            "reference",
            "trap",
            &mut Scripted::answering(|_| Outcome::Answered),
        );
    }

    /// A timer told to trap in a unit begins one, dispatches a note and a blob
    /// into it, says what each answered, and panics with the unit open.
    #[test]
    fn a_timer_told_to_trap_in_a_unit_panics_with_the_unit_open() {
        let mut world = Scripted::answering(|_| Outcome::Answered);
        let trapped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            on_timer("reference", "trapped:peer:photo", &mut world);
        }));
        let said = trapped.expect_err("it returned");
        assert!(
            said.downcast_ref::<String>()
                .is_some_and(|why| why.contains("trapped in the unit it began")),
            "{said:?}"
        );
        assert_eq!(world.units, [("begin", 0)], "begun, and never ended");
        let args = r#"{"note":"photo","text":"trapped"}"#.to_owned();
        assert_eq!(
            world.dispatched,
            [
                ("peer.note.put".to_owned(), args.clone()),
                ("peer.blob.put".to_owned(), args)
            ]
        );
        assert_eq!(
            world.logged[1].1,
            r#"trapped "answered" "answered" "answered""#
        );
    }

    #[test]
    fn hearing_an_event_says_who_it_runs_for_and_does_what_its_data_says() {
        let mut world = Scripted::answering(|_| Outcome::Answered);
        on_event("peer", "reference.noted", r#"{"n":1}"#, &mut world);
        assert_eq!(
            world.logged,
            [(
                "info".to_owned(),
                r#"heard reference.noted {"n":1} for ana-id, confirmed false"#.to_owned()
            )]
        );
        let mut writing = Scripted::answering(|_| Outcome::Answered);
        writing.refusing_dispatch = Some(Refused::Denied("read only".to_owned()));
        on_event(
            "peer",
            "reference.noted",
            r#"{"then":"write"}"#,
            &mut writing,
        );
        assert_eq!(
            writing.dispatched[0].0, "reference.note.put",
            "the publisher's command, not its own"
        );
        assert_eq!(writing.logged[1].1, r#"write "denied: read only""#);
        let mut deferring = Scripted::answering(|_| Outcome::Answered);
        on_event(
            "peer",
            "reference.noted",
            r#"{"then":"defer"}"#,
            &mut deferring,
        );
        assert_eq!(
            deferring.world,
            [("set", "1 false put:reference:photo".to_owned())],
            "a timer whose callback dispatches the publisher's command"
        );
        assert!(
            deferring.dispatched.is_empty(),
            "put off, not dispatched: {:?}",
            deferring.dispatched
        );
        assert_eq!(deferring.logged[1].1, r#"defer "answered""#);
        let mut relaying = Scripted::answering(|_| Outcome::Answered);
        on_event(
            "peer",
            "reference.noted",
            r#"{"then":"relay"}"#,
            &mut relaying,
        );
        assert_eq!(
            relaying.world,
            [(
                "publish",
                r#"peer.relayed {"from":"reference.noted"}"#.to_owned()
            )]
        );
        assert_eq!(relaying.logged[1].1, r#"relay "answered""#);
        let mut fetching = Scripted::answering(|_| Outcome::Answered);
        fetching.fetch_answer = Some(Err(Refused::Denied("not granted".to_owned())));
        on_event(
            "peer",
            "reference.noted",
            r#"{"then":"fetch"}"#,
            &mut fetching,
        );
        assert_eq!(
            fetching
                .fetched
                .iter()
                .map(|request| (request.method.as_str(), request.url.as_str()))
                .collect::<Vec<_>>(),
            [("GET", "https://example.org/heard")]
        );
        assert_eq!(fetching.logged[1].1, r#"fetch "denied: not granted""#);
    }

    #[test]
    #[should_panic(expected = "hearing `reference.noted` trapped")]
    fn an_event_telling_it_to_trap_makes_it_panic() {
        on_event(
            "peer",
            "reference.noted",
            r#"{"then":"trap"}"#,
            &mut Scripted::answering(|_| Outcome::Answered),
        );
    }

    #[test]
    fn a_stored_value_that_is_not_an_activation_is_refused_as_invalid() {
        for (stored, says) in [
            ("{", "is not an activation"),
            ("[]", "it is not an object"),
            (r#"{"variant":"normal"}"#, "`namespace` is not text"),
            (
                r#"{"namespace":1,"variant":"normal"}"#,
                "`namespace` is not text",
            ),
            (r#"{"namespace":"peer"}"#, "`variant` is not text"),
            (
                r#"{"also":true,"namespace":"peer","variant":"normal"}"#,
                "a field beside",
            ),
            (
                r#"{"namespace":"peer","variant":"sideways"}"#,
                "no variant is `sideways`",
            ),
        ] {
            assert!(
                matches!(Activation::read(stored), Err(Refused::Invalid(ref why)) if why.contains(says)),
                "{stored}: {:?}",
                Activation::read(stored)
            );
        }
    }

    /// Each variant breaks the first command and only it, in the one way it
    /// names — the judge refusing those it can see and passing the rest, which
    /// only the kernel, holding the manifest and the decision point, can refuse.
    #[test]
    fn each_variant_breaks_the_first_command_in_the_one_way_it_names() {
        use nc_schema::descriptor::{DescriptorProblem, Field};

        for variant in Variant::ALL {
            let activation = Activation {
                namespace: "peer".to_owned(),
                variant,
                subscribe: Vec::new(),
            };
            let declared = commands_for(&activation);
            assert_eq!(declared.len(), commands().len(), "{variant:?}");
            for (declared, written) in declared.iter().zip(commands()).skip(1) {
                assert_eq!(
                    *declared,
                    super::Command {
                        name: written.name.replacen(NAMESPACE, "peer", 1),
                        ..written.clone()
                    },
                    "{variant:?} touched a command after the first"
                );
            }
            let verdict = judged(&declared[0]).map(drop);
            let expected = match variant {
                Variant::MissingAction => Err(vec![DescriptorProblem::Missing(Field::Action)]),
                Variant::MissingSelector => {
                    Err(vec![DescriptorProblem::Missing(Field::ResourceSelector)])
                }
                Variant::EmptyName => Err(vec![DescriptorProblem::Missing(Field::Name)]),
                Variant::EmptyTitle => Err(vec![DescriptorProblem::Missing(Field::Title)]),
                Variant::EmptySchema => Err(vec![DescriptorProblem::Missing(Field::ArgsSchema)]),
                Variant::ReadThatMutates => Err(vec![DescriptorProblem::ReadThatMutates]),
                Variant::UnknownKeyword | Variant::BadSelector => {
                    assert!(
                        matches!(verdict.as_ref().map_err(Vec::as_slice), Err([_])),
                        "{variant:?}: {verdict:?}"
                    );
                    continue;
                }
                Variant::Normal
                | Variant::Trap
                | Variant::OutsideNamespace
                | Variant::ReservedNamespace
                | Variant::UnknownAction => Ok(()),
            };
            assert_eq!(verdict, expected, "{variant:?}");
        }
        let named = |variant| {
            commands_for(&Activation {
                namespace: "peer".to_owned(),
                variant,
                subscribe: Vec::new(),
            })[0]
                .name
                .clone()
        };
        assert_eq!(named(Variant::OutsideNamespace), "peer-elsewhere.note.get");
        assert_eq!(named(Variant::ReservedNamespace), "kernel.note.get");
        assert_eq!(
            commands_for(&Activation {
                namespace: "peer".to_owned(),
                variant: Variant::UnknownAction,
                subscribe: Vec::new(),
            })[0]
                .action,
            "Rename"
        );
    }
}
