//! The interface's own state: what every window of the desktop interface
//! shows, held by the kernel (`NOCTUA.md §12.1`).
//!
//! **The interface is a view, and this is what it is a view of.** The kernel
//! holds, for the one realm it serves:
//!
//! | Part | What it holds |
//! |---|---|
//! | windows | each window, shown or hidden, its size and place once set, and its stack of pages — the workspace area, one workspace until Stage 17 |
//! | pages | every page open in any window or in the drawer: its kind, its topic, the sub-pages it went into and how far it is scrolled |
//! | drawer | the pages open in it and the one in focus, whether it is collapsed, where a page opened from the leader menu goes, and its share of the height |
//! | sidebar | whether it is shown |
//! | footer | whether it is shown now, where the setting hides it: a reveal a Leader tap begins and a keyboard leaving the footer and the drawer ends |
//! | info line | the message it says, and whether it is expanded |
//! | prompt | what it holds, and what it asks for: the command list, or a command's parameters |
//! | seats | each seat's focus, what its pointer hovers, what its keyboard selection is on, which of the two moved last, and what it drags |
//! | settings | the three settings of how the interface is shown: the language it speaks, whether the footer is always shown, and the light or dark mode |
//!
//! A page is one of a closed set of kinds: `home`, `settings`, `help`, `dev`
//! and `surface` go anywhere, `settings`, `help` and `dev` with a topic or
//! without — the list, or one setting, one help topic, one dev page — and
//! `surface` with the surface it shows; `leader`, `commands`, `shortcuts` and
//! `actions` — the leader menu, the command list, the `/` list and the `w`
//! list — open in the drawer alone (§12.9). One page shown in a window and in
//! the drawer is two views of one page (§12.12): a page is opened once for its
//! kind and topic, and opening it again adds a view of it where it is asked
//! for.
//!
//! **Every change is a command** (§2.8, §20.1): `shell.read` answers the whole
//! state at the revision it is at, and the `shell.`, `window.`, `leader.`,
//! `page.`, `drawer.`, `sidebar.`, `footer.`, `info.` and `prompt.` commands
//! change it — the families [`Verb`] lists, each decided on
//! `Feature::"interface"` (`family.a_member_drives_the_interface`). **Each is a
//! read** (`reversibility = "read"`, `mutating = false`): what a window shows
//! is not what a person's undo takes back, so none opens a unit and none is a
//! step, and an undo passes over every one to the person's last real change.
//! Each is journaled once all the same, its entry naming the command and the
//! feature and never the arguments.
//!
//! **Three settings are a part of this state** ([`Setting`], `NOCTUA.md
//! §12.15`): `settings.language`, `settings.footer` and `settings.appearance`,
//! each a `.read` answering the one object its `.set` takes whole — the
//! language the interface speaks, the system's or one the catalog ships; the
//! footer always shown, or hidden until it is asked for (§12.3); and the mode,
//! the system's, light or dark (§9.6). They depart from a setting kept as an
//! entry of the store (`crate::settings`) in what they are: each is how the
//! interface is shown and nothing a command reads, so its value is held here
//! with the rest of what a window shows, reaches every window with the state
//! it follows, is kept with it between starts, and its set is no step — a
//! read on `Feature::"interface"` like every change of this module's, never
//! on a feature of its own name, with no undo to take it back and the setting
//! set again as the way back. A set still runs at the top of a call tree
//! alone (`crate::registry::only_at_the_top`), so no plugin's command changes
//! how a person's interface speaks. Each value is one of a closed list of
//! words, the first its default, and a word outside it is refused, from a
//! command and from a saved state alike.
//!
//! **What a command changes is held until its call tree ends** (`crate::units`'
//! `Later`): the state it leaves is the call tree's, read back by every command
//! after it in the tree, and the kernel's only once the tree has ended with
//! every entry it gave the journal taken — or, inside a unit, once the unit is
//! kept. A tree whose entry the journal refused releases nothing it holds
//! itself: what it held is let go the moment the entry is refused, and the
//! changes it holds after are never released. A state held is released only
//! over the state it was computed from, so one a unit inside the same tree
//! already released is never written back over by the older one the tree
//! held before it. **A unit's state carries the tree's**: computed over what
//! the tree held when the unit began — a timer's callback, the one tree that
//! holds changes of its own beside a unit — it is released whole when the
//! unit is kept, and the tree lets its own go then
//! (`CallTree::kept_over_its_own`), so the commands after the unit read what
//! it released, and a callback's own is released after its unit, only when
//! no unit was kept holding one. **Except once the journal refused an entry
//! of the tree**: a unit begun then computes over the kernel's state, never
//! over what the tree holds itself (`CallTree::shell_held`), so a unit kept
//! carries no change the journal did not take, nor one the tree will not
//! release. `shell.read` inside a tree that holds a change answers that
//! change, which is not the kernel's yet, with no revision.
//!
//! **Kept between starts, outside every unit.** The pump saves the state to
//! the channel's local store when it changed (`StoreBackend::keep`, under the
//! kernel's own holder and the key [`KEY`], which no unit writes), a stop
//! saves it last, once nothing runs ([`crate::Kernel::keep_interface`]), and
//! the kernel reads it back when it opens. A save the store refuses is logged
//! as `kernel.shell_unkept` and tried again at the next pump — at a stop there
//! is none, and what changed since the last save is lost. **What is read back
//! is never trusted**: a saved state of another shape, or one that breaks any
//! rule this module holds the state to ([`State::holds`]) — a bound, a page no
//! place shows, a seat past `shell_seats_max` — is set aside whole, logged as
//! `kernel.shell_discarded` with the shape it named, and the kernel begins from
//! the first run's state, which shows nothing but the footer (§12.2). Every
//! identifier is one canonical JSON writes: a window or a page past the last
//! one is refused (`[NC-KERN-0028]`), never given a number another holds.
//!
//! **What a Leader tap does is decided here** (§12.8), so the CLI's `noctua
//! leader` and a hotkey do the same: with the leader menu open in the drawer of
//! the seat's window, shown, a tap sends the next page opened to the other
//! region; with the seat's keyboard on the prompt line of a window that is
//! shown — a line seen however the footer is set, since a folded footer opens
//! around the keyboard inside it — it opens the leader menu in the drawer;
//! otherwise — the window hidden with the menu open in it included — it shows
//! the window and puts the seat's keyboard on the prompt line. Every tap shows
//! the footer (§12.3). **A tap never hides the interface**: §12.8 gives a tap
//! no such rule, and a menu no one sees never keeps a tap from showing it.
//!
//! # What this answers (`NOCTUA.md §2.4`)
//!
//! A state that grows without bound — every count is held to its catalog
//! limit, and every text to its own; a page, a window or a seat named that the
//! state does not hold, refused rather than invented; a change the journal did
//! not take, which is never released; and a saved state that is not one this
//! module wrote, which is never read as one. **What it does not answer:** a
//! process running as the same user, which can drive the interface as the
//! person through the CLI or edit the saved state in the profile's database,
//! as it can any of the person's data; and an interface's rate of drawing —
//! what moves at that rate reaches the daemon as a note, which is no command
//! and changes nothing here.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use nc_canon::Value;
use nc_catalog::{ErrorCode, KERNEL};
use nc_ids::OpaqueId;

use crate::backend::StoreBackend;
use crate::dispatch::{self, CallTree, Failure, Refusal, RefusalKind};
use crate::logging::LogSink as _;
use crate::{Environment, Shared};

/// The key the kernel keeps the interface's state under, beside its settings.
pub(crate) const KEY: &str = "shell.state";

/// The shape of the state this build writes and reads: a saved state naming
/// any other is set aside.
///
/// **A change of what a saved state holds is a new shape**, so a state an
/// earlier build kept is set aside as that build's, logged with the shape it
/// named, and never as one of this build's that breaks a rule: shape 1 held
/// no settings of how the interface is shown, and shape 2 holds the three.
pub(crate) const SHAPE: i64 = 2;

/// The most characters a topic, a hover's or a drag's target and a parameter's
/// name may hold, which every schema naming one declares.
pub(crate) const WORD_CHARS: usize = 64;

/// The widest a window may be, and the tallest: the sixteen bits an X window's
/// size is written in.
pub(crate) const WINDOW_MAX: i64 = 65_535;

/// How far either way a window's corner may be placed: an X position's 16 bits.
pub(crate) const PLACE_MAX: i64 = 32_767;

/// The least a window's corner may be placed at.
pub(crate) const PLACE_MIN: i64 = -32_768;

/// The furthest the next identifier goes: the largest integer canonical JSON
/// writes, so every identifier given, and the next, is written as itself.
const NEXT_MAX: u64 = i64::MAX.unsigned_abs();

/// The share of the height the drawer takes until a person sets one, in
/// percent: a third.
const SHARE: u32 = 33;

/// The drawer is always shorter than the workspace area (§12.11): below half
/// of the height the two share, whatever the catalog is edited to.
const _: () = assert!(KERNEL.shell_drawer_share_max < 50);

/// The drawer holds at most what the interface does.
const _: () = assert!(KERNEL.shell_drawer_pages_max <= KERNEL.shell_pages_max);

/// One window at least, which is the one kept hidden and ready.
const _: () = assert!(KERNEL.shell_windows_max >= 1);

/// Where a page is shown: a window's workspace area, or the drawer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Place {
    /// A window's workspace area.
    Workspace,
    /// The drawer, below the footer.
    Drawer,
}

impl Place {
    /// Its word, as the commands and the state write it.
    const fn word(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::Drawer => "drawer",
        }
    }

    /// The place `word` names.
    fn of(word: &str) -> Option<Self> {
        [Self::Workspace, Self::Drawer]
            .into_iter()
            .find(|place| place.word() == word)
    }

    /// The other one.
    const fn other(self) -> Self {
        match self {
            Self::Workspace => Self::Drawer,
            Self::Drawer => Self::Workspace,
        }
    }
}

/// The part of a window a seat's keyboard is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Region {
    /// The workspace area, and the page in focus there.
    Workspace,
    /// The drawer, and the page in focus there.
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
    /// Every one, in the order the schema lists them.
    const ALL: [Self; 6] = [
        Self::Workspace,
        Self::Drawer,
        Self::Prompt,
        Self::Sidebar,
        Self::Info,
        Self::Modeline,
    ];

    /// Its word, as the commands and the state write it.
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

    /// The region `word` names.
    fn of(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|region| region.word() == word)
    }
}

/// What a page is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// The home page.
    Home,
    /// Settings: the list, or one setting's page (§12.15).
    Settings,
    /// Help: the list, or one topic (§12.16).
    Help,
    /// Dev: the list, or one page (§12.16).
    Dev,
    /// A surface a plugin shows (§11.6).
    Surface,
    /// The leader menu (§12.9).
    Leader,
    /// The command list (§12.9).
    Commands,
    /// The focused page's own shortcuts, the `/` list (§12.9).
    Shortcuts,
    /// The container actions, the `w` list (§12.9).
    Actions,
}

/// Whether a kind of page takes a topic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Topic {
    /// Never.
    None,
    /// With one or without: the list, or one of its entries.
    Optional,
    /// Always.
    Required,
}

impl Kind {
    /// Every one, in the order the schema lists them.
    const ALL: [Self; 9] = [
        Self::Home,
        Self::Settings,
        Self::Help,
        Self::Dev,
        Self::Surface,
        Self::Leader,
        Self::Commands,
        Self::Shortcuts,
        Self::Actions,
    ];

    /// Its word, as the commands and the state write it.
    const fn word(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Settings => "settings",
            Self::Help => "help",
            Self::Dev => "dev",
            Self::Surface => "surface",
            Self::Leader => "leader",
            Self::Commands => "commands",
            Self::Shortcuts => "shortcuts",
            Self::Actions => "actions",
        }
    }

    /// The kind `word` names.
    fn of(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.word() == word)
    }

    /// Whether it opens in the drawer alone: the leader menu and the three
    /// lists it and the prompt open (§12.9).
    const fn drawer_only(self) -> bool {
        matches!(
            self,
            Self::Leader | Self::Commands | Self::Shortcuts | Self::Actions
        )
    }

    /// Whether it takes a topic.
    const fn topic(self) -> Topic {
        match self {
            Self::Settings | Self::Help | Self::Dev => Topic::Optional,
            Self::Surface => Topic::Required,
            Self::Home | Self::Leader | Self::Commands | Self::Shortcuts | Self::Actions => {
                Topic::None
            }
        }
    }
}

/// What a seat's pointer or keyboard moved last.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum By {
    /// The pointer: what it hovers.
    Pointer,
    /// The keyboard: what its selection is on.
    Keyboard,
}

impl By {
    /// Its word.
    const fn word(self) -> &'static str {
        match self {
            Self::Pointer => "pointer",
            Self::Keyboard => "keyboard",
        }
    }

    /// The one `word` names.
    fn of(word: &str) -> Option<Self> {
        [Self::Pointer, Self::Keyboard]
            .into_iter()
            .find(|by| by.word() == word)
    }
}

/// A seat's keyboard focus: a window, and the part of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Focus {
    window: u64,
    region: Region,
}

/// One seat: a pointer with a keyboard (§12.18).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Seat {
    focus: Option<Focus>,
    hover: Option<String>,
    selection: Option<String>,
    latest: By,
    drag: Option<String>,
}

impl Default for Seat {
    fn default() -> Self {
        Self {
            focus: None,
            hover: None,
            selection: None,
            latest: By::Pointer,
            drag: None,
        }
    }
}

/// One window of the interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Window {
    id: u64,
    shown: bool,
    /// Its width and height in CSS pixels, once set.
    size: Option<(i64, i64)>,
    /// Where its top left corner is, once set.
    place: Option<(i64, i64)>,
    /// Its stack of pages, in the order opened.
    pages: Vec<u64>,
    /// The page in front.
    focused: Option<u64>,
}

impl Window {
    /// A window numbered `id`, shown or not, with nothing in it.
    const fn new(id: u64, shown: bool) -> Self {
        Self {
            id,
            shown,
            size: None,
            place: None,
            pages: Vec::new(),
            focused: None,
        }
    }
}

/// One page open in the interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Page {
    id: u64,
    kind: Kind,
    topic: Option<String>,
    /// The sub-pages it went into, outermost first.
    trail: Vec<String>,
    /// How far down it is scrolled, in CSS pixels.
    scroll: u32,
}

/// The drawer, below the footer (§12.11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Drawer {
    pages: Vec<u64>,
    focused: Option<u64>,
    collapsed: bool,
    /// Where a page opened from the leader menu goes.
    toward: Place,
    /// Whether a Leader tap sent the next page to the other place.
    flipped: bool,
    /// Its share of the height it shares with the workspace area, in percent.
    share: u32,
}

/// What the prompt line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Asking {
    /// The command list, which the rest of the draft filters.
    Commands,
    /// A command's parameters, and the one the next text is for.
    Parameters {
        command: String,
        parameter: Option<String>,
    },
}

/// The word a setting that follows the system is written as.
const SYSTEM: &str = "system";
/// Every word the language setting may be: the system's, then the tag of
/// every locale the catalog ships, in its order — which a test holds to
/// `nc_catalog::Locale::ALL`, so a locale added there is a word added here.
const LANGUAGES: [&str; 3] = [
    SYSTEM,
    nc_catalog::Locale::En.tag(),
    nc_catalog::Locale::PtBR.tag(),
];
/// The footer setting's word for a footer always shown.
const ALWAYS: &str = "always";
/// The footer setting's word for a footer hidden until it is asked for.
const HIDDEN: &str = "hidden";
/// The one field of the language setting's object.
const LANGUAGE: &str = "language";
/// The one field of the footer setting's object.
const VISIBILITY: &str = "visibility";
/// The one field of the appearance setting's object.
const MODE: &str = "mode";

/// A setting whose value is a part of the interface's state
/// (`NOCTUA.md §12.15`): how the interface is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Setting {
    /// `settings.language`: the language the interface speaks (§10.9).
    Language,
    /// `settings.footer`: whether the footer is always shown (§12.3).
    Footer,
    /// `settings.appearance`: the light or dark mode (§9.6).
    Appearance,
}

impl Setting {
    /// Every one, in the order `§12.15` names them.
    pub(crate) const ALL: [Self; 3] = [Self::Footer, Self::Language, Self::Appearance];

    /// The one field of its object, in what its read answers, what its set
    /// takes and what the state writes.
    pub(crate) const fn field(self) -> &'static str {
        match self {
            Self::Language => LANGUAGE,
            Self::Footer => VISIBILITY,
            Self::Appearance => MODE,
        }
    }

    /// Every word it may be, its default first.
    ///
    /// The language is the system's — the environment's, then English, as the
    /// interface resolves it — or the tag of a locale the catalog ships; the
    /// footer is always shown, or hidden until the Leader or the pointer asks
    /// for it; the mode is the system's, light or dark.
    pub(crate) const fn words(self) -> &'static [&'static str] {
        match self {
            Self::Language => &LANGUAGES,
            Self::Footer => &[ALWAYS, HIDDEN],
            Self::Appearance => &[SYSTEM, "light", "dark"],
        }
    }

    /// Where in its list the word `written` is, and nowhere for any other.
    fn one_of(self, written: &str) -> Option<usize> {
        self.words().iter().position(|word| *word == written)
    }

    /// The command that reads it.
    const fn read(self) -> Verb {
        match self {
            Self::Language => Verb::SettingsLanguageRead,
            Self::Footer => Verb::SettingsFooterRead,
            Self::Appearance => Verb::SettingsAppearanceRead,
        }
    }

    /// The command that sets it.
    const fn set(self) -> Verb {
        match self {
            Self::Language => Verb::SettingsLanguageSet,
            Self::Footer => Verb::SettingsFooterSet,
            Self::Appearance => Verb::SettingsAppearanceSet,
        }
    }

    /// The setting `verb` reads or sets, if it is one's command.
    fn of(verb: Verb) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|setting| setting.read() == verb || setting.set() == verb)
    }
}

/// What each setting of the interface's is: a word of its own list, the
/// first until a person chooses another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Chosen {
    language: &'static str,
    visibility: &'static str,
    mode: &'static str,
}

impl Default for Chosen {
    fn default() -> Self {
        Self {
            language: SYSTEM,
            visibility: ALWAYS,
            mode: SYSTEM,
        }
    }
}

impl Chosen {
    /// What `setting` is.
    const fn word_of(self, setting: Setting) -> &'static str {
        match setting {
            Setting::Language => self.language,
            Setting::Footer => self.visibility,
            Setting::Appearance => self.mode,
        }
    }

    /// Makes `setting` the word `written`, where it is one of its list.
    fn choose(&mut self, setting: Setting, written: &str) -> Option<()> {
        let word = setting
            .one_of(written)
            .and_then(|at| setting.words().get(at).copied())?;
        match setting {
            Setting::Language => self.language = word,
            Setting::Footer => self.visibility = word,
            Setting::Appearance => self.mode = word,
        }
        Some(())
    }

    /// The settings as the state writes them, and as a read answers one:
    /// each under its field.
    fn to_value(self) -> Value {
        Value::object(
            Setting::ALL.map(|setting| (setting.field(), Value::text(self.word_of(setting)))),
        )
    }

    /// The settings `value` writes, each a word of its list and nothing
    /// beside them.
    fn read_back(value: &Value) -> Option<Self> {
        let mut fields = Fields::of(value)?;
        let mut chosen = Self::default();
        for setting in Setting::ALL {
            chosen.choose(setting, &fields.text(setting.field())?)?;
        }
        fields.done()?;
        Some(chosen)
    }
}

/// The interface's state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct State {
    windows: Vec<Window>,
    pages: BTreeMap<u64, Page>,
    drawer: Drawer,
    sidebar: bool,
    footer: bool,
    message: Option<String>,
    expanded: bool,
    draft: String,
    asking: Option<Asking>,
    seats: BTreeMap<u64, Seat>,
    /// The three settings of how the interface is shown.
    settings: Chosen,
    /// The identifier the next window or page is given.
    next: u64,
}

impl Default for State {
    /// The first run's state: one window, hidden and ready, with nothing in
    /// it, so it shows nothing but the footer when it is first shown (§12.2).
    fn default() -> Self {
        Self {
            windows: vec![Window::new(1, false)],
            pages: BTreeMap::new(),
            drawer: Drawer {
                pages: Vec::new(),
                focused: None,
                collapsed: false,
                toward: Place::Workspace,
                flipped: false,
                share: SHARE,
            },
            sidebar: false,
            footer: true,
            message: None,
            expanded: false,
            draft: String::new(),
            asking: None,
            seats: BTreeMap::new(),
            settings: Chosen::default(),
            next: 2,
        }
    }
}

/// What the kernel holds of the interface: its state, the revision it is at,
/// how often a state was released, and whether it changed since it was kept.
#[derive(Debug, Default)]
pub(crate) struct Cell {
    state: State,
    /// Moves by one each time the state changes: what `shell.read` answers.
    revision: u64,
    /// Moves by one each time a held state is released, changed or not:
    /// what a held state names as the one it was computed from.
    released: u64,
    /// Whether the state changed since the store last kept it.
    dirty: bool,
}

impl Cell {
    /// A cell holding `state`, at revision zero, as the store kept it.
    pub(crate) fn over(state: State) -> Self {
        Self {
            state,
            ..Self::default()
        }
    }

    /// The state, and the revision it is at.
    pub(crate) fn read(&self) -> (u64, &State) {
        (self.revision, &self.state)
    }
}

/// A state a call tree holds until it ends, and the release it was computed
/// over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Held {
    state: State,
    base: u64,
}

/// Releases `held` into the kernel's cell, once its call tree ended or its
/// unit was kept.
///
/// **Only over the state it was computed from**: a state released since —
/// by a unit inside the same tree, computed from this one — is newer, and is
/// left as it is.
pub(crate) fn released<E: Environment>(shared: &Shared<E>, held: Held) {
    let revision = {
        let mut cell = dispatch::lock(&shared.shell);
        release(&mut cell, held);
        cell.revision
    };
    told(&shared.shell_moves, revision);
}

/// Tells `moves` the revision the state is at, once the state is there: a
/// receiver woken by it reads that state or a later one.
fn told(moves: &tokio::sync::watch::Sender<u64>, revision: u64) {
    moves.send_if_modified(|seen| {
        let another = *seen != revision;
        *seen = revision;
        another
    });
}

/// Releases `held` into `cell`, answering whether it was released.
fn release(cell: &mut Cell, held: Held) -> bool {
    if held.base != cell.released {
        return false;
    }
    cell.released = cell.released.wrapping_add(1);
    if held.state != cell.state {
        cell.state = held.state;
        cell.revision = cell.revision.wrapping_add(1);
        cell.dirty = true;
    }
    true
}

/// A command of the interface: every one the catalog declares in the
/// namespaces of the interface's state, and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verb {
    ShellRead,
    ShellFocus,
    ShellHover,
    ShellDrag,
    WindowOpen,
    WindowShow,
    WindowHide,
    WindowClose,
    WindowMove,
    WindowResize,
    LeaderTap,
    PageOpen,
    PageSettings,
    PageHelp,
    PageDev,
    PageShortcuts,
    PageActions,
    PageClose,
    PageFocus,
    PageNext,
    PagePrevious,
    PageInto,
    PageBack,
    PageScroll,
    DrawerCollapse,
    DrawerExpand,
    DrawerBack,
    DrawerToward,
    DrawerResize,
    SidebarShow,
    SidebarHide,
    SidebarCycle,
    FooterShow,
    FooterHide,
    InfoSay,
    InfoClear,
    InfoExpand,
    InfoCollapse,
    PromptSet,
    PromptAsk,
    PromptClear,
    SettingsFooterRead,
    SettingsFooterSet,
    SettingsLanguageRead,
    SettingsLanguageSet,
    SettingsAppearanceRead,
    SettingsAppearanceSet,
}

impl Verb {
    /// Every one: the only place one is made.
    pub(crate) const ALL: [Self; 47] = [
        Self::ShellRead,
        Self::ShellFocus,
        Self::ShellHover,
        Self::ShellDrag,
        Self::WindowOpen,
        Self::WindowShow,
        Self::WindowHide,
        Self::WindowClose,
        Self::WindowMove,
        Self::WindowResize,
        Self::LeaderTap,
        Self::PageOpen,
        Self::PageSettings,
        Self::PageHelp,
        Self::PageDev,
        Self::PageShortcuts,
        Self::PageActions,
        Self::PageClose,
        Self::PageFocus,
        Self::PageNext,
        Self::PagePrevious,
        Self::PageInto,
        Self::PageBack,
        Self::PageScroll,
        Self::DrawerCollapse,
        Self::DrawerExpand,
        Self::DrawerBack,
        Self::DrawerToward,
        Self::DrawerResize,
        Self::SidebarShow,
        Self::SidebarHide,
        Self::SidebarCycle,
        Self::FooterShow,
        Self::FooterHide,
        Self::InfoSay,
        Self::InfoClear,
        Self::InfoExpand,
        Self::InfoCollapse,
        Self::PromptSet,
        Self::PromptAsk,
        Self::PromptClear,
        Self::SettingsFooterRead,
        Self::SettingsFooterSet,
        Self::SettingsLanguageRead,
        Self::SettingsLanguageSet,
        Self::SettingsAppearanceRead,
        Self::SettingsAppearanceSet,
    ];

    /// The name of the command it is, as the catalog declares it.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::ShellRead => "shell.read",
            Self::ShellFocus => "shell.focus",
            Self::ShellHover => "shell.hover",
            Self::ShellDrag => "shell.drag",
            Self::WindowOpen => "window.open",
            Self::WindowShow => "window.show",
            Self::WindowHide => "window.hide",
            Self::WindowClose => "window.close",
            Self::WindowMove => "window.move",
            Self::WindowResize => "window.resize",
            Self::LeaderTap => "leader.tap",
            Self::PageOpen => "page.open",
            Self::PageSettings => "page.settings",
            Self::PageHelp => "page.help",
            Self::PageDev => "page.dev",
            Self::PageShortcuts => "page.shortcuts",
            Self::PageActions => "page.actions",
            Self::PageClose => "page.close",
            Self::PageFocus => "page.focus",
            Self::PageNext => "page.next",
            Self::PagePrevious => "page.previous",
            Self::PageInto => "page.into",
            Self::PageBack => "page.back",
            Self::PageScroll => "page.scroll",
            Self::DrawerCollapse => "drawer.collapse",
            Self::DrawerExpand => "drawer.expand",
            Self::DrawerBack => "drawer.back",
            Self::DrawerToward => "drawer.toward",
            Self::DrawerResize => "drawer.resize",
            Self::SidebarShow => "sidebar.show",
            Self::SidebarHide => "sidebar.hide",
            Self::SidebarCycle => "sidebar.cycle",
            Self::FooterShow => "footer.show",
            Self::FooterHide => "footer.hide",
            Self::InfoSay => "info.say",
            Self::InfoClear => "info.clear",
            Self::InfoExpand => "info.expand",
            Self::InfoCollapse => "info.collapse",
            Self::PromptSet => "prompt.set",
            Self::PromptAsk => "prompt.ask",
            Self::PromptClear => "prompt.clear",
            Self::SettingsFooterRead => "settings.footer.read",
            Self::SettingsFooterSet => "settings.footer.set",
            Self::SettingsLanguageRead => "settings.language.read",
            Self::SettingsLanguageSet => "settings.language.set",
            Self::SettingsAppearanceRead => "settings.appearance.read",
            Self::SettingsAppearanceSet => "settings.appearance.set",
        }
    }

    /// Whether it changes nothing: `shell.read`, and a setting's read.
    pub(crate) const fn is_a_read(self) -> bool {
        matches!(
            self,
            Self::ShellRead
                | Self::SettingsFooterRead
                | Self::SettingsLanguageRead
                | Self::SettingsAppearanceRead
        )
    }

    /// Every argument it reads, by the name its schema declares.
    #[cfg(test)]
    pub(crate) const fn reads(self) -> &'static [&'static str] {
        match self {
            Self::ShellRead
            | Self::WindowOpen
            | Self::PageSettings
            | Self::PageHelp
            | Self::PageDev
            | Self::PageShortcuts
            | Self::PageActions
            | Self::DrawerCollapse
            | Self::DrawerExpand
            | Self::DrawerBack
            | Self::SidebarShow
            | Self::SidebarHide
            | Self::FooterShow
            | Self::FooterHide
            | Self::InfoClear
            | Self::InfoExpand
            | Self::InfoCollapse
            | Self::PromptClear
            | Self::SettingsFooterRead
            | Self::SettingsLanguageRead
            | Self::SettingsAppearanceRead => &[],
            Self::SettingsFooterSet => &[VISIBILITY],
            Self::SettingsLanguageSet => &[LANGUAGE],
            Self::SettingsAppearanceSet => &[MODE],
            Self::ShellFocus => &[SEAT, WINDOW, REGION],
            Self::ShellHover => &[SEAT, BY, TARGET],
            Self::ShellDrag => &[SEAT, TARGET],
            Self::WindowShow
            | Self::WindowHide
            | Self::WindowClose
            | Self::PageNext
            | Self::PagePrevious => &[WINDOW],
            Self::WindowMove => &[WINDOW, X, Y],
            Self::WindowResize => &[WINDOW, WIDTH, HEIGHT],
            Self::LeaderTap | Self::SidebarCycle => &[SEAT],
            Self::PageOpen => &[KIND, TOPIC, IN, WINDOW],
            Self::PageClose | Self::PageBack | Self::PageFocus => &[PAGE, IN, WINDOW],
            Self::PageInto => &[PAGE, IN, WINDOW, TOPIC],
            Self::PageScroll => &[PAGE, IN, WINDOW, TO],
            Self::DrawerToward => &[REGION],
            Self::DrawerResize => &[SHARE_ARGUMENT],
            Self::InfoSay | Self::PromptSet => &[TEXT],
            Self::PromptAsk => &[COMMAND, PARAMETER],
        }
    }
}

/// The seat a command acts for: the system's own, `1`, unless it names another.
const SEAT: &str = "seat";
/// The window a command acts on.
const WINDOW: &str = "window";
/// The part of a window a seat's keyboard goes to, or where pages open.
const REGION: &str = "region";
/// Whether a seat's pointer or its keyboard moved.
const BY: &str = "by";
/// What a seat points at or drags.
const TARGET: &str = "target";
/// Where a window's corner goes.
const X: &str = "x";
/// Where a window's corner goes.
const Y: &str = "y";
/// A window's new width.
const WIDTH: &str = "width";
/// A window's new height.
const HEIGHT: &str = "height";
/// What kind of page opens.
const KIND: &str = "kind";
/// The topic a page opens on, or goes into.
const TOPIC: &str = "topic";
/// Where a page opens or is acted on.
const IN: &str = "in";
/// The page acted on.
const PAGE: &str = "page";
/// How far a page is scrolled.
const TO: &str = "to";
/// The drawer's share of the height.
const SHARE_ARGUMENT: &str = "share";
/// The info line's message, or the prompt's draft.
const TEXT: &str = "text";
/// The command the prompt asks the parameters of.
const COMMAND: &str = "command";
/// The parameter the prompt asks for.
const PARAMETER: &str = "parameter";

/// What a note may say: each fact by the name a note says it under, beside the
/// command it is the equivalent of, in the order of their names.
///
/// **What moves at the rate an interface draws** (`NOCTUA.md §12.1`): the
/// prompt's draft, where a seat's keyboard is, what its pointer or keyboard
/// selection is on, and how far a page is scrolled. Each is said as the
/// arguments its command takes, so the CLI and MCP reach the same change by
/// that command, journaled — and an interface by a note, journaled nowhere.
pub(crate) const FACTS: [(&str, Verb); 4] = [
    ("draft", Verb::PromptSet),
    ("focus", Verb::ShellFocus),
    ("hover", Verb::ShellHover),
    ("scroll", Verb::PageScroll),
];

/// The facts `detail` says, each as the command it is the equivalent of and
/// the arguments it is said with, in the order of their names.
///
/// # Errors
///
/// `invalid` with `[NC-VAL-0002]` for a detail that is not one object, says
/// no fact, or says one that is none of [`FACTS`].
pub(crate) fn facts(detail: &Value) -> Result<Vec<(&str, Verb, &Value)>, Refusal> {
    let Value::Object(fields) = detail else {
        return Err(invalid("a note is one object".to_owned()));
    };
    if fields.is_empty() {
        return Err(invalid(format!(
            "a note says one fact at least, of {}",
            the_facts()
        )));
    }
    fields
        .iter()
        .map(|(name, args)| {
            FACTS
                .iter()
                .find(|(fact, _)| fact == name)
                .map(|(fact, verb)| (*fact, *verb, args))
                .ok_or_else(|| {
                    invalid(format!(
                        "a note says `{}`, which is none of the facts a note says: {}",
                        name.escape_debug(),
                        the_facts()
                    ))
                })
        })
        .collect()
}

/// The facts a note says, as a refusal lists them.
fn the_facts() -> String {
    FACTS
        .iter()
        .map(|(fact, verb)| format!("`{fact}`, as `{}` takes it", verb.name()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Applies the note `detail` to the kernel's state for `acting`, journaled
/// nowhere: what [`crate::Kernel::note`] does.
///
/// **In order, the first refusal the answer, and nothing applied before it**:
/// the facts read ([`facts`]); each one's arguments held to its command's
/// schema and its resource taken as the command's is
/// (`dispatch::understood`); each decided for `acting` as its command would
/// be, by the decision point's probe, which journals and spends nothing; then,
/// under the bus — between two call trees, so no tree holds a state computed
/// over one the note moves — each made by its command's rule, in order, over
/// one copy of the state, which is released whole or not at all.
pub(crate) async fn noted<E: Environment>(
    shared: &Shared<E>,
    acting: &crate::session::Acting,
    detail: &Value,
) -> Result<(), Refusal> {
    let said = facts(detail)?;
    let began = std::time::Instant::now();
    let mut understood = Vec::with_capacity(said.len());
    for (fact, verb, args) in said {
        let name = verb.name();
        let command = shared.command(name).ok_or_else(|| {
            Refusal::new(
                RefusalKind::Internal,
                ErrorCode::KernelUnknownCommand,
                format!("the note's `{fact}` is what `{name}` changes, which no registry holds"),
            )
        })?;
        let (_, json, resource) =
            dispatch::understood(shared, &command, name, &args.to_canonical())?;
        let action = &command.listed().action;
        if !crate::session::may(shared, began, acting, acting.spent(), action, &resource).await {
            return Err(Refusal::new(
                RefusalKind::Denied,
                ErrorCode::PolicyDenied,
                format!(
                    "the note's `{fact}` is what `{name}` changes, which the decision point does \
                     not allow; nothing of the note was applied"
                ),
            ));
        }
        understood.push((verb, json));
    }
    let _between_trees = shared.bus.lock().await;
    let (mut state, base) = {
        let cell = dispatch::lock(&shared.shell);
        (cell.state.clone(), cell.released)
    };
    for (verb, json) in &understood {
        changed(&mut state, *verb, json, &Now { shared })?;
    }
    released(shared, Held { state, base });
    Ok(())
}

/// What a command of the interface needs to know of the rest of the kernel:
/// which settings, commands and surfaces there are.
pub(crate) trait Known {
    /// The arguments the command registered as `name` takes, if one is.
    fn arguments_of(&self, name: &str) -> Option<Vec<String>>;
    /// Whether a surface of this identifier is open.
    fn surface(&self, id: &str) -> bool;
}

/// What the kernel knows now, read once for one command.
struct Now<'a, E: Environment> {
    shared: &'a Shared<E>,
}

impl<E: Environment> Known for Now<'_, E> {
    fn arguments_of(&self, name: &str) -> Option<Vec<String>> {
        let admitted = self.shared.command(name)?;
        let schema: serde_json::Value =
            serde_json::from_str(&admitted.listed().args_schema).ok()?;
        Some(
            schema
                .get("properties")
                .and_then(serde_json::Value::as_object)
                .map(|properties| properties.keys().cloned().collect())
                .unwrap_or_default(),
        )
    }

    fn surface(&self, id: &str) -> bool {
        dispatch::lock(&self.shared.surfaces)
            .listed()
            .iter()
            .any(|surface| surface.id() == id)
    }
}

/// Runs the command `verb` with `args` in `tree`: answers the state, or holds
/// the state it leaves until the tree ends.
///
/// # Errors
///
/// What [`changed`] refuses: `not-found` with `[NC-KERN-0027]` for a window or
/// a page the state does not hold, or the page in focus where none is, and
/// with `[NC-KERN-0001]` for a command the prompt is asked about that nothing
/// registered; `invalid` with `[NC-KERN-0028]` past a bound, and with
/// `[NC-VAL-0002]` for arguments the schema could not refuse.
pub(crate) fn ran<E: Environment>(
    shared: &Shared<E>,
    tree: &Arc<Mutex<CallTree>>,
    verb: Verb,
    args: &serde_json::Value,
) -> Result<String, Failure> {
    let held = dispatch::lock(tree).shell_held().cloned();
    if verb == Verb::ShellRead {
        return Ok(the_read(shared, held.as_ref()).to_canonical());
    }
    let setting = Setting::of(verb);
    if let Some(setting) = setting.filter(|_| verb.is_a_read()) {
        let chosen = held.map_or_else(
            || dispatch::lock(&shared.shell).state.settings,
            |held| held.state.settings,
        );
        return Ok(answered(setting, chosen));
    }
    let Held { mut state, base } = held.unwrap_or_else(|| {
        let cell = dispatch::lock(&shared.shell);
        Held {
            state: cell.state.clone(),
            base: cell.released,
        }
    });
    changed(&mut state, verb, args, &Now { shared })
        .map_err(|refusal| Failure::new(refusal, "shell-refused"))?;
    let answer = setting.map_or_else(
        || Value::object([("shell", state.to_value())]).to_canonical(),
        |setting| answered(setting, state.settings),
    );
    dispatch::lock(tree)
        .later_mut()
        .hold_shell(Held { state, base });
    Ok(answer)
}

/// What a setting's read answers, and its set once made: the one object the
/// set takes whole, `{"<field>":"<word>"}`.
fn answered(setting: Setting, chosen: Chosen) -> String {
    Value::object([(setting.field(), Value::text(chosen.word_of(setting)))]).to_canonical()
}

/// What `shell.read` answers: the kernel's state at the revision it is at, or,
/// in a call tree that holds a change, that change alone.
///
/// **A revision names a state the kernel held**, and the one a tree holds is
/// not the kernel's yet — it may never be, if the journal refuses the tree's
/// entry — so it is answered with no revision rather than beside one that
/// names the state before it.
fn the_read<E: Environment>(shared: &Shared<E>, held: Option<&Held>) -> Value {
    if let Some(held) = held {
        return Value::object([("shell", held.state.to_value())]);
    }
    let cell = dispatch::lock(&shared.shell);
    Value::object([
        ("revision", count(cell.revision)),
        ("shell", cell.state.to_value()),
    ])
}

/// What `verb` with `args` makes of `state`, judged against what `known`
/// says of the kernel.
///
/// **No keyboard is left on a drawer that shows nothing open**: whatever the
/// command, a seat's keyboard on the drawer once it is emptied or collapsed
/// goes back to its window's prompt line ([`State::drawer_shut`]), so a
/// window never holds a keyboard on a part it does not draw. **And no reveal
/// outlives the last keyboard on the footer**: whatever the command, a
/// keyboard moved — where the window says it went, to the sidebar, or gone
/// with the window it was in — with none left on the footer or the drawer
/// lets a footer its setting hides go ([`State::footer_let_go`]).
///
/// # Errors
///
/// As [`ran`] says; `state` is left as it was.
pub(crate) fn changed(
    state: &mut State,
    verb: Verb,
    args: &serde_json::Value,
    known: &dyn Known,
) -> Result<(), Refusal> {
    let keyboards = state.keyboards();
    let mut next = state.clone();
    Change {
        state: &mut next,
        args,
        known,
    }
    .made(verb)?;
    next.drawer_shut();
    if next.keyboards() != keyboards {
        next.footer_let_go();
    }
    *state = next;
    Ok(())
}

/// One command's change, over a copy of the state it is kept to only when it
/// is made whole.
struct Change<'a> {
    state: &'a mut State,
    args: &'a serde_json::Value,
    known: &'a dyn Known,
}

impl Change<'_> {
    /// Makes the change `verb` asks for.
    fn made(&mut self, verb: Verb) -> Result<(), Refusal> {
        match verb {
            Verb::ShellRead
            | Verb::SettingsFooterRead
            | Verb::SettingsLanguageRead
            | Verb::SettingsAppearanceRead => Ok(()),
            Verb::SettingsFooterSet => self.chosen(Setting::Footer),
            Verb::SettingsLanguageSet => self.chosen(Setting::Language),
            Verb::SettingsAppearanceSet => self.chosen(Setting::Appearance),
            Verb::ShellFocus => self.focus(),
            Verb::ShellHover => self.hover(),
            Verb::ShellDrag => self.drag(),
            Verb::WindowOpen => self.window_open(),
            Verb::WindowShow | Verb::WindowHide => {
                let id = self.window()?;
                self.state.window_mut(id)?.shown = verb == Verb::WindowShow;
                Ok(())
            }
            Verb::WindowClose => self.window_close(),
            Verb::WindowMove => self.window_move(),
            Verb::WindowResize => self.window_resize(),
            Verb::LeaderTap => self.leader_tap(),
            Verb::PageOpen => self.page_open(),
            Verb::PageSettings => self.page_of_kind(Kind::Settings),
            Verb::PageHelp => self.page_of_kind(Kind::Help),
            Verb::PageDev => self.page_of_kind(Kind::Dev),
            Verb::PageShortcuts => self.page_of_kind(Kind::Shortcuts),
            Verb::PageActions => self.page_of_kind(Kind::Actions),
            Verb::PageClose => self.page_close(),
            Verb::PageFocus => self.page_focus(),
            Verb::PageNext | Verb::PagePrevious => {
                let id = self.window()?;
                self.state.window_mut(id)?.step(verb == Verb::PageNext);
                Ok(())
            }
            Verb::PageInto => self.page_into(),
            Verb::PageBack => {
                let page = self.page()?;
                self.state.page_mut(page)?.trail.pop();
                Ok(())
            }
            Verb::PageScroll => self.page_scroll(),
            Verb::DrawerCollapse
            | Verb::DrawerExpand
            | Verb::DrawerBack
            | Verb::DrawerToward
            | Verb::DrawerResize => self.drawer_made(verb),
            Verb::SidebarShow
            | Verb::SidebarHide
            | Verb::SidebarCycle
            | Verb::FooterShow
            | Verb::FooterHide
            | Verb::InfoSay
            | Verb::InfoClear
            | Verb::InfoExpand
            | Verb::InfoCollapse
            | Verb::PromptSet
            | Verb::PromptAsk
            | Verb::PromptClear => self.lines_made(verb),
        }
    }

    /// Makes the change a command of the drawer's asks for.
    fn drawer_made(&mut self, verb: Verb) -> Result<(), Refusal> {
        match verb {
            Verb::DrawerCollapse => {
                self.state.drawer.collapsed = !self.state.drawer.pages.is_empty();
            }
            Verb::DrawerExpand => self.state.drawer.collapsed = false,
            Verb::DrawerBack => self.drawer_back(),
            Verb::DrawerToward => {
                let toward = self
                    .place(REGION)?
                    .ok_or_else(|| invalid(format!("`{REGION}` is required, and was not given")))?;
                self.state.drawer.toward = toward;
                self.state.drawer.flipped = false;
            }
            Verb::DrawerResize => {
                let share = self.integer(SHARE_ARGUMENT)?;
                in_range(
                    share,
                    1,
                    i64::from(KERNEL.shell_drawer_share_max),
                    SHARE_ARGUMENT,
                )?;
                self.state.drawer.share = u32::try_from(share).unwrap_or(SHARE);
            }
            _ => {}
        }
        Ok(())
    }

    /// Makes the change a command of the sidebar's, the footer's, the info
    /// line's or the prompt's asks for.
    fn lines_made(&mut self, verb: Verb) -> Result<(), Refusal> {
        match verb {
            Verb::SidebarShow => self.state.sidebar = true,
            Verb::SidebarHide => self.state.sidebar_hidden(),
            Verb::SidebarCycle => self.sidebar_cycle()?,
            Verb::FooterShow | Verb::FooterHide => self.state.footer = verb == Verb::FooterShow,
            Verb::InfoSay => {
                let text = self.text()?;
                if text
                    .chars()
                    .any(|character| character != '\n' && nc_schema::unseen(character))
                {
                    return Err(invalid(format!(
                        "`{TEXT}` holds a character a reader would not see, which the info \
                         line never shows"
                    )));
                }
                self.state.message = Some(text);
            }
            Verb::InfoClear => self.state.message = None,
            Verb::InfoExpand | Verb::InfoCollapse => {
                self.state.expanded = verb == Verb::InfoExpand;
            }
            Verb::PromptSet => self.prompt_set()?,
            Verb::PromptAsk => self.prompt_ask()?,
            Verb::PromptClear => {
                self.state.draft.clear();
                self.state.asking = None;
                self.state.close_in_drawer(Kind::Commands);
            }
            _ => {}
        }
        Ok(())
    }

    /// A setting's set: `setting` made the word its one field names, a word
    /// of its list — which the schema lists too, so one outside it is the
    /// arguments' own refusal, `[NC-VAL-0002]`.
    fn chosen(&mut self, setting: Setting) -> Result<(), Refusal> {
        let field = setting.field();
        let written = self
            .optional_text(field)?
            .ok_or_else(|| invalid(format!("`{field}` is required, and was not given")))?
            .to_owned();
        self.state
            .settings
            .choose(setting, &written)
            .ok_or_else(|| {
                invalid(format!(
                    "`{field}` is none of {}",
                    setting
                        .words()
                        .iter()
                        .map(|word| format!("`{word}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })?;
        if setting == Setting::Footer {
            // Chosen always, the footer is shown now, so the next tap's rule
            // reads the line as seen; chosen hidden, it hides now unless a
            // keyboard holds it (§12.3).
            if self.state.settings.visibility == ALWAYS {
                self.state.footer = true;
            } else {
                self.state.footer_let_go();
            }
        }
        Ok(())
    }

    /// `window.move`: where a window's corner goes.
    fn window_move(&mut self) -> Result<(), Refusal> {
        let id = self.window()?;
        let place = (self.integer(X)?, self.integer(Y)?);
        in_range(place.0, PLACE_MIN, PLACE_MAX, X)?;
        in_range(place.1, PLACE_MIN, PLACE_MAX, Y)?;
        self.state.window_mut(id)?.place = Some(place);
        Ok(())
    }

    /// `page.scroll`: how far a page is scrolled.
    fn page_scroll(&mut self) -> Result<(), Refusal> {
        let page = self.page()?;
        let to = self.integer(TO)?;
        in_range(to, 0, i64::from(u32::MAX), TO)?;
        self.state.page_mut(page)?.scroll = u32::try_from(to).unwrap_or(u32::MAX);
        Ok(())
    }

    /// The seat named, or the system's own.
    fn seat(&self) -> Result<u64, Refusal> {
        let seat = self.optional_integer(SEAT)?.unwrap_or(1);
        in_range(seat, 1, i64::from(KERNEL.shell_seats_max), SEAT)?;
        Ok(u64::try_from(seat).unwrap_or(1))
    }

    /// The window named, or the one the system's seat is in.
    fn window(&self) -> Result<u64, Refusal> {
        self.window_of(1)
    }

    /// The window named, or the one `seat` is in.
    fn window_of(&self, seat: u64) -> Result<u64, Refusal> {
        match self.optional_integer(WINDOW)? {
            Some(named) => {
                let id = u64::try_from(named).unwrap_or(0);
                self.state.window(id).map(|window| window.id)
            }
            None => Ok(self.state.current(seat)),
        }
    }

    /// The page named, or the one in focus where `in` says.
    fn page(&self) -> Result<u64, Refusal> {
        if let Some(named) = self.optional_integer(PAGE)? {
            let id = u64::try_from(named).unwrap_or(0);
            return self.state.page(id).map(|page| page.id);
        }
        match self.place(IN)?.unwrap_or(Place::Workspace) {
            Place::Drawer => self
                .state
                .drawer
                .focused
                .ok_or_else(|| unknown("no page has focus in the drawer".to_owned())),
            Place::Workspace => {
                let window = self.window()?;
                self.state.window(window)?.focused.ok_or_else(|| {
                    unknown(format!(
                        "no page has focus in window {window}'s workspace area"
                    ))
                })
            }
        }
    }

    /// The integer argument `name`, which the schema made one.
    fn integer(&self, name: &str) -> Result<i64, Refusal> {
        self.optional_integer(name)?
            .ok_or_else(|| invalid(format!("`{name}` is required, and was not given")))
    }

    /// The integer argument `name`, if it was given.
    fn optional_integer(&self, name: &str) -> Result<Option<i64>, Refusal> {
        match self.args.get(name) {
            None => Ok(None),
            Some(value) => value
                .as_i64()
                .map(Some)
                .ok_or_else(|| invalid(format!("`{name}` is not a whole number"))),
        }
    }

    /// The text argument `name`, if it was given.
    fn optional_text(&self, name: &str) -> Result<Option<&str>, Refusal> {
        match self.args.get(name) {
            None => Ok(None),
            Some(value) => value
                .as_str()
                .map(Some)
                .ok_or_else(|| invalid(format!("`{name}` is not a text"))),
        }
    }

    /// The word argument `name`, if it was given: held to [`is_word`].
    fn optional_word(&self, name: &str) -> Result<Option<String>, Refusal> {
        match self.optional_text(name)? {
            None => Ok(None),
            Some(text) if is_word(text) => Ok(Some(text.to_owned())),
            Some(_) => Err(invalid(format!(
                "`{name}` is not one to {WORD_CHARS} letters, digits or `-_.:/`"
            ))),
        }
    }

    /// The place argument `name`, if it was given.
    fn place(&self, name: &str) -> Result<Option<Place>, Refusal> {
        match self.optional_text(name)? {
            None => Ok(None),
            Some(text) => Place::of(text)
                .map(Some)
                .ok_or_else(|| invalid(format!("`{name}` is neither `workspace` nor `drawer`"))),
        }
    }

    /// The text of `text`, held to `shell_text_bytes`.
    ///
    /// **An argument refused as its schema refuses one**: the schema counts
    /// the text in characters, to the same number, and this in bytes, so a
    /// text within the one and past the other is the same refusal,
    /// `[NC-VAL-0002]` — never `[NC-KERN-0028]`, which is the state full.
    fn text(&self) -> Result<String, Refusal> {
        let text = self
            .optional_text(TEXT)?
            .ok_or_else(|| invalid(format!("`{TEXT}` is required, and was not given")))?;
        if u64::try_from(text.len()).unwrap_or(u64::MAX) > KERNEL.shell_text_bytes {
            return Err(invalid(format!(
                "`{TEXT}` holds {} bytes, past the {} `[kernel] shell_text_bytes` allows",
                text.len(),
                KERNEL.shell_text_bytes
            )));
        }
        Ok(text.to_owned())
    }

    /// `shell.focus`: a seat's keyboard on a part of a window.
    fn focus(&mut self) -> Result<(), Refusal> {
        let seat = self.seat()?;
        let window = self.window_of(seat)?;
        let region = self
            .optional_text(REGION)?
            .and_then(Region::of)
            .ok_or_else(|| invalid(format!("`{REGION}` names no part of a window")))?;
        self.state.seat_mut(seat).focus = Some(Focus { window, region });
        Ok(())
    }

    /// `shell.hover`: what a seat's pointer hovers or its selection is on.
    fn hover(&mut self) -> Result<(), Refusal> {
        let seat = self.seat()?;
        let by = self
            .optional_text(BY)?
            .and_then(By::of)
            .ok_or_else(|| invalid(format!("`{BY}` is neither `pointer` nor `keyboard`")))?;
        let target = self.optional_word(TARGET)?;
        let held = self.state.seat_mut(seat);
        match by {
            By::Pointer => held.hover = target,
            By::Keyboard => held.selection = target,
        }
        held.latest = by;
        Ok(())
    }

    /// `shell.drag`: what a seat drags, or that it stopped.
    fn drag(&mut self) -> Result<(), Refusal> {
        let seat = self.seat()?;
        let target = self.optional_word(TARGET)?;
        self.state.seat_mut(seat).drag = target;
        Ok(())
    }

    /// `window.open`: one more window, shown.
    fn window_open(&mut self) -> Result<(), Refusal> {
        if self.state.windows.len() >= limit(KERNEL.shell_windows_max) {
            return Err(full(format!(
                "the interface holds {} windows already, as many as `[kernel] \
                 shell_windows_max` allows",
                self.state.windows.len()
            )));
        }
        let id = self.state.minted()?;
        self.state.windows.push(Window::new(id, true));
        Ok(())
    }

    /// `window.close`: a window gone with the pages only it showed, or the
    /// last one hidden.
    fn window_close(&mut self) -> Result<(), Refusal> {
        let id = self.window()?;
        if self.state.windows.len() == 1 {
            self.state.window_mut(id)?.shown = false;
            return Ok(());
        }
        self.state.windows.retain(|window| window.id != id);
        for seat in self.state.seats.values_mut() {
            if seat.focus.is_some_and(|focus| focus.window == id) {
                seat.focus = None;
            }
        }
        self.state.swept();
        Ok(())
    }

    /// `window.resize`: a window's size, never below the smallest.
    fn window_resize(&mut self) -> Result<(), Refusal> {
        let id = self.window()?;
        let (width, height) = (self.integer(WIDTH)?, self.integer(HEIGHT)?);
        in_range(
            width,
            i64::from(KERNEL.shell_window_min_width),
            WINDOW_MAX,
            WIDTH,
        )?;
        in_range(
            height,
            i64::from(KERNEL.shell_window_min_height),
            WINDOW_MAX,
            HEIGHT,
        )?;
        self.state.window_mut(id)?.size = Some((width, height));
        Ok(())
    }

    /// `leader.tap`: the three rules of §12.8, in the order the menu's
    /// precedes the prompt's.
    ///
    /// **Each rule only where what it asks about is seen.** The menu's: open
    /// in the drawer of the seat's window, and that window shown. A menu left
    /// open in a window hidden — by `window.hide`, or by closing the last one
    /// — shows nothing, so the tap is rule 1's and shows the window, rather
    /// than flip where a page no one sees goes. The prompt's: the seat's
    /// keyboard on the prompt line of a window shown, **a line seen whatever
    /// the state's "shown now" says** — a footer is always shown, or it is the
    /// design system's folding footer, which opens while a keyboard is inside
    /// it — so a keyboard a click or Tab put there under `hidden`, or one
    /// `footer.hide` left there, is one tap from the menu, as rule 2 says.
    /// **Every tap shows the footer** (§12.3).
    fn leader_tap(&mut self) -> Result<(), Refusal> {
        let seat = self.seat()?;
        let window = self.state.current(seat);
        let shown = self.state.window(window)?.shown;
        // A hidden footer appears when the Leader is pressed, whichever rule
        // the tap follows.
        self.state.footer = true;
        if shown && self.state.leader_open() {
            self.state.drawer.flipped = !self.state.drawer.flipped;
            return Ok(());
        }
        let on_the_prompt = Focus {
            window,
            region: Region::Prompt,
        };
        if shown && self.state.seats.get(&seat).and_then(|held| held.focus) == Some(on_the_prompt) {
            self.state
                .opened(Kind::Leader, None, Place::Drawer, window)?;
            self.state.seat_mut(seat).focus = Some(Focus {
                window,
                region: Region::Drawer,
            });
            return Ok(());
        }
        self.state.window_mut(window)?.shown = true;
        self.state.seat_mut(seat).focus = Some(on_the_prompt);
        Ok(())
    }

    /// `page.open`: a page opened, or one more view of it, where it is asked
    /// for or where the drawer's direction says.
    fn page_open(&mut self) -> Result<(), Refusal> {
        let kind = self
            .optional_text(KIND)?
            .and_then(Kind::of)
            .ok_or_else(|| invalid(format!("`{KIND}` names no kind of page")))?;
        let topic = self.optional_word(TOPIC)?;
        match (kind.topic(), &topic) {
            (Topic::None, Some(_)) => {
                return Err(invalid(format!(
                    "a `{}` page takes no `{TOPIC}`",
                    kind.word()
                )));
            }
            (Topic::Required, None) => {
                return Err(invalid(format!(
                    "a `{}` page needs a `{TOPIC}`",
                    kind.word()
                )));
            }
            _ => {}
        }
        if let Some(topic) = &topic {
            self.topic_is_known(kind, topic)?;
        }
        let asked = self.place(IN)?;
        let place = if kind.drawer_only() {
            if asked == Some(Place::Workspace) {
                return Err(invalid(format!(
                    "a `{}` page opens in the drawer alone",
                    kind.word()
                )));
            }
            Place::Drawer
        } else {
            asked.unwrap_or_else(|| self.state.direction())
        };
        let window = self.window()?;
        self.state.opened(kind, topic, place, window)
    }

    /// `page.settings`, `page.help`, `page.dev`, `page.shortcuts` and
    /// `page.actions`: the page of `kind` on no topic, opened as `page.open`
    /// opens it when it names that kind alone — where the drawer's direction
    /// says, or in the drawer for a kind that opens there alone.
    ///
    /// **A command for each entry of the leader menu that opens a page**
    /// (§12.9): a binding names a command and carries no argument (§12.8), so
    /// the menu's `s`, `h`, `d`, `/` and `w` each need one that takes none.
    fn page_of_kind(&mut self, kind: Kind) -> Result<(), Refusal> {
        let place = if kind.drawer_only() {
            Place::Drawer
        } else {
            self.state.direction()
        };
        let window = self.window()?;
        self.state.opened(kind, None, place, window)
    }

    /// Whether `topic` names what a page of `kind` can show.
    fn topic_is_known(&self, kind: Kind, topic: &str) -> Result<(), Refusal> {
        let known = match kind {
            Kind::Settings => self
                .known
                .arguments_of(&format!("settings.{topic}.read"))
                .is_some(),
            Kind::Surface => self.known.surface(topic),
            _ => true,
        };
        if known {
            Ok(())
        } else {
            Err(unknown(format!(
                "there is no {} `{topic}` to show",
                match kind {
                    Kind::Settings => "setting",
                    _ => "surface",
                }
            )))
        }
    }

    /// `page.close`: a page out of one place, or out of every place it is
    /// shown when it is named and no place is.
    fn page_close(&mut self) -> Result<(), Refusal> {
        let named = self.optional_integer(PAGE)?.is_some();
        let page = self.page()?;
        let window = self.window()?;
        match (named, self.place(IN)?) {
            (true, None) => {
                for held in &mut self.state.windows {
                    held.remove(page);
                }
                self.state.drawer_remove(page);
            }
            (_, Some(Place::Drawer)) => {
                if !self.state.drawer.pages.contains(&page) {
                    return Err(unknown(format!("page {page} is not open in the drawer")));
                }
                self.state.drawer_remove(page);
            }
            (_, Some(Place::Workspace) | None) => {
                let held = self.state.window_mut(window)?;
                if !held.pages.contains(&page) {
                    return Err(unknown(format!(
                        "page {page} is not open in window {window}'s workspace area"
                    )));
                }
                held.remove(page);
            }
        }
        self.state.swept();
        Ok(())
    }

    /// `page.focus`: a page brought to the front where it is shown.
    fn page_focus(&mut self) -> Result<(), Refusal> {
        let page = self.page()?;
        let asked = self.place(IN)?;
        let named_window = self.optional_integer(WINDOW)?.is_some();
        let window = self.window()?;
        let in_window = |state: &State| {
            if named_window
                || state
                    .window(window)
                    .is_ok_and(|held| held.pages.contains(&page))
            {
                Some(window)
            } else {
                state
                    .windows
                    .iter()
                    .find(|held| held.pages.contains(&page))
                    .map(|held| held.id)
            }
        };
        let place = match asked {
            Some(place) => place,
            None if in_window(self.state).is_some() => Place::Workspace,
            None => Place::Drawer,
        };
        match place {
            Place::Drawer => {
                if !self.state.drawer.pages.contains(&page) {
                    return Err(unknown(format!("page {page} is not open in the drawer")));
                }
                self.state.drawer.focused = Some(page);
                self.state.drawer.collapsed = false;
            }
            Place::Workspace => {
                let window = in_window(self.state).unwrap_or(window);
                let held = self.state.window_mut(window)?;
                if !held.pages.contains(&page) {
                    return Err(unknown(format!(
                        "page {page} is not open in window {window}'s workspace area"
                    )));
                }
                held.focused = Some(page);
            }
        }
        Ok(())
    }

    /// `page.into`: one sub-page deeper.
    fn page_into(&mut self) -> Result<(), Refusal> {
        let page = self.page()?;
        let topic = self
            .optional_word(TOPIC)?
            .ok_or_else(|| invalid(format!("`{TOPIC}` is required, and was not given")))?;
        let held = self.state.page_mut(page)?;
        if held.trail.len() >= limit(KERNEL.shell_trail_max) {
            return Err(full(format!(
                "page {page} is {} sub-pages deep already, as deep as `[kernel] \
                 shell_trail_max` allows",
                held.trail.len()
            )));
        }
        held.trail.push(topic);
        Ok(())
    }

    /// `drawer.back`: what Esc does in the drawer (§12.11).
    fn drawer_back(&mut self) {
        let Some(page) = self.state.drawer.focused else {
            return;
        };
        let back = self
            .state
            .pages
            .get_mut(&page)
            .and_then(|held| held.trail.pop());
        if back.is_none() {
            self.state.drawer_remove(page);
            self.state.swept();
        }
    }

    /// `sidebar.cycle`: what Leader Space does (§12.14).
    fn sidebar_cycle(&mut self) -> Result<(), Refusal> {
        let seat = self.seat()?;
        let window = self.state.current(seat);
        let on_it = Focus {
            window,
            region: Region::Sidebar,
        };
        if !self.state.sidebar {
            self.state.sidebar = true;
        } else if self.state.seats.get(&seat).and_then(|held| held.focus) == Some(on_it) {
            self.state.sidebar_hidden();
        } else {
            self.state.seat_mut(seat).focus = Some(on_it);
        }
        Ok(())
    }

    /// `prompt.set`: the draft, and the command list opened by a `;` or a `:`
    /// it begins with (§12.7).
    fn prompt_set(&mut self) -> Result<(), Refusal> {
        let text = self.text()?;
        let lists = text.starts_with([';', ':']);
        self.state.draft = text;
        if matches!(self.state.asking, Some(Asking::Parameters { .. })) {
            return Ok(());
        }
        if lists {
            let window = self.window()?;
            self.state.asking = Some(Asking::Commands);
            self.state
                .opened(Kind::Commands, None, Place::Drawer, window)?;
        } else if self.state.asking.take().is_some() {
            self.state.close_in_drawer(Kind::Commands);
        }
        Ok(())
    }

    /// `prompt.ask`: the prompt kept on a command, asking for its parameters.
    fn prompt_ask(&mut self) -> Result<(), Refusal> {
        let command = self
            .optional_text(COMMAND)?
            .ok_or_else(|| invalid(format!("`{COMMAND}` is required, and was not given")))?;
        let arguments = self.known.arguments_of(command).ok_or_else(|| {
            Refusal::new(
                RefusalKind::NotFound,
                ErrorCode::KernelUnknownCommand,
                format!(
                    "the prompt was asked about {}, which no command is registered as",
                    crate::registry::quoted_up_to(command, nc_schema::descriptor::NAME_MAX)
                ),
            )
        })?;
        let parameter = self.optional_word(PARAMETER)?;
        if let Some(parameter) = &parameter
            && !arguments.iter().any(|argument| argument == parameter)
        {
            return Err(invalid(format!(
                "`{command}` takes no parameter `{parameter}`"
            )));
        }
        self.state.asking = Some(Asking::Parameters {
            command: command.to_owned(),
            parameter,
        });
        Ok(())
    }
}

impl State {
    /// The window numbered `id`.
    fn window(&self, id: u64) -> Result<&Window, Refusal> {
        self.windows
            .iter()
            .find(|window| window.id == id)
            .ok_or_else(|| unknown(format!("window {id} is not one the interface holds")))
    }

    /// The window numbered `id`, to change.
    fn window_mut(&mut self, id: u64) -> Result<&mut Window, Refusal> {
        self.windows
            .iter_mut()
            .find(|window| window.id == id)
            .ok_or_else(|| unknown(format!("window {id} is not one the interface holds")))
    }

    /// The page numbered `id`.
    fn page(&self, id: u64) -> Result<&Page, Refusal> {
        self.pages
            .get(&id)
            .ok_or_else(|| unknown(format!("page {id} is not open in the interface")))
    }

    /// The page numbered `id`, to change.
    fn page_mut(&mut self, id: u64) -> Result<&mut Page, Refusal> {
        self.pages
            .get_mut(&id)
            .ok_or_else(|| unknown(format!("page {id} is not open in the interface")))
    }

    /// The seat numbered `id`, made when it is first named.
    fn seat_mut(&mut self, id: u64) -> &mut Seat {
        self.seats.entry(id).or_default()
    }

    /// The window `seat` is in: the one its focus names, else the first one
    /// shown, else the first.
    fn current(&self, seat: u64) -> u64 {
        let focused = self
            .seats
            .get(&seat)
            .and_then(|held| held.focus)
            .map(|focus| focus.window)
            .filter(|id| self.window(*id).is_ok());
        focused
            .or_else(|| {
                self.windows
                    .iter()
                    .find(|window| window.shown)
                    .map(|window| window.id)
            })
            .or_else(|| self.windows.first().map(|window| window.id))
            .unwrap_or(1)
    }

    /// A new identifier for a window or a page.
    ///
    /// # Errors
    ///
    /// `invalid` with `[NC-KERN-0028]` once the next would pass [`NEXT_MAX`]:
    /// past it, an identifier would be written as one another holds, and the
    /// state read back as two windows or two pages of one number.
    fn minted(&mut self) -> Result<u64, Refusal> {
        let id = self.next;
        if id >= NEXT_MAX {
            return Err(full(format!(
                "the interface has given every identifier a window or a page can have, the \
                 last {}",
                NEXT_MAX.saturating_sub(1)
            )));
        }
        self.next = id.saturating_add(1);
        Ok(id)
    }

    /// Whether the leader menu is open: in focus in the drawer, which is not
    /// collapsed.
    fn leader_open(&self) -> bool {
        !self.drawer.collapsed
            && self
                .drawer
                .focused
                .and_then(|id| self.pages.get(&id))
                .is_some_and(|page| page.kind == Kind::Leader)
    }

    /// Where a page opened from the leader menu goes now: the drawer's
    /// direction, or the other place for this once after a Leader tap.
    fn direction(&self) -> Place {
        if self.drawer.flipped {
            self.drawer.toward.other()
        } else {
            self.drawer.toward
        }
    }

    /// Opens a page of `kind` on `topic` in `place`, or one more view of it.
    ///
    /// **Once for its kind and topic**: a page open already is shown again
    /// where it is asked for — the drawer, or `window`'s workspace area — and
    /// brought to the front there. Any other page closes the leader menu first.
    fn opened(
        &mut self,
        kind: Kind,
        topic: Option<String>,
        place: Place,
        window: u64,
    ) -> Result<(), Refusal> {
        self.window(window)?;
        if kind != Kind::Leader {
            self.close_in_drawer(Kind::Leader);
        }
        let existing = self
            .pages
            .values()
            .find(|page| page.kind == kind && page.topic == topic)
            .map(|page| page.id);
        let shown_there = existing.is_some_and(|id| match place {
            Place::Drawer => self.drawer.pages.contains(&id),
            Place::Workspace => self
                .window(window)
                .is_ok_and(|held| held.pages.contains(&id)),
        });
        if existing.is_none() && self.pages.len() >= limit(KERNEL.shell_pages_max) {
            return Err(full(format!(
                "the interface holds {} pages already, as many as `[kernel] shell_pages_max` \
                 allows",
                self.pages.len()
            )));
        }
        if place == Place::Drawer
            && !shown_there
            && self.drawer.pages.len() >= limit(KERNEL.shell_drawer_pages_max)
        {
            return Err(full(format!(
                "the drawer holds {} pages already, as many as `[kernel] \
                 shell_drawer_pages_max` allows",
                self.drawer.pages.len()
            )));
        }
        let id = if let Some(id) = existing {
            id
        } else {
            let id = self.minted()?;
            self.pages.insert(
                id,
                Page {
                    id,
                    kind,
                    topic,
                    trail: Vec::new(),
                    scroll: 0,
                },
            );
            id
        };
        match place {
            Place::Drawer => {
                if !shown_there {
                    self.drawer.pages.push(id);
                }
                self.drawer.focused = Some(id);
                self.drawer.collapsed = false;
            }
            Place::Workspace => {
                let held = self.window_mut(window)?;
                if !shown_there {
                    held.pages.push(id);
                }
                held.focused = Some(id);
            }
        }
        if kind != Kind::Leader {
            self.drawer.flipped = false;
        }
        Ok(())
    }

    /// Takes the page of `kind` out of the drawer, if one is there.
    fn close_in_drawer(&mut self, kind: Kind) {
        let held: Vec<u64> = self
            .drawer
            .pages
            .iter()
            .copied()
            .filter(|id| self.pages.get(id).is_some_and(|page| page.kind == kind))
            .collect();
        for id in held {
            self.drawer_remove(id);
        }
        self.swept();
    }

    /// Takes the page `id` out of the drawer, its focus moving to the page
    /// before it, and the drawer no longer collapsed once it holds nothing.
    fn drawer_remove(&mut self, id: u64) {
        let focused = self.drawer.focused;
        self.drawer.focused = removed(&mut self.drawer.pages, id, focused);
        if self.drawer.pages.is_empty() {
            self.drawer.collapsed = false;
        }
    }

    /// Forgets every page no place shows, and the leader menu's flip once the
    /// menu is gone.
    fn swept(&mut self) {
        let shown: Vec<u64> = self
            .windows
            .iter()
            .flat_map(|window| window.pages.iter().copied())
            .chain(self.drawer.pages.iter().copied())
            .collect();
        self.pages.retain(|id, _| shown.contains(id));
        if !self.pages.values().any(|page| page.kind == Kind::Leader) {
            self.drawer.flipped = false;
        }
    }

    /// Whether the drawer shows anything a keyboard can be on: what the prompt
    /// asks for, which it lists collapsed or not, or a page, while it is not
    /// collapsed.
    fn drawer_open(&self) -> bool {
        self.asking.is_some() || (!self.drawer.pages.is_empty() && !self.drawer.collapsed)
    }

    /// Every seat whose keyboard is on a drawer that shows nothing open —
    /// emptied, or collapsed — gone back to its window's prompt line: where
    /// the leader menu's keyboard came from (§12.8, the second rule), and
    /// where what is typed next goes.
    fn drawer_shut(&mut self) {
        if self.drawer_open() {
            return;
        }
        for seat in self.seats.values_mut() {
            if let Some(focus) = &mut seat.focus
                && focus.region == Region::Drawer
            {
                focus.region = Region::Prompt;
            }
        }
    }

    /// Every seat's keyboard that is on a part of a window, by seat: what a
    /// command compares to tell whether it moved one ([`changed`]).
    fn keyboards(&self) -> Vec<(u64, Focus)> {
        self.seats
            .iter()
            .filter_map(|(id, seat)| seat.focus.map(|focus| (*id, focus)))
            .collect()
    }

    /// Whether a seat's keyboard is on the footer — on one of its three lines
    /// — or on the drawer: what holds a hidden footer shown (§12.3).
    fn holds_the_footer(&self) -> bool {
        self.seats.values().any(|seat| {
            seat.focus.is_some_and(|focus| {
                matches!(
                    focus.region,
                    Region::Prompt | Region::Info | Region::Modeline | Region::Drawer
                )
            })
        })
    }

    /// A footer its setting hides, hidden again once no keyboard holds it.
    ///
    /// **"Shown now" is a reveal where the setting hides the footer**: a Leader
    /// tap or `footer.show` shows it, it stays while a seat's keyboard is on
    /// the footer or the drawer, and a keyboard moved with none left on them
    /// ends it — moved where the window says it went or by a command, or gone
    /// with the window it was in ([`changed`]) — what §12.3 asks for focus,
    /// decided here so every window folds it at once. The pointer's
    /// part, and a keyboard's inside the footer it is on, are the design
    /// system's folding footer's own, which opens under a pointer or around a
    /// focus whatever this says. A footer always shown is left as it is.
    fn footer_let_go(&mut self) {
        if self.settings.visibility == HIDDEN {
            self.footer = self.footer && self.holds_the_footer();
        }
    }

    /// Hides the sidebar, every seat whose keyboard was on it going back to
    /// its window's workspace area.
    fn sidebar_hidden(&mut self) {
        self.sidebar = false;
        for seat in self.seats.values_mut() {
            if let Some(focus) = &mut seat.focus
                && focus.region == Region::Sidebar
            {
                focus.region = Region::Workspace;
            }
        }
    }
}

impl Window {
    /// Takes the page `id` out of its stack, its focus moving to the page
    /// before it.
    fn remove(&mut self, id: u64) {
        let focused = self.focused;
        self.focused = removed(&mut self.pages, id, focused);
    }

    /// Brings the next page of the stack to the front, or the previous one;
    /// at the end it stays.
    fn step(&mut self, forward: bool) {
        let Some(at) = self
            .focused
            .and_then(|id| self.pages.iter().position(|page| *page == id))
        else {
            return;
        };
        let to = if forward {
            at.saturating_add(1)
        } else {
            at.saturating_sub(1)
        };
        if let Some(page) = self.pages.get(to) {
            self.focused = Some(*page);
        }
    }
}

/// Takes `id` out of `pages`, answering what `focused` becomes.
///
/// Itself when another page had focus, else the page before the one taken,
/// else the one after it, else none.
fn removed(pages: &mut Vec<u64>, id: u64, focused: Option<u64>) -> Option<u64> {
    let Some(at) = pages.iter().position(|page| *page == id) else {
        return focused;
    };
    pages.remove(at);
    if focused != Some(id) {
        return focused;
    }
    pages
        .get(at.saturating_sub(1))
        .or_else(|| pages.get(at))
        .copied()
}

/// Whether `text` is a word a topic, a target or a parameter may be: one to
/// [`WORD_CHARS`] ASCII letters, digits or `-_.:/`.
pub(crate) fn is_word(text: &str) -> bool {
    !text.is_empty()
        && text.chars().count() <= WORD_CHARS
        && text
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.:/".contains(character))
}

/// A bound of the catalog's, as a count.
fn limit(bound: u32) -> usize {
    usize::try_from(bound).unwrap_or(usize::MAX)
}

/// Refuses `value` of the argument `name` outside `low..=high`.
fn in_range(value: i64, low: i64, high: i64, name: &str) -> Result<(), Refusal> {
    if (low..=high).contains(&value) {
        Ok(())
    } else {
        Err(invalid(format!(
            "`{name}` is {value}, outside {low} to {high}"
        )))
    }
}

/// `not-found` with `[NC-KERN-0027]`: what the state does not hold.
fn unknown(sentence: String) -> Refusal {
    Refusal::new(
        RefusalKind::NotFound,
        ErrorCode::KernelShellUnknown,
        sentence,
    )
}

/// `invalid` with `[NC-KERN-0028]`: past a bound of the catalog's.
fn full(sentence: String) -> Refusal {
    Refusal::new(RefusalKind::Invalid, ErrorCode::KernelShellFull, sentence)
}

/// `invalid` with `[NC-VAL-0002]`: arguments the schema could not refuse.
fn invalid(sentence: String) -> Refusal {
    Refusal::new(
        RefusalKind::Invalid,
        ErrorCode::ValidationArgumentsInvalid,
        sentence,
    )
}

/// A count, as the state writes it.
fn count(counted: u64) -> Value {
    Value::Integer(i64::try_from(counted).unwrap_or(i64::MAX))
}

/// Identifiers, as the state writes them.
fn ids(ids: &[u64]) -> Value {
    Value::Array(ids.iter().copied().map(count).collect())
}

/// An optional text, as the state writes it: absent when there is none.
fn maybe(fields: &mut Vec<(&'static str, Value)>, name: &'static str, text: Option<&str>) {
    if let Some(text) = text {
        fields.push((name, Value::text(text)));
    }
}

impl State {
    /// The state as `shell.read` answers it and the store keeps it.
    pub(crate) fn to_value(&self) -> Value {
        let mut info = vec![("expanded", Value::Bool(self.expanded))];
        maybe(&mut info, "message", self.message.as_deref());
        Value::object([
            ("windows", self.windows_value()),
            ("pages", self.pages_value()),
            ("drawer", self.drawer_value()),
            (
                "sidebar",
                Value::object([("shown", Value::Bool(self.sidebar))]),
            ),
            (
                "footer",
                Value::object([("shown", Value::Bool(self.footer))]),
            ),
            ("info", Value::object(info)),
            ("prompt", self.prompt_value()),
            ("seats", self.seats_value()),
            ("settings", self.settings.to_value()),
            ("next", count(self.next)),
        ])
    }

    /// The windows, as the state writes them.
    fn windows_value(&self) -> Value {
        Value::Array(
            self.windows
                .iter()
                .map(|window| {
                    let mut fields = vec![
                        ("id", count(window.id)),
                        ("shown", Value::Bool(window.shown)),
                        ("pages", ids(&window.pages)),
                    ];
                    if let Some(focused) = window.focused {
                        fields.push(("focused", count(focused)));
                    }
                    if let Some((width, height)) = window.size {
                        fields.push(("width", Value::Integer(width)));
                        fields.push(("height", Value::Integer(height)));
                    }
                    if let Some((x, y)) = window.place {
                        fields.push(("x", Value::Integer(x)));
                        fields.push(("y", Value::Integer(y)));
                    }
                    Value::object(fields)
                })
                .collect(),
        )
    }

    /// The pages, as the state writes them.
    fn pages_value(&self) -> Value {
        Value::Array(
            self.pages
                .values()
                .map(|page| {
                    let mut fields = vec![
                        ("id", count(page.id)),
                        ("kind", Value::text(page.kind.word())),
                        (
                            "trail",
                            Value::Array(page.trail.iter().map(Value::text).collect()),
                        ),
                        ("scroll", Value::Integer(i64::from(page.scroll))),
                    ];
                    maybe(&mut fields, "topic", page.topic.as_deref());
                    Value::object(fields)
                })
                .collect(),
        )
    }

    /// The drawer, as the state writes it.
    fn drawer_value(&self) -> Value {
        let mut drawer = vec![
            ("pages", ids(&self.drawer.pages)),
            ("collapsed", Value::Bool(self.drawer.collapsed)),
            ("toward", Value::text(self.drawer.toward.word())),
            ("flipped", Value::Bool(self.drawer.flipped)),
            ("share", Value::Integer(i64::from(self.drawer.share))),
        ];
        if let Some(focused) = self.drawer.focused {
            drawer.push(("focused", count(focused)));
        }
        Value::object(drawer)
    }

    /// The prompt, as the state writes it.
    fn prompt_value(&self) -> Value {
        let mut prompt = vec![("draft", Value::text(self.draft.as_str()))];
        if let Some(asking) = &self.asking {
            prompt.push((
                "asking",
                match asking {
                    Asking::Commands => Value::object([("for", Value::text("commands"))]),
                    Asking::Parameters { command, parameter } => {
                        let mut fields = vec![
                            ("for", Value::text("parameters")),
                            ("command", Value::text(command.as_str())),
                        ];
                        maybe(&mut fields, "parameter", parameter.as_deref());
                        Value::object(fields)
                    }
                },
            ));
        }
        Value::object(prompt)
    }

    /// The seats, as the state writes them.
    fn seats_value(&self) -> Value {
        Value::Array(
            self.seats
                .iter()
                .map(|(id, seat)| {
                    let mut fields = vec![
                        ("seat", count(*id)),
                        ("latest", Value::text(seat.latest.word())),
                    ];
                    if let Some(focus) = seat.focus {
                        fields.push((
                            "focus",
                            Value::object([
                                ("window", count(focus.window)),
                                ("region", Value::text(focus.region.word())),
                            ]),
                        ));
                    }
                    maybe(&mut fields, "hover", seat.hover.as_deref());
                    maybe(&mut fields, "selection", seat.selection.as_deref());
                    maybe(&mut fields, "drag", seat.drag.as_deref());
                    Value::object(fields)
                })
                .collect(),
        )
    }

    /// The state `value` writes, if it is one this module writes and it holds
    /// to every rule it is held to; none otherwise.
    pub(crate) fn from_value(value: &Value) -> Option<Self> {
        let mut top = Fields::of(value)?;
        let windows = top
            .array("windows")?
            .iter()
            .map(Window::read)
            .collect::<Option<Vec<_>>>()?;
        let pages = top
            .array("pages")?
            .iter()
            .map(|page| Page::read(page).map(|page| (page.id, page)))
            .collect::<Option<Vec<_>>>()?;
        let drawer = Drawer::read(top.take("drawer")?)?;
        let sidebar = shown(top.take("sidebar")?)?;
        let footer = shown(top.take("footer")?)?;
        let mut info = Fields::of(top.take("info")?)?;
        let expanded = info.boolean("expanded")?;
        let message = info.optional_text("message").ok()?;
        info.done()?;
        let mut prompt = Fields::of(top.take("prompt")?)?;
        let draft = prompt.text("draft")?;
        let asking = match prompt.optional("asking") {
            None => None,
            Some(asking) => Some(Asking::read(asking)?),
        };
        prompt.done()?;
        let seats = top
            .array("seats")?
            .iter()
            .map(Seat::read)
            .collect::<Option<Vec<_>>>()?;
        let settings = Chosen::read_back(top.take("settings")?)?;
        let next = top.id("next")?;
        top.done()?;
        let unique = pages.len();
        let pages: BTreeMap<u64, Page> = pages.into_iter().collect();
        let seat_count = seats.len();
        let seats: BTreeMap<u64, Seat> = seats.into_iter().collect();
        let state = Self {
            windows,
            pages,
            drawer,
            sidebar,
            footer,
            message,
            expanded,
            draft,
            asking,
            seats,
            settings,
            next,
        };
        (state.pages.len() == unique && state.seats.len() == seat_count && state.holds())
            .then_some(state)
    }

    /// Whether the state holds to every rule a command keeps it to: what
    /// [`State::from_value`] asks of a state it did not see made.
    pub(crate) fn holds(&self) -> bool {
        self.rules().iter().all(|(_, kept)| *kept)
    }

    /// Every rule a command keeps the state to, by what it says, and whether
    /// the state keeps it.
    ///
    /// **One clause, one rule**: each is asked on its own, never inside
    /// another, so a test breaks each alone and reads that it alone is broken
    /// (`rule_breakers`) — a clause asked inside another is one that could be
    /// lost with every state the other refuses still refused, and nothing
    /// told. A page is held under its own identifier by the reading itself,
    /// which keys each page by the identifier it writes.
    fn rules(&self) -> Vec<(&str, bool)> {
        let mut rules = self.window_rules();
        rules.extend(self.page_rules());
        rules.extend(self.drawer_rules());
        rules.extend(self.line_rules());
        rules
    }

    /// The rules of the windows.
    fn window_rules(&self) -> Vec<(&str, bool)> {
        let window_ids: Vec<u64> = self.windows.iter().map(|window| window.id).collect();
        let windows = |rule: &dyn Fn(&Window) -> bool| self.windows.iter().all(rule);
        vec![
            ("the interface holds a window", !self.windows.is_empty()),
            (
                "the windows are at most their bound",
                self.windows.len() <= limit(KERNEL.shell_windows_max),
            ),
            ("no two windows share an identifier", once(&window_ids)),
            (
                "a window's identifier is one at least",
                windows(&|window| window.id >= 1),
            ),
            (
                "a window's identifier was given",
                windows(&|window| window.id < self.next),
            ),
            (
                "a window's stack holds a page once",
                windows(&|window| once(&window.pages)),
            ),
            (
                "a window's stack holds pages the state holds",
                windows(&|window| self.holds_pages(&window.pages)),
            ),
            (
                "a window's focus is on its stack, and on nothing only when it is empty",
                windows(&|window| in_front(&window.pages, window.focused)),
            ),
            (
                "a window shows no list of the drawer's",
                windows(&|window| {
                    window.pages.iter().all(|id| {
                        self.pages
                            .get(id)
                            .is_none_or(|page| !page.kind.drawer_only())
                    })
                }),
            ),
            (
                "a window's size is within the smallest and the largest",
                windows(&|window| {
                    window.size.is_none_or(|(width, height)| {
                        (i64::from(KERNEL.shell_window_min_width)..=WINDOW_MAX).contains(&width)
                            && (i64::from(KERNEL.shell_window_min_height)..=WINDOW_MAX)
                                .contains(&height)
                    })
                }),
            ),
            (
                "a window's corner is within an X position's sixteen bits",
                windows(&|window| {
                    window.place.is_none_or(|(x, y)| {
                        (PLACE_MIN..=PLACE_MAX).contains(&x) && (PLACE_MIN..=PLACE_MAX).contains(&y)
                    })
                }),
            ),
        ]
    }

    /// The rules of the pages.
    fn page_rules(&self) -> Vec<(&str, bool)> {
        let pages = |rule: &dyn Fn(&Page) -> bool| self.pages.values().all(rule);
        let shown_somewhere = |id: &u64| {
            self.drawer.pages.contains(id)
                || self.windows.iter().any(|window| window.pages.contains(id))
        };
        vec![
            (
                "the pages are at most their bound",
                self.pages.len() <= limit(KERNEL.shell_pages_max),
            ),
            (
                "a page's identifier is one at least",
                pages(&|page| page.id >= 1),
            ),
            (
                "a page's identifier was given",
                pages(&|page| page.id < self.next),
            ),
            (
                "a page's identifier is no window's",
                pages(&|page| self.windows.iter().all(|window| window.id != page.id)),
            ),
            (
                "a page is shown somewhere",
                pages(&|page| shown_somewhere(&page.id)),
            ),
            (
                "a page has a topic where its kind takes one, and only there",
                pages(&|page| {
                    !matches!(
                        (page.kind.topic(), &page.topic),
                        (Topic::None, Some(_)) | (Topic::Required, None)
                    )
                }),
            ),
            (
                "a page's topic is a word",
                pages(&|page| page.topic.as_deref().is_none_or(is_word)),
            ),
            (
                "a page's sub-pages are at most their bound",
                pages(&|page| page.trail.len() <= limit(KERNEL.shell_trail_max)),
            ),
            (
                "a page's sub-pages are words",
                pages(&|page| page.trail.iter().all(|step| is_word(step))),
            ),
            (
                "a page is the one of its kind and topic",
                pages(&|page| {
                    self.pages
                        .values()
                        .filter(|other| other.kind == page.kind && other.topic == page.topic)
                        .count()
                        == 1
                }),
            ),
        ]
    }

    /// The rules of the drawer.
    fn drawer_rules(&self) -> Vec<(&str, bool)> {
        vec![
            (
                "the drawer's pages are at most their bound",
                self.drawer.pages.len() <= limit(KERNEL.shell_drawer_pages_max),
            ),
            ("the drawer holds a page once", once(&self.drawer.pages)),
            (
                "the drawer holds pages the state holds",
                self.holds_pages(&self.drawer.pages),
            ),
            (
                "the drawer's focus is on a page of it, and on nothing only when it is empty",
                in_front(&self.drawer.pages, self.drawer.focused),
            ),
            (
                "the drawer is collapsed only when it holds a page",
                !self.drawer.collapsed || !self.drawer.pages.is_empty(),
            ),
            (
                "the drawer's share is within its bound",
                (1..=KERNEL.shell_drawer_share_max).contains(&self.drawer.share),
            ),
        ]
    }

    /// The rules of the info line, the prompt and the seats.
    fn line_rules(&self) -> Vec<(&str, bool)> {
        let message = |rule: &dyn Fn(&str) -> bool| self.message.as_deref().is_none_or(rule);
        let within =
            |text: &str| u64::try_from(text.len()).unwrap_or(u64::MAX) <= KERNEL.shell_text_bytes;
        let parameters = |rule: &dyn Fn(&str, Option<&str>) -> bool| match &self.asking {
            None | Some(Asking::Commands) => true,
            Some(Asking::Parameters { command, parameter }) => rule(command, parameter.as_deref()),
        };
        let seats = |rule: &dyn Fn(&Seat) -> bool| self.seats.values().all(rule);
        vec![
            (
                "the draft is at most its bound in bytes",
                within(&self.draft),
            ),
            (
                "a message says something",
                message(&|said| !said.is_empty()),
            ),
            ("a message is at most its bound in bytes", message(&within)),
            (
                "a message is all a reader sees",
                message(&|said| {
                    !said
                        .chars()
                        .any(|character| character != '\n' && nc_schema::unseen(character))
                }),
            ),
            (
                "the prompt asks of a command by its name",
                parameters(&|command, _| {
                    nc_schema::descriptor::command_name_is_well_formed(command)
                }),
            ),
            (
                "the parameter the prompt asks for is a word",
                parameters(&|_, parameter| parameter.is_none_or(is_word)),
            ),
            (
                "a seat is one the bound allows",
                self.seats
                    .keys()
                    .all(|id| (1..=u64::from(KERNEL.shell_seats_max)).contains(id)),
            ),
            (
                "a seat's focus is on a window the state holds",
                seats(&|seat| {
                    seat.focus.is_none_or(|focus| {
                        self.windows.iter().any(|window| window.id == focus.window)
                    })
                }),
            ),
            (
                "a seat's keyboard is on the drawer only while it shows something open",
                seats(&|seat| {
                    self.drawer_open()
                        || seat
                            .focus
                            .is_none_or(|focus| focus.region != Region::Drawer)
                }),
            ),
            (
                "what a seat hovers, selects or drags is a word",
                seats(&|seat| {
                    [&seat.hover, &seat.selection, &seat.drag]
                        .into_iter()
                        .all(|target| target.as_deref().is_none_or(is_word))
                }),
            ),
        ]
    }

    /// Whether every one of `ids` is a page the state holds.
    fn holds_pages(&self, ids: &[u64]) -> bool {
        ids.iter().all(|id| self.pages.contains_key(id))
    }
}

/// Whether no identifier is twice in `ids`.
fn once(ids: &[u64]) -> bool {
    let mut seen = ids.to_vec();
    seen.sort_unstable();
    seen.dedup();
    seen.len() == ids.len()
}

/// Whether `focused` is one of `ids`, or nothing where `ids` is empty: the
/// page in front of a stack.
fn in_front(ids: &[u64], focused: Option<u64>) -> bool {
    match focused {
        Some(id) => ids.contains(&id),
        None => ids.is_empty(),
    }
}

impl Window {
    /// The window `value` writes, if it is one.
    fn read(value: &Value) -> Option<Self> {
        let mut fields = Fields::of(value)?;
        let id = fields.id("id")?;
        let shown = fields.boolean("shown")?;
        let pages = fields.ids("pages")?;
        let focused = fields.optional_id("focused").ok()?;
        let size = fields.pair("width", "height").ok()?;
        let place = fields.pair("x", "y").ok()?;
        fields.done()?;
        Some(Self {
            id,
            shown,
            size,
            place,
            pages,
            focused,
        })
    }
}

impl Page {
    /// The page `value` writes, if it is one.
    fn read(value: &Value) -> Option<Self> {
        let mut fields = Fields::of(value)?;
        let id = fields.id("id")?;
        let kind = Kind::of(&fields.text("kind")?)?;
        let topic = fields.optional_text("topic").ok()?;
        let trail = fields
            .array("trail")?
            .iter()
            .map(|step| match step {
                Value::Text(text) => Some(text.clone()),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()?;
        let scroll = u32::try_from(fields.integer("scroll")?).ok()?;
        fields.done()?;
        Some(Self {
            id,
            kind,
            topic,
            trail,
            scroll,
        })
    }
}

impl Drawer {
    /// The drawer `value` writes, if it is one.
    fn read(value: &Value) -> Option<Self> {
        let mut fields = Fields::of(value)?;
        let pages = fields.ids("pages")?;
        let focused = fields.optional_id("focused").ok()?;
        let collapsed = fields.boolean("collapsed")?;
        let toward = Place::of(&fields.text("toward")?)?;
        let flipped = fields.boolean("flipped")?;
        let share = u32::try_from(fields.integer("share")?).ok()?;
        fields.done()?;
        Some(Self {
            pages,
            focused,
            collapsed,
            toward,
            flipped,
            share,
        })
    }
}

impl Asking {
    /// What the prompt asks for, as `value` writes it, if it is that.
    fn read(value: &Value) -> Option<Self> {
        let mut fields = Fields::of(value)?;
        let asking = match fields.text("for")?.as_str() {
            "commands" => Self::Commands,
            "parameters" => Self::Parameters {
                command: fields.text("command")?,
                parameter: fields.optional_text("parameter").ok()?,
            },
            _ => return None,
        };
        fields.done()?;
        Some(asking)
    }
}

impl Seat {
    /// The seat `value` writes, and its number, if it is one.
    fn read(value: &Value) -> Option<(u64, Self)> {
        let mut fields = Fields::of(value)?;
        let id = fields.id("seat")?;
        let latest = By::of(&fields.text("latest")?)?;
        let focus = match fields.optional("focus") {
            None => None,
            Some(focus) => {
                let mut focus = Fields::of(focus)?;
                let window = focus.id("window")?;
                let region = Region::of(&focus.text("region")?)?;
                focus.done()?;
                Some(Focus { window, region })
            }
        };
        let hover = fields.optional_text("hover").ok()?;
        let selection = fields.optional_text("selection").ok()?;
        let drag = fields.optional_text("drag").ok()?;
        fields.done()?;
        Some((
            id,
            Self {
                focus,
                hover,
                selection,
                latest,
                drag,
            },
        ))
    }
}

/// Whether `value` is `{"shown":<bool>}`, and which.
fn shown(value: &Value) -> Option<bool> {
    let mut fields = Fields::of(value)?;
    let shown = fields.boolean("shown")?;
    fields.done()?;
    Some(shown)
}

/// A field that is there and is not what it must be.
#[derive(Debug)]
struct Unread;

/// An object read strictly: every field taken by name, and none left over.
struct Fields<'a> {
    held: &'a BTreeMap<String, Value>,
    taken: usize,
}

impl<'a> Fields<'a> {
    /// The fields of `value`, if it is an object.
    fn of(value: &'a Value) -> Option<Self> {
        match value {
            Value::Object(held) => Some(Self { held, taken: 0 }),
            _ => None,
        }
    }

    /// The field `name`, if it is there.
    fn optional(&mut self, name: &str) -> Option<&'a Value> {
        let value = self.held.get(name);
        if value.is_some() {
            self.taken = self.taken.saturating_add(1);
        }
        value
    }

    /// The field `name`, which must be there.
    fn take(&mut self, name: &str) -> Option<&'a Value> {
        self.optional(name)
    }

    /// Whether every field was taken.
    fn done(&self) -> Option<()> {
        (self.taken == self.held.len()).then_some(())
    }

    fn boolean(&mut self, name: &str) -> Option<bool> {
        match self.take(name)? {
            Value::Bool(value) => Some(*value),
            _ => None,
        }
    }

    fn integer(&mut self, name: &str) -> Option<i64> {
        match self.take(name)? {
            Value::Integer(value) => Some(*value),
            _ => None,
        }
    }

    fn id(&mut self, name: &str) -> Option<u64> {
        u64::try_from(self.integer(name)?).ok()
    }

    fn optional_id(&mut self, name: &str) -> Result<Option<u64>, Unread> {
        match self.optional(name) {
            None => Ok(None),
            Some(Value::Integer(value)) => u64::try_from(*value).map(Some).map_err(|_| Unread),
            Some(_) => Err(Unread),
        }
    }

    fn text(&mut self, name: &str) -> Option<String> {
        match self.take(name)? {
            Value::Text(text) => Some(text.clone()),
            _ => None,
        }
    }

    fn optional_text(&mut self, name: &str) -> Result<Option<String>, Unread> {
        match self.optional(name) {
            None => Ok(None),
            Some(Value::Text(text)) => Ok(Some(text.clone())),
            Some(_) => Err(Unread),
        }
    }

    fn array(&mut self, name: &str) -> Option<&'a [Value]> {
        match self.take(name)? {
            Value::Array(items) => Some(items),
            _ => None,
        }
    }

    fn ids(&mut self, name: &str) -> Option<Vec<u64>> {
        self.array(name)?
            .iter()
            .map(|item| match item {
                Value::Integer(value) => u64::try_from(*value).ok(),
                _ => None,
            })
            .collect()
    }

    /// Two integer fields that are there together or not at all.
    fn pair(&mut self, first: &str, second: &str) -> Result<Option<(i64, i64)>, Unread> {
        match (self.optional(first), self.optional(second)) {
            (None, None) => Ok(None),
            (Some(Value::Integer(one)), Some(Value::Integer(two))) => Ok(Some((*one, *two))),
            _ => Err(Unread),
        }
    }
}

/// The state as the store keeps it: its shape, and it.
pub(crate) fn snapshot(state: &State) -> String {
    Value::object([
        ("shape", Value::Integer(SHAPE)),
        ("shell", state.to_value()),
    ])
    .to_canonical()
}

/// What a kept text holds: the state, or the shape it named — zero when it
/// named none or was not read whole — when it is not one this build reads.
pub(crate) fn unkept(text: &str) -> Result<State, u64> {
    let value = nc_canon::parse(text).map_err(|_| 0_u64)?;
    let mut fields = Fields::of(&value).ok_or(0_u64)?;
    let shape = match fields.take("shape") {
        Some(Value::Integer(shape)) => *shape,
        _ => return Err(0),
    };
    let named = u64::try_from(shape).unwrap_or(0);
    if shape != SHAPE {
        return Err(named);
    }
    let state = fields.take("shell").and_then(State::from_value);
    match (state, fields.done()) {
        (Some(state), Some(())) => Ok(state),
        _ => Err(named),
    }
}

/// The state the store keeps for `realm`, read back as the kernel opens: the
/// first run's, logged as set aside, when it is not one this build reads or
/// the store could not be read.
pub(crate) async fn restored<S: StoreBackend, L: crate::logging::LogSink>(
    store: &S,
    realm: &OpaqueId,
    log: &L,
) -> State {
    let kept = store.get(realm, crate::settings::HOLDER, KEY).await;
    let read = match kept {
        Ok(None) => return State::default(),
        Ok(Some(entry)) => unkept(&entry.value),
        Err(_) => Err(0),
    };
    read.unwrap_or_else(|found| {
        log.record(&nc_log::events::KernelShellDiscarded { found });
        State::default()
    })
}

/// Saves the state to the store when it changed since it was last kept,
/// answering what came of it: nothing when there was nothing to save.
///
/// Under the bus, as the pump runs, so no tree changes the state while it is
/// written; and the state is marked kept only when what was written is still
/// what the kernel holds.
pub(crate) async fn kept<E: Environment>(shared: &Shared<E>) -> Option<Result<(), ErrorCode>> {
    let (text, at) = {
        let cell = dispatch::lock(&shared.shell);
        if !cell.dirty {
            return None;
        }
        (snapshot(&cell.state), cell.released)
    };
    match shared.store.keep(&shared.realm, KEY, &text).await {
        Ok(()) => {
            let mut cell = dispatch::lock(&shared.shell);
            if cell.released == at {
                cell.dirty = false;
            }
            Some(Ok(()))
        }
        Err(refused) => {
            let code = crate::units::commit_refused(&refused).code();
            shared
                .log
                .record(&nc_log::events::KernelShellUnkept { code });
            Some(Err(code))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use nc_canon::Value;
    use nc_catalog::{ErrorCode, KERNEL};
    use serde_json::json;

    use super::{
        Asking, By, Cell, Focus, Held, Kind, Known, PLACE_MAX, PLACE_MIN, Place, Region, SHAPE,
        State, Verb, WINDOW_MAX, WORD_CHARS, changed, is_word, snapshot, unkept,
    };
    use crate::builtin::Builtin;
    use crate::dispatch::{Refusal, RefusalKind};

    /// What a test's kernel knows: one setting, `mcp`, with its read, and one
    /// surface.
    struct Fake;

    impl Known for Fake {
        fn arguments_of(&self, name: &str) -> Option<Vec<String>> {
            match name {
                "settings.mcp.read" => Some(Vec::new()),
                "settings.mcp.set" => Some(vec!["on".to_owned()]),
                _ => None,
            }
        }

        fn surface(&self, id: &str) -> bool {
            id == "surface-1"
        }
    }

    /// `state` once `verb` ran with `args`, which must be allowed.
    fn ran(state: &State, verb: Verb, args: &serde_json::Value) -> State {
        let mut next = state.clone();
        changed(&mut next, verb, args, &Fake)
            .unwrap_or_else(|refused| panic!("{}: {args}: {refused}", verb.name()));
        assert!(
            next.holds(),
            "{} left a state that breaks a rule",
            verb.name()
        );
        next
    }

    /// What `verb` with `args` is refused with over `state`, which it leaves
    /// as it was.
    fn refused(state: &State, verb: Verb, args: &serde_json::Value) -> Refusal {
        let mut next = state.clone();
        let refusal = changed(&mut next, verb, args, &Fake)
            .expect_err(&format!("{} {args} is refused", verb.name()));
        assert_eq!(&next, state, "a refused change changed the state");
        refusal
    }

    /// The kind and the code of a refusal.
    fn said(refusal: &Refusal) -> (RefusalKind, ErrorCode) {
        (refusal.kind(), refusal.code())
    }

    /// The window numbered `id` of `state`.
    fn window(state: &State, id: u64) -> &super::Window {
        state.window(id).expect("the window is held")
    }

    /// The page of `kind` open in `state`.
    fn page_of(state: &State, kind: Kind) -> u64 {
        state
            .pages
            .values()
            .find(|page| page.kind == kind)
            .map_or_else(|| panic!("no {kind:?} page is open"), |page| page.id)
    }

    /// The declared commands of the interface are the ones this module
    /// implements.
    ///
    /// **Both ways and every field a dispatch is decided by**: each verb is
    /// declared, under the name it runs, taking the arguments this module
    /// reads and no other; `Read` for `shell.read` and `Write` for the rest,
    /// on `Feature::"interface"`; and each a read that changes nothing undo
    /// takes back, so none opens a unit. And every command the catalog
    /// declares in the interface's namespaces is a verb here.
    #[test]
    fn the_declared_commands_of_the_interface_are_the_ones_this_module_implements() {
        for verb in Verb::ALL {
            let name = verb.name();
            let declared =
                nc_catalog::command(name).unwrap_or_else(|| panic!("`{name}` is declared"));
            let takes: BTreeSet<&str> = declared.arguments.iter().copied().collect();
            let read: BTreeSet<&str> = verb.reads().iter().copied().collect();
            assert_eq!(takes, read, "`{name}` declares {:?}", declared.arguments);
            assert_eq!(declared.arguments.len(), verb.reads().len(), "`{name}`");
            let action = if verb.is_a_read() { "Read" } else { "Write" };
            assert_eq!(
                (
                    declared.action,
                    declared.resource_selector,
                    declared.mutating
                ),
                (action, "feature:interface", false),
                "`{name}`"
            );
            assert!(
                matches!(declared.reversibility, nc_catalog::Reversibility::Read),
                "`{name}` is undone by nothing, so it is a read"
            );
            assert_eq!(Builtin::named(name), Some(Builtin::Shell(verb)), "`{name}`");
        }
        let families = [
            "shell", "window", "leader", "page", "drawer", "sidebar", "footer", "info", "prompt",
        ];
        // The settings' namespace is shared with the settings kept in the
        // store and in the keybinding configuration (`crate::settings`), so
        // of it the three settings of this module are named one by one.
        let settings = ["footer", "language", "appearance"];
        let declared: BTreeSet<&str> = nc_catalog::COMMANDS
            .iter()
            .map(|command| command.name)
            .filter(|name| {
                families
                    .iter()
                    .any(|family| name.starts_with(&format!("{family}.")))
                    || settings
                        .iter()
                        .any(|setting| name.starts_with(&format!("settings.{setting}.")))
            })
            .collect();
        let verbs: BTreeSet<&str> = Verb::ALL.iter().map(|verb| verb.name()).collect();
        assert_eq!(declared, verbs);
        assert_eq!(verbs.len(), Verb::ALL.len(), "two verbs run one command");
        for family in families {
            assert!(
                crate::registry::RESERVED.contains(&family),
                "`{family}.` is a namespace the kernel keeps"
            );
        }
    }

    /// Each of the three settings is one word of its own closed list, its
    /// first until a person sets another, and a word outside it changes
    /// nothing.
    ///
    /// **The list is held three ways to one source**: what a set takes and
    /// refuses, what the command's schema lists — in the same order — and,
    /// for the language, the tags of the locales the catalog ships after
    /// `system`. A read answers the one object its set takes whole, and so
    /// does a set once made.
    #[test]
    fn each_setting_of_the_interface_is_one_word_of_its_own_list() {
        use super::{Chosen, Setting, answered};
        let first = State::default();
        assert_eq!(first.settings, Chosen::default());
        let fields: Vec<&str> = Setting::ALL.map(Setting::field).to_vec();
        assert_eq!(fields, ["visibility", "language", "mode"]);
        assert_eq!(
            Setting::Language.words(),
            ["system", "en", "pt-BR"],
            "`system`, then every locale the catalog ships"
        );
        let shipped: Vec<&str> = core::iter::once("system")
            .chain(nc_catalog::Locale::ALL.iter().map(|locale| locale.tag()))
            .collect();
        assert_eq!(
            Setting::Language.words(),
            shipped,
            "a locale the catalog ships is a language a person may choose"
        );
        assert_eq!(Setting::Footer.words(), ["always", "hidden"]);
        assert_eq!(Setting::Appearance.words(), ["system", "light", "dark"]);
        for setting in Setting::ALL {
            let (field, words) = (setting.field(), setting.words());
            assert_eq!(
                first.settings.word_of(setting),
                words[0],
                "{field}: the default"
            );
            assert_eq!(
                answered(setting, first.settings),
                format!("{{\"{field}\":\"{}\"}}", words[0])
            );
            let listed: Vec<String> = schema(setting.set().name())["properties"][field]["enum"]
                .as_array()
                .expect("a closed list")
                .iter()
                .map(|word| word.as_str().expect("a word").to_owned())
                .collect();
            assert_eq!(listed, words, "`{}`", setting.set().name());
            assert_eq!(
                (setting.read().name(), setting.set().name()),
                (
                    format!("settings.{}.read", name_of(setting)).as_str(),
                    format!("settings.{}.set", name_of(setting)).as_str()
                )
            );
            assert!(setting.read().is_a_read() && !setting.set().is_a_read());
            assert_eq!(Setting::of(setting.read()), Some(setting));
            assert_eq!(Setting::of(setting.set()), Some(setting));
            for word in words {
                let set = ran(&first, setting.set(), &json!({ field: word }));
                assert_eq!(set.settings.word_of(setting), *word);
                assert_eq!(
                    answered(setting, set.settings),
                    format!("{{\"{field}\":\"{word}\"}}")
                );
                // One setting set leaves the other two, and the rest of the
                // state, as they were.
                for other in Setting::ALL {
                    if other != setting {
                        assert_eq!(set.settings.word_of(other), first.settings.word_of(other));
                    }
                }
                let mut rest = set.clone();
                rest.settings = first.settings;
                if setting == Setting::Footer {
                    // What is shown now follows the footer's setting, as
                    // `a_hidden_footer_shown_by_a_tap_hides_again_once_no_keyboard_holds_it`
                    // holds.
                    rest.footer = first.footer;
                }
                assert_eq!(rest, first);
                // A read changes nothing.
                assert_eq!(ran(&set, setting.read(), &json!({})), set);
            }
            for other in [
                json!("System"),
                json!("fr"),
                json!(""),
                json!(1),
                json!(null),
            ] {
                let refusal = refused(&first, setting.set(), &json!({ field: other }));
                assert_eq!(
                    (refusal.kind(), refusal.code()),
                    (RefusalKind::Invalid, ErrorCode::ValidationArgumentsInvalid),
                    "{field}: {other}"
                );
            }
            let missing = refused(&first, setting.set(), &json!({}));
            assert_eq!(missing.code(), ErrorCode::ValidationArgumentsInvalid);
        }
        assert_eq!(Setting::of(Verb::FooterHide), None);
        assert_eq!(Setting::of(Verb::ShellRead), None);
        assert!(Verb::ShellRead.is_a_read() && !Verb::FooterHide.is_a_read());
    }

    /// The word a setting's two commands are named by.
    const fn name_of(setting: super::Setting) -> &'static str {
        match setting {
            super::Setting::Language => "language",
            super::Setting::Footer => "footer",
            super::Setting::Appearance => "appearance",
        }
    }

    /// A saved state's settings are read only as words of their lists, each
    /// there and nothing beside them.
    #[test]
    fn a_saved_states_settings_are_read_only_as_words_of_their_lists() {
        use super::Chosen;
        let chosen = Chosen {
            language: "pt-BR",
            visibility: "hidden",
            mode: "dark",
        };
        assert_eq!(Chosen::read_back(&chosen.to_value()), Some(chosen));
        assert_eq!(
            chosen.to_value().to_canonical(),
            r#"{"language":"pt-BR","mode":"dark","visibility":"hidden"}"#
        );
        let with = |field: &str, value: Option<Value>| {
            let mut fields: Vec<(&str, Value)> = [
                ("language", Value::text("pt-BR")),
                ("mode", Value::text("dark")),
                ("visibility", Value::text("hidden")),
            ]
            .into_iter()
            .filter(|(name, _)| *name != field)
            .collect();
            fields.extend(value.map(|value| (field, value)));
            Value::object(fields)
        };
        assert_eq!(
            Chosen::read_back(&with("mode", Some(Value::text("dark")))),
            Some(chosen)
        );
        for field in ["language", "visibility", "mode"] {
            assert_eq!(Chosen::read_back(&with(field, None)), None, "no {field}");
            for other in [Value::text("Dark"), Value::text(""), Value::Bool(true)] {
                assert_eq!(
                    Chosen::read_back(&with(field, Some(other))),
                    None,
                    "{field}"
                );
            }
        }
        assert_eq!(Chosen::read_back(&with("more", Some(Value::Null))), None);
        assert_eq!(Chosen::read_back(&Value::Null), None);
        // And the state is set aside whole with them.
        let state = State {
            settings: chosen,
            ..State::default()
        };
        let text = snapshot(&state);
        assert_eq!(unkept(&text), Ok(state));
        assert!(unkept(&text.replace("\"mode\":\"dark\"", "\"mode\":\"dim\"")).is_err());
    }

    /// The schema of `command`, read off the catalog.
    fn schema(command: &str) -> serde_json::Value {
        let declared = nc_catalog::command(command).expect("declared");
        serde_json::from_str(declared.args_schema).expect("a schema is JSON")
    }

    /// Every bound the schemas declare is the one this module and the catalog
    /// hold.
    ///
    /// **Each schema's own number, held to its source**: a seat to
    /// `shell_seats_max`, a window's size to `shell_window_min_width` and
    /// `shell_window_min_height` and the sixteen bits of an X window's, its
    /// place to theirs, the drawer's share to `shell_drawer_share_max`, a
    /// scroll to what the state holds, a word to [`WORD_CHARS`], a text to
    /// `shell_text_bytes`, a command to the longest name there is — and every
    /// closed choice to the words this module reads.
    #[test]
    fn every_bound_the_schemas_declare_is_the_catalogs() {
        for verb in Verb::ALL {
            let properties = schema(verb.name())["properties"].clone();
            if let Some(seat) = properties.get("seat") {
                assert_eq!(
                    (seat["minimum"].as_i64(), seat["maximum"].as_u64()),
                    (Some(1), Some(u64::from(KERNEL.shell_seats_max))),
                    "`{}`",
                    verb.name()
                );
            }
            for name in ["topic", "target", "parameter"] {
                if let Some(word) = properties.get(name) {
                    assert_eq!(
                        word["maxLength"].as_u64(),
                        Some(WORD_CHARS as u64),
                        "`{}` `{name}`",
                        verb.name()
                    );
                }
            }
            if let Some(place) = properties.get("in") {
                assert_eq!(place["enum"], json!(["workspace", "drawer"]));
            }
            if let Some(text) = properties.get("text") {
                assert_eq!(text["maxLength"].as_u64(), Some(KERNEL.shell_text_bytes));
            }
        }
        let resize = schema("window.resize");
        let size = &resize["properties"];
        assert_eq!(
            (
                size["width"]["minimum"].as_u64(),
                size["height"]["minimum"].as_u64(),
                size["width"]["maximum"].as_i64(),
                size["height"]["maximum"].as_i64()
            ),
            (
                Some(u64::from(KERNEL.shell_window_min_width)),
                Some(u64::from(KERNEL.shell_window_min_height)),
                Some(WINDOW_MAX),
                Some(WINDOW_MAX)
            )
        );
        assert_eq!(
            (
                KERNEL.shell_window_min_width,
                KERNEL.shell_window_min_height
            ),
            (320, 240),
            "the smallest window every page is built for"
        );
        let moved = schema("window.move");
        for axis in ["x", "y"] {
            assert_eq!(
                (
                    moved["properties"][axis]["minimum"].as_i64(),
                    moved["properties"][axis]["maximum"].as_i64()
                ),
                (Some(PLACE_MIN), Some(PLACE_MAX))
            );
        }
        assert_eq!(
            schema("drawer.resize")["properties"]["share"]["maximum"].as_u64(),
            Some(u64::from(KERNEL.shell_drawer_share_max))
        );
        assert_eq!(
            schema("page.scroll")["properties"]["to"]["maximum"].as_u64(),
            Some(u64::from(u32::MAX))
        );
        assert_eq!(
            schema("prompt.ask")["properties"]["command"]["maxLength"].as_u64(),
            Some(nc_schema::descriptor::NAME_MAX as u64)
        );
        let kinds: Vec<&str> = Kind::ALL.iter().map(|kind| kind.word()).collect();
        assert_eq!(
            schema("page.open")["properties"]["kind"]["enum"],
            json!(kinds)
        );
        let regions: Vec<&str> = Region::ALL.iter().map(|region| region.word()).collect();
        assert_eq!(
            schema("shell.focus")["properties"]["region"]["enum"],
            json!(regions)
        );
        assert_eq!(
            schema("drawer.toward")["properties"]["region"]["enum"],
            json!(["workspace", "drawer"])
        );
        assert_eq!(
            schema("shell.hover")["properties"]["by"]["enum"],
            json!([By::Pointer.word(), By::Keyboard.word()])
        );
    }

    /// The first run shows nothing but the footer.
    ///
    /// **One window, hidden and ready**, holding no page, the drawer, the
    /// sidebar and the info line empty and the footer shown (§12.2) — so the
    /// first show of it is the footer alone.
    #[test]
    fn the_first_run_shows_nothing_but_the_footer() {
        let first = State::default();
        assert!(first.holds());
        assert_eq!(first.windows.len(), 1);
        assert!(!window(&first, 1).shown, "a window kept hidden and ready");
        assert!(first.pages.is_empty() && first.drawer.pages.is_empty());
        assert_eq!(window(&first, 1).pages.len(), 0);
        assert!(first.footer && !first.sidebar);
        assert_eq!((first.message.as_deref(), first.draft.as_str()), (None, ""));
        assert_eq!(first.asking, None);
        assert_eq!(first.drawer.toward, Place::Workspace);
    }

    /// A Leader tap shows the interface and the footer and puts the keyboard
    /// on the prompt line; a second opens the leader menu in the drawer; a
    /// third sends the next page to the other place; and none hides anything.
    ///
    /// **§12.8's three rules, in order**, from the footer hidden: the first
    /// tap shows the window and the footer and puts seat 1's keyboard on its
    /// prompt line; the second, the keyboard there, opens the leader menu in
    /// the drawer and gives it the keyboard; the third, the menu open, flips
    /// where the next page goes — a fourth flips it back, a fifth again — and
    /// the page opened then goes to the drawer, closing the menu and the flip
    /// with it. A tap with the keyboard on the prompt of a hidden window shows
    /// it rather than open the menu, one with the menu open in a window hidden
    /// or closed shows it rather than flip, every tap shows the footer, and
    /// the seat a tap names is the one that moves.
    #[test]
    fn a_leader_tap_follows_the_three_rules_and_never_hides_the_interface() {
        let hidden = ran(&State::default(), Verb::FooterHide, &json!({}));
        assert!(!hidden.footer);
        let first = ran(&hidden, Verb::LeaderTap, &json!({}));
        assert!(window(&first, 1).shown && first.footer);
        assert_eq!(
            first.seats[&1].focus,
            Some(Focus {
                window: 1,
                region: Region::Prompt
            })
        );
        assert!(first.drawer.pages.is_empty(), "the first tap opens no menu");
        let second = ran(&first, Verb::LeaderTap, &json!({}));
        let menu = page_of(&second, Kind::Leader);
        assert_eq!(second.drawer.focused, Some(menu));
        assert!(second.leader_open() && window(&second, 1).shown);
        assert_eq!(
            second.seats[&1].focus.map(|focus| focus.region),
            Some(Region::Drawer)
        );
        let third = ran(&second, Verb::LeaderTap, &json!({}));
        assert!(third.drawer.flipped && third.leader_open() && window(&third, 1).shown);
        let fourth = ran(&third, Verb::LeaderTap, &json!({}));
        assert!(!fourth.drawer.flipped, "a tap flips it back");
        let fifth = ran(&fourth, Verb::LeaderTap, &json!({}));
        let opened = ran(&fifth, Verb::PageOpen, &json!({"kind": "settings"}));
        let settings = page_of(&opened, Kind::Settings);
        assert_eq!(opened.drawer.pages, [settings], "the menu closed");
        assert!(window(&opened, 1).pages.is_empty(), "sent to the drawer");
        assert!(!opened.drawer.flipped, "for the next page only");
        let next = ran(&opened, Verb::PageOpen, &json!({"kind": "help"}));
        assert_eq!(window(&next, 1).pages, [page_of(&next, Kind::Help)]);
        let hidden_again = ran(&first, Verb::WindowHide, &json!({}));
        let shown_again = ran(&hidden_again, Verb::LeaderTap, &json!({}));
        assert!(window(&shown_again, 1).shown);
        assert!(
            shown_again.drawer.pages.is_empty(),
            "a hidden window is shown first"
        );
        // **A menu no window shows keeps no tap from showing one**: the menu
        // open in a window hidden — or in the last one closed, which hides it
        // — is rule 1's, and the tap shows the window and puts the keyboard
        // on the prompt, the flip untouched; the tap after it, the menu now
        // seen, flips.
        for hiding in [Verb::WindowHide, Verb::WindowClose] {
            let unseen = ran(&second, hiding, &json!({}));
            assert!(unseen.leader_open() && !window(&unseen, 1).shown);
            let shown = ran(&unseen, Verb::LeaderTap, &json!({}));
            assert!(window(&shown, 1).shown && shown.footer, "{}", hiding.name());
            assert!(!shown.drawer.flipped, "{}", hiding.name());
            assert_eq!(
                shown.seats[&1].focus.map(|focus| focus.region),
                Some(Region::Prompt),
                "{}",
                hiding.name()
            );
            let flipped = ran(&shown, Verb::LeaderTap, &json!({}));
            assert!(flipped.drawer.flipped, "{}", hiding.name());
        }
        // With the menu open and the footer hidden, a tap flips and shows the
        // footer.
        let menu_seen = ran(&second, Verb::FooterHide, &json!({}));
        let flipped = ran(&menu_seen, Verb::LeaderTap, &json!({}));
        assert!(
            flipped.drawer.flipped && flipped.footer,
            "a tap with the menu open left the footer hidden"
        );
        let other = ran(&first, Verb::LeaderTap, &json!({"seat": 2}));
        assert!(
            other.drawer.pages.is_empty(),
            "seat 2's keyboard was not there"
        );
        assert_eq!(
            other.seats[&2].focus.map(|focus| focus.region),
            Some(Region::Prompt)
        );
    }

    /// One tap opens the leader menu from a prompt line the keyboard is on,
    /// however the footer is set.
    ///
    /// **A prompt line the keyboard is on is seen** (§12.3, §12.8 rule 2): a
    /// footer is always shown, or it is the design system's folding footer,
    /// open while a keyboard is inside it — so the state's "shown now" false,
    /// by `footer.hide` under either setting or by the keyboard reaching the
    /// line by a click or Tab under `hidden`, which begins no reveal, is no
    /// reason for a tap to show a line the person already sees and open
    /// nothing. The tap opens the menu, gives it the keyboard and shows the
    /// footer.
    #[test]
    fn a_tap_opens_the_menu_from_a_prompt_line_the_keyboard_is_on_however_the_footer_is_set() {
        let first = ran(&State::default(), Verb::LeaderTap, &json!({}));
        let hidden_setting = json!({"visibility": "hidden"});
        let folded = ran(&first, Verb::SettingsFooterSet, &hidden_setting);
        let away = ran(
            &ran(&first, Verb::ShellFocus, &json!({"region": "workspace"})),
            Verb::SettingsFooterSet,
            &hidden_setting,
        );
        assert!(!away.footer, "the setting hidden left the footer shown");
        for (how, unseen) in [
            (
                "`footer.hide` always",
                ran(&first, Verb::FooterHide, &json!({})),
            ),
            (
                "`footer.hide` hidden",
                ran(&folded, Verb::FooterHide, &json!({})),
            ),
            (
                "the keyboard reaching the line hidden",
                ran(&away, Verb::ShellFocus, &json!({"region": "prompt"})),
            ),
        ] {
            assert!(window(&unseen, 1).shown && !unseen.footer, "{how}");
            assert_eq!(
                unseen.seats[&1].focus.map(|focus| focus.region),
                Some(Region::Prompt),
                "{how}"
            );
            let tapped = ran(&unseen, Verb::LeaderTap, &json!({}));
            assert!(
                tapped.leader_open() && tapped.footer && window(&tapped, 1).shown,
                "{how}: a tap left the menu shut over a prompt line the person saw"
            );
            assert_eq!(
                tapped.seats[&1].focus.map(|focus| focus.region),
                Some(Region::Drawer),
                "{how}"
            );
        }
    }

    /// Each command of a leader menu's entry that opens a page leaves the
    /// state `page.open` naming its kind alone leaves — from the first run,
    /// with the drawer's direction turned, with the menu open and its next
    /// target flipped, and with the page open already — and reads no
    /// argument.
    #[test]
    fn each_entry_of_the_leader_menu_opens_its_page_as_page_open_does() {
        let first = State::default();
        let toward = ran(&first, Verb::DrawerToward, &json!({"region": "drawer"}));
        let tapped = ran(
            &ran(
                &ran(&first, Verb::LeaderTap, &json!({})),
                Verb::LeaderTap,
                &json!({}),
            ),
            Verb::LeaderTap,
            &json!({}),
        );
        assert!(
            tapped.drawer.flipped,
            "the menu is open and its target flipped"
        );
        let opened = ran(&first, Verb::PageOpen, &json!({"kind": "help"}));
        for (verb, kind) in [
            (Verb::PageSettings, "settings"),
            (Verb::PageHelp, "help"),
            (Verb::PageDev, "dev"),
            (Verb::PageShortcuts, "shortcuts"),
            (Verb::PageActions, "actions"),
        ] {
            assert!(verb.reads().is_empty(), "{}", verb.name());
            for state in [&first, &toward, &tapped, &opened] {
                assert_eq!(
                    ran(state, verb, &json!({})),
                    ran(state, Verb::PageOpen, &json!({"kind": kind})),
                    "{}",
                    verb.name()
                );
            }
        }
    }

    /// A page opens where it is asked for or where the drawer's direction
    /// says, once for its kind and topic, and the drawer's lists open in the
    /// drawer alone.
    ///
    /// **Two views of one page** (§12.12): settings opened in the workspace
    /// area and then in the drawer is one page in both, and opening it again
    /// where it is adds nothing. The direction is kept (§12.9). A topic is
    /// refused where its kind takes none and missing where it needs one, a
    /// setting's topic is a setting the registry holds and a surface's a
    /// surface that is open; and a list of the drawer's asked for in the
    /// workspace area is refused.
    #[test]
    fn a_page_opens_once_where_it_is_asked_for_and_the_drawers_lists_in_the_drawer_alone() {
        let one = ran(
            &State::default(),
            Verb::PageOpen,
            &json!({"kind": "settings"}),
        );
        let settings = page_of(&one, Kind::Settings);
        assert_eq!(window(&one, 1).pages, [settings]);
        let two = ran(
            &one,
            Verb::PageOpen,
            &json!({"kind": "settings", "in": "drawer"}),
        );
        assert_eq!(
            (two.pages.len(), two.drawer.pages.as_slice()),
            (1, &[settings][..])
        );
        assert_eq!(window(&two, 1).pages, [settings], "two views of one page");
        assert_eq!(ran(&two, Verb::PageOpen, &json!({"kind": "settings"})), two);
        let toward = ran(&one, Verb::DrawerToward, &json!({"region": "drawer"}));
        let mcp = ran(
            &toward,
            Verb::PageOpen,
            &json!({"kind": "settings", "topic": "mcp"}),
        );
        assert_eq!(mcp.drawer.pages.len(), 1, "the direction is kept");
        let surface = ran(
            &one,
            Verb::PageOpen,
            &json!({"kind": "surface", "topic": "surface-1"}),
        );
        assert_eq!(surface.pages.len(), 2);
        for (args, kind, code) in [
            (
                json!({"kind": "home", "topic": "x"}),
                RefusalKind::Invalid,
                ErrorCode::ValidationArgumentsInvalid,
            ),
            (
                json!({"kind": "surface"}),
                RefusalKind::Invalid,
                ErrorCode::ValidationArgumentsInvalid,
            ),
            (
                json!({"kind": "settings", "topic": "nothing"}),
                RefusalKind::NotFound,
                ErrorCode::KernelShellUnknown,
            ),
            (
                json!({"kind": "surface", "topic": "surface-2"}),
                RefusalKind::NotFound,
                ErrorCode::KernelShellUnknown,
            ),
            (
                json!({"kind": "leader", "in": "workspace"}),
                RefusalKind::Invalid,
                ErrorCode::ValidationArgumentsInvalid,
            ),
            (
                json!({"kind": "help", "topic": "a b"}),
                RefusalKind::Invalid,
                ErrorCode::ValidationArgumentsInvalid,
            ),
            (
                json!({"kind": "help", "window": 9}),
                RefusalKind::NotFound,
                ErrorCode::KernelShellUnknown,
            ),
            (
                json!({"kind": "page"}),
                RefusalKind::Invalid,
                ErrorCode::ValidationArgumentsInvalid,
            ),
        ] {
            assert_eq!(
                said(&refused(&one, Verb::PageOpen, &args)),
                (kind, code),
                "{args}"
            );
        }
        for kind in ["leader", "commands", "shortcuts", "actions"] {
            let list = ran(&one, Verb::PageOpen, &json!({"kind": kind}));
            assert_eq!(list.drawer.pages.len(), 1, "{kind} opened in the drawer");
            assert_eq!(window(&list, 1).pages, [settings], "{kind}");
        }
    }

    /// The pages, the drawer's pages, the windows and a page's sub-pages are
    /// each held to their bound, and a page already open costs nothing.
    #[test]
    fn every_count_is_held_to_its_bound() {
        let mut state = State::default();
        let topics = usize::try_from(KERNEL.shell_pages_max).expect("small");
        for topic in 0..topics {
            state = ran(
                &state,
                Verb::PageOpen,
                &json!({"kind": "help", "topic": format!("t{topic}")}),
            );
        }
        assert_eq!(
            said(&refused(&state, Verb::PageOpen, &json!({"kind": "dev"}))),
            (RefusalKind::Invalid, ErrorCode::KernelShellFull)
        );
        ran(
            &state,
            Verb::PageOpen,
            &json!({"kind": "help", "topic": "t0"}),
        );
        let mut drawer = State::default();
        let drawn = usize::try_from(KERNEL.shell_drawer_pages_max).expect("small");
        for topic in 0..drawn {
            drawer = ran(
                &drawer,
                Verb::PageOpen,
                &json!({"kind": "help", "topic": format!("t{topic}"), "in": "drawer"}),
            );
        }
        assert_eq!(
            said(&refused(
                &drawer,
                Verb::PageOpen,
                &json!({"kind": "dev", "in": "drawer"})
            )),
            (RefusalKind::Invalid, ErrorCode::KernelShellFull)
        );
        let mut windows = State::default();
        for _ in 1..KERNEL.shell_windows_max {
            windows = ran(&windows, Verb::WindowOpen, &json!({}));
        }
        assert_eq!(
            windows.windows.len(),
            usize::try_from(KERNEL.shell_windows_max).expect("small")
        );
        assert_eq!(
            said(&refused(&windows, Verb::WindowOpen, &json!({}))),
            (RefusalKind::Invalid, ErrorCode::KernelShellFull)
        );
        let mut deep = ran(&State::default(), Verb::PageOpen, &json!({"kind": "help"}));
        for step in 0..KERNEL.shell_trail_max {
            deep = ran(&deep, Verb::PageInto, &json!({"topic": format!("s{step}")}));
        }
        assert_eq!(
            said(&refused(&deep, Verb::PageInto, &json!({"topic": "past"}))),
            (RefusalKind::Invalid, ErrorCode::KernelShellFull)
        );
        for (verb, args) in [
            (Verb::ShellFocus, json!({"seat": 5, "region": "prompt"})),
            (Verb::ShellFocus, json!({"seat": 0, "region": "prompt"})),
            (Verb::WindowResize, json!({"width": 319, "height": 240})),
            (Verb::WindowResize, json!({"width": 320, "height": 239})),
            (Verb::WindowResize, json!({"width": 65_536, "height": 240})),
            (Verb::WindowMove, json!({"x": -32_769, "y": 0})),
            (Verb::WindowMove, json!({"x": 0, "y": 32_768})),
            (Verb::DrawerResize, json!({"share": 50})),
            (Verb::DrawerResize, json!({"share": 0})),
            (Verb::PageScroll, json!({"page": 1, "to": -1})),
        ] {
            assert_eq!(
                said(&refused(&State::default(), verb, &args)).1,
                if verb == Verb::PageScroll {
                    ErrorCode::KernelShellUnknown
                } else {
                    ErrorCode::ValidationArgumentsInvalid
                },
                "{} {args}",
                verb.name()
            );
        }
        let sized = ran(
            &State::default(),
            Verb::WindowResize,
            &json!({"width": 320, "height": 240}),
        );
        assert_eq!(window(&sized, 1).size, Some((320, 240)));
        let placed = ran(
            &State::default(),
            Verb::WindowMove,
            &json!({"x": -32_768, "y": 32_767}),
        );
        assert_eq!(window(&placed, 1).place, Some((-32_768, 32_767)));
    }

    /// A text past its bytes is an argument refused, as its schema refuses
    /// one past its characters, and never the state full.
    ///
    /// **One character more than the bound, or as many as the bound each
    /// written in two bytes**: the schema counts the second within its
    /// `maxLength`, so the module's count in bytes is the one that refuses it,
    /// with `[NC-VAL-0002]` as the schema would.
    #[test]
    fn a_text_past_its_bytes_is_an_argument_refused() {
        let long = "a".repeat(usize::try_from(KERNEL.shell_text_bytes).expect("small") + 1);
        let wide = "\u{e9}".repeat(usize::try_from(KERNEL.shell_text_bytes).expect("small"));
        for verb in [Verb::InfoSay, Verb::PromptSet] {
            for text in [&long, &wide] {
                assert_eq!(
                    said(&refused(&State::default(), verb, &json!({"text": text}))),
                    (RefusalKind::Invalid, ErrorCode::ValidationArgumentsInvalid),
                    "{}",
                    verb.name()
                );
            }
        }
    }

    /// An identifier is never given past the last one canonical JSON writes,
    /// so no two windows or pages are read back as one number.
    ///
    /// **From a kept state whose next identifier is the largest a state
    /// writes**: it is read back, and a window or a page opened over it is
    /// refused with `[NC-KERN-0028]`, the state left as it was and still read
    /// back. The control, one identifier below, gives that last one — and the
    /// state it leaves is read back as itself, its window and its next both
    /// written as what they are.
    #[test]
    fn an_identifier_is_never_given_past_the_last_one_a_state_writes() {
        let at_the_last = State {
            next: super::NEXT_MAX,
            ..State::default()
        };
        let text = snapshot(&at_the_last);
        assert_eq!(unkept(&text), Ok(at_the_last.clone()));
        for (verb, args) in [
            (Verb::WindowOpen, json!({})),
            (Verb::PageOpen, json!({"kind": "help"})),
            (Verb::PageOpen, json!({"kind": "leader"})),
        ] {
            assert_eq!(
                said(&refused(&at_the_last, verb, &args)),
                (RefusalKind::Invalid, ErrorCode::KernelShellFull),
                "{} {args}",
                verb.name()
            );
        }
        let below = State {
            next: super::NEXT_MAX - 1,
            ..State::default()
        };
        let opened = ran(&below, Verb::WindowOpen, &json!({}));
        assert_eq!(
            (opened.windows[1].id, opened.next),
            (super::NEXT_MAX - 1, super::NEXT_MAX)
        );
        assert_eq!(unkept(&snapshot(&opened)), Ok(opened.clone()));
        assert_eq!(
            said(&refused(&opened, Verb::WindowOpen, &json!({}))),
            (RefusalKind::Invalid, ErrorCode::KernelShellFull)
        );
    }

    /// A page closes from one place, or from every place when it is named and
    /// no place is, and a page nothing shows is forgotten.
    ///
    /// **And the focus moves to the page before it**, or after it when it was
    /// first, in the window's stack and in the drawer alike.
    #[test]
    fn a_page_closes_from_one_place_or_from_every_place_and_is_forgotten_when_nothing_shows_it() {
        let mut state = State::default();
        for kind in ["home", "settings", "help"] {
            state = ran(&state, Verb::PageOpen, &json!({"kind": kind}));
        }
        let (home, settings, help) = (
            page_of(&state, Kind::Home),
            page_of(&state, Kind::Settings),
            page_of(&state, Kind::Help),
        );
        state = ran(
            &state,
            Verb::PageOpen,
            &json!({"kind": "settings", "in": "drawer"}),
        );
        state = ran(
            &state,
            Verb::PageFocus,
            &json!({"page": settings, "in": "workspace"}),
        );
        let closed = ran(&state, Verb::PageClose, &json!({}));
        assert_eq!(window(&closed, 1).pages, [home, help]);
        assert_eq!(window(&closed, 1).focused, Some(home), "the page before it");
        assert!(
            closed.pages.contains_key(&settings),
            "the drawer still shows it"
        );
        let everywhere = ran(&state, Verb::PageClose, &json!({"page": settings}));
        assert!(!everywhere.pages.contains_key(&settings));
        assert!(everywhere.drawer.pages.is_empty() && !everywhere.drawer.collapsed);
        let in_the_drawer = ran(&state, Verb::PageClose, &json!({"in": "drawer"}));
        assert_eq!(in_the_drawer.drawer.pages.len(), 0);
        assert_eq!(window(&in_the_drawer, 1).pages, [home, settings, help]);
        let first = ran(&state, Verb::PageFocus, &json!({"page": home}));
        let after = ran(&first, Verb::PageClose, &json!({}));
        assert_eq!(
            window(&after, 1).focused,
            Some(settings),
            "the page after it"
        );
        assert_eq!(
            said(&refused(
                &closed,
                Verb::PageClose,
                &json!({"page": settings, "in": "workspace"})
            )),
            (RefusalKind::NotFound, ErrorCode::KernelShellUnknown)
        );
        assert_eq!(
            said(&refused(&State::default(), Verb::PageClose, &json!({}))),
            (RefusalKind::NotFound, ErrorCode::KernelShellUnknown)
        );
        assert_eq!(
            said(&refused(
                &State::default(),
                Verb::PageFocus,
                &json!({"page": 99})
            )),
            (RefusalKind::NotFound, ErrorCode::KernelShellUnknown)
        );
    }

    /// The next and the previous page stop at the ends of the stack, and
    /// sub-pages are gone into and back out of.
    #[test]
    fn pages_step_within_their_stack_and_in_and_out_of_their_sub_pages() {
        let mut state = State::default();
        for kind in ["home", "settings", "help"] {
            state = ran(&state, Verb::PageOpen, &json!({"kind": kind}));
        }
        let ids = window(&state, 1).pages.clone();
        assert_eq!(window(&state, 1).focused, Some(ids[2]));
        let at_the_end = ran(&state, Verb::PageNext, &json!({}));
        assert_eq!(window(&at_the_end, 1).focused, Some(ids[2]));
        let back = ran(&state, Verb::PagePrevious, &json!({}));
        assert_eq!(window(&back, 1).focused, Some(ids[1]));
        let front = ran(
            &ran(&back, Verb::PagePrevious, &json!({})),
            Verb::PagePrevious,
            &json!({}),
        );
        assert_eq!(window(&front, 1).focused, Some(ids[0]));
        let forward = ran(&front, Verb::PageNext, &json!({}));
        assert_eq!(window(&forward, 1).focused, Some(ids[1]));
        let into = ran(&state, Verb::PageInto, &json!({"topic": "errors"}));
        assert_eq!(into.pages[&ids[2]].trail, ["errors"]);
        let out = ran(&into, Verb::PageBack, &json!({}));
        assert_eq!(out.pages[&ids[2]].trail.len(), 0);
        assert_eq!(
            ran(&out, Verb::PageBack, &json!({})),
            out,
            "nothing to go back from"
        );
        let scrolled = ran(
            &state,
            Verb::PageScroll,
            &json!({"page": ids[0], "to": 480}),
        );
        assert_eq!(scrolled.pages[&ids[0]].scroll, 480);
    }

    /// Esc in the drawer goes back one sub-page, or closes the drawer's page
    /// in focus; the drawer collapses with something in it and expands as it
    /// was; and its share is set.
    #[test]
    fn the_drawer_goes_back_closes_collapses_and_expands_as_section_twelve_eleven_says() {
        let empty = ran(&State::default(), Verb::DrawerCollapse, &json!({}));
        assert!(!empty.drawer.collapsed, "nothing to collapse");
        let open = ran(
            &State::default(),
            Verb::PageOpen,
            &json!({"kind": "help", "in": "drawer"}),
        );
        let help = page_of(&open, Kind::Help);
        let deeper = ran(
            &open,
            Verb::PageInto,
            &json!({"in": "drawer", "topic": "cli"}),
        );
        let back = ran(&deeper, Verb::DrawerBack, &json!({}));
        assert_eq!(
            (back.drawer.pages.as_slice(), back.pages[&help].trail.len()),
            (&[help][..], 0)
        );
        let closed = ran(&back, Verb::DrawerBack, &json!({}));
        assert!(closed.drawer.pages.is_empty() && closed.pages.is_empty());
        assert_eq!(ran(&closed, Verb::DrawerBack, &json!({})), closed);
        let collapsed = ran(&open, Verb::DrawerCollapse, &json!({}));
        assert!(collapsed.drawer.collapsed && collapsed.drawer.pages == [help]);
        let expanded = ran(&collapsed, Verb::DrawerExpand, &json!({}));
        assert_eq!(expanded, open);
        let focused = ran(&collapsed, Verb::PageFocus, &json!({"page": help}));
        assert!(
            !focused.drawer.collapsed,
            "focusing a page in it expands it"
        );
        let shared = ran(&open, Verb::DrawerResize, &json!({"share": 49}));
        assert_eq!(shared.drawer.share, 49);
    }

    /// A keyboard on a drawer that comes to show nothing open goes back to
    /// its window's prompt line, where the leader menu's came from, so no
    /// seat's keyboard is left on a part a window does not draw.
    ///
    /// **Every way the drawer shuts, the keyboard on it each time**: Esc on
    /// the leader menu, its one page; the menu closed by `page.close`; closed
    /// by the page an entry of it opens in the workspace area; the drawer
    /// collapsed; and the command list closed, by `prompt.clear` and by
    /// typing past the `;`, with the keyboard on its list — and a keyboard
    /// put on a drawer that shows nothing goes to the prompt line at once. A
    /// drawer that still shows a page keeps the keyboard; a seat whose
    /// keyboard is elsewhere stays there; and each seat goes to the prompt
    /// line of its own window.
    #[test]
    fn a_keyboard_on_a_drawer_that_shuts_goes_back_to_the_prompt_line() {
        let on = |state: &State, seat: u64| state.seats.get(&seat).and_then(|held| held.focus);
        let prompt = |window: u64| {
            Some(Focus {
                window,
                region: Region::Prompt,
            })
        };
        let shown = ran(&State::default(), Verb::LeaderTap, &json!({}));
        let menu = ran(&shown, Verb::LeaderTap, &json!({}));
        assert_eq!(on(&menu, 1).map(|focus| focus.region), Some(Region::Drawer));
        let leader = page_of(&menu, Kind::Leader);
        let listing = ran(
            &ran(&shown, Verb::PromptSet, &json!({"text": ";"})),
            Verb::ShellFocus,
            &json!({"region": "drawer"}),
        );
        assert_eq!(
            on(&listing, 1).map(|focus| focus.region),
            Some(Region::Drawer),
            "the command list holds a keyboard"
        );
        for (how, shut) in [
            ("Esc", ran(&menu, Verb::DrawerBack, &json!({}))),
            (
                "page.close",
                ran(&menu, Verb::PageClose, &json!({"page": leader})),
            ),
            ("an entry", ran(&menu, Verb::PageSettings, &json!({}))),
            ("collapse", ran(&menu, Verb::DrawerCollapse, &json!({}))),
            ("prompt.clear", ran(&listing, Verb::PromptClear, &json!({}))),
            (
                "typing",
                ran(&listing, Verb::PromptSet, &json!({"text": "x"})),
            ),
            (
                "put there",
                ran(&shown, Verb::ShellFocus, &json!({"region": "drawer"})),
            ),
        ] {
            assert_eq!(on(&shut, 1), prompt(1), "{how}: {shut:?}");
        }
        let beside = ran(
            &ran(
                &shown,
                Verb::PageOpen,
                &json!({"kind": "help", "in": "drawer"}),
            ),
            Verb::LeaderTap,
            &json!({}),
        );
        let kept = ran(&beside, Verb::DrawerBack, &json!({}));
        assert_eq!(kept.drawer.pages, [page_of(&kept, Kind::Help)]);
        assert_eq!(
            on(&kept, 1).map(|focus| focus.region),
            Some(Region::Drawer),
            "a drawer showing a page keeps the keyboard"
        );
        let two = ran(&menu, Verb::WindowOpen, &json!({}));
        let second = two.windows[1].id;
        let seated = ran(
            &ran(
                &two,
                Verb::ShellFocus,
                &json!({"seat": 2, "window": second, "region": "drawer"}),
            ),
            Verb::ShellFocus,
            &json!({"seat": 3, "region": "sidebar"}),
        );
        let shut = ran(&seated, Verb::DrawerBack, &json!({}));
        assert_eq!(
            (on(&shut, 1), on(&shut, 2), on(&shut, 3)),
            (
                prompt(1),
                prompt(second),
                Some(Focus {
                    window: 1,
                    region: Region::Sidebar
                })
            )
        );
    }

    /// A window opens shown, and closes with the pages only it showed — the
    /// last one hidden instead — and every seat in it loses its focus.
    #[test]
    fn a_window_opens_shown_and_closes_with_what_only_it_showed_and_the_last_hides() {
        let two = ran(&State::default(), Verb::WindowOpen, &json!({}));
        let id = two.windows[1].id;
        assert!(two.windows[1].shown);
        let mut state = ran(&two, Verb::PageOpen, &json!({"kind": "help", "window": id}));
        state = ran(
            &state,
            Verb::PageOpen,
            &json!({"kind": "home", "window": id}),
        );
        state = ran(
            &state,
            Verb::PageOpen,
            &json!({"kind": "home", "window": 1}),
        );
        state = ran(
            &state,
            Verb::ShellFocus,
            &json!({"seat": 2, "window": id, "region": "prompt"}),
        );
        let closed = ran(&state, Verb::WindowClose, &json!({"window": id}));
        assert_eq!(closed.windows.len(), 1);
        assert_eq!(
            closed.pages.len(),
            1,
            "help went with its window, home stays"
        );
        assert_eq!(closed.seats[&2].focus, None);
        let last = ran(&closed, Verb::WindowClose, &json!({}));
        assert_eq!(last.windows.len(), 1, "the last window is kept, hidden");
        assert!(!last.windows[0].shown);
        assert_eq!(
            said(&refused(&closed, Verb::WindowShow, &json!({"window": id}))),
            (RefusalKind::NotFound, ErrorCode::KernelShellUnknown)
        );
    }

    /// The sidebar is shown, then focused, then hidden by Leader Space, and
    /// hiding it gives the keyboard back to the workspace area.
    #[test]
    fn the_sidebar_is_shown_focused_and_hidden_in_turn() {
        let shown = ran(&State::default(), Verb::SidebarCycle, &json!({}));
        assert!(shown.sidebar && shown.seats.is_empty());
        let focused = ran(&shown, Verb::SidebarCycle, &json!({}));
        assert_eq!(
            focused.seats[&1].focus.map(|focus| focus.region),
            Some(Region::Sidebar)
        );
        let hidden = ran(&focused, Verb::SidebarCycle, &json!({}));
        assert!(!hidden.sidebar);
        assert_eq!(
            hidden.seats[&1].focus.map(|focus| focus.region),
            Some(Region::Workspace)
        );
        let by_hand = ran(&focused, Verb::SidebarHide, &json!({}));
        assert_eq!(by_hand, hidden);
        assert!(ran(&hidden, Verb::SidebarShow, &json!({})).sidebar);
    }

    /// A `;` or a `:` the draft begins with opens the command list in the
    /// drawer and anything else closes it; a command's parameters are asked
    /// for by name; and clearing empties the prompt.
    #[test]
    fn the_prompt_opens_the_command_list_and_asks_for_a_commands_parameters() {
        for opening in [";win", ":"] {
            let listing = ran(
                &State::default(),
                Verb::PromptSet,
                &json!({"text": opening}),
            );
            assert_eq!(listing.asking, Some(Asking::Commands));
            assert_eq!(
                listing.drawer.focused,
                Some(page_of(&listing, Kind::Commands))
            );
            let typed = ran(&listing, Verb::PromptSet, &json!({"text": "win"}));
            assert_eq!(typed.asking, None);
            assert!(typed.drawer.pages.is_empty(), "the list closes");
        }
        let reserved = ran(&State::default(), Verb::PromptSet, &json!({"text": "/x"}));
        assert_eq!((reserved.asking.as_ref(), reserved.pages.len()), (None, 0));
        let asked = ran(
            &State::default(),
            Verb::PromptAsk,
            &json!({"command": "settings.mcp.set", "parameter": "on"}),
        );
        assert_eq!(
            asked.asking,
            Some(Asking::Parameters {
                command: "settings.mcp.set".to_owned(),
                parameter: Some("on".to_owned())
            })
        );
        let typed = ran(&asked, Verb::PromptSet, &json!({"text": ";true"}));
        assert_eq!(typed.asking, asked.asking, "a parameter's value is text");
        assert_eq!(
            said(&refused(
                &State::default(),
                Verb::PromptAsk,
                &json!({"command": "nothing.here"})
            )),
            (RefusalKind::NotFound, ErrorCode::KernelUnknownCommand)
        );
        assert_eq!(
            said(&refused(
                &State::default(),
                Verb::PromptAsk,
                &json!({"command": "settings.mcp.set", "parameter": "off"})
            )),
            (RefusalKind::Invalid, ErrorCode::ValidationArgumentsInvalid)
        );
        let listing = ran(&State::default(), Verb::PromptSet, &json!({"text": ";a"}));
        let cleared = ran(&listing, Verb::PromptClear, &json!({}));
        assert_eq!(
            (cleared.draft.as_str(), cleared.asking.as_ref()),
            ("", None)
        );
        assert_eq!(cleared.drawer.pages.len(), 0);
    }

    /// The info line says what it is told, in text a reader sees whole, and
    /// expands and collapses; the footer shows and hides.
    #[test]
    fn the_info_line_says_what_a_reader_sees_and_the_footer_shows_and_hides() {
        let said_it = ran(
            &State::default(),
            Verb::InfoSay,
            &json!({"text": "Nothing handles that\nhere"}),
        );
        assert_eq!(
            said_it.message.as_deref(),
            Some("Nothing handles that\nhere")
        );
        assert_eq!(ran(&said_it, Verb::InfoClear, &json!({})).message, None);
        for unseen in ["a\u{202E}b", "a\u{200B}", "a\u{FE0F}"] {
            assert_eq!(
                said(&refused(
                    &State::default(),
                    Verb::InfoSay,
                    &json!({"text": unseen})
                )),
                (RefusalKind::Invalid, ErrorCode::ValidationArgumentsInvalid),
                "{unseen:?}"
            );
        }
        assert!(ran(&State::default(), Verb::InfoExpand, &json!({})).expanded);
        assert!(
            !ran(
                &ran(&State::default(), Verb::InfoExpand, &json!({})),
                Verb::InfoCollapse,
                &json!({})
            )
            .expanded
        );
        assert!(!ran(&State::default(), Verb::FooterHide, &json!({})).footer);
        assert!(
            ran(
                &ran(&State::default(), Verb::FooterHide, &json!({})),
                Verb::FooterShow,
                &json!({})
            )
            .footer
        );
    }

    /// A footer the setting hides is a reveal: shown by a tap, kept while a
    /// keyboard is on the footer or the drawer, and hidden again once none is.
    ///
    /// **§12.3's rule for focus, each clause by a state it alone decides**:
    /// choosing `hidden` with no keyboard on the footer hides it now, and with
    /// one on the prompt line keeps it; a tap shows it and puts the keyboard on
    /// the prompt line, where it stays shown; the keyboard on the drawer or on
    /// another line of the footer keeps it; the keyboard on the workspace area
    /// or the sidebar hides it — unless another seat's keyboard is still on
    /// the footer; a keyboard coming back shows nothing a tap did not; and
    /// chosen `always`, it is shown now and no keyboard leaving hides it.
    #[test]
    fn a_hidden_footer_shown_by_a_tap_hides_again_once_no_keyboard_holds_it() {
        let hidden = json!({"visibility": "hidden"});
        let away = json!({"region": "workspace"});
        let chosen = ran(&State::default(), Verb::SettingsFooterSet, &hidden);
        assert!(
            !chosen.footer,
            "the setting hidden leaves the footer shown now"
        );
        let on_the_prompt = ran(
            &State::default(),
            Verb::ShellFocus,
            &json!({"region": "prompt"}),
        );
        assert!(
            ran(&on_the_prompt, Verb::SettingsFooterSet, &hidden).footer,
            "the setting hidden hides a footer a keyboard is on"
        );
        let tapped = ran(&chosen, Verb::LeaderTap, &json!({}));
        assert!(tapped.footer, "a tap leaves a hidden footer hidden");
        for region in ["prompt", "info", "modeline", "drawer"] {
            assert!(
                ran(&tapped, Verb::ShellFocus, &json!({"region": region})).footer,
                "the keyboard on the {region} hides the footer"
            );
        }
        for region in ["workspace", "sidebar"] {
            let left = ran(&tapped, Verb::ShellFocus, &json!({"region": region}));
            assert!(
                !left.footer,
                "the keyboard on the {region} keeps the footer shown"
            );
            assert!(
                !ran(&left, Verb::ShellFocus, &json!({"region": "prompt"})).footer,
                "a keyboard coming back shows the footer"
            );
        }
        let two = ran(
            &tapped,
            Verb::ShellFocus,
            &json!({"seat": 2, "region": "prompt"}),
        );
        let one_left = ran(&two, Verb::ShellFocus, &away);
        assert!(one_left.footer, "another seat on the prompt line let it go");
        assert!(
            !ran(
                &one_left,
                Verb::ShellFocus,
                &json!({"seat": 2, "region": "workspace"})
            )
            .footer,
            "the last seat to leave kept it shown"
        );
        let always = ran(
            &ran(&tapped, Verb::ShellFocus, &away),
            Verb::SettingsFooterSet,
            &json!({"visibility": "always"}),
        );
        assert!(always.footer, "chosen always, the footer is not shown now");
        assert!(
            ran(
                &ran(&always, Verb::ShellFocus, &json!({"region": "prompt"})),
                Verb::ShellFocus,
                &away
            )
            .footer,
            "a keyboard leaving hid a footer always shown"
        );
    }

    /// A hidden footer is let go however the last keyboard leaves it.
    ///
    /// **A command is judged as the window is**: one that moves the keyboard
    /// to the sidebar, or closes the window it was in, lets the footer go as a
    /// keyboard the window moves does —
    /// unless another seat's keyboard is still on the footer — and
    /// `footer.show`, beginning a reveal no keyboard holds, is kept by a
    /// command that moves no keyboard and ended by one that moves a keyboard
    /// anywhere but the footer.
    #[test]
    fn a_hidden_footer_is_let_go_however_the_last_keyboard_leaves_it() {
        let chosen = ran(
            &State::default(),
            Verb::SettingsFooterSet,
            &json!({"visibility": "hidden"}),
        );
        let tapped = ran(&chosen, Verb::LeaderTap, &json!({}));
        let sidebar = ran(&tapped, Verb::SidebarCycle, &json!({}));
        assert!(sidebar.footer, "the sidebar shown moved no keyboard");
        let on_the_sidebar = ran(&sidebar, Verb::SidebarCycle, &json!({}));
        assert_eq!(
            on_the_sidebar.seats[&1].focus.map(|focus| focus.region),
            Some(Region::Sidebar)
        );
        assert!(
            !on_the_sidebar.footer,
            "the keyboard moved to the sidebar by a command kept the footer shown"
        );
        let in_the_second = ran(
            &ran(&chosen, Verb::WindowOpen, &json!({})),
            Verb::ShellFocus,
            &json!({"window": 2, "region": "workspace"}),
        );
        let tapped_there = ran(&in_the_second, Verb::LeaderTap, &json!({}));
        assert_eq!(
            tapped_there.seats[&1].focus,
            Some(Focus {
                window: 2,
                region: Region::Prompt
            })
        );
        assert!(tapped_there.footer);
        let closed = ran(&tapped_there, Verb::WindowClose, &json!({"window": 2}));
        assert!(closed.seats[&1].focus.is_none());
        assert!(
            !closed.footer,
            "the window the keyboard was in closed, and the footer stayed shown"
        );
        let other_seat = ran(
            &tapped_there,
            Verb::ShellFocus,
            &json!({"seat": 2, "window": 1, "region": "prompt"}),
        );
        assert!(
            ran(&other_seat, Verb::WindowClose, &json!({"window": 2})).footer,
            "another seat on the prompt line let it go"
        );
        let asked = ran(&chosen, Verb::FooterShow, &json!({}));
        assert!(asked.footer, "`footer.show` began no reveal");
        assert!(
            ran(&asked, Verb::InfoSay, &json!({"text": "kept"})).footer,
            "a command that moved no keyboard ended a reveal"
        );
        assert!(
            ran(
                &asked,
                Verb::ShellHover,
                &json!({"seat": 2, "by": "pointer", "target": "modeline.glyph"})
            )
            .footer,
            "a seat come with its pointer alone moved a keyboard"
        );
        assert!(
            !ran(&asked, Verb::ShellFocus, &json!({"region": "sidebar"})).footer,
            "a keyboard moved with none on the footer kept a reveal"
        );
    }

    /// Each seat keeps its own focus, hover, selection and drag, and which of
    /// its pointer and its keyboard moved last.
    #[test]
    fn each_seat_keeps_its_own_focus_hover_selection_and_drag() {
        let mut state = ran(
            &State::default(),
            Verb::ShellHover,
            &json!({"by": "pointer", "target": "modeline.glyph"}),
        );
        state = ran(
            &state,
            Verb::ShellHover,
            &json!({"seat": 2, "by": "keyboard", "target": "list/3"}),
        );
        state = ran(
            &state,
            Verb::ShellDrag,
            &json!({"seat": 2, "target": "page:7"}),
        );
        state = ran(
            &state,
            Verb::ShellFocus,
            &json!({"seat": 2, "region": "info"}),
        );
        assert_eq!(
            (state.seats[&1].hover.as_deref(), state.seats[&1].latest),
            (Some("modeline.glyph"), By::Pointer)
        );
        assert_eq!(
            (
                state.seats[&2].selection.as_deref(),
                state.seats[&2].latest,
                state.seats[&2].drag.as_deref()
            ),
            (Some("list/3"), By::Keyboard, Some("page:7"))
        );
        assert_eq!(state.seats[&1].focus, None);
        let stopped = ran(&state, Verb::ShellDrag, &json!({"seat": 2}));
        assert_eq!(stopped.seats[&2].drag, None);
        let cleared = ran(&state, Verb::ShellHover, &json!({"by": "pointer"}));
        assert_eq!(cleared.seats[&1].hover, None);
        assert_eq!(
            said(&refused(
                &State::default(),
                Verb::ShellFocus,
                &json!({"window": 4, "region": "prompt"})
            )),
            (RefusalKind::NotFound, ErrorCode::KernelShellUnknown)
        );
        assert_eq!(
            said(&refused(
                &State::default(),
                Verb::ShellHover,
                &json!({"by": "pointer", "target": "a b"})
            )),
            (RefusalKind::Invalid, ErrorCode::ValidationArgumentsInvalid)
        );
    }

    /// A state of every part, as commands leave it, every field of it holding
    /// what the first run's does not.
    ///
    /// Two windows, the second hidden; a page in a window, scrolled, and two
    /// in the drawer, one gone a sub-page deeper and the other the leader
    /// menu, opened by a second tap and flipped by a third; the drawer
    /// collapsed, sent toward itself and resized; the sidebar shown and the
    /// footer hidden; a message expanded; the prompt asking for a parameter
    /// with a draft no ASCII writes; seat 1's keyboard on the drawer the menu
    /// opened in, which the prompt's asking keeps open though collapsed; and
    /// seat 1's pointer and seat 3's keyboard each on something, seat 3
    /// dragging.
    fn varied() -> State {
        let mut state = State::default();
        for (verb, args) in [
            (Verb::LeaderTap, json!({})),
            (Verb::WindowOpen, json!({})),
            (Verb::WindowResize, json!({"width": 800, "height": 600})),
            (Verb::WindowMove, json!({"x": -4, "y": 10})),
            (Verb::PageOpen, json!({"kind": "settings", "topic": "mcp"})),
            (Verb::PageOpen, json!({"kind": "help", "in": "drawer"})),
            (Verb::PageInto, json!({"in": "drawer", "topic": "cli"})),
            (Verb::PageScroll, json!({"to": 12})),
            (Verb::DrawerToward, json!({"region": "drawer"})),
            (Verb::DrawerResize, json!({"share": 40})),
            (Verb::SidebarShow, json!({})),
            (Verb::InfoSay, json!({"text": "said"})),
            (Verb::InfoExpand, json!({})),
            (
                Verb::PromptAsk,
                json!({"command": "settings.mcp.set", "parameter": "on"}),
            ),
            (
                Verb::PromptSet,
                json!({"text": "tr\u{00FC}e \u{1F600}\u{FE0F}"}),
            ),
            (
                Verb::ShellHover,
                json!({"by": "pointer", "target": "modeline.glyph"}),
            ),
            (
                Verb::ShellHover,
                json!({"seat": 3, "by": "keyboard", "target": "a"}),
            ),
            (Verb::ShellDrag, json!({"seat": 3, "target": "b"})),
            (Verb::WindowHide, json!({"window": 2})),
            (Verb::LeaderTap, json!({})),
            (Verb::LeaderTap, json!({})),
            (Verb::DrawerCollapse, json!({})),
            (Verb::FooterHide, json!({})),
            (Verb::SettingsFooterSet, json!({"visibility": "hidden"})),
            (Verb::SettingsLanguageSet, json!({"language": "pt-BR"})),
            (Verb::SettingsAppearanceSet, json!({"mode": "dark"})),
        ] {
            state = ran(&state, verb, &args);
        }
        state
    }

    /// What is kept is read back as itself.
    ///
    /// **Every field, at a value the first run's state does not hold**, so a
    /// reading that took any one of them for its default is refused here: the
    /// fields are named whole, with no `..`, and a field added to the state
    /// does not compile here until [`varied`] gives it a value of its own.
    #[test]
    fn what_is_kept_is_read_back_as_itself() {
        for state in [State::default(), varied()] {
            let text = snapshot(&state);
            assert_eq!(unkept(&text), Ok(state.clone()), "{text}");
            assert!(text.starts_with(&format!("{{\"shape\":{SHAPE},")), "{text}");
        }
        let first = State::default();
        let State {
            windows,
            pages,
            drawer,
            sidebar,
            footer,
            message,
            expanded,
            draft,
            asking,
            seats,
            settings,
            next,
        } = varied();
        let super::Chosen {
            language,
            visibility,
            mode,
        } = settings;
        assert!(
            language != first.settings.language
                && visibility != first.settings.visibility
                && mode != first.settings.mode
        );
        assert!(
            windows != first.windows
                && pages != first.pages
                && sidebar != first.sidebar
                && footer != first.footer
                && message != first.message
                && expanded != first.expanded
                && draft != first.draft
                && asking != first.asking
                && seats != first.seats
                && next != first.next
        );
        let super::Drawer {
            pages: drawn,
            focused,
            collapsed,
            toward,
            flipped,
            share,
        } = drawer;
        assert!(
            drawn != first.drawer.pages
                && focused != first.drawer.focused
                && collapsed != first.drawer.collapsed
                && toward != first.drawer.toward
                && flipped != first.drawer.flipped
                && share != first.drawer.share,
            "the drawer"
        );
        let shown: Vec<bool> = windows.iter().map(|window| window.shown).collect();
        assert_eq!(shown, [true, false], "a window shown and one hidden");
        assert!(windows.iter().any(|window| window.size.is_some()
            && window.place.is_some()
            && window.focused.is_some()));
        assert!(pages.values().any(|page| page.topic.is_some()));
        assert!(pages.values().any(|page| !page.trail.is_empty()));
        assert!(pages.values().any(|page| page.scroll > 0));
        let seat = |id: u64| seats.get(&id).cloned().unwrap_or_default();
        let super::Seat {
            focus,
            hover,
            selection: _,
            latest: _,
            drag: _,
        } = seat(1);
        let super::Seat {
            focus: _,
            hover: _,
            selection,
            latest,
            drag,
        } = seat(3);
        let none = super::Seat::default();
        assert!(
            focus.is_some()
                && hover.is_some()
                && selection.is_some()
                && latest != none.latest
                && drag.is_some(),
            "the seats"
        );
    }

    /// One edit of a state that breaks a rule a command never breaks.
    type Edit = Box<dyn Fn(&mut State)>;

    /// A page numbered `id`, of `kind` and on `topic`, at the top and in no
    /// place yet.
    fn a_page(id: u64, kind: Kind, topic: Option<&str>) -> super::Page {
        super::Page {
            id,
            kind,
            topic: topic.map(str::to_owned),
            trail: Vec::new(),
            scroll: 0,
        }
    }

    /// Gives the page numbered `from` the number `to`, in every place it is
    /// shown and in focus.
    fn renumbered(state: &mut State, from: u64, to: u64) {
        let mut page = state.pages.remove(&from).expect("the page is held");
        page.id = to;
        state.pages.insert(to, page);
        let swap = |id: &mut u64| {
            if *id == from {
                *id = to;
            }
        };
        for window in &mut state.windows {
            window.pages.iter_mut().for_each(swap);
            window.focused.iter_mut().for_each(swap);
        }
        state.drawer.pages.iter_mut().for_each(swap);
        state.drawer.focused.iter_mut().for_each(swap);
    }

    /// `state`'s window numbered `id`, to edit.
    fn window_mut(state: &mut State, id: u64) -> &mut super::Window {
        state.window_mut(id).expect("the window is held")
    }

    /// `state`'s page of `kind`, to edit.
    fn page_mut(state: &mut State, kind: Kind) -> &mut super::Page {
        let id = page_of(state, kind);
        state.page_mut(id).expect("the page is held")
    }

    /// A bound of the catalog's, as a count.
    fn bound(limit: u32) -> usize {
        usize::try_from(limit).expect("small")
    }

    /// Every edit of [`varied`] that breaks one rule a command never breaks,
    /// beside the rule [`State::rules`] names it by — and that rule alone.
    ///
    /// **Each clause of the rules, broken where it is the only one broken**,
    /// so taking any clause out leaves a state this table holds refused by
    /// nothing: no window, or windows past their bound or two of one number,
    /// numbered zero or never given; a page twice in a stack, one no state
    /// holds, a focus out of the stack or on nothing over it, a list of the
    /// drawer's in a window, a size or a corner out of range; pages past their
    /// bound, one numbered zero, never given or a window's, one shown nowhere,
    /// a topic on a kind that takes none and none on one that needs it, a
    /// topic or a sub-page that is no word, too many sub-pages, two of one
    /// kind and topic; the drawer past its bound, holding a page twice or one
    /// no state holds, its focus out of it or on nothing over it, collapsed
    /// with nothing in it, its share at half or at nothing; a draft or a
    /// message past its bytes, a message empty or holding what no reader
    /// sees, a command that is no name and a parameter that is no word; and a
    /// seat past the bound or numbered zero, its focus on no window, its
    /// keyboard on a drawer that shows nothing open, and what it hovers,
    /// selects or drags no word.
    #[expect(
        clippy::too_many_lines,
        reason = "one table, a row for each clause of the rules: split, a clause could lose \
                  its row unseen"
    )]
    fn rule_breakers() -> Vec<(String, Edit)> {
        let helping = |state: &State| page_of(state, Kind::Help);
        let settings = |state: &State| page_of(state, Kind::Settings);
        let leader = |state: &State| page_of(state, Kind::Leader);
        let long = "a".repeat(usize::try_from(KERNEL.shell_text_bytes).expect("small") + 1);
        let long_too = long.clone();
        let table: Vec<(&str, Edit)> = vec![
            (
                "the interface holds a window",
                Box::new(|state: &mut State| {
                    let shown: Vec<u64> = state
                        .windows
                        .iter()
                        .flat_map(|window| window.pages.clone())
                        .collect();
                    state.windows.clear();
                    state.drawer.pages.extend(shown);
                    for seat in state.seats.values_mut() {
                        seat.focus = None;
                    }
                }),
            ),
            (
                "the windows are at most their bound",
                Box::new(|state: &mut State| {
                    while state.windows.len() <= bound(KERNEL.shell_windows_max) {
                        let id = state.minted().expect("minted");
                        state.windows.push(super::Window::new(id, false));
                    }
                }),
            ),
            (
                "no two windows share an identifier",
                Box::new(|state: &mut State| {
                    let id = state.windows[0].id;
                    state.windows.push(super::Window::new(id, false));
                }),
            ),
            (
                "a window's identifier is one at least",
                Box::new(|state: &mut State| window_mut(state, 2).id = 0),
            ),
            (
                "a window's identifier was given",
                Box::new(|state: &mut State| window_mut(state, 2).id = state.next),
            ),
            (
                "a window's stack holds a page once",
                Box::new(move |state: &mut State| {
                    let page = settings(state);
                    window_mut(state, 1).pages.push(page);
                }),
            ),
            (
                "a window's stack holds pages the state holds",
                Box::new(|state: &mut State| {
                    let held = window_mut(state, 2);
                    held.pages = vec![99];
                    held.focused = Some(99);
                }),
            ),
            (
                "a window's focus is on its stack, and on nothing only when it is empty",
                Box::new(|state: &mut State| window_mut(state, 1).focused = None),
            ),
            (
                "a window's focus is on its stack, and on nothing only when it is empty",
                Box::new(move |state: &mut State| {
                    let page = settings(state);
                    window_mut(state, 2).focused = Some(page);
                }),
            ),
            (
                "a window shows no list of the drawer's",
                Box::new(move |state: &mut State| {
                    let page = leader(state);
                    let held = window_mut(state, 2);
                    held.pages = vec![page];
                    held.focused = Some(page);
                }),
            ),
            (
                "a window's size is within the smallest and the largest",
                Box::new(|state: &mut State| window_mut(state, 1).size = Some((319, 600))),
            ),
            (
                "a window's size is within the smallest and the largest",
                Box::new(|state: &mut State| window_mut(state, 1).size = Some((800, 239))),
            ),
            (
                "a window's size is within the smallest and the largest",
                Box::new(|state: &mut State| {
                    window_mut(state, 1).size = Some((WINDOW_MAX + 1, 600));
                }),
            ),
            (
                "a window's size is within the smallest and the largest",
                Box::new(|state: &mut State| {
                    window_mut(state, 1).size = Some((800, WINDOW_MAX + 1));
                }),
            ),
            (
                "a window's corner is within an X position's sixteen bits",
                Box::new(|state: &mut State| {
                    window_mut(state, 1).place = Some((PLACE_MAX + 1, 0));
                }),
            ),
            (
                "a window's corner is within an X position's sixteen bits",
                Box::new(|state: &mut State| {
                    window_mut(state, 1).place = Some((0, PLACE_MIN - 1));
                }),
            ),
            (
                "the pages are at most their bound",
                Box::new(|state: &mut State| {
                    let mut topic = 0;
                    while state.pages.len() <= bound(KERNEL.shell_pages_max) {
                        let id = state.minted().expect("minted");
                        let named = format!("t{topic}");
                        state.pages.insert(id, a_page(id, Kind::Help, Some(&named)));
                        let held = window_mut(state, 2);
                        held.pages.push(id);
                        held.focused = Some(id);
                        topic += 1;
                    }
                }),
            ),
            (
                "a page's identifier is one at least",
                Box::new(move |state: &mut State| {
                    let page = helping(state);
                    renumbered(state, page, 0);
                }),
            ),
            (
                "a page's identifier was given",
                Box::new(|state: &mut State| {
                    state.next = state.pages.keys().copied().max().expect("a page");
                }),
            ),
            (
                "a page's identifier is no window's",
                Box::new(move |state: &mut State| {
                    let page = settings(state);
                    renumbered(state, page, 2);
                }),
            ),
            (
                "a page is shown somewhere",
                Box::new(|state: &mut State| {
                    let id = state.minted().expect("minted");
                    state.pages.insert(id, a_page(id, Kind::Dev, None));
                }),
            ),
            (
                "a page has a topic where its kind takes one, and only there",
                Box::new(|state: &mut State| {
                    page_mut(state, Kind::Leader).topic = Some("x".to_owned());
                }),
            ),
            (
                "a page has a topic where its kind takes one, and only there",
                Box::new(|state: &mut State| page_mut(state, Kind::Help).kind = Kind::Surface),
            ),
            (
                "a page's topic is a word",
                Box::new(|state: &mut State| {
                    page_mut(state, Kind::Settings).topic = Some("a b".to_owned());
                }),
            ),
            (
                "a page's sub-pages are at most their bound",
                Box::new(|state: &mut State| {
                    page_mut(state, Kind::Help).trail =
                        vec!["x".to_owned(); bound(KERNEL.shell_trail_max) + 1];
                }),
            ),
            (
                "a page's sub-pages are words",
                Box::new(|state: &mut State| {
                    page_mut(state, Kind::Help).trail = vec!["a b".to_owned()];
                }),
            ),
            (
                "a page is the one of its kind and topic",
                Box::new(|state: &mut State| {
                    let id = state.minted().expect("minted");
                    state
                        .pages
                        .insert(id, a_page(id, Kind::Settings, Some("mcp")));
                    let held = window_mut(state, 2);
                    held.pages.push(id);
                    held.focused = Some(id);
                }),
            ),
            (
                "the drawer's pages are at most their bound",
                Box::new(|state: &mut State| {
                    let mut topic = 0;
                    while state.drawer.pages.len() <= bound(KERNEL.shell_drawer_pages_max) {
                        let id = state.minted().expect("minted");
                        let named = format!("t{topic}");
                        state.pages.insert(id, a_page(id, Kind::Help, Some(&named)));
                        state.drawer.pages.push(id);
                        topic += 1;
                    }
                }),
            ),
            (
                "the drawer holds a page once",
                Box::new(move |state: &mut State| {
                    let page = helping(state);
                    state.drawer.pages.push(page);
                }),
            ),
            (
                "the drawer holds pages the state holds",
                Box::new(|state: &mut State| state.drawer.pages.push(99)),
            ),
            (
                "the drawer's focus is on a page of it, and on nothing only when it is empty",
                Box::new(|state: &mut State| state.drawer.focused = None),
            ),
            (
                "the drawer's focus is on a page of it, and on nothing only when it is empty",
                Box::new(move |state: &mut State| state.drawer.focused = Some(settings(state))),
            ),
            (
                "the drawer is collapsed only when it holds a page",
                Box::new(|state: &mut State| {
                    for page in state.drawer.pages.clone() {
                        state.pages.remove(&page);
                    }
                    state.drawer.pages.clear();
                    state.drawer.focused = None;
                }),
            ),
            (
                "the drawer's share is within its bound",
                Box::new(|state: &mut State| {
                    state.drawer.share = KERNEL.shell_drawer_share_max + 1;
                }),
            ),
            (
                "the drawer's share is within its bound",
                Box::new(|state: &mut State| state.drawer.share = 0),
            ),
            (
                "the draft is at most its bound in bytes",
                Box::new(move |state: &mut State| state.draft.clone_from(&long)),
            ),
            (
                "a message says something",
                Box::new(|state: &mut State| state.message = Some(String::new())),
            ),
            (
                "a message is at most its bound in bytes",
                Box::new(move |state: &mut State| state.message = Some(long_too.clone())),
            ),
            (
                "a message is all a reader sees",
                Box::new(|state: &mut State| state.message = Some("a\u{202E}".to_owned())),
            ),
            (
                "the prompt asks of a command by its name",
                Box::new(|state: &mut State| {
                    state.asking = Some(Asking::Parameters {
                        command: "Not A Name".to_owned(),
                        parameter: None,
                    });
                }),
            ),
            (
                "the parameter the prompt asks for is a word",
                Box::new(|state: &mut State| {
                    state.asking = Some(Asking::Parameters {
                        command: "settings.mcp.set".to_owned(),
                        parameter: Some("a b".to_owned()),
                    });
                }),
            ),
            (
                "a seat is one the bound allows",
                Box::new(|state: &mut State| {
                    let past = u64::from(KERNEL.shell_seats_max) + 1;
                    state.seats.insert(past, super::Seat::default());
                }),
            ),
            (
                "a seat is one the bound allows",
                Box::new(|state: &mut State| {
                    state.seats.insert(0, super::Seat::default());
                }),
            ),
            (
                "a seat's focus is on a window the state holds",
                Box::new(|state: &mut State| {
                    state.seat_mut(1).focus = Some(Focus {
                        window: 99,
                        region: Region::Prompt,
                    });
                }),
            ),
            (
                "a seat's keyboard is on the drawer only while it shows something open",
                // The drawer is collapsed, so with the prompt asking for
                // nothing it shows nothing open, and seat 1's keyboard is on
                // it.
                Box::new(|state: &mut State| state.asking = None),
            ),
            (
                "what a seat hovers, selects or drags is a word",
                Box::new(|state: &mut State| state.seat_mut(1).hover = Some("a b".to_owned())),
            ),
            (
                "what a seat hovers, selects or drags is a word",
                Box::new(|state: &mut State| {
                    state.seat_mut(3).selection = Some("a b".to_owned());
                }),
            ),
            (
                "what a seat hovers, selects or drags is a word",
                Box::new(|state: &mut State| state.seat_mut(3).drag = Some("a b".to_owned())),
            ),
        ];
        table
            .into_iter()
            .map(|(rule, edit)| (rule.to_owned(), edit))
            .collect()
    }

    /// A state an earlier build kept is set aside as that build's.
    ///
    /// **What a saved state holds is its shape's**: the build before the
    /// interface's settings kept the state without them, under shape 1, and
    /// that state is named by a shape this build does not write — so it is
    /// logged as an older build's, never as one of this build's own that
    /// breaks a rule.
    #[test]
    fn a_state_kept_before_the_settings_is_set_aside_as_an_earlier_builds() {
        let mut earlier: serde_json::Value =
            serde_json::from_str(&snapshot(&varied())).expect("the snapshot is JSON");
        earlier["shell"]
            .as_object_mut()
            .expect("the state is an object")
            .remove("settings")
            .expect("the state holds its settings");
        earlier["shape"] = json!(1);
        let found = unkept(&earlier.to_string()).expect_err("an earlier build's state");
        assert_eq!(found, 1, "the shape it named");
        assert_ne!(
            i64::try_from(found).ok(),
            Some(SHAPE),
            "a state without the settings named this build's own shape"
        );
    }

    /// A kept state of another shape, or not one this module writes, is set
    /// aside and never read as one.
    ///
    /// **Every rule, from the outside, one at a time**: another shape is
    /// named; a text that is not JSON, a shape that is not a number and a
    /// field beside the state are shape zero; each state of
    /// [`rule_breakers`] breaks the one rule it names and no other, and is
    /// refused — so every rule [`State::rules`] asks is one a test breaks
    /// alone, and the table names every rule; and a page or a seat written
    /// twice is refused by the reading.
    #[test]
    fn a_kept_state_of_another_shape_or_not_one_this_module_writes_is_set_aside() {
        let kept = snapshot(&varied());
        let shell = varied().to_value().to_canonical();
        let this = format!("\"shape\":{SHAPE}");
        assert_eq!(unkept(&kept.replacen(&this, "\"shape\":0", 1)), Err(0));
        assert_eq!(unkept(&kept.replacen(&this, "\"shape\":7", 1)), Err(7));
        assert_eq!(unkept("not json"), Err(0));
        assert_eq!(unkept(r#"{"shape":"1"}"#), Err(0));
        let own = u64::try_from(SHAPE).expect("a shape is a count");
        assert_eq!(
            unkept(&format!(r#"{{"other":1,"shape":{SHAPE},"shell":{shell}}}"#)),
            Err(own)
        );
        assert_eq!(unkept(&format!(r#"{{"shape":{SHAPE}}}"#)), Err(own));
        let edits = rule_breakers();
        for (at, (rule, edit)) in edits.iter().enumerate() {
            let mut state = varied();
            edit(&mut state);
            let broken: Vec<&str> = state
                .rules()
                .into_iter()
                .filter(|(_, kept)| !kept)
                .map(|(rule, _)| rule)
                .collect();
            assert_eq!(broken, [rule.as_str()], "edit {at}: {state:?}");
            assert_eq!(unkept(&snapshot(&state)), Err(own), "edit {at}: {rule}");
        }
        let named: BTreeSet<&str> = edits.iter().map(|(rule, _)| rule.as_str()).collect();
        let state = varied();
        let rules = state.rules();
        let asked: BTreeSet<&str> = rules.iter().map(|(rule, _)| *rule).collect();
        assert_eq!(named, asked, "a rule no edit breaks alone");
        assert_eq!(rules.len(), asked.len(), "two rules are named alike");
        for written in ["pages", "seats"] {
            let mut twice = nc_canon::parse(&shell).expect("the state is JSON");
            if let Value::Object(fields) = &mut twice
                && let Some(Value::Array(items)) = fields.get_mut(written)
            {
                let first = items[0].clone();
                items.push(first);
            }
            assert_ne!(twice.to_canonical(), shell, "{written}: one written twice");
            assert_eq!(
                unkept(
                    &Value::object([("shape", Value::Integer(1)), ("shell", twice)]).to_canonical()
                ),
                Err(1),
                "{written}: one written twice"
            );
        }
    }

    /// The widest state there can be is an answer a dispatch may give.
    ///
    /// **Every count at its bound and every text at its longest**, each of
    /// its characters one canonical JSON writes as six: what `shell.read`
    /// answers then still fits `payload_bytes`.
    #[test]
    fn the_widest_state_fits_one_payload() {
        let longest = "\u{1}".repeat(usize::try_from(KERNEL.shell_text_bytes).expect("small"));
        let word = "a".repeat(WORD_CHARS);
        let mut state = State {
            draft: longest.clone(),
            message: Some(longest),
            ..State::default()
        };
        for _ in 1..KERNEL.shell_windows_max {
            let id = state.minted().expect("minted");
            state.windows.push(super::Window::new(id, true));
        }
        for _ in 0..KERNEL.shell_pages_max {
            let id = state.minted().expect("minted");
            state.pages.insert(
                id,
                super::Page {
                    id,
                    kind: Kind::Help,
                    topic: Some(word.clone()),
                    trail: vec![
                        word.clone();
                        usize::try_from(KERNEL.shell_trail_max).expect("small")
                    ],
                    scroll: u32::MAX,
                },
            );
            for window in &mut state.windows {
                window.pages.push(id);
            }
        }
        for seat in 1..=u64::from(KERNEL.shell_seats_max) {
            state.seats.insert(
                seat,
                super::Seat {
                    focus: Some(Focus {
                        window: 1,
                        region: Region::Modeline,
                    }),
                    hover: Some(word.clone()),
                    selection: Some(word.clone()),
                    latest: By::Keyboard,
                    drag: Some(word.clone()),
                },
            );
        }
        let answer = Value::object([
            ("revision", Value::Integer(i64::MAX)),
            ("shell", state.to_value()),
        ])
        .to_canonical();
        assert!(
            u64::try_from(answer.len()).expect("fits") <= KERNEL.payload_bytes,
            "{} bytes",
            answer.len()
        );
        assert!(
            answer.len() > 100_000,
            "the widest state is wide: {}",
            answer.len()
        );
    }

    /// A state held is released over the one it was computed from alone.
    ///
    /// **Never over a newer one**, which a unit inside the same tree released;
    /// and a change moves the revision, where a release that changes nothing
    /// does not.
    #[test]
    fn a_held_state_is_released_over_the_one_it_was_computed_from_alone() {
        let mut cell = Cell::default();
        let shown = ran(&State::default(), Verb::WindowShow, &json!({}));
        let held = |state: &State, base| Held {
            state: state.clone(),
            base,
        };
        assert!(super::release(&mut cell, held(&shown, 0)));
        assert_eq!((cell.revision, cell.released, cell.dirty), (1, 1, true));
        assert!(
            !super::release(&mut cell, held(&State::default(), 0)),
            "an older one"
        );
        assert_eq!(cell.state, shown);
        assert!(super::release(&mut cell, held(&shown, 1)));
        assert_eq!(
            (cell.revision, cell.released),
            (1, 2),
            "the same state moves no revision"
        );
    }

    /// A word is one to sixty-four letters, digits or `-_.:/`.
    #[test]
    fn a_word_is_short_plain_and_visible() {
        for good in [
            "a",
            "surface-12",
            "list/3",
            "page:7",
            "a.b_c",
            &"a".repeat(64),
        ] {
            assert!(is_word(good), "{good:?}");
        }
        for bad in ["", "a b", "é", "a\u{200B}", &"a".repeat(65), "a\n"] {
            assert!(!is_word(bad), "{bad:?}");
        }
    }

    /// A note says the four facts of the interface's rate, each as the command
    /// it is the equivalent of, and nothing else.
    ///
    /// **One fact for each command an interface tells at the rate it draws**:
    /// the facts are in the order of their names, each one's command is a
    /// change of the interface's state, and a note of all four is read as
    /// them, each with its own arguments. A detail that is not an object, a
    /// note of no fact, one naming no fact, and one naming a command where a
    /// fact goes are refused with `[NC-VAL-0002]`, the refusal listing the
    /// facts.
    #[test]
    fn a_note_says_the_four_facts_of_the_interfaces_rate_and_nothing_else() {
        let names: Vec<&str> = super::FACTS.iter().map(|(fact, _)| *fact).collect();
        assert_eq!(names, ["draft", "focus", "hover", "scroll"]);
        assert_eq!(
            super::FACTS.map(|(_, verb)| verb),
            [
                Verb::PromptSet,
                Verb::ShellFocus,
                Verb::ShellHover,
                Verb::PageScroll
            ]
        );
        let note = Value::object([
            ("scroll", Value::object([("to", Value::Integer(3))])),
            ("draft", Value::object([("text", Value::text("a"))])),
        ]);
        let read = super::facts(&note).expect("read");
        assert_eq!(
            read.iter()
                .map(|(fact, verb, args)| (*fact, *verb, args.to_canonical()))
                .collect::<Vec<_>>(),
            [
                ("draft", Verb::PromptSet, r#"{"text":"a"}"#.to_owned()),
                ("scroll", Verb::PageScroll, r#"{"to":3}"#.to_owned()),
            ]
        );
        for (detail, saying) in [
            (Value::Array(Vec::new()), "one object"),
            (
                Value::object(Vec::<(&str, Value)>::new()),
                "one fact at least",
            ),
            (Value::object([("zoom", Value::Null)]), "`zoom`"),
            (
                Value::object([("page.scroll", Value::Null)]),
                "`page.scroll`",
            ),
        ] {
            let refused = super::facts(&detail).expect_err("refused");
            assert_eq!(
                said(&refused),
                (RefusalKind::Invalid, ErrorCode::ValidationArgumentsInvalid),
                "{detail:?}"
            );
            assert!(refused.sentence().contains(saying), "{refused:?}");
        }
        let refused = super::facts(&Value::object([("zoom", Value::Null)])).expect_err("none");
        assert!(
            refused
                .sentence()
                .contains("`scroll`, as `page.scroll` takes it"),
            "{refused:?}"
        );
    }

    /// A move is told only when the revision is another one.
    #[test]
    fn a_move_is_told_only_when_the_revision_is_another_one() {
        let moves = tokio::sync::watch::Sender::new(0_u64);
        let mut seen = moves.subscribe();
        super::told(&moves, 0);
        assert!(
            !seen.has_changed().expect("held"),
            "the same revision was told"
        );
        super::told(&moves, 3);
        assert!(seen.has_changed().expect("held"));
        assert_eq!(*seen.borrow_and_update(), 3);
    }
}
