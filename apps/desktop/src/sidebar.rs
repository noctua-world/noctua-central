//! The sidebar (`NOCTUA.md §12.14`): the design system's `Drawer` at the
//! window's start side, beside the workspace area, holding the list of the
//! workspaces, the groups with their pages and the pages on their own — an
//! `OptionList`, each workspace an `OptionGroup` and each page an
//! `OptionItem`.
//!
//! **What it shows is a function of the state** ([`view`]), as the footer's
//! and the drawer's are: whether the sidebar is shown, the window's pages and
//! the one in front, and a seat's keyboard selection — so one state draws one
//! sidebar in every window. Shown, it slides open; hidden, it is collapsed,
//! keeping its place and nothing a reader or a keyboard can reach.
//!
//! **It opens and closes by the kernel's commands**: the modeline's first
//! glyph runs `sidebar.show` (§12.5), and Leader Space runs `sidebar.cycle`,
//! which shows it, puts the keyboard on it where it is shown and hides it
//! where the keyboard is already there — the window then puts the keyboard
//! where the state says. **Choosing an entry focuses it** ([`chosen`]):
//! `page.focus` on that page in the window's workspace area, dispatched as
//! the person, for a page the window's workspace area holds and no other.
//!
//! **Decided here, where `NOCTUA.md` is silent**: until Stage 17 there is one
//! workspace and no group, so the list is one group, named by the
//! workspace's number, holding the pages of the window's workspace area in
//! the order they were opened, the one in front marked as the list's answer;
//! a page is named and drawn as a tab of the drawer names and draws it
//! ([`crate::drawer::page_title`]); the drawer's pages are its tabs rail's
//! and not the sidebar's; the sidebar is named by the word the catalog files
//! every command of its own under, its first tag in the window's locale
//! (`crate::drawer::part_named`) — `sidebar`, `barra lateral` — and its list
//! by `sidebar.show`'s display name, the command that opens it, until the
//! interface's own texts exist in both locales; and the
//! keyboard walking the list is said as the seat's selection on the page's
//! number, which the info line shows as it was said until pages declare their
//! titles.
//!
//! **Not drawn yet**, and why: the prompt line filtering the list while the
//! sidebar has focus, which arrives with the pages that are lists (§12.7);
//! more than one workspace, and groups (Stage 17); and the sentence an empty
//! list says, which arrives with the interface's own texts.

use dioxus::prelude::*;
use nc_canon::Value;
use nc_catalog::Locale;
use noctua_design::{Drawer, DrawerSide, IconDrawing, OptionGroup, OptionItem, OptionList};

use crate::drawer::{List, focused, glyph_of, identifier, page_title, part_named};
use crate::footer::title_of;
use crate::mirror::{Place, Shell};

/// The sidebar's id in the document, which a draw brings it by.
pub const ID: &str = "noctua-sidebar";

/// One entry of the sidebar: a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The page's identifier, the entry's value.
    pub page: u64,
    /// What the page is called.
    pub label: String,
    /// Its kind's glyph.
    pub glyph: IconDrawing,
    /// Whether it is the page in front of its workspace area.
    pub front: bool,
    /// Whether the keyboard is on it.
    pub active: bool,
}

/// What the sidebar shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    /// Whether there is a state to draw it from: none before the daemon has
    /// answered, and no sidebar then.
    pub drawn: bool,
    /// Whether it is shown.
    pub shown: bool,
    /// Its name.
    pub label: String,
    /// Its list's name.
    pub list: String,
    /// The workspaces, each its number and its pages.
    pub workspaces: Vec<(u32, Vec<Entry>)>,
}

impl View {
    /// The sidebar of a document before the daemon has answered: not there.
    #[must_use]
    pub const fn hidden() -> Self {
        Self {
            drawn: false,
            shown: false,
            label: String::new(),
            list: String::new(),
            workspaces: Vec::new(),
        }
    }
}

/// What the sidebar shows of `shell`, for the window numbered `window`, with
/// seat 1's keyboard on `selected`, in `locale`.
#[must_use]
pub fn view(
    shell: Option<&Shell>,
    window: Option<u64>,
    selected: Option<&str>,
    locale: Locale,
) -> View {
    let Some(shell) = shell else {
        return View::hidden();
    };
    let held = window.and_then(|id| shell.windows.iter().find(|held| held.id == id));
    let entries: Vec<Entry> = held
        .map(|held| {
            held.pages
                .iter()
                .filter_map(|id| shell.pages.get(id))
                .map(|page| Entry {
                    page: page.id,
                    label: page_title(page, locale),
                    glyph: glyph_of(page.kind),
                    front: held.focused == Some(page.id),
                    active: selected.and_then(identifier) == Some(page.id),
                })
                .collect()
        })
        .unwrap_or_default();
    View {
        drawn: true,
        shown: shell.sidebar,
        label: part_named(List::Sidebar.named_by(), locale),
        list: title_of(List::Sidebar.named_by(), locale),
        workspaces: vec![(1, entries)],
    }
}

/// The page of `window`'s workspace area the entry `value` names, in `shell`:
/// one that area holds, and nothing for any other value, which the window
/// runs nothing for.
#[must_use]
pub fn chosen(shell: &Shell, window: u64, value: &str) -> Option<u64> {
    let page = identifier(value)?;
    shell
        .windows
        .iter()
        .any(|held| held.id == window && held.pages.contains(&page))
        .then_some(page)
}

/// What choosing the entry of `page` in `window` runs: `page.focus` on it in
/// the window's workspace area.
#[must_use]
pub fn chosen_run(page: u64, window: u64) -> (String, Value) {
    focused(page, Place::Workspace, Some(window))
}

/// The sidebar's markup for `view`: its region, whole.
#[must_use]
pub fn markup(view: &View) -> String {
    dioxus_ssr::render_element(rsx! {
        TheSidebar { view: view.clone() }
    })
}

/// The sidebar, at the start of the row the workspace area is in: a region
/// holding the list of what is open, collapsed while it is hidden.
///
/// **Its region is no box of its own** (`contents`), so the design system's
/// `Drawer` stands in the row beside the workspace area, at the side it names.
#[component]
pub fn TheSidebar(view: View) -> Element {
    let View {
        drawn,
        shown,
        label,
        list,
        workspaces,
    } = view;
    rsx! {
        div { id: ID, class: "contents",
            if drawn {
                Drawer { name: ID, label, side: DrawerSide::Start, collapsed: !shown,
                    OptionList { name: List::Sidebar.name(), label: list,
                        for (number, entries) in workspaces {
                            OptionGroup { label: number.to_string(),
                                for entry in entries {
                                    OptionItem {
                                        value: entry.page.to_string(),
                                        label: entry.label,
                                        icon: entry.glyph,
                                        selected: entry.front,
                                        active: entry.active,
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

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::BTreeMap;

    use nc_catalog::Locale;

    use super::{View, chosen, chosen_run, markup, view};
    use crate::drawer::{List, page_title};
    use crate::footer::title_of;
    use crate::mirror::{Drawer, Kind, Page, Place, Shell, Window};

    /// A page numbered `id` of `kind`.
    fn page(id: u64, kind: Kind, topic: Option<&str>) -> Page {
        Page {
            id,
            kind,
            topic: topic.map(str::to_owned),
            trail: Vec::new(),
            scroll: 0,
        }
    }

    /// A state whose window 1 holds settings, help on a topic and dev in its
    /// workspace area, help in front, the sidebar shown where `shown`, and
    /// the drawer holding the leader menu.
    fn with_pages(shown: bool) -> Shell {
        let mut pages = BTreeMap::new();
        for held in [
            page(2, Kind::Settings, None),
            page(3, Kind::Help, Some("keys")),
            page(4, Kind::Dev, None),
            page(5, Kind::Leader, None),
        ] {
            pages.insert(held.id, held);
        }
        Shell {
            windows: vec![Window {
                id: 1,
                shown: true,
                pages: vec![2, 3, 4],
                focused: Some(3),
                size: None,
                place: None,
            }],
            pages,
            drawer: Drawer {
                pages: vec![5],
                focused: Some(5),
                collapsed: false,
                toward: Place::Workspace,
                flipped: false,
                share: 33,
            },
            sidebar: shown,
            footer: true,
            expanded: false,
            message: None,
            draft: String::new(),
            asking: None,
            seats: BTreeMap::new(),
            next: 6,
        }
    }

    /// The sidebar lists the workspaces — the one there is until Stage 17,
    /// by its number — and in it the pages of the window's workspace area,
    /// in the order opened, each named and drawn by its kind, the one in
    /// front the list's answer and the keyboard's where the seat's selection
    /// names it; the drawer's pages are not listed.
    #[test]
    pub(crate) fn the_sidebar_lists_the_workspace_and_its_pages() {
        for locale in Locale::ALL {
            let shell = with_pages(true);
            let shown = view(Some(&shell), Some(1), Some("4"), *locale);
            assert!(shown.drawn && shown.shown);
            assert_eq!(shown.list, title_of("sidebar.show", *locale));
            assert_eq!(
                shown.label,
                nc_catalog::tags("command.sidebar.show.tags", *locale).expect("tags")[0],
                "the sidebar is named by the word its commands are filed under"
            );
            let [(number, entries)] = shown.workspaces.as_slice() else {
                panic!("one workspace: {shown:?}");
            };
            assert_eq!(*number, 1);
            assert_eq!(
                entries.iter().map(|entry| entry.page).collect::<Vec<_>>(),
                [2, 3, 4],
                "the window's pages in the order opened, and no page of the drawer"
            );
            let help = entries
                .iter()
                .find(|entry| entry.page == 3)
                .expect("listed");
            assert_eq!(
                help.label,
                page_title(shell.pages.get(&3).expect("held"), *locale)
            );
            assert!(help.label.ends_with(" keys"), "its topic: {}", help.label);
            assert_eq!(help.glyph, crate::drawer::glyph_of(Kind::Help));
            assert_eq!(
                entries
                    .iter()
                    .map(|entry| (entry.front, entry.active))
                    .collect::<Vec<_>>(),
                [(false, false), (true, false), (false, true)]
            );
            let written = markup(&shown);
            assert!(written.contains("data-nd-side=\"start\""), "{written}");
            assert!(written.contains("data-nd-collapsed=\"false\""), "{written}");
            assert!(
                written.contains("data-nd-option-list=\"sidebar\""),
                "{written}"
            );
            assert!(
                written.contains("role=\"group\" aria-label=\"1\""),
                "the workspace is a group named by its number: {written}"
            );
            assert_eq!(written.matches("role=\"option\"").count(), 3, "{written}");
            assert_eq!(
                written.matches("aria-selected=\"true\"").count(),
                1,
                "{written}"
            );
            assert!(
                written.contains("tabindex=\"0\""),
                "the list takes the keyboard: {written}"
            );
        }
    }

    /// Hidden, the sidebar is the design system's drawer collapsed, which
    /// keeps its place and lets nothing reach it; before the daemon has
    /// answered there is no sidebar; and its region is never a box of its
    /// own.
    #[test]
    pub(crate) fn a_hidden_sidebar_is_collapsed_and_none_is_drawn_before_the_state() {
        let hidden = view(Some(&with_pages(false)), Some(1), None, Locale::En);
        assert!(hidden.drawn && !hidden.shown);
        let written = markup(&hidden);
        assert!(written.contains("data-nd-collapsed=\"true\""), "{written}");
        let none = view(None, Some(1), None, Locale::En);
        assert_eq!(none, View::hidden());
        assert_eq!(
            markup(&none),
            "<div id=\"noctua-sidebar\" class=\"contents\"></div>"
        );
        let elsewhere = view(Some(&with_pages(true)), Some(9), None, Locale::En);
        assert_eq!(
            elsewhere.workspaces,
            [(1, Vec::new())],
            "a window the state does not hold lists nothing"
        );
    }

    /// Choosing an entry focuses its page in the window's workspace area, a
    /// page that area holds and no other; anything else the page might say
    /// is run as nothing.
    #[test]
    pub(crate) fn choosing_an_entry_focuses_its_page() {
        let shell = with_pages(true);
        assert_eq!(chosen(&shell, 1, "4"), Some(4));
        for other in ["5", "9", "0", "", "4 ", "+4", "page.focus", &"4".repeat(21)] {
            assert_eq!(chosen(&shell, 1, other), None, "{other:?}");
        }
        assert_eq!(chosen(&shell, 2, "4"), None, "another window's");
        let (command, arguments) = chosen_run(4, 1);
        assert_eq!(command, "page.focus");
        assert_eq!(
            arguments,
            nc_canon::Value::object([
                ("page", nc_canon::Value::Integer(4)),
                ("in", nc_canon::Value::text("workspace")),
                ("window", nc_canon::Value::Integer(1)),
            ])
        );
        assert_eq!(List::of("sidebar"), Some(List::Sidebar));
    }
}
