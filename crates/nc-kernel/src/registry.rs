//! The commands the kernel can dispatch: its own, and every one an activated
//! plugin declared and the kernel admitted.
//!
//! **The kernel's own come from the catalog.** Every command of
//! `catalog/commands.toml` whose namespace is one the kernel keeps —
//! [`RESERVED`]: `journal.`, `kernel.`, `limits.`, `naming.` — is a built-in,
//! registered when the registry is made. A catalog command in any other
//! namespace is one the catalog describes for a plugin to implement
//! (`NOCTUA.md §15.2`): it is registered when a plugin declares it, matching
//! the catalog in every field a dispatch is decided by, and is not registered
//! otherwise. The title is the one field not compared: such a command is shown
//! by the catalog's entry, as every command of the kernel's own is.
//!
//! **Every command is listed with its entry** ([`CommandEntry`]): its display
//! name, line description, paragraph and tags in every locale, its categories
//! and its glyph — the catalog's, resolved from its keys, for a command the
//! catalog writes, and the plugin's own for one it does not.
//!
//! **A plugin's commands are judged at `activate`, whole** (`NOCTUA.md §11.7`),
//! and every problem is reported rather than the first, each with its own code:
//!
//! - each command by `nc_schema::descriptor::judge`, the judge the catalog
//!   compiler holds every built-in to: a field left empty is
//!   `[NC-PLUG-0008]`, naming the command — or, when it is the name that is
//!   missing, its position — and the field; anything else the judge refuses is
//!   `[NC-PLUG-0009]`;
//! - each name by the rules a namespace is owned by, `[NC-KERN-0015]`: not in
//!   a namespace the kernel keeps, inside the plugin's own (`<namespace>.`),
//!   declared once, not already registered, and, when the catalog describes
//!   it, matching the catalog;
//! - each action and selector by the decision point's schema,
//!   `[NC-PLUG-0009]`: no feature of the platform at all — a `feature:`
//!   selector is the kernel's own commands' — an action the schema declares, a
//!   resource type that action applies to, and the whole realm selected only by
//!   a command that reads — a realm-wide change is the kernel's;
//! - each command whose name is its plugin's to have by its entry,
//!   `[NC-PLUG-0029]`: the entries the manifest is described by
//!   (`crate::entries`) hold a row for the command, under its name inside the
//!   namespace, unless the catalog describes it — then they hold none, since
//!   the catalog's is the one entry; the row is held to
//!   `nc_schema::entry::judge`, the judge every command of the catalog is held
//!   to, its texts in every locale — a text missing from one is refused naming
//!   the command and the locale; a declaration the descriptor judge admitted
//!   declares as its title the entry's English display name, so the component
//!   and its directory cannot say two things; and no display name, in any
//!   locale, is one a command of the catalog shows in any locale, compared by
//!   `the_same_sentence`. A row no command claims is neither registered nor
//!   refused;
//! - each subscription by the manifest, `[NC-PLUG-0009]`: one its
//!   `[events] subscribe` declares, once;
//! - and the count by the catalog: more than `commands_per_plugin` commands is
//!   `[NC-PLUG-0009]`, refused before any of them is judged — a declaration
//!   refused like every other here, so the person is told the plugin was not
//!   started, and not `[NC-PLUG-0017]`, whose message says only that a limit
//!   was reached and nothing of a plugin left unstarted.
//!
//! What this answers (`NOCTUA.md §2.4`): command squatting — a plugin cannot
//! register a command in a namespace it was not given, in one the kernel keeps,
//! or under a name already registered — and a command nothing can authorize,
//! which never reaches the registry. A plugin cannot describe a command that
//! is not its own, nor show one of its own under a name a command of the
//! catalog is shown by — `Undo`, in a list beside the kernel's — and what it
//! says of its commands is judged as the catalog's is before anything shows
//! it. A plugin that would select the realm to
//! change it is refused, because a permit for that action on the realm would
//! then authorize every such command of every plugin; and one that would select
//! a feature of the platform is refused, because the permit for that feature is
//! a person's permit for the kernel's own command — a plugin selecting it would
//! borrow it, and the journal's entity would no longer say which command ran.
//! What it does not answer:
//! whether a plugin's command acts on the resource it was authorized for,
//! inside its own namespace, which is the plugin's code and no check here.

use std::collections::BTreeMap;

use nc_authz::Pdp;
use nc_catalog::{ErrorCode, KERNEL, Locale};
use nc_schema::descriptor::{self, DescriptorProblem, Judged, Reversibility};
use nc_schema::entry::{self, Rules, Text};
use nc_schema::selector::{REALM_TYPE, Selector};

use crate::manifest::PluginManifest;
use crate::{LoadProblem, PluginSlot};

/// The namespaces the kernel keeps for its own commands.
pub const RESERVED: [&str; 4] = ["journal", "kernel", "limits", "naming"];

/// The one action a plugin's command may select the whole realm under.
const THE_REALM_IS_READ_WITH: &str = "Read";

/// How many characters of an action, an entity type or a topic a refusal repeats.
///
/// A command's name is not cut: a refusal repeats it only when it is well
/// formed, which holds it to ASCII and to `NAME_MAX` characters, and a name
/// cut short would be one the plugin never declared. A name a plugin's entries
/// hold is cut to it as well (`crate::entries`), since one the grammar refuses
/// can be any text.
pub(crate) const QUOTED: usize = 64;

/// Who runs a registered command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    /// The kernel itself: a command of the catalog in a namespace it keeps.
    Kernel,
    /// The plugin loaded in this slot.
    Plugin(PluginSlot),
}

/// The categories a plugin's entry may name.
///
/// **None yet, as for the catalog's commands**: the categories registry is
/// what a name is held to, and until the catalog carries it every category an
/// entry names is one no list can show it under, which the judge refuses.
const CATEGORIES: [&str; 0] = [];

/// What a command shows a person, in every locale (`NOCTUA.md §20.4`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandEntry {
    /// Its texts in each locale, in `Locale::ALL`'s order.
    texts: Vec<EntryTexts>,
    /// The categories it is listed under, by name, as its entry names them.
    pub categories: Vec<String>,
    /// The glyph it is shown with, a code point the Symbols-Only font draws.
    pub glyph: Option<char>,
}

/// What a command shows a person in one locale.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EntryTexts {
    /// The name a person reads first.
    pub display_name: String,
    /// One line saying what it does.
    pub line: String,
    /// A paragraph saying more.
    pub paragraph: String,
    /// The words a search finds it by, synonyms included.
    pub tags: Vec<String>,
}

impl CommandEntry {
    /// An entry of `texts`, one for each locale in `Locale::ALL`'s order.
    ///
    /// The last is repeated for a locale it does not reach, and a locale past
    /// it is dropped.
    #[must_use]
    pub fn new(mut texts: Vec<EntryTexts>, categories: Vec<String>, glyph: Option<char>) -> Self {
        let last = texts.last().cloned().unwrap_or_default();
        texts.resize(Locale::ALL.len(), last);
        Self {
            texts,
            categories,
            glyph,
        }
    }

    /// What it shows in `locale`.
    #[must_use]
    pub fn in_locale(&self, locale: Locale) -> &EntryTexts {
        &self.texts[locale.index()]
    }
}

/// A command the kernel can dispatch, as the registry lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredCommand {
    /// Its name.
    pub name: String,
    /// What a person is shown for it: the catalog's entry for a command the
    /// catalog writes, the plugin's own for one it does not.
    pub entry: CommandEntry,
    /// The JSON Schema of its arguments, as declared.
    pub args_schema: String,
    /// Whether it mutates state.
    pub mutating: bool,
    /// The Cedar action it is decided under.
    pub action: String,
    /// How its resource is taken from its arguments.
    pub resource_selector: String,
    /// Read, write or irreversible.
    pub reversibility: Reversibility,
    /// Who runs it.
    pub owner: Owner,
}

/// One command as its author declared it, in the kernel's own types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Declared {
    /// Dotted, lower-case, namespaced.
    pub(crate) name: String,
    /// The label a person reads, or the catalog's message key.
    pub(crate) title: String,
    /// The argument schema's text.
    pub(crate) args_schema: String,
    /// Whether it changes state.
    pub(crate) mutating: bool,
    /// The Cedar action.
    pub(crate) action: String,
    /// `realm`, `argument:<name>` or `feature:<id>`.
    pub(crate) resource_selector: String,
    /// Read, write or irreversible.
    pub(crate) reversibility: Reversibility,
}

impl Declared {
    /// The declaration as the judge reads it.
    fn as_written(&self) -> descriptor::Declared<'_> {
        descriptor::Declared {
            name: &self.name,
            title: &self.title,
            args_schema: &self.args_schema,
            action: &self.action,
            resource_selector: &self.resource_selector,
            mutating: self.mutating,
            reversibility: self.reversibility,
        }
    }

    /// The declaration as the registry lists it, run by `owner` and shown
    /// by `entry`.
    fn listed(&self, owner: Owner, entry: CommandEntry) -> RegisteredCommand {
        RegisteredCommand {
            name: self.name.clone(),
            entry,
            args_schema: self.args_schema.clone(),
            mutating: self.mutating,
            action: self.action.clone(),
            resource_selector: self.resource_selector.clone(),
            reversibility: self.reversibility,
            owner,
        }
    }
}

/// A command the registry holds: as it is listed, and as the judge read it.
#[derive(Debug, Clone)]
pub(crate) struct Admitted {
    listed: RegisteredCommand,
    judged: Judged,
}

impl Admitted {
    /// The command as the registry lists it: its declaration and its owner.
    pub(crate) const fn listed(&self) -> &RegisteredCommand {
        &self.listed
    }

    /// The compiled schema its arguments are validated with, and the selector
    /// its resource is taken by.
    pub(crate) const fn judged(&self) -> &Judged {
        &self.judged
    }
}

/// Every command the kernel can dispatch, and the ones the catalog describes
/// for a plugin to implement.
#[derive(Debug, Default)]
pub(crate) struct Registry {
    commands: BTreeMap<String, Admitted>,
    described: BTreeMap<String, Admitted>,
}

impl Registry {
    /// The kernel's own commands, read out of the catalog, each with its
    /// entry resolved in every locale.
    ///
    /// # Panics
    ///
    /// When a command of the catalog is one the judge refuses, or names a key
    /// no locale resolves — which the catalog compiler refuses to build, with
    /// the same judge and the same keys.
    pub(crate) fn from_catalog() -> Self {
        Self::over(nc_catalog::COMMANDS.iter().map(|command| {
            (
                Declared {
                    name: command.name.to_owned(),
                    title: command.title_key.to_owned(),
                    args_schema: command.args_schema.to_owned(),
                    mutating: command.mutating,
                    action: command.action.to_owned(),
                    resource_selector: command.resource_selector.to_owned(),
                    reversibility: match command.reversibility {
                        nc_catalog::Reversibility::Read => Reversibility::Read,
                        nc_catalog::Reversibility::Write => Reversibility::Write,
                        nc_catalog::Reversibility::Irreversible => Reversibility::Irreversible,
                    },
                },
                catalog_entry(command),
            )
        }))
    }

    /// A registry over `catalog`: each command in a namespace the kernel keeps
    /// is the kernel's, and every other one is described for a plugin.
    fn over(catalog: impl IntoIterator<Item = (Declared, CommandEntry)>) -> Self {
        let mut registry = Self::default();
        for (command, entry) in catalog {
            let judged = descriptor::judge(&command.as_written()).unwrap_or_else(|problems| {
                panic!(
                    "the catalog's `{}` is one the judge refuses, and the catalog compiler \
                     holds every command to the same judge: {problems:?}",
                    command.name
                )
            });
            let admitted = Admitted {
                listed: command.listed(Owner::Kernel, entry),
                judged,
            };
            if in_a_reserved_namespace(&command.name) {
                registry.commands.insert(command.name, admitted);
            } else {
                registry.described.insert(command.name, admitted);
            }
        }
        registry
    }

    /// Every command the kernel can dispatch, in name order.
    pub(crate) fn listed(&self) -> Vec<RegisteredCommand> {
        self.commands
            .values()
            .map(|admitted| admitted.listed.clone())
            .collect()
    }

    /// The command registered as `name`, if one is.
    ///
    /// **Registered, not described**: a catalog command no plugin declared is
    /// one nothing runs, and is not found.
    pub(crate) fn lookup(&self, name: &str) -> Option<Admitted> {
        self.commands.get(name).cloned()
    }

    /// Adds what [`Registry::admit`] admitted.
    pub(crate) fn register(&mut self, admitted: Vec<Admitted>) {
        for command in admitted {
            self.commands.insert(command.listed.name.clone(), command);
        }
    }

    /// Takes out every command the plugin in `slot` declared: one unloaded.
    ///
    /// A catalog command it implemented is described again, for the next
    /// plugin to declare.
    pub(crate) fn forget(&mut self, slot: PluginSlot) {
        self.commands
            .retain(|_, command| command.listed.owner != Owner::Plugin(slot));
    }

    /// Judges what the plugin `manifest` describes, loaded in `slot`, declared
    /// at `activate`, against this registry and the decision point's schema.
    ///
    /// # Errors
    ///
    /// Every problem found, each with its own code — the module's header
    /// lists them — and never an empty list.
    pub(crate) fn admit(
        &self,
        manifest: &PluginManifest,
        slot: PluginSlot,
        commands: &[Declared],
        subscriptions: &[String],
        pdp: &Pdp,
    ) -> Result<Vec<Admitted>, Vec<LoadProblem>> {
        let plugin = manifest.id();
        let most = usize::try_from(KERNEL.commands_per_plugin).unwrap_or(usize::MAX);
        if commands.len() > most {
            return Err(vec![LoadProblem::new(
                ErrorCode::PluginDescriptorInvalid,
                format!(
                    "`{plugin}` declares {} commands, and a plugin may declare at most {most} \
                     (`commands_per_plugin` in `catalog/limits.toml`)",
                    commands.len()
                ),
            )]);
        }
        let actions = pdp.actions();
        let mut problems = Vec::new();
        let mut admitted = Vec::with_capacity(commands.len());
        for (at, command) in commands.iter().enumerate() {
            let before = problems.len();
            let subject = subject_of(command, at, plugin);
            let judged = descriptor::judge(&command.as_written())
                .map_err(|found| {
                    problems.extend(found.into_iter().map(|problem| {
                        LoadProblem::new(
                            if matches!(problem, DescriptorProblem::Missing(_)) {
                                ErrorCode::PluginDescriptorMissingField
                            } else {
                                ErrorCode::PluginDescriptorInvalid
                            },
                            format!("{subject} {problem}"),
                        )
                    }));
                })
                .ok();
            if descriptor::command_name_is_well_formed(&command.name)
                && let Some(why) =
                    self.name_refused(command, judged.as_ref(), &commands[..at], manifest)
            {
                problems.push(LoadProblem::new(
                    ErrorCode::KernelNameTaken,
                    format!("{subject} {why}"),
                ));
            }
            if let Some(judged) = &judged
                && let Some(why) = undecidable(command, judged, &actions, pdp)
            {
                problems.push(LoadProblem::new(
                    ErrorCode::PluginDescriptorInvalid,
                    format!("{subject} {why}"),
                ));
            }
            // **Only a name the plugin may have is looked up in its entries**:
            // one malformed, outside its namespace or in one the kernel keeps
            // is refused above already, and a row is keyed by the name inside
            // the namespace, which such a name does not have.
            let entry = match relative_to(&command.name, manifest.namespace()) {
                Some(relative) if descriptor::command_name_is_well_formed(&command.name) => self
                    .entry_of(command, relative, judged.is_some(), manifest)
                    .map_err(|found| {
                        problems.extend(found.into_iter().map(|why| {
                            LoadProblem::new(
                                ErrorCode::PluginEntriesInvalid,
                                format!("{subject} {why}"),
                            )
                        }));
                    })
                    .ok(),
                _ => None,
            };
            if let (true, Some(judged), Some(entry)) = (problems.len() == before, judged, entry) {
                admitted.push(self.described.get(&command.name).map_or_else(
                    || Admitted {
                        listed: command.listed(Owner::Plugin(slot), entry),
                        judged: judged.clone(),
                    },
                    |described| Admitted {
                        listed: RegisteredCommand {
                            owner: Owner::Plugin(slot),
                            ..described.listed.clone()
                        },
                        judged: judged.clone(),
                    },
                ));
            }
        }
        problems.extend(unheard(manifest, subscriptions));
        if problems.is_empty() {
            Ok(admitted)
        } else {
            Err(problems)
        }
    }

    /// Why a command may not have its name, when it may not: the predicate a
    /// refusal writes after its subject.
    fn name_refused(
        &self,
        command: &Declared,
        judged: Option<&Judged>,
        earlier: &[Declared],
        manifest: &PluginManifest,
    ) -> Option<String> {
        let namespace = manifest.namespace();
        if in_a_reserved_namespace(&command.name) {
            return Some(format!(
                "is in the namespace `{}.`, which the kernel keeps for its own commands",
                first_segment(&command.name)
            ));
        }
        if !command.name.starts_with(&format!("{namespace}.")) {
            return Some(format!(
                "is outside its namespace: every command of `{}` is named `{namespace}.` and more",
                manifest.id()
            ));
        }
        if earlier.iter().any(|before| before.name == command.name) {
            return Some("is declared twice".to_owned());
        }
        if self.commands.contains_key(&command.name) {
            return Some("is already registered, by a plugin loaded before this one".to_owned());
        }
        let described = self.described.get(&command.name)?;
        let field = differs(command, judged?, described)?;
        Some(format!(
            "is the catalog's command of that name, and its {field} is not the catalog's"
        ))
    }

    /// The entry `command` is shown by, named `relative` inside its plugin's
    /// namespace — or every reason it has none, each the predicate a refusal
    /// writes after its subject.
    ///
    /// The catalog's, for a command the catalog describes, whose plugin then
    /// writes it no row; the plugin's own row otherwise, held to the one
    /// judge, and then — once the judge admits it — to the title the command
    /// declared, when the descriptor judge admitted that (`declared`), and to
    /// the display names the catalog's commands are shown by.
    fn entry_of(
        &self,
        command: &Declared,
        relative: &str,
        declared: bool,
        manifest: &PluginManifest,
    ) -> Result<CommandEntry, Vec<String>> {
        let row = manifest.entries().row(relative);
        if let Some(described) = self.described.get(&command.name) {
            return match row {
                None => Ok(described.listed.entry.clone()),
                Some(_) => Err(vec![format!(
                    "is described by the catalog, and its plugin's `commands.toml` describes it \
                     too, as `{relative}`: the catalog's entry is the one it is shown by"
                )]),
            };
        }
        let Some(row) = row else {
            return Err(vec![format!(
                "has no entry: its plugin's `commands.toml` lists no `{relative}`"
            )]);
        };
        let entry = row
            .judged(&Rules {
                requires: &Text::ALL,
                categories: &CATEGORIES,
                entry_bytes: KERNEL.entry_bytes,
            })
            .map_err(|found| found.iter().map(ToString::to_string).collect::<Vec<_>>())?;
        let mut whys = Vec::new();
        let english = &entry.in_locale(Locale::En).display_name;
        if declared && command.title != *english {
            whys.push(format!(
                "declares the title {}, and its English display name is {}: the two are one \
                 text, written in the component and in its directory",
                quoted(&command.title),
                quoted(english)
            ));
        }
        for &locale in Locale::ALL {
            let ours = &entry.in_locale(locale).display_name;
            if let Some(theirs) = self.catalog_commands().find(|theirs| {
                Locale::ALL.iter().any(|&shown| {
                    entry::the_same_sentence(
                        ours,
                        &theirs.listed.entry.in_locale(shown).display_name,
                    )
                })
            }) {
                whys.push(format!(
                    "has the `{}` display name {}, which the catalog's `{}` is shown by",
                    locale.tag(),
                    quoted(ours),
                    theirs.listed.name
                ));
            }
        }
        if whys.is_empty() {
            Ok(entry)
        } else {
            Err(whys)
        }
    }

    /// Every command of the catalog this registry was made over: the kernel's
    /// own, and those it describes for a plugin.
    fn catalog_commands(&self) -> impl Iterator<Item = &Admitted> {
        self.commands
            .values()
            .filter(|command| command.listed.owner == Owner::Kernel)
            .chain(self.described.values())
    }
}

/// The entry the catalog writes for `command`, resolved from its keys in
/// every locale.
///
/// # Panics
///
/// When a key resolves in no locale — which the catalog compiler refuses to
/// build, so a kernel built over the catalog never meets one.
fn catalog_entry(command: &nc_catalog::Command) -> CommandEntry {
    let resolved = |key: &str, locale: Locale| {
        nc_catalog::message(key, locale).unwrap_or_else(|| {
            panic!(
                "the catalog's `{}` names `{key}`, which no locale resolves, and the catalog \
                 compiler refuses a key that does not",
                command.name
            )
        })
    };
    CommandEntry::new(
        Locale::ALL
            .iter()
            .map(|&locale| EntryTexts {
                display_name: resolved(command.title_key, locale).to_owned(),
                line: resolved(command.description_key, locale).to_owned(),
                paragraph: resolved(command.paragraph_key, locale).to_owned(),
                tags: nc_catalog::tags(command.tags_key, locale)
                    .unwrap_or_else(|| {
                        panic!(
                            "the catalog's `{}` names the tags `{}`, which no locale resolves",
                            command.name, command.tags_key
                        )
                    })
                    .iter()
                    .map(|&tag| tag.to_owned())
                    .collect(),
            })
            .collect(),
        command
            .categories
            .iter()
            .map(|&category| category.to_owned())
            .collect(),
        command.glyph,
    )
}

/// The name `name` has inside `namespace`, when it is in that namespace.
///
/// A plugin's namespace is never one the kernel keeps, which `Kernel::load`
/// refuses before any of its commands is judged.
fn relative_to<'a>(name: &'a str, namespace: &str) -> Option<&'a str> {
    name.strip_prefix(namespace)?.strip_prefix('.')
}

/// Why a judged command cannot be decided as declared, when it cannot.
///
/// A feature of the platform selected at all, an action the schema does not
/// declare, a resource type the action does not apply to, or the whole realm
/// selected by a command that does not read it.
fn undecidable(
    command: &Declared,
    judged: &Judged,
    actions: &[String],
    pdp: &Pdp,
) -> Option<String> {
    if let Selector::Feature { id } = &judged.selector {
        return Some(format!(
            "selects the feature {} of the platform, which only a command of the kernel's \
             own selects",
            quoted(id)
        ));
    }
    if !actions.contains(&command.action) {
        return Some(format!(
            "names the action {}, which the decision point's schema does not declare — it \
             declares {}",
            quoted(&command.action),
            actions.join(", ")
        ));
    }
    let entity = judged.selector.entity_type();
    if !pdp
        .resources_for(&command.action)
        .iter()
        .any(|resource| resource == entity)
    {
        return Some(format!(
            "selects a {}, which the action `{}` does not apply to",
            quoted(entity),
            command.action
        ));
    }
    if entity == REALM_TYPE && (command.action != THE_REALM_IS_READ_WITH || command.mutating) {
        return Some(format!(
            "selects the whole realm and is not a read under `{THE_REALM_IS_READ_WITH}`: a \
             plugin may select the realm only to read it, and a change to the whole realm is \
             the kernel's"
        ));
    }
    None
}

/// The first field a dispatch is decided by in which `command` is not the
/// catalog's `described`, by the name a person reads.
fn differs(command: &Declared, judged: &Judged, described: &Admitted) -> Option<String> {
    let catalog = &described.listed;
    [
        ("argument schema", judged.schema != described.judged.schema),
        ("action", command.action != catalog.action),
        (
            "resource selector",
            command.resource_selector != catalog.resource_selector,
        ),
        ("mutating flag", command.mutating != catalog.mutating),
        (
            "reversibility",
            command.reversibility != catalog.reversibility,
        ),
    ]
    .into_iter()
    .find(|(_, differs)| *differs)
    .map(|(field, _)| field.to_owned())
}

/// Every refusal of the topics a plugin subscribed to at `activate`: each must
/// be one its manifest declares, once.
fn unheard(manifest: &PluginManifest, subscriptions: &[String]) -> Vec<LoadProblem> {
    let plugin = manifest.id();
    let declared = manifest.subscriptions();
    if subscriptions.len() > declared.len() {
        return vec![LoadProblem::new(
            ErrorCode::PluginDescriptorInvalid,
            format!(
                "`{plugin}` subscribes to {} topics at activate, and its manifest's \
                 `[events] subscribe` declares {}",
                subscriptions.len(),
                declared.len()
            ),
        )];
    }
    let mut problems = Vec::new();
    for (at, topic) in subscriptions.iter().enumerate() {
        let why = if subscriptions[..at].contains(topic) {
            "twice"
        } else if declared.contains(topic) {
            continue;
        } else {
            "and its manifest's `[events] subscribe` does not declare it"
        };
        problems.push(LoadProblem::new(
            ErrorCode::PluginDescriptorInvalid,
            format!("`{plugin}` subscribes to {} {why}", quoted(topic)),
        ));
    }
    problems
}

/// Who a refusal is about: the command by its name, or by its position when
/// its name is not one a refusal can repeat.
fn subject_of(command: &Declared, at: usize, plugin: &str) -> String {
    if descriptor::command_name_is_well_formed(&command.name) {
        format!("`{}` of `{plugin}`", command.name)
    } else {
        format!("the command at position {} of `{plugin}`", at + 1)
    }
}

/// Whether `name` is in a namespace the kernel keeps.
fn in_a_reserved_namespace(name: &str) -> bool {
    RESERVED.contains(&first_segment(name))
}

/// A name's first segment: its namespace.
fn first_segment(name: &str) -> &str {
    name.split('.').next().unwrap_or(name)
}

/// `text` as a refusal repeats it: at most [`QUOTED`] characters, each
/// escaped, between backticks.
fn quoted(text: &str) -> String {
    quoted_up_to(text, QUOTED)
}

/// `text` as a refusal repeats it, cut after `most` characters.
///
/// Each character escaped as Rust's `Debug` escapes one — a control, a
/// direction override, anything a reader would not see — between backticks,
/// and `…` after the last when the text held more. The one rule for every
/// text a plugin wrote that a refusal of the kernel repeats, because the
/// refusal is the kernel's sentence and whoever reads it reads it as such;
/// each caller passes the bound its text is held to.
pub(crate) fn quoted_up_to(text: &str, most: usize) -> String {
    let mut shown: String = text
        .chars()
        .take(most)
        .flat_map(char::escape_debug)
        .collect();
    if text.chars().nth(most).is_some() {
        shown.push('…');
    }
    format!("`{shown}`")
}

#[cfg(test)]
mod tests {
    use super::{CommandEntry, Declared, EntryTexts, Owner, RESERVED, Registry, quoted};
    use crate::PluginSlot;
    use crate::entries::PluginEntries;
    use crate::manifest::PluginManifest;
    use nc_catalog::{ErrorCode, Locale};
    use nc_schema::descriptor::{self, NAME_MAX, Reversibility};

    /// A note's arguments: one required entity of type `Content`.
    const A_NOTE: &str = r#"{"type":"object","properties":{"note":{"type":"string","x-entity":"Content","maxLength":128}},"required":["note"],"additionalProperties":false}"#;

    /// Two required entities of type `Content`: the note and its page.
    const TWO: &str = r#"{"type":"object","properties":{"note":{"type":"string","x-entity":"Content","maxLength":128},"page":{"type":"string","x-entity":"Content","maxLength":128}},"required":["note","page"],"additionalProperties":false}"#;

    /// No arguments at all.
    const NOTHING: &str = r#"{"type":"object","properties":{},"additionalProperties":false}"#;

    fn pdp() -> nc_authz::Pdp {
        nc_authz::Pdp::embedded("family").expect("the family pack is carried")
    }

    fn manifest(text: &str) -> PluginManifest {
        PluginManifest::parse(text).expect("a manifest the kernel reads")
    }

    fn notes() -> PluginManifest {
        manifest("id = \"noctua:notes\"\nnamespace = \"notes\"\ncapabilities = []\n")
    }

    /// The texts of one entry in one locale, every rule met.
    fn texts(display_name: &str, line: &str, tag: &str) -> EntryTexts {
        EntryTexts {
            display_name: display_name.to_owned(),
            line: line.to_owned(),
            paragraph: format!("{line}, and nothing more than that."),
            tags: vec![tag.to_owned()],
        }
    }

    /// The entry a catalog command a test describes is shown by: written by
    /// the catalog, so no plugin's.
    fn the_catalogs() -> CommandEntry {
        CommandEntry::new(
            vec![
                texts("Look a note up", "Shows a note", "lookup"),
                texts("Consultar uma nota", "Mostra uma nota", "consulta"),
            ],
            Vec::new(),
            None,
        )
    }

    /// A registry over `catalog`, each command shown by [`the_catalogs`].
    fn over(catalog: impl IntoIterator<Item = Declared>) -> Registry {
        Registry::over(catalog.into_iter().map(|command| (command, the_catalogs())))
    }

    /// One locale's texts of one row, as its file writes them.
    fn row_of(relative: &str, display_name: &str, line: &str, tag: &str) -> String {
        format!(
            "[command.\"{relative}\"]\ndisplay_name = \"{display_name}\"\nline = \"{line}\"\n\
             paragraph = \"{line}, and nothing more than that.\"\ntags = [\"{tag}\"]\n"
        )
    }

    /// Entries of a row for each of `relatives`, every rule met, each English
    /// display name the one beside it.
    fn entries(relatives: &[(&str, &str)]) -> PluginEntries {
        let (mut table, mut english, mut portuguese) =
            (String::new(), String::new(), String::new());
        for (relative, display_name) in relatives {
            table += "[[command]]\nname = \"";
            table += relative;
            table += "\"\n";
            english.push_str(&row_of(
                relative,
                display_name,
                "Shows what a note says",
                "note",
            ));
            portuguese.push_str(&row_of(
                relative,
                &format!("Ler: {display_name}"),
                "Mostra o que uma nota diz",
                "nota",
            ));
        }
        PluginEntries::parse(
            &table,
            &[(Locale::En, &english), (Locale::PtBR, &portuguese)],
        )
        .expect("entries of the shape the kernel reads")
    }

    /// `manifest`, described by a sound row for each of `commands`.
    ///
    /// Each command `registry` does not describe and whose name is the
    /// plugin's to have, its English display name the command's title.
    fn described(
        registry: &Registry,
        manifest: PluginManifest,
        commands: &[Declared],
    ) -> PluginManifest {
        let namespace = manifest.namespace().to_owned();
        let mut rows: Vec<(&str, &str)> = Vec::new();
        for command in commands {
            let Some(relative) = super::relative_to(&command.name, &namespace) else {
                continue;
            };
            if descriptor::command_name_is_well_formed(&command.name)
                && !registry.described.contains_key(&command.name)
                && !rows.iter().any(|(listed, _)| *listed == relative)
            {
                let title = Some(command.title.as_str()).filter(|title| !title.is_empty());
                rows.push((relative, title.unwrap_or("Read a note")));
            }
        }
        manifest.described_by(entries(&rows))
    }

    /// A read of a note, named `name`, which every rule admits.
    fn a_read(name: &str) -> Declared {
        Declared {
            name: name.to_owned(),
            title: "Read a note".to_owned(),
            args_schema: A_NOTE.to_owned(),
            mutating: false,
            action: "Read".to_owned(),
            resource_selector: "argument:note".to_owned(),
            reversibility: Reversibility::Read,
        }
    }

    /// The codes and sentences of what `admit` refused, or nothing when it
    /// admitted every command.
    fn refused(
        registry: &Registry,
        manifest: &PluginManifest,
        commands: &[Declared],
        subscriptions: &[&str],
    ) -> Vec<(ErrorCode, String)> {
        let subscriptions: Vec<String> = subscriptions.iter().map(|&s| s.to_owned()).collect();
        let manifest = described(registry, manifest.clone(), commands);
        match registry.admit(&manifest, PluginSlot(0), commands, &subscriptions, &pdp()) {
            Ok(_) => Vec::new(),
            Err(problems) => problems
                .iter()
                .map(|problem| (problem.code(), problem.sentence().to_owned()))
                .collect(),
        }
    }

    #[test]
    fn the_kernels_own_commands_are_the_catalogs_in_the_namespaces_it_keeps() {
        let registry = Registry::from_catalog();
        let listed = registry.listed();
        assert!(
            !listed.is_empty(),
            "the catalog declares the kernel's commands"
        );
        assert_eq!(
            listed.len(),
            nc_catalog::COMMANDS
                .iter()
                .filter(|command| RESERVED
                    .iter()
                    .any(|kept| command.name.starts_with(&format!("{kept}."))))
                .count()
        );
        for command in &listed {
            assert_eq!(command.owner, Owner::Kernel, "`{}`", command.name);
            let written = nc_catalog::command(&command.name).expect("a catalog command");
            // **Every text in every locale, resolved from its key**: a registry
            // that listed the English texts in both, or the keys themselves,
            // would show a Portuguese reader English, or a key.
            for &locale in Locale::ALL {
                let shown = command.entry.in_locale(locale);
                let resolved =
                    |key| nc_catalog::message(key, locale).expect("a key the catalog has");
                assert_eq!(
                    shown.display_name,
                    resolved(written.title_key),
                    "`{}`",
                    command.name
                );
                assert_eq!(
                    shown.line,
                    resolved(written.description_key),
                    "`{}`",
                    command.name
                );
                assert_eq!(
                    shown.paragraph,
                    resolved(written.paragraph_key),
                    "`{}`",
                    command.name
                );
                assert_eq!(
                    shown.tags,
                    nc_catalog::tags(written.tags_key, locale).expect("tags the catalog has"),
                    "`{}`",
                    command.name
                );
            }
            assert_ne!(
                command.entry.in_locale(Locale::En).display_name,
                command.entry.in_locale(Locale::PtBR).display_name,
                "`{}`",
                command.name
            );
            assert_eq!(command.entry.glyph, written.glyph, "`{}`", command.name);
            assert_eq!(
                command.entry.categories, written.categories,
                "`{}`",
                command.name
            );
            assert_eq!(command.args_schema, written.args_schema);
            assert_eq!(command.action, written.action);
            assert_eq!(command.resource_selector, written.resource_selector);
            assert_eq!(command.mutating, written.mutating);
            // **Every field, the reversibility included**: a registry
            // listing every command as a read — `limits.prune`, the catalog's
            // `write`, listed as `Read` — fails here, while the decision
            // point's irreversible precondition is what reads it.
            assert_eq!(
                command.reversibility,
                match written.reversibility {
                    nc_catalog::Reversibility::Read => Reversibility::Read,
                    nc_catalog::Reversibility::Write => Reversibility::Write,
                    nc_catalog::Reversibility::Irreversible => Reversibility::Irreversible,
                },
                "`{}`",
                command.name
            );
        }
        assert!(
            listed
                .iter()
                .any(|command| command.reversibility != Reversibility::Read),
            "the catalog's own commands include one that is not a read, or the comparison \
             above cannot tell a registry that lists every command as one"
        );
        assert!(
            registry.described.is_empty(),
            "every catalog command is the kernel's today: {:?}",
            registry.described
        );
    }

    #[test]
    fn a_plugin_declaring_commands_every_rule_admits_registers_them_as_its_own() {
        let mut registry = Registry::from_catalog();
        let declared = [a_read("notes.note.get"), a_read("notes.note.peek")];
        let admitted = registry
            .admit(
                &described(&registry, notes(), &declared),
                PluginSlot(3),
                &declared,
                &[],
                &pdp(),
            )
            .expect("every rule admits both");
        registry.register(admitted);
        let ours: Vec<_> = registry
            .listed()
            .into_iter()
            .filter(|command| command.owner == Owner::Plugin(PluginSlot(3)))
            .map(|command| command.name)
            .collect();
        assert_eq!(ours, ["notes.note.get", "notes.note.peek"]);
    }

    #[test]
    fn a_plugin_forgotten_takes_its_commands_with_it_and_no_other() {
        let mut registry = Registry::from_catalog();
        let kernels = registry.listed().len();
        let declared = [a_read("notes.note.get")];
        let admitted = registry
            .admit(
                &described(&registry, notes(), &declared),
                PluginSlot(3),
                &declared,
                &[],
                &pdp(),
            )
            .expect("every rule admits it");
        registry.register(admitted);
        registry.forget(PluginSlot(4));
        assert!(
            registry.lookup("notes.note.get").is_some(),
            "another slot's"
        );
        registry.forget(PluginSlot(3));
        assert!(registry.lookup("notes.note.get").is_none());
        assert_eq!(registry.listed().len(), kernels, "the kernel's own stay");
    }

    #[test]
    fn a_field_left_empty_is_a_missing_field_naming_the_command_and_the_field() {
        for (field, emptied) in [
            (
                "action",
                Declared {
                    action: String::new(),
                    ..a_read("notes.note.get")
                },
            ),
            (
                "resource selector",
                Declared {
                    resource_selector: " ".to_owned(),
                    ..a_read("notes.note.get")
                },
            ),
            (
                "title",
                Declared {
                    title: String::new(),
                    ..a_read("notes.note.get")
                },
            ),
            (
                "argument schema",
                Declared {
                    args_schema: String::new(),
                    ..a_read("notes.note.get")
                },
            ),
        ] {
            assert_eq!(
                refused(&Registry::from_catalog(), &notes(), &[emptied], &[]),
                [(
                    ErrorCode::PluginDescriptorMissingField,
                    format!("`notes.note.get` of `noctua:notes` declares no {field}")
                )]
            );
        }
    }

    #[test]
    fn a_command_with_no_name_is_named_by_its_position() {
        let problems = refused(
            &Registry::from_catalog(),
            &notes(),
            &[
                a_read("notes.note.get"),
                Declared {
                    name: String::new(),
                    ..a_read("x")
                },
            ],
            &[],
        );
        assert_eq!(
            problems,
            [(
                ErrorCode::PluginDescriptorMissingField,
                "the command at position 2 of `noctua:notes` declares no name".to_owned()
            )]
        );
    }

    /// A command whose name is malformed is named by its position, and its name
    /// is never repeated.
    ///
    /// **What naming the plugin does not hold.** The test below holds only that
    /// the sentence names the plugin, which a `subject_of` repeating any name
    /// not blank would pass — a name of 10,000 characters behind a direction
    /// override repeated whole, the override raw.
    #[test]
    fn a_malformed_name_is_named_by_its_position_and_never_repeated() {
        let problems = refused(
            &Registry::from_catalog(),
            &notes(),
            &[Declared {
                name: format!("Notes.\u{202e}{}", "x".repeat(10_000)),
                ..a_read("x")
            }],
            &[],
        );
        assert_eq!(problems.len(), 1, "{} problems", problems.len());
        let (code, sentence) = &problems[0];
        assert_eq!(*code, ErrorCode::PluginDescriptorInvalid);
        assert!(
            sentence.starts_with("the command at position 1 of `noctua:notes` ")
                && !sentence.contains('\u{202e}')
                && !sentence.contains("xxx"),
            "the refusal repeats {} characters, opening {:?}",
            sentence.chars().count(),
            sentence.chars().take(80).collect::<String>()
        );
    }

    #[test]
    fn anything_else_the_judge_refuses_is_an_invalid_descriptor() {
        let unknown_keyword =
            A_NOTE.replace("\"maxLength\":128", "\"maxLength\":128,\"pattern\":\"a\"");
        for (what, broken) in [
            (
                "a malformed name",
                Declared {
                    name: "Notes.Get".to_owned(),
                    ..a_read("x")
                },
            ),
            (
                "an unknown keyword",
                Declared {
                    args_schema: unknown_keyword.clone(),
                    ..a_read("notes.note.get")
                },
            ),
            (
                "a selector naming nothing",
                Declared {
                    resource_selector: "argument:nothing".to_owned(),
                    ..a_read("notes.note.get")
                },
            ),
            (
                "a read that mutates",
                Declared {
                    mutating: true,
                    ..a_read("notes.note.get")
                },
            ),
        ] {
            let problems = refused(&Registry::from_catalog(), &notes(), &[broken], &[]);
            assert_eq!(problems.len(), 1, "{what}: {problems:?}");
            assert_eq!(problems[0].0, ErrorCode::PluginDescriptorInvalid, "{what}");
            assert!(
                problems[0].1.contains("of `noctua:notes` "),
                "{what}: {}",
                problems[0].1
            );
        }
    }

    #[test]
    fn a_name_outside_the_plugins_namespace_or_in_one_the_kernel_keeps_is_taken() {
        for (name, says) in [
            (
                "journal.verify",
                "the namespace `journal.`, which the kernel keeps",
            ),
            (
                "kernel.note.get",
                "the namespace `kernel.`, which the kernel keeps",
            ),
            (
                "limits.note.get",
                "the namespace `limits.`, which the kernel keeps",
            ),
            ("elsewhere.note.get", "is outside its namespace"),
            ("notes-elsewhere.note.get", "is outside its namespace"),
        ] {
            let problems = refused(&Registry::from_catalog(), &notes(), &[a_read(name)], &[]);
            assert_eq!(problems.len(), 1, "`{name}`: {problems:?}");
            assert_eq!(problems[0].0, ErrorCode::KernelNameTaken, "`{name}`");
            assert!(
                problems[0]
                    .1
                    .starts_with(&format!("`{name}` of `noctua:notes` "))
                    && problems[0].1.contains(says),
                "`{name}`: {}",
                problems[0].1
            );
        }
    }

    #[test]
    fn a_name_declared_twice_or_already_registered_is_taken() {
        let problems = refused(
            &Registry::from_catalog(),
            &notes(),
            &[a_read("notes.note.get"), a_read("notes.note.get")],
            &[],
        );
        assert_eq!(
            problems,
            [(
                ErrorCode::KernelNameTaken,
                "`notes.note.get` of `noctua:notes` is declared twice".to_owned()
            )]
        );
        let mut registry = Registry::from_catalog();
        let declared = [a_read("notes.note.get")];
        let first = registry
            .admit(
                &described(&registry, notes(), &declared),
                PluginSlot(0),
                &declared,
                &[],
                &pdp(),
            )
            .expect("admitted");
        registry.register(first);
        let problems = refused(&registry, &notes(), &[a_read("notes.note.get")], &[]);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert_eq!(problems[0].0, ErrorCode::KernelNameTaken);
        assert!(
            problems[0].1.contains("already registered"),
            "{}",
            problems[0].1
        );
    }

    #[test]
    fn a_command_the_catalog_describes_is_admitted_only_as_the_catalog_writes_it() {
        let registry = over([a_read("notes.note.get")]);
        let seen = registry.listed();
        assert!(
            seen.is_empty(),
            "a described command is nobody's until declared: {seen:?}"
        );
        let admitted = registry
            .admit(
                &notes(),
                PluginSlot(1),
                &[a_read("notes.note.get")],
                &[],
                &pdp(),
            )
            .expect("the catalog's own declaration is admitted");
        assert_eq!(admitted.len(), 1);
        for (field, changed) in [
            (
                "argument schema",
                Declared {
                    args_schema: A_NOTE.replace("128", "64"),
                    ..a_read("notes.note.get")
                },
            ),
            (
                "action",
                Declared {
                    action: "Share".to_owned(),
                    ..a_read("notes.note.get")
                },
            ),
            (
                "mutating flag",
                Declared {
                    mutating: true,
                    reversibility: Reversibility::Write,
                    ..a_read("notes.note.get")
                },
            ),
        ] {
            let problems = refused(&registry, &notes(), &[changed], &[]);
            assert!(
                problems.iter().any(|(code, sentence)| *code == ErrorCode::KernelNameTaken
                    && sentence.ends_with(&format!("is the catalog's command of that name, and its {field} is not the catalog's"))),
                "{field}: {problems:?}"
            );
        }
        let reversed = Declared {
            reversibility: Reversibility::Irreversible,
            mutating: true,
            ..a_read("notes.note.get")
        };
        let problems = refused(
            &over([Declared {
                reversibility: Reversibility::Write,
                mutating: true,
                ..a_read("notes.note.get")
            }]),
            &notes(),
            &[reversed],
            &[],
        );
        assert!(
            problems
                .iter()
                .any(|(_, sentence)| sentence.contains("its reversibility")),
            "{problems:?}"
        );
        let elsewhere = over([Declared {
            resource_selector: "argument:note".to_owned(),
            args_schema: A_NOTE.to_owned(),
            ..a_read("notes.note.get")
        }]);
        let selected_otherwise = Declared {
            args_schema: A_NOTE.replace("\"note\"", "\"page\""),
            resource_selector: "argument:page".to_owned(),
            ..a_read("notes.note.get")
        };
        let problems = refused(&elsewhere, &notes(), &[selected_otherwise], &[]);
        assert!(
            problems
                .iter()
                .any(|(_, sentence)| sentence.contains("its argument schema")),
            "{problems:?}"
        );
    }

    /// A claimed command is listed as the catalog writes it.
    ///
    /// Its entry is the catalog's, it is run by the plugin that claimed it,
    /// and one selecting another argument than the catalog's is refused naming
    /// the selector.
    #[test]
    fn a_claimed_command_is_the_catalogs_and_its_selector_is_held_to_it() {
        let described = Declared {
            title: "command.notes.note.get.title".to_owned(),
            args_schema: TWO.to_owned(),
            ..a_read("notes.note.get")
        };
        let registry = over([described]);
        let ours = Declared {
            args_schema: TWO.to_owned(),
            ..a_read("notes.note.get")
        };
        let admitted = registry
            .admit(
                &notes(),
                PluginSlot(2),
                std::slice::from_ref(&ours),
                &[],
                &pdp(),
            )
            .expect("the catalog's declaration, whatever the title");
        assert_eq!(admitted[0].listed.entry, the_catalogs());
        assert_eq!(admitted[0].listed.owner, Owner::Plugin(PluginSlot(2)));
        let elsewhere = Declared {
            resource_selector: "argument:page".to_owned(),
            ..ours
        };
        let problems = refused(&registry, &notes(), &[elsewhere], &[]);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(
            problems[0]
                .1
                .ends_with("its resource selector is not the catalog's"),
            "{}",
            problems[0].1
        );
    }

    #[test]
    #[should_panic(expected = "the catalog compiler holds every command to the same judge")]
    fn a_catalog_command_the_judge_refuses_is_a_build_the_compiler_did_not_make() {
        let _ = over([Declared {
            action: String::new(),
            ..a_read("journal.broken")
        }]);
    }

    #[test]
    fn an_action_the_schema_does_not_declare_or_a_resource_it_does_not_apply_to_is_invalid() {
        let unknown = Declared {
            action: "Rename".to_owned(),
            ..a_read("notes.note.get")
        };
        let problems = refused(&Registry::from_catalog(), &notes(), &[unknown], &[]);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert_eq!(problems[0].0, ErrorCode::PluginDescriptorInvalid);
        assert!(
            problems[0].1.contains("names the action `Rename`"),
            "{}",
            problems[0].1
        );
        let folder = Declared {
            args_schema: A_NOTE.replace("Content", "Folder"),
            ..a_read("notes.note.get")
        };
        let problems = refused(&Registry::from_catalog(), &notes(), &[folder], &[]);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert_eq!(problems[0].0, ErrorCode::PluginDescriptorInvalid);
        assert!(
            problems[0]
                .1
                .contains("selects a `Folder`, which the action `Read`"),
            "{}",
            problems[0].1
        );
    }

    /// The entity type a refused command selects is repeated cut to a bound.
    ///
    /// **The plugin's own text, of any length.** The type is the
    /// schema's `x-entity`, which the judge holds to a Cedar type name of any
    /// length; repeated whole, a type of 100,000 characters would give a
    /// refusal of 100,090.
    #[test]
    fn an_entity_type_a_refusal_repeats_is_cut_to_a_bound() {
        let long = Declared {
            args_schema: A_NOTE.replace("Content", &"A".repeat(100_000)),
            ..a_read("notes.note.get")
        };
        let problems = refused(&Registry::from_catalog(), &notes(), &[long], &[]);
        assert_eq!(problems.len(), 1, "{} problems", problems.len());
        let (code, sentence) = &problems[0];
        assert_eq!(*code, ErrorCode::PluginDescriptorInvalid);
        assert!(
            sentence.starts_with("`notes.note.get` of `noctua:notes` selects a `AAAA")
                && sentence.contains("…`, which the action `Read` does not apply to")
                && sentence.chars().count() < 1_000,
            "the refusal repeats {} characters, opening {:?}",
            sentence.chars().count(),
            sentence.chars().take(80).collect::<String>()
        );
    }

    #[test]
    fn a_plugin_selects_the_whole_realm_only_to_read_it() {
        let realm = |action: &str, mutating: bool| Declared {
            args_schema: NOTHING.to_owned(),
            resource_selector: "realm".to_owned(),
            action: action.to_owned(),
            mutating,
            reversibility: if mutating {
                Reversibility::Write
            } else {
                Reversibility::Read
            },
            ..a_read("notes.everything.count")
        };
        assert_eq!(
            refused(
                &Registry::from_catalog(),
                &notes(),
                &[realm("Read", false)],
                &[]
            ),
            [] as [(nc_catalog::ErrorCode, std::string::String); 0]
        );
        for (action, mutating) in [("Write", true), ("Read", true), ("Delete", false)] {
            let problems = refused(
                &Registry::from_catalog(),
                &notes(),
                &[realm(action, mutating)],
                &[],
            );
            assert_eq!(problems.len(), 1, "{action}/{mutating}: {problems:?}");
            assert_eq!(problems[0].0, ErrorCode::PluginDescriptorInvalid);
            assert!(
                problems[0].1.contains("selects the whole realm"),
                "{}",
                problems[0].1
            );
        }
    }

    /// A plugin that selects a feature is refused at `activate`, naming the
    /// command and the feature.
    ///
    /// **Whatever the action, and the feature the kernel's own commands
    /// select included.** The judge accepts the form from anyone, so this is
    /// the refusal that keeps a person's permit for `naming` from being borrowed:
    /// with it gone, a plugin's read on `feature:naming` is decided by the
    /// naming permit and journaled as if it were the kernel's.
    #[test]
    fn a_plugin_that_selects_a_feature_is_refused_at_activate() {
        let featured = |action: &str, id: &str| Declared {
            args_schema: NOTHING.to_owned(),
            resource_selector: format!("feature:{id}"),
            action: action.to_owned(),
            ..a_read("notes.name.propose")
        };
        for (action, id) in [("Read", "naming"), ("Read", "gallery"), ("Share", "naming")] {
            assert_eq!(
                refused(
                    &Registry::from_catalog(),
                    &notes(),
                    &[featured(action, id)],
                    &[]
                ),
                [(
                    ErrorCode::PluginDescriptorInvalid,
                    format!(
                        "`notes.name.propose` of `noctua:notes` selects the feature `{id}` of \
                         the platform, which only a command of the kernel's own selects"
                    )
                )],
                "{action} on `{id}`"
            );
        }
        let kernels = Registry::from_catalog()
            .listed()
            .into_iter()
            .filter(|command| command.resource_selector.starts_with("feature:"))
            .count();
        assert!(
            kernels > 0,
            "no command of the kernel's own selects a feature, so the refusal above \
             guards a form nothing uses"
        );
    }

    #[test]
    fn every_problem_of_every_command_is_reported_not_the_first() {
        let problems = refused(
            &Registry::from_catalog(),
            &notes(),
            &[
                Declared {
                    action: String::new(),
                    title: String::new(),
                    ..a_read("notes.a.get")
                },
                a_read("kernel.b.get"),
                Declared {
                    action: "Rename".to_owned(),
                    ..a_read("notes.c.get")
                },
            ],
            &[],
        );
        let codes: Vec<ErrorCode> = problems.iter().map(|(code, _)| *code).collect();
        assert_eq!(
            codes,
            [
                ErrorCode::PluginDescriptorMissingField,
                ErrorCode::PluginDescriptorMissingField,
                ErrorCode::KernelNameTaken,
                ErrorCode::PluginDescriptorInvalid,
            ],
            "{problems:?}"
        );
    }

    #[test]
    fn more_commands_than_a_plugin_may_declare_are_refused_before_any_is_judged_as_not_started() {
        let most = usize::try_from(nc_catalog::KERNEL.commands_per_plugin).expect("fits");
        let at_the_bound: Vec<Declared> = (0..most)
            .map(|n| a_read(&format!("notes.n{n}.get")))
            .collect();
        assert_eq!(
            refused(&Registry::from_catalog(), &notes(), &at_the_bound, &[]),
            [] as [(nc_catalog::ErrorCode, std::string::String); 0]
        );
        let past: Vec<Declared> = (0..=most)
            .map(|_| Declared {
                name: String::new(),
                ..a_read("x")
            })
            .collect();
        let problems = refused(&Registry::from_catalog(), &notes(), &past, &[]);
        assert_eq!(problems.len(), 1, "{} problems", problems.len());
        assert_eq!(problems[0].0, ErrorCode::PluginDescriptorInvalid);
        assert!(
            problems[0].1.contains("`commands_per_plugin`"),
            "{}",
            problems[0].1
        );
        // **And the person shown the code is told the plugin was not started**,
        // as every other refused load tells them — not NC-PLUG-0017, a
        // `warning` whose message says a part reached the most it may keep,
        // while a load refused here keeps nothing and never starts.
        let code = problems[0].0;
        assert_eq!(code.severity(), nc_catalog::Severity::Error);
        for (locale, says) in [
            (nc_catalog::Locale::En, "was not started"),
            (nc_catalog::Locale::PtBR, "não foi iniciada"),
        ] {
            let message = nc_catalog::message(code.message_key(), locale)
                .expect("the code's message is in the catalog");
            assert!(message.contains(says), "{locale:?}: {message}");
        }
    }

    #[test]
    fn a_subscription_is_one_the_manifest_declares_and_is_made_once() {
        let hearing = manifest(
            "id = \"noctua:notes\"\nnamespace = \"notes\"\ncapabilities = [\"events.subscribe\"]\n\
             [events]\nsubscribe = [\"photos.added\", \"photos.*\"]\n",
        );
        let registry = Registry::from_catalog();
        assert_eq!(
            refused(&registry, &hearing, &[], &["photos.added", "photos.*"]),
            [] as [(nc_catalog::ErrorCode, std::string::String); 0]
        );
        assert_eq!(
            refused(&registry, &hearing, &[], &["photos.removed"]),
            [(
                ErrorCode::PluginDescriptorInvalid,
                "`noctua:notes` subscribes to `photos.removed` and its manifest's \
                 `[events] subscribe` does not declare it"
                    .to_owned()
            )]
        );
        assert_eq!(
            refused(&registry, &hearing, &[], &["photos.added", "photos.added"]),
            [(
                ErrorCode::PluginDescriptorInvalid,
                "`noctua:notes` subscribes to `photos.added` twice".to_owned()
            )]
        );
        let problems = refused(&registry, &notes(), &[], &["photos.added"]);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].1.contains("declares 0"), "{}", problems[0].1);
        // **And the person shown the code is told a subscription can be what
        // was refused**, in both locales: every refusal above is of a
        // subscription, which a message naming a command alone would not say.
        for (locale, says) in [
            (nc_catalog::Locale::En, "subscription"),
            (nc_catalog::Locale::PtBR, "assinatura"),
        ] {
            let message =
                nc_catalog::message(ErrorCode::PluginDescriptorInvalid.message_key(), locale)
                    .expect("the code's message is in the catalog");
            assert!(message.contains(says), "{locale:?}: {message}");
        }
    }

    #[test]
    fn an_action_or_a_topic_a_refusal_repeats_is_cut_and_escaped() {
        assert_eq!(quoted("photos.added"), "`photos.added`");
        assert_eq!(quoted("a\u{202e}b"), "`a\\u{202e}b`");
        let long = "a".repeat(super::QUOTED + 1);
        assert_eq!(quoted(&long), format!("`{}…`", "a".repeat(super::QUOTED)));
        assert_eq!(
            quoted(&long[1..]),
            format!("`{}`", "a".repeat(super::QUOTED))
        );
    }

    #[test]
    fn a_well_formed_name_is_repeated_whole_past_what_an_action_is_cut_to() {
        // **Not every name the registry repeats is held to sixty-four**: a
        // command's name is repeated whole up to the ninety-six a well-formed
        // one may have.
        let longest = format!("notes.{}", "a".repeat(NAME_MAX - "notes.".len()));
        assert!(longest.len() > super::QUOTED);
        let past = "R".repeat(super::QUOTED + 1);
        let problems = refused(
            &Registry::from_catalog(),
            &notes(),
            &[Declared {
                action: past.clone(),
                ..a_read(&longest)
            }],
            &[],
        );
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(
            problems[0]
                .1
                .starts_with(&format!("`{longest}` of `noctua:notes` names the action ")),
            "{}",
            problems[0].1
        );
        assert!(
            problems[0].1.contains(&quoted(&past)) && !problems[0].1.contains(&past),
            "the action is cut: {}",
            problems[0].1
        );
    }

    /// The codes and sentences `admit` refused `declared` with, `manifest`
    /// described by `entries` and nothing else.
    fn refused_with(
        registry: &Registry,
        entries: PluginEntries,
        declared: &[Declared],
    ) -> Vec<(ErrorCode, String)> {
        match registry.admit(
            &notes().described_by(entries),
            PluginSlot(0),
            declared,
            &[],
            &pdp(),
        ) {
            Ok(_) => Vec::new(),
            Err(problems) => problems
                .iter()
                .map(|problem| (problem.code(), problem.sentence().to_owned()))
                .collect(),
        }
    }

    /// The entries of one row, `note.get`, its texts in each locale as given.
    fn one_row(english: &str, portuguese: &str) -> PluginEntries {
        PluginEntries::parse(
            "[[command]]\nname = \"note.get\"\n",
            &[(Locale::En, english), (Locale::PtBR, portuguese)],
        )
        .expect("entries of the shape the kernel reads")
    }

    /// A command whose entry misses a locale is refused, naming both.
    ///
    /// At `activate`, the command and the locale — the whole locale, or one
    /// text of it.
    #[test]
    fn a_command_whose_entry_misses_a_locale_is_refused_naming_the_command_and_the_locale() {
        let english = row_of("note.get", "Read a note", "Shows what a note says", "note");
        assert_eq!(
            refused_with(
                &Registry::from_catalog(),
                one_row(&english, ""),
                &[a_read("notes.note.get")]
            ),
            [(
                ErrorCode::PluginEntriesInvalid,
                "`notes.note.get` of `noctua:notes` says nothing at all in `pt-BR`".to_owned()
            )]
        );
        let unnamed = row_of(
            "note.get",
            "Ler uma nota",
            "Mostra o que uma nota diz",
            "nota",
        )
        .replace("display_name = \"Ler uma nota\"\n", "");
        assert_eq!(
            refused_with(
                &Registry::from_catalog(),
                one_row(&english, &unnamed),
                &[a_read("notes.note.get")]
            ),
            [(
                ErrorCode::PluginEntriesInvalid,
                "`notes.note.get` of `noctua:notes` has no `pt-BR` display name".to_owned()
            )]
        );
    }

    /// A command the plugin's `commands.toml` lists no row for is refused,
    /// and nothing of the plugin is admitted.
    #[test]
    fn a_command_with_no_row_in_commands_toml_is_refused() {
        let registry = Registry::from_catalog();
        let declared = [a_read("notes.note.get"), a_read("notes.note.peek")];
        assert_eq!(
            refused_with(
                &registry,
                entries(&[("note.get", "Read a note")]),
                &declared
            ),
            [(
                ErrorCode::PluginEntriesInvalid,
                "`notes.note.peek` of `noctua:notes` has no entry: its plugin's \
                 `commands.toml` lists no `note.peek`"
                    .to_owned()
            )]
        );
        assert_eq!(
            refused_with(&registry, PluginEntries::default(), &declared[..1]).len(),
            1,
            "a manifest no entries describe registers none of its commands"
        );
    }

    /// A plugin that describes a command of the catalog, or copies a catalog
    /// command's name, is refused.
    ///
    /// **The catalog's entry is the one it is shown by**: a row for a command
    /// the catalog describes is refused, and so is a display name a command of
    /// the catalog is shown by; with no row, the command is admitted as the
    /// catalog writes it.
    #[test]
    fn a_plugin_that_describes_a_catalog_command_or_copies_its_name_is_refused() {
        let registry = over([a_read("notes.note.get")]);
        assert_eq!(
            refused_with(
                &registry,
                entries(&[("note.get", "Read a note")]),
                &[a_read("notes.note.get")]
            ),
            [(
                ErrorCode::PluginEntriesInvalid,
                "`notes.note.get` of `noctua:notes` is described by the catalog, and its \
                 plugin's `commands.toml` describes it too, as `note.get`: the catalog's entry \
                 is the one it is shown by"
                    .to_owned()
            )]
        );
        assert_eq!(
            refused_with(
                &registry,
                PluginEntries::default(),
                &[a_read("notes.note.get")]
            ),
            []
        );
        let copied = Declared {
            title: "Redo".to_owned(),
            ..a_read("notes.note.get")
        };
        let problems = refused_with(
            &Registry::from_catalog(),
            entries(&[("note.get", "Redo")]),
            &[copied],
        );
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert_eq!(problems[0].0, ErrorCode::PluginEntriesInvalid);
        assert!(
            problems[0]
                .1
                .ends_with("which the catalog's `kernel.redo` is shown by"),
            "{}",
            problems[0].1
        );
    }

    /// A title the component declares that is not its entry's English
    /// display name is refused, and an empty title is refused once, as a
    /// field the declaration misses.
    #[test]
    fn a_title_that_is_not_the_english_display_name_is_refused() {
        let renamed = Declared {
            title: "Read the note".to_owned(),
            ..a_read("notes.note.get")
        };
        assert_eq!(
            refused_with(
                &Registry::from_catalog(),
                entries(&[("note.get", "Read a note")]),
                &[renamed]
            ),
            [(
                ErrorCode::PluginEntriesInvalid,
                "`notes.note.get` of `noctua:notes` declares the title `Read the note`, and its \
                 English display name is `Read a note`: the two are one text, written in the \
                 component and in its directory"
                    .to_owned()
            )]
        );
        let untitled = Declared {
            title: String::new(),
            ..a_read("notes.note.get")
        };
        assert_eq!(
            refused_with(
                &Registry::from_catalog(),
                entries(&[("note.get", "Read a note")]),
                &[untitled]
            ),
            [(
                ErrorCode::PluginDescriptorMissingField,
                "`notes.note.get` of `noctua:notes` declares no title".to_owned()
            )]
        );
    }

    /// A display name a command of the catalog is shown by is refused, in
    /// either locale and whichever locale the catalog shows it in, as the
    /// untranslated rule compares sentences.
    #[test]
    fn a_display_name_the_catalog_shows_is_refused_in_either_locale() {
        let registry = Registry::from_catalog();
        for (english, portuguese, locale, shown) in [
            ("Undo", "Ler uma nota", "en", "Undo"),
            ("undo.", "Ler uma nota", "en", "undo."),
            ("Read a note", "Desfazer", "pt-BR", "Desfazer"),
            ("Desfazer", "Ler uma nota", "en", "Desfazer"),
        ] {
            let entries = one_row(
                &row_of("note.get", english, "Shows what a note says", "note"),
                &row_of("note.get", portuguese, "Mostra o que uma nota diz", "nota"),
            );
            let declared = Declared {
                title: english.to_owned(),
                ..a_read("notes.note.get")
            };
            assert_eq!(
                refused_with(&registry, entries, &[declared]),
                [(
                    ErrorCode::PluginEntriesInvalid,
                    format!(
                        "`notes.note.get` of `noctua:notes` has the `{locale}` display name \
                         `{shown}`, which the catalog's `kernel.undo` is shown by"
                    )
                )],
                "{english} / {portuguese}"
            );
        }
    }

    /// A plugin's entry past the bound in bytes is refused, as a catalog's is.
    ///
    /// **The catalog's bound, read by the kernel**: a paragraph inside its
    /// bound in characters, two bytes each, takes the locale's texts past the
    /// bound together, and an entry at it is admitted.
    #[test]
    fn a_plugins_entry_past_the_entry_bound_is_refused_as_the_catalogs_would_be() {
        let most = usize::try_from(nc_catalog::KERNEL.entry_bytes).expect("fits");
        let portuguese = row_of(
            "note.get",
            "Ler uma nota",
            "Mostra o que uma nota diz",
            "nota",
        );
        // A tag is its own bytes and the quotation marks and the comma it is
        // listed in.
        let others = "Read a note".len() + "Shows what a note says".len() + "note".len() + 3;
        let written = |paragraph: &str| {
            format!(
                "[command.\"note.get\"]\ndisplay_name = \"Read a note\"\nline = \"Shows what a \
                 note says\"\nparagraph = \"{paragraph}\"\ntags = [\"note\"]\n"
            )
        };
        let at_the_bound = "é".repeat((most - others) / 2) + &"a".repeat((most - others) % 2);
        assert_eq!(
            refused_with(
                &Registry::from_catalog(),
                one_row(&written(&at_the_bound), &portuguese),
                &[a_read("notes.note.get")]
            ),
            []
        );
        let past = format!("{at_the_bound}a");
        assert_eq!(
            refused_with(
                &Registry::from_catalog(),
                one_row(&written(&past), &portuguese),
                &[a_read("notes.note.get")]
            ),
            [(
                ErrorCode::PluginEntriesInvalid,
                format!(
                    "`notes.note.get` of `noctua:notes` has `en` texts of {} bytes together, \
                     each tag with the quotation marks and the comma it is listed in, past the \
                     {most} an entry may hold in one locale",
                    most + 1
                )
            )]
        );
    }

    /// A category a plugin's entry names is held to the catalog's, as a
    /// command of the catalog's is.
    #[test]
    fn a_category_a_plugins_entry_names_is_one_the_catalog_holds() {
        let english = row_of("note.get", "Read a note", "Shows what a note says", "note");
        let portuguese = row_of(
            "note.get",
            "Ler uma nota",
            "Mostra o que uma nota diz",
            "nota",
        );
        let filed = PluginEntries::parse(
            "[[command]]\nname = \"note.get\"\ncategories = [\"noctua.reference\"]\n",
            &[(Locale::En, &english), (Locale::PtBR, &portuguese)],
        )
        .expect("entries of the shape the kernel reads");
        assert_eq!(
            refused_with(
                &Registry::from_catalog(),
                filed,
                &[a_read("notes.note.get")]
            ),
            [(
                ErrorCode::PluginEntriesInvalid,
                "`notes.note.get` of `noctua:notes` names the category `noctua.reference`, \
                 which its registry does not hold"
                    .to_owned()
            )]
        );
    }

    /// A row no declared command claims is neither registered nor refused.
    #[test]
    fn a_row_no_declared_command_claims_is_neither_registered_nor_refused() {
        let mut registry = Registry::from_catalog();
        let declared = [a_read("notes.note.get")];
        let admitted = registry
            .admit(
                &notes().described_by(entries(&[
                    ("note.get", "Read a note"),
                    ("note.spare", "Read a spare note"),
                ])),
                PluginSlot(0),
                &declared,
                &[],
                &pdp(),
            )
            .expect("a row left over is no refusal");
        registry.register(admitted);
        assert!(registry.lookup("notes.note.get").is_some());
        assert!(registry.lookup("notes.note.spare").is_none());
    }

    /// A plugin's command is registered with its entry in every locale, as
    /// its plugin wrote it.
    #[test]
    fn a_registered_plugin_command_carries_its_entry_in_every_locale() {
        let registry = Registry::from_catalog();
        let english = row_of("note.get", "Read a note", "Shows what a note says", "note");
        let portuguese = row_of(
            "note.get",
            "Ler uma nota",
            "Mostra o que uma nota diz",
            "nota",
        );
        let glyphed = PluginEntries::parse(
            "[[command]]\nname = \"note.get\"\nglyph = \"U+F0E2\"\n",
            &[(Locale::En, &english), (Locale::PtBR, &portuguese)],
        )
        .expect("entries of the shape the kernel reads");
        let admitted = registry
            .admit(
                &notes().described_by(glyphed),
                PluginSlot(0),
                &[a_read("notes.note.get")],
                &[],
                &pdp(),
            )
            .expect("every rule admits it");
        assert_eq!(
            admitted[0].listed.entry,
            CommandEntry::new(
                vec![
                    texts("Read a note", "Shows what a note says", "note"),
                    texts("Ler uma nota", "Mostra o que uma nota diz", "nota"),
                ],
                Vec::new(),
                Some('\u{f0e2}'),
            )
        );
    }
}
