//! The drawer (`NOCTUA.md §12.11`): the region below the footer, the design
//! system's `Drawer`, with a controls rail down its left side, the page in
//! focus in it, and a tabs rail down its right side while several pages are
//! open in it — and what it shows of the lists the keys open (§12.9): the
//! leader menu, the command list and a chosen command's parameters, and the
//! `/` and `w` lists, each the design system's `OptionList`, its rows
//! `OptionItem`s and a row's matched characters `MatchText`.
//!
//! **What it shows is a function of the state** ([`view`]), as the footer's
//! is: the drawer's pages and the one in focus, whether it is collapsed, where
//! the next page opened from the leader menu goes, what the prompt asks for
//! and holds, and a seat's keyboard selection, with the keys in force and the
//! registry's commands — so one state draws one drawer in every window.
//!
//! | The state shows | The drawer lists | A row runs |
//! |---|---|---|
//! | the prompt asking for the command list | every command the text after the `;` or `:` finds, best first, each its display name with the words' hits, its line and its parameters | the command, or its parameters asked for |
//! | the prompt asking for a command's parameters | the parameters not given and the values of the one being typed | the draft completed |
//! | the leader menu in focus | every binding of the menu's table in force, in columns, each its key and its command's display name | the binding's command |
//! | the `w` list in focus | the window's own actions | the action, or its parameters asked for |
//! | the `/` list in focus | the focused page's shortcuts: none, until pages declare them | — |
//!
//! **The rails are the design system's** (`DrawerRail`), each as wide as the
//! footer's corners. The controls rail is a `Toolbar` in its rail density,
//! drawn whenever the drawer is: collapse and close, two to a row, then the
//! controls of the page in focus — the leader menu's direction control, which
//! shows where the next page opened from the menu goes, a Leader tap's flip
//! included. The tabs rail is a `Tabs` in its rail appearance, one tab for
//! each page open in the drawer, in the order they were opened, the page in
//! focus chosen, and no rail with one page open.
//!
//! **Every control and every tab runs a command, dispatched as the person**
//! ([`Control`], [`tab`]): collapse is `drawer.collapse`; close is
//! `page.close` in the drawer — the page in focus when the kernel runs it —
//! or `prompt.clear` while the prompt's list is what the drawer shows, which
//! closes that list as Esc does; the direction control is `drawer.toward`,
//! naming the other place than the one it shows; and a tab is `page.focus`
//! on the page it names, in the drawer. The window gives each its arguments
//! from the state it holds and from nothing the page says but which control
//! or which tab, so the CLI does all of it.
//!
//! **Decided here, where `NOCTUA.md` is silent**: a page in a tab is named by
//! the display name of the command that opens its kind, and its topic after
//! it, with a glyph for its kind, until pages declare their titles; the
//! drawer is named by the word the catalog files every command of its own
//! under, its first tag in the window's locale ([`part_named`]) — `drawer`,
//! `gaveta` — the controls rail by what the drawer shows, the list's name or
//! the page in focus's, a tabs rail by `page.focus`'s display name, a list by
//! the display name of the command that opens it, until the interface's own
//! texts exist in both locales; and **the content is a tab panel of each tab**
//! while the tabs rail is there — the design system's `TabPanel`, the page in
//! focus's holding what the drawer shows and every other's nothing — so every
//! tab names a panel the page holds.
//!
//! **Not drawn yet**, and why: what a page other than a list shows in the
//! drawer, which arrives with the pages; the sentence an empty list says,
//! which arrives with the interface's own texts; and the share of the height
//! the state holds for the drawer (`drawer.resize`), since the design
//! system's `Drawer` takes none — it is as tall as what it holds, up to its
//! token, `--noctua-shell-drawer-max` of the window, which the document
//! measures it against by putting it straight in the window's column.

use dioxus::prelude::*;
use nc_canon::Value;
use nc_catalog::Locale;
use noctua_design::{
    Button, ButtonSize, ButtonTone, ButtonVariant, Drawer, DrawerRail, DrawerRailSide, DrawerSide,
    IconDrawing, OptionItem, OptionList, OptionListLayout, Tab, TabPanel, Tabs, TabsAppearance,
    Toolbar, ToolbarDensity, ToolbarGroup,
};

use crate::commands::{Commands, offered};
use crate::footer::title_of;
use crate::keys::Keymap;
use crate::mirror::{Asking, Kind, Page, Place, Shell};

/// The drawer's id in the document, which a draw brings it by.
pub const ID: &str = "noctua-drawer";

/// The name of the tabs rail's strip, which its tabs repeat and the page says
/// a tab chosen by.
pub const TABS: &str = "pages";

/// A command of the drawer's own, whose first tag is the word the drawer is
/// named by ([`part_named`]).
const ID_COMMAND: &str = "drawer.expand";

/// The name a part of the window is given for a screen reader, of `command`,
/// one of that part's own: the first of its tags in `locale` — the word the
/// catalog files every command of the part under, `drawer` and `gaveta`,
/// `sidebar` and `barra lateral` — and its display name where it has none.
#[must_use]
pub fn part_named(command: &str, locale: Locale) -> String {
    nc_catalog::command(command)
        .and_then(|entry| nc_catalog::tags(entry.tags_key, locale))
        .and_then(|tags| tags.first())
        .map_or_else(|| title_of(command, locale), |tag| (*tag).to_owned())
}

/// The window's own actions, which the `w` list holds (§12.2, §12.9).
pub const ACTIONS: [&str; 4] = [
    "window.hide",
    "window.close",
    "window.move",
    "window.resize",
];

/// A list the page says a choice in: the drawer's lists, its tabs rail and
/// the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum List {
    /// The leader menu.
    Menu,
    /// The command list, or a chosen command's parameters.
    Commands,
    /// The `w` list.
    Actions,
    /// The `/` list.
    Shortcuts,
    /// The drawer's tabs rail, a tab for each page open in it.
    Tabs,
    /// The sidebar (§12.14).
    Sidebar,
}

impl List {
    /// Every one.
    pub const ALL: [Self; 6] = [
        Self::Menu,
        Self::Commands,
        Self::Actions,
        Self::Shortcuts,
        Self::Tabs,
        Self::Sidebar,
    ];

    /// Its name, which the page's events carry and the list's id is.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Menu => "leader",
            Self::Commands => "commands",
            Self::Actions => "actions",
            Self::Shortcuts => "shortcuts",
            Self::Tabs => TABS,
            Self::Sidebar => "sidebar",
        }
    }

    /// The list named `word`.
    #[must_use]
    pub fn of(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|list| list.name() == word)
    }

    /// The command whose display name names it, until the interface's own
    /// texts exist.
    #[must_use]
    pub const fn named_by(self) -> &'static str {
        match self {
            Self::Menu => "leader.tap",
            Self::Commands => "kernel.find",
            Self::Actions => "page.actions",
            Self::Shortcuts => "page.shortcuts",
            Self::Tabs => "page.focus",
            Self::Sidebar => "sidebar.show",
        }
    }
}

/// The command whose display name names a page of `kind`: the one that opens
/// that kind, until pages declare their titles.
const fn opened_by(kind: Kind) -> &'static str {
    match kind {
        Kind::Home | Kind::Surface => "page.open",
        Kind::Settings => "page.settings",
        Kind::Help => "page.help",
        Kind::Dev => "page.dev",
        Kind::Leader => List::Menu.named_by(),
        Kind::Commands => List::Commands.named_by(),
        Kind::Shortcuts => List::Shortcuts.named_by(),
        Kind::Actions => List::Actions.named_by(),
    }
}

/// The glyph a page of `kind` is drawn with in a rail or a list.
#[must_use]
pub const fn glyph_of(kind: Kind) -> IconDrawing {
    match kind {
        Kind::Home => IconDrawing::HOUSE,
        Kind::Settings => IconDrawing::SETTINGS,
        Kind::Help => IconDrawing::CIRCLE_QUESTION_MARK,
        Kind::Dev => IconDrawing::CODE,
        Kind::Surface => IconDrawing::APP_WINDOW,
        Kind::Leader => IconDrawing::KEYBOARD,
        Kind::Commands => IconDrawing::SQUARE_TERMINAL,
        Kind::Shortcuts => IconDrawing::SQUARE_SLASH,
        Kind::Actions => IconDrawing::PANELS_TOP_LEFT,
    }
}

/// What `page` is called in `locale`: the display name of the command that
/// opens its kind, and its topic after it where it has one.
#[must_use]
pub fn page_title(page: &Page, locale: Locale) -> String {
    let title = title_of(opened_by(page.kind), locale);
    match &page.topic {
        Some(topic) => format!("{title} {topic}"),
        None => title,
    }
}

/// The page `value` names, as a tab or an entry carries one: its identifier,
/// written in ASCII digits alone; nothing for any other value.
#[must_use]
pub fn identifier(value: &str) -> Option<u64> {
    (!value.is_empty() && value.len() <= 20 && value.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| value.parse().ok())
        .flatten()
        .filter(|id| *id >= 1)
}

/// `page.focus` on `page` in `place`, in `window` where one is named: what
/// choosing a page from a rail or a list runs.
#[must_use]
pub fn focused(page: u64, place: Place, window: Option<u64>) -> (String, Value) {
    let page = Value::Integer(i64::try_from(page).unwrap_or(i64::MAX));
    let place = Value::text(match place {
        Place::Workspace => "workspace",
        Place::Drawer => "drawer",
    });
    let arguments = match window {
        Some(window) => Value::object([
            ("page", page),
            ("in", place),
            (
                "window",
                Value::Integer(i64::try_from(window).unwrap_or(i64::MAX)),
            ),
        ]),
        None => Value::object([("page", page), ("in", place)]),
    };
    ("page.focus".to_owned(), arguments)
}

/// The page of the drawer the tab `value` names: one open in it, and nothing
/// for any other value, which the window runs nothing for.
#[must_use]
pub fn tab(shell: &Shell, value: &str) -> Option<u64> {
    identifier(value).filter(|page| shell.drawer.pages.contains(page))
}

/// Where the next page opened from the leader menu goes: the drawer's
/// direction, or the other place once a Leader tap flipped it (§12.8).
#[must_use]
pub const fn next_target(drawer: &crate::mirror::Drawer) -> Place {
    match (drawer.toward, drawer.flipped) {
        (Place::Workspace, true) | (Place::Drawer, false) => Place::Drawer,
        (Place::Drawer, true) | (Place::Workspace, false) => Place::Workspace,
    }
}

/// A control of the drawer's controls rail (§12.11): a closed set, each found
/// by the name of the command it runs, which the page says back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// Collapse: hide the whole drawer, keeping what is open in it.
    Collapse,
    /// Close: close the drawer page that has focus.
    Close,
    /// The leader menu's direction control (§12.9).
    Toward,
}

impl Control {
    /// Every one.
    pub const ALL: [Self; 3] = [Self::Collapse, Self::Close, Self::Toward];

    /// The word its control is found by: the name of the command it runs,
    /// whose display name names it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Collapse => "drawer.collapse",
            Self::Close => "page.close",
            Self::Toward => "drawer.toward",
        }
    }

    /// The control whose word is `word`; none for any other word.
    #[must_use]
    pub fn of(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|control| control.word() == word)
    }

    /// The command it runs over `shell`, and its arguments: collapse
    /// `drawer.collapse`; close `prompt.clear` while the prompt asks for
    /// something, whose list is then what the drawer shows, and otherwise
    /// `page.close` in the drawer, which closes the page in focus there when
    /// it runs; the direction `drawer.toward`, naming the other place than
    /// the one the next page goes to.
    #[must_use]
    pub fn run(self, shell: &Shell) -> (String, Value) {
        let none = || Value::object(Vec::<(&str, Value)>::new());
        let (command, arguments) = match self {
            Self::Collapse => ("drawer.collapse", none()),
            Self::Close if shell.asking.is_some() => ("prompt.clear", none()),
            Self::Close => ("page.close", Value::object([("in", Value::text("drawer"))])),
            Self::Toward => (
                "drawer.toward",
                Value::object([(
                    "region",
                    Value::text(match next_target(&shell.drawer) {
                        Place::Workspace => "drawer",
                        Place::Drawer => "workspace",
                    }),
                )]),
            ),
        };
        (command.to_owned(), arguments)
    }
}

/// One row of a list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// What the row is, as the page's events carry it.
    pub value: String,
    /// Its name.
    pub label: String,
    /// The characters of its name a query matched.
    pub hits: Option<String>,
    /// Its second line.
    pub detail: Option<String>,
    /// The key that acts on it, as a US board labels it.
    pub shortcut: Option<String>,
    /// Whether the keyboard is on it.
    pub active: bool,
}

/// The leader menu's direction control: where the next page opened from the
/// menu goes, and its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toward {
    /// Whether it goes to the drawer, which the control shows pressed.
    pub drawer: bool,
    /// `drawer.toward`'s display name.
    pub label: String,
}

/// The controls rail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rail {
    /// Its name: what the drawer shows, the list's name or the page in
    /// focus's.
    pub label: String,
    /// `drawer.collapse`'s display name, collapse's.
    pub collapse: String,
    /// `page.close`'s display name, close's.
    pub close: String,
    /// The controls of the page in focus: the direction control, while the
    /// leader menu is what the drawer shows.
    pub toward: Option<Toward>,
}

/// One tab of the tabs rail: a page open in the drawer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageTab {
    /// The page's identifier, the tab's value.
    pub page: u64,
    /// What the page is called.
    pub label: String,
    /// Its kind's glyph.
    pub glyph: IconDrawing,
    /// Whether it is the page in focus in the drawer.
    pub selected: bool,
}

/// What the drawer shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    /// Whether it is there at all: something open in it, or the prompt
    /// asking for its list.
    pub shown: bool,
    /// Whether it is collapsed.
    pub collapsed: bool,
    /// Its name ([`part_named`]).
    pub label: String,
    /// The list it shows, its name, and its rows.
    pub list: Option<(List, String, Vec<Row>)>,
    /// The controls rail, whenever the drawer is there.
    pub rail: Option<Rail>,
    /// The tabs rail, its name and its tabs, while two pages or more are open
    /// in the drawer.
    pub tabs: Option<(String, Vec<PageTab>)>,
}

impl View {
    /// The drawer of a document before the daemon has answered: not there.
    #[must_use]
    pub const fn hidden() -> Self {
        Self {
            shown: false,
            collapsed: false,
            label: String::new(),
            list: None,
            rail: None,
            tabs: None,
        }
    }

    /// The list the prompt names in `aria-controls`: the command list's,
    /// while it is shown.
    #[must_use]
    pub fn controls(&self) -> Option<String> {
        self.list
            .as_ref()
            .filter(|(list, ..)| *list == List::Commands)
            .map(|(list, ..)| list.name().to_owned())
    }
}

/// The parameters of `command`, as a row's second line shows them after its
/// line: each by its name, an optional one in brackets.
fn parameters_said(command: &crate::commands::Command) -> String {
    command
        .parameters
        .iter()
        .map(|parameter| {
            if parameter.required {
                parameter.name.clone()
            } else {
                format!("[{}]", parameter.name)
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// What one command shows as a row, its name's `hits` given.
fn command_row(command: &crate::commands::Command, hits: Option<String>, locale: Locale) -> Row {
    let texts = command.texts(locale);
    let parameters = parameters_said(command);
    let line = texts.map(|texts| texts.line.clone()).unwrap_or_default();
    Row {
        value: command.name.clone(),
        label: texts.map_or_else(|| command.name.clone(), |texts| texts.title.clone()),
        hits,
        detail: Some(if parameters.is_empty() {
            line
        } else {
            format!("{line} {parameters}")
        }),
        shortcut: None,
        active: false,
    }
}

/// The rows the prompt's list holds for `asking`, with `draft` typed.
fn prompts(asking: &Asking, draft: &str, commands: &Commands, locale: Locale) -> Vec<Row> {
    match asking {
        Asking::Commands => {
            let typed = draft.strip_prefix([';', ':']).unwrap_or(draft);
            commands
                .found(typed, locale)
                .into_iter()
                .filter_map(|found| {
                    commands
                        .list()
                        .get(found.index)
                        .map(|command| command_row(command, found.hits, locale))
                })
                .collect()
        }
        Asking::Parameters { command, .. } => commands
            .named(command)
            .map(|command| {
                offered(draft, command)
                    .into_iter()
                    .map(|offer| Row {
                        value: offer.value(),
                        label: match offer {
                            crate::commands::Offer::Parameter(name) => format!("{name}="),
                            crate::commands::Offer::Value(value) => value,
                        },
                        hits: None,
                        detail: None,
                        shortcut: None,
                        active: false,
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// What the drawer shows of `shell`, with the prompt holding `draft` and
/// seat 1's keyboard on `selected`, the keys `keymap` and the commands
/// `commands` in force, in `locale`.
#[must_use]
pub fn view(
    shell: Option<&Shell>,
    draft: &str,
    selected: Option<&str>,
    (keymap, commands): (&Keymap, &Commands),
    locale: Locale,
) -> View {
    let Some(shell) = shell else {
        return View::hidden();
    };
    let focused = shell
        .drawer
        .focused
        .and_then(|id| shell.pages.get(&id))
        .map(|page| page.kind);
    let (list, mut rows) = match (&shell.asking, focused) {
        (Some(asking), _) => (
            Some(List::Commands),
            prompts(asking, draft, commands, locale),
        ),
        (None, Some(Kind::Leader)) => (
            Some(List::Menu),
            keymap
                .menu()
                .into_iter()
                .map(|entry| Row {
                    label: title_of(&entry.command, locale),
                    value: entry.command,
                    hits: None,
                    detail: None,
                    shortcut: Some(entry.key.written_for(crate::keys::keyboard()).to_owned()),
                    active: false,
                })
                .collect(),
        ),
        (None, Some(Kind::Actions)) => (
            Some(List::Actions),
            ACTIONS
                .iter()
                .filter_map(|name| commands.named(name))
                .map(|command| command_row(command, None, locale))
                .collect(),
        ),
        (None, Some(Kind::Shortcuts)) => (Some(List::Shortcuts), Vec::new()),
        (None, _) => (None, Vec::new()),
    };
    // The keyboard's row: the seat's selection where it is one of them, else
    // the command list's best.
    let at = rows
        .iter()
        .position(|row| Some(row.value.as_str()) == selected)
        .or_else(|| {
            (matches!(shell.asking, Some(Asking::Commands)) && !rows.is_empty()).then_some(0)
        });
    if let Some(row) = at.and_then(|at| rows.get_mut(at)) {
        row.active = true;
    }
    let shown = !shell.drawer.pages.is_empty() || shell.asking.is_some();
    let named = list.map(|list| title_of(list.named_by(), locale));
    let rail = shown.then(|| Rail {
        label: named.clone().unwrap_or_else(|| {
            shell
                .drawer
                .focused
                .and_then(|id| shell.pages.get(&id))
                .map_or_else(
                    || part_named(ID_COMMAND, locale),
                    |page| page_title(page, locale),
                )
        }),
        collapse: title_of(Control::Collapse.word(), locale),
        close: title_of(Control::Close.word(), locale),
        toward: (list == Some(List::Menu)).then(|| Toward {
            drawer: next_target(&shell.drawer) == Place::Drawer,
            label: title_of(Control::Toward.word(), locale),
        }),
    });
    let tabs: Vec<PageTab> = shell
        .drawer
        .pages
        .iter()
        .filter_map(|id| shell.pages.get(id))
        .map(|page| PageTab {
            page: page.id,
            label: page_title(page, locale),
            glyph: glyph_of(page.kind),
            selected: shell.drawer.focused == Some(page.id),
        })
        .collect();
    View {
        shown,
        collapsed: shell.drawer.collapsed && shell.asking.is_none(),
        label: part_named(ID_COMMAND, locale),
        list: list.zip(named).map(|(list, named)| (list, named, rows)),
        rail,
        tabs: (tabs.len() > 1).then(|| (title_of(List::Tabs.named_by(), locale), tabs)),
    }
}

/// The drawer's markup for `view`: its region, whole.
#[must_use]
pub fn markup(view: &View) -> String {
    dioxus_ssr::render_element(rsx! {
        TheDrawer { view: view.clone() }
    })
}

/// The drawer, below the footer: a region holding its controls rail, the
/// list it shows and its tabs rail, or nothing while nothing is open in it.
///
/// **Its region is no box of its own** (`contents`), so the design system's
/// `Drawer` stands in the window's column and its cap, a share of the box
/// around it, is a share of the window's height.
#[component]
pub fn TheDrawer(view: View) -> Element {
    let View {
        shown,
        collapsed,
        label,
        list,
        rail,
        tabs,
    } = view;
    let panels: Vec<(String, bool)> = tabs
        .iter()
        .flat_map(|(_, tabs)| tabs)
        .map(|tab| (tab.page.to_string(), tab.selected))
        .collect();
    rsx! {
        div { id: ID, class: "contents",
            if shown {
                Drawer { name: ID, label, side: DrawerSide::Bottom, collapsed,
                    if let Some(rail) = rail {
                        ControlsRail { rail }
                    }
                    div {
                        if panels.is_empty() {
                            Shown { list }
                        } else {
                            for (value, selected) in panels {
                                TabPanel { name: TABS, value,
                                    if selected {
                                        Shown { list: list.clone() }
                                    }
                                }
                            }
                        }
                    }
                    if let Some((label, tabs)) = tabs {
                        DrawerRail { side: DrawerRailSide::End,
                            Tabs { name: TABS, label, appearance: TabsAppearance::Rail,
                                for tab in tabs {
                                    Tab {
                                        name: TABS,
                                        value: tab.page.to_string(),
                                        label: tab.label,
                                        icon: tab.glyph,
                                        selected: tab.selected,
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// What the drawer shows: the list, where it shows one.
#[component]
fn Shown(list: Option<(List, String, Vec<Row>)>) -> Element {
    rsx! {
        if let Some((list, label, rows)) = list {
            OptionList {
                name: list.name(),
                label,
                layout: if list == List::Menu { OptionListLayout::Columns } else { OptionListLayout::Rows },
                focusable: list != List::Commands,
                for row in rows {
                    OptionItem {
                        value: row.value,
                        label: row.label,
                        hits: row.hits,
                        detail: row.detail,
                        shortcut: row.shortcut,
                        active: row.active,
                    }
                }
            }
        }
    }
}

/// The controls rail, down the drawer's left side: collapse and close, two to
/// a row, then the controls of the page in focus.
#[component]
fn ControlsRail(rail: Rail) -> Element {
    let Rail {
        label,
        collapse,
        close,
        toward,
    } = rail;
    rsx! {
        DrawerRail { side: DrawerRailSide::Start,
            Toolbar { label, density: ToolbarDensity::Rail,
                ToolbarGroup {
                    Button {
                        variant: ButtonVariant::Transparent,
                        tone: ButtonTone::Neutral,
                        size: ButtonSize::Small,
                        hide_label: true,
                        icon: IconDrawing::PANEL_BOTTOM_CLOSE,
                        label: collapse,
                        action: Control::Collapse.word(),
                    }
                    Button {
                        variant: ButtonVariant::Transparent,
                        tone: ButtonTone::Neutral,
                        size: ButtonSize::Small,
                        hide_label: true,
                        icon: IconDrawing::SQUARE_X,
                        label: close,
                        action: Control::Close.word(),
                    }
                }
                if let Some(toward) = toward {
                    ToolbarGroup {
                        Button {
                            variant: ButtonVariant::Transparent,
                            tone: ButtonTone::Neutral,
                            size: ButtonSize::Small,
                            hide_label: true,
                            icon: IconDrawing::ARROW_UP_TO_LINE,
                            icon_alt: IconDrawing::ARROW_DOWN_TO_LINE,
                            pressed: toward.drawer,
                            label: toward.label,
                            action: Control::Toward.word(),
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::BTreeMap;

    use nc_catalog::Locale;
    use nc_schema::keys::{Binding, Chord, Leader, LeaderPart, Modifier};

    use nc_canon::Value;

    use super::{
        ACTIONS, Control, List, PageTab, TABS, focused, glyph_of, identifier, markup, next_target,
        page_title, tab, view,
    };
    use crate::commands::Commands;
    use crate::footer::title_of;
    use crate::keys::Keymap;
    use crate::mirror::{Asking, Drawer, Kind, Page, Place, Shell};

    /// The catalog's keys under the development Leader.
    fn keymap() -> Keymap {
        let catalog = nc_channel::keys::catalog_bindings();
        let defaults: Vec<(&str, Vec<Binding>)> = catalog
            .iter()
            .map(|(command, bound)| (command.as_str(), bound.clone()))
            .collect();
        Keymap::new(
            LeaderPart {
                leader: Leader::of(Chord::parse("Alt+Super+Space").expect("a chord"))
                    .expect("a Leader"),
                chord: Modifier::Shift,
            },
            nc_schema::keys::tables(&defaults, &[]).expect("tables"),
        )
    }

    /// A state whose drawer holds one page of `kind`, in focus.
    fn with(kind: Kind) -> Shell {
        let mut pages = BTreeMap::new();
        pages.insert(
            3,
            Page {
                id: 3,
                kind,
                topic: None,
                trail: Vec::new(),
                scroll: 0,
            },
        );
        Shell {
            windows: Vec::new(),
            pages,
            drawer: Drawer {
                pages: vec![3],
                focused: Some(3),
                collapsed: false,
                toward: Place::Workspace,
                flipped: false,
                share: 33,
            },
            sidebar: false,
            footer: true,
            expanded: false,
            message: None,
            draft: String::new(),
            asking: None,
            seats: BTreeMap::new(),
            settings: crate::mirror::Settings::default(),
            next: 4,
        }
    }

    /// The leader menu fills the drawer in columns, each entry a key and the
    /// display name of the command its binding runs, in the window's locale.
    #[test]
    pub(crate) fn the_leader_menu_is_its_keys_and_names_in_columns() {
        for locale in Locale::ALL {
            let shown = view(
                Some(&with(Kind::Leader)),
                "",
                None,
                (&keymap(), &Commands::from_catalog()),
                *locale,
            );
            let (list, _, rows) = shown.list.clone().expect("a list");
            assert_eq!(list, List::Menu);
            assert_eq!(rows.len(), keymap().menu().len());
            let settings = rows
                .iter()
                .find(|row| row.value == "page.settings")
                .expect("an entry");
            assert_eq!(settings.label, title_of("page.settings", *locale));
            assert_eq!(settings.shortcut.as_deref(), Some("S"));
            let written = markup(&shown);
            assert!(written.contains("data-nd-layout=\"columns\""), "{written}");
            assert!(
                written.contains("data-nd-option-list=\"leader\""),
                "{written}"
            );
            assert!(
                written.contains("tabindex=\"0\""),
                "the menu takes the keyboard"
            );
        }
    }

    /// The command list holds what the text after the `;` finds, the best
    /// first and active, each row its display name with the words' hits, its
    /// line and its parameters; a seat's selection among them is the active
    /// one; the prompt names it.
    #[test]
    pub(crate) fn the_command_list_holds_what_the_query_finds_with_its_hits() {
        let mut shell = with(Kind::Commands);
        shell.asking = Some(Asking::Commands);
        let commands = Commands::from_catalog();
        let shown = view(
            Some(&shell),
            ";open set",
            None,
            (&keymap(), &commands),
            Locale::En,
        );
        let (list, _, rows) = shown.list.clone().expect("a list");
        assert_eq!(list, List::Commands);
        assert!(rows[0].active, "the best is the keyboard's");
        assert_eq!(rows.iter().filter(|row| row.active).count(), 1);
        assert!(rows.iter().any(|row| row.value == "page.settings"));
        assert_eq!(shown.controls().as_deref(), Some("commands"));
        for opening in [";", ":"] {
            let every = view(
                Some(&shell),
                opening,
                None,
                (&keymap(), &commands),
                Locale::En,
            );
            let (_, _, rows) = every.list.expect("a list");
            assert_eq!(
                rows.len(),
                commands.list().len(),
                "{opening} lists every command"
            );
            let opened = rows
                .iter()
                .find(|row| row.value == "page.open")
                .expect("listed");
            assert_eq!(
                opened.detail.as_deref(),
                Some(
                    format!(
                        "{} kind [in] [topic] [window]",
                        nc_catalog::message("command.page.open.description", Locale::En)
                            .expect("a line")
                    )
                    .as_str()
                ),
                "its line, then its parameters"
            );
            let settings = rows
                .iter()
                .find(|row| row.value == "page.settings")
                .expect("listed");
            assert_eq!(settings.label, title_of("page.settings", Locale::En));
        }
        let chosen = view(
            Some(&shell),
            ";open",
            Some("page.help"),
            (&keymap(), &commands),
            Locale::En,
        );
        let (_, _, rows) = chosen.list.expect("a list");
        assert!(
            rows.iter()
                .any(|row| row.value == "page.help" && row.active)
        );
        assert_eq!(rows.iter().filter(|row| row.active).count(), 1);
        let written = markup(&view(
            Some(&shell),
            ";",
            None,
            (&keymap(), &commands),
            Locale::En,
        ));
        assert_eq!(
            written.matches("role=\"option\"").count(),
            commands.list().len(),
            "an empty query lists every command"
        );
    }

    /// A row's name is coloured per word, by the positions each word matched
    /// on it: the first word's characters one tone, the second's another.
    #[test]
    pub(crate) fn a_rows_name_is_coloured_per_word() {
        let mut shell = with(Kind::Commands);
        shell.asking = Some(Asking::Commands);
        let one = Commands::new(vec![crate::commands::Command {
            name: "a.b".to_owned(),
            texts: Locale::ALL
                .iter()
                .map(|_| crate::commands::Texts {
                    title: "Open settings".to_owned(),
                    line: "A line.".to_owned(),
                    tags: vec!["x".to_owned()],
                })
                .collect(),
            parameters: Vec::new(),
        }]);
        let coloured = view(
            Some(&shell),
            ";open set",
            None,
            (&keymap(), &one),
            Locale::En,
        );
        let (_, _, rows) = coloured.list.expect("a list");
        assert_eq!(rows[0].hits.as_deref(), Some("0-3|5-7"));
        let written = markup(&view(
            Some(&shell),
            ";open set",
            None,
            (&keymap(), &one),
            Locale::En,
        ));
        assert!(
            written.contains("data-nd-word=\"1\">Open</mark>")
                && written.contains("data-nd-word=\"2\">set</mark>"),
            "{written}"
        );
    }

    /// Commands named `a.<n>`, each its display name in English and in
    /// Brazilian Portuguese as `titles` gives them, with no tag.
    fn named(titles: &[(&str, &str)]) -> Commands {
        Commands::new(
            titles
                .iter()
                .enumerate()
                .map(|(at, (english, portuguese))| crate::commands::Command {
                    name: format!("a.{at}"),
                    texts: Locale::ALL
                        .iter()
                        .map(|locale| crate::commands::Texts {
                            title: (*if *locale == Locale::En {
                                english
                            } else {
                                portuguese
                            })
                            .to_owned(),
                            line: "A line.".to_owned(),
                            tags: Vec::new(),
                        })
                        .collect(),
                    parameters: Vec::new(),
                })
                .collect(),
        )
    }

    /// The command list `commands` holds for `typed` after the `;`, with the
    /// keyboard on `selected`, in `locale`, as markup.
    fn listed(commands: &Commands, typed: &str, selected: Option<&str>, locale: Locale) -> String {
        let mut shell = with(Kind::Commands);
        shell.asking = Some(Asking::Commands);
        markup(&view(
            Some(&shell),
            &format!(";{typed}"),
            selected,
            (&keymap(), commands),
            locale,
        ))
    }

    /// The option of the command `name` in `written`, whole.
    fn option<'a>(written: &'a str, name: &str) -> &'a str {
        let start = written
            .find(&format!("data-nd-value=\"{name}\""))
            .unwrap_or_else(|| panic!("{name} is not listed: {written}"));
        let end = written[start..].find("</li>").expect("closed") + start;
        &written[start..end]
    }

    /// The query is split into words at its spaces and every word must match:
    /// a row one word cannot match is not listed, and each listed row is one
    /// every word matched (§12.4).
    #[test]
    pub(crate) fn a_row_one_word_cannot_match_is_not_listed() {
        let commands = named(&[
            ("Open settings", "Abrir ajustes"),
            ("Open help", "Abrir ajuda"),
        ]);
        let written = listed(&commands, "open set", None, Locale::En);
        assert_eq!(written.matches("role=\"option\"").count(), 1, "{written}");
        assert!(written.contains("data-nd-value=\"a.0\""), "{written}");
        let both = listed(&commands, "open", None, Locale::En);
        assert_eq!(both.matches("role=\"option\"").count(), 2, "{both}");
    }

    /// The text matched is every language's at once: a word of the other
    /// locale's display name lists the row, which shows the window's own and
    /// marks nothing on it, since no word matched that field (§12.4).
    #[test]
    pub(crate) fn a_word_found_in_either_language_lists_its_row() {
        let commands = named(&[
            ("Open settings", "Abrir ajustes"),
            ("Open help", "Abrir ajuda"),
        ]);
        for (typed, locale, shown) in [
            ("ajustes", Locale::En, "Open settings"),
            ("settings", Locale::PtBR, "Abrir ajustes"),
        ] {
            let written = listed(&commands, typed, None, locale);
            assert_eq!(written.matches("role=\"option\"").count(), 1, "{written}");
            let row = option(&written, "a.0");
            assert!(row.contains(&format!(">{shown}</span>")), "{row}");
            assert!(!row.contains("<mark"), "{row}");
        }
    }

    /// The first word's characters are marked as the first, the second's as
    /// the second, and the third's and every later word's share the one
    /// mark of the rest (§12.4).
    #[test]
    pub(crate) fn the_third_and_every_later_word_share_one_tone() {
        let commands = named(&[("Alpha Beta Gamma Delta", "Um dois tr\u{ea}s quatro")]);
        let written = listed(&commands, "alpha beta gamma delta", None, Locale::En);
        let row = option(&written, "a.0");
        for (word, tone) in [("Alpha", 1), ("Beta", 2), ("Gamma", 3), ("Delta", 3)] {
            assert!(
                row.contains(&format!("data-nd-word=\"{tone}\">{word}</mark>")),
                "{word}: {row}"
            );
        }
        assert_eq!(row.matches("<mark").count(), 4, "{row}");
    }

    /// Two words may claim one character, which is marked as the first
    /// word's; the second word marks what it alone matched (§12.4).
    #[test]
    pub(crate) fn two_words_claiming_one_character_draw_it_in_the_first_words_tone() {
        let commands = named(&[("Settings", "Ajustes")]);
        let row = listed(&commands, "set settings", None, Locale::En);
        let row = option(&row, "a.0");
        assert!(
            row.contains("data-nd-word=\"1\">Set</mark><mark class=\"nd-match\" role=\"none\" data-nd-word=\"2\">tings</mark>"),
            "{row}"
        );
    }

    /// A position is a character of the name drawn, never a byte: a word
    /// folded over its accents marks the accented characters it matched,
    /// and a grapheme cluster is marked whole (§12.4).
    #[test]
    pub(crate) fn a_position_is_a_character_and_a_cluster_is_marked_whole() {
        let commands = named(&[
            ("A\u{e7}\u{e3}o r\u{e1}pida", "A\u{e7}\u{e3}o r\u{e1}pida"),
            ("Cafe\u{301} menu", "Card\u{e1}pio"),
        ]);
        let accented = option(&listed(&commands, "acao rap", None, Locale::En), "a.0").to_owned();
        assert!(
            accented.contains("data-nd-word=\"1\">A\u{e7}\u{e3}o</mark> <mark class=\"nd-match\" role=\"none\" data-nd-word=\"2\">r\u{e1}p</mark>ida"),
            "{accented}"
        );
        let combined = option(&listed(&commands, "cafe", None, Locale::En), "a.1").to_owned();
        assert!(
            combined.contains("data-nd-word=\"1\">Cafe\u{301}</mark> menu"),
            "{combined}"
        );
    }

    /// The row the keyboard is on keeps its marks as they are on every other
    /// row: being selected draws them alike, each word's in its own tone
    /// (§12.4).
    #[test]
    pub(crate) fn the_selected_row_keeps_its_match_colours() {
        let commands = named(&[
            ("Open settings", "Abrir ajustes"),
            ("Open setup", "Abrir preparo"),
        ]);
        let marks = |row: &str| -> String {
            let (_, text) = row
                .split_once("<span class=\"nd-option-item-text\">")
                .expect("its text");
            text.to_owned()
        };
        let resting = listed(&commands, "open set", Some("a.0"), Locale::En);
        let chosen = listed(&commands, "open set", Some("a.1"), Locale::En);
        assert!(
            option(&chosen, "a.1").contains("data-nd-active=\"true\""),
            "{chosen}"
        );
        assert!(
            !option(&resting, "a.1").contains("data-nd-active"),
            "{resting}"
        );
        assert_eq!(
            marks(option(&chosen, "a.1")),
            marks(option(&resting, "a.1"))
        );
        assert!(
            marks(option(&chosen, "a.1")).contains("data-nd-word=\"1\">Open</mark>")
                && marks(option(&chosen, "a.1")).contains("data-nd-word=\"2\">set</mark>"),
            "{chosen}"
        );
    }

    /// A chosen command's parameters are offered, none active, so Enter runs
    /// the command with what was given.
    #[test]
    fn a_chosen_commands_parameters_are_offered() {
        let mut shell = with(Kind::Commands);
        shell.asking = Some(Asking::Parameters {
            command: "page.open".to_owned(),
            parameter: None,
        });
        let shown = view(
            Some(&shell),
            "kind=he",
            None,
            (&keymap(), &Commands::from_catalog()),
            Locale::En,
        );
        let (_, _, rows) = shown.list.expect("a list");
        assert_eq!(
            rows.iter()
                .map(|row| row.value.as_str())
                .collect::<Vec<_>>(),
            ["value:help"]
        );
        assert!(rows.iter().all(|row| !row.active));
    }

    /// The `w` list holds the window's own actions, and the `/` list nothing
    /// yet; any other page in focus shows no list, and an empty drawer is
    /// not there.
    #[test]
    pub(crate) fn the_w_list_holds_the_windows_actions_and_nothing_else_is_listed() {
        let commands = Commands::from_catalog();
        let shown = view(
            Some(&with(Kind::Actions)),
            "",
            None,
            (&keymap(), &commands),
            Locale::PtBR,
        );
        let (list, _, rows) = shown.list.expect("a list");
        assert_eq!(list, List::Actions);
        assert_eq!(
            rows.iter()
                .map(|row| row.value.as_str())
                .collect::<Vec<_>>(),
            ACTIONS
        );
        let shown = view(
            Some(&with(Kind::Shortcuts)),
            "",
            None,
            (&keymap(), &commands),
            Locale::En,
        );
        assert_eq!(
            shown.list.map(|(list, _, rows)| (list, rows.len())),
            Some((List::Shortcuts, 0))
        );
        let shown = view(
            Some(&with(Kind::Settings)),
            "",
            None,
            (&keymap(), &commands),
            Locale::En,
        );
        assert!(shown.shown && shown.list.is_none());
        let mut empty = with(Kind::Settings);
        empty.drawer.pages.clear();
        empty.drawer.focused = None;
        let shown = view(Some(&empty), "", None, (&keymap(), &commands), Locale::En);
        assert!(!shown.shown);
        assert!(!markup(&shown).contains("nd-drawer"));
        assert!(!view(None, "", None, (&keymap(), &commands), Locale::En).shown);
        for list in List::ALL {
            assert_eq!(List::of(list.name()), Some(list));
        }
        assert_eq!(List::of("menu"), None);
    }

    // -- the rails ------------------------------------------------------------

    /// A state whose drawer holds a page of each of `kinds`, numbered from 3
    /// in that order, the last in focus.
    fn holding(kinds: &[(Kind, Option<&str>)]) -> Shell {
        let mut shell = with(Kind::Home);
        shell.pages.clear();
        shell.drawer.pages.clear();
        for (id, (kind, topic)) in (3_u64..).zip(kinds) {
            shell.pages.insert(
                id,
                Page {
                    id,
                    kind: *kind,
                    topic: topic.map(str::to_owned),
                    trail: Vec::new(),
                    scroll: 0,
                },
            );
            shell.drawer.pages.push(id);
            shell.drawer.focused = Some(id);
        }
        shell
    }

    /// What the drawer shows of `shell` in `locale`, with nothing typed.
    fn of(shell: &Shell, locale: Locale) -> super::View {
        view(
            Some(shell),
            "",
            None,
            (&keymap(), &Commands::from_catalog()),
            locale,
        )
    }

    /// Whenever something is open in the drawer, a controls rail runs down
    /// its left side holding collapse and close, two to a row — one group of
    /// two controls, each a glyph named by its command's display name in the
    /// window's locale — and the drawer's region is no box of its own, so the
    /// design system's cap is a share of the window's height. With nothing
    /// open there is no rail.
    #[test]
    pub(crate) fn a_controls_rail_holds_collapse_and_close_whenever_something_is_open() {
        for locale in Locale::ALL {
            for kind in [Kind::Settings, Kind::Actions, Kind::Leader, Kind::Commands] {
                let shown = of(&holding(&[(kind, None)]), *locale);
                let rail = shown.rail.clone().expect("a controls rail");
                assert_eq!(rail.collapse, title_of("drawer.collapse", *locale));
                assert_eq!(rail.close, title_of("page.close", *locale));
                let written = markup(&shown);
                assert!(
                    written.starts_with("<div id=\"noctua-drawer\" class=\"contents\">"),
                    "{written}"
                );
                let start = written
                    .find("class=\"nd-drawer-rail\" data-nd-side=\"start\"")
                    .expect("a rail at the start");
                let toolbar = &written[start..];
                assert!(
                    toolbar.contains("role=\"toolbar\"")
                        && toolbar.contains("data-nd-density=\"rail\""),
                    "{written}"
                );
                // One group of the rail's density is one row of two: the
                // first group holds the two controls, and the page's own
                // follow in a group of their own.
                let (first, rest) = toolbar
                    .split_once("role=\"group\"")
                    .map(|(_, after)| after.split_once("role=\"group\"").unwrap_or((after, "")))
                    .expect("a group of controls");
                assert_eq!(
                    first.matches("<button").count(),
                    2,
                    "collapse and close share one row: {written}"
                );
                assert!(
                    matches!(
                        (
                            first.find("data-nd-action=\"drawer.collapse\""),
                            first.find("data-nd-action=\"page.close\"")
                        ),
                        (Some(collapse), Some(close)) if collapse < close
                    ),
                    "collapse, then close: {written}"
                );
                assert_eq!(
                    rest.is_empty(),
                    kind != Kind::Leader,
                    "only the leader menu has controls of its own: {written}"
                );
                assert!(
                    written.find("nd-drawer-rail") < written.find("data-nd-option-list")
                        || !written.contains("data-nd-option-list"),
                    "the controls rail is written before the content: {written}"
                );
            }
        }
        let mut empty = holding(&[]);
        empty.drawer.focused = None;
        assert_eq!(of(&empty, Locale::En).rail, None);
    }

    /// A collapsed drawer with something open in it is the design system's
    /// drawer collapsed, its pages and its rails kept, and the modeline shows
    /// how many there are (`crate::footer`).
    #[test]
    pub(crate) fn a_collapsed_drawer_keeps_what_is_open_in_it() {
        let mut shell = holding(&[(Kind::Settings, None), (Kind::Help, None)]);
        shell.drawer.collapsed = true;
        let shown = of(&shell, Locale::En);
        assert!(shown.shown && shown.collapsed);
        assert!(shown.rail.is_some() && shown.tabs.is_some());
        assert!(markup(&shown).contains("data-nd-collapsed=\"true\""));
    }

    /// The controls of the page in focus follow the controls rail's first
    /// row: while the leader menu is what the drawer shows, the direction
    /// control, pressed while the next page opened from the menu goes to the
    /// drawer — the drawer's direction, or the other place once a Leader tap
    /// flipped it — and named by `drawer.toward`'s display name; on any other
    /// page, none.
    #[test]
    pub(crate) fn the_leader_menus_direction_control_shows_where_the_next_page_goes() {
        for (toward, flipped, drawer) in [
            (Place::Workspace, false, false),
            (Place::Workspace, true, true),
            (Place::Drawer, false, true),
            (Place::Drawer, true, false),
        ] {
            let mut shell = holding(&[(Kind::Leader, None)]);
            shell.drawer.toward = toward;
            shell.drawer.flipped = flipped;
            assert_eq!(next_target(&shell.drawer) == Place::Drawer, drawer);
            let shown = of(&shell, Locale::PtBR);
            let control = shown
                .rail
                .clone()
                .and_then(|rail| rail.toward)
                .expect("the direction control");
            assert_eq!(control.drawer, drawer, "{toward:?} {flipped}");
            assert_eq!(control.label, title_of("drawer.toward", Locale::PtBR));
            let written = markup(&shown);
            let groups = written.matches("role=\"group\"").count();
            assert_eq!(groups, 2, "its own row, beneath the first: {written}");
            assert_eq!(
                written.contains("aria-pressed=\"true\""),
                drawer,
                "pressed while the next page goes to the drawer: {written}"
            );
            assert!(written.contains("data-nd-action=\"drawer.toward\""));
        }
        for kind in [Kind::Actions, Kind::Settings, Kind::Commands] {
            let shown = of(&holding(&[(kind, None)]), Locale::En);
            assert_eq!(shown.rail.and_then(|rail| rail.toward), None, "{kind:?}");
        }
    }

    /// With several pages open in the drawer a tabs rail runs down its right
    /// side, a tab for each in the order they were opened — named by what
    /// opens its kind and its topic, drawn by its kind's glyph — the page in
    /// focus chosen; with one page open there is no tabs rail.
    #[test]
    pub(crate) fn a_tabs_rail_lists_the_pages_only_while_several_are_open() {
        for locale in Locale::ALL {
            let single = of(&holding(&[(Kind::Settings, None)]), *locale);
            assert_eq!(single.tabs, None);
            assert!(!markup(&single).contains("data-nd-tabs"));
            let mut shell = holding(&[
                (Kind::Settings, Some("mcp")),
                (Kind::Help, None),
                (Kind::Leader, None),
            ]);
            for in_focus in [5_u64, 4] {
                shell.drawer.focused = Some(in_focus);
                let shown = of(&shell, *locale);
                let (label, tabs) = shown.tabs.clone().expect("a tabs rail");
                assert_eq!(label, title_of("page.focus", *locale));
                assert_eq!(
                    tabs,
                    [3_u64, 4, 5]
                        .iter()
                        .map(|id| {
                            let page = shell.pages.get(id).expect("held");
                            PageTab {
                                page: *id,
                                label: page_title(page, *locale),
                                glyph: glyph_of(page.kind),
                                selected: *id == in_focus,
                            }
                        })
                        .collect::<Vec<_>>()
                );
                assert_eq!(
                    tabs.first().map(|tab| tab.label.clone()),
                    Some(format!("{} mcp", title_of("page.settings", *locale)))
                );
                let written = markup(&shown);
                let end = written
                    .find("class=\"nd-drawer-rail\" data-nd-side=\"end\"")
                    .expect("a rail at the end");
                if in_focus == 5 {
                    assert!(
                        written
                            .find("data-nd-option-list")
                            .is_some_and(|content| content < end),
                        "the tabs rail is written after the content: {written}"
                    );
                }
                let rail = &written[end..];
                assert!(
                    rail.contains(&format!("data-nd-tabs=\"{TABS}\"")),
                    "{written}"
                );
                assert!(rail.contains("data-nd-appearance=\"rail\""), "{written}");
                assert_eq!(rail.matches("class=\"nd-tab\"").count(), 3, "{written}");
                assert_eq!(
                    rail.matches("data-nd-selected=\"true\"").count(),
                    1,
                    "{written}"
                );
                assert!(
                    rail.contains(&format!(
                        "data-nd-value=\"{in_focus}\" data-nd-selected=\"true\""
                    )),
                    "{written}"
                );
            }
        }
    }

    /// Every tab names a panel the drawer holds: with the tabs rail there,
    /// the content is the design system's tab panel of each tab, by the
    /// strip's name and the tab's page, the page in focus's holding what the
    /// drawer shows and every other's nothing — so no tab's `aria-controls`
    /// or address names an element the page does not hold. With one page
    /// there is no rail and no panel.
    #[test]
    pub(crate) fn every_tab_names_a_panel_the_drawer_holds() {
        let ids = |written: &str, name: &str| -> Vec<String> {
            written
                .split(&format!(" {name}=\""))
                .skip(1)
                .filter_map(|rest| rest.split_once('"').map(|(value, _)| value.to_owned()))
                .collect()
        };
        let mut shell = holding(&[
            (Kind::Settings, None),
            (Kind::Help, None),
            (Kind::Leader, None),
        ]);
        for in_focus in [5_u64, 3] {
            shell.drawer.focused = Some(in_focus);
            let written = markup(&of(&shell, Locale::En));
            let held = ids(&written, "id");
            let named = ids(&written, "aria-controls");
            assert_eq!(named.len(), 3, "{written}");
            for panel in &named {
                assert!(
                    held.contains(panel),
                    "{panel} is not in the drawer: {written}"
                );
                assert!(
                    written.contains(&format!("href=\"#{panel}\"")),
                    "the tab's address is its panel: {written}"
                );
            }
            let panels: Vec<&str> = written.split("class=\"nd-tab-panel\"").skip(1).collect();
            assert_eq!(panels.len(), 3, "{written}");
            for (panel, page) in panels.iter().zip([3_u64, 4, 5]) {
                assert!(
                    panel.starts_with(&format!(" id=\"nd-tabs-{TABS}-{page}-panel\"")),
                    "{panel}"
                );
                let body = panel.split("class=\"nd-drawer-rail\"").next().unwrap_or("");
                assert_eq!(
                    body.contains("data-nd-option-list"),
                    page == in_focus && in_focus == 5,
                    "only the page in focus's panel holds what the drawer shows: {written}"
                );
            }
        }
        let single = markup(&of(&holding(&[(Kind::Leader, None)]), Locale::En));
        assert!(
            !single.contains("nd-tab-panel") && !single.contains("aria-controls"),
            "{single}"
        );
    }

    /// The drawer and its controls rail are named apart from each other and
    /// from the footer: the drawer by the word the catalog files every
    /// command of its own under — the first tag of each, the same word in
    /// each locale — and the rail by what the drawer shows, the list's name
    /// or the page in focus's.
    #[test]
    pub(crate) fn the_drawer_and_its_rail_are_named_apart_from_the_footer() {
        for locale in Locale::ALL {
            let noun = super::part_named("drawer.expand", *locale);
            for command in nc_catalog::COMMANDS
                .iter()
                .filter(|command| command.name.starts_with("drawer."))
            {
                assert_eq!(
                    nc_catalog::tags(command.tags_key, *locale)
                        .and_then(|tags| tags.first())
                        .copied(),
                    Some(noun.as_str()),
                    "{} is filed under the drawer's word",
                    command.name
                );
            }
            for (kind, rail) in [
                (Kind::Leader, title_of(List::Menu.named_by(), *locale)),
                (Kind::Settings, title_of("page.settings", *locale)),
            ] {
                let shown = of(&holding(&[(kind, None)]), *locale);
                assert_eq!(shown.label, noun);
                assert_eq!(
                    shown.rail.as_ref().map(|rail| rail.label.clone()),
                    Some(rail.clone())
                );
                let written = markup(&shown);
                let labels = {
                    let mut labels: Vec<&str> = written
                        .split(" aria-label=\"")
                        .skip(1)
                        .filter_map(|rest| rest.split_once('"').map(|(label, _)| label))
                        .collect();
                    labels.truncate(2);
                    labels
                };
                assert_eq!(labels, [noun.as_str(), rail.as_str()], "{written}");
                assert!(
                    !labels.contains(
                        &crate::text::said(nc_catalog::interface::Text::PartFooter, *locale)
                            .as_str()
                    ),
                    "the footer's name is the footer's: {written}"
                );
            }
        }
    }

    /// Every control runs its command with the arguments the window gives
    /// it from the state: collapse `drawer.collapse`; close the page in focus
    /// in the drawer, or the prompt's list while it is what the drawer shows;
    /// the direction control the other place than the one it shows. A tab
    /// runs `page.focus` on a page the drawer holds, and on no other.
    #[test]
    pub(crate) fn every_control_and_tab_runs_its_command() {
        let ran = |command: &str, arguments: Value| (command.to_owned(), arguments);
        let none = Value::object(Vec::<(&str, Value)>::new());
        let shell = holding(&[(Kind::Settings, None), (Kind::Leader, None)]);
        assert_eq!(
            Control::Collapse.run(&shell),
            ran("drawer.collapse", none.clone())
        );
        assert_eq!(
            Control::Close.run(&shell),
            ran("page.close", Value::object([("in", Value::text("drawer"))]))
        );
        let mut asking = shell.clone();
        asking.asking = Some(Asking::Commands);
        assert_eq!(Control::Close.run(&asking), ran("prompt.clear", none));
        for (toward, flipped, named) in [
            (Place::Workspace, false, "drawer"),
            (Place::Workspace, true, "workspace"),
            (Place::Drawer, false, "workspace"),
            (Place::Drawer, true, "drawer"),
        ] {
            let mut turned = shell.clone();
            turned.drawer.toward = toward;
            turned.drawer.flipped = flipped;
            assert_eq!(
                Control::Toward.run(&turned),
                ran(
                    "drawer.toward",
                    Value::object([("region", Value::text(named))])
                ),
                "{toward:?} {flipped}"
            );
        }
        for control in Control::ALL {
            assert_eq!(Control::of(control.word()), Some(control));
            assert!(
                nc_catalog::command(control.word()).is_some(),
                "{}",
                control.word()
            );
            assert!(
                nc_catalog::command(&control.run(&asking).0).is_some()
                    && nc_catalog::command(&control.run(&shell).0).is_some()
            );
        }
        for other in ["drawer.expand", "page.close ", "", "drawer.back"] {
            assert_eq!(Control::of(other), None, "{other:?}");
        }
        assert_eq!(tab(&shell, "4"), Some(4));
        for other in ["7", "0", "", " 4", "4a", "-4", "+4", &"4".repeat(21)] {
            assert_eq!(tab(&shell, other), None, "{other:?}");
        }
        assert_eq!(identifier("0"), None, "no page is numbered 0");
        assert_eq!(identifier("18446744073709551615"), Some(u64::MAX));
        assert_eq!(identifier("18446744073709551616"), None);
        assert_eq!(
            focused(4, Place::Drawer, None),
            ran(
                "page.focus",
                Value::object([("page", Value::Integer(4)), ("in", Value::text("drawer"))])
            )
        );
    }
}
