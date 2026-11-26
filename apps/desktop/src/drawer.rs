//! What the drawer shows of the lists the keys open (`NOCTUA.md §12.9`,
//! §12.11): the leader menu, the command list and a chosen command's
//! parameters, and the `/` and `w` lists — each the design system's
//! `OptionList`, its rows `OptionItem`s and a row's matched characters
//! `MatchText`, in a `Drawer` below the footer.
//!
//! **What it shows is a function of the state** ([`view`]), as the footer's
//! is: the drawer's pages and the one in focus, what the prompt asks for and
//! holds, and a seat's keyboard selection, with the keys in force and the
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
//! **Not drawn yet**, and why: the drawer's rails, its tabs and what any
//! other page shows in it, which arrive with the drawer and the pages; a word
//! naming each list and the drawer in each locale, which arrives with the
//! interface's own texts — a list is named by the display name of the command
//! that opens it, and the drawer by the product's name; and the sentence an
//! empty list says, for the same reason.

use dioxus::prelude::*;
use nc_catalog::Locale;
use noctua_design::{Drawer, DrawerSide, OptionItem, OptionList, OptionListLayout};

use crate::commands::{Commands, offered};
use crate::footer::title_of;
use crate::keys::Keymap;
use crate::mirror::{Asking, Kind, Shell};

/// The drawer's id in the document, which a draw brings it by.
pub const ID: &str = "noctua-drawer";

/// The window's own actions, which the `w` list holds (§12.2, §12.9).
pub const ACTIONS: [&str; 4] = [
    "window.hide",
    "window.close",
    "window.move",
    "window.resize",
];

/// A list the drawer shows.
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
}

impl List {
    /// Every one.
    pub const ALL: [Self; 4] = [Self::Menu, Self::Commands, Self::Actions, Self::Shortcuts];

    /// Its name, which the page's events carry and the list's id is.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Menu => "leader",
            Self::Commands => "commands",
            Self::Actions => "actions",
            Self::Shortcuts => "shortcuts",
        }
    }

    /// The list named `word`.
    #[must_use]
    pub fn of(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|list| list.name() == word)
    }

    /// The command whose display name names it, until the interface's own
    /// texts exist.
    const fn named_by(self) -> &'static str {
        match self {
            Self::Menu => "leader.tap",
            Self::Commands => "kernel.find",
            Self::Actions => "page.actions",
            Self::Shortcuts => "page.shortcuts",
        }
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

/// What the drawer shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    /// Whether it is there at all: something open in it, or the prompt
    /// asking for its list.
    pub shown: bool,
    /// Whether it is collapsed.
    pub collapsed: bool,
    /// The list it shows, its name, and its rows.
    pub list: Option<(List, String, Vec<Row>)>,
}

impl View {
    /// The drawer of a document before the daemon has answered: not there.
    #[must_use]
    pub const fn hidden() -> Self {
        Self {
            shown: false,
            collapsed: false,
            list: None,
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
    View {
        shown: !shell.drawer.pages.is_empty() || shell.asking.is_some(),
        collapsed: shell.drawer.collapsed && shell.asking.is_none(),
        list: list.map(|list| (list, title_of(list.named_by(), locale), rows)),
    }
}

/// The drawer's markup for `view`: its region, whole.
#[must_use]
pub fn markup(view: &View) -> String {
    dioxus_ssr::render_element(rsx! {
        TheDrawer { view: view.clone() }
    })
}

/// The drawer, below the footer: a region holding the list it shows, or
/// nothing while nothing is open in it.
#[component]
pub fn TheDrawer(view: View) -> Element {
    let View {
        shown,
        collapsed,
        list,
    } = view;
    rsx! {
        div { id: ID,
            if shown {
                Drawer { name: ID, label: crate::footer::NAME, side: DrawerSide::Bottom, collapsed,
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
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::BTreeMap;

    use nc_catalog::Locale;
    use nc_schema::keys::{Binding, Chord, Leader, LeaderPart, Modifier};

    use super::{ACTIONS, List, markup, view};
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
    fn a_rows_name_is_coloured_per_word() {
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
}
