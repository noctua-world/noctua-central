//! The declarative UI: the surfaces a plugin opens, and the trees it renders
//! into them for the host to draw (`NOCTUA.md §11.3`).
//!
//! **A closed set of nodes, judged on every render.** A tree is canonical JSON
//! within `payload_bytes`, and every node of it is an object holding `node` —
//! one of the kinds below — and no key its kind does not name:
//!
//! | `node` | Holds | May hold |
//! |---|---|---|
//! | `column`, `row` | `children`, a list of nodes | `gap`: `none`, `small`, `medium` or `large` |
//! | `text` | `text` | `tone`: `normal`, `muted`, `danger` or `success` |
//! | `heading` | `text`, and `level` from 1 to 3 | |
//! | `button` | `label`, and `command` | `args`, an object; `bind`, a list of input names; `disabled` |
//! | `toggle` | `label`, `checked` and `command` | `args`; `bind` |
//! | `input` | `name` — a lower-case word, `_` allowed — and `label` | `value`; `kind`: `text`, `integer` or `multiline`; `max_length` |
//! | `list` | `items`, a list of nodes | `empty`, the text shown when it has none |
//! | `image` | `blob`, a BLAKE3 hash, and `alt` | |
//! | `divider` | nothing more | |
//!
//! A tree holds at most `ui_tree_nodes` nodes and nests at most
//! `ui_tree_depth` deep; each text is at most `ui_text_bytes` bytes and holds
//! no character a reader sees nothing of but a line break; `max_length` is at
//! most `ui_text_bytes`. **An interaction names a command, and the kernel
//! holds it to the registry**: the command a `button` or a `toggle` names is
//! registered when the tree is rendered, every key of its `args` and every name
//! it binds is a property of that command's schema — never one written in
//! both — and every name it binds is an `input` of the same tree, each input's
//! name once. There is no URL, no markup and no style to write: an image
//! names a blob by its BLAKE3 hash, never by an address, and a text is text.
//!
//! **What a person sees is the last tree the kernel kept.** A tree refused is
//! refused whole with `[NC-PLUG-0016]`, naming the place in it and why, and the
//! surface keeps the last one it accepted, which [`crate::Kernel::surfaces`]
//! lists for a renderer to draw — once the call tree that did it releases it:
//! a surface opened, a tree rendered and a close are each held by the tree as
//! an event or a timer is ([`Shown`]), and reach what is shown only when the
//! tree's unit is kept, or when a tree that holds none ends with every entry
//! it gave the journal taken. A surface is opened under a title a person can
//! read — at most `nc_schema::descriptor::TITLE_MAX` characters, none of them
//! one that shows nothing — a plugin holds at most `surfaces_per_plugin` open
//! at once (`[NC-PLUG-0017]`), and a surface it does not hold, another's or
//! none, is not found (`[NC-PLUG-0019]`).
//!
//! What this answers (`NOCTUA.md §2.4`): a plugin drawing what the host does
//! not render — markup, a script, a remote image, a style — an interaction
//! that dispatches a command nothing registered or with arguments its command
//! does not take, a tree that grows the kernel without end, and a plugin
//! drawing into another's surface. What it does not answer: the click itself,
//! which a renderer dispatches through the decision point as the person who
//! clicked, as any dispatch, and which is never a confirmation — there is no
//! renderer in this build, so no click is dispatched at all; and whether this
//! machine holds the blob an image names, which judging a tree does not ask —
//! a blob can arrive or be swept after the tree naming it is kept, so it is the
//! renderer's to ask where the blob is when it draws one.

use std::collections::{BTreeMap, BTreeSet};

use nc_canon::Value;
use nc_catalog::{ErrorCode, KERNEL};

use crate::PluginSlot;
use crate::dispatch::{Refusal, RefusalKind};
use crate::registry::Admitted;

/// Where a surface is shown, as the world's `surface-kind` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceKind {
    /// A side or auxiliary panel.
    Panel,
    /// A full-screen document.
    Document,
    /// An overlay that comes and goes.
    Overlay,
    /// A node anchored in 3D space.
    Node3d,
}

impl SurfaceKind {
    /// The world's word for it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Panel => "panel",
            Self::Document => "document",
            Self::Overlay => "overlay",
            Self::Node3d => "node-3d",
        }
    }
}

/// One surface a plugin holds open: its identifier, its owner, where it is
/// shown, its title, and the last tree rendered into it that the kernel kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Surface {
    id: String,
    owner: PluginSlot,
    kind: SurfaceKind,
    title: String,
    tree: Option<String>,
}

impl Surface {
    /// The identifier the plugin names it by, which the kernel chose.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The plugin that holds it.
    #[must_use]
    pub const fn owner(&self) -> PluginSlot {
        self.owner
    }

    /// Where it is shown.
    #[must_use]
    pub const fn kind(&self) -> SurfaceKind {
        self.kind
    }

    /// Its title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The last tree rendered into it that the kernel kept, as canonical
    /// text: none until one is.
    #[must_use]
    pub fn tree(&self) -> Option<&str> {
        self.tree.as_deref()
    }
}

/// The refusal of a surface `id` the plugin does not hold.
pub(crate) fn not_held(id: &str) -> Refusal {
    Refusal::new(
        RefusalKind::NotFound,
        ErrorCode::PluginNotFound,
        format!(
            "the plugin holds no surface {}",
            crate::registry::quoted_up_to(id, QUOTED)
        ),
    )
}

/// How many characters of what a plugin named a refusal repeats.
const QUOTED: usize = 64;

/// The refusal of a tree or a title, said as `why`.
fn refused(why: String) -> Refusal {
    Refusal::new(RefusalKind::Invalid, ErrorCode::PluginUiInvalid, why)
}

/// What a call tree did to a surface, held until the tree releases it.
///
/// **Held as an event and a timer are** (`crate::units::Later`): in the unit
/// of the tree when it holds one, released when the unit is kept, and in the
/// tree's own when it does not, released once the tree ends with every entry
/// it gave the journal taken. **A reviewer found a real bug with it**: a
/// surface was opened, rendered into and closed at once, so a read whose
/// entry the journal refused — its answer withheld — and a unit the journal
/// did not keep each left a surface showing what they did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Shown {
    /// A surface opened, under the identifier its plugin was answered.
    Opened(Surface),
    /// A tree [`validated`] accepted, rendered into the surface `id` of the
    /// plugin in `owner`.
    Rendered {
        /// The plugin that rendered it.
        owner: PluginSlot,
        /// The surface it was rendered into.
        id: String,
        /// The tree, as canonical text.
        tree: String,
    },
    /// The surface `id` of the plugin in `owner`, closed.
    Closed {
        /// The plugin that closed it.
        owner: PluginSlot,
        /// The surface it closed.
        id: String,
    },
}

impl Shown {
    /// Whether it is a tree the plugin in `owner` rendered into its surface
    /// `id`.
    pub(crate) fn renders_into(&self, owner: PluginSlot, id: &str) -> bool {
        matches!(self, Self::Rendered { owner: by, id: into, .. } if *by == owner && into == id)
    }
}

/// Every surface open, by its identifier, and the last identifier handed out.
#[derive(Debug, Default)]
pub(crate) struct Surfaces {
    open: BTreeMap<String, Surface>,
    minted: u64,
}

impl Surfaces {
    /// A surface of `kind` titled `title` the plugin in `owner` opens, under
    /// an identifier never handed out before, for its call tree to hold until
    /// it releases it — once `held`, what the tree did to the surfaces and
    /// has not released, is applied, which the plugin's bound counts.
    ///
    /// # Errors
    ///
    /// `invalid` with `[NC-PLUG-0016]` for a title no person could read, and
    /// `unavailable` with `[NC-PLUG-0017]` when the plugin holds as many
    /// surfaces as it may.
    pub(crate) fn open(
        &mut self,
        owner: PluginSlot,
        kind: SurfaceKind,
        title: &str,
        held: &[&Shown],
    ) -> Result<Surface, Refusal> {
        titled(title)?;
        let held = self
            .after(held)
            .values()
            .filter(|surface| surface.owner == owner)
            .count();
        if u64::try_from(held).unwrap_or(u64::MAX) >= u64::from(KERNEL.surfaces_per_plugin) {
            return Err(Refusal::new(
                RefusalKind::Unavailable,
                ErrorCode::PluginBoundReached,
                format!(
                    "a plugin holds at most {} surfaces open, and this one would be one more",
                    KERNEL.surfaces_per_plugin
                ),
            ));
        }
        self.minted = self.minted.saturating_add(1);
        Ok(Surface {
            id: format!("surface-{}", self.minted),
            owner,
            kind,
            title: title.to_owned(),
            tree: None,
        })
    }

    /// Closes the surface `id` of the plugin in `owner`.
    ///
    /// # Errors
    ///
    /// `not-found` with `[NC-PLUG-0019]` when the plugin holds no such
    /// surface.
    pub(crate) fn close(&mut self, owner: PluginSlot, id: &str) -> Result<(), Refusal> {
        if !self.holds(owner, id, &[]) {
            return Err(not_held(id));
        }
        self.open.remove(id);
        Ok(())
    }

    /// Whether the plugin in `owner` holds the surface `id`, once `held`,
    /// what its call tree did to the surfaces and has not released, is
    /// applied.
    pub(crate) fn holds(&self, owner: PluginSlot, id: &str, held: &[&Shown]) -> bool {
        self.after(held)
            .get(id)
            .is_some_and(|surface| surface.owner == owner)
    }

    /// Every surface open once `held` is applied to what is shown, in the
    /// order it was done — each without the tree it shows, which no question
    /// asked of it reads.
    fn after(&self, held: &[&Shown]) -> BTreeMap<String, Surface> {
        let bare = |surface: &Surface| Surface {
            id: surface.id.clone(),
            owner: surface.owner,
            kind: surface.kind,
            title: surface.title.clone(),
            tree: None,
        };
        let mut open: BTreeMap<String, Surface> = self
            .open
            .iter()
            .map(|(id, surface)| (id.clone(), bare(surface)))
            .collect();
        for shown in held {
            match shown {
                Shown::Opened(surface) => {
                    open.insert(surface.id.clone(), bare(surface));
                }
                Shown::Closed { owner, id } => {
                    if open.get(id).is_some_and(|surface| surface.owner == *owner) {
                        open.remove(id);
                    }
                }
                Shown::Rendered { .. } => {}
            }
        }
        open
    }

    /// Keeps `tree`, a tree [`validated`] accepted, as what the surface `id`
    /// of the plugin in `owner` shows.
    ///
    /// # Errors
    ///
    /// `not-found` with `[NC-PLUG-0019]` when the plugin holds no such
    /// surface.
    pub(crate) fn render(
        &mut self,
        owner: PluginSlot,
        id: &str,
        tree: String,
    ) -> Result<(), Refusal> {
        match self.open.get_mut(id) {
            Some(surface) if surface.owner == owner => {
                surface.tree = Some(tree);
                Ok(())
            }
            _ => Err(not_held(id)),
        }
    }

    /// Shows what a call tree did, as it releases it: a surface opened is
    /// shown, and a tree rendered into or a close of a surface its plugin
    /// still holds is applied to it.
    ///
    /// **Nothing is judged again here**: each was judged against what was
    /// shown and what the tree held when the plugin did it, and no other call
    /// tree ran in between — so a surface gone since is one an unload closed,
    /// and what was done to it goes with it.
    pub(crate) fn shown(&mut self, shown: Shown) {
        match shown {
            Shown::Opened(surface) => {
                self.open.insert(surface.id.clone(), surface);
            }
            Shown::Rendered { owner, id, tree } => {
                let _closed_meanwhile = self.render(owner, &id, tree);
            }
            Shown::Closed { owner, id } => {
                let _closed_meanwhile = self.close(owner, &id);
            }
        }
    }

    /// Every surface open, in the order their identifiers sort.
    pub(crate) fn listed(&self) -> Vec<Surface> {
        self.open.values().cloned().collect()
    }

    /// Closes every surface the plugin in `owner` holds: one being unloaded.
    pub(crate) fn forget(&mut self, owner: PluginSlot) {
        self.open.retain(|_, surface| surface.owner != owner);
    }
}

/// Refuses `title` unless a person can read it.
///
/// # Errors
///
/// `invalid` with `[NC-PLUG-0016]` for a title that is blank, longer than
/// `nc_schema::descriptor::TITLE_MAX` characters, or holds a character that
/// shows nothing.
pub(crate) fn titled(title: &str) -> Result<(), Refusal> {
    let most = nc_schema::descriptor::TITLE_MAX;
    if title.trim().is_empty() {
        return Err(refused(
            "a surface is opened under a title, and this one is blank".to_owned(),
        ));
    }
    if title.chars().count() > most {
        return Err(refused(format!(
            "a surface's title is at most {most} characters, and this one is {}",
            title.chars().count()
        )));
    }
    if title.chars().any(nc_schema::unseen) {
        return Err(refused(
            "a surface's title holds a character a reader sees nothing of".to_owned(),
        ));
    }
    Ok(())
}

/// The kinds of node a tree may hold, each with the keys it must and the keys
/// it may hold beside `node`.
const KINDS: [(&str, &[&str], &[&str]); 10] = [
    ("column", &["children"], &["gap"]),
    ("row", &["children"], &["gap"]),
    ("text", &["text"], &["tone"]),
    ("heading", &["text", "level"], &[]),
    (
        "button",
        &["label", "command"],
        &["args", "bind", "disabled"],
    ),
    (
        "toggle",
        &["label", "checked", "command"],
        &["args", "bind"],
    ),
    (
        "input",
        &["name", "label"],
        &["value", "kind", "max_length"],
    ),
    ("list", &["items"], &["empty"]),
    ("image", &["blob", "alt"], &[]),
    ("divider", &[], &[]),
];

/// The words each key of a closed choice may be.
const CHOICES: [(&str, &[&str]); 3] = [
    ("gap", &["none", "small", "medium", "large"]),
    ("tone", &["normal", "muted", "danger", "success"]),
    ("kind", &["text", "integer", "multiline"]),
];

/// The keys whose value is a text a person reads.
const TEXTS: [&str; 5] = ["text", "label", "value", "empty", "alt"];

/// How long an input's name may be, in bytes.
const INPUT_NAME_MAX: usize = 64;

/// Where in a tree something was refused, and why.
struct Fault {
    path: String,
    why: String,
}

/// `why`, said of `path`.
fn fault<T>(path: &str, why: impl Into<String>) -> Result<T, Fault> {
    Err(Fault {
        path: path.to_owned(),
        why: why.into(),
    })
}

/// One walk over a tree: how many nodes it has met, the inputs it named, and
/// every name a node binds, to be held to those inputs once the walk is done.
struct Walk<'a> {
    command: &'a dyn Fn(&str) -> Option<Admitted>,
    nodes: u64,
    inputs: BTreeSet<String>,
    binds: Vec<(String, String)>,
}

/// `tree`, as the canonical JSON the kernel keeps for a surface, once every
/// rule of the module's header holds of it; `command` is the registry, asked
/// for each command an interaction names.
///
/// # Errors
///
/// `invalid` with `[NC-PLUG-0016]`, naming where the tree is refused, as
/// a JSON pointer, and why.
pub(crate) fn validated(
    tree: &str,
    command: &dyn Fn(&str) -> Option<Admitted>,
) -> Result<String, Refusal> {
    if u64::try_from(tree.len()).unwrap_or(u64::MAX) > KERNEL.payload_bytes {
        return Err(refused(format!(
            "a tree is at most {} bytes, the bound of a payload, and this one is {}",
            KERNEL.payload_bytes,
            tree.len()
        )));
    }
    let value = nc_canon::parse(tree).map_err(|error| {
        refused(format!(
            "a tree is canonical JSON, and this one is not: {error}"
        ))
    })?;
    let mut walk = Walk {
        command,
        nodes: 0,
        inputs: BTreeSet::new(),
        binds: Vec::new(),
    };
    walk.node(&value, "", 1)
        .and_then(|()| walk.bound())
        .map_err(|Fault { path, why }| {
            let at = if path.is_empty() { "/" } else { path.as_str() };
            refused(format!("the tree is refused at `{at}`: it {why}"))
        })?;
    Ok(value.to_canonical())
}

impl Walk<'_> {
    /// Judges the node `value` at `path`, `depth` nodes deep, and every node
    /// beneath it.
    fn node(&mut self, value: &Value, path: &str, depth: u64) -> Result<(), Fault> {
        let Value::Object(fields) = value else {
            return fault(path, "is not a node: an object holding `node`");
        };
        self.nodes = self.nodes.saturating_add(1);
        if self.nodes > u64::from(KERNEL.ui_tree_nodes) {
            return fault(
                path,
                format!(
                    "is a node past the {} a tree may hold",
                    KERNEL.ui_tree_nodes
                ),
            );
        }
        if depth > u64::from(KERNEL.ui_tree_depth) {
            return fault(
                path,
                format!("nests deeper than the {} a tree may", KERNEL.ui_tree_depth),
            );
        }
        let Some(Value::Text(kind)) = fields.get("node") else {
            return fault(path, "is not a node: it names no `node` as text");
        };
        let Some((_, required, optional)) = KINDS.iter().find(|(name, _, _)| name == kind) else {
            return fault(
                &format!("{path}/node"),
                format!(
                    "names {}, which is no node the kernel renders",
                    crate::registry::quoted_up_to(kind, QUOTED)
                ),
            );
        };
        for key in fields.keys() {
            if key != "node"
                && !required.contains(&key.as_str())
                && !optional.contains(&key.as_str())
            {
                return fault(
                    &format!("{path}/{}", segment(key)),
                    format!("is a key a `{kind}` does not hold"),
                );
            }
        }
        for key in *required {
            if !fields.contains_key(*key) {
                return fault(path, format!("is a `{kind}` without its `{key}`"));
            }
        }
        for (key, held) in fields {
            if key != "node" {
                self.key(kind, key, held, &format!("{path}/{key}"), depth)?;
            }
        }
        if let Some(Value::Text(name)) = fields.get("command") {
            self.interaction(fields, name, path)?;
        }
        Ok(())
    }

    /// Judges the value `held` of the key `key` of a `kind` node at `path`.
    fn key(
        &mut self,
        kind: &str,
        key: &str,
        held: &Value,
        path: &str,
        depth: u64,
    ) -> Result<(), Fault> {
        if TEXTS.contains(&key) {
            return text(held, path);
        }
        if let Some((_, words)) = CHOICES.iter().find(|(choice, _)| *choice == key) {
            return match held {
                Value::Text(word) if words.contains(&word.as_str()) => Ok(()),
                _ => fault(path, format!("is none of {}", words.join(", "))),
            };
        }
        match (key, held) {
            ("children" | "items", Value::Array(nodes)) => {
                for (at, node) in nodes.iter().enumerate() {
                    self.node(node, &format!("{path}/{at}"), depth.saturating_add(1))?;
                }
                Ok(())
            }
            ("level", Value::Integer(level)) if (1..=3).contains(level) => Ok(()),
            ("max_length", Value::Integer(most))
                if u64::try_from(*most)
                    .is_ok_and(|most| (1..=KERNEL.ui_text_bytes).contains(&most)) =>
            {
                Ok(())
            }
            ("checked" | "disabled", Value::Bool(_))
            | ("command", Value::Text(_))
            | ("args", Value::Object(_))
            | ("bind", Value::Array(_)) => Ok(()),
            ("name", Value::Text(name)) => self.input(name, path),
            ("blob", Value::Text(hash)) if crate::blobs::Hash::parse(hash).is_some() => Ok(()),
            _ => fault(path, format!("is not what a `{kind}`'s `{key}` holds")),
        }
    }

    /// Records the input `name` at `path`, once a tree may hold it.
    fn input(&mut self, name: &str, path: &str) -> Result<(), Fault> {
        let shaped = name.len() <= INPUT_NAME_MAX
            && name.starts_with(|first: char| first.is_ascii_lowercase())
            && name
                .chars()
                .all(|rest| rest.is_ascii_lowercase() || rest.is_ascii_digit() || rest == '_');
        if !shaped {
            return fault(
                path,
                format!(
                    "is not an input's name: a lower-case word of at most {INPUT_NAME_MAX} \
                     bytes, `_` allowed"
                ),
            );
        }
        if !self.inputs.insert(name.to_owned()) {
            return fault(path, format!("names the input `{name}` a second time"));
        }
        Ok(())
    }

    /// Judges the interaction of the node `fields` at `path`, which names the
    /// command `name`: registered, and every key of its `args` and every name
    /// it binds a property of that command's schema, none in both.
    fn interaction(
        &mut self,
        fields: &BTreeMap<String, Value>,
        name: &str,
        path: &str,
    ) -> Result<(), Fault> {
        let Some(command) = (self.command)(name) else {
            return fault(
                &format!("{path}/command"),
                format!(
                    "names {}, which is no command the kernel has registered",
                    crate::registry::quoted_up_to(name, QUOTED)
                ),
            );
        };
        let properties = command.judged().schema.properties();
        let given: Vec<&str> = match fields.get("args") {
            Some(Value::Object(args)) => args.keys().map(String::as_str).collect(),
            _ => Vec::new(),
        };
        for key in &given {
            if !properties.contains(key) {
                return fault(
                    &format!("{path}/args"),
                    format!(
                        "names {}, which is no argument `{name}` takes",
                        crate::registry::quoted_up_to(key, QUOTED)
                    ),
                );
            }
        }
        let bound: Vec<&Value> = match fields.get("bind") {
            Some(Value::Array(names)) => names.iter().collect(),
            _ => Vec::new(),
        };
        for (at, bind) in bound.into_iter().enumerate() {
            let place = format!("{path}/bind/{at}");
            let Value::Text(bind) = bind else {
                return fault(&place, "is not the name of an input");
            };
            if !properties.contains(&bind.as_str()) {
                return fault(
                    &place,
                    format!(
                        "binds {}, which is no argument `{name}` takes",
                        crate::registry::quoted_up_to(bind, QUOTED)
                    ),
                );
            }
            if given.contains(&bind.as_str()) {
                return fault(
                    &place,
                    format!("binds `{bind}`, which `args` gives already"),
                );
            }
            self.binds.push((place, bind.clone()));
        }
        Ok(())
    }

    /// Refuses a name a node binds that no input of the tree is named.
    fn bound(&self) -> Result<(), Fault> {
        for (place, bind) in &self.binds {
            if !self.inputs.contains(bind) {
                return fault(
                    place,
                    format!("binds `{bind}`, and no input of the tree is named so"),
                );
            }
        }
        Ok(())
    }
}

/// `key` as one segment of the JSON pointer a refusal names, cut after
/// [`QUOTED`] characters: `~` and `/` written as the pointer writes them, and
/// every character a reader could not see, or that would end the backticks the
/// pointer is shown in, escaped.
fn segment(key: &str) -> String {
    let mut shown = String::new();
    for character in key.chars().take(QUOTED) {
        match character {
            '~' => shown.push_str("~0"),
            '/' => shown.push_str("~1"),
            '`' => shown.push_str("\\u{60}"),
            other => shown.extend(other.escape_debug()),
        }
    }
    if key.chars().nth(QUOTED).is_some() {
        shown.push('…');
    }
    shown
}

/// Judges the text `held` at `path`: text, within `ui_text_bytes`, and showing
/// every character but a line break.
fn text(held: &Value, path: &str) -> Result<(), Fault> {
    let Value::Text(text) = held else {
        return fault(path, "is not text");
    };
    if u64::try_from(text.len()).unwrap_or(u64::MAX) > KERNEL.ui_text_bytes {
        return fault(
            path,
            format!(
                "is {} bytes, past the {} a text may be",
                text.len(),
                KERNEL.ui_text_bytes
            ),
        );
    }
    if text
        .chars()
        .any(|character| character != '\n' && nc_schema::unseen(character))
    {
        return fault(
            path,
            "holds a character a reader sees nothing of, which only a line break may be",
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use nc_catalog::{ErrorCode, KERNEL};

    use super::{Fault, Shown, Surface, SurfaceKind, Surfaces, Walk, titled, validated};
    use crate::PluginSlot;
    use crate::dispatch::{Refusal, RefusalKind};
    use crate::registry::{Admitted, Registry};

    /// The kernel's own commands, as a registry holds them: `journal.verify`
    /// takes `from`, and `kernel.undo` takes nothing.
    fn registered(name: &str) -> Option<Admitted> {
        Registry::from_catalog().lookup(name)
    }

    /// What `tree` is judged as, against the kernel's own commands.
    fn judged(tree: &str) -> Result<String, (RefusalKind, ErrorCode, String)> {
        validated(tree, &registered).map_err(|refused| {
            (
                refused.kind(),
                refused.code(),
                refused.sentence().to_owned(),
            )
        })
    }

    /// Asserts `tree` is refused at `at`, its sentence holding `saying`.
    fn refused_at(tree: &str, at: &str, saying: &str) {
        let (kind, code, sentence) = judged(tree).expect_err(tree);
        assert_eq!(
            (kind, code),
            (RefusalKind::Invalid, ErrorCode::PluginUiInvalid),
            "{tree}"
        );
        assert!(
            sentence.contains(&format!("refused at `{at}`")) && sentence.contains(saying),
            "{tree}: {sentence}"
        );
    }

    #[test]
    fn every_kind_of_node_is_kept_as_its_canonical_text() {
        let tree = r#"{"node":"column","gap":"small","children":[
            {"node":"heading","text":"Notes","level":1},
            {"node":"heading","text":"Today","level":3},
            {"node":"text","text":"one\ntwo","tone":"muted"},
            {"node":"row","children":[{"node":"divider"}]},
            {"node":"input","name":"from","label":"From","value":"0","kind":"integer","max_length":8},
            {"node":"button","label":"Verify","command":"journal.verify","bind":["from"],"disabled":true},
            {"node":"toggle","label":"Undo","checked":false,"command":"kernel.undo","args":{}},
            {"node":"list","items":[],"empty":"Nothing yet"},
            {"node":"image","blob":"0000000000000000000000000000000000000000000000000000000000000000","alt":"A picture"}
        ]}"#;
        let kept = judged(&tree.replace('\n', "")).expect("every kind, held to its rules");
        assert!(kept.starts_with(r#"{"children":["#), "sorted keys: {kept}");
        assert_eq!(
            nc_canon::parse(&kept).expect("canonical").to_canonical(),
            kept
        );
    }

    #[test]
    fn a_node_outside_the_closed_set_or_holding_a_key_its_kind_does_not_is_refused() {
        refused_at(
            r#"{"node":"script","text":"x"}"#,
            "/node",
            "no node the kernel renders",
        );
        refused_at(
            r#"{"node":"text","text":"x","style":"color:red"}"#,
            "/style",
            "a key",
        );
        refused_at(
            r#"{"node":"text","text":"x","href":"https://a.b"}"#,
            "/href",
            "a key",
        );
        refused_at(
            r#"{"node":"text","text":"x","a/b~`c":1}"#,
            "/a~1b~0\\u{60}c",
            "a key",
        );
        let (_, _, sentence) = judged(&format!(
            r#"{{"node":"divider","{}":1}}"#,
            "\\u001b".repeat(10_000)
        ))
        .expect_err("a key no node holds");
        assert!(
            sentence.len() < 1_000 && !sentence.contains('\u{1b}'),
            "a key a node does not hold is named escaped and cut: {sentence}"
        );
        refused_at(
            r#"{"node":"heading","text":"x"}"#,
            "/",
            "without its `level`",
        );
        refused_at(r#"{"text":"x"}"#, "/", "names no `node`");
        refused_at(r"[1]", "/", "is not a node");
        for level in [0, 4] {
            refused_at(
                &format!(r#"{{"node":"heading","text":"x","level":{level}}}"#),
                "/level",
                "is not what",
            );
        }
        refused_at(
            r#"{"node":"button","label":"x","command":"kernel.undo","disabled":"yes"}"#,
            "/disabled",
            "is not what",
        );
        refused_at(
            r#"{"node":"text","text":"x","tone":"loud"}"#,
            "/tone",
            "is none of",
        );
        refused_at(
            r#"{"node":"row","children":{"node":"divider"}}"#,
            "/children",
            "is not what",
        );
        refused_at(
            r#"{"node":"row","children":[{"node":"text","text":3}]}"#,
            "/children/0/text",
            "is not text",
        );
        refused_at(
            r#"{"node":"image","blob":"https://a.b/c.png","alt":"x"}"#,
            "/blob",
            "is not what",
        );
        refused_at(
            r#"{"node":"input","name":"a","label":"x","max_length":0}"#,
            "/max_length",
            "is not what",
        );
        refused_at(
            &format!(
                r#"{{"node":"input","name":"a","label":"x","max_length":{}}}"#,
                KERNEL.ui_text_bytes + 1
            ),
            "/max_length",
            "is not what",
        );
        assert!(
            judged(&format!(
                r#"{{"node":"input","name":"a","label":"x","max_length":{}}}"#,
                KERNEL.ui_text_bytes
            ))
            .is_ok(),
            "a bound of exactly a text's"
        );
        let (kind, code, sentence) =
            judged(r#"{"node":"text","text":"x","tone":1.5}"#).expect_err("a float");
        assert_eq!(
            (kind, code),
            (RefusalKind::Invalid, ErrorCode::PluginUiInvalid)
        );
        assert!(sentence.contains("canonical JSON"), "{sentence}");
    }

    #[test]
    fn a_text_is_held_to_its_bound_and_to_the_characters_a_reader_sees() {
        let bound = usize::try_from(KERNEL.ui_text_bytes).expect("fits");
        let at = format!(r#"{{"node":"text","text":"{}"}}"#, "a".repeat(bound));
        assert!(judged(&at).is_ok(), "a text of exactly the bound");
        let past = format!(r#"{{"node":"text","text":"{}"}}"#, "a".repeat(bound + 1));
        refused_at(&past, "/text", "past the");
        refused_at(
            "{\"node\":\"text\",\"text\":\"a\\u202eb\"}",
            "/text",
            "sees nothing of",
        );
        // **And a default-ignorable point**, drawn as nothing: a reviewer
        // found variation selectors carrying bytes unseen past this rule.
        refused_at(
            "{\"node\":\"text\",\"text\":\"a\\ufe0f\\ufe01b\"}",
            "/text",
            "sees nothing of",
        );
        refused_at(
            r#"{"node":"button","label":"a\u0007","command":"kernel.undo"}"#,
            "/label",
            "sees nothing of",
        );
        assert!(
            judged(r#"{"node":"text","text":"a\nb"}"#).is_ok(),
            "a line break shows"
        );
    }

    #[test]
    fn a_tree_is_held_to_its_count_of_nodes_and_its_depth() {
        let most = usize::try_from(KERNEL.ui_tree_nodes).expect("fits");
        let wide = |nodes: usize| {
            format!(
                r#"{{"node":"column","children":[{}]}}"#,
                vec![r#"{"node":"divider"}"#; nodes - 1].join(",")
            )
        };
        assert!(
            judged(&wide(most)).is_ok(),
            "exactly the nodes a tree may hold"
        );
        refused_at(
            &wide(most + 1),
            &format!("/children/{}", most - 1),
            "past the",
        );
        let deep = |depth: u32| {
            let mut tree = r#"{"node":"divider"}"#.to_owned();
            for _ in 1..depth {
                tree = format!(r#"{{"node":"row","children":[{tree}]}}"#);
            }
            tree
        };
        assert!(
            judged(&deep(KERNEL.ui_tree_depth)).is_ok(),
            "exactly the depth"
        );
        let (_, _, sentence) = judged(&deep(KERNEL.ui_tree_depth + 1)).expect_err("too deep");
        assert!(sentence.contains("nests deeper"), "{sentence}");
        // **One node deeper is refused before the walk reaches it**: a node
        // nests an object and a list, so 33 of them are 65 levels of JSON, past
        // the 64 `nc_canon::parse::MAX_DEPTH` reads. The walk's own bound is
        // what holds the tree when either number moves, so it is judged alone,
        // on a node it is handed past the bound.
        let mut walk = Walk {
            command: &registered,
            nodes: 0,
            inputs: BTreeSet::new(),
            binds: Vec::new(),
        };
        let divider = nc_canon::parse(r#"{"node":"divider"}"#).expect("a node");
        let Err(Fault { path, why }) =
            walk.node(&divider, "/children/0", u64::from(KERNEL.ui_tree_depth) + 1)
        else {
            panic!("a node past the depth is refused");
        };
        assert_eq!(path, "/children/0");
        assert_eq!(
            why,
            format!("nests deeper than the {} a tree may", KERNEL.ui_tree_depth)
        );
        let bound = usize::try_from(KERNEL.payload_bytes).expect("fits");
        let (_, code, sentence) = judged(&format!(
            r#"{{"node":"text","text":"{}"}}"#,
            "a".repeat(bound)
        ))
        .expect_err("past a payload");
        assert_eq!(code, ErrorCode::PluginUiInvalid);
        assert!(sentence.contains("bound of a payload"), "{sentence}");
    }

    #[test]
    fn an_interaction_names_a_registered_command_and_only_the_arguments_it_takes() {
        refused_at(
            r#"{"node":"button","label":"x","command":"nobody.nothing"}"#,
            "/command",
            "no command the kernel has registered",
        );
        refused_at(
            r#"{"node":"button","label":"x","command":"journal.verify","args":{"to":1}}"#,
            "/args",
            "no argument `journal.verify` takes",
        );
        assert!(
            judged(r#"{"node":"button","label":"x","command":"journal.verify","args":{"from":1}}"#)
                .is_ok()
        );
        refused_at(
            r#"{"node":"column","children":[{"node":"input","name":"to","label":"x"},{"node":"button","label":"x","command":"journal.verify","bind":["to"]}]}"#,
            "/children/1/bind/0",
            "no argument",
        );
        refused_at(
            r#"{"node":"button","label":"x","command":"journal.verify","bind":["from"]}"#,
            "/bind/0",
            "no input of the tree",
        );
        refused_at(
            r#"{"node":"column","children":[{"node":"input","name":"from","label":"x"},{"node":"button","label":"x","command":"journal.verify","args":{"from":1},"bind":["from"]}]}"#,
            "/children/1/bind/0",
            "gives already",
        );
        refused_at(
            r#"{"node":"button","label":"x","command":"journal.verify","bind":[1]}"#,
            "/bind/0",
            "is not the name of an input",
        );
        refused_at(
            r#"{"node":"toggle","label":"x","checked":"yes","command":"kernel.undo"}"#,
            "/checked",
            "is not what",
        );
    }

    #[test]
    fn each_input_is_named_once_and_as_a_word() {
        refused_at(
            r#"{"node":"column","children":[{"node":"input","name":"a","label":"x"},{"node":"input","name":"a","label":"y"}]}"#,
            "/children/1/name",
            "a second time",
        );
        for name in ["A", "1a", "a-b", "a b", ""] {
            refused_at(
                &format!(r#"{{"node":"input","name":"{name}","label":"x"}}"#),
                "/name",
                "is not an input's name",
            );
        }
        let longest = "a".repeat(64);
        assert!(
            judged(&format!(
                r#"{{"node":"input","name":"{longest}","label":"x"}}"#
            ))
            .is_ok()
        );
        refused_at(
            &format!(r#"{{"node":"input","name":"{longest}a","label":"x"}}"#),
            "/name",
            "is not an input's name",
        );
    }

    /// Opens a surface of `kind` titled `title` for `owner` and shows it at
    /// once, as a call tree that released it would: its identifier.
    fn shown_open(
        surfaces: &mut Surfaces,
        owner: PluginSlot,
        kind: SurfaceKind,
        title: &str,
    ) -> Result<String, Refusal> {
        let opened = surfaces.open(owner, kind, title, &[])?;
        let id = opened.id().to_owned();
        surfaces.shown(Shown::Opened(opened));
        Ok(id)
    }

    #[test]
    fn a_surface_is_its_owners_and_keeps_the_last_tree_kept() {
        let mut surfaces = Surfaces::default();
        let (one, two) = (PluginSlot(0), PluginSlot(1));
        let id = shown_open(&mut surfaces, one, SurfaceKind::Panel, "Notes").expect("opened");
        assert!(surfaces.holds(one, &id, &[]) && !surfaces.holds(two, &id, &[]));
        let foreign = surfaces
            .render(two, &id, "{}".to_owned())
            .expect_err("another's");
        assert_eq!(
            (foreign.kind(), foreign.code()),
            (RefusalKind::NotFound, ErrorCode::PluginNotFound)
        );
        assert_eq!(
            surfaces.close(two, &id).expect_err("another's").code(),
            ErrorCode::PluginNotFound
        );
        assert_eq!(
            surfaces.close(one, "surface-99").expect_err("none").code(),
            ErrorCode::PluginNotFound
        );
        surfaces
            .render(one, &id, r#"{"node":"divider"}"#.to_owned())
            .expect("its own");
        let listed = surfaces.listed();
        assert_eq!(listed.len(), 1);
        assert_eq!(
            (
                listed[0].id(),
                listed[0].owner(),
                listed[0].kind(),
                listed[0].title(),
                listed[0].tree()
            ),
            (
                id.as_str(),
                one,
                SurfaceKind::Panel,
                "Notes",
                Some(r#"{"node":"divider"}"#)
            )
        );
        surfaces.close(one, &id).expect("its own");
        assert_eq!(surfaces.listed(), [] as [crate::ui::Surface; 0]);
        let again = shown_open(&mut surfaces, one, SurfaceKind::Overlay, "Again").expect("opened");
        assert_ne!(again, id, "an identifier is never handed out twice");
        surfaces.forget(one);
        assert_eq!(surfaces.listed(), [] as [crate::ui::Surface; 0]);
    }

    #[test]
    fn a_plugin_holds_no_more_surfaces_than_its_bound() {
        let mut surfaces = Surfaces::default();
        for _ in 0..KERNEL.surfaces_per_plugin {
            shown_open(&mut surfaces, PluginSlot(0), SurfaceKind::Document, "A").expect("room");
        }
        let refused = surfaces
            .open(PluginSlot(0), SurfaceKind::Document, "A", &[])
            .expect_err("at the bound");
        assert_eq!(
            (refused.kind(), refused.code()),
            (RefusalKind::Unavailable, ErrorCode::PluginBoundReached)
        );
        assert!(
            surfaces
                .open(PluginSlot(1), SurfaceKind::Document, "A", &[])
                .is_ok(),
            "another's room"
        );
    }

    /// What a call tree holds and has not released is shown nowhere, and is
    /// counted and asked as if it were: a surface it opened is its plugin's
    /// and counts against its bound, one it closed is not its plugin's, and a
    /// tree it rendered is not shown — each until it is shown.
    #[test]
    fn what_a_tree_did_to_the_surfaces_is_counted_and_shown_only_once_released() {
        let mut surfaces = Surfaces::default();
        let (one, two) = (PluginSlot(0), PluginSlot(1));
        let shown = shown_open(&mut surfaces, one, SurfaceKind::Panel, "Shown").expect("room");
        let mut held = Vec::new();
        for _ in 1..KERNEL.surfaces_per_plugin {
            let refs: Vec<&Shown> = held.iter().collect();
            let opened = surfaces
                .open(one, SurfaceKind::Panel, "Held", &refs)
                .expect("room");
            held.push(Shown::Opened(opened));
        }
        let refs: Vec<&Shown> = held.iter().collect();
        assert_eq!(
            surfaces
                .open(one, SurfaceKind::Panel, "Past", &refs)
                .expect_err("what the tree holds counts against the bound")
                .code(),
            ErrorCode::PluginBoundReached
        );
        let Some(Shown::Opened(first)) = held.first() else {
            panic!("a surface was held open")
        };
        let first = first.id().to_owned();
        assert!(surfaces.holds(one, &first, &refs) && !surfaces.holds(two, &first, &refs));
        assert!(
            !surfaces.holds(one, &first, &[]),
            "not shown before it is released"
        );
        held.push(Shown::Closed {
            owner: one,
            id: shown.clone(),
        });
        held.push(Shown::Rendered {
            owner: one,
            id: first.clone(),
            tree: r#"{"node":"divider"}"#.to_owned(),
        });
        let refs: Vec<&Shown> = held.iter().collect();
        assert!(!surfaces.holds(one, &shown, &refs), "closed in the tree");
        assert!(surfaces.holds(one, &shown, &[]), "and shown until released");
        assert_eq!(surfaces.listed().len(), 1);
        for each in held {
            surfaces.shown(each);
        }
        let listed = surfaces.listed();
        assert_eq!(
            listed.len(),
            usize::try_from(KERNEL.surfaces_per_plugin - 1).expect("fits")
        );
        assert!(listed.iter().all(|surface| surface.id() != shown));
        assert_eq!(
            listed
                .iter()
                .find(|surface| surface.id() == first)
                .and_then(Surface::tree),
            Some(r#"{"node":"divider"}"#)
        );
    }

    #[test]
    fn a_surface_is_titled_as_a_person_can_read() {
        let most = nc_schema::descriptor::TITLE_MAX;
        assert!(titled(&"é".repeat(most)).is_ok(), "characters, not bytes");
        for title in [
            String::new(),
            "   ".to_owned(),
            "é".repeat(most + 1),
            "a\u{202e}b".to_owned(),
            "a\u{fe0f}b".to_owned(),
            "a\u{3164}b".to_owned(),
        ] {
            let refused = titled(&title).expect_err("unreadable");
            assert_eq!(refused.code(), ErrorCode::PluginUiInvalid, "{title:?}");
        }
        assert_eq!(SurfaceKind::Node3d.word(), "node-3d");
    }
}
