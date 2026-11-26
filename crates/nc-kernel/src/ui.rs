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
//! most `ui_text_bytes`, and counts characters. **An interaction names a command, and the kernel
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
//! **Who rendered each tree is kept with it** ([`Renderer`]): the plugin is
//! the surface's owner, and beside it the person the call tree acted for and,
//! when an agent's session acted for them — its command's tree, or a timer's
//! or an event's callback it put off — that agent. `surfaces.list` and
//! `surfaces.read` show it, reads on `Feature::"interface"`.
//!
//! **A person activates a node by a command, and the kernel reads what it
//! runs from the tree it kept** (`surfaces.activate`, [`Surfaces::activation`]): the
//! surface by its identifier and the node by the JSON pointer a refusal of the
//! tree names it by — `""` or `/` for the tree's root — a `button` or a
//! `toggle` that is not `disabled`, its command and its `args` read from the
//! kept tree and never from the caller, who gives only the text typed into
//! each input the node binds, read as the input's `kind` says. **A surface whose
//! kept tree an agent's call tree rendered is never activated**
//! (`[NC-KERN-0035]`): what it names would run with the person's whole
//! authority, chosen by the agent. Otherwise the command is dispatched nested in
//! the activation's own call tree, as the tree acts — through the decision
//! point as any dispatch, journaled, never confirmed, and refused there when it
//! runs only at the top of a call tree — and is a step of its own when it
//! changes state: the activation opens no unit of its own
//! ([`dispatches_what_it_shows`]) and is no step, and the command's unit is,
//! labelled by the command. A surface or a node not there is `[NC-KERN-0036]`,
//! and a node that runs nothing, an input it does not bind, or a text typed
//! into an input past its bounds — the input's own `max_length` in
//! characters, where it declares one, and `ui_text_bytes` in bytes, which the
//! command's schema cannot say, since its `maxLength` counts characters — is
//! `[NC-KERN-0037]`.
//!
//! What this answers (`NOCTUA.md §2.4`): a plugin drawing what the host does
//! not render — markup, a script, a remote image, a style — an interaction
//! that dispatches a command nothing registered or with arguments its command
//! does not take, a tree that grows the kernel without end, a plugin drawing
//! into another's surface, and a click on a tree an agent rendered run as the
//! person. What it does not answer: whether a tree a person's call tree
//! rendered was drawn from what an agent wrote before — a plugin rendering
//! from its own store, which an agent's command may have written, renders as
//! the person; a renderer that dispatches a node's command by its name rather
//! than through `surfaces.activate`, which is any client's to do as the
//! person, and a click is never a confirmation; and whether this
//! machine holds the blob an image names, which judging a tree does not ask —
//! a blob can arrive or be swept after the tree naming it is kept, so it is the
//! renderer's to ask where the blob is when it draws one.

use std::collections::{BTreeMap, BTreeSet};

use nc_authz::Principal;
use nc_canon::Value;
use nc_catalog::{ErrorCode, KERNEL};

use crate::dispatch::{Failure, Refusal, RefusalKind};
use crate::registry::Admitted;
use crate::session::Acting;
use crate::{Environment, PluginSlot, Shared};

/// The command that lists the surfaces open.
pub(crate) const LIST: &str = "surfaces.list";

/// The command that reads one surface, with the tree it shows.
pub(crate) const READ: &str = "surfaces.read";

/// The command that activates a node of a surface's tree: what a click or a
/// submit is.
pub(crate) const ACTIVATE: &str = "surfaces.activate";

/// The surface `surfaces.read` and `surfaces.activate` name.
const SURFACE: &str = "surface";

/// The node `surfaces.activate` names, as a JSON pointer into the tree.
const NODE: &str = "node";

/// What a person typed into each input the node binds.
const INPUTS: &str = "inputs";

/// An input's name, in `inputs`.
const INPUT_NAME: &str = "name";

/// What was typed into it, in `inputs`.
const INPUT_VALUE: &str = "value";

/// The most characters a surface's identifier is named by: `surface-` and a
/// `u64`'s twenty digits, with room.
pub(crate) const SURFACE_CHARS: usize = 64;

/// Every identifier [`Surfaces::open`] hands out fits [`SURFACE_CHARS`]:
/// `surface-` and the twenty digits of the largest `u64`.
const _: () = assert!("surface-".len() + 20 <= SURFACE_CHARS);

/// The most characters a node's pointer holds.
pub(crate) const NODE_CHARS: usize = 512;

/// How many digits a number below `bound` is written with, at most.
const fn digits(bound: u32) -> usize {
    let mut digits = 1;
    let mut left = bound / 10;
    while left > 0 {
        digits += 1;
        left /= 10;
    }
    digits
}

/// The pointer to the deepest node a tree may hold fits [`NODE_CHARS`]: each
/// level below the root `/children/` and an index below `ui_tree_nodes`.
const _: () = assert!(
    (KERNEL.ui_tree_depth as usize - 1) * ("/children/".len() + digits(KERNEL.ui_tree_nodes))
        <= NODE_CHARS
);

/// Whether `name` is the command that dispatches what a surface shows: the
/// one command of the kernel's own that changes state only through what it
/// dispatches, so it opens no unit of its own and leaves no note of having
/// left the machine — the command it dispatches is decided, noted and kept as
/// itself.
pub(crate) fn dispatches_what_it_shows(name: &str) -> bool {
    name == ACTIVATE
}

/// Who rendered a surface's tree: the person the call tree acted for, and the
/// agent that acted for them when an agent's session did. The plugin that
/// rendered it is the surface's owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Renderer {
    person: Principal,
    agent: Option<Principal>,
}

impl Renderer {
    /// Who a call tree acting as `acting` renders as.
    pub(crate) fn of(acting: &Acting) -> Self {
        Self {
            person: acting.whose_history().clone(),
            agent: acting.via_agent().then(|| acting.principal().clone()),
        }
    }

    /// The person the call tree acted for: who acted, or the person an
    /// agent's session acts for.
    #[must_use]
    pub const fn person(&self) -> &Principal {
        &self.person
    }

    /// The agent whose session acted, when one did.
    #[must_use]
    pub const fn agent(&self) -> Option<&Principal> {
        self.agent.as_ref()
    }

    /// What `surfaces.list` and `surfaces.read` show of it.
    fn shown(&self) -> Value {
        Value::object([
            ("person", Value::text(self.person.id().as_str())),
            (
                "agent",
                self.agent
                    .as_ref()
                    .map_or(Value::Null, |agent| Value::text(agent.id().as_str())),
            ),
        ])
    }
}

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
/// shown, its title, and the last tree rendered into it that the kernel kept,
/// with who rendered it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Surface {
    id: String,
    owner: PluginSlot,
    kind: SurfaceKind,
    title: String,
    tree: Option<String>,
    rendered_by: Option<Renderer>,
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

    /// Who rendered the tree it shows: none until one is kept.
    #[must_use]
    pub const fn rendered_by(&self) -> Option<&Renderer> {
        self.rendered_by.as_ref()
    }

    /// What `surfaces.list` shows of it, but the plugin that holds it, whose
    /// identity is the plugins' to say: everything but its tree.
    fn listed(&self) -> BTreeMap<String, Value> {
        [
            ("id", Value::text(self.id.as_str())),
            ("kind", Value::text(self.kind.word())),
            ("title", Value::text(self.title.as_str())),
            (
                "rendered_by",
                self.rendered_by
                    .as_ref()
                    .map_or(Value::Null, Renderer::shown),
            ),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect()
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
/// it gave the journal taken. **Never at once**: a surface opened, rendered
/// into and closed at once would let a read whose entry the journal refused —
/// its answer withheld — and a unit the journal did not keep each leave a
/// surface showing what they did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Shown {
    /// A surface opened, under the identifier its plugin was answered.
    Opened(Surface),
    /// A tree [`validated`] accepted, rendered into the surface `id` of the
    /// plugin in `owner` by a call tree acting as `by` says.
    Rendered {
        /// The plugin that rendered it.
        owner: PluginSlot,
        /// The surface it was rendered into.
        id: String,
        /// The tree, as canonical text.
        tree: String,
        /// Who the call tree that rendered it acted as.
        by: Renderer,
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
            rendered_by: None,
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
            rendered_by: None,
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
    /// of the plugin in `owner` shows, rendered `by` whom.
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
        by: Renderer,
    ) -> Result<(), Refusal> {
        match self.open.get_mut(id) {
            Some(surface) if surface.owner == owner => {
                surface.tree = Some(tree);
                surface.rendered_by = Some(by);
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
            Shown::Rendered {
                owner,
                id,
                tree,
                by,
            } => {
                let _closed_meanwhile = self.render(owner, &id, tree, by);
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

    /// What activating the node `node` of the surface `surface` runs, with
    /// `inputs` — each input's name and the text typed into it — given to the
    /// inputs the node binds.
    ///
    /// **Read from the tree the surface shows, never from the caller**: the
    /// command and its `args` are the node's, and the caller gives only the
    /// text of each input it binds, read as the input's `kind` says — an
    /// `integer`'s as an integer, every other as text.
    ///
    /// # Errors
    ///
    /// `denied` with `[NC-KERN-0035]` when an agent's call tree rendered the
    /// tree the surface shows, whatever the node; `not-found` with
    /// `[NC-KERN-0036]` for a surface not open, one that shows no tree yet,
    /// and a pointer that names nothing in the tree; and `invalid` with
    /// `[NC-KERN-0037]` for a node that is not a `button` or a `toggle`, one
    /// that is `disabled`, an input the node does not bind or named twice,
    /// and an `integer` input given what is not one.
    pub(crate) fn activation(
        &self,
        surface: &str,
        node: &str,
        inputs: &[(String, String)],
    ) -> Result<Activation, Failure> {
        let unknown = |why: String| {
            Failure::new(
                Refusal::new(RefusalKind::NotFound, ErrorCode::KernelSurfaceUnknown, why),
                "surface-unknown",
            )
        };
        let shown = quoted(surface);
        let Some(open) = self.open.get(surface) else {
            return Err(unknown(format!("no surface {shown} is open")));
        };
        if open
            .rendered_by
            .as_ref()
            .and_then(Renderer::agent)
            .is_some()
        {
            return Err(Failure::new(
                Refusal::new(
                    RefusalKind::Denied,
                    ErrorCode::KernelSurfaceByAnAgent,
                    format!(
                        "the tree the surface {shown} shows was rendered by an agent's call tree, \
                         so nothing it names is run for the person"
                    ),
                ),
                "rendered-by-an-agent",
            ));
        }
        let Some(tree) = open
            .tree
            .as_deref()
            .and_then(|tree| nc_canon::parse(tree).ok())
        else {
            return Err(unknown(format!("the surface {shown} shows no tree yet")));
        };
        let pointer = quoted(node);
        let Some(Value::Object(fields)) = at(&tree, node) else {
            return Err(unknown(format!(
                "{pointer} names no node of the tree the surface {shown} shows"
            )));
        };
        let refused = |why: String| {
            Failure::new(
                Refusal::new(RefusalKind::Invalid, ErrorCode::KernelNotActivatable, why),
                "not-activatable",
            )
        };
        let kind = match fields.get("node") {
            Some(Value::Text(kind)) => kind.as_str(),
            _ => "",
        };
        let Some(Value::Text(command)) = fields.get("command").filter(|_| ACTIVE.contains(&kind))
        else {
            return Err(refused(format!(
                "the node at {pointer} is a `{kind}`, which runs nothing: only a `button` or a \
                 `toggle` is activated"
            )));
        };
        if fields.get("disabled") == Some(&Value::Bool(true)) {
            return Err(refused(format!("the node at {pointer} is disabled")));
        }
        let mut args = match fields.get("args") {
            Some(Value::Object(args)) => args.clone(),
            _ => BTreeMap::new(),
        };
        let binds = binds_of(fields);
        for (name, typed) in inputs {
            let input = quoted(name);
            if !binds.contains(&name.as_str()) {
                return Err(refused(format!(
                    "the node at {pointer} binds no input {input}"
                )));
            }
            within_bounds(&tree, name, typed).map_err(refused)?;
            let value = if input_kind(&tree, name) == Some("integer") {
                Value::Integer(typed.parse().map_err(|_| {
                    refused(format!(
                        "the input {input} takes an integer, and {} is not one",
                        quoted(typed)
                    ))
                })?)
            } else {
                Value::text(typed.as_str())
            };
            if args.insert(name.clone(), value).is_some() {
                return Err(refused(format!("the input {input} is given twice")));
            }
        }
        Ok(Activation {
            command: command.clone(),
            args: Value::Object(args).to_canonical(),
        })
    }
}

/// The kinds of node a person activates.
const ACTIVE: [&str; 2] = ["button", "toggle"];

/// What activating a node runs: the command its tree names, and the
/// arguments' canonical text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Activation {
    /// The command.
    pub(crate) command: String,
    /// Its arguments, as canonical JSON.
    pub(crate) args: String,
}

/// `text` as a refusal of the surfaces repeats it.
fn quoted(text: &str) -> String {
    crate::registry::quoted_up_to(text, QUOTED)
}

/// The node of `tree` the JSON pointer `pointer` names: the root for `""` or
/// `/`, which no node's key can be, and otherwise each segment a key of an
/// object or the index of a list, `~1` read as `/` and `~0` as `~`.
fn at<'a>(tree: &'a Value, pointer: &str) -> Option<&'a Value> {
    if pointer.is_empty() || pointer == "/" {
        return Some(tree);
    }
    let mut here = tree;
    for segment in pointer.strip_prefix('/')?.split('/') {
        let key = segment.replace("~1", "/").replace("~0", "~");
        here = match here {
            Value::Object(fields) => fields.get(&key)?,
            Value::Array(items) => {
                let index = (key == "0"
                    || (!key.starts_with('0') && key.bytes().all(|byte| byte.is_ascii_digit())))
                .then(|| key.parse::<usize>().ok())
                .flatten()?;
                items.get(index)?
            }
            _ => return None,
        };
    }
    Some(here)
}

/// The names of the inputs the node `fields` binds.
fn binds_of(fields: &BTreeMap<String, Value>) -> Vec<&str> {
    match fields.get("bind") {
        Some(Value::Array(names)) => names
            .iter()
            .filter_map(|name| match name {
                Value::Text(name) => Some(name.as_str()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Whether `typed`, typed into the input named `name` in `tree`, is within its bounds.
///
/// Its bounds are `ui_text_bytes` in bytes, which a schema's `maxLength`
/// cannot say since it counts characters, and the input's own `max_length` in
/// characters where it declares one; otherwise the sentence that says why not.
fn within_bounds(tree: &Value, name: &str, typed: &str) -> Result<(), String> {
    let input = quoted(name);
    if u64::try_from(typed.len()).unwrap_or(u64::MAX) > KERNEL.ui_text_bytes {
        return Err(format!(
            "the text typed into the input {input} is {} bytes, past the {} a text may be",
            typed.len(),
            KERNEL.ui_text_bytes
        ));
    }
    if let Some(most) = input_max_length(tree, name)
        && typed.chars().count() > most
    {
        return Err(format!(
            "the input {input} takes at most {most} characters, and {} were typed",
            typed.chars().count()
        ));
    }
    Ok(())
}

/// The `kind` of the input named `name` in `tree` — `text` when it names
/// none — or none when no input of the tree is named so.
fn input_kind<'a>(tree: &'a Value, name: &str) -> Option<&'a str> {
    input_named(tree, name).map(|fields| match fields.get("kind") {
        Some(Value::Text(kind)) => kind.as_str(),
        _ => "text",
    })
}

/// The most characters the input named `name` in `tree` takes.
///
/// Its `max_length`, or none where it declares none
/// or no input is named so.
fn input_max_length(tree: &Value, name: &str) -> Option<usize> {
    match input_named(tree, name)?.get("max_length") {
        Some(Value::Integer(most)) => usize::try_from(*most).ok(),
        _ => None,
    }
}

/// The fields of the input named `name` in `tree`, or none when no input of
/// the tree is named so.
fn input_named<'a>(tree: &'a Value, name: &str) -> Option<&'a BTreeMap<String, Value>> {
    match tree {
        Value::Object(fields) => {
            let is_it = fields.get("node") == Some(&Value::text("input"))
                && fields.get("name") == Some(&Value::text(name));
            if is_it {
                return Some(fields);
            }
            ["children", "items"]
                .iter()
                .filter_map(|key| fields.get(*key))
                .find_map(|nodes| input_named(nodes, name))
        }
        Value::Array(nodes) => nodes.iter().find_map(|node| input_named(node, name)),
        _ => None,
    }
}

/// Each input `args` gives `surfaces.activate`, by its name, with the text
/// typed into it.
fn inputs_of(args: &serde_json::Value) -> Vec<(String, String)> {
    args.get(INPUTS)
        .and_then(serde_json::Value::as_array)
        .map(|inputs| {
            inputs
                .iter()
                .filter_map(|input| {
                    let text = |key| input.get(key).and_then(serde_json::Value::as_str);
                    Some((text(INPUT_NAME)?.to_owned(), text(INPUT_VALUE)?.to_owned()))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The text `args` holds under `key`, or none.
fn text_of<'a>(args: &'a serde_json::Value, key: &str) -> &'a str {
    args.get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
}

/// `surfaces.list`: every surface open, in the order their identifiers sort.
///
/// `{"surfaces":[…]}`, each with its `id`, its `kind`, its `title`, the
/// identity of the `plugin` that holds it, and `rendered_by` — the `person`
/// and the `agent`, or `null`, of the call tree that rendered the tree it
/// shows — `null` until one is kept. Never the tree, which `surfaces.read`
/// answers one surface at a time.
pub(crate) fn listed<E: Environment>(shared: &Shared<E>) -> String {
    // **The surfaces' lock let go before the plugins' is taken**, which an
    // unload takes in the other order.
    let open: Vec<(PluginSlot, BTreeMap<String, Value>)> = crate::dispatch::lock(&shared.surfaces)
        .open
        .values()
        .map(|surface| (surface.owner, surface.listed()))
        .collect();
    let shown = open
        .into_iter()
        .map(|(owner, fields)| Value::Object(with_its_plugin(shared, owner, fields)))
        .collect();
    Value::object([("surfaces", Value::Array(shown))]).to_canonical()
}

/// `fields`, with the identity of the plugin in `owner` as their `plugin`:
/// `null` for one no longer loaded.
fn with_its_plugin<E: Environment>(
    shared: &Shared<E>,
    owner: PluginSlot,
    mut fields: BTreeMap<String, Value>,
) -> BTreeMap<String, Value> {
    fields.insert(
        "plugin".to_owned(),
        shared.plugin_of(owner).map_or(Value::Null, Value::text),
    );
    fields
}

/// `surfaces.read`: the surface `args` names, as `surfaces.list` shows it and
/// with its `tree`, `null` until one is kept.
///
/// # Errors
///
/// `not-found` with `[NC-KERN-0036]` when no surface of that identifier is
/// open.
pub(crate) fn read<E: Environment>(
    shared: &Shared<E>,
    args: &serde_json::Value,
) -> Result<String, Failure> {
    let id = text_of(args, SURFACE);
    let found = crate::dispatch::lock(&shared.surfaces)
        .open
        .get(id)
        .cloned();
    let Some(surface) = found else {
        return Err(Failure::new(
            Refusal::new(
                RefusalKind::NotFound,
                ErrorCode::KernelSurfaceUnknown,
                format!("no surface {} is open", quoted(id)),
            ),
            "surface-unknown",
        ));
    };
    let mut fields = with_its_plugin(shared, surface.owner, surface.listed());
    fields.insert(
        "tree".to_owned(),
        surface
            .tree
            .as_deref()
            .and_then(|tree| nc_canon::parse(tree).ok())
            .unwrap_or(Value::Null),
    );
    Ok(Value::Object(fields).to_canonical())
}

/// `surfaces.activate`: the node `args` names of the surface it names,
/// activated — the command the tree names dispatched nested in `tree`, as it
/// acts, with the arguments [`Surfaces::activation`] reads, and answered as
/// it answers.
///
/// # Errors
///
/// What [`Surfaces::activation`] refuses, and what the decision point or the
/// command refuses the dispatch with, its code and its kind kept.
pub(crate) async fn activated<E: Environment>(
    shared: &std::sync::Arc<Shared<E>>,
    tree: &std::sync::Arc<std::sync::Mutex<crate::dispatch::CallTree>>,
    args: &serde_json::Value,
) -> Result<String, Failure> {
    let activation = crate::dispatch::lock(&shared.surfaces).activation(
        text_of(args, SURFACE),
        text_of(args, NODE),
        &inputs_of(args),
    )?;
    crate::dispatch::nested(shared, tree, &activation.command, &activation.args, None)
        .await
        .map_err(|refusal| Failure::new(refusal, "activated-refused"))
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

    use nc_authz::{Principal, PrincipalKind};

    use super::{
        ACTIVATE, Activation, Fault, INPUT_NAME, INPUT_NAME_MAX, INPUT_VALUE, INPUTS, LIST, NODE,
        NODE_CHARS, READ, Renderer, SURFACE, SURFACE_CHARS, Shown, Surface, SurfaceKind, Surfaces,
        Walk, dispatches_what_it_shows, titled, validated,
    };
    use crate::PluginSlot;
    use crate::dispatch::{Refusal, RefusalKind};
    use crate::registry::{Admitted, Registry};
    use crate::session::{Acting, Session};

    /// The world the policy cases are about.
    fn world() -> nc_authz::MemoryDirectory {
        let pdp = nc_authz::Pdp::embedded("family").expect("the family pack is carried");
        nc_authz::MemoryDirectory::from_world(include_str!("../../../policy/world.json"), &pdp)
            .expect("the world loads")
    }

    /// `ana`, the person the world names first, and an agent of hers.
    fn ana_and_her_agent() -> (Principal, Principal) {
        let world = world();
        let of = |kind, id: &str| {
            world
                .principal(kind, id)
                .unwrap_or_else(|| panic!("the world holds {id}"))
        };
        (
            of(PrincipalKind::User, "ana"),
            of(PrincipalKind::Agent, "ana-assistant"),
        )
    }

    /// Who a call tree of `ana`'s own renders as.
    fn by_ana() -> Renderer {
        Renderer::of(&Acting::Itself(ana_and_her_agent().0))
    }

    /// Who a call tree of her agent's session renders as.
    fn by_her_agent() -> Renderer {
        let (ana, agent) = ana_and_her_agent();
        Renderer::of(&Acting::Agent(Session::opened_for(agent, ana)))
    }

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
        // **And a default-ignorable point**, drawn as nothing: without it,
        // variation selectors would carry bytes unseen past this rule.
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
            .render(two, &id, "{}".to_owned(), by_ana())
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
            .render(one, &id, r#"{"node":"divider"}"#.to_owned(), by_ana())
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
            by: by_her_agent(),
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
        assert_eq!(
            listed
                .iter()
                .find(|surface| surface.id() == first)
                .and_then(Surface::rendered_by),
            Some(&by_her_agent()),
            "a tree is shown with who rendered it"
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

    /// A surface keeps who rendered the tree it shows: the person a call tree
    /// acted for and, for an agent's session, the agent — the last tree's,
    /// never the first's.
    #[test]
    fn a_surface_keeps_who_rendered_the_tree_it_shows() {
        let (ana, agent) = ana_and_her_agent();
        let (person, agents) = (by_ana(), by_her_agent());
        assert_eq!((person.person(), person.agent()), (&ana, None));
        assert_eq!((agents.person(), agents.agent()), (&ana, Some(&agent)));
        let mut surfaces = Surfaces::default();
        let id =
            shown_open(&mut surfaces, PluginSlot(0), SurfaceKind::Panel, "Notes").expect("open");
        assert_eq!(
            surfaces.listed()[0].rendered_by(),
            None,
            "none until a tree is"
        );
        for by in [by_her_agent(), by_ana(), by_her_agent()] {
            surfaces
                .render(
                    PluginSlot(0),
                    &id,
                    r#"{"node":"divider"}"#.to_owned(),
                    by.clone(),
                )
                .expect("its own");
            assert_eq!(surfaces.listed()[0].rendered_by(), Some(&by));
        }
        let listed = surfaces.listed()[0].listed();
        assert_eq!(
            nc_canon::Value::Object(listed).to_canonical(),
            format!(
                r#"{{"id":"{id}","kind":"panel","rendered_by":{{"agent":"{}","person":"{}"}},"title":"Notes"}}"#,
                agent.id().as_str(),
                ana.id().as_str()
            )
        );
    }

    /// A surface open in `surfaces`, owned by the first plugin, showing
    /// `tree` as rendered `by` whom: its identifier.
    fn showing(surfaces: &mut Surfaces, tree: &str, by: Renderer) -> String {
        let id = shown_open(surfaces, PluginSlot(0), SurfaceKind::Panel, "Form").expect("open");
        let kept = validated(tree, &registered).expect("a tree the kernel keeps");
        surfaces
            .render(PluginSlot(0), &id, kept, by)
            .expect("its own");
        id
    }

    /// A form: a text input, an integer input and a button binding both,
    /// under fixed arguments of its own, beside a disabled button and a text.
    const FORM: &str = r#"{"node":"column","children":[{"node":"input","name":"title","label":"Title"},{"node":"input","name":"from","label":"From","kind":"integer"},{"node":"button","label":"Go","command":"naming.propose","args":{"extension":"txt"},"bind":["title"]},{"node":"button","label":"Verify","command":"journal.verify","bind":["from"]},{"node":"button","label":"Off","command":"kernel.undo","disabled":true},{"node":"text","text":"x"},{"node":"toggle","label":"T","checked":true,"command":"kernel.find","args":{"query":"a/b"}}]}"#;

    /// The refusal `activated` is, by its kind, its code and its word.
    fn refused_as(
        activated: Result<Activation, crate::dispatch::Failure>,
    ) -> (RefusalKind, nc_catalog::ErrorCode, String) {
        let failure = activated.expect_err("refused");
        (
            failure.refusal().kind(),
            failure.refusal().code(),
            failure.word().to_owned(),
        )
    }

    /// What a node runs is read from the tree the surface shows: its command
    /// and its arguments, each input it binds given as its kind reads it.
    #[test]
    fn what_a_node_runs_is_read_from_the_tree_and_its_inputs_as_their_kind_reads_them() {
        let mut surfaces = Surfaces::default();
        let id = showing(&mut surfaces, FORM, by_ana());
        let typed = |name: &str, value: &str| (name.to_owned(), value.to_owned());
        assert_eq!(
            surfaces
                .activation(&id, "/children/2", &[typed("title", "A note")])
                .expect("a button"),
            Activation {
                command: "naming.propose".to_owned(),
                args: r#"{"extension":"txt","title":"A note"}"#.to_owned(),
            }
        );
        assert_eq!(
            surfaces
                .activation(&id, "/children/3", &[typed("from", "12")])
                .expect("an integer input"),
            Activation {
                command: "journal.verify".to_owned(),
                args: r#"{"from":12}"#.to_owned(),
            }
        );
        assert_eq!(
            surfaces
                .activation(&id, "/children/2", &[])
                .expect("an input left out is the command's schema's to judge")
                .args,
            r#"{"extension":"txt"}"#
        );
        assert_eq!(
            surfaces
                .activation(&id, "/children/6", &[])
                .expect("a toggle")
                .command,
            "kernel.find"
        );
    }

    /// What a node does not run, and what names no node, is refused before
    /// anything is dispatched: a node that runs nothing, one disabled, an
    /// input it does not bind, given twice or not of its kind, each with
    /// `[NC-KERN-0037]`; a surface not open, one with no tree, and a pointer
    /// that names nothing, each with `[NC-KERN-0036]`.
    #[test]
    fn what_a_node_does_not_run_and_what_names_no_node_are_refused() {
        let mut surfaces = Surfaces::default();
        let id = showing(&mut surfaces, FORM, by_ana());
        let typed = |name: &str, value: &str| (name.to_owned(), value.to_owned());
        for (node, inputs, why) in [
            ("/children/5", vec![], "a text runs nothing"),
            ("", vec![], "the root, a column, runs nothing"),
            ("/", vec![], "the root, a column, runs nothing"),
            ("/children/4", vec![], "a disabled button"),
            (
                "/children/2",
                vec![typed("from", "1")],
                "an input it does not bind",
            ),
            (
                "/children/3",
                vec![typed("from", "twelve")],
                "an integer that is not one",
            ),
            (
                "/children/2",
                vec![typed("title", "a"), typed("title", "b")],
                "an input given twice",
            ),
            (
                "/children/2",
                vec![typed("extension", "md")],
                "its own argument, not an input",
            ),
        ] {
            assert_eq!(
                refused_as(surfaces.activation(&id, node, &inputs)),
                (
                    RefusalKind::Invalid,
                    nc_catalog::ErrorCode::KernelNotActivatable,
                    "not-activatable".to_owned()
                ),
                "{why}"
            );
        }
        for (surface, node, why) in [
            ("surface-99", "/children/2", "a surface not open"),
            (id.as_str(), "/children/9", "an index past the list"),
            (
                id.as_str(),
                "/children/02",
                "an index written with a leading zero",
            ),
            (id.as_str(), "/children/-1", "an index that is not one"),
            (id.as_str(), "/items/0", "a key the node does not hold"),
            (
                id.as_str(),
                "children/2",
                "a pointer without its leading slash",
            ),
            (id.as_str(), "/children/2/label/x", "past a text"),
        ] {
            assert_eq!(
                refused_as(surfaces.activation(surface, node, &[])),
                (
                    RefusalKind::NotFound,
                    nc_catalog::ErrorCode::KernelSurfaceUnknown,
                    "surface-unknown".to_owned()
                ),
                "{why}"
            );
        }
        let bare =
            shown_open(&mut surfaces, PluginSlot(0), SurfaceKind::Panel, "Bare").expect("open");
        assert_eq!(
            refused_as(surfaces.activation(&bare, "", &[])).1,
            nc_catalog::ErrorCode::KernelSurfaceUnknown,
            "a surface that shows no tree yet"
        );
        // **Only a button or a toggle, whatever a node holds**: no kind but
        // those two may hold a `command` today, which `validated` refuses, so
        // the tree is kept as a renderer would be handed one a later kind
        // made — and its command is still not run.
        surfaces
            .render(
                PluginSlot(0),
                &bare,
                r#"{"command":"kernel.find","label":"x","name":"a","node":"input"}"#.to_owned(),
                by_ana(),
            )
            .expect("its own");
        assert_eq!(
            refused_as(surfaces.activation(&bare, "", &[])).1,
            nc_catalog::ErrorCode::KernelNotActivatable,
            "a node of another kind holding a command"
        );
    }

    /// What is typed into an input is held to the input's own `max_length`
    /// in characters and to `ui_text_bytes` in bytes, before anything is
    /// dispatched.
    ///
    /// **Characters for the one, bytes for the other**: eight characters fit
    /// an input of `max_length` 8 whatever bytes they take, and a ninth is
    /// `[NC-KERN-0037]`; an input that declares none takes `ui_text_bytes`
    /// bytes, and one byte more is refused — so is a text of fewer characters
    /// than that taking more bytes, which the command's schema, counting
    /// characters, lets through.
    #[test]
    fn what_is_typed_is_held_to_its_inputs_length_and_to_a_texts_bytes() {
        /// A tree whose button binds an input of `max_length` 8 and one that
        /// declares none.
        const BOUNDED: &str = r#"{"node":"column","children":[{"node":"input","name":"title","label":"Title","max_length":8},{"node":"input","name":"extension","label":"Extension","kind":"multiline"},{"node":"button","label":"Go","command":"naming.propose","bind":["title","extension"]}]}"#;
        let mut surfaces = Surfaces::default();
        let id = showing(&mut surfaces, BOUNDED, by_ana());
        let typed = |name: &str, value: String| vec![(name.to_owned(), value)];
        let most = usize::try_from(KERNEL.ui_text_bytes).expect("a count");
        for (inputs, why) in [
            (typed("title", "abcdefgh".to_owned()), "eight characters"),
            (
                typed("title", "é".repeat(8)),
                "eight characters of two bytes",
            ),
            (
                typed("extension", "a".repeat(most)),
                "as many bytes as a text may be",
            ),
        ] {
            assert!(
                surfaces.activation(&id, "/children/2", &inputs).is_ok(),
                "{why}"
            );
        }
        for (inputs, why) in [
            (typed("title", "abcdefghi".to_owned()), "a ninth character"),
            (
                typed("extension", "a".repeat(most + 1)),
                "one byte past a text's",
            ),
            (
                typed("extension", "é".repeat(most / 2 + 1)),
                "fewer characters than a text's bytes, taking more bytes",
            ),
        ] {
            assert_eq!(
                refused_as(surfaces.activation(&id, "/children/2", &inputs)),
                (
                    RefusalKind::Invalid,
                    nc_catalog::ErrorCode::KernelNotActivatable,
                    "not-activatable".to_owned()
                ),
                "{why}"
            );
        }
    }

    /// A pointer's segments are read as JSON pointers write them: `~1` is
    /// `/` and `~0` is `~`, in that order.
    #[test]
    fn a_pointer_reads_its_escapes_as_json_pointers_write_them() {
        let tree = nc_canon::parse(r#"{"a/b":{"~1":[0,{"node":"divider"}]}}"#).expect("a value");
        assert_eq!(
            super::at(&tree, "/a~1b/~01/1"),
            Some(&nc_canon::parse(r#"{"node":"divider"}"#).expect("a node"))
        );
        assert_eq!(
            super::at(&tree, "/a~1b/~01/0"),
            Some(&nc_canon::Value::Integer(0))
        );
        assert_eq!(super::at(&tree, "/a/b"), None);
        assert_eq!(super::at(&tree, ""), Some(&tree));
    }

    /// A surface whose tree an agent's call tree rendered is never activated,
    /// whatever its node — and is again once the person's tree renders it.
    #[test]
    fn a_surface_an_agent_rendered_is_never_activated() {
        let mut surfaces = Surfaces::default();
        let id = showing(&mut surfaces, FORM, by_her_agent());
        for node in ["/children/2", "/children/5", "/children/9", ""] {
            assert_eq!(
                refused_as(surfaces.activation(&id, node, &[])),
                (
                    RefusalKind::Denied,
                    nc_catalog::ErrorCode::KernelSurfaceByAnAgent,
                    "rendered-by-an-agent".to_owned()
                ),
                "{node}"
            );
        }
        let kept = validated(FORM, &registered).expect("kept");
        surfaces
            .render(PluginSlot(0), &id, kept, by_ana())
            .expect("its own");
        assert!(surfaces.activation(&id, "/children/2", &[]).is_ok());
    }

    /// The activation alone dispatches what a surface shows, which opens no
    /// unit of its own and leaves no note of having left the machine.
    #[test]
    fn only_the_activation_dispatches_what_a_surface_shows() {
        assert!(dispatches_what_it_shows(ACTIVATE));
        for name in [
            LIST,
            READ,
            "kernel.undo",
            "settings.mcp.set",
            "surfaces.other",
        ] {
            assert!(!dispatches_what_it_shows(name), "`{name}`");
        }
    }

    /// The declared surfaces commands are the ones this module implements: the
    /// arguments each reads, both ways, and the decision each is made under —
    /// on the interface's feature, the two reads changing nothing and the
    /// activation a write.
    #[test]
    fn the_declared_surfaces_commands_are_the_ones_this_module_implements() {
        for (name, reads, action, mutating) in [
            (LIST, &[][..], "Read", false),
            (READ, &[SURFACE][..], "Read", false),
            (ACTIVATE, &[SURFACE, NODE, INPUTS][..], "Write", true),
        ] {
            let declared =
                nc_catalog::command(name).unwrap_or_else(|| panic!("`{name}` is declared"));
            let takes: BTreeSet<&str> = declared.arguments.iter().copied().collect();
            let read: BTreeSet<&str> = reads.iter().copied().collect();
            assert_eq!(takes, read, "`{name}` declares {:?}", declared.arguments);
            assert_eq!(declared.arguments.len(), reads.len(), "`{name}`");
            assert_eq!(
                (
                    declared.action,
                    declared.resource_selector,
                    declared.mutating
                ),
                (action, "feature:interface", mutating),
                "`{name}`"
            );
            assert!(
                matches!(
                    (declared.reversibility, mutating),
                    (nc_catalog::Reversibility::Read, false)
                        | (nc_catalog::Reversibility::Write, true)
                ),
                "`{name}`"
            );
        }
        assert_eq!(
            (
                LIST,
                READ,
                ACTIVATE,
                SURFACE,
                NODE,
                INPUTS,
                INPUT_NAME,
                INPUT_VALUE
            ),
            (
                "surfaces.list",
                "surfaces.read",
                "surfaces.activate",
                "surface",
                "node",
                "inputs",
                "name",
                "value"
            )
        );
    }

    /// Every bound the surfaces' schemas declare is the one this module and
    /// the catalog hold.
    ///
    /// A schema's `maxLength` counts characters: what is typed into an input
    /// is declared at most `ui_text_bytes` characters there, and held to that
    /// many bytes by `activation`
    /// (`what_is_typed_is_held_to_its_inputs_length_and_to_a_texts_bytes`).
    #[test]
    fn every_bound_the_surfaces_schemas_declare_is_the_one_held_here() {
        let schema = |name: &str| -> serde_json::Value {
            serde_json::from_str(nc_catalog::command(name).expect("declared").args_schema)
                .expect("a schema is JSON")
        };
        for name in [READ, ACTIVATE] {
            assert_eq!(
                schema(name)["properties"][SURFACE]["maxLength"].as_u64(),
                Some(SURFACE_CHARS as u64),
                "`{name}`"
            );
        }
        let activate = schema(ACTIVATE);
        let properties = &activate["properties"];
        let input = &properties[INPUTS]["items"]["properties"];
        assert_eq!(
            (
                properties[NODE]["maxLength"].as_u64(),
                properties[INPUTS]["maxItems"].as_u64(),
                input[INPUT_NAME]["maxLength"].as_u64(),
                input[INPUT_VALUE]["maxLength"].as_u64(),
            ),
            (
                Some(NODE_CHARS as u64),
                Some(u64::from(KERNEL.ui_tree_nodes)),
                Some(INPUT_NAME_MAX as u64),
                Some(KERNEL.ui_text_bytes),
            )
        );
        assert!(
            format!("surface-{}", u64::MAX).len() <= SURFACE_CHARS,
            "every identifier the kernel hands out fits"
        );
    }
}
