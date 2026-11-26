//! The footer (`NOCTUA.md §12.3`): its modeline, its info line and its
//! prompt line, each drawn from the interface's state the daemon holds, every
//! part of it a primitive of the design system's — `Footer`, `FooterLine`,
//! `FooterCorner`, `Prompt`, `Button`, `Icon` and `Text` — and nothing written
//! here but how they are composed.
//!
//! **What the footer shows is a function of the state** ([`view`]): the
//! window's state as [`crate::mirror`] holds it, what the window knows of
//! itself — its locale and its channel — and nothing else, so the same state
//! draws the same footer in every window and after every restart (§12.1). The
//! view is drawn as markup ([`markup`]), which the document holds when it is
//! built and the page is brought to whenever it changes ([`Drawing`]).
//!
//! | Line | What it shows | What it runs |
//! |---|---|---|
//! | modeline | the glyph that opens the sidebar, the focused page's glyph and title as the page declares them, then the channel where it is not production, the connection state and the drawer indicator (§12.5) | `sidebar.show`, `drawer.expand` |
//! | info line | the info glyph, then the message said on it, or what a seat's pointer or keyboard is on; expanded, the message and every hover, each on a line of its own (§12.6) | `info.expand`, `info.collapse` |
//! | prompt line | the design system's `Prompt` — its chevron, a real text area with a block cursor drawn by the design system's own script, no placeholder (§12.7) | `prompt.set`, said as a note |
//!
//! **Every control runs a command, named by its command's display name**
//! ([`Action`]): the control's `data-nd-action` is the command's name, the
//! page says it back when the control is activated, and the window runs it as
//! the person — so the CLI does all the footer does (§2.8). A control's name
//! is the display name the catalog gives its command in the window's locale,
//! and the prompt's is `prompt.set`'s, the command its typing is.
//!
//! **What is typed is said, not asked** (§12.1): each change of the prompt's
//! text is a draft the window says as a note, `prompt.set`'s arguments, held
//! to `shell_text_bytes` — a longer text is cut to it, and the page with it.
//! A draft the daemon's state holds is written into the page only when it is
//! not one this window said ([`Typed`]), so the person's own typing is never
//! written back over a later keystroke, and a draft set from elsewhere — the
//! CLI's `prompt.set`, a draft kept over a restart — is.
//!
//! **Decided here, where `NOCTUA.md` is silent**: the message said on the
//! info line takes the line while it is there, and a hover shows again once
//! it is cleared; of a seat's two hands the one that moved last is shown, or
//! the other where the last holds nothing; the line with one line's room shows
//! the first seat, in the state's order, that is on something; and a hover on
//! a registered command shows its display name and its line, any other
//! target as it was said.
//!
//! **Not drawn yet**, and why: the realm and the person, since no realm
//! exists before Stage 22 — the modeline shows the connection as offline,
//! the only state there is without one; the focused page's identity, since no
//! page declares one until pages are drawn; the prompt's send control, and
//! what typing `;`, `:` and `/` and sending do, which arrive with the command
//! list; and a word naming the connection, the channel and a seat in each
//! locale, which arrives with the interface's own texts — the channel is
//! shown by the word its manifest writes, and a seat by its number.

use dioxus::prelude::*;
use nc_canon::Value;
use nc_catalog::Locale;
use nc_channel::Channel;
use noctua_design::{
    Button, ButtonSize, ButtonTone, ButtonVariant, Footer, FooterCorner, FooterCornerSide,
    FooterLine, FooterLineKind, FooterVisibility, Icon, IconDrawing, IconSize, Prompt, Text,
    TextSize, TextTone, TextWeight,
};

use crate::mirror::{By, Seat, Shell};

/// The name the footer is given for a screen reader: the product's, which
/// no locale translates, until the interface's own texts arrive in both
/// locales.
pub const NAME: &str = crate::host::TITLE;

/// The prompt's name, as a script finds it: its root's `data-nd-prompt`, and
/// its text area's id after `nd-prompt-`.
pub const PROMPT: &str = "prompt";

/// What a control of the footer runs, when it is activated: a closed set,
/// each a command of the kernel's that takes no argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// The modeline's first glyph: `sidebar.show` (§12.5, §12.14).
    ShowSidebar,
    /// The drawer indicator: `drawer.expand` (§12.5, §12.11).
    ExpandDrawer,
    /// The info line's chevron, while the line is one line: `info.expand`.
    ExpandInfo,
    /// The info line's chevron, while it is expanded: `info.collapse`.
    CollapseInfo,
}

impl Action {
    /// Every one.
    pub const ALL: [Self; 4] = [
        Self::ShowSidebar,
        Self::ExpandDrawer,
        Self::ExpandInfo,
        Self::CollapseInfo,
    ];

    /// The command it runs, which is also the word its control is found by.
    #[must_use]
    pub const fn command(self) -> &'static str {
        match self {
            Self::ShowSidebar => "sidebar.show",
            Self::ExpandDrawer => "drawer.expand",
            Self::ExpandInfo => "info.expand",
            Self::CollapseInfo => "info.collapse",
        }
    }

    /// The action whose command is `word`; none for any other word.
    #[must_use]
    pub fn of(word: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|action| action.command() == word)
    }

    /// The arguments its command is run with: none.
    #[must_use]
    pub fn arguments(self) -> Value {
        Value::object(Vec::<(&str, Value)>::new())
    }
}

/// The display name the catalog gives `command` in `locale`; the command's
/// own name where the catalog holds no such command.
#[must_use]
pub fn title_of(command: &str, locale: Locale) -> String {
    nc_catalog::command(command)
        .and_then(|entry| nc_catalog::message(entry.title_key, locale))
        .unwrap_or(command)
        .to_owned()
}

/// What a page declares of itself for the modeline: its glyph and its title
/// (§12.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// Its glyph.
    pub glyph: IconDrawing,
    /// Its title, in the window's locale.
    pub title: String,
}

/// The drawer indicator: how many pages are open in the collapsed drawer,
/// and the name of the control that expands it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Indicator {
    /// How many pages are open in the drawer.
    pub count: usize,
    /// `drawer.expand`'s display name.
    pub label: String,
}

/// What a target a seat is on is, as the info line says it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum About {
    /// A registered command: its display name and its line, in the window's
    /// locale.
    Command {
        /// Its display name.
        title: String,
        /// Its line description.
        line: String,
    },
    /// Anything else, as it was said.
    Said(String),
}

/// What `target` is, in `locale`.
#[must_use]
pub fn about(target: &str, locale: Locale) -> About {
    nc_catalog::command(target)
        .and_then(|entry| {
            Some(About::Command {
                title: nc_catalog::message(entry.title_key, locale)?.to_owned(),
                line: nc_catalog::message(entry.description_key, locale)?.to_owned(),
            })
        })
        .unwrap_or_else(|| About::Said(target.to_owned()))
}

/// What one seat's pointer hovers or its keyboard selection is on, as a row
/// of the info line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hover {
    /// The seat's number, where the state holds more than one seat.
    pub seat: Option<u64>,
    /// Which of its hands: the pointer, or the keyboard.
    pub by: By,
    /// What it is on.
    pub about: About,
}

/// One row of the info line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// The message said on it.
    Message(String),
    /// A hover.
    Hover(Hover),
}

/// The info line (§12.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Info {
    /// Whether it is expanded.
    pub expanded: bool,
    /// What it shows, a row each: one at most while it is one line.
    pub rows: Vec<Row>,
    /// `info.expand`'s display name, the chevron's while the line is one
    /// line.
    pub expand: String,
    /// `info.collapse`'s display name, the chevron's while it is expanded.
    pub collapse: String,
}

/// What the footer shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    /// Whether the state shows the footer now.
    pub shown: bool,
    /// `sidebar.show`'s display name, the modeline's first glyph's.
    pub sidebar: String,
    /// The focused page's identity, where it declares one.
    pub page: Option<Identity>,
    /// The channel's word, where it is not production.
    pub channel: Option<&'static str>,
    /// The drawer indicator, while the drawer is collapsed with something
    /// open in it.
    pub drawer: Option<Indicator>,
    /// The info line.
    pub info: Info,
    /// `prompt.set`'s display name, the prompt's.
    pub prompt: String,
    /// The list the prompt filters, which it names in `aria-controls`: the
    /// command list's, while the drawer shows it.
    pub controls: Option<String>,
}

/// What the window knows of itself: its locale and its channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Context {
    /// The locale it draws in.
    pub locale: Locale,
    /// The channel of the install it is the interface of.
    pub channel: Channel,
}

/// The target `seat`'s hand that moved last is on, or the other's where that
/// one is on nothing.
fn latest(seat: &Seat) -> Option<(By, &str)> {
    let pointer = seat.hover.as_deref().map(|target| (By::Pointer, target));
    let keyboard = seat
        .selection
        .as_deref()
        .map(|target| (By::Keyboard, target));
    match seat.latest {
        By::Pointer => pointer.or(keyboard),
        By::Keyboard => keyboard.or(pointer),
    }
}

/// The info line's rows: while it is one line, the message, or else the
/// first seat's latest hover; expanded, the message and then every seat's
/// hovers, the pointer's before the keyboard's.
fn rows(shell: &Shell, locale: Locale) -> Vec<Row> {
    let several = shell.seats.len() > 1;
    let hover = |number: u64, by: By, target: &str| {
        Row::Hover(Hover {
            seat: several.then_some(number),
            by,
            about: about(target, locale),
        })
    };
    let mut rows: Vec<Row> = shell.message.iter().cloned().map(Row::Message).collect();
    if shell.expanded {
        for (number, seat) in &shell.seats {
            for (by, target) in [(By::Pointer, &seat.hover), (By::Keyboard, &seat.selection)] {
                if let Some(target) = target {
                    rows.push(hover(*number, by, target));
                }
            }
        }
    } else if rows.is_empty()
        && let Some((number, (by, target))) = shell
            .seats
            .iter()
            .find_map(|(number, seat)| latest(seat).map(|latest| (*number, latest)))
    {
        rows.push(hover(number, by, target));
    }
    rows
}

/// What the footer shows of `shell` — nothing of it before the daemon has
/// answered — in the window `context` describes.
#[must_use]
pub fn view(shell: Option<&Shell>, context: Context) -> View {
    let locale = context.locale;
    let drawer = shell
        .filter(|shell| shell.drawer.collapsed && !shell.drawer.pages.is_empty())
        .map(|shell| Indicator {
            count: shell.drawer.pages.len(),
            label: title_of(Action::ExpandDrawer.command(), locale),
        });
    View {
        shown: shell.is_none_or(|shell| shell.footer),
        sidebar: title_of(Action::ShowSidebar.command(), locale),
        page: None,
        channel: match context.channel {
            Channel::Production => None,
            other => Some(other.word()),
        },
        drawer,
        info: Info {
            expanded: shell.is_some_and(|shell| shell.expanded),
            rows: shell.map(|shell| rows(shell, locale)).unwrap_or_default(),
            expand: title_of(Action::ExpandInfo.command(), locale),
            collapse: title_of(Action::CollapseInfo.command(), locale),
        },
        prompt: title_of("prompt.set", locale),
        controls: None,
    }
}

/// The footer's markup for `view`: the `<footer>` element, whole.
#[must_use]
pub fn markup(view: &View) -> String {
    dioxus_ssr::render_element(rsx! {
        TheFooter { view: view.clone() }
    })
}

/// The glyph that tells a pointer's hover from a keyboard's selection.
const fn glyph_of(by: By) -> IconDrawing {
    match by {
        By::Pointer => IconDrawing::MOUSE_POINTER_2,
        By::Keyboard => IconDrawing::KEYBOARD,
    }
}

/// The footer, edge to edge at the bottom of the window: a corner at each
/// end and its three lines between them, top to bottom (`NOCTUA.md §12.3`).
#[component]
pub fn TheFooter(view: View) -> Element {
    let View {
        shown,
        sidebar,
        page,
        channel,
        drawer,
        info,
        prompt,
        controls,
    } = view;
    rsx! {
        Footer { label: NAME, visibility: FooterVisibility::Always, shown,
            FooterCorner { side: FooterCornerSide::Start }
            FooterLine { kind: FooterLineKind::Modeline,
                Button {
                    variant: ButtonVariant::Transparent,
                    tone: ButtonTone::Neutral,
                    size: ButtonSize::Small,
                    hide_label: true,
                    icon: IconDrawing::PANEL_LEFT,
                    label: sidebar,
                    action: Action::ShowSidebar.command(),
                }
                if let Some(page) = page {
                    Icon { icon: page.glyph, size: IconSize::Small }
                    Text { size: TextSize::Caption, class: "truncate", "{page.title}" }
                }
                div { class: "flex-1" }
                if let Some(channel) = channel {
                    Text { size: TextSize::Caption, tone: TextTone::Muted, "{channel}" }
                }
                Icon { icon: IconDrawing::CLOUD_OFF, size: IconSize::Small }
                if let Some(drawer) = drawer {
                    Button {
                        variant: ButtonVariant::Transparent,
                        tone: ButtonTone::Neutral,
                        size: ButtonSize::Small,
                        hide_label: true,
                        icon: IconDrawing::PANEL_BOTTOM_OPEN,
                        label: drawer.label,
                        action: Action::ExpandDrawer.command(),
                    }
                    Text { size: TextSize::Caption, numeric: true, "{drawer.count}" }
                }
            }
            InfoLine { info }
            FooterLine { kind: FooterLineKind::Prompt,
                Prompt { name: PROMPT, label: prompt, controls }
            }
            FooterCorner { side: FooterCornerSide::End }
        }
    }
}

/// The info line: its glyph first, then a row for each of what it shows,
/// and the chevron that expands it and collapses it again (§12.6).
#[component]
fn InfoLine(info: Info) -> Element {
    let Info {
        expanded,
        rows,
        expand,
        collapse,
    } = info;
    let action = if expanded {
        Action::CollapseInfo
    } else {
        Action::ExpandInfo
    };
    let first = rows.first().cloned();
    let rest = rows.into_iter().skip(1);
    rsx! {
        FooterLine {
            kind: FooterLineKind::Info,
            expandable: true,
            expanded,
            expand_label: expand,
            collapse_label: collapse,
            action: action.command(),
            live: true,
            div { class: "flex-1 overflow-hidden",
                div {
                    Icon { icon: IconDrawing::INFO, size: IconSize::Small }
                    if let Some(row) = first {
                        " "
                        Said { row }
                    }
                }
                for row in rest {
                    div {
                        Said { row }
                    }
                }
            }
        }
    }
}

/// One row of the info line, as it is said: a message, or a hover — its
/// seat's number where there are several, its hand's glyph, and what it is on.
#[component]
fn Said(row: Row) -> Element {
    match row {
        Row::Message(message) => rsx! {
            Text { size: TextSize::Caption, "{message}" }
        },
        Row::Hover(Hover { seat, by, about }) => rsx! {
            if let Some(seat) = seat {
                Text { size: TextSize::Caption, tone: TextTone::Muted, numeric: true, "{seat}" }
                " "
            }
            Icon { icon: glyph_of(by), size: IconSize::Small }
            " "
            match about {
                About::Command { title, line } => rsx! {
                    Text { size: TextSize::Caption, weight: TextWeight::Medium, "{title}" }
                    " "
                    Text { size: TextSize::Caption, tone: TextTone::Muted, "{line}" }
                },
                About::Said(target) => rsx! {
                    Text { size: TextSize::Caption, "{target}" }
                },
            }
        },
    }
}

/// `text` cut to at most `bytes` bytes, at the last character boundary that
/// fits.
#[must_use]
pub fn within(text: &str, bytes: usize) -> &str {
    if text.len() <= bytes {
        return text;
    }
    let mut end = bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// The most a draft holds: the kernel's `shell_text_bytes`, which
/// `prompt.set` is held to.
fn draft_bound() -> usize {
    usize::try_from(nc_catalog::KERNEL.shell_text_bytes).unwrap_or(usize::MAX)
}

/// The most drafts this window remembers having said that the daemon's
/// state has not shown yet; past it the oldest is forgotten.
const UNSHOWN_MAX: usize = 64;

/// What the page's prompt holds, and the drafts this window said that the
/// daemon's state has not shown yet.
///
/// **The daemon's state lags what is typed**: a state answered while a note
/// was on its way holds an older draft, and a state another change moved
/// holds the draft it held. So a draft the state holds is written into the
/// page only when it changed since the state last said one, and is not one of
/// the drafts this window said — its own, older than what the page holds —
/// nor what the page holds already.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Typed {
    /// What the page's prompt holds.
    held: String,
    /// The draft the daemon's state held when it last said one.
    stated: String,
    /// The drafts this window said, oldest first, that the state has not
    /// shown.
    unshown: Vec<String>,
}

impl Typed {
    /// The page's prompt holds `text`, which the person typed: what this
    /// window says — `text`, cut to the bound `prompt.set` is held to — and
    /// what the page is to be written with, where that is less than it holds.
    pub fn typed(&mut self, text: &str) -> (String, Option<String>) {
        let said = within(text, draft_bound()).to_owned();
        if self.unshown.len() >= UNSHOWN_MAX {
            self.unshown.remove(0);
        }
        self.unshown.push(said.clone());
        self.held.clone_from(&said);
        let cut = (said.len() < text.len()).then(|| said.clone());
        (said, cut)
    }

    /// The daemon's state holds `draft`: what the page's prompt is to be
    /// written with, if anything.
    pub fn stated(&mut self, draft: &str) -> Option<String> {
        if draft == self.stated {
            return None;
        }
        draft.clone_into(&mut self.stated);
        if let Some(at) = self.unshown.iter().position(|said| said == draft) {
            self.unshown.drain(..=at);
            return None;
        }
        if draft == self.held {
            return None;
        }
        draft.clone_into(&mut self.held);
        self.unshown.clear();
        Some(draft.to_owned())
    }
}

/// `text` written as a script's string literal: JSON's, which a script reads
/// as the same text.
fn literal(text: &str) -> String {
    Value::text(text).to_canonical()
}

/// The script that brings the page's footer to `markup`.
#[must_use]
pub fn draw(markup: &str) -> String {
    format!("window.noctua.draw({});", literal(markup))
}

/// The script that writes `draft` into the page's prompt.
#[must_use]
pub fn draft(draft: &str) -> String {
    format!("window.noctua.draft({});", literal(draft))
}

/// What the page holds of the footer, and the scripts that bring it to what
/// the state shows: nothing is sent before the document has loaded, and
/// once it has, a footer only when its markup changed, and a draft only when
/// [`Typed`] says to write it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drawing {
    /// Whether the document said it loaded.
    loaded: bool,
    /// The footer's markup the page holds.
    drawn: String,
    /// The footer's markup the state shows.
    wanted: String,
    /// The drawer's markup the page holds.
    drawer_drawn: String,
    /// The drawer's markup the state shows.
    drawer_wanted: String,
    /// The sidebar's markup the page holds.
    sidebar_drawn: String,
    /// The sidebar's markup the state shows.
    sidebar_wanted: String,
    /// A draft to write into the page once it has loaded.
    waiting: Option<String>,
    /// What the page's prompt holds.
    typed: Typed,
}

impl Drawing {
    /// The page as its document is built: its footer `markup`.
    #[must_use]
    pub fn new(markup: String) -> Self {
        let drawer = crate::drawer::markup(&crate::drawer::View::hidden());
        let sidebar = crate::sidebar::markup(&crate::sidebar::View::hidden());
        Self {
            loaded: false,
            drawn: markup.clone(),
            wanted: markup,
            drawer_drawn: drawer.clone(),
            drawer_wanted: drawer,
            sidebar_drawn: sidebar.clone(),
            sidebar_wanted: sidebar,
            waiting: None,
            typed: Typed::default(),
        }
    }

    /// The footer's markup, and the scripts that bring the page to it.
    fn brought(&mut self) -> Vec<String> {
        let mut scripts = Vec::new();
        if !self.loaded {
            return scripts;
        }
        if self.wanted != self.drawn {
            self.drawn.clone_from(&self.wanted);
            scripts.push(draw(&self.drawn));
        }
        if self.drawer_wanted != self.drawer_drawn {
            self.drawer_drawn.clone_from(&self.drawer_wanted);
            scripts.push(draw(&self.drawer_drawn));
        }
        if self.sidebar_wanted != self.sidebar_drawn {
            self.sidebar_drawn.clone_from(&self.sidebar_wanted);
            scripts.push(draw(&self.sidebar_drawn));
        }
        if let Some(waiting) = self.waiting.take() {
            scripts.push(draft(&waiting));
        }
        scripts
    }

    /// The document said it loaded: the scripts that bring it to what the
    /// state shows.
    pub fn loaded(&mut self) -> Vec<String> {
        self.loaded = true;
        self.brought()
    }

    /// The state shows the footer `markup` and the prompt's `draft`: the
    /// scripts that bring the page to them.
    pub fn shows(&mut self, markup: String, draft: &str) -> Vec<String> {
        self.wanted = markup;
        if let Some(written) = self.typed.stated(draft) {
            self.waiting = Some(written);
        }
        self.brought()
    }

    /// The state shows the drawer `markup`: the scripts that bring the page
    /// to it.
    pub fn drawer(&mut self, markup: String) -> Vec<String> {
        self.drawer_wanted = markup;
        self.brought()
    }

    /// The state shows the sidebar `markup`: the scripts that bring the page
    /// to it.
    pub fn sidebar(&mut self, markup: String) -> Vec<String> {
        self.sidebar_wanted = markup;
        self.brought()
    }

    /// The window wrote `text` into the prompt — a parameter or a value
    /// completed: the draft this window says, as if typed, and the scripts
    /// that write it into the page.
    pub fn written(&mut self, text: &str) -> (String, Vec<String>) {
        let (said, _) = self.typed.typed(text);
        self.waiting = Some(said.clone());
        (said, self.brought())
    }

    /// The person typed `text` into the page's prompt: the draft this window
    /// says, and the scripts that bring the page to it.
    pub fn typed(&mut self, text: &str) -> (String, Vec<String>) {
        let (said, cut) = self.typed.typed(text);
        if cut.is_some() {
            self.waiting = cut;
        }
        (said, self.brought())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use nc_catalog::Locale;
    use nc_channel::Channel;

    use super::{
        About, Action, Context, Drawing, Hover, Identity, Info, Row, Typed, View, about, draft,
        draw, markup, title_of, view, within,
    };
    use crate::mirror::{By, Drawer, Place, Seat, Shell};

    /// The first run's state as the footer reads it: the drawer and the
    /// sidebar empty, the footer shown and nothing said.
    fn first_run() -> Shell {
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
            next: 2,
        }
    }

    /// A seat, its hands on `hover` and `selection`, the `latest` the one
    /// that moved last.
    fn seat(latest: By, hover: Option<&str>, selection: Option<&str>) -> Seat {
        Seat {
            latest,
            focus: None,
            hover: hover.map(str::to_owned),
            selection: selection.map(str::to_owned),
            drag: None,
        }
    }

    /// A development checkout's channel.
    fn development() -> Channel {
        Channel::Development(nc_channel::Checkout::parse("0123abcd").expect("a checkout"))
    }

    /// A window drawing in `locale`, of the production channel.
    fn production(locale: Locale) -> Context {
        Context {
            locale,
            channel: Channel::Production,
        }
    }

    /// What lies between `open` and the `close` that ends the element `open`
    /// begins, the first time it does, counting elements of its own name
    /// opened inside it.
    fn element<'a>(written: &'a str, open: &str) -> &'a str {
        let start = written
            .find(open)
            .unwrap_or_else(|| panic!("no `{open}` in {written}"));
        let name: String = open
            .trim_start_matches('<')
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect();
        let (opening, closing) = (format!("<{name}"), format!("</{name}>"));
        let mut depth = 0_usize;
        let mut at = start;
        loop {
            let next_open = written[at + 1..].find(&opening).map(|found| found + at + 1);
            let next_close = written[at..].find(&closing).map_or_else(
                || panic!("`{open}` is never closed in {written}"),
                |found| found + at,
            );
            match next_open {
                Some(opened) if opened < next_close => {
                    depth += 1;
                    at = opened;
                }
                _ => {
                    if depth == 0 {
                        return &written[start..next_close + closing.len()];
                    }
                    depth -= 1;
                    at = next_close + 1;
                }
            }
        }
    }

    /// The line of `kind` in `written`.
    fn line<'a>(written: &'a str, kind: &str) -> &'a str {
        element(
            written,
            &format!("<div class=\"nd-footer-line\" data-nd-kind=\"{kind}\""),
        )
    }

    /// The value of every `name="…"` in `written`, in order.
    fn values<'a>(written: &'a str, name: &str) -> Vec<&'a str> {
        written
            .split(&format!(" {name}=\""))
            .skip(1)
            .filter_map(|rest| rest.split_once('"').map(|(value, _)| value))
            .collect()
    }

    /// Every glyph `written` draws, by the design system's name for it.
    fn glyphs(written: &str) -> Vec<&str> {
        values(written, "data-nd-icon")
    }

    /// The footer `shell` shows in `context`, as markup.
    fn drawn(shell: &Shell, context: Context) -> String {
        markup(&view(Some(shell), context))
    }

    // -- §12.3 the footer ---------------------------------------------------

    /// Three lines, top to bottom — the modeline, the info line and the
    /// prompt line — between a corner at each end, in one footer named for a
    /// screen reader.
    #[test]
    fn the_footer_is_three_lines_between_two_corners() {
        for shell in [None, Some(&first_run())] {
            let written = markup(&view(shell, production(Locale::En)));
            assert!(written.starts_with("<footer"), "{written}");
            assert_eq!(written.matches("<footer").count(), 1, "{written}");
            assert_eq!(
                values(&written, "data-nd-kind"),
                ["modeline", "info", "prompt"],
                "{written}"
            );
            assert_eq!(
                values(&written, "data-nd-side"),
                ["start", "end"],
                "{written}"
            );
            let first_corner = written.find("data-nd-side=\"start\"").expect("a corner");
            let first_line = written.find("data-nd-kind=\"modeline\"").expect("a line");
            let last_line = written.find("data-nd-kind=\"prompt\"").expect("a line");
            let last_corner = written.find("data-nd-side=\"end\"").expect("a corner");
            assert!(
                first_corner < first_line && last_line < last_corner,
                "{written}"
            );
            assert!(
                written.contains("role=\"group\"")
                    && written.contains(&format!("aria-label=\"{}\"", super::NAME)),
                "{written}"
            );
        }
    }

    /// The corners do nothing yet: each is empty.
    #[test]
    fn the_corners_do_nothing_yet() {
        let written = drawn(&first_run(), production(Locale::En));
        for side in ["start", "end"] {
            let corner = element(
                &written,
                &format!("<div class=\"nd-footer-corner\" data-nd-side=\"{side}\""),
            );
            assert!(corner.ends_with("></div>"), "{corner}");
            assert_eq!(corner.matches('<').count(), 2, "{corner}");
        }
    }

    /// Until its setting exists the footer is always shown, and the state's
    /// own "shown now" is written for the folded footer the setting will
    /// choose.
    #[test]
    fn the_footer_is_always_shown_until_its_setting_exists() {
        let mut shell = first_run();
        for shown in [true, false] {
            shell.footer = shown;
            let written = drawn(&shell, production(Locale::En));
            assert_eq!(values(&written, "data-nd-visibility"), ["always"]);
            assert_eq!(
                values(&written, "data-nd-shown"),
                [if shown { "true" } else { "false" }]
            );
        }
        let before = markup(&view(None, production(Locale::En)));
        assert_eq!(values(&before, "data-nd-shown"), ["true"], "{before}");
    }

    /// Every glyph the footer draws is a design-system `Icon`, and nothing in
    /// it is given a colour, a size or a border of the window's own: no
    /// inline style, and no class but the design system's and the layout
    /// utilities the sheet holds.
    #[test]
    fn every_glyph_is_an_icon_and_every_shade_a_token() {
        let mut shell = everything();
        shell.expanded = true;
        let written = drawn(&shell, production(Locale::PtBR));
        assert!(written.matches("<svg").count() >= 6, "{written}");
        assert_eq!(
            written.matches("<svg").count(),
            written.matches("<svg class=\"nd-icon-glyph").count(),
            "{written}"
        );
        assert_eq!(written.matches("<svg").count(), glyphs(&written).len());
        assert!(!written.contains("style="), "{written}");
        for classes in values(&written, "class") {
            for class in classes.split_whitespace() {
                assert!(
                    class.starts_with("nd-")
                        || crate::sheet::SHEET.contains(&format!(".{class}{{")),
                    "`{class}` is neither the design system's nor a utility of the sheet"
                );
            }
        }
    }

    // -- §12.5 the modeline -------------------------------------------------

    /// A state holding everything the footer shows: a message, the drawer
    /// collapsed with two pages, and two seats on something.
    fn everything() -> Shell {
        let mut shell = first_run();
        shell.message = Some("Saved.".to_owned());
        shell.drawer.collapsed = true;
        shell.drawer.pages = vec![3, 4];
        shell.seats.insert(
            1,
            seat(By::Keyboard, Some("drawer.expand"), Some("info.expand")),
        );
        shell
            .seats
            .insert(2, seat(By::Pointer, Some("an item"), None));
        shell
    }

    /// The modeline opens with the glyph that opens the sidebar — a control
    /// that runs `sidebar.show`, named by its display name in the window's
    /// locale — before anything the state holds, and with nothing held.
    #[test]
    fn the_modeline_opens_with_the_sidebars_glyph() {
        for locale in Locale::ALL {
            for shell in [None, Some(everything())] {
                let written = markup(&view(shell.as_ref(), production(*locale)));
                let modeline = line(&written, "modeline");
                let content = modeline
                    .split_once("<div class=\"nd-footer-line-content\">")
                    .map(|(_, content)| content)
                    .expect("its content");
                assert!(content.starts_with("<button"), "{content}");
                let first = element(content, "<button");
                assert_eq!(values(first, "data-nd-action"), ["sidebar.show"]);
                assert_eq!(values(first, "data-nd-icon"), ["panel-left"]);
                assert_eq!(
                    values(first, "aria-label"),
                    [title_of("sidebar.show", *locale).as_str()]
                );
            }
        }
        assert_eq!(title_of("sidebar.show", Locale::En), "Show the sidebar");
        assert_eq!(
            title_of("sidebar.show", Locale::PtBR),
            "Mostrar a barra lateral"
        );
    }

    /// The focused page follows the sidebar's glyph, its glyph and its title
    /// as it declares them, before anything at the line's end; a page that
    /// declares nothing draws nothing there.
    #[test]
    fn the_focused_page_follows_the_sidebars_glyph() {
        let mut declared = view(Some(&everything()), production(Locale::En));
        assert_eq!(declared.page, None, "no page declares an identity yet");
        let without = markup(&declared);
        declared.page = Some(Identity {
            glyph: noctua_design::IconDrawing::SETTINGS,
            title: "Footer & <prompt>".to_owned(),
        });
        let written = markup(&declared);
        let modeline = line(&written, "modeline");
        assert_eq!(
            glyphs(modeline),
            ["panel-left", "settings", "cloud-off", "panel-bottom-open"]
        );
        let title = modeline
            .find("Footer &#38; &#60;prompt&#62;")
            .expect("its title, escaped");
        let glyph = modeline
            .find("data-nd-icon=\"settings\"")
            .expect("its glyph");
        let spacer = modeline
            .find("<div class=\"flex-1\"")
            .expect("the line's middle");
        assert!(glyph < title && title < spacer, "{modeline}");
        assert_eq!(
            glyphs(line(&without, "modeline")),
            ["panel-left", "cloud-off", "panel-bottom-open"]
        );
    }

    /// The line's end, in order: no realm and no person, since none exists;
    /// the channel's word where it is not production; the connection state,
    /// offline; and the drawer indicator last, a glyph and the number of
    /// pages open in the collapsed drawer, the control `drawer.expand`.
    #[test]
    fn the_modelines_end_is_the_channel_the_connection_and_the_drawer() {
        let shell = everything();
        for (channel, word) in [
            (Channel::Production, None),
            (Channel::Staging, Some("staging")),
            (development(), Some("development")),
        ] {
            let written = drawn(
                &shell,
                Context {
                    locale: Locale::En,
                    channel,
                },
            );
            let modeline = line(&written, "modeline");
            let end = modeline
                .split_once("<div class=\"flex-1\"></div>")
                .map(|(_, end)| end)
                .expect("the line's end");
            let connection = end.find("data-nd-icon=\"cloud-off\"").expect("offline");
            let drawer = end
                .find("data-nd-action=\"drawer.expand\"")
                .expect("the drawer");
            let count = end.find(">2</span>").expect("its count");
            assert!(connection < drawer && drawer < count, "{end}");
            assert_eq!(glyphs(end), ["cloud-off", "panel-bottom-open"], "{end}");
            assert_eq!(
                end.matches("<span").count(),
                1 + usize::from(word.is_some()),
                "the count, and the channel where it is not production: {end}"
            );
            match word {
                None => assert!(!end.contains("staging") && !end.contains("development")),
                Some(word) => {
                    let at = end.find(&format!(">{word}</span>")).expect("the channel");
                    assert!(at < connection, "{end}");
                }
            }
            assert_eq!(
                values(end, "aria-label"),
                [title_of("drawer.expand", Locale::En).as_str()]
            );
            assert!(end.trim_end().ends_with("</span></div></div>"), "{end}");
        }
    }

    /// The drawer indicator is shown only while the drawer is collapsed with
    /// something open in it.
    #[test]
    fn the_drawer_indicator_shows_only_while_the_drawer_is_collapsed_with_pages() {
        for (collapsed, pages, shown) in [
            (true, vec![3], true),
            (true, vec![], false),
            (false, vec![3, 4], false),
            (false, vec![], false),
        ] {
            let mut shell = first_run();
            shell.drawer.collapsed = collapsed;
            shell.drawer.pages.clone_from(&pages);
            let built = view(Some(&shell), production(Locale::En));
            assert_eq!(
                built.drawer.as_ref().map(|indicator| indicator.count),
                shown.then_some(pages.len()),
                "{collapsed} {pages:?}"
            );
            let written = markup(&built);
            assert_eq!(
                written.contains("data-nd-action=\"drawer.expand\""),
                shown,
                "{collapsed} {pages:?}"
            );
        }
        assert_eq!(view(None, production(Locale::En)).drawer, None);
    }

    // -- §12.6 the info line ------------------------------------------------

    /// The info line always opens with the info glyph: with nothing to say,
    /// with a message, and with a hover.
    #[test]
    fn the_info_line_opens_with_the_info_glyph() {
        let mut said = first_run();
        said.message = Some("Saved.".to_owned());
        let mut hovered = first_run();
        hovered.seats.insert(1, seat(By::Pointer, Some("x"), None));
        for shell in [None, Some(first_run()), Some(said), Some(hovered)] {
            let written = markup(&view(shell.as_ref(), production(Locale::En)));
            let info = line(&written, "info");
            assert_eq!(glyphs(info).first(), Some(&"info"), "{info}");
            let content = info
                .split_once("<div class=\"nd-footer-line-content\" role=\"status\">")
                .map(|(_, content)| content)
                .expect("a live content");
            assert!(
                content.starts_with("<div class=\"flex-1 overflow-hidden\"><div><svg"),
                "{content}"
            );
        }
    }

    /// What the info line shows: the message said on it — over any hover
    /// while it is one line — and otherwise what a seat's pointer hovers or
    /// its keyboard selection is on, a registered command by its display name
    /// and its line in the window's locale, anything else as it was said.
    #[test]
    fn the_info_line_shows_the_message_or_what_a_seat_is_on() {
        let mut shell = first_run();
        shell.message = Some("Saved <now>.".to_owned());
        shell
            .seats
            .insert(1, seat(By::Pointer, Some("drawer.expand"), None));
        let built = view(Some(&shell), production(Locale::PtBR));
        assert_eq!(built.info.rows, [Row::Message("Saved <now>.".to_owned())]);
        let written = markup(&built);
        let info = line(&written, "info");
        assert!(
            info.contains("Saved &#60;now&#62;.") && !info.contains("<now>"),
            "{info}"
        );
        shell.message = None;
        for (locale, title) in [
            (Locale::En, "Expand the drawer"),
            (Locale::PtBR, "Expandir a gaveta"),
        ] {
            let built = view(Some(&shell), production(locale));
            let Some(Row::Hover(hover)) = built.info.rows.first() else {
                panic!("a hover: {:?}", built.info.rows);
            };
            assert_eq!(hover.about, about("drawer.expand", locale));
            let About::Command {
                title: said,
                line: described,
            } = &hover.about
            else {
                panic!("a command: {:?}", hover.about);
            };
            assert_eq!(said, title);
            let written = markup(&built);
            let info = line(&written, "info");
            assert!(
                info.contains(said.as_str()) && info.contains(described.as_str()),
                "{info}"
            );
        }
        assert_eq!(
            about("not a command", Locale::En),
            About::Said("not a command".to_owned())
        );
    }

    /// A pointer's hover and a keyboard's selection are told apart by glyph,
    /// and the line shows the hand that moved last — the other where the last
    /// is on nothing.
    #[test]
    fn the_info_line_shows_the_latest_hand_by_its_glyph() {
        for (latest, hover, selection, by, glyph) in [
            (
                By::Pointer,
                Some("a"),
                Some("b"),
                By::Pointer,
                "mouse-pointer-2",
            ),
            (By::Keyboard, Some("a"), Some("b"), By::Keyboard, "keyboard"),
            (By::Pointer, None, Some("b"), By::Keyboard, "keyboard"),
            (
                By::Keyboard,
                Some("a"),
                None,
                By::Pointer,
                "mouse-pointer-2",
            ),
        ] {
            let mut shell = first_run();
            shell.seats.insert(1, seat(latest, hover, selection));
            let built = view(Some(&shell), production(Locale::En));
            let expected = if by == By::Pointer { hover } else { selection };
            assert_eq!(
                built.info.rows,
                [Row::Hover(Hover {
                    seat: None,
                    by,
                    about: About::Said(expected.expect("a target").to_owned()),
                })],
                "{latest:?} {hover:?} {selection:?}"
            );
            let written = markup(&built);
            assert_eq!(
                glyphs(line(&written, "info")),
                ["info", glyph, "chevron-up"]
            );
        }
        let mut idle = first_run();
        idle.seats.insert(1, seat(By::Pointer, None, None));
        idle.seats
            .insert(2, seat(By::Keyboard, None, Some("second")));
        let rows = view(Some(&idle), production(Locale::En)).info.rows;
        assert_eq!(
            rows,
            [Row::Hover(Hover {
                seat: Some(2),
                by: By::Keyboard,
                about: About::Said("second".to_owned()),
            })],
            "the first seat on something"
        );
    }

    /// One line by default: one row at most, whatever the state holds.
    #[test]
    fn the_info_line_is_one_line_by_default() {
        let built = view(Some(&everything()), production(Locale::En));
        assert!(!built.info.expanded);
        assert_eq!(built.info.rows.len(), 1, "{:?}", built.info.rows);
        let written = markup(&built);
        let info = line(&written, "info");
        assert_eq!(info.matches("<div>").count(), 1, "{info}");
        assert!(!info.contains("data-nd-expanded"), "{info}");
    }

    /// The chevron at the line's end runs `info.expand` while the line is one
    /// line and `info.collapse` once it is expanded, each named by its
    /// display name; expanded, the line shows the message and both hovers of
    /// every seat, each on a line of its own with its glyph.
    #[test]
    fn the_chevron_expands_the_info_line_to_every_hover() {
        for locale in Locale::ALL {
            let mut shell = everything();
            for expanded in [false, true] {
                shell.expanded = expanded;
                let written = drawn(&shell, production(*locale));
                let info = line(&written, "info");
                let toggle = element(info, "<button class=\"nd-footer-line-toggle\"");
                let (action, label) = if expanded {
                    ("info.collapse", title_of("info.collapse", *locale))
                } else {
                    ("info.expand", title_of("info.expand", *locale))
                };
                assert_eq!(values(toggle, "data-nd-action"), [action]);
                assert_eq!(values(toggle, "aria-label"), [label.as_str()]);
                assert_eq!(
                    values(toggle, "aria-expanded"),
                    [if expanded { "true" } else { "false" }]
                );
                assert_eq!(Action::of(action).map(Action::command), Some(action));
            }
            let built = view(Some(&shell), production(*locale));
            assert_eq!(built.info.rows.len(), 4, "{:?}", built.info.rows);
            let written = markup(&built);
            let info = line(&written, "info");
            assert_eq!(info.matches("<div>").count(), 4, "{info}");
            assert_eq!(
                glyphs(info),
                [
                    "info",
                    "mouse-pointer-2",
                    "keyboard",
                    "mouse-pointer-2",
                    "chevron-up"
                ]
            );
            assert!(info.contains("data-nd-expanded=\"true\""), "{info}");
        }
    }

    /// With several seats each hover row names its seat, by its number; with
    /// one it names none.
    #[test]
    fn each_hover_row_names_its_seat_where_there_are_several() {
        let mut shell = everything();
        shell.message = None;
        shell.expanded = true;
        let rows = view(Some(&shell), production(Locale::En)).info.rows;
        let seats: Vec<Option<u64>> = rows
            .iter()
            .map(|row| match row {
                Row::Hover(hover) => hover.seat,
                Row::Message(_) => None,
            })
            .collect();
        assert_eq!(seats, [Some(1), Some(1), Some(2)]);
        let written = drawn(&shell, production(Locale::En));
        let info = line(&written, "info");
        for number in ["1", "2"] {
            assert!(
                info.contains(&format!("data-nd-numeric=\"true\">{number}</span>")),
                "{info}"
            );
        }
        shell.seats.remove(&2);
        let rows = view(Some(&shell), production(Locale::En)).info.rows;
        assert!(
            rows.iter()
                .all(|row| matches!(row, Row::Hover(hover) if hover.seat.is_none())),
            "{rows:?}"
        );
    }

    // -- §12.7 the prompt line ----------------------------------------------

    /// The prompt line is the design system's `Prompt`: it opens with its
    /// chevron, is a real text area named by `prompt.set`'s display name, and
    /// shows no placeholder.
    #[test]
    fn the_prompt_opens_with_its_glyph_and_has_no_placeholder() {
        for locale in Locale::ALL {
            let written = drawn(&first_run(), production(*locale));
            let prompt = line(&written, "prompt");
            assert_eq!(glyphs(prompt), ["chevron-right"], "{prompt}");
            assert_eq!(values(prompt, "data-nd-prompt"), [super::PROMPT]);
            let area = element(prompt, "<textarea");
            assert_eq!(
                values(area, "aria-label"),
                [title_of("prompt.set", *locale).as_str()]
            );
            assert!(!prompt.contains("placeholder"), "{prompt}");
            assert!(
                prompt.find("chevron-right") < prompt.find("<textarea"),
                "{prompt}"
            );
        }
    }

    // -- every bullet, held ------------------------------------------------

    /// How a bullet of `NOCTUA.md` the interface answers to is held.
    #[derive(Debug, Clone, Copy)]
    enum Held {
        /// By this test.
        By(fn()),
        /// By this test, but for what is said, which arrives later.
        Partly(fn(), &'static str),
        /// By a test of another crate of this repository, or a suite of this
        /// crate's that starts a process: its file, from the repository's
        /// root, and its name.
        Elsewhere(&'static str, &'static str),
        /// By such a test, but for what is said, which arrives later.
        ElsewherePartly(&'static str, &'static str, &'static str),
        /// By the design system's own suite: the primitive's, as it says.
        Design(&'static str),
        /// Not yet: what builds it.
        Later(&'static str),
        /// By nothing, since it claims nothing a test could hold: why.
        Unclaimed(&'static str),
    }

    /// The sections of `NOCTUA.md §12` Stage 16 builds: every one but the
    /// workspaces' and the layout modes' (§12.12, §12.13), which are Stage
    /// 17's, and what a page can be and the terminals (§12.20, §12.21),
    /// which are later stages'.
    const SECTIONS: [&str; 17] = [
        "12.1", "12.2", "12.3", "12.4", "12.5", "12.6", "12.7", "12.8", "12.9", "12.10", "12.11",
        "12.14", "12.15", "12.16", "12.17", "12.18", "12.19",
    ];

    /// Every bullet of the sections of `NOCTUA.md §12` Stage 16 builds
    /// ([`SECTIONS`]), by its section and the first 32 characters of its text
    /// with its emphasis taken out, and how each is held.
    const BULLETS: [(&str, &str, Held); 99] = [
        (
            "12.1",
            "Closing the interface stops noth",
            Held::ElsewherePartly(
                "crates/nc-kernel/src/shell.rs",
                "a_window_opens_shown_and_closes_with_what_only_it_showed_and_the_last_hides",
                "the desktop killed while the daemon keeps what it showed, by a suite against a \
                 real daemon later in Stage 16 (acceptance criterion 2)",
            ),
        ),
        (
            "12.1",
            "Opening it again shows everythin",
            Held::ElsewherePartly(
                "crates/nc-kernel/tests/the_decision_point.rs",
                "the_interface_is_kept_outside_every_unit_and_read_back_when_the_kernel_opens_again",
                "the desktop opened again drawing exactly what it showed, by a suite against a \
                 real daemon later in Stage 16 (acceptance criterion 2)",
            ),
        ),
        (
            "12.1",
            "Several interface windows may at",
            Held::Partly(
                crate::host::tests::this_window_is_the_states_first_shown_as_it_says,
                "a second desktop attached to the same daemon and drawing the same state, by a \
                 suite against a real daemon later in Stage 16",
            ),
        ),
        (
            "12.1",
            "Every action the interface perfo",
            Held::Partly(
                every_act_of_the_window_is_a_command,
                "the person's answer to a call held for them, which is no command, with the \
                 confirmation surface later in Stage 16",
            ),
        ),
        (
            "12.2",
            "No title bar and no system butto",
            Held::Elsewhere(
                "apps/desktop/tests/the_window_opens_hidden.rs",
                "the_window_opens_hidden_paints_its_theme_first_and_closes",
            ),
        ),
        (
            "12.2",
            "The first time the app opens, it",
            Held::By(crate::page::tests::the_first_run_shows_the_footer_alone),
        ),
        (
            "12.2",
            "\"No buttons\" is a visual decisio",
            Held::Partly(
                the_window_hides_closes_moves_and_resizes_by_command,
                "moving and resizing by the system's own gestures, which no test drives, on the \
                 list kept by hand at the Stage's close",
            ),
        ),
        (
            "12.2",
            "Under a tiling window manager th",
            Held::Unclaimed(
                "how a compositor arranges an undecorated window is the system's, on the list kept \
                 by hand at the Stage's close",
            ),
        ),
        (
            "12.3",
            "Backgrounds. The modeline and th",
            Held::By(every_glyph_is_an_icon_and_every_shade_a_token),
        ),
        (
            "12.3",
            "The corners are squares as tall ",
            Held::By(the_corners_do_nothing_yet),
        ),
        (
            "12.3",
            "Visibility is a setting (§12.15)",
            Held::Partly(
                the_footer_is_always_shown_until_its_setting_exists,
                "the setting that folds the footer, with the settings pages",
            ),
        ),
        (
            "12.3",
            "Every glyph the interface draws ",
            Held::By(every_glyph_is_an_icon_and_every_shade_a_token),
        ),
        (
            "12.3",
            "The line height, the corners' si",
            Held::By(every_glyph_is_an_icon_and_every_shade_a_token),
        ),
        (
            "12.4",
            "The query is split into words at",
            Held::By(crate::drawer::tests::a_row_one_word_cannot_match_is_not_listed),
        ),
        (
            "12.4",
            "The text matched is the item's n",
            Held::By(crate::drawer::tests::a_word_found_in_either_language_lists_its_row),
        ),
        (
            "12.4",
            "Matched characters are coloured ",
            Held::By(crate::drawer::tests::a_rows_name_is_coloured_per_word),
        ),
        (
            "12.4",
            "the first word's characters take",
            Held::Partly(
                crate::drawer::tests::the_third_and_every_later_word_share_one_tone,
                "the shade of each word's mark is the design system's `MatchText`, from the colour \
                 system's match contexts",
            ),
        ),
        (
            "12.4",
            "the second word's take a softer ",
            Held::Partly(
                crate::drawer::tests::the_third_and_every_later_word_share_one_tone,
                "the shade of each word's mark is the design system's `MatchText`, from the colour \
                 system's match contexts",
            ),
        ),
        (
            "12.4",
            "the third and every later word's",
            Held::Partly(
                crate::drawer::tests::the_third_and_every_later_word_share_one_tone,
                "the shade of each word's mark is the design system's `MatchText`, from the colour \
                 system's match contexts",
            ),
        ),
        (
            "12.4",
            "The selected item keeps its matc",
            Held::Partly(
                crate::drawer::tests::the_selected_row_keeps_its_match_colours,
                "that the selected row's background leaves its marks legible is `OptionItem`'s and \
                 `MatchText`'s, in the design system's suite",
            ),
        ),
        (
            "12.4",
            "Ranking favours matches at word ",
            Held::Elsewhere(
                "crates/nc-fuzzy/src/align.rs",
                "a_run_at_a_word_start_outranks_a_run_inside_a_word",
            ),
        ),
        (
            "12.4",
            "Case and accents are folded one ",
            Held::Elsewhere(
                "crates/nc-fuzzy/src/fold.rs",
                "a_letter_folds_to_its_lower_case_base_letter",
            ),
        ),
        (
            "12.4",
            "Separators — every space, and `-",
            Held::Elsewhere(
                "crates/nc-fuzzy/src/query.rs",
                "separators_inside_a_word_are_dropped_and_a_word_of_separators_is_no_word",
            ),
        ),
        (
            "12.4",
            "Each word is matched on its own.",
            Held::Partly(
                crate::drawer::tests::two_words_claiming_one_character_draw_it_in_the_first_words_tone,
                "each word matched alone, and the words in another order changing only the \
                 colours, are the fuzzy core's: `two_words_may_claim_the_same_characters` in \
                 `crates/nc-fuzzy/src/rank.rs`",
            ),
        ),
        (
            "12.4",
            "Ranking: a word typed whole, the",
            Held::Elsewhere(
                "crates/nc-fuzzy/tests/the_corpus_is_the_rule.rs",
                "every_order_row_ranks_as_it_says",
            ),
        ),
        (
            "12.4",
            "Positions are character indices ",
            Held::By(crate::drawer::tests::a_position_is_a_character_and_a_cluster_is_marked_whole),
        ),
        (
            "12.4",
            "A query is read to `nc_fuzzy::QU",
            Held::Elsewhere(
                "crates/nc-fuzzy/src/query.rs",
                "a_query_longer_than_its_bound_is_refused",
            ),
        ),
        (
            "12.4",
            "The daemon filters through `kern",
            Held::Elsewhere(
                "crates/nc-kernel/src/find.rs",
                "an_english_query_finds_a_command_through_its_english_display_name",
            ),
        ),
        (
            "12.5",
            "At its start, a fixed glyph that",
            Held::By(the_modeline_opens_with_the_sidebars_glyph),
        ),
        (
            "12.5",
            "Then the focused page of the wor",
            Held::Partly(
                the_focused_page_follows_the_sidebars_glyph,
                "what each page declares, with the pages",
            ),
        ),
        (
            "12.5",
            "At its end, in this order, the l",
            Held::By(the_modelines_end_is_the_channel_the_connection_and_the_drawer),
        ),
        (
            "12.5",
            "the realm and the person acting,",
            Held::Partly(
                the_modelines_end_is_the_channel_the_connection_and_the_drawer,
                "the realm and the person, once realms exist (Stage 22)",
            ),
        ),
        (
            "12.5",
            "the channel, whenever it is not ",
            Held::By(the_modelines_end_is_the_channel_the_connection_and_the_drawer),
        ),
        (
            "12.5",
            "the connection state — online or",
            Held::Partly(
                the_modelines_end_is_the_channel_the_connection_and_the_drawer,
                "online, once a realm's server exists (Stage 22)",
            ),
        ),
        (
            "12.5",
            "the drawer indicator, last — a g",
            Held::By(the_drawer_indicator_shows_only_while_the_drawer_is_collapsed_with_pages),
        ),
        (
            "12.6",
            "What it shows:",
            Held::By(the_info_line_shows_the_message_or_what_a_seat_is_on),
        ),
        (
            "12.6",
            "information about what the point",
            Held::Partly(
                the_info_line_shows_the_message_or_what_a_seat_is_on,
                "the window saying its own hovers, with the seats",
            ),
        ),
        (
            "12.6",
            "information about what the keybo",
            Held::Partly(
                the_info_line_shows_the_latest_hand_by_its_glyph,
                "the window saying its own selection, with the seats",
            ),
        ),
        (
            "12.6",
            "momentary notifications.",
            Held::By(the_info_line_shows_the_message_or_what_a_seat_is_on),
        ),
        (
            "12.6",
            "Pointer and keyboard are told ap",
            Held::By(the_info_line_shows_the_latest_hand_by_its_glyph),
        ),
        (
            "12.6",
            "One line by default. Content lon",
            Held::By(the_info_line_is_one_line_by_default),
        ),
        (
            "12.6",
            "Hovering the line or giving it k",
            Held::By(the_chevron_expands_the_info_line_to_every_hover),
        ),
        (
            "12.6",
            "With several seats (§12.18), eac",
            Held::By(each_hover_row_names_its_seat_where_there_are_several),
        ),
        (
            "12.7",
            "It starts with a prompt glyph, a",
            Held::Partly(
                the_prompt_opens_with_its_glyph_and_has_no_placeholder,
                "the send control, with the command list and the interface's texts",
            ),
        ),
        (
            "12.7",
            "It grows as the text no longer f",
            Held::Design(
                "the prompt's growth to `--noctua-shell-prompt-max` and its scrollbar are \
                 `Prompt`'s and its script's, `prompt.js`",
            ),
        ),
        (
            "12.7",
            "A blinking block cursor, like a ",
            Held::Design(
                "the block, its blinking and `prefers-reduced-motion`, selection, composition, \
                 the clipboard and the text area's name are `Prompt`'s and its script's, \
                 `prompt.js`; that the window runs that script on its prompt is held by \
                 `--probe` in `apps/desktop/tests/the_window_opens_hidden.rs`",
            ),
        ),
        (
            "12.7",
            "What typing does depends on the ",
            Held::By(crate::drawer::tests::the_command_list_holds_what_the_query_finds_with_its_hits),
        ),
        (
            "12.7",
            "`;` or `:` in an empty prompt li",
            Held::Partly(
                crate::drawer::tests::the_command_list_holds_what_the_query_finds_with_its_hits,
                "the list opened in the drawer is the kernel's `prompt.set`, held in \
                 `crates/nc-kernel/src/shell.rs`",
            ),
        ),
        (
            "12.7",
            "`/` as the first character is re",
            Held::Partly(
                crate::drawer::tests::the_w_list_holds_the_windows_actions_and_nothing_else_is_listed,
                "nothing claims it: the kernel's `prompt.set` opens no list for it, and the \
                 drawer lists nothing the state does not open",
            ),
        ),
        (
            "12.7",
            "Any other text filters the focus",
            Held::Later("a focused page that is a list, with the pages"),
        ),
        (
            "12.7",
            "Send (the control, or Enter) act",
            Held::Partly(
                crate::host::tests::the_prompts_list_chooses_completes_and_runs,
                "the send control, and the info line's sentence for a prompt nothing handles, \
                 with the interface's texts",
            ),
        ),
        (
            "12.7",
            "The Leader focuses it (§12.8).",
            Held::Later(
                "the kernel's `leader.tap` puts the keyboard there; the window putting it where \
                 the state says is `host::Running::redraw`'s, exercised in a real window by no \
                 test yet — with the seats",
            ),
        ),
        (
            "12.8",
            "Wayland delivers a global key on",
            Held::Elsewhere(
                "crates/nc-ipc/src/leader.rs",
                "a_hotkey_is_registered_a_binding_asks_nothing_and_a_refusal_is_told",
            ),
        ),
        (
            "12.8",
            "X11, Windows and macOS offer a g",
            Held::Elsewhere(
                "crates/nc-platform/src/leader.rs",
                "the_delivery_is_read_from_the_system_and_the_session",
            ),
        ),
        (
            "12.8",
            "Neither the portal nor Windows' ",
            Held::By(crate::keys::tests::a_press_without_the_keyboard_is_a_tap_and_with_it_a_hold),
        ),
        (
            "12.8",
            "One interface, `nc_platform::lea",
            Held::Elsewhere(
                "crates/nc-platform/src/leader.rs",
                "a_registration_is_refused_where_the_session_delivers_none",
            ),
        ),
        (
            "12.8",
            "Each system is asked for a physi",
            Held::Elsewhere(
                "crates/nc-platform/src/leader.rs",
                "every_key_has_the_code_each_systems_list_gives_it",
            ),
        ),
        (
            "12.8",
            "X11 by this repository's own cli",
            Held::Elsewhere(
                "crates/nc-platform/src/leader.rs",
                "this_system_grants_the_chord_hears_it_and_gives_it_back",
            ),
        ),
        (
            "12.8",
            "Windows by `RegisterHotKey` with",
            Held::Elsewhere(
                "crates/nc-platform/src/leader.rs",
                "this_system_grants_the_chord_hears_it_and_gives_it_back",
            ),
        ),
        (
            "12.8",
            "macOS by Carbon's `RegisterEvent",
            Held::ElsewherePartly(
                "crates/nc-platform/src/leader.rs",
                "this_system_grants_the_chord_hears_it_and_gives_it_back",
                "a press of the chord on macOS, which no test may send there, on the list kept by \
                 hand at the Stage's close",
            ),
        ),
        (
            "12.8",
            "A chord another program holds is",
            Held::Elsewhere(
                "crates/nc-platform/src/leader.rs",
                "a_chord_another_program_holds_is_taken_on_each_system",
            ),
        ),
        (
            "12.8",
            "Not claimed: that macOS refuses ",
            Held::Unclaimed("it says what the Leader's delivery does not claim"),
        ),
        (
            "12.8",
            "GNOME and KDE bind Alt+Space by ",
            Held::Later("the installer, which frees the chord (Stage 19)"),
        ),
        (
            "12.8",
            "The staging and development Lead",
            Held::Later("those channels' installers, which free the chords (Stage 19)"),
        ),
        (
            "12.9",
            "It fills the drawer's width in c",
            Held::By(crate::drawer::tests::the_leader_menu_is_its_keys_and_names_in_columns),
        ),
        (
            "12.9",
            "Pressing an entry's key closes t",
            Held::Partly(
                an_entry_acts_and_the_direction_control_shows_where_it_opens,
                "the flip is the kernel's `leader.tap` and the direction kept the kernel's \
                 `drawer.toward`, held in `crates/nc-kernel/src/shell.rs`",
            ),
        ),
        (
            "12.9",
            "Every entry acts or opens its ta",
            Held::Partly(
                crate::keys::tests::the_menu_is_the_menu_tables_bindings_in_force,
                "the workspaces' Tab and digits and `w`'s page and group actions, with the \
                 workspaces (Stage 17)",
            ),
        ),
        (
            "12.9",
            "It lists every command — the cat",
            Held::By(crate::commands::tests::the_registrys_listings_make_one_command_of_each_locales_texts),
        ),
        (
            "12.9",
            "Each row shows the command's dis",
            Held::By(crate::drawer::tests::the_command_list_holds_what_the_query_finds_with_its_hits),
        ),
        (
            "12.9",
            "What follows the `;` filters it ",
            Held::Partly(
                crate::host::tests::the_prompts_list_chooses_completes_and_runs,
                "the arrows' walk is the design system's `option-list.js`; the send control \
                 arrives with the interface's texts",
            ),
        ),
        (
            "12.9",
            "Parameters are optional where th",
            Held::By(crate::host::tests::a_command_chosen_runs_or_asks_for_its_parameters),
        ),
        (
            "12.11",
            "Its height is capped by a token,",
            Held::Partly(
                crate::drawer::tests::a_controls_rail_holds_collapse_and_close_whenever_something_is_open,
                "the cap is `Drawer`'s `--noctua-shell-drawer-max`, which the window's layout \
                 lets act — held in a real webview by `--probe` in \
                 `apps/desktop/tests/the_window_opens_hidden.rs`; a drawer shorter than the \
                 workspace area in a window under 480 CSS pixels tall, and the share \
                 `drawer.resize` keeps, wait for the design system's `Drawer`",
            ),
        ),
        (
            "12.11",
            "Several pages can be open in it ",
            Held::By(crate::drawer::tests::a_tabs_rail_lists_the_pages_only_while_several_are_open),
        ),
        (
            "12.11",
            "Whenever something is open in it",
            Held::By(a_controls_rail_holds_its_two_and_the_pages_own),
        ),
        (
            "12.11",
            "collapse — hide the whole drawer",
            Held::By(crate::drawer::tests::every_control_and_tab_runs_its_command),
        ),
        (
            "12.11",
            "close — close the drawer page th",
            Held::By(crate::drawer::tests::every_control_and_tab_runs_its_command),
        ),
        (
            "12.11",
            "Both rails are exactly as wide a",
            Held::Design(
                "`DrawerRail` is `--noctua-shell-rail` wide, as `FooterCorner` is, and a rail's \
                 `Toolbar` and `Tabs` hold each control to `--noctua-target-min`",
            ),
        ),
        (
            "12.11",
            "Esc closes the drawer page that ",
            Held::Partly(
                crate::host::tests::the_prompts_list_chooses_completes_and_runs,
                "going back one sub-page is the kernel's `drawer.back`, held in \
                 `crates/nc-kernel/src/shell.rs`; the page's script saying Esc on the drawer \
                 is exercised in a real window by no test yet — with the seats",
            ),
        ),
        (
            "12.11",
            "Collapsing the drawer with somet",
            Held::By(a_collapsed_drawer_keeps_its_pages_and_the_modeline_counts_them),
        ),
        (
            "12.14",
            "It opens from the modeline's fir",
            Held::Partly(
                the_sidebar_opens_from_the_modeline_and_slides_shut,
                "show, focus and hide are the kernel's `sidebar.cycle`, held in \
                 `crates/nc-kernel/src/shell.rs`, and Leader Space the catalog's binding; the \
                 window putting the keyboard on the sidebar is exercised in a real window by \
                 no test yet — with the seats",
            ),
        ),
        (
            "12.14",
            "Choosing an entry focuses it; th",
            Held::Partly(
                crate::sidebar::tests::choosing_an_entry_focuses_its_page,
                "the prompt line filtering the list while the sidebar has focus, with the \
                 pages that are lists (§12.7)",
            ),
        ),
        (
            "12.15",
            "On the left, a list of categorie",
            Held::Later("the settings pages, later in Stage 16"),
        ),
        (
            "12.15",
            "Choosing a category lists, in a ",
            Held::Later("the settings pages, later in Stage 16"),
        ),
        (
            "12.15",
            "Each one opens a page for exactl",
            Held::Later("the settings pages, later in Stage 16"),
        ),
        (
            "12.16",
            "Live logs — every log and error ",
            Held::Later("the help and dev pages, later in Stage 16"),
        ),
        (
            "12.16",
            "The registries page — documentat",
            Held::Later("the help and dev pages, later in Stage 16"),
        ),
        (
            "12.16",
            "The minimum §15.3 sets: every er",
            Held::Later("the help and dev pages, later in Stage 16"),
        ),
        (
            "12.17",
            "It says that the local server is",
            Held::Later("the alert line, later in Stage 16"),
        ),
        (
            "12.17",
            "It offers restore now, which sta",
            Held::Later("the alert line, later in Stage 16"),
        ),
        (
            "12.17",
            "It shows a countdown; if the dae",
            Held::Later("the alert line, later in Stage 16"),
        ),
        (
            "12.17",
            "An explicit stop is respected. W",
            Held::Later("the alert line, later in Stage 16"),
        ),
        (
            "12.17",
            "When the daemon returns, the lin",
            Held::Later("the alert line, later in Stage 16"),
        ),
        (
            "12.18",
            "A seat is one pointer with one k",
            Held::ElsewherePartly(
                "crates/nc-kernel/src/shell.rs",
                "each_seat_keeps_its_own_focus_hover_selection_and_drag",
                "a second seat's focus, selection and drag drawn by the window, with the seats \
                 later in Stage 16",
            ),
        ),
        (
            "12.18",
            "The system's own seat uses the n",
            Held::Later(
                "the window writing a seat's attributes, with the seats later in Stage 16; their \
                 styling as the native pseudo-classes is the design system's `seats.js`",
            ),
        ),
        (
            "12.18",
            "How each system delivers several",
            Held::Elsewhere(
                "crates/nc-ipc/src/leader.rs",
                "a_press_is_a_tap_of_the_systems_own_seat",
            ),
        ),
        (
            "12.18",
            "tap, long-press and drag on a to",
            Held::Later("touch, with the seats later in Stage 16"),
        ),
        (
            "12.18",
            "two-finger scrolling;",
            Held::Later("touch, with the seats later in Stage 16"),
        ),
        (
            "12.18",
            "pinch to zoom in the infinite an",
            Held::Later("the infinite and 3D layout modes (Stage 17)"),
        ),
        (
            "12.18",
            "swipes to move along the scroll ",
            Held::Later("the scroll mode and the workspaces (Stage 17)"),
        ),
    ];

    /// An entry of the leader menu closes it first unless it opens a page,
    /// and the direction control shows where that page goes (§12.9).
    fn an_entry_acts_and_the_direction_control_shows_where_it_opens() {
        crate::host::tests::an_entry_closes_the_menu_first_unless_it_opens_a_page();
        crate::drawer::tests::the_leader_menus_direction_control_shows_where_the_next_page_goes();
    }

    /// The drawer's controls rail holds collapse and close, and beneath them
    /// the page in focus's own controls (§12.11).
    fn a_controls_rail_holds_its_two_and_the_pages_own() {
        crate::drawer::tests::a_controls_rail_holds_collapse_and_close_whenever_something_is_open();
        crate::drawer::tests::the_leader_menus_direction_control_shows_where_the_next_page_goes();
    }

    /// A collapsed drawer keeps what is open in it, and the modeline counts
    /// it (§12.11).
    fn a_collapsed_drawer_keeps_its_pages_and_the_modeline_counts_them() {
        crate::drawer::tests::a_collapsed_drawer_keeps_what_is_open_in_it();
        the_drawer_indicator_shows_only_while_the_drawer_is_collapsed_with_pages();
    }

    /// Every act of the window is a command: each control of the footer, of
    /// the drawer's rail and of a surface, and each word the page says, read
    /// only as one of the closed set it runs (§12.1).
    fn every_act_of_the_window_is_a_command() {
        every_action_is_a_command_without_arguments();
        crate::drawer::tests::every_control_and_tab_runs_its_command();
        crate::host::tests::only_the_closed_set_is_read_from_the_webview();
        crate::surface::tests::every_control_is_found_by_its_node_and_never_by_its_command();
    }

    /// The window hides, closes, moves and resizes by command: the `w` list
    /// holds its four actions, and a close the system asks for is
    /// `window.close` (§12.2).
    fn the_window_hides_closes_moves_and_resizes_by_command() {
        crate::drawer::tests::the_w_list_holds_the_windows_actions_and_nothing_else_is_listed();
        crate::host::tests::a_close_is_window_close_naming_the_window();
    }

    /// The modeline's first glyph opens the sidebar, which lists what is
    /// open, and a hidden sidebar is the design system's drawer collapsed
    /// (§12.14).
    fn the_sidebar_opens_from_the_modeline_and_slides_shut() {
        the_modeline_opens_with_the_sidebars_glyph();
        crate::sidebar::tests::the_sidebar_lists_the_workspace_and_its_pages();
        crate::sidebar::tests::a_hidden_sidebar_is_collapsed_and_none_is_drawn_before_the_state();
    }

    /// The bullets of the sections `BULLETS` answers to in `reference`, the
    /// text of `NOCTUA.md`, each by its section and its opening as `BULLETS`
    /// writes it.
    fn bullets_of(reference: &str) -> Vec<(String, String)> {
        let mut found = Vec::new();
        let mut section: Option<&str> = None;
        for text in reference.lines() {
            if let Some(heading) = text.strip_prefix("### ") {
                section = heading
                    .split(' ')
                    .next()
                    .filter(|number| SECTIONS.contains(number));
                continue;
            }
            if text.starts_with("## ") {
                section = None;
            }
            let Some(section) = section else { continue };
            let Some(bullet) = text
                .strip_prefix("- ")
                .or_else(|| text.strip_prefix("  - "))
            else {
                continue;
            };
            let opening: String = bullet.replace("**", "").chars().take(32).collect();
            found.push((section.to_owned(), opening));
        }
        found
    }

    /// The workspace this repository is in, where there is one: the
    /// directory holding the workspace's marker, with this repository inside
    /// its `repos/` (`NOCTUA.md §3.5`) — a marker anywhere else above it is
    /// somebody else's, and its documents are not this repository's.
    fn workspace() -> Option<std::path::PathBuf> {
        let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()?
            .parent()?;
        let repos = repository.parent()?;
        let workspace = repos.parent()?;
        (repos.file_name()? == "repos" && workspace.join(".noctua-workspace").is_file())
            .then(|| workspace.to_path_buf())
    }

    /// The repository this crate is in.
    fn repository() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("the repository is there")
    }

    /// Whether the file `file` of the repository declares the test `test`: a
    /// function of that name under a `#[test]` or a `#[tokio::test]`, among
    /// the attributes right above it.
    fn declares(file: &str, test: &str) -> bool {
        let Ok(source) = std::fs::read_to_string(repository().join(file)) else {
            return false;
        };
        let lines: Vec<&str> = source.lines().collect();
        lines.iter().enumerate().any(|(at, line)| {
            let declared = line
                .trim_start()
                .trim_start_matches("pub(crate) ")
                .trim_start_matches("async ")
                .starts_with(&format!("fn {test}("));
            declared
                && lines[..at]
                    .iter()
                    .rev()
                    .map(|above| above.trim())
                    .take_while(|above| {
                        !above.is_empty()
                            && !above.starts_with("//")
                            && !above.ends_with('}')
                            && !above.ends_with(';')
                    })
                    .any(|above| above == "#[test]" || above.starts_with("#[tokio::test"))
        })
    }

    /// Every bullet of the sections Stage 16 builds is a row of `BULLETS`,
    /// and every row a bullet, in order — read from `NOCTUA.md` where this
    /// repository is in its workspace, and said to be skipped where it is
    /// not; and each row says how it is held — a test of this crate, which
    /// passes, a test elsewhere in this repository, which its file declares,
    /// the design system's own, what builds it, or why nothing could.
    #[test]
    #[expect(
        clippy::print_stdout,
        reason = "a test that cannot read the reference says so on the harness's own output, \
                  as every skip of this tree's is said"
    )]
    fn every_bullet_of_the_interfaces_sections_is_held() {
        for (section, opening, held) in BULLETS {
            assert_eq!(opening.chars().count().min(32), opening.chars().count());
            assert!(SECTIONS.contains(&section), "{section} {opening}");
            match held {
                Held::By(test) => test(),
                Held::Partly(test, said) => {
                    test();
                    assert!(!said.trim().is_empty(), "{section} {opening}");
                }
                Held::Elsewhere(file, test) => {
                    assert!(
                        declares(file, test),
                        "{section} {opening}: `{file}` declares no test `{test}`"
                    );
                }
                Held::ElsewherePartly(file, test, said) => {
                    assert!(
                        declares(file, test),
                        "{section} {opening}: `{file}` declares no test `{test}`"
                    );
                    assert!(!said.trim().is_empty(), "{section} {opening}");
                }
                Held::Design(said) | Held::Later(said) | Held::Unclaimed(said) => {
                    assert!(!said.trim().is_empty(), "{section} {opening}");
                }
            }
        }
        let Some(workspace) = workspace() else {
            println!(
                "skipped: this repository is in no workspace's `repos/` (§3.5), so the bullets \
                 of `NOCTUA.md` were not read"
            );
            return;
        };
        let reference = std::fs::read_to_string(workspace.join("NOCTUA.md"))
            .expect("the workspace's NOCTUA.md is readable");
        let found = bullets_of(&reference);
        let rows: Vec<(String, String)> = BULLETS
            .iter()
            .map(|(section, opening, _)| ((*section).to_owned(), (*opening).to_owned()))
            .collect();
        assert_eq!(
            found, rows,
            "a bullet of a section Stage 16 builds changed: each is a row of `BULLETS`, held by \
             a test, or said to be the design system's, still to come or claiming nothing"
        );
    }

    // -- what the page is sent ----------------------------------------------

    /// Each action's command is a registered command that takes no argument,
    /// with a display name in every locale, and no other word is an action.
    #[test]
    fn every_action_is_a_command_without_arguments() {
        for action in Action::ALL {
            let entry = nc_catalog::command(action.command()).expect("registered");
            assert!(entry.arguments.is_empty(), "{}", action.command());
            assert_eq!(Action::of(action.command()), Some(action));
            assert_eq!(
                action.arguments(),
                nc_canon::Value::object(Vec::<(&str, _)>::new())
            );
            for locale in Locale::ALL {
                assert_ne!(title_of(action.command(), *locale), action.command());
            }
        }
        for word in [
            "",
            "sidebar",
            "sidebar.show ",
            "window.close",
            "Sidebar.show",
        ] {
            assert_eq!(Action::of(word), None, "{word:?}");
        }
        assert_eq!(title_of("not.a.command", Locale::En), "not.a.command");
    }

    /// A draft is cut to the bound at a character boundary, never inside one.
    #[test]
    fn a_draft_is_cut_at_a_character_boundary() {
        assert_eq!(within("abc", 3), "abc");
        assert_eq!(within("abcd", 3), "abc");
        assert_eq!(within("aé", 2), "a");
        assert_eq!(within("aé", 3), "aé");
        assert_eq!(within("", 0), "");
        let bound = usize::try_from(nc_catalog::KERNEL.shell_text_bytes).expect("small");
        let mut typed = Typed::default();
        let long = "é".repeat(bound);
        let (said, cut) = typed.typed(&long);
        assert_eq!(said.len(), bound);
        assert_eq!(cut.as_deref(), Some(said.as_str()));
        let (said, cut) = typed.typed("short");
        assert_eq!((said.as_str(), cut), ("short", None));
    }

    /// The state's draft is written into the page when it is someone else's,
    /// and never when it is one this window said, nor what the page holds,
    /// nor the draft the state held already.
    #[test]
    fn only_a_draft_this_window_did_not_say_is_written() {
        let mut typed = Typed::default();
        assert_eq!(typed.stated(""), None, "the first run's empty draft");
        assert_eq!(
            typed.stated("kept"),
            Some("kept".to_owned()),
            "a draft kept"
        );
        typed.typed("keptx");
        typed.typed("kept");
        typed.typed("kep");
        assert_eq!(typed.stated("keptx"), None, "an echo of an older draft");
        assert_eq!(typed.stated("keptx"), None, "the same state again");
        assert_eq!(typed.stated("kept"), None, "a later echo");
        assert_eq!(typed.stated("kep"), None, "the page holds it");
        assert_eq!(typed.stated("set"), Some("set".to_owned()), "the CLI's");
        assert_eq!(typed.stated("set"), None);
        assert_eq!(
            typed.stated("kept"),
            Some("kept".to_owned()),
            "older, but no echo"
        );
        typed.typed("a");
        typed.typed("");
        assert_eq!(typed.stated("other"), Some("other".to_owned()));
        assert_eq!(
            typed.stated(""),
            Some(String::new()),
            "cleared from elsewhere"
        );
        let mut typed = Typed::default();
        typed.typed("a");
        assert_eq!(
            typed.stated(""),
            None,
            "a state another change moved, still holding the draft it held"
        );
    }

    /// Past [`super::UNSHOWN_MAX`] drafts the state has not shown, the
    /// oldest is forgotten, and the state holding it is then written.
    #[test]
    fn the_oldest_unshown_draft_is_forgotten_past_the_bound() {
        let mut typed = Typed::default();
        for typed_now in 0..=super::UNSHOWN_MAX {
            typed.typed(&typed_now.to_string());
        }
        let mut forgotten = typed.clone();
        assert_eq!(forgotten.stated("0"), Some("0".to_owned()));
        assert_eq!(typed.stated("1"), None);
    }

    /// Nothing is sent before the document has loaded; once it has, the
    /// footer is sent only when its markup changed, and a draft only when it
    /// is to be written, each as the call the page's script answers.
    #[test]
    fn the_page_is_sent_only_what_changed_once_it_loaded() {
        let first = markup(&view(None, production(Locale::En)));
        let mut drawing = Drawing::new(first.clone());
        let shell = everything();
        let shown = drawn(&shell, production(Locale::En));
        assert_ne!(shown, first);
        assert_eq!(drawing.shows(shown.clone(), "kept"), Vec::<String>::new());
        assert_eq!(drawing.loaded(), [draw(&shown), draft("kept")]);
        assert_eq!(drawing.shows(shown.clone(), "kept"), Vec::<String>::new());
        assert_eq!(drawing.shows(first.clone(), "kept"), [draw(&first)]);
        let (said, sent) = drawing.typed("kepty");
        assert_eq!((said.as_str(), sent), ("kepty", Vec::new()));
        assert_eq!(drawing.shows(first.clone(), "kept"), Vec::<String>::new());
        assert_eq!(
            drawing.shows(first, "from elsewhere"),
            [draft("from elsewhere")]
        );
        assert_eq!(
            draw("<footer a=\"b\">\u{2028}</footer>"),
            "window.noctua.draw(\"<footer a=\\\"b\\\">\u{2028}</footer>\");"
        );
        assert_eq!(draft("a\nb"), "window.noctua.draft(\"a\\nb\");");
    }

    /// The view of no state: the footer shown, nothing said and nothing
    /// open, the controls named in the window's locale.
    #[test]
    fn before_the_daemon_answers_the_footer_holds_its_controls_alone() {
        let built = view(None, production(Locale::PtBR));
        assert_eq!(
            built,
            View {
                shown: true,
                sidebar: "Mostrar a barra lateral".to_owned(),
                page: None,
                channel: None,
                drawer: None,
                info: Info {
                    expanded: false,
                    rows: Vec::new(),
                    expand: title_of("info.expand", Locale::PtBR),
                    collapse: title_of("info.collapse", Locale::PtBR),
                },
                prompt: title_of("prompt.set", Locale::PtBR),
                controls: None,
            }
        );
    }
}
