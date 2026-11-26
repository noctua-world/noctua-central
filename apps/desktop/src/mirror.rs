//! The interface's state as this window holds it: the daemon's, followed by
//! a watch, and the facts this window said at the rate it draws, shown at once
//! until the daemon's state has taken them (`NOCTUA.md §12.1`).
//!
//! **The daemon's state is the authority, and this is a copy of it.** Each
//! watch answers the whole state at a revision (`nc_ipc::client::Changed`),
//! and [`Mirror::watched`] reads it strictly — every field the kernel writes
//! taken, of the type it writes it in, every word one of the closed set the
//! kernel writes, and any other field refused — so a state of another shape is
//! never drawn as one it is not: the copy held before stays, and the refusal
//! says what was not read. The rules the kernel holds a state to — a bound, a
//! page every window names — are the kernel's, held where it writes the state;
//! a part one part names and the state does not hold is looked up here as
//! none, never invented.
//!
//! **What moves at the rate a window draws is said, not asked** (`§12.1`):
//! the prompt's draft, where a seat's keyboard is, what its pointer hovers or
//! its keyboard selects, and how far a page is scrolled reach the daemon as a
//! note, each fact the arguments of the command that makes the same change —
//! `prompt.set`, `shell.focus`, `shell.hover` and `page.scroll`. A fact this
//! window said is shown at once ([`Mirror::said`]), over the daemon's state,
//! until the daemon has taken the note that told it ([`Mirror::told`]) and
//! answered a watch past the revision it held then: the state from then on
//! holds the fact, or a later change of someone else's, which is shown
//! instead. A note the daemon refused takes every fact it told back.
//!
//! **A revision is one daemon's** (`nc_ipc::client`): a new connection begins
//! having seen nothing, and [`Mirror::reconnected`] forgets the revision held
//! and every fact said, which the new daemon never heard.
//!
//! # What this answers (`NOCTUA.md §2.4`)
//!
//! A state the daemon sent in a shape this build does not read, drawn as
//! another — refused whole, and the copy before it kept; and a fact said over
//! a state that has moved past it, shown in place of a change someone else
//! made — retired at the first revision past the one held when the daemon took
//! it. **What it does not answer:** a daemon that sends a state it did not
//! hold, which the handshake proved to be the channel's own (`§11.1`).

use std::collections::BTreeMap;

use nc_canon::Value;
use nc_ipc::client::Changed;

/// Why a state the daemon sent was not read: what in it is not what the
/// kernel writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unread(String);

impl core::fmt::Display for Unread {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            formatter,
            "the interface's state the daemon sent was not read, so the state held before \
             it is kept: {}",
            self.0
        )
    }
}

impl core::error::Error for Unread {}

/// What a page is: the closed set the kernel writes (`NOCTUA.md §12.9`, §12.12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The home page.
    Home,
    /// Settings: the list, or one setting's page.
    Settings,
    /// Help: the list, or one topic.
    Help,
    /// Dev: the list, or one page.
    Dev,
    /// A surface a plugin shows.
    Surface,
    /// The leader menu.
    Leader,
    /// The command list.
    Commands,
    /// The focused page's own shortcuts, the `/` list.
    Shortcuts,
    /// The container actions, the `w` list.
    Actions,
}

impl Kind {
    /// Every one, with the word the kernel writes it as.
    const WORDS: [(Self, &'static str); 9] = [
        (Self::Home, "home"),
        (Self::Settings, "settings"),
        (Self::Help, "help"),
        (Self::Dev, "dev"),
        (Self::Surface, "surface"),
        (Self::Leader, "leader"),
        (Self::Commands, "commands"),
        (Self::Shortcuts, "shortcuts"),
        (Self::Actions, "actions"),
    ];
}

/// The two places a page is shown in: a window's workspace area, or the drawer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// The workspace area.
    Workspace,
    /// The drawer.
    Drawer,
}

impl Place {
    /// Every one, with the word the kernel writes it as.
    const WORDS: [(Self, &'static str); 2] =
        [(Self::Workspace, "workspace"), (Self::Drawer, "drawer")];
}

/// The part of a window a seat's keyboard is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    /// The workspace area.
    Workspace,
    /// The drawer.
    Drawer,
    /// The prompt line.
    Prompt,
    /// The sidebar.
    Sidebar,
    /// The info line.
    Info,
    /// The modeline.
    Modeline,
}

impl Region {
    /// Every one, with the word the kernel and `shell.focus` write it as.
    const WORDS: [(Self, &'static str); 6] = [
        (Self::Workspace, "workspace"),
        (Self::Drawer, "drawer"),
        (Self::Prompt, "prompt"),
        (Self::Sidebar, "sidebar"),
        (Self::Info, "info"),
        (Self::Modeline, "modeline"),
    ];

    /// The word a fact writes it as: its row of [`Region::WORDS`].
    const fn word(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::Drawer => "drawer",
            Self::Prompt => "prompt",
            Self::Sidebar => "sidebar",
            Self::Info => "info",
            Self::Modeline => "modeline",
        }
    }
}

/// What a seat moved last: its pointer, or its keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum By {
    /// The pointer: what it hovers.
    Pointer,
    /// The keyboard: what its selection is on.
    Keyboard,
}

impl By {
    /// Every one, with the word the kernel and `shell.hover` write it as.
    const WORDS: [(Self, &'static str); 2] =
        [(Self::Pointer, "pointer"), (Self::Keyboard, "keyboard")];

    /// The word a fact writes it as: its row of [`By::WORDS`].
    const fn word(self) -> &'static str {
        match self {
            Self::Pointer => "pointer",
            Self::Keyboard => "keyboard",
        }
    }
}

/// The word `value` is, of `words`.
fn word_of<T: Copy>(words: &[(T, &'static str)], value: &str) -> Option<T> {
    words
        .iter()
        .find(|(_, word)| *word == value)
        .map(|(item, _)| *item)
}

/// A window, as the daemon holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    /// Its identifier.
    pub id: u64,
    /// Whether it is shown.
    pub shown: bool,
    /// Its stack of pages, the workspace area's, by identifier.
    pub pages: Vec<u64>,
    /// The page in front of its stack, when it holds one.
    pub focused: Option<u64>,
    /// Its width and height, once set.
    pub size: Option<(i64, i64)>,
    /// Where its corner is, once placed.
    pub place: Option<(i64, i64)>,
}

/// A page, as the daemon holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// Its identifier.
    pub id: u64,
    /// What it is.
    pub kind: Kind,
    /// Its topic, for a kind that takes one.
    pub topic: Option<String>,
    /// The sub-pages it went into, outermost first.
    pub trail: Vec<String>,
    /// How far it is scrolled.
    pub scroll: u32,
}

/// The drawer, as the daemon holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drawer {
    /// The pages open in it, by identifier.
    pub pages: Vec<u64>,
    /// The page in focus in it, when it holds one.
    pub focused: Option<u64>,
    /// Whether it is collapsed.
    pub collapsed: bool,
    /// Where a page opened from the leader menu goes.
    pub toward: Place,
    /// Whether the next page opened goes to the other place, once.
    pub flipped: bool,
    /// Its share of the height it shares with the workspace area, in percent.
    pub share: u32,
}

/// What the prompt asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asking {
    /// The command list.
    Commands,
    /// A command's parameters.
    Parameters {
        /// The command.
        command: String,
        /// The parameter it asks for, when it asks for one.
        parameter: Option<String>,
    },
}

/// Where a seat's keyboard is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Focus {
    /// The window.
    pub window: u64,
    /// The part of it.
    pub region: Region,
}

/// A seat — one pointer and one keyboard (`NOCTUA.md §12.18`) — as the daemon
/// holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seat {
    /// What it moved last.
    pub latest: By,
    /// Where its keyboard is.
    pub focus: Option<Focus>,
    /// What its pointer hovers.
    pub hover: Option<String>,
    /// What its keyboard selection is on.
    pub selection: Option<String>,
    /// What it drags.
    pub drag: Option<String>,
}

/// The interface's state, as the daemon sent it (`NOCTUA.md §12.1`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shell {
    /// Every window, in the order the daemon holds them.
    pub windows: Vec<Window>,
    /// Every page open anywhere, by identifier.
    pub pages: BTreeMap<u64, Page>,
    /// The drawer.
    pub drawer: Drawer,
    /// Whether the sidebar is shown.
    pub sidebar: bool,
    /// Whether the footer is shown now.
    pub footer: bool,
    /// Whether the info line is expanded.
    pub expanded: bool,
    /// What the info line says, when it says something.
    pub message: Option<String>,
    /// What the prompt holds.
    pub draft: String,
    /// What the prompt asks for, when it asks for something.
    pub asking: Option<Asking>,
    /// Every seat, by number.
    pub seats: BTreeMap<u64, Seat>,
    /// The identifier the next window or page is given.
    pub next: u64,
}

/// The fields of one object, each taken once, the rest refused.
struct Fields<'a> {
    /// What the object is, for a refusal.
    what: &'static str,
    /// The fields not taken yet.
    left: BTreeMap<&'a str, &'a Value>,
}

impl<'a> Fields<'a> {
    /// The fields of `value`, which is one object.
    fn of(what: &'static str, value: &'a Value) -> Result<Self, Unread> {
        let Value::Object(fields) = value else {
            return Err(Unread(format!("{what} is not one object")));
        };
        Ok(Self {
            what,
            left: fields
                .iter()
                .map(|(key, value)| (key.as_str(), value))
                .collect(),
        })
    }

    /// The field `key`, or nothing where the object does not hold it.
    fn optional(&mut self, key: &str) -> Option<&'a Value> {
        self.left.remove(key)
    }

    /// The field `key`, which the object holds.
    fn take(&mut self, key: &str) -> Result<&'a Value, Unread> {
        self.optional(key)
            .ok_or_else(|| Unread(format!("{} holds no `{key}`", self.what)))
    }

    /// The field `key`, a whole number.
    fn integer(&mut self, key: &str) -> Result<i64, Unread> {
        match self.take(key)? {
            Value::Integer(number) => Ok(*number),
            _ => Err(Unread(format!(
                "{}'s `{key}` is not a whole number",
                self.what
            ))),
        }
    }

    /// The field `key`, an identifier: a whole number, one at least.
    fn id(&mut self, key: &str) -> Result<u64, Unread> {
        let number = self.integer(key)?;
        u64::try_from(number)
            .ok()
            .filter(|id| *id >= 1)
            .ok_or_else(|| {
                Unread(format!(
                    "{}'s `{key}` is {number}, not an identifier",
                    self.what
                ))
            })
    }

    /// The field `key`, an identifier, or nothing where the object does not
    /// hold it.
    fn optional_id(&mut self, key: &str) -> Result<Option<u64>, Unread> {
        if self.left.contains_key(key) {
            self.id(key).map(Some)
        } else {
            Ok(None)
        }
    }

    /// The field `key`, a whole number that fits 32 bits.
    fn count(&mut self, key: &str) -> Result<u32, Unread> {
        let number = self.integer(key)?;
        u32::try_from(number).map_err(|_| {
            Unread(format!(
                "{}'s `{key}` is {number}, out of its range",
                self.what
            ))
        })
    }

    /// The field `key`, true or false.
    fn boolean(&mut self, key: &str) -> Result<bool, Unread> {
        match self.take(key)? {
            Value::Bool(value) => Ok(*value),
            _ => Err(Unread(format!(
                "{}'s `{key}` is not true or false",
                self.what
            ))),
        }
    }

    /// The field `key`, a text.
    fn text(&mut self, key: &str) -> Result<String, Unread> {
        match self.take(key)? {
            Value::Text(text) => Ok(text.clone()),
            _ => Err(Unread(format!("{}'s `{key}` is not a text", self.what))),
        }
    }

    /// The field `key`, a text, or nothing where the object does not hold it.
    fn optional_text(&mut self, key: &str) -> Result<Option<String>, Unread> {
        if self.left.contains_key(key) {
            self.text(key).map(Some)
        } else {
            Ok(None)
        }
    }

    /// The field `key`, one of `words`.
    fn word<T: Copy>(&mut self, key: &str, words: &[(T, &'static str)]) -> Result<T, Unread> {
        let text = self.text(key)?;
        word_of(words, &text).ok_or_else(|| {
            Unread(format!(
                "{}'s `{key}` is `{}`, which is none of the words it is written as",
                self.what,
                text.escape_debug()
            ))
        })
    }

    /// The field `key`, a list of identifiers.
    fn ids(&mut self, key: &str) -> Result<Vec<u64>, Unread> {
        let Value::Array(items) = self.take(key)? else {
            return Err(Unread(format!("{}'s `{key}` is not a list", self.what)));
        };
        items
            .iter()
            .map(|item| match item {
                Value::Integer(number) => u64::try_from(*number).ok().filter(|id| *id >= 1),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| {
                Unread(format!(
                    "{}'s `{key}` holds what is not an identifier",
                    self.what
                ))
            })
    }

    /// The field `key`, a list of objects.
    fn list(&mut self, key: &str) -> Result<&'a [Value], Unread> {
        match self.take(key)? {
            Value::Array(items) => Ok(items),
            _ => Err(Unread(format!("{}'s `{key}` is not a list", self.what))),
        }
    }

    /// The two fields `first` and `second`, both whole numbers, or neither.
    fn pair(&mut self, first: &str, second: &str) -> Result<Option<(i64, i64)>, Unread> {
        match (
            self.left.contains_key(first),
            self.left.contains_key(second),
        ) {
            (false, false) => Ok(None),
            (true, true) => Ok(Some((self.integer(first)?, self.integer(second)?))),
            _ => Err(Unread(format!(
                "{} holds one of `{first}` and `{second}` without the other",
                self.what
            ))),
        }
    }

    /// Nothing is left that was not taken.
    fn done(self) -> Result<(), Unread> {
        match self.left.keys().next() {
            None => Ok(()),
            Some(key) => Err(Unread(format!(
                "{} holds `{}`, which this build does not read",
                self.what,
                key.escape_debug()
            ))),
        }
    }
}

impl Window {
    /// The window `value` writes.
    fn read(value: &Value) -> Result<Self, Unread> {
        let mut fields = Fields::of("a window", value)?;
        let window = Self {
            id: fields.id("id")?,
            shown: fields.boolean("shown")?,
            pages: fields.ids("pages")?,
            focused: fields.optional_id("focused")?,
            size: fields.pair("width", "height")?,
            place: fields.pair("x", "y")?,
        };
        fields.done()?;
        Ok(window)
    }
}

impl Page {
    /// The page `value` writes.
    fn read(value: &Value) -> Result<Self, Unread> {
        let mut fields = Fields::of("a page", value)?;
        let id = fields.id("id")?;
        let kind = fields.word("kind", &Kind::WORDS)?;
        let topic = fields.optional_text("topic")?;
        let trail = fields
            .list("trail")?
            .iter()
            .map(|step| match step {
                Value::Text(text) => Ok(text.clone()),
                _ => Err(Unread(
                    "a page's `trail` holds what is not a text".to_owned(),
                )),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let scroll = fields.count("scroll")?;
        fields.done()?;
        Ok(Self {
            id,
            kind,
            topic,
            trail,
            scroll,
        })
    }
}

impl Drawer {
    /// The drawer `value` writes.
    fn read(value: &Value) -> Result<Self, Unread> {
        let mut fields = Fields::of("the drawer", value)?;
        let drawer = Self {
            pages: fields.ids("pages")?,
            focused: fields.optional_id("focused")?,
            collapsed: fields.boolean("collapsed")?,
            toward: fields.word("toward", &Place::WORDS)?,
            flipped: fields.boolean("flipped")?,
            share: fields.count("share")?,
        };
        fields.done()?;
        Ok(drawer)
    }
}

impl Asking {
    /// What the prompt asks for, as `value` writes it.
    fn read(value: &Value) -> Result<Self, Unread> {
        let mut fields = Fields::of("what the prompt asks for", value)?;
        let asking = match fields.text("for")?.as_str() {
            "commands" => Self::Commands,
            "parameters" => Self::Parameters {
                command: fields.text("command")?,
                parameter: fields.optional_text("parameter")?,
            },
            other => {
                return Err(Unread(format!(
                    "the prompt asks, by its `for`, for `{}`, which is neither the command \
                     list nor a command's parameters",
                    other.escape_debug()
                )));
            }
        };
        fields.done()?;
        Ok(asking)
    }
}

impl Seat {
    /// The seat `value` writes, and its number.
    fn read(value: &Value) -> Result<(u64, Self), Unread> {
        let mut fields = Fields::of("a seat", value)?;
        let id = fields.id("seat")?;
        let latest = fields.word("latest", &By::WORDS)?;
        let focus = match fields.optional("focus") {
            None => None,
            Some(focus) => {
                let mut focus = Fields::of("a seat's focus", focus)?;
                let read = Focus {
                    window: focus.id("window")?,
                    region: focus.word("region", &Region::WORDS)?,
                };
                focus.done()?;
                Some(read)
            }
        };
        let seat = Self {
            latest,
            focus,
            hover: fields.optional_text("hover")?,
            selection: fields.optional_text("selection")?,
            drag: fields.optional_text("drag")?,
        };
        fields.done()?;
        Ok((id, seat))
    }
}

/// The pages `listed` writes, each under its identifier, which no two share.
fn pages_of(listed: &[Value]) -> Result<BTreeMap<u64, Page>, Unread> {
    let mut pages = BTreeMap::new();
    for page in listed {
        let page = Page::read(page)?;
        if let Some(twice) = pages.insert(page.id, page) {
            return Err(Unread(format!("two pages are page {}", twice.id)));
        }
    }
    Ok(pages)
}

/// Whether `value` is `{"shown": <bool>}`, and which, for `what`.
fn shown(what: &'static str, value: &Value) -> Result<bool, Unread> {
    let mut fields = Fields::of(what, value)?;
    let shown = fields.boolean("shown")?;
    fields.done()?;
    Ok(shown)
}

impl Shell {
    /// The state `value` writes, read as the kernel writes it.
    ///
    /// # Errors
    ///
    /// [`Unread`], saying which part is not what the kernel writes: a field
    /// missing or of another type, a word outside its closed set, a field this
    /// build does not read, or two pages or two seats under one identifier.
    pub fn read(value: &Value) -> Result<Self, Unread> {
        let mut top = Fields::of("the state", value)?;
        let windows = top
            .list("windows")?
            .iter()
            .map(Window::read)
            .collect::<Result<Vec<_>, _>>()?;
        let pages = pages_of(top.list("pages")?)?;
        let drawer = Drawer::read(top.take("drawer")?)?;
        let sidebar = shown("the sidebar", top.take("sidebar")?)?;
        let footer = shown("the footer", top.take("footer")?)?;
        let mut info = Fields::of("the info line", top.take("info")?)?;
        let expanded = info.boolean("expanded")?;
        let message = info.optional_text("message")?;
        info.done()?;
        let mut prompt = Fields::of("the prompt", top.take("prompt")?)?;
        let draft = prompt.text("draft")?;
        let asking = prompt.optional("asking").map(Asking::read).transpose()?;
        prompt.done()?;
        let mut seats = BTreeMap::new();
        for seat in top.list("seats")? {
            let (id, seat) = Seat::read(seat)?;
            if seats.insert(id, seat).is_some() {
                return Err(Unread(format!("two seats are seat {id}")));
            }
        }
        let next = top.id("next")?;
        top.done()?;
        Ok(Self {
            windows,
            pages,
            drawer,
            sidebar,
            footer,
            expanded,
            message,
            draft,
            asking,
            seats,
            next,
        })
    }
}

/// A fact this window says at the rate it draws, as the arguments of the
/// command that makes the same change (`NOCTUA.md §12.1`).
///
/// **Each names what it is about in full** — the seat, the window, the page —
/// so the daemon defaults nothing for it, and the mirror shows it over the
/// state as it was said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fact {
    /// What the prompt holds: `prompt.set`.
    Draft(String),
    /// Where a seat's keyboard is: `shell.focus`.
    Focus {
        /// The seat.
        seat: u64,
        /// Where its keyboard is.
        on: Focus,
    },
    /// What a seat's pointer hovers or its keyboard selects, or nothing:
    /// `shell.hover`.
    Hover {
        /// The seat.
        seat: u64,
        /// The pointer, or the keyboard.
        by: By,
        /// What it is on, or nothing.
        target: Option<String>,
    },
    /// How far a page is scrolled: `page.scroll`.
    Scroll {
        /// The page.
        page: u64,
        /// How far.
        to: u32,
    },
}

/// What a fact is about, so a later one about the same replaces it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum About {
    /// The prompt's draft.
    Draft,
    /// A seat's keyboard.
    Focus(u64),
    /// A seat's pointer, or its keyboard selection.
    Hover(u64, Hand),
    /// A page's scroll.
    Scroll(u64),
}

/// Which of a seat's two hands a hover fact is about: [`By`], ordered, for a
/// key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Hand {
    /// The pointer.
    Pointer,
    /// The keyboard.
    Keyboard,
}

impl Fact {
    /// The name a note says it under: `nc_kernel`'s facts.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Draft(_) => "draft",
            Self::Focus { .. } => "focus",
            Self::Hover { .. } => "hover",
            Self::Scroll { .. } => "scroll",
        }
    }

    /// The arguments of the command it is the equivalent of.
    #[must_use]
    pub fn arguments(&self) -> Value {
        let id = |id: u64| Value::Integer(i64::try_from(id).unwrap_or(i64::MAX));
        match self {
            Self::Draft(text) => Value::object([("text", Value::text(text.as_str()))]),
            Self::Focus { seat, on } => Value::object([
                ("seat", id(*seat)),
                ("window", id(on.window)),
                ("region", Value::text(on.region.word())),
            ]),
            Self::Hover { seat, by, target } => {
                let mut fields = vec![("seat", id(*seat)), ("by", Value::text(by.word()))];
                if let Some(target) = target {
                    fields.push(("target", Value::text(target.as_str())));
                }
                Value::object(fields)
            }
            Self::Scroll { page, to } => {
                Value::object([("page", id(*page)), ("to", Value::Integer(i64::from(*to)))])
            }
        }
    }

    /// What it is about.
    const fn about(&self) -> About {
        match self {
            Self::Draft(_) => About::Draft,
            Self::Focus { seat, .. } => About::Focus(*seat),
            Self::Hover { seat, by, .. } => About::Hover(
                *seat,
                match by {
                    By::Pointer => Hand::Pointer,
                    By::Keyboard => Hand::Keyboard,
                },
            ),
            Self::Scroll { page, .. } => About::Scroll(*page),
        }
    }
}

/// Where a fact said is, on its way to the daemon's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// Said, and in no note yet.
    Said,
    /// In a note the daemon has not answered.
    Told,
    /// Taken by the daemon while the mirror held the revision here: the
    /// state from the next revision on holds it.
    Taken(Option<u64>),
}

/// How a watch moved the mirror.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moved {
    /// The state is the one held: the wait ended with nothing changed.
    Unchanged,
    /// The state is the one at this revision, held now.
    To(u64),
}

/// The interface's state as this window holds it.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Mirror {
    /// The revision held, which the next watch names as seen.
    revision: Option<u64>,
    /// The state at that revision.
    shell: Option<Shell>,
    /// The facts said, each by what it is about, with where it is.
    said: BTreeMap<About, (Fact, Stage)>,
}

impl Mirror {
    /// A mirror holding nothing: what a window holds before the daemon
    /// answered its first watch.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The revision held, which the next watch names as seen.
    #[must_use]
    pub const fn revision(&self) -> Option<u64> {
        self.revision
    }

    /// The daemon's state, as the last watch answered it.
    #[must_use]
    pub const fn shell(&self) -> Option<&Shell> {
        self.shell.as_ref()
    }

    /// Takes what a watch answered.
    ///
    /// An answer holding a state replaces the one held — once it is read —
    /// and retires every fact the daemon took before it; one holding none
    /// says the state is the one held.
    ///
    /// # Errors
    ///
    /// [`Unread`] for a state this build does not read, and the mirror is
    /// left as it was.
    pub fn watched(&mut self, changed: Changed) -> Result<Moved, Unread> {
        let Some(state) = changed.state else {
            return Ok(Moved::Unchanged);
        };
        let shell = Shell::read(&state)?;
        self.revision = Some(changed.revision);
        self.shell = Some(shell);
        self.said.retain(|_, (_, stage)| match stage {
            Stage::Taken(held) => held.is_some_and(|held| changed.revision <= held),
            Stage::Said | Stage::Told => true,
        });
        Ok(Moved::To(changed.revision))
    }

    /// Takes `fact`, which this window says: shown at once, over the state,
    /// in place of anything said before about the same.
    pub fn said(&mut self, fact: Fact) {
        self.said.insert(fact.about(), (fact, Stage::Said));
    }

    /// The facts said and not yet in a note, one of each name — the most a
    /// note carries — marked told, as the one note that tells them.
    ///
    /// **Nothing while a note is unanswered**, so one note is in flight at a
    /// time and the daemon's answer to it says which facts it took.
    ///
    /// # Errors
    ///
    /// The note's own refusal, for facts whose arguments are past
    /// `ipc_note_max_bytes`; every fact in it is then taken back.
    pub fn to_tell(&mut self) -> Option<Result<nc_ipc::Note, nc_ipc::IpcError>> {
        if self.said.values().any(|(_, stage)| *stage == Stage::Told) {
            return None;
        }
        let mut names: Vec<&'static str> = Vec::new();
        let mut detail: Vec<(&'static str, Value)> = Vec::new();
        for (fact, stage) in self.said.values_mut() {
            if *stage == Stage::Said && !names.contains(&fact.name()) {
                names.push(fact.name());
                detail.push((fact.name(), fact.arguments()));
                *stage = Stage::Told;
            }
        }
        if detail.is_empty() {
            return None;
        }
        let note = nc_ipc::Note::new(Value::object(detail));
        if note.is_err() {
            self.told(false);
        }
        Some(note)
    }

    /// Takes the daemon's answer to the note in flight: `taken`, and every
    /// fact it told is held over the state until a watch answers past the
    /// revision held now; refused, and every one of them is taken back.
    pub fn told(&mut self, taken: bool) {
        let held = self.revision;
        self.said
            .retain(|_, (_, stage)| *stage != Stage::Told || taken);
        for (_, stage) in self.said.values_mut() {
            if *stage == Stage::Told {
                *stage = Stage::Taken(held);
            }
        }
    }

    /// Forgets the revision held and every fact said: a connection to a
    /// daemon begins having seen nothing, and the new daemon heard none of
    /// them. The state is kept, to draw until the new daemon answers.
    pub fn reconnected(&mut self) {
        self.revision = None;
        self.said.clear();
    }

    /// What the prompt holds: the draft this window said, or the state's.
    #[must_use]
    pub fn draft(&self) -> Option<&str> {
        match self.said.get(&About::Draft) {
            Some((Fact::Draft(text), _)) => Some(text),
            _ => self.shell.as_ref().map(|shell| shell.draft.as_str()),
        }
    }

    /// How far `page` is scrolled: as this window said, or as the state
    /// holds it; nothing for a page neither names.
    #[must_use]
    pub fn scroll(&self, page: u64) -> Option<u32> {
        match self.said.get(&About::Scroll(page)) {
            Some((Fact::Scroll { to, .. }, _)) => Some(*to),
            _ => self
                .shell
                .as_ref()
                .and_then(|shell| shell.pages.get(&page))
                .map(|page| page.scroll),
        }
    }

    /// Where `seat`'s keyboard is: as this window said, or as the state
    /// holds it.
    #[must_use]
    pub fn focus(&self, seat: u64) -> Option<Focus> {
        match self.said.get(&About::Focus(seat)) {
            Some((Fact::Focus { on, .. }, _)) => Some(*on),
            _ => self
                .shell
                .as_ref()
                .and_then(|shell| shell.seats.get(&seat))
                .and_then(|seat| seat.focus),
        }
    }

    /// What `seat`'s pointer hovers, or its keyboard selects, by `by`: as
    /// this window said, or as the state holds it.
    #[must_use]
    pub fn hovered(&self, seat: u64, by: By) -> Option<&str> {
        let hand = match by {
            By::Pointer => Hand::Pointer,
            By::Keyboard => Hand::Keyboard,
        };
        match self.said.get(&About::Hover(seat, hand)) {
            Some((Fact::Hover { target, .. }, _)) => target.as_deref(),
            _ => self
                .shell
                .as_ref()
                .and_then(|shell| shell.seats.get(&seat))
                .and_then(|seat| match by {
                    By::Pointer => seat.hover.as_deref(),
                    By::Keyboard => seat.selection.as_deref(),
                }),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use nc_canon::Value;
    use nc_ipc::client::Changed;

    use super::{Asking, By, Fact, Focus, Kind, Mirror, Moved, Place, Region, Shell, Unread};

    /// The first run's state, as the kernel writes it, its windows `windows`.
    pub(crate) fn a_state_with_windows(windows: Vec<Value>) -> Value {
        Value::object([
            ("windows", Value::Array(windows)),
            ("pages", Value::Array(Vec::new())),
            (
                "drawer",
                Value::object([
                    ("pages", Value::Array(Vec::new())),
                    ("collapsed", Value::Bool(false)),
                    ("toward", Value::text("workspace")),
                    ("flipped", Value::Bool(false)),
                    ("share", Value::Integer(33)),
                ]),
            ),
            ("sidebar", Value::object([("shown", Value::Bool(false))])),
            ("footer", Value::object([("shown", Value::Bool(true))])),
            ("info", Value::object([("expanded", Value::Bool(false))])),
            ("prompt", Value::object([("draft", Value::text(""))])),
            ("seats", Value::Array(Vec::new())),
            ("next", Value::Integer(2)),
        ])
    }

    /// The first run's state, as the kernel writes it: one window, hidden.
    fn first_run() -> Value {
        a_state_with_windows(vec![Value::object([
            ("id", Value::Integer(1)),
            ("shown", Value::Bool(false)),
            ("pages", Value::Array(Vec::new())),
        ])])
    }

    /// A state holding every field the kernel writes, each at a value the
    /// first run's state does not hold, so a field read wrongly cannot pass
    /// for its default.
    fn varied() -> Value {
        let ids = |ids: &[i64]| Value::Array(ids.iter().map(|id| Value::Integer(*id)).collect());
        Value::object([
            (
                "windows",
                Value::Array(vec![
                    Value::object([
                        ("id", Value::Integer(1)),
                        ("shown", Value::Bool(true)),
                        ("pages", ids(&[3, 4])),
                        ("focused", Value::Integer(4)),
                        ("width", Value::Integer(800)),
                        ("height", Value::Integer(600)),
                        ("x", Value::Integer(-20)),
                        ("y", Value::Integer(40)),
                    ]),
                    Value::object([
                        ("id", Value::Integer(2)),
                        ("shown", Value::Bool(false)),
                        ("pages", ids(&[])),
                    ]),
                ]),
            ),
            (
                "pages",
                Value::Array(vec![
                    Value::object([
                        ("id", Value::Integer(3)),
                        ("kind", Value::text("settings")),
                        ("topic", Value::text("mcp")),
                        ("trail", Value::Array(vec![Value::text("on")])),
                        ("scroll", Value::Integer(120)),
                    ]),
                    Value::object([
                        ("id", Value::Integer(4)),
                        ("kind", Value::text("home")),
                        ("trail", Value::Array(Vec::new())),
                        ("scroll", Value::Integer(0)),
                    ]),
                    Value::object([
                        ("id", Value::Integer(5)),
                        ("kind", Value::text("leader")),
                        ("trail", Value::Array(Vec::new())),
                        ("scroll", Value::Integer(0)),
                    ]),
                ]),
            ),
            (
                "drawer",
                Value::object([
                    ("pages", ids(&[5])),
                    ("focused", Value::Integer(5)),
                    ("collapsed", Value::Bool(true)),
                    ("toward", Value::text("drawer")),
                    ("flipped", Value::Bool(true)),
                    ("share", Value::Integer(49)),
                ]),
            ),
            ("sidebar", Value::object([("shown", Value::Bool(true))])),
            ("footer", Value::object([("shown", Value::Bool(false))])),
            (
                "info",
                Value::object([
                    ("expanded", Value::Bool(true)),
                    ("message", Value::text("said")),
                ]),
            ),
            (
                "prompt",
                Value::object([
                    ("draft", Value::text(";set")),
                    (
                        "asking",
                        Value::object([
                            ("for", Value::text("parameters")),
                            ("command", Value::text("settings.mcp.set")),
                            ("parameter", Value::text("on")),
                        ]),
                    ),
                ]),
            ),
            (
                "seats",
                Value::Array(vec![Value::object([
                    ("seat", Value::Integer(2)),
                    ("latest", Value::text("keyboard")),
                    (
                        "focus",
                        Value::object([
                            ("window", Value::Integer(1)),
                            ("region", Value::text("modeline")),
                        ]),
                    ),
                    ("hover", Value::text("pointed")),
                    ("selection", Value::text("chosen")),
                    ("drag", Value::text("dragged")),
                ])]),
            ),
            ("next", Value::Integer(6)),
        ])
    }

    fn read(value: &Value) -> Shell {
        Shell::read(value).expect("the state is read")
    }

    /// `value` with the object at `path` changed by `edit`.
    fn edited(value: &Value, path: &[&str], edit: impl FnOnce(&mut Value)) -> Value {
        let mut value = value.clone();
        let mut at = &mut value;
        for step in path {
            at = match at {
                Value::Object(fields) => fields.get_mut(*step).expect("the path is there"),
                Value::Array(items) => items
                    .get_mut(step.parse::<usize>().expect("an index"))
                    .expect("the index is there"),
                _ => panic!("the path leads through a leaf"),
            };
        }
        edit(at);
        value
    }

    /// `value` with `key` set in the object at `path`.
    fn with(value: &Value, path: &[&str], key: &str, set: Value) -> Value {
        edited(value, path, |at| {
            let Value::Object(fields) = at else {
                panic!("the path leads to an object")
            };
            fields.insert(key.to_owned(), set);
        })
    }

    /// `value` without `key` in the object at `path`.
    fn without(value: &Value, path: &[&str], key: &str) -> Value {
        edited(value, path, |at| {
            let Value::Object(fields) = at else {
                panic!("the path leads to an object")
            };
            assert!(fields.remove(key).is_some(), "{key} was there");
        })
    }

    /// The first run's state reads as the first run: one window, hidden,
    /// nothing open, the footer shown.
    #[test]
    fn the_first_run_reads_as_one_window_hidden_and_the_footer_shown() {
        let shell = read(&first_run());
        assert_eq!(shell.windows.len(), 1);
        let window = &shell.windows[0];
        assert_eq!((window.id, window.shown), (1, false));
        assert!(window.pages.is_empty() && window.focused.is_none());
        assert_eq!((window.size, window.place), (None, None));
        assert!(shell.pages.is_empty() && shell.drawer.pages.is_empty());
        assert!(shell.footer && !shell.sidebar && !shell.expanded);
        assert_eq!((shell.draft.as_str(), shell.asking.clone()), ("", None));
        assert_eq!(shell.drawer.toward, Place::Workspace);
        assert_eq!((shell.drawer.share, shell.next), (33, 2));
    }

    /// Every field the kernel writes is read as itself, at a value the first
    /// run's state does not hold.
    #[test]
    fn every_field_is_read_as_itself() {
        let shell = read(&varied());
        let first = &shell.windows[0];
        assert_eq!((first.id, first.shown), (1, true));
        assert_eq!((first.pages.clone(), first.focused), (vec![3, 4], Some(4)));
        assert_eq!(
            (first.size, first.place),
            (Some((800, 600)), Some((-20, 40)))
        );
        assert_eq!(shell.windows[1].id, 2);
        let settings = &shell.pages[&3];
        assert_eq!(settings.kind, Kind::Settings);
        assert_eq!(settings.topic.as_deref(), Some("mcp"));
        assert_eq!(settings.trail, vec!["on".to_owned()]);
        assert_eq!(settings.scroll, 120);
        assert_eq!(
            (shell.pages[&4].kind, shell.pages[&5].kind),
            (Kind::Home, Kind::Leader)
        );
        assert_eq!(shell.pages[&4].topic, None);
        let drawer = &shell.drawer;
        assert_eq!((drawer.pages.clone(), drawer.focused), (vec![5], Some(5)));
        assert!(drawer.collapsed && drawer.flipped);
        assert_eq!((drawer.toward, drawer.share), (Place::Drawer, 49));
        assert!(shell.sidebar && !shell.footer && shell.expanded);
        assert_eq!(shell.message.as_deref(), Some("said"));
        assert_eq!(shell.draft, ";set");
        assert_eq!(
            shell.asking,
            Some(Asking::Parameters {
                command: "settings.mcp.set".to_owned(),
                parameter: Some("on".to_owned()),
            })
        );
        let seat = &shell.seats[&2];
        assert_eq!(seat.latest, By::Keyboard);
        assert_eq!(
            seat.focus,
            Some(Focus {
                window: 1,
                region: Region::Modeline,
            })
        );
        assert_eq!(seat.hover.as_deref(), Some("pointed"));
        assert_eq!(seat.selection.as_deref(), Some("chosen"));
        assert_eq!(seat.drag.as_deref(), Some("dragged"));
        assert_eq!(shell.next, 6);
        let asking = with(
            &first_run(),
            &["prompt"],
            "asking",
            Value::object([("for", Value::text("commands"))]),
        );
        assert_eq!(read(&asking).asking, Some(Asking::Commands));
    }

    /// Every word of a closed set is read, as the kernel writes it.
    #[test]
    fn every_word_of_each_closed_set_is_read() {
        for (kind, word) in Kind::WORDS {
            let state = with(&varied(), &["pages", "1"], "kind", Value::text(word));
            assert_eq!(read(&state).pages[&4].kind, kind, "{word}");
        }
        for (region, word) in Region::WORDS {
            assert_eq!(
                region.word(),
                word,
                "a fact writes the word the state is read by"
            );
            let state = with(
                &varied(),
                &["seats", "0", "focus"],
                "region",
                Value::text(word),
            );
            let focus = read(&state).seats[&2].focus.expect("a focus");
            assert_eq!(focus.region, region, "{word}");
        }
        for (by, word) in By::WORDS {
            assert_eq!(
                by.word(),
                word,
                "a fact writes the word the state is read by"
            );
            let state = with(&varied(), &["seats", "0"], "latest", Value::text(word));
            assert_eq!(read(&state).seats[&2].latest, by, "{word}");
        }
        for (place, word) in Place::WORDS {
            let state = with(&varied(), &["drawer"], "toward", Value::text(word));
            assert_eq!(read(&state).drawer.toward, place, "{word}");
        }
    }

    /// A field this build does not read is refused, wherever it is.
    #[test]
    fn a_field_this_build_does_not_read_is_refused_wherever_it_is() {
        for path in [
            &[][..],
            &["windows", "0"],
            &["pages", "0"],
            &["drawer"],
            &["sidebar"],
            &["footer"],
            &["info"],
            &["prompt"],
            &["prompt", "asking"],
            &["seats", "0"],
            &["seats", "0", "focus"],
        ] {
            let state = with(&varied(), path, "alert", Value::Bool(true));
            let refused = Shell::read(&state).expect_err("an unread field is refused");
            assert!(
                refused.to_string().contains("`alert`"),
                "{path:?}: {refused}"
            );
        }
    }

    /// A field the kernel always writes is refused when it is missing.
    #[test]
    fn a_field_the_kernel_always_writes_is_refused_missing() {
        for (path, key) in [
            (&[][..], "windows"),
            (&[], "pages"),
            (&[], "drawer"),
            (&[], "sidebar"),
            (&[], "footer"),
            (&[], "info"),
            (&[], "prompt"),
            (&[], "seats"),
            (&[], "next"),
            (&["windows", "0"], "id"),
            (&["windows", "0"], "shown"),
            (&["windows", "0"], "pages"),
            (&["pages", "0"], "id"),
            (&["pages", "0"], "kind"),
            (&["pages", "0"], "trail"),
            (&["pages", "0"], "scroll"),
            (&["drawer"], "pages"),
            (&["drawer"], "collapsed"),
            (&["drawer"], "toward"),
            (&["drawer"], "flipped"),
            (&["drawer"], "share"),
            (&["sidebar"], "shown"),
            (&["footer"], "shown"),
            (&["info"], "expanded"),
            (&["prompt"], "draft"),
            (&["prompt", "asking"], "for"),
            (&["prompt", "asking"], "command"),
            (&["seats", "0"], "seat"),
            (&["seats", "0"], "latest"),
            (&["seats", "0", "focus"], "window"),
            (&["seats", "0", "focus"], "region"),
        ] {
            let state = without(&varied(), path, key);
            let refused = Shell::read(&state).expect_err("a missing field is refused");
            assert!(
                refused.to_string().contains(&format!("`{key}`")),
                "{path:?} {key}: {refused}"
            );
        }
    }

    /// A field of another type, a word outside its set, an identifier below
    /// one, a size or a place half written and two parts under one
    /// identifier are each refused.
    #[test]
    fn a_value_of_another_shape_is_refused() {
        let refused = |state: &Value| Shell::read(state).expect_err("refused");
        for (path, key, set) in [
            (&["windows", "0"][..], "shown", Value::text("true")),
            (&["windows", "0"], "id", Value::Integer(0)),
            (
                &["windows", "0"],
                "pages",
                Value::Array(vec![Value::text("3")]),
            ),
            (&["windows", "0"], "focused", Value::Integer(-1)),
            (&["pages", "0"], "kind", Value::text("terminal")),
            (&["pages", "0"], "scroll", Value::Integer(-1)),
            (&["pages", "0"], "scroll", Value::Integer(1 << 32)),
            (
                &["pages", "0"],
                "trail",
                Value::Array(vec![Value::Integer(1)]),
            ),
            (&["pages", "0"], "topic", Value::Integer(1)),
            (&["drawer"], "toward", Value::text("sidebar")),
            (&["drawer"], "share", Value::Bool(true)),
            (&["info"], "message", Value::Null),
            (&["prompt"], "draft", Value::Integer(0)),
            (&["prompt", "asking"], "for", Value::text("everything")),
            (&["seats", "0"], "latest", Value::text("touch")),
            (&["seats", "0", "focus"], "region", Value::text("footer")),
            (&[], "next", Value::Integer(0)),
            (&[], "windows", Value::object([("id", Value::Integer(1))])),
        ] {
            let state = with(&varied(), path, key, set);
            let said = refused(&state).to_string();
            assert!(said.contains(&format!("`{key}`")), "{path:?} {key}: {said}");
        }
        for half in [["width"], ["height"], ["x"], ["y"]] {
            let state = without(&varied(), &["windows", "0"], half[0]);
            let said = refused(&state).to_string();
            assert!(said.contains("without the other"), "{half:?}: {said}");
        }
        let twice = edited(&varied(), &["pages"], |pages| {
            let Value::Array(items) = pages else {
                panic!("a list")
            };
            items.push(items[0].clone());
        });
        assert!(refused(&twice).to_string().contains("two pages are page 3"));
        let twice = edited(&varied(), &["seats"], |seats| {
            let Value::Array(items) = seats else {
                panic!("a list")
            };
            items.push(items[0].clone());
        });
        assert!(refused(&twice).to_string().contains("two seats are seat 2"));
        assert!(
            refused(&Value::Array(Vec::new()))
                .to_string()
                .contains("not one object")
        );
    }

    fn changed(revision: u64, state: Option<Value>) -> Changed {
        Changed { revision, state }
    }

    /// A watch that holds a state moves the mirror to it; one that holds none
    /// leaves it; one this build does not read leaves the state and the
    /// revision held before it, saying why.
    #[test]
    fn a_watch_moves_the_mirror_to_a_state_it_reads_and_to_nothing_else() {
        let mut mirror = Mirror::new();
        assert_eq!((mirror.revision(), mirror.shell()), (None, None));
        assert_eq!(
            mirror.watched(changed(4, Some(first_run()))),
            Ok(Moved::To(4))
        );
        assert_eq!(mirror.revision(), Some(4));
        assert_eq!(mirror.shell(), Some(&read(&first_run())));
        assert_eq!(mirror.watched(changed(4, None)), Ok(Moved::Unchanged));
        assert_eq!(mirror.revision(), Some(4));
        let unread = with(&varied(), &[], "alert", Value::Bool(true));
        let refused: Unread = mirror
            .watched(changed(5, Some(unread)))
            .expect_err("refused");
        assert!(
            refused
                .to_string()
                .contains("the state held before it is kept"),
            "{refused}"
        );
        assert_eq!(mirror.revision(), Some(4));
        assert_eq!(mirror.shell(), Some(&read(&first_run())));
        assert_eq!(mirror.watched(changed(6, Some(varied()))), Ok(Moved::To(6)));
        assert_eq!(mirror.shell(), Some(&read(&varied())));
    }

    /// Each fact is said as the arguments of the command that makes the same
    /// change, under the name a note says it by.
    #[test]
    fn each_fact_is_the_arguments_of_its_command() {
        let focus = Fact::Focus {
            seat: 1,
            on: Focus {
                window: 2,
                region: Region::Prompt,
            },
        };
        for (fact, name, arguments) in [
            (Fact::Draft("x".to_owned()), "draft", r#"{"text":"x"}"#),
            (focus, "focus", r#"{"region":"prompt","seat":1,"window":2}"#),
            (
                Fact::Hover {
                    seat: 3,
                    by: By::Keyboard,
                    target: Some("row".to_owned()),
                },
                "hover",
                r#"{"by":"keyboard","seat":3,"target":"row"}"#,
            ),
            (
                Fact::Hover {
                    seat: 3,
                    by: By::Pointer,
                    target: None,
                },
                "hover",
                r#"{"by":"pointer","seat":3}"#,
            ),
            (
                Fact::Scroll { page: 7, to: 900 },
                "scroll",
                r#"{"page":7,"to":900}"#,
            ),
        ] {
            assert_eq!(fact.name(), name);
            assert_eq!(fact.arguments().to_canonical(), arguments, "{name}");
        }
    }

    /// A fact said is shown over the state at once, and the state's own
    /// value is shown where nothing was said.
    #[test]
    fn a_fact_said_is_shown_over_the_state_at_once() {
        let mut mirror = Mirror::new();
        assert_eq!(mirror.draft(), None);
        mirror.watched(changed(1, Some(varied()))).expect("read");
        assert_eq!(mirror.draft(), Some(";set"));
        assert_eq!(mirror.scroll(3), Some(120));
        assert_eq!(mirror.scroll(9), None);
        assert_eq!(mirror.hovered(2, By::Pointer), Some("pointed"));
        assert_eq!(mirror.hovered(2, By::Keyboard), Some("chosen"));
        let on = Focus {
            window: 1,
            region: Region::Prompt,
        };
        mirror.said(Fact::Draft("typed".to_owned()));
        mirror.said(Fact::Scroll { page: 3, to: 7 });
        mirror.said(Fact::Focus { seat: 2, on });
        mirror.said(Fact::Hover {
            seat: 2,
            by: By::Pointer,
            target: None,
        });
        assert_eq!(mirror.draft(), Some("typed"));
        assert_eq!(mirror.scroll(3), Some(7));
        assert_eq!(mirror.focus(2), Some(on));
        assert_eq!(
            mirror.hovered(2, By::Pointer),
            None,
            "said: nothing hovered"
        );
        assert_eq!(mirror.hovered(2, By::Keyboard), Some("chosen"), "not said");
        mirror.said(Fact::Draft("typed more".to_owned()));
        assert_eq!(
            mirror.draft(),
            Some("typed more"),
            "the later said replaces"
        );
    }

    /// One note tells one fact of each name, and none is told while a note
    /// is unanswered.
    #[test]
    fn one_note_tells_one_fact_of_each_name_and_one_note_is_in_flight() {
        let mut mirror = Mirror::new();
        assert!(mirror.to_tell().is_none(), "nothing said");
        mirror.said(Fact::Draft("a".to_owned()));
        mirror.said(Fact::Scroll { page: 3, to: 1 });
        mirror.said(Fact::Scroll { page: 4, to: 2 });
        let note = mirror.to_tell().expect("a note").expect("within its bound");
        assert_eq!(
            note.detail().to_canonical(),
            r#"{"draft":{"text":"a"},"scroll":{"page":3,"to":1}}"#
        );
        assert!(mirror.to_tell().is_none(), "a note is in flight");
        mirror.told(true);
        let note = mirror
            .to_tell()
            .expect("the second scroll")
            .expect("within its bound");
        assert_eq!(
            note.detail().to_canonical(),
            r#"{"scroll":{"page":4,"to":2}}"#
        );
    }

    /// A fact the daemon took is shown until a watch answers past the
    /// revision held when it took it; one in a note the daemon refused is
    /// taken back at once.
    #[test]
    fn a_fact_taken_retires_past_its_revision_and_one_refused_is_taken_back() {
        let mut mirror = Mirror::new();
        mirror.watched(changed(1, Some(first_run()))).expect("read");
        mirror.said(Fact::Draft("taken".to_owned()));
        let _ = mirror.to_tell();
        mirror.told(true);
        mirror.watched(changed(1, Some(first_run()))).expect("read");
        assert_eq!(
            mirror.draft(),
            Some("taken"),
            "the revision held when it was taken"
        );
        let later = with(
            &first_run(),
            &["prompt"],
            "draft",
            Value::text("someone else's"),
        );
        mirror.watched(changed(2, Some(later))).expect("read");
        assert_eq!(
            mirror.draft(),
            Some("someone else's"),
            "retired past its revision"
        );
        mirror.said(Fact::Draft("refused".to_owned()));
        let _ = mirror.to_tell();
        assert_eq!(mirror.draft(), Some("refused"), "shown while in flight");
        mirror.told(false);
        assert_eq!(mirror.draft(), Some("someone else's"), "taken back");
        mirror.said(Fact::Draft("said".to_owned()));
        mirror.watched(changed(9, Some(first_run()))).expect("read");
        assert_eq!(
            mirror.draft(),
            Some("said"),
            "a fact not told is not retired"
        );
    }

    /// A fact taken before the daemon answered any watch retires at the first
    /// state that arrives.
    #[test]
    fn a_fact_taken_before_any_state_retires_at_the_first() {
        let mut mirror = Mirror::new();
        mirror.said(Fact::Draft("early".to_owned()));
        let _ = mirror.to_tell();
        mirror.told(true);
        assert_eq!(mirror.draft(), Some("early"));
        mirror.watched(changed(1, Some(first_run()))).expect("read");
        assert_eq!(mirror.draft(), Some(""));
    }

    /// A connection made again forgets the revision and every fact said, and
    /// keeps the state to draw until the new daemon answers.
    #[test]
    fn a_connection_made_again_forgets_the_revision_and_the_facts() {
        let mut mirror = Mirror::new();
        mirror.watched(changed(5, Some(varied()))).expect("read");
        mirror.said(Fact::Draft("unheard".to_owned()));
        mirror.reconnected();
        assert_eq!(mirror.revision(), None);
        assert_eq!(mirror.draft(), Some(";set"));
        assert!(mirror.to_tell().is_none(), "nothing is left to tell");
        assert_eq!(mirror.shell(), Some(&read(&varied())));
    }

    /// A note past its bound is refused, and every fact in it taken back.
    #[test]
    fn a_note_past_its_bound_is_refused_and_its_facts_taken_back() {
        let mut mirror = Mirror::new();
        let long =
            "x".repeat(usize::try_from(nc_catalog::DAEMON.ipc_note_max_bytes).expect("a size"));
        mirror.said(Fact::Draft(long));
        assert!(mirror.to_tell().expect("a note").is_err());
        assert_eq!(mirror.draft(), None, "taken back");
        assert!(mirror.to_tell().is_none());
    }
}
