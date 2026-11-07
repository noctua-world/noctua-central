//! The commands the kernel can dispatch: its own, and every one an activated
//! plugin declared and the kernel admitted.
//!
//! **The kernel's own come from the catalog.** Every command of
//! `catalog/commands.toml` whose namespace is one the kernel keeps —
//! [`RESERVED`]: `journal.`, `kernel.`, `limits.` — is a built-in, registered
//! when the registry is made. A catalog command in any other namespace is one
//! the catalog describes for a plugin to implement (`NOCTUA.md §15.2`): it is
//! registered when a plugin declares it, matching the catalog in every field a
//! dispatch is decided by, and is not registered otherwise. The title is the
//! one field not compared — the catalog's is a message key both locales
//! translate, and it is the one listed.
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
//!   `[NC-PLUG-0009]`: an action the schema declares, a resource type that
//!   action applies to, and the whole realm selected only by a command that
//!   reads — a realm-wide change is the kernel's;
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
//! which never reaches the registry. A plugin that would select the realm to
//! change it is refused, because a permit for that action on the realm would
//! then authorize every such command of every plugin. What it does not answer:
//! whether a plugin's command acts on the resource it was authorized for,
//! inside its own namespace, which is the plugin's code and no check here.

use std::collections::BTreeMap;

use nc_authz::Pdp;
use nc_catalog::{ErrorCode, KERNEL};
use nc_schema::descriptor::{self, DescriptorProblem, Judged, Reversibility};
use nc_schema::selector::REALM_TYPE;

use crate::manifest::PluginManifest;
use crate::{LoadProblem, PluginSlot};

/// The namespaces the kernel keeps for its own commands.
pub const RESERVED: [&str; 3] = ["journal", "kernel", "limits"];

/// The one action a plugin's command may select the whole realm under.
const THE_REALM_IS_READ_WITH: &str = "Read";

/// How many characters of an action, an entity type or a topic a refusal repeats.
///
/// A command's name is not cut: a refusal repeats it only when it is well
/// formed, which holds it to ASCII and to `NAME_MAX` characters, and a name
/// cut short would be one the plugin never declared.
const QUOTED: usize = 64;

/// Who runs a registered command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    /// The kernel itself: a command of the catalog in a namespace it keeps.
    Kernel,
    /// The plugin loaded in this slot.
    Plugin(PluginSlot),
}

/// A command the kernel can dispatch, as the registry lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredCommand {
    /// Its name.
    pub name: String,
    /// What a person is shown for it: the catalog's message key for a command
    /// the catalog declares, the plugin's own label for one it does not.
    pub title: String,
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
    /// `realm`, or `argument:<name>`.
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

    /// The declaration as the registry lists it, run by `owner`.
    fn listed(&self, owner: Owner) -> RegisteredCommand {
        RegisteredCommand {
            name: self.name.clone(),
            title: self.title.clone(),
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
    /// The kernel's own commands, read out of the catalog.
    ///
    /// # Panics
    ///
    /// When a command of the catalog is one the judge refuses — which the
    /// catalog compiler refuses to build, with the same judge.
    pub(crate) fn from_catalog() -> Self {
        Self::over(nc_catalog::COMMANDS.iter().map(|command| Declared {
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
        }))
    }

    /// A registry over `catalog`: each command in a namespace the kernel keeps
    /// is the kernel's, and every other one is described for a plugin.
    fn over(catalog: impl IntoIterator<Item = Declared>) -> Self {
        let mut registry = Self::default();
        for command in catalog {
            let judged = descriptor::judge(&command.as_written()).unwrap_or_else(|problems| {
                panic!(
                    "the catalog's `{}` is one the judge refuses, and the catalog compiler \
                     holds every command to the same judge: {problems:?}",
                    command.name
                )
            });
            let admitted = Admitted {
                listed: command.listed(Owner::Kernel),
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
            if let (true, Some(judged)) = (problems.len() == before, judged) {
                admitted.push(self.described.get(&command.name).map_or_else(
                    || Admitted {
                        listed: command.listed(Owner::Plugin(slot)),
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
}

/// Why a judged command cannot be decided as declared, when it cannot: an
/// action the schema does not declare, a resource type the action does not
/// apply to, or the whole realm selected by a command that does not read it.
fn undecidable(
    command: &Declared,
    judged: &Judged,
    actions: &[String],
    pdp: &Pdp,
) -> Option<String> {
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
    use super::{Declared, Owner, RESERVED, Registry, quoted};
    use crate::PluginSlot;
    use crate::manifest::PluginManifest;
    use nc_catalog::ErrorCode;
    use nc_schema::descriptor::{NAME_MAX, Reversibility};

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
        match registry.admit(manifest, PluginSlot(0), commands, &subscriptions, &pdp()) {
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
            assert_eq!(command.title, written.title_key);
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
        let admitted = registry
            .admit(
                &notes(),
                PluginSlot(3),
                &[a_read("notes.note.get"), a_read("notes.note.peek")],
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
        let admitted = registry
            .admit(
                &notes(),
                PluginSlot(3),
                &[a_read("notes.note.get")],
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
        let first = registry
            .admit(
                &notes(),
                PluginSlot(0),
                &[a_read("notes.note.get")],
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
        let registry = Registry::over([a_read("notes.note.get")]);
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
            &Registry::over([Declared {
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
        let elsewhere = Registry::over([Declared {
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
    /// Its title is the catalog's key, it is run by the plugin that claimed it,
    /// and one selecting another argument than the catalog's is refused naming
    /// the selector.
    #[test]
    fn a_claimed_command_is_the_catalogs_and_its_selector_is_held_to_it() {
        let described = Declared {
            title: "command.notes.note.get.title".to_owned(),
            args_schema: TWO.to_owned(),
            ..a_read("notes.note.get")
        };
        let registry = Registry::over([described]);
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
        assert_eq!(admitted[0].listed.title, "command.notes.note.get.title");
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
        let _ = Registry::over([Declared {
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
}
