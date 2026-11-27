//! The surfaces a plugin shows (`NOCTUA.md §11.6`, §12.20): the tree the
//! kernel kept for each, drawn by the design system's components and by
//! nothing of the window's own but layout utilities.
//!
//! **The kinds the kernel keeps, and nothing else** ([`Kind`]): a tree is
//! read strictly ([`View::read`]) — every node one of the closed set
//! `crates/nc-kernel/src/ui.rs` names, holding every key its kind must and no
//! key its kind does not, each of the shape the kernel judged it to, within
//! the kernel's count of nodes and depth — and a tree holding anything else
//! is refused whole, so the window never draws a kind the kernel does not
//! keep, nor a part of a tree it could not read. The set, and the words of
//! each closed choice, are the kernel's own table, which a test of this
//! module reads and holds this one to.
//!
//! | `node` | Drawn as |
//! |---|---|
//! | `column` | its children in a column, by layout utilities |
//! | `row` | its children in a row, wrapping where the row has no room, by layout utilities |
//! | `text` | `Paragraph`, quieter where its tone is `muted`, every line break kept |
//! | `heading` | `Heading`, at its level and that level's size |
//! | `button` | `Button`, disabled where the node says so |
//! | `toggle` | `Switch`, on where the node is checked |
//! | `input` | `Input`, or `NumberInput` for an integer and `Textarea` for several lines, with its value and its bound |
//! | `list` | `List`, an item for each of its nodes — or, holding none, its `empty` text, quieter |
//! | `image` | its `alt` text, quieter |
//! | `divider` | `Divider` |
//!
//! **Every text is the plugin's, and is drawn as text** (§11.6): what a
//! node says, and the surface's title, which names its region for a screen
//! reader, are escaped where they are rendered, so a text holding markup
//! shows that markup and is never read as any.
//!
//! **A control names its node, never what the node runs** ([`hook`]): a
//! `button` and a `toggle` are found by their surface and the JSON pointer
//! to their node in the tree, `<surface>#<pointer>` — the pointer a refusal
//! of the tree names the node by, `/` for its root — which no registered
//! command is spelled as, so the footer's and the drawer's closed sets read
//! it as nothing. What a node runs, and with which arguments, is read from
//! the tree the kernel kept, never from the window (`surfaces.activate`,
//! §11.6); the window keeps neither, and writes neither into the page.
//!
//! **Decided here, where `NOCTUA.md` is silent**: a heading's size is its
//! level's, so the levels read as an outline; an input's field is named
//! `<surface>-<name>` in the page, since the design system derives every id
//! of a field from its name, and two surfaces may name an input alike; a
//! toggle's two values are `true` and `false`, the words `checked` is
//! written in; and a surface of every kind — a panel, a document, an overlay
//! or a node placed in a 3D scene — draws its tree alike, since where a
//! surface is placed is the pages' and the layout modes'.
//!
//! **Not drawn yet**, and why: a column's or a row's `gap`, since the design
//! system's Tailwind bridge maps none of its spacing tokens, so every gap is
//! drawn as none; a `text`'s `danger` and `success` tones, since the design
//! system's `Paragraph` holds no such tone, so each is drawn in the default
//! one; an image's picture, since no blob reaches the webview, so its `alt`
//! stands in for it; and an integer input's bound, which `NumberInput` does
//! not take — the kernel holds what is typed to it. **Nor is a surface
//! drawn in the window yet**: asking the daemon which surfaces are open and
//! what each shows, and a control activated as `surfaces.activate`, arrive
//! with those commands later in Stage 16.

use std::collections::BTreeMap;

use dioxus::prelude::*;
use nc_canon::Value;
use nc_catalog::KERNEL;
use noctua_design::{
    Button, Divider, Heading, HeadingLevel, HeadingSize, Input, List, ListItem, NumberInput,
    Paragraph, ParagraphTone, Switch, Textarea,
};

/// Why a surface was not read: where in its tree, and what the window does
/// not read there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unread {
    /// Where, as a JSON pointer into the tree; empty for the surface itself.
    pub at: String,
    /// What is there.
    pub why: String,
}

impl core::fmt::Display for Unread {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.at.is_empty() {
            write!(formatter, "the surface is not read: it {}", self.why)
        } else {
            write!(
                formatter,
                "the surface's tree is not read at `{}`: it {}",
                self.at, self.why
            )
        }
    }
}

impl core::error::Error for Unread {}

/// `why`, said of `at`.
fn unread<T>(at: &str, why: impl Into<String>) -> Result<T, Unread> {
    Err(Unread {
        at: at.to_owned(),
        why: why.into(),
    })
}

/// A kind of node a tree may hold: the kernel's closed set (`KINDS` in
/// `crates/nc-kernel/src/ui.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Its children, in a column.
    Column,
    /// Its children, in a row.
    Row,
    /// A text.
    Text,
    /// A heading.
    Heading,
    /// A button that runs a command.
    Button,
    /// A switch that runs a command.
    Toggle,
    /// A field a person types into.
    Input,
    /// A list of nodes.
    List,
    /// A picture, by its blob.
    Image,
    /// A rule between two parts.
    Divider,
}

impl Kind {
    /// Every one, in the kernel's order.
    pub const ALL: [Self; 10] = [
        Self::Column,
        Self::Row,
        Self::Text,
        Self::Heading,
        Self::Button,
        Self::Toggle,
        Self::Input,
        Self::List,
        Self::Image,
        Self::Divider,
    ];

    /// Its word, the node's `node`.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Column => "column",
            Self::Row => "row",
            Self::Text => "text",
            Self::Heading => "heading",
            Self::Button => "button",
            Self::Toggle => "toggle",
            Self::Input => "input",
            Self::List => "list",
            Self::Image => "image",
            Self::Divider => "divider",
        }
    }

    /// The keys a node of it holds beside `node`, always.
    #[must_use]
    pub const fn holds(self) -> &'static [&'static str] {
        match self {
            Self::Column | Self::Row => &["children"],
            Self::Text => &["text"],
            Self::Heading => &["text", "level"],
            Self::Button => &["label", "command"],
            Self::Toggle => &["label", "checked", "command"],
            Self::Input => &["name", "label"],
            Self::List => &["items"],
            Self::Image => &["blob", "alt"],
            Self::Divider => &[],
        }
    }

    /// The keys a node of it may hold beside those.
    #[must_use]
    pub const fn may_hold(self) -> &'static [&'static str] {
        match self {
            Self::Column | Self::Row => &["gap"],
            Self::Text => &["tone"],
            Self::Button => &["args", "bind", "disabled"],
            Self::Toggle => &["args", "bind"],
            Self::Input => &["value", "kind", "max_length"],
            Self::List => &["empty"],
            Self::Heading | Self::Image | Self::Divider => &[],
        }
    }

    /// The kind whose word is `word`; none for any other.
    #[must_use]
    pub fn of(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.word() == word)
    }
}

/// A key whose value is one of a closed set of words (`CHOICES` in
/// `crates/nc-kernel/src/ui.rs`).
pub trait Choice: Copy + 'static {
    /// The key.
    const KEY: &'static str;
    /// Every word it may be, in the kernel's order, each beside what it is
    /// read as.
    const WORDS: &'static [(&'static str, Self)];
    /// The value a node that leaves the key out holds.
    const UNSAID: Self;
}

/// How far apart a column's or a row's children are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gap {
    /// Touching.
    None,
    /// A little.
    Small,
    /// More.
    Medium,
    /// Most.
    Large,
}

impl Choice for Gap {
    const KEY: &'static str = "gap";
    const WORDS: &'static [(&'static str, Self)] = &[
        ("none", Self::None),
        ("small", Self::Small),
        ("medium", Self::Medium),
        ("large", Self::Large),
    ];
    const UNSAID: Self = Self::None;
}

/// How a text reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// As any text.
    Normal,
    /// Quieter, for text that supports the text around it.
    Muted,
    /// Saying something went wrong.
    Danger,
    /// Saying something went well.
    Success,
}

impl Choice for Tone {
    const KEY: &'static str = "tone";
    const WORDS: &'static [(&'static str, Self)] = &[
        ("normal", Self::Normal),
        ("muted", Self::Muted),
        ("danger", Self::Danger),
        ("success", Self::Success),
    ];
    const UNSAID: Self = Self::Normal;
}

/// What an input is typed as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// One line of text.
    Text,
    /// A whole number.
    Integer,
    /// Several lines of text.
    Multiline,
}

impl Choice for Field {
    const KEY: &'static str = "kind";
    const WORDS: &'static [(&'static str, Self)] = &[
        ("text", Self::Text),
        ("integer", Self::Integer),
        ("multiline", Self::Multiline),
    ];
    const UNSAID: Self = Self::Text;
}

/// One node of a surface's tree, as the window draws it: what it shows, and
/// for a control the pointer it is found by — never the command it runs,
/// nor that command's arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// Its children, in a column.
    Column {
        /// How far apart they are.
        gap: Gap,
        /// The children.
        children: Vec<Self>,
    },
    /// Its children, in a row.
    Row {
        /// How far apart they are.
        gap: Gap,
        /// The children.
        children: Vec<Self>,
    },
    /// A text.
    Text {
        /// What it says.
        text: String,
        /// How it reads.
        tone: Tone,
    },
    /// A heading.
    Heading {
        /// What it says.
        text: String,
        /// Its level, from 1 to 3.
        level: u8,
    },
    /// A button.
    Button {
        /// What it says.
        label: String,
        /// Whether it is disabled.
        disabled: bool,
        /// The inputs whose text its command is given.
        binds: Vec<String>,
        /// The pointer to its node.
        at: String,
    },
    /// A switch.
    Toggle {
        /// What it switches.
        label: String,
        /// Whether it is on.
        checked: bool,
        /// The inputs whose text its command is given.
        binds: Vec<String>,
        /// The pointer to its node.
        at: String,
    },
    /// A field a person types into.
    Input {
        /// Its name in the tree, which a control binds.
        name: String,
        /// What it asks for.
        label: String,
        /// What it holds before anything is typed.
        value: Option<String>,
        /// What it is typed as.
        field: Field,
        /// The most characters it takes.
        max_length: Option<u64>,
    },
    /// A list of nodes.
    List {
        /// Its items.
        items: Vec<Self>,
        /// What it says when it holds none.
        empty: Option<String>,
    },
    /// A picture.
    Image {
        /// The BLAKE3 hash of the blob it shows.
        blob: String,
        /// What it shows, in words.
        alt: String,
    },
    /// A rule.
    Divider,
}

/// What one surface shows: its identifier, its title and the tree the kernel
/// kept for it, none until one is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    /// Its identifier, as the kernel minted it.
    pub surface: String,
    /// Its title.
    pub title: String,
    /// Its tree.
    pub tree: Option<Node>,
}

impl View {
    /// The surface `surface`, titled `title`, showing `tree`, the tree as the
    /// kernel kept it.
    ///
    /// # Errors
    ///
    /// An [`Unread`] for an identifier the kernel does not mint —
    /// `surface-` and its number — and for a tree that holds a node of a kind
    /// outside [`Kind`], a key its kind does not hold, a key it must hold
    /// left out, a value of another shape than the kernel judges it to, or
    /// more nodes or more depth than the kernel lets a tree hold.
    pub fn read(surface: &str, title: &str, tree: Option<&Value>) -> Result<Self, Unread> {
        let number = surface.strip_prefix("surface-").unwrap_or_default();
        if number.is_empty()
            || number.len() > 20
            || !number.bytes().all(|byte| byte.is_ascii_digit())
        {
            return unread("", "is named as no surface the kernel opens");
        }
        let tree = match tree {
            Some(tree) => Some(Reading { nodes: 0 }.node(tree, "/", 1)?),
            None => None,
        };
        Ok(Self {
            surface: surface.to_owned(),
            title: title.to_owned(),
            tree,
        })
    }
}

/// The pointer to the node `at` of the list `key` of the node `parent`.
fn under(parent: &str, key: &str, at: usize) -> String {
    let parent = if parent == "/" { "" } else { parent };
    format!("{parent}/{key}/{at}")
}

/// What a control of `surface` at `at` is found by in the page.
#[must_use]
pub fn hook(surface: &str, at: &str) -> String {
    format!("{surface}#{at}")
}

/// One read of a tree: how many nodes it has met.
struct Reading {
    nodes: u32,
}

/// A node's keys and values.
type Fields = BTreeMap<String, Value>;

impl Reading {
    /// The node `value` at `at`, `depth` nodes deep, and every node beneath
    /// it.
    fn node(&mut self, value: &Value, at: &str, depth: u32) -> Result<Node, Unread> {
        let Value::Object(fields) = value else {
            return unread(at, "is not a node: an object holding `node`");
        };
        self.nodes = self.nodes.saturating_add(1);
        if self.nodes > KERNEL.ui_tree_nodes {
            return unread(at, "is a node past the most a tree holds");
        }
        if depth > KERNEL.ui_tree_depth {
            return unread(at, "nests deeper than a tree may");
        }
        let Some(kind) = (match fields.get("node") {
            Some(Value::Text(word)) => Kind::of(word),
            _ => None,
        }) else {
            return unread(at, "names no kind of node the kernel keeps");
        };
        for key in fields.keys() {
            if key != "node"
                && !kind.holds().contains(&key.as_str())
                && !kind.may_hold().contains(&key.as_str())
            {
                return unread(at, format!("holds a key a `{}` does not", kind.word()));
            }
        }
        if let Some(key) = kind.holds().iter().find(|key| !fields.contains_key(**key)) {
            return unread(at, format!("is a `{}` without its `{key}`", kind.word()));
        }
        Ok(match kind {
            Kind::Column => Node::Column {
                gap: choice(fields, at)?,
                children: self.nodes_of(fields, "children", at, depth)?,
            },
            Kind::Row => Node::Row {
                gap: choice(fields, at)?,
                children: self.nodes_of(fields, "children", at, depth)?,
            },
            Kind::Text => Node::Text {
                text: text(fields, "text", at)?,
                tone: choice(fields, at)?,
            },
            Kind::Heading => Node::Heading {
                text: text(fields, "text", at)?,
                level: level(fields, at)?,
            },
            Kind::Button => {
                commanded(fields, at)?;
                Node::Button {
                    label: text(fields, "label", at)?,
                    disabled: flag(fields, "disabled", at)?,
                    binds: binds(fields, at)?,
                    at: at.to_owned(),
                }
            }
            Kind::Toggle => {
                commanded(fields, at)?;
                Node::Toggle {
                    label: text(fields, "label", at)?,
                    checked: flag(fields, "checked", at)?,
                    binds: binds(fields, at)?,
                    at: at.to_owned(),
                }
            }
            Kind::Input => Node::Input {
                name: named(fields, at)?,
                label: text(fields, "label", at)?,
                value: maybe_text(fields, "value", at)?,
                field: choice(fields, at)?,
                max_length: max_length(fields, at)?,
            },
            Kind::List => Node::List {
                items: self.nodes_of(fields, "items", at, depth)?,
                empty: maybe_text(fields, "empty", at)?,
            },
            Kind::Image => Node::Image {
                blob: blob(fields, at)?,
                alt: text(fields, "alt", at)?,
            },
            Kind::Divider => Node::Divider,
        })
    }

    /// The nodes the list `key` of the node `fields` at `at` holds, each a
    /// node deeper.
    fn nodes_of(
        &mut self,
        fields: &Fields,
        key: &str,
        at: &str,
        depth: u32,
    ) -> Result<Vec<Node>, Unread> {
        let Some(Value::Array(nodes)) = fields.get(key) else {
            return unread(at, format!("holds a `{key}` that is not a list of nodes"));
        };
        nodes
            .iter()
            .enumerate()
            .map(|(index, node)| self.node(node, &under(at, key, index), depth.saturating_add(1)))
            .collect()
    }
}

/// The level of the heading `fields` at `at`: 1, 2 or 3.
fn level(fields: &Fields, at: &str) -> Result<u8, Unread> {
    match fields.get("level") {
        Some(Value::Integer(level @ 1..=3)) => Ok(u8::try_from(*level).unwrap_or(1)),
        _ => unread(at, "holds a `level` that is not 1, 2 or 3"),
    }
}

/// The most characters the input `fields` at `at` takes, where it says: at
/// least one, and at most the bytes a text holds.
fn max_length(fields: &Fields, at: &str) -> Result<Option<u64>, Unread> {
    match fields.get("max_length") {
        None => Ok(None),
        Some(Value::Integer(most))
            if u64::try_from(*most)
                .is_ok_and(|most| (1..=KERNEL.ui_text_bytes).contains(&most)) =>
        {
            Ok(u64::try_from(*most).ok())
        }
        Some(_) => unread(at, "holds a `max_length` past what a text holds"),
    }
}

/// The blob the image `fields` at `at` shows: a BLAKE3 hash, in 64 lower-case
/// hexadecimal digits.
fn blob(fields: &Fields, at: &str) -> Result<String, Unread> {
    match fields.get("blob") {
        Some(Value::Text(hash))
            if hash.len() == 64
                && hash
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) =>
        {
            Ok(hash.clone())
        }
        _ => unread(at, "holds a `blob` that is no BLAKE3 hash"),
    }
}

/// The text `key` of the node `fields` at `at`, which it holds.
fn text(fields: &Fields, key: &str, at: &str) -> Result<String, Unread> {
    maybe_text(fields, key, at)?.map_or_else(|| unread(at, format!("is without its `{key}`")), Ok)
}

/// The text `key` of the node `fields` at `at`, where it holds one.
fn maybe_text(fields: &Fields, key: &str, at: &str) -> Result<Option<String>, Unread> {
    match fields.get(key) {
        None => Ok(None),
        Some(Value::Text(text)) => Ok(Some(text.clone())),
        Some(_) => unread(at, format!("holds a `{key}` that is not text")),
    }
}

/// The flag `key` of the node `fields` at `at`: false where it holds none.
fn flag(fields: &Fields, key: &str, at: &str) -> Result<bool, Unread> {
    match fields.get(key) {
        None => Ok(false),
        Some(Value::Bool(flag)) => Ok(*flag),
        Some(_) => unread(
            at,
            format!("holds a `{key}` that is neither true nor false"),
        ),
    }
}

/// The word of the choice `T` the node `fields` at `at` holds, or the one a
/// node that leaves it out holds.
fn choice<T: Choice>(fields: &Fields, at: &str) -> Result<T, Unread> {
    match fields.get(T::KEY) {
        None => Ok(T::UNSAID),
        Some(Value::Text(word)) => T::WORDS
            .iter()
            .find(|(written, _)| written == word)
            .map(|(_, choice)| *choice)
            .map_or_else(
                || unread(at, format!("holds a `{}` of no word it may be", T::KEY)),
                Ok,
            ),
        Some(_) => unread(at, format!("holds a `{}` that is not a word", T::KEY)),
    }
}

/// Holds the node `fields` at `at` to what an interaction is: a command named
/// as text and its arguments as an object, neither of which the window keeps.
fn commanded(fields: &Fields, at: &str) -> Result<(), Unread> {
    if !matches!(fields.get("command"), Some(Value::Text(_))) {
        return unread(at, "names its `command` as something other than text");
    }
    if !matches!(fields.get("args"), None | Some(Value::Object(_))) {
        return unread(at, "holds `args` that are not an object");
    }
    Ok(())
}

/// The inputs the node `fields` at `at` binds.
fn binds(fields: &Fields, at: &str) -> Result<Vec<String>, Unread> {
    match fields.get("bind") {
        None => Ok(Vec::new()),
        Some(Value::Array(names)) => names
            .iter()
            .map(|name| match name {
                Value::Text(name) if is_a_name(name) => Ok(name.clone()),
                _ => unread(at, "binds something that is not an input's name"),
            })
            .collect(),
        Some(_) => unread(at, "holds a `bind` that is not a list"),
    }
}

/// The name of the input `fields` at `at`.
fn named(fields: &Fields, at: &str) -> Result<String, Unread> {
    match fields.get("name") {
        Some(Value::Text(name)) if is_a_name(name) => Ok(name.clone()),
        _ => unread(
            at,
            "is named as no input is: a lower-case word, `_` allowed",
        ),
    }
}

/// Whether `name` is an input's name as the kernel holds one: a lower-case
/// letter, then letters, digits and `_`, at most 64 bytes.
fn is_a_name(name: &str) -> bool {
    name.len() <= 64
        && name.starts_with(|first: char| first.is_ascii_lowercase())
        && name
            .chars()
            .all(|rest| rest.is_ascii_lowercase() || rest.is_ascii_digit() || rest == '_')
}

/// The surface's markup for `view`: its region, whole.
#[must_use]
pub fn markup(view: &View) -> String {
    dioxus_ssr::render_element(rsx! {
        TheSurface { view: view.clone() }
    })
}

/// A surface: a region named by its title, holding its tree.
#[component]
pub fn TheSurface(view: View) -> Element {
    let View {
        surface,
        title,
        tree,
    } = view;
    rsx! {
        section { id: "noctua-{surface}", "aria-label": "{title}", class: "flex flex-col",
            if let Some(node) = tree {
                Drawn { node, surface: surface.clone() }
            }
        }
    }
}

/// The design system's tone for a text of `tone`: its default for every tone
/// but `muted`, since `Paragraph` holds no other.
const fn tone_of(tone: Tone) -> ParagraphTone {
    match tone {
        Tone::Muted => ParagraphTone::Muted,
        Tone::Normal | Tone::Danger | Tone::Success => ParagraphTone::Default,
    }
}

/// The size a heading of `level` is drawn at: its level's.
const fn size_of(level: u8) -> HeadingSize {
    match level {
        1 => HeadingSize::H1,
        2 => HeadingSize::H2,
        _ => HeadingSize::H3,
    }
}

/// One node of `surface`'s tree, and every node beneath it.
#[component]
fn Drawn(node: Node, surface: String) -> Element {
    match node {
        Node::Column { children, .. } => rsx! {
            div { class: "flex flex-col",
                for child in children {
                    Drawn { node: child, surface: surface.clone() }
                }
            }
        },
        Node::Row { children, .. } => rsx! {
            div { class: "flex flex-row flex-wrap",
                for child in children {
                    Drawn { node: child, surface: surface.clone() }
                }
            }
        },
        Node::Text { text, tone } => {
            let lines: Vec<(bool, String)> = text
                .split('\n')
                .enumerate()
                .map(|(index, line)| (index > 0, line.to_owned()))
                .collect();
            rsx! {
                Paragraph { tone: tone_of(tone),
                    for (broken, line) in lines {
                        if broken {
                            br {}
                        }
                        "{line}"
                    }
                }
            }
        }
        Node::Heading { text, level } => rsx! {
            Heading {
                level: HeadingLevel::ALL
                    .get(usize::from(level).saturating_sub(1))
                    .copied()
                    .unwrap_or_default(),
                size: size_of(level),
                "{text}"
            }
        },
        Node::Button {
            label,
            disabled,
            at,
            ..
        } => rsx! {
            Button { label, disabled, action: hook(&surface, &at) }
        },
        Node::Toggle {
            label, checked, at, ..
        } => rsx! {
            Switch { name: hook(&surface, &at), label, on: "true", off: "false", checked }
        },
        Node::Input {
            name,
            label,
            value,
            field,
            max_length,
        } => {
            let name = format!("{surface}-{name}");
            let maxlength = max_length.map(|most| most.to_string());
            match field {
                Field::Text => rsx! {
                    Input { name, label, value, maxlength }
                },
                Field::Integer => rsx! {
                    NumberInput { name, label, value }
                },
                Field::Multiline => rsx! {
                    Textarea { name, label, value, maxlength }
                },
            }
        }
        Node::List { items, empty } => {
            if items.is_empty() {
                rsx! {
                    if let Some(empty) = empty {
                        Paragraph { tone: ParagraphTone::Muted, "{empty}" }
                    }
                }
            } else {
                rsx! {
                    List {
                        for item in items {
                            ListItem {
                                Drawn { node: item, surface: surface.clone() }
                            }
                        }
                    }
                }
            }
        }
        Node::Image { alt, .. } => rsx! {
            Paragraph { tone: ParagraphTone::Muted, "{alt}" }
        },
        Node::Divider => rsx! {
            Divider {}
        },
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use nc_canon::Value;
    use nc_catalog::KERNEL;

    use super::{Choice, Field, Gap, Kind, Node, Tone, View, hook, markup};

    /// The kernel's module of surfaces, whose tables the window is held to.
    const KERNEL_UI: &str = include_str!("../../../crates/nc-kernel/src/ui.rs");

    /// The kernel's suite whose plugin renders a tree through
    /// `reference.ui.show`.
    const KERNEL_SUITE: &str =
        include_str!("../../../crates/nc-kernel/tests/the_rest_of_the_world.rs");

    /// The tree `reference.ui.show` renders and the kernel keeps in its own
    /// suite (`a_ui_interaction_naming_an_unregistered_command_is_refused`):
    /// an input and a button binding it.
    pub(crate) const SHOWN: &str = r#"{"children":[{"label":"Text","name":"text","node":"input"},{"args":{"note":"photo"},"bind":["text"],"command":"reference.note.put","label":"Save","node":"button"}],"node":"column"}"#;

    /// The tree of every kind the kernel keeps in its own unit test
    /// (`every_kind_of_node_is_kept_as_its_canonical_text`), read from it.
    fn every() -> Value {
        let (_, after) = KERNEL_UI
            .split_once("fn every_kind_of_node_is_kept_as_its_canonical_text() {")
            .expect("the kernel keeps a tree of every kind");
        let (_, tree) = after.split_once("let tree = r#\"").expect("its tree");
        let (tree, _) = tree.split_once("\"#;").expect("its tree, closed");
        nc_canon::parse(&tree.replace('\n', "")).expect("a tree the kernel keeps")
    }

    /// The text of the kernel's table `name`, between its `= [` and its `];`,
    /// with every space taken out.
    fn table(name: &str) -> String {
        let (_, after) = KERNEL_UI
            .split_once(&format!("const {name}:"))
            .unwrap_or_else(|| panic!("the kernel declares {name}"));
        let (_, body) = after.split_once("= [").expect("a table");
        let (body, _) = body.split_once("];").expect("a table, closed");
        body.chars()
            .filter(|character| !character.is_whitespace())
            .collect()
    }

    /// Every string literal of `text`, in order.
    fn literals(text: &str) -> Vec<String> {
        text.split('"')
            .skip(1)
            .step_by(2)
            .map(str::to_owned)
            .collect()
    }

    /// Each row of a table of tuples, written without spaces, split at the
    /// `,&[` that opens each list it holds.
    fn rows(body: &str) -> Vec<Vec<Vec<String>>> {
        body.trim_start_matches('(')
            .trim_end_matches(',')
            .trim_end_matches(')')
            .split("),(")
            .map(|row| row.split(",&[").map(literals).collect())
            .collect()
    }

    /// The surface `surface-1`, titled `Notes`, showing `tree`, read.
    fn shown(tree: &str) -> View {
        View::read(
            "surface-1",
            "Notes",
            Some(&nc_canon::parse(tree).expect("a tree")),
        )
        .expect("a tree the window reads")
    }

    /// Why `tree` is not read, as `surface-1` showing it.
    fn refused(tree: &str) -> String {
        View::read(
            "surface-1",
            "Notes",
            Some(&nc_canon::parse(tree).expect("canonical JSON")),
        )
        .expect_err(tree)
        .to_string()
    }

    /// The value of every `name="…"` in `written`, in order.
    fn values<'a>(written: &'a str, name: &str) -> Vec<&'a str> {
        written
            .split(&format!(" {name}=\""))
            .skip(1)
            .filter_map(|rest| rest.split_once('"').map(|(value, _)| value))
            .collect()
    }

    /// Every kind of node of `node`'s tree, each once, in the order met.
    fn kinds(node: &Node, met: &mut Vec<Kind>) {
        let (kind, below) = match node {
            Node::Column { children, .. } => (Kind::Column, children.as_slice()),
            Node::Row { children, .. } => (Kind::Row, children.as_slice()),
            Node::Text { .. } => (Kind::Text, &[][..]),
            Node::Heading { .. } => (Kind::Heading, &[][..]),
            Node::Button { .. } => (Kind::Button, &[][..]),
            Node::Toggle { .. } => (Kind::Toggle, &[][..]),
            Node::Input { .. } => (Kind::Input, &[][..]),
            Node::List { items, .. } => (Kind::List, items.as_slice()),
            Node::Image { .. } => (Kind::Image, &[][..]),
            Node::Divider => (Kind::Divider, &[][..]),
        };
        if !met.contains(&kind) {
            met.push(kind);
        }
        for child in below {
            kinds(child, met);
        }
    }

    /// The key of the choice `T` and its words, in order.
    fn words<T: Choice>() -> (String, Vec<String>) {
        (
            T::KEY.to_owned(),
            T::WORDS
                .iter()
                .map(|(written, _)| (*written).to_owned())
                .collect(),
        )
    }

    /// The window's kinds are the kernel's — each word, the keys it holds
    /// and the keys it may hold, in the kernel's order — and the words of
    /// each closed choice are the kernel's, read from `KINDS` and `CHOICES`
    /// in `crates/nc-kernel/src/ui.rs`, so a kind or a word the kernel
    /// comes to keep fails here until the window draws it.
    #[test]
    fn every_kind_the_kernel_keeps_is_one_the_window_draws() {
        let kernels: Vec<(String, Vec<String>, Vec<String>)> = rows(&table("KINDS"))
            .into_iter()
            .map(|row| {
                let [word, holds, may_hold] =
                    <[Vec<String>; 3]>::try_from(row).expect("three parts");
                (word.concat(), holds, may_hold)
            })
            .collect();
        let windows: Vec<(String, Vec<String>, Vec<String>)> = Kind::ALL
            .iter()
            .map(|kind| {
                let owned = |keys: &[&str]| keys.iter().map(|key| (*key).to_owned()).collect();
                (
                    kind.word().to_owned(),
                    owned(kind.holds()),
                    owned(kind.may_hold()),
                )
            })
            .collect();
        assert_eq!(windows, kernels);
        assert_eq!(kernels.len(), 10, "{kernels:?}");
        for kind in Kind::ALL {
            assert_eq!(Kind::of(kind.word()), Some(kind));
        }
        assert_eq!(Kind::of("script"), None);
        let kernels: Vec<(String, Vec<String>)> = rows(&table("CHOICES"))
            .into_iter()
            .map(|row| {
                let [key, words] = <[Vec<String>; 2]>::try_from(row).expect("two parts");
                (key.concat(), words)
            })
            .collect();
        assert_eq!(
            vec![words::<Gap>(), words::<Tone>(), words::<Field>()],
            kernels
        );
        assert_eq!(
            (Gap::UNSAID, Tone::UNSAID, Field::UNSAID),
            (Gap::None, Tone::Normal, Field::Text),
            "the first word of each, as a node that leaves it out is drawn"
        );
    }

    /// The trees drawn here are the kernel's own: the one its suite renders
    /// through `reference.ui.show` and keeps, and the one of every kind its
    /// unit test keeps.
    #[test]
    fn the_trees_drawn_here_are_the_ones_the_kernels_suites_keep() {
        assert!(
            KERNEL_SUITE.contains(&format!("let valid = r#\"{SHOWN}\"#;")),
            "the kernel's suite renders {SHOWN} through `reference.ui.show`"
        );
        assert!(KERNEL_SUITE.contains("fixture.ana(\"reference.ui.show\", &into(valid))"));
        let mut met = Vec::new();
        kinds(
            View::read("surface-1", "Notes", Some(&every()))
                .expect("read")
                .tree
                .as_ref()
                .expect("a tree"),
            &mut met,
        );
        met.sort_by_key(|kind| Kind::ALL.iter().position(|each| each == kind));
        assert_eq!(met, Kind::ALL, "the tree holds every kind");
    }

    /// The tree `reference.ui.show` renders is drawn as a column holding the
    /// design system's `Input`, named by its surface and its name, and its
    /// `Button`, found by its node.
    #[test]
    fn the_tree_reference_ui_show_renders_is_its_input_and_its_button() {
        let view = shown(SHOWN);
        assert_eq!(
            view.tree,
            Some(Node::Column {
                gap: Gap::None,
                children: vec![
                    Node::Input {
                        name: "text".to_owned(),
                        label: "Text".to_owned(),
                        value: None,
                        field: Field::Text,
                        max_length: None,
                    },
                    Node::Button {
                        label: "Save".to_owned(),
                        disabled: false,
                        binds: vec!["text".to_owned()],
                        at: "/children/1".to_owned(),
                    },
                ],
            })
        );
        let written = markup(&view);
        assert!(
            written.starts_with(
                "<section id=\"noctua-surface-1\" aria-label=\"Notes\" class=\"flex flex-col\">\
                 <div class=\"flex flex-col\">"
            ),
            "{written}"
        );
        assert_eq!(
            values(&written, "data-nd-input"),
            ["surface-1-text"],
            "{written}"
        );
        assert!(
            written.contains("id=\"nd-input-surface-1-text\""),
            "{written}"
        );
        assert!(written.contains(">Text</label>"), "{written}");
        assert_eq!(
            values(&written, "data-nd-action"),
            ["surface-1#/children/1"]
        );
        assert!(written.contains(">Save</span>"), "{written}");
    }

    /// Every kind is drawn by the design system's component for it: the
    /// tree of every kind the kernel keeps holds each one's root.
    #[test]
    fn every_kind_is_drawn_by_a_design_component() {
        let written = markup(&View::read("surface-1", "Notes", Some(&every())).expect("read"));
        for (root, count) in [
            ("<div class=\"flex flex-col\"", 1),
            ("<div class=\"flex flex-row flex-wrap\"", 1),
            ("<h1 class=\"nd-heading\" data-nd-size=\"h1\"", 1),
            ("<h3 class=\"nd-heading\" data-nd-size=\"h3\"", 1),
            (
                "<p class=\"nd-paragraph\" data-nd-size=\"body\" data-nd-tone=\"muted\">one<br/>two</p>",
                1,
            ),
            ("<hr class=\"nd-divider\"", 1),
            ("<div class=\"nd-number-input\" ", 1),
            ("<button class=\"nd-button\"", 1),
            ("<button class=\"nd-switch\"", 1),
            (">Nothing yet</p>", 1),
            (">A picture</p>", 1),
        ] {
            assert_eq!(written.matches(root).count(), count, "{root} in {written}");
        }
        assert!(written.contains("aria-checked=\"false\""), "{written}");
        assert!(
            written.contains(" disabled"),
            "the button the node disables: {written}"
        );
        assert!(
            !written.contains("<ul"),
            "an empty list draws its text: {written}"
        );
        assert!(!written.contains("<img"), "no picture is loaded: {written}");
    }

    /// Each tree a window does not read, where it is refused, and what the
    /// refusal says.
    const REFUSED: [(&str, &str, &str); 32] = [
        (r#"{"node":"script","text":"x"}"#, "/", "no kind of node"),
        (r#"{"text":"x"}"#, "/", "no kind of node"),
        (r#"{"node":3}"#, "/", "no kind of node"),
        ("[1]", "/", "is not a node"),
        (
            r#"{"node":"text","text":"x","style":"color:red"}"#,
            "/",
            "a key a `text` does not",
        ),
        (
            r#"{"node":"heading","text":"x"}"#,
            "/",
            "without its `level`",
        ),
        (
            r#"{"node":"heading","text":"x","level":0}"#,
            "/",
            "not 1, 2 or 3",
        ),
        (
            r#"{"node":"heading","text":"x","level":4}"#,
            "/",
            "not 1, 2 or 3",
        ),
        (
            r#"{"node":"heading","text":"x","level":"1"}"#,
            "/",
            "not 1, 2 or 3",
        ),
        (
            r#"{"node":"text","text":3}"#,
            "/",
            "`text` that is not text",
        ),
        (
            r#"{"node":"text","text":"x","tone":"loud"}"#,
            "/",
            "`tone` of no word",
        ),
        (
            r#"{"node":"text","text":"x","tone":1}"#,
            "/",
            "`tone` that is not a word",
        ),
        (
            r#"{"node":"row","children":{"node":"divider"}}"#,
            "/",
            "not a list of nodes",
        ),
        (
            r#"{"node":"row","children":[{"node":"text","text":"x"},{"node":"embed"}]}"#,
            "/children/1",
            "no kind",
        ),
        (
            r#"{"node":"list","items":[{"node":"divider","x":1}]}"#,
            "/items/0",
            "a key a `divider`",
        ),
        (
            r#"{"node":"button","label":"x","command":3}"#,
            "/",
            "`command` as something other than text",
        ),
        (
            r#"{"node":"button","label":"x","command":"a.b","args":[]}"#,
            "/",
            "`args` that are not an object",
        ),
        (
            r#"{"node":"button","label":"x","command":"a.b","disabled":"yes"}"#,
            "/",
            "neither true nor false",
        ),
        (
            r#"{"node":"button","label":"x","command":"a.b","bind":"text"}"#,
            "/",
            "`bind` that is not a list",
        ),
        (
            r#"{"node":"button","label":"x","command":"a.b","bind":[1]}"#,
            "/",
            "not an input's name",
        ),
        (
            r#"{"node":"button","label":"x","command":"a.b","bind":["A"]}"#,
            "/",
            "not an input's name",
        ),
        (
            r#"{"node":"toggle","label":"x","checked":"no","command":"a.b"}"#,
            "/",
            "neither true nor false",
        ),
        (
            r#"{"node":"input","name":"A","label":"x"}"#,
            "/",
            "as no input is",
        ),
        (
            r#"{"node":"input","name":"a b","label":"x"}"#,
            "/",
            "as no input is",
        ),
        (
            r#"{"node":"input","name":"1a","label":"x"}"#,
            "/",
            "as no input is",
        ),
        (
            r#"{"node":"input","name":"a","label":"x","kind":"date"}"#,
            "/",
            "`kind` of no word",
        ),
        (
            r#"{"node":"input","name":"a","label":"x","max_length":0}"#,
            "/",
            "`max_length`",
        ),
        (
            r#"{"node":"input","name":"a","label":"x","max_length":"8"}"#,
            "/",
            "`max_length`",
        ),
        (
            r#"{"node":"image","blob":"https://a.b/c.png","alt":"x"}"#,
            "/",
            "no BLAKE3 hash",
        ),
        (
            r#"{"node":"image","blob":"000000000000000000000000000000000000000000000000000000000000000g","alt":"x"}"#,
            "/",
            "no BLAKE3 hash",
        ),
        (
            r#"{"node":"image","blob":"abc","alt":"x"}"#,
            "/",
            "no BLAKE3 hash",
        ),
        (
            r#"{"node":"column","gap":"huge","children":[]}"#,
            "/",
            "`gap` of no word",
        ),
    ];

    /// A tree holding a node of no kind the kernel keeps, a key its kind does
    /// not hold, a key it must hold left out, or a value of another shape is
    /// refused whole, saying where.
    #[test]
    fn a_tree_holding_anything_else_is_refused_whole() {
        for (tree, at, saying) in REFUSED {
            let said = refused(tree);
            assert!(
                said.contains(&format!("at `{at}`")) && said.contains(saying),
                "{tree}: {said}"
            );
        }
        let longest = format!(
            r#"{{"node":"input","name":"{}","label":"x"}}"#,
            "a".repeat(64)
        );
        assert!(
            View::read(
                "surface-1",
                "x",
                Some(&nc_canon::parse(&longest).expect("a tree"))
            )
            .is_ok()
        );
        assert!(
            refused(&format!(
                r#"{{"node":"input","name":"{}","label":"x"}}"#,
                "a".repeat(65)
            ))
            .contains("as no input is")
        );
        let bound = KERNEL.ui_text_bytes;
        assert!(
            View::read(
                "surface-1",
                "x",
                Some(
                    &nc_canon::parse(&format!(
                        r#"{{"node":"input","name":"a","label":"x","max_length":{bound}}}"#
                    ))
                    .expect("a tree")
                )
            )
            .is_ok(),
            "a bound of exactly a text's"
        );
        assert!(
            refused(&format!(
                r#"{{"node":"input","name":"a","label":"x","max_length":{}}}"#,
                bound + 1
            ))
            .contains("`max_length`")
        );
    }

    /// A tree is read within the kernel's count of nodes and its depth, and a
    /// tree past either is refused at the node that goes past it.
    #[test]
    fn a_tree_is_read_within_the_kernels_count_of_nodes_and_depth() {
        let most = usize::try_from(KERNEL.ui_tree_nodes).expect("fits");
        let wide = |nodes: usize| {
            format!(
                r#"{{"node":"column","children":[{}]}}"#,
                vec![r#"{"node":"divider"}"#; nodes - 1].join(",")
            )
        };
        assert!(
            View::read(
                "surface-1",
                "x",
                Some(&nc_canon::parse(&wide(most)).expect("a tree"))
            )
            .is_ok()
        );
        let said = refused(&wide(most + 1));
        assert!(
            said.contains(&format!("at `/children/{}`", most - 1))
                && said.contains("past the most"),
            "{said}"
        );
        let deep = |depth: u32| {
            let mut tree = r#"{"node":"divider"}"#.to_owned();
            for _ in 1..depth {
                tree = format!(r#"{{"node":"row","children":[{tree}]}}"#);
            }
            tree
        };
        assert!(
            View::read(
                "surface-1",
                "x",
                Some(&nc_canon::parse(&deep(KERNEL.ui_tree_depth)).expect("a tree"))
            )
            .is_ok(),
            "exactly the depth"
        );
        // **One node deeper is past what `nc_canon` parses**, so the window's
        // own bound is asked of a value built without it.
        let mut value = Value::object([("node", Value::text("divider"))]);
        for _ in 0..KERNEL.ui_tree_depth {
            value = Value::object([
                ("node", Value::text("row")),
                ("children", Value::Array(vec![value])),
            ]);
        }
        let said = View::read("surface-1", "x", Some(&value))
            .expect_err("too deep")
            .to_string();
        assert!(said.contains("nests deeper"), "{said}");
    }

    /// A control is found by its surface and the pointer to its node — `/`
    /// for the root, and each list and index down to it — and never by the
    /// command its node runs, which no word of the page holds, nor any of its
    /// arguments; no word it is found by is a command's name, so neither the
    /// footer's nor the drawer's closed set reads one.
    #[test]
    pub(crate) fn every_control_is_found_by_its_node_and_never_by_its_command() {
        let root = shown(r#"{"node":"button","label":"Go","command":"sidebar.show"}"#);
        assert_eq!(values(&markup(&root), "data-nd-action"), ["surface-1#/"]);
        let nested = shown(
            r#"{"node":"column","children":[{"node":"row","children":[{"node":"text","text":"x"},{"node":"toggle","label":"On","checked":true,"command":"info.expand","args":{"x":"secret-argument"}}]},{"node":"list","items":[{"node":"divider"},{"node":"button","label":"Go","command":"drawer.collapse"}]}]}"#,
        );
        let written = markup(&nested);
        assert_eq!(
            values(&written, "data-nd-group"),
            ["surface-1#/children/0/children/1"]
        );
        assert_eq!(
            values(&written, "data-nd-action"),
            ["surface-1#/children/1/items/1"]
        );
        assert!(written.contains("aria-checked=\"true\""), "{written}");
        assert_eq!(
            (
                values(&written, "data-nd-on"),
                values(&written, "data-nd-off")
            ),
            (vec!["true"], vec!["false"]),
            "a toggle's two values are the words `checked` is written in"
        );
        for never in [
            "sidebar.show",
            "info.expand",
            "drawer.collapse",
            "secret-argument",
        ] {
            assert!(
                !markup(&root).contains(never) && !written.contains(never),
                "the page holds {never}: {written}"
            );
        }
        for word in values(&written, "data-nd-action")
            .into_iter()
            .chain(values(&written, "data-nd-group"))
            .chain(values(&markup(&root), "data-nd-action"))
        {
            assert!(
                !nc_schema::descriptor::command_name_is_well_formed(word),
                "{word}"
            );
            assert_eq!(crate::footer::Action::of(word), None, "{word}");
            assert_eq!(crate::drawer::Control::of(word), None, "{word}");
        }
        assert_eq!(hook("surface-12", "/items/3"), "surface-12#/items/3");
    }

    /// Every text a plugin gives is drawn as text: a title, a node's text,
    /// label, value, empty text and alt text holding markup show that markup,
    /// escaped, and the page is handed no element of it.
    #[test]
    fn every_text_is_drawn_as_text() {
        let markup_of = "<script>alert(1)</script><img src=x onerror=\\\"y\\\">&amp;'";
        let tree = format!(
            r#"{{"node":"column","children":[{{"node":"text","text":"{markup_of}"}},{{"node":"heading","text":"{markup_of}","level":2}},{{"node":"button","label":"{markup_of}","command":"a.b"}},{{"node":"toggle","label":"{markup_of}","checked":false,"command":"a.b"}},{{"node":"input","name":"a","label":"{markup_of}","value":"{markup_of}"}},{{"node":"input","name":"b","label":"x","value":"{markup_of}","kind":"multiline"}},{{"node":"list","items":[],"empty":"{markup_of}"}},{{"node":"image","blob":"{}","alt":"{markup_of}"}}]}}"#,
            "0".repeat(64)
        );
        let view = View::read(
            "surface-1",
            "<b>\"Notes\"</b>",
            Some(&nc_canon::parse(&tree).expect("a tree")),
        )
        .expect("read");
        let written = markup(&view);
        for element in ["<script", "<img", "<b>", "onerror=\""] {
            assert!(!written.contains(element), "{element} in {written}");
        }
        assert!(
            written
                .matches("&#60;script&#62;alert(1)&#60;/script&#62;")
                .count()
                >= 8,
            "{written}"
        );
        assert!(
            written.contains("aria-label=\"&#60;b&#62;&#34;Notes&#34;&#60;/b&#62;\""),
            "{written}"
        );
    }

    /// A text keeps every line break it holds, and reads quieter where its
    /// tone is `muted`; `danger` and `success` are drawn in the default tone,
    /// which the design system's `Paragraph` holds beside `muted` alone.
    #[test]
    fn a_text_keeps_its_line_breaks_and_a_muted_one_reads_quieter() {
        for (tone, drawn) in [
            ("normal", "default"),
            ("muted", "muted"),
            ("danger", "default"),
            ("success", "default"),
        ] {
            let written = markup(&shown(&format!(
                r#"{{"node":"text","text":"a\nb\n\nc","tone":"{tone}"}}"#
            )));
            assert!(
                written.contains(&format!(
                    "<p class=\"nd-paragraph\" data-nd-size=\"body\" data-nd-tone=\"{drawn}\">a<br/>b<br/><br/>c</p>"
                )),
                "{tone}: {written}"
            );
        }
        let plain = markup(&shown(r#"{"node":"text","text":"a"}"#));
        assert!(plain.contains("data-nd-tone=\"default\">a</p>"), "{plain}");
    }

    /// A heading is drawn at its level, and at that level's size.
    #[test]
    fn a_heading_is_drawn_at_its_level() {
        for level in 1..=3 {
            let written = markup(&shown(&format!(
                r#"{{"node":"heading","text":"T","level":{level}}}"#
            )));
            assert!(
                written.contains(&format!(
                    "<h{level} class=\"nd-heading\" data-nd-size=\"h{level}\""
                )),
                "{written}"
            );
        }
    }

    /// An input is drawn as its field — one line, a whole number or several
    /// lines — with its label, the value it holds before anything is typed,
    /// and its bound where the field takes one, named by its surface and its
    /// name.
    #[test]
    fn an_input_is_drawn_as_its_field_with_its_value_and_its_bound() {
        let line = markup(&shown(
            r#"{"node":"input","name":"title","label":"Title","value":"Draft","max_length":12}"#,
        ));
        assert!(line.contains("class=\"nd-input"), "{line}");
        assert!(line.contains("name=\"surface-1-title\""), "{line}");
        assert!(line.contains("value=\"Draft\""), "{line}");
        assert!(line.contains("maxlength=\"12\""), "{line}");
        let number = markup(&shown(
            r#"{"node":"input","name":"n","label":"N","value":"3","kind":"integer","max_length":4}"#,
        ));
        assert!(number.contains("class=\"nd-number-input"), "{number}");
        assert!(
            !number.contains("maxlength"),
            "`NumberInput` takes no bound: {number}"
        );
        assert!(
            number.contains("name=\"surface-1-n\"") && number.contains("value=\"3\""),
            "{number}"
        );
        let lines = markup(&shown(
            r#"{"node":"input","name":"body","label":"Body","value":"x","kind":"multiline","max_length":40}"#,
        ));
        assert!(lines.contains("class=\"nd-textarea"), "{lines}");
        assert!(
            lines.contains("name=\"surface-1-body\"") && lines.contains("maxlength=\"40\""),
            "{lines}"
        );
        let bare = markup(&shown(r#"{"node":"input","name":"a","label":"A"}"#));
        assert!(
            !bare.contains("maxlength") && !bare.contains("value="),
            "{bare}"
        );
    }

    /// A list holds an item for each of its nodes, drawn as any node is, and
    /// its `empty` text, quieter, only while it holds none.
    #[test]
    fn a_list_holds_an_item_of_each_node_or_says_it_is_empty() {
        let full = markup(&shown(
            r#"{"node":"list","items":[{"node":"text","text":"one"},{"node":"text","text":"two"}],"empty":"None"}"#,
        ));
        assert_eq!(
            full.matches("<li class=\"nd-list-item\"").count(),
            2,
            "{full}"
        );
        assert!(full.contains("<ul class=\"nd-list\""), "{full}");
        assert!(!full.contains("None"), "{full}");
        let empty = markup(&shown(r#"{"node":"list","items":[],"empty":"None"}"#));
        assert!(empty.contains("data-nd-tone=\"muted\">None</p>"), "{empty}");
        assert!(!empty.contains("<ul"), "{empty}");
        let silent = markup(&shown(r#"{"node":"list","items":[]}"#));
        assert_eq!(
            silent,
            "<section id=\"noctua-surface-1\" aria-label=\"Notes\" class=\"flex flex-col\"></section>"
        );
    }

    /// A surface is a region named by its title, empty until a tree is kept
    /// for it, and only a surface named as the kernel names one is read.
    #[test]
    fn a_surface_is_a_region_named_by_its_title() {
        let empty = View::read("surface-7", "Notes", None).expect("read");
        assert_eq!(
            markup(&empty),
            "<section id=\"noctua-surface-7\" aria-label=\"Notes\" class=\"flex flex-col\"></section>"
        );
        for named in [
            "surface-",
            "surface-x",
            "panel-1",
            "surface-1 ",
            "",
            &format!("surface-{}", "1".repeat(21)),
        ] {
            let said = View::read(named, "Notes", None)
                .expect_err(named)
                .to_string();
            assert!(said.contains("named as no surface"), "{said}");
        }
        assert!(View::read(&format!("surface-{}", u64::MAX), "Notes", None).is_ok());
    }

    /// Nothing a surface draws is given a colour, a size or a border of the
    /// window's own: no inline style, and no class but the design system's
    /// and the layout utilities the sheet holds.
    #[test]
    fn nothing_is_drawn_but_the_design_systems_components_and_the_sheets_utilities() {
        let written = markup(&View::read("surface-1", "Notes", Some(&every())).expect("read"));
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
}
