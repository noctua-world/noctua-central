//! Every seat but the system's own, drawn as the design system draws a
//! second pointer and a second keyboard (`NOCTUA.md §12.18`, §9.7).
//!
//! **The system's own seat is the engine's**: seat `1`, the one pointer and
//! the one keyboard the system gives the window ([`SYSTEMS_OWN`]), whose
//! hover, focus and press the engine keeps as `:hover`, `:focus-visible` and
//! `:active`, and which the window says to the daemon as that seat's facts
//! (`crate::host`). Nothing here writes it.
//!
//! **Every other seat is told to the page** ([`told`], [`script`]): for each
//! seat the interface's state holds but the system's own, what its pointer
//! hovers, where its keyboard is and what it drags, which the page's script
//! writes onto the page as the design system's seat attributes through its
//! own `seats.js` — `data-nd-seat-hover` on the element and everything above
//! it, `data-nd-seat-focus` on one element, `data-nd-seat-drag` on one — and
//! which the design system's sheets style exactly as they style the native
//! pseudo-classes, rule for rule (§9.7). The page finds each:
//!
//! | The seat's | is drawn on |
//! |---|---|
//! | pointer's hover | the element its target names |
//! | keyboard | the element its selection names, inside the part of the window its keyboard is on; else that part's own — the prompt's text area, or the drawer's or the sidebar's list, or the first control of its rail — as the window puts the system's own keyboard there |
//! | drag | the element its target names |
//!
//! **A target is found by what it names, never by markup the state writes**:
//! the element whose `data-nd-action` — a control's command — or whose
//! `data-nd-value` — a list's option, a tab — is the target, compared as text,
//! so no word of the state becomes a selector; a target the page draws
//! nothing for draws that seat on nothing. A seat whose keyboard is in another
//! window is drawn in that one, not this; a seat the state no longer holds is
//! taken off the page whole; and since a draw that replaces an element drops
//! what was written on it, the seats are told again after every draw.
//!
//! **Decided here, where `NOCTUA.md` is silent**: the system's own seat is
//! `1`, as the kernel's commands default to it; the parts of a window a seat's
//! keyboard is drawn on are the three the window puts the system's own
//! keyboard on — the prompt line, the drawer and the sidebar — and a seat
//! whose keyboard is on the workspace area, the info line or the modeline is
//! drawn with no focus until those parts hold something a keyboard is on; and
//! no seat is drawn pressing, since the state holds no press.
//!
//! **What the design system does not draw** (reported to it, never patched
//! here): a list marks the option the system's own keyboard is on with
//! `data-nd-active`, which is no pseudo-class and has no seat twin, so a
//! second seat's selection on an option is drawn as that seat's focus on the
//! option, which the design system's sheets style on the list around it and
//! not on the option.

use nc_canon::Value;

use crate::mirror::{Region, Shell};

/// The system's own seat: the one the engine keeps the native states of, and
/// the one a command acts for when it names none.
pub const SYSTEMS_OWN: u64 = 1;

/// The script that tells the page no seat: what [`script`] writes of none,
/// and what a page holds before it is told any.
pub const NONE: &str = "window.noctua.seats([]);";

/// What the page is told of one seat that is not the system's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Told {
    /// The seat's number.
    pub seat: u64,
    /// What its pointer hovers.
    pub hover: Option<String>,
    /// What its keyboard selection is on.
    pub selection: Option<String>,
    /// The part of this window its keyboard is on, where the window draws a
    /// keyboard there: `prompt`, `drawer` or `sidebar`.
    pub part: Option<&'static str>,
    /// What it drags.
    pub drag: Option<String>,
}

/// The word of the part of a window `region` is, where a keyboard on it is
/// drawn: the three parts the window puts the system's own keyboard on.
#[must_use]
pub const fn part_of(region: Region) -> Option<&'static str> {
    match region {
        Region::Prompt => Some("prompt"),
        Region::Drawer => Some("drawer"),
        Region::Sidebar => Some("sidebar"),
        Region::Workspace | Region::Info | Region::Modeline => None,
    }
}

/// Every seat of `shell` but the system's own that is drawn in the window
/// `window`: each whose keyboard is on this window or on none, in the order
/// of their numbers, with the part of this window its keyboard is on.
#[must_use]
pub fn told(shell: &Shell, window: u64) -> Vec<Told> {
    shell
        .seats
        .iter()
        .filter(|(id, seat)| {
            **id != SYSTEMS_OWN && seat.focus.is_none_or(|focus| focus.window == window)
        })
        .map(|(id, seat)| Told {
            seat: *id,
            hover: seat.hover.clone(),
            selection: seat.selection.clone(),
            part: seat.focus.and_then(|focus| part_of(focus.region)),
            drag: seat.drag.clone(),
        })
        .collect()
}

/// `word` as a value of what the page is told, or null.
fn word_or_null(word: Option<&str>) -> Value {
    word.map_or(Value::Null, Value::text)
}

/// The script that tells the page every seat in `told`, and takes off it
/// every seat it drew that `told` does not hold: `window.noctua.seats`, with
/// each seat's number as the word `seats.js` writes it.
#[must_use]
pub fn script(told: &[Told]) -> String {
    let seats = told
        .iter()
        .map(|seat| {
            Value::object([
                ("seat", Value::text(seat.seat.to_string())),
                ("hover", word_or_null(seat.hover.as_deref())),
                ("selection", word_or_null(seat.selection.as_deref())),
                ("part", word_or_null(seat.part)),
                ("drag", word_or_null(seat.drag.as_deref())),
            ])
        })
        .collect();
    format!(
        "window.noctua.seats({});",
        Value::Array(seats).to_canonical()
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::BTreeMap;

    use super::{SYSTEMS_OWN, Told, part_of, script, told};
    use crate::mirror::{By, Drawer, Focus, Place, Region, Seat, Shell};

    /// A state of two windows and no seat.
    fn two_windows() -> Shell {
        Shell {
            windows: Vec::new(),
            pages: BTreeMap::new(),
            drawer: Drawer {
                pages: Vec::new(),
                focused: None,
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
            next: 3,
        }
    }

    /// A seat whose keyboard is on `region` of `window`, where it is on one.
    fn seat(focus: Option<(u64, Region)>, hover: Option<&str>, selection: Option<&str>) -> Seat {
        Seat {
            latest: By::Pointer,
            focus: focus.map(|(window, region)| Focus { window, region }),
            hover: hover.map(str::to_owned),
            selection: selection.map(str::to_owned),
            drag: None,
        }
    }

    /// The page is told every seat but the system's own, by number, each with
    /// its hover, its selection, the part its keyboard is on and its drag;
    /// one whose keyboard is in another window is drawn there and not here,
    /// and one whose keyboard is on no window is drawn here.
    #[test]
    pub(crate) fn every_seat_but_the_systems_own_is_told_to_the_page() {
        let mut shell = two_windows();
        shell.seats.insert(
            SYSTEMS_OWN,
            seat(Some((1, Region::Prompt)), Some("info.expand"), None),
        );
        shell.seats.insert(
            2,
            Seat {
                drag: Some("x".to_owned()),
                ..seat(Some((1, Region::Drawer)), Some("drawer.expand"), Some("4"))
            },
        );
        shell.seats.insert(
            3,
            seat(Some((2, Region::Prompt)), Some("info.expand"), None),
        );
        shell
            .seats
            .insert(4, seat(None, None, Some("sidebar.show")));
        assert_eq!(
            told(&shell, 1),
            [
                Told {
                    seat: 2,
                    hover: Some("drawer.expand".to_owned()),
                    selection: Some("4".to_owned()),
                    part: Some("drawer"),
                    drag: Some("x".to_owned()),
                },
                Told {
                    seat: 4,
                    hover: None,
                    selection: Some("sidebar.show".to_owned()),
                    part: None,
                    drag: None,
                },
            ]
        );
        assert_eq!(
            told(&shell, 2)
                .iter()
                .map(|seat| seat.seat)
                .collect::<Vec<_>>(),
            [3, 4],
            "each seat is drawn in the window its keyboard is in"
        );
        shell.seats.clear();
        assert_eq!(told(&shell, 1), [], "no seat, nothing told");
        shell.seats.insert(
            SYSTEMS_OWN,
            seat(Some((1, Region::Drawer)), Some("drawer.expand"), Some("4")),
        );
        assert_eq!(told(&shell, 1), [], "the system's own seat is the engine's");
    }

    /// A keyboard is drawn on the prompt line, the drawer and the sidebar —
    /// the parts the window puts the system's own keyboard on — and on no
    /// other part.
    #[test]
    fn a_keyboard_is_drawn_on_the_three_parts_the_window_focuses() {
        for (region, part) in [
            (Region::Prompt, Some("prompt")),
            (Region::Drawer, Some("drawer")),
            (Region::Sidebar, Some("sidebar")),
            (Region::Workspace, None),
            (Region::Info, None),
            (Region::Modeline, None),
        ] {
            assert_eq!(part_of(region), part, "{region:?}");
        }
    }

    /// The page is told the seats as one call it reads, each seat's number a
    /// word and each of its parts a word or null; and no seat is told as an
    /// empty list, which takes every seat drawn off the page.
    #[test]
    pub(crate) fn the_seats_are_told_as_one_call() {
        assert_eq!(script(&[]), super::NONE);
        assert_eq!(super::NONE, "window.noctua.seats([]);");
        assert_eq!(
            script(&[
                Told {
                    seat: 2,
                    hover: Some("info.expand".to_owned()),
                    selection: None,
                    part: Some("prompt"),
                    drag: None,
                },
                Told {
                    seat: 12,
                    hover: None,
                    selection: Some("a\"b".to_owned()),
                    part: None,
                    drag: Some("4".to_owned()),
                },
            ]),
            "window.noctua.seats([\
             {\"drag\":null,\"hover\":\"info.expand\",\"part\":\"prompt\",\"seat\":\"2\",\
             \"selection\":null},\
             {\"drag\":\"4\",\"hover\":null,\"part\":null,\"seat\":\"12\",\
             \"selection\":\"a\\\"b\"}]);"
        );
    }
}
