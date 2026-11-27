//! The channel's settings: MCP's switch, `settings.mcp.read` and
//! `settings.mcp.set`, and the four shortcut settings (`NOCTUA.md §12.15`).
//!
//! **A setting is two commands of the kernel's own, and its value is the
//! kernel's entry in the channel's store.** `settings.<setting>.read` answers
//! the setting as the one object `settings.<setting>.set` takes whole. Both are
//! decided on the platform's feature of the setting's name — `Read` and `Write`
//! on `Feature::"mcp"`, which the family pack gives every person of the realm
//! (`family.a_member_switches_mcp`) and no agent or device — and the settings
//! are listed under category `noctua.settings`, beside a person's agents'
//! commands (`crate::agents`), which change who may act for them too, so there
//! is no second list of them. The value is one entry of the local store under the holder
//! [`HOLDER`], which no plugin's identity can be (`crate::manifest`), and a
//! setting never written is its default: MCP is off until a person turns it
//! on. The store is the channel's profile's own, so a setting switched in one
//! channel is switched in no other (`NOCTUA.md §17.1`).
//!
//! **The set is a step the person's undo takes back.** It stages the value in
//! the unit the decision point opened for it, which commits whole with its
//! journal entry or not at all (`crate::units`), and its one effect is that
//! entry — so it is no step that left the machine
//! ([`writes_only_its_entry`], `crate::history`), and an undo puts the value
//! back as it was and reaches the steps before it. **Switching it on waits for
//! the person's confirmation in the desktop interface** ([`switches_mcp_on`]),
//! as a command that cannot be undone does — held while the desktop the daemon
//! started is attached, refused with `[NC-KERN-0032]` otherwise — and so does
//! an undo of a switch off or a redo of a switch on, which would put it back
//! on ([`puts_mcp_on`], `crate::history::would_switch_mcp_on`); switching it
//! off never does. **It runs only at the top of
//! a call tree** (`crate::registry::only_at_the_top`): dispatched from inside a
//! command — a plugin run as the person, a timer's or an event's callback — it
//! is refused with `[NC-KERN-0025]` before it is decided. The read runs
//! anywhere, since it changes nothing.
//!
//! **A shortcut setting's value is a file, never an entry** ([`Which`],
//! `NOCTUA.md §12.8`): `settings.leader`, `settings.chord`,
//! `settings.menu-keys` and `settings.chords` read and write the channel's
//! keybinding configuration through the keeper the kernel's host says
//! ([`Shortcuts`], `crate::Kernel::keep_shortcuts`). The kernel reads what a
//! set asks by the keybinding grammar, has the keeper judge it, journals the
//! set as allowed and only then has it written (`crate::dispatch`), since a
//! file is outside every unit — so a shortcut's set is a step that left the
//! machine, which every undo stops at. All four are decided on the
//! interface's feature, `Feature::"interface"`, which the pack gives every
//! person and no agent or device: the keys are how a person reaches and
//! drives their own interface. A kernel told of no keeper refuses them.
//!
//! # What this answers (`NOCTUA.md §2.4`)
//!
//! Who may change the keys a person's commands are reached by: that person,
//! by a decided and journaled dispatch at the top of a call tree, never a
//! plugin, an agent or a device; and what a set may write: one setting's
//! value, read by the closed grammar and refused before anything is
//! journaled as allowed where its keeper would not write it.
//!
//! Who may switch whether agents are let in: a person, by a decided and
//! journaled dispatch of their own, never a plugin, a timer or an event in
//! their name, and never an agent or a device, which the permit refuses; and
//! who may switch them in: that person, confirmed in the desktop interface,
//! never anything that only speaks the protocol — the CLI, a process running
//! as the person, an undo or a redo. A
//! value this module did not write — any text in the entry but the two it
//! writes — is refused as a row the kernel did not write (`internal`), never
//! read as on or as off. **What it does not answer:** a process running as the
//! same user, which can run the CLI as the person — to switch MCP off — or
//! edit the entry in the profile's database, as it can any of the person's
//! data, or tamper with the desktop interface (`NOCTUA.md §11.1`); and what MCP being
//! on lets in, which is the daemon's to enforce and the decision point's to
//! decide per call.

use std::sync::{Arc, Mutex};

use nc_canon::Value;
use nc_catalog::ErrorCode;
use nc_ids::OpaqueId;
use nc_local::{Entry, LocalError};
use nc_schema::keys::{Chord, Key, Leader, Modifier, Table, Tables};

use crate::backend::StoreBackend;
use crate::dispatch::{self, CallTree, Failure, Refusal, RefusalKind};
use crate::{Environment, Shared, units};

/// The command that reads whether MCP is on.
pub(crate) const MCP_READ: &str = "settings.mcp.read";

/// The command that switches MCP on or off.
pub(crate) const MCP_SET: &str = "settings.mcp.set";

/// Whether MCP is on: the one field of MCP's setting.
const ON: &str = "on";

/// The holder the kernel keeps its settings under in the store: the store's
/// own name for the kernel's holder.
///
/// Never a plugin's identity, which is `<word>:<word>`, so no plugin reads or
/// writes it.
pub(crate) const HOLDER: &str = nc_local::KERNEL;

/// The key MCP's setting is kept under.
const MCP: &str = "settings.mcp";

/// Whether MCP is on before a person writes it: off.
const MCP_DEFAULT: bool = false;

/// How many characters of an entry no writer here wrote a refusal repeats.
const QUOTED: usize = 64;

/// The channel's settings, each one never written its default.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// Whether MCP is on, which a person switches with `settings.mcp.set`.
    pub mcp: bool,
}

/// What MCP's setting is written as, and answered as: `{"on":<bool>}`.
fn written(on: bool) -> String {
    Value::object([(ON, Value::Bool(on))]).to_canonical()
}

/// Whether a command's one effect is its setting's entry, in its own unit.
///
/// The history takes such a command for no step that left the machine, so
/// the person's undo puts the value back and reaches past it. It names the
/// command itself and never the namespace: a later setting whose effect does
/// leave the machine is not taken back by being a setting.
pub(crate) fn writes_only_its_entry(name: &str) -> bool {
    name == MCP_SET
}

/// Whether a call of `name` with `args` switches MCP on: the set asking for
/// on, whatever MCP is now, and never the set asking for off.
///
/// **What the decision point holds for a person's confirmation in the
/// desktop interface**, as it holds a command that cannot be undone
/// (`crate::dispatch`): MCP on lets an agent in, so nothing that only speaks
/// the protocol — the CLI, a process running as the person, a dispatched
/// command — switches it on.
pub(crate) fn switches_mcp_on(name: &str, args: &serde_json::Value) -> bool {
    name == MCP_SET && args.get(ON).and_then(serde_json::Value::as_bool) == Some(true)
}

/// Whether putting `to` back in the key `key` of `holder`, over `from`, would
/// switch MCP on: what an undo of a switch off and a redo of a switch on wait
/// for a person's confirmation for, as the set asking for on does
/// (`crate::history::would_switch_mcp_on`).
pub(crate) fn puts_mcp_on(
    holder: &str,
    key: &str,
    from: Option<&Entry>,
    to: Option<&Entry>,
) -> bool {
    let on = |entry: Option<&Entry>| entry.is_some_and(|entry| entry.value == written(true));
    holder == HOLDER && key == MCP && on(to) && !on(from)
}

/// The settings `store` holds committed for `realm`.
///
/// # Errors
///
/// What the store refused the read with, as `crate::units::store_failed`
/// answers it; and `internal`, with the code of a row the kernel did not
/// write, for an entry holding anything but what this module writes.
pub(crate) async fn read<S: StoreBackend>(
    store: &S,
    realm: &OpaqueId,
) -> Result<Settings, Refusal> {
    let entry = store
        .get(realm, HOLDER, MCP)
        .await
        .map_err(|failed| units::store_failed(&failed))?;
    Ok(Settings {
        mcp: on_in(entry.as_ref())?,
    })
}

/// Whether `entry`, MCP's committed entry, says MCP is on.
///
/// None is the default. **Only the two texts this module writes are read**,
/// whatever else the store keeps: JSON of another shape, `on` of another type,
/// or a field beside it is a row the kernel did not write.
fn on_in(entry: Option<&Entry>) -> Result<bool, Refusal> {
    let Some(entry) = entry else {
        return Ok(MCP_DEFAULT);
    };
    [true, false]
        .into_iter()
        .find(|on| entry.value == written(*on))
        .ok_or_else(|| {
            units::store_failed(&LocalError::Unexpected(format!(
                "the kernel's entry `{MCP}` holds {}, which nothing here writes, so it was \
                 read as neither on nor off",
                crate::registry::quoted_up_to(&entry.value, QUOTED)
            )))
        })
}

/// A shortcut setting (`NOCTUA.md §12.8`, `§12.15`): one of the four a
/// channel's choice of keys is set by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Which {
    /// `settings.leader`: the channel's Leader.
    Leader,
    /// `settings.chord`: the chord modifier.
    Chord,
    /// `settings.menu-keys`: the leader menu's keys.
    MenuKeys,
    /// `settings.chords`: the keys pressed with the Leader held.
    Chords,
}

impl Which {
    /// Every one, in the order `§12.15` names them.
    #[cfg(test)]
    pub(crate) const ALL: [Self; 4] = [Self::Leader, Self::Chord, Self::MenuKeys, Self::Chords];

    /// The command that reads it.
    pub(crate) const fn read(self) -> &'static str {
        match self {
            Self::Leader => "settings.leader.read",
            Self::Chord => "settings.chord.read",
            Self::MenuKeys => "settings.menu-keys.read",
            Self::Chords => "settings.chords.read",
        }
    }

    /// The command that sets it.
    pub(crate) const fn set(self) -> &'static str {
        match self {
            Self::Leader => "settings.leader.set",
            Self::Chord => "settings.chord.set",
            Self::MenuKeys => "settings.menu-keys.set",
            Self::Chords => "settings.chords.set",
        }
    }

    /// The one field of its object.
    const fn field(self) -> &'static str {
        match self {
            Self::Leader => "leader",
            Self::Chord => CHORD,
            Self::MenuKeys => "keys",
            Self::Chords => "chords",
        }
    }
}

/// In a chord's entry, whether the chord modifier is held as well; and the
/// one field of the chord modifier's setting.
const CHORD: &str = "chord";

/// In an entry of the leader menu's keys or of the chords, the command a key
/// runs.
const COMMAND: &str = "command";

/// In an entry of the leader menu's keys or of the chords, the key.
const KEY: &str = "key";

/// The keys in force for a channel, as its keybinding configuration and the
/// catalog leave them: what the shortcut settings answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keys {
    /// The Leader in force.
    pub leader: Leader,
    /// The chord modifier in force.
    pub chord: Modifier,
    /// Every binding in force, beside the command it runs.
    pub tables: Tables,
}

/// One shortcut setting's whole value, as its set asks for it: read by the
/// keybinding grammar (`nc_schema::keys`), and judged by nothing yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shortcut {
    /// The channel's Leader, a chord as the grammar writes one.
    Leader(Chord),
    /// The chord modifier.
    Chord(Modifier),
    /// The leader menu's keys: every key of the menu's table, beside the
    /// command it runs.
    Menu(Vec<(String, Key)>),
    /// The chords: every key pressed with the Leader held, beside the command
    /// it runs and whether the chord modifier is held as well.
    Held(Vec<(String, Key, bool)>),
}

/// Where a kernel's channel keeps its person's choice of keys
/// (`NOCTUA.md §12.8`): the channel's keybinding configuration in a daemon,
/// read and written through `nc_channel::keys`.
///
/// **The kernel decides and journals, and the keeper judges and writes.** A
/// shortcut setting's read answers what [`Shortcuts::read`] answers; its set
/// is rehearsed by [`Shortcuts::proposed`], which writes nothing, journaled
/// as allowed, and only then made by [`Shortcuts::written`]
/// (`crate::dispatch`). The three are called with the kernel's bus held, one
/// tree at a time, and may block for as long as a small file takes.
pub trait Shortcuts: core::fmt::Debug + Send + Sync + 'static {
    /// The keys in force.
    ///
    /// # Errors
    ///
    /// A [`Refusal`] when they could not be said.
    fn read(&self) -> Result<Keys, Refusal>;

    /// Judges `shortcut` over the choice kept now, and writes nothing.
    ///
    /// # Errors
    ///
    /// A [`Refusal`] for a choice the keeper would not write: one that
    /// collides, or one made over a file it does not write over.
    fn proposed(&self, shortcut: &Shortcut) -> Result<(), Refusal>;

    /// Judges `shortcut` again and writes it, answering the keys then in
    /// force.
    ///
    /// # Errors
    ///
    /// What [`Shortcuts::proposed`] refuses, and a [`Refusal`] for a choice
    /// that could not be written or delivered; nothing is changed then.
    fn written(&self, shortcut: &Shortcut) -> Result<Keys, Refusal>;
}

/// The keeper `shared` was told of.
///
/// # Errors
///
/// `unavailable` with `[NC-CHAN-0027]` for a kernel told of none, which keeps
/// no keybinding configuration to read or write.
fn keeper<E: Environment>(shared: &Shared<E>, name: &str) -> Result<Arc<dyn Shortcuts>, Failure> {
    dispatch::lock(&shared.shortcuts).clone().ok_or_else(|| {
        Failure::new(
            Refusal::new(
                RefusalKind::Unavailable,
                ErrorCode::ChannelShortcutsUnsaved,
                format!(
                    "`{name}` was asked of a kernel that keeps no keybinding configuration, so \
                     nothing was read or changed"
                ),
            ),
            "shortcuts-unkept",
        )
    })
}

/// What a shortcut setting is answered as, and takes whole: `{"leader":
/// "<chord>"}`, `{"chord":"<modifier>"}`, `{"keys":[{"command":…,"key":…}]}`
/// for every key of the menu's table and `{"chords":[{"chord":<bool>,
/// "command":…,"key":…}]}` for every key of the two tables a held Leader has
/// — each table key by key, as the vocabulary orders them.
fn answered(which: Which, keys: &Keys) -> String {
    let entries = |held: bool| {
        Value::Array(
            keys.tables
                .iter()
                .filter(|(binding, _)| (binding.table != Table::Menu) == held)
                .map(|(binding, command)| {
                    let mut entry = vec![
                        (COMMAND, Value::text(command)),
                        (KEY, Value::text(binding.key.code())),
                    ];
                    if held {
                        entry.push((CHORD, Value::Bool(binding.table == Table::HeldWithChord)));
                    }
                    Value::object(entry)
                })
                .collect(),
        )
    };
    let value = match which {
        Which::Leader => Value::text(keys.leader.written()),
        Which::Chord => Value::text(keys.chord.word()),
        Which::MenuKeys => entries(false),
        Which::Chords => entries(true),
    };
    Value::object([(which.field(), value)]).to_canonical()
}

/// The refusal of a shortcut the grammar does not write, `[NC-CHAN-0025]`,
/// saying `what` of the set `which`.
fn reserved(which: Which, what: &str) -> Failure {
    Failure::new(
        Refusal::new(
            RefusalKind::Invalid,
            ErrorCode::ChannelShortcutKeyReserved,
            format!(
                "`{}` was asked for {what}, so nothing was written and the keys in force stand",
                which.set()
            ),
        ),
        "shortcut-reserved",
    )
}

/// The shortcut a set of `which` asks for in `args`, read by the keybinding
/// grammar.
///
/// **Read as the file is read**: a chord and a modifier by the one spelling
/// the grammar writes, a key by the name `KeyboardEvent.code` gives it in the
/// closed vocabulary — so a modifier alone, a lock key, the numpad, a media
/// key and Esc name no key — and a command by a name a command can have.
/// What a refusal repeats of the arguments is escaped and cut at [`QUOTED`]
/// characters.
///
/// # Errors
///
/// `invalid` with `[NC-CHAN-0025]` for a text the grammar does not write;
/// and `invalid` with the arguments' own code for arguments of another
/// shape, which the schema refuses first.
fn asked_shortcut(which: Which, args: &serde_json::Value) -> Result<Shortcut, Failure> {
    let quoted = |text: &str| crate::registry::quoted_up_to(text, QUOTED);
    let misshapen = || {
        Failure::new(
            Refusal::new(
                RefusalKind::Invalid,
                ErrorCode::ValidationArgumentsInvalid,
                format!(
                    "`{}` was asked with no `{}` of the shape it takes, so nothing was written",
                    which.set(),
                    which.field()
                ),
            ),
            "arguments-invalid",
        )
    };
    let field = args.get(which.field()).ok_or_else(misshapen)?;
    let entries = |held: bool| -> Result<Vec<(String, Key, bool)>, Failure> {
        field
            .as_array()
            .ok_or_else(misshapen)?
            .iter()
            .map(|entry| {
                let text = |name: &str| {
                    entry
                        .get(name)
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(misshapen)
                };
                let (command, key) = (text(COMMAND)?, text(KEY)?);
                let with_chord = if held {
                    entry
                        .get(CHORD)
                        .and_then(serde_json::Value::as_bool)
                        .ok_or_else(misshapen)?
                } else {
                    false
                };
                if !nc_schema::command_name_is_well_formed(command) {
                    return Err(reserved(
                        which,
                        &format!("{}, which no command can be named", quoted(command)),
                    ));
                }
                let key = Key::of_code(key).ok_or_else(|| {
                    reserved(
                        which,
                        &format!(
                            "the key {}, which is no key a shortcut holds: a key is named as \
                             `KeyboardEvent.code` names it, and the modifiers, the lock keys, \
                             the numpad, the media keys and Esc are kept out",
                            quoted(key)
                        ),
                    )
                })?;
                Ok((command.to_owned(), key, with_chord))
            })
            .collect()
    };
    match which {
        Which::Leader => {
            let text = field.as_str().ok_or_else(misshapen)?;
            Chord::parse(text)
                .map(Shortcut::Leader)
                .map_err(|malformed| {
                    reserved(which, &format!("a Leader that is no chord: {malformed}"))
                })
        }
        Which::Chord => {
            let word = field.as_str().ok_or_else(misshapen)?;
            Modifier::of_word(word).map(Shortcut::Chord).ok_or_else(|| {
                reserved(
                    which,
                    &format!(
                        "the chord modifier {}, which is none of `Control`, `Alt`, `Shift` and \
                         `Super`",
                        quoted(word)
                    ),
                )
            })
        }
        Which::MenuKeys => Ok(Shortcut::Menu(
            entries(false)?
                .into_iter()
                .map(|(command, key, _)| (command, key))
                .collect(),
        )),
        Which::Chords => entries(true).map(Shortcut::Held),
    }
}

/// A shortcut setting's read: the keys in force, as [`answered`] writes
/// `which` of them.
///
/// # Errors
///
/// What [`keeper`] and the keeper's read refuse.
pub(crate) fn shortcut_read<E: Environment>(
    shared: &Shared<E>,
    which: Which,
) -> Result<String, Failure> {
    keeper(shared, which.read())?
        .read()
        .map(|keys| answered(which, &keys))
        .map_err(|refusal| Failure::new(refusal, "shortcuts-unread"))
}

/// The shortcut a set of `which` asks for in `args`, once the keeper judged
/// it one it would write — **nothing written yet**: what the decision point
/// journals as allowed before [`shortcut_written`] makes it.
///
/// # Errors
///
/// What [`asked_shortcut`] and [`keeper`] refuse, and the keeper's refusal of
/// the choice.
pub(crate) fn shortcut_proposed<E: Environment>(
    shared: &Shared<E>,
    which: Which,
    args: &serde_json::Value,
) -> Result<Shortcut, Failure> {
    let shortcut = asked_shortcut(which, args)?;
    keeper(shared, which.set())?
        .proposed(&shortcut)
        .map_err(|refusal| Failure::new(refusal, "shortcut-refused"))?;
    Ok(shortcut)
}

/// A shortcut setting's set, made: `shortcut` written by the keeper, and
/// answered as `which` stands once it is.
///
/// # Errors
///
/// What [`keeper`] refuses, and the keeper's refusal to write the choice,
/// which changed nothing.
pub(crate) fn shortcut_written<E: Environment>(
    shared: &Shared<E>,
    which: Which,
    shortcut: &Shortcut,
) -> Result<String, Failure> {
    keeper(shared, which.set())?
        .written(shortcut)
        .map(|keys| answered(which, &keys))
        .map_err(|refusal| Failure::new(refusal, "shortcut-unwritten"))
}

/// `settings.mcp.read`: whether MCP is on, `{"on":<bool>}`.
///
/// # Errors
///
/// What [`read`] refuses, journaled as the store's refusal and answered
/// with its code.
pub(crate) async fn mcp_read<E: Environment>(shared: &Shared<E>) -> Result<String, Failure> {
    read(&*shared.store, &shared.realm)
        .await
        .map(|settings| written(settings.mcp))
        .map_err(|refusal| Failure::new(refusal, "store-refused"))
}

/// `settings.mcp.set`: MCP switched as `args` says, staged in the tree's unit
/// over what the store holds committed now, and answered as it will stand.
///
/// # Errors
///
/// `invalid` when `args` holds no boolean `on`, which the schema refuses
/// first; what the store refused the read with; `internal` with
/// `[NC-KERN-0008]` when the tree holds no unit, which the decision point
/// opened for the command; and the unit's own bounds.
pub(crate) async fn mcp_set<E: Environment>(
    shared: &Shared<E>,
    tree: &Arc<Mutex<CallTree>>,
    args: &serde_json::Value,
) -> Result<String, Failure> {
    let on = asked(args)?;
    let committed = shared
        .store
        .get(&shared.realm, HOLDER, MCP)
        .await
        .map_err(|failed| Failure::new(units::store_failed(&failed), "store-refused"))?;
    let mut tree = dispatch::lock(tree);
    let unit = tree.unit_mut().ok_or_else(|| {
        Failure::new(
            Refusal::new(
                RefusalKind::Internal,
                ErrorCode::KernelUnitAborted,
                format!("`{MCP_SET}` ran outside the unit the decision point opened for it"),
            ),
            "unit-lost",
        )
    })?;
    let id = unit.unit();
    unit.overlay_mut()
        .stage(id, HOLDER, MCP, committed.as_ref(), Some(written(on)))
        .map_err(|refusal| Failure::new(refusal, "bound-reached"))?;
    Ok(written(on))
}

/// The `on` a set asks for.
///
/// # Errors
///
/// `invalid` when `args` holds no boolean `on`: a set never guesses which
/// way to switch.
fn asked(args: &serde_json::Value) -> Result<bool, Failure> {
    args.get(ON)
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| {
            Failure::new(
                Refusal::new(
                    RefusalKind::Invalid,
                    ErrorCode::ValidationArgumentsInvalid,
                    format!(
                        "`{MCP_SET}` was asked with no boolean `{ON}`, so nothing was switched"
                    ),
                ),
                "arguments-invalid",
            )
        })
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use nc_local::{Expect, Quotas, StagedWrite, UnitKind, UnitMeta, UnitRecord};

    use nc_catalog::ErrorCode;
    use nc_schema::keys::{Chord, Key, Leader, Modifier};

    use super::{
        CHORD, COMMAND, HOLDER, KEY, Keys, MCP, MCP_READ, MCP_SET, ON, QUOTED, Settings, Shortcut,
        Which, answered, asked, asked_shortcut, read, writes_only_its_entry, written,
    };
    use crate::backend::{MemoryBackend, StoreBackend as _};
    use crate::builtin::Builtin;
    use crate::dispatch::RefusalKind;

    /// Every argument `settings.mcp.read` reads, by the name the command
    /// declares: none.
    const READ_READS: [&str; 0] = [];

    /// Every argument `settings.mcp.set` reads, by the name the command
    /// declares.
    const SET_READS: [&str; 1] = [ON];

    /// The declared settings are the ones this module implements.
    ///
    /// **Both ways**, against `Command::arguments`, which the compiler emits
    /// from the parse it validates the schema with, and the decision each is
    /// made under: `Read` and `Write` on the platform's `mcp` feature, never
    /// the realm, the set a change of state that can be undone and the read
    /// none — so a unit is opened for the set alone, and the permit that
    /// decides them is a person's.
    #[test]
    fn the_declared_settings_are_the_ones_this_module_implements() {
        for (name, reads, action, mutating) in [
            (MCP_READ, &READ_READS[..], "Read", false),
            (MCP_SET, &SET_READS[..], "Write", true),
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
                (action, "feature:mcp", mutating),
                "`{name}`"
            );
            assert_eq!(
                declared.categories,
                ["noctua.settings"],
                "`{name}` is not listed among the settings"
            );
        }
        assert!(matches!(
            nc_catalog::command(MCP_READ)
                .expect("declared")
                .reversibility,
            nc_catalog::Reversibility::Read
        ));
        assert!(matches!(
            nc_catalog::command(MCP_SET)
                .expect("declared")
                .reversibility,
            nc_catalog::Reversibility::Write
        ));
        assert_eq!(
            (MCP_READ, MCP_SET, ON, MCP),
            (
                "settings.mcp.read",
                "settings.mcp.set",
                "on",
                "settings.mcp"
            )
        );
        assert_eq!(Builtin::named(MCP_READ), Some(Builtin::McpRead));
        assert_eq!(Builtin::named(MCP_SET), Some(Builtin::McpSet));
    }

    /// The settings are held under no identity a plugin can have.
    ///
    /// A plugin's identity is `<word>:<word>`, and the store keys every entry
    /// by the identity it is kept for, so no plugin reads, writes or lists the
    /// kernel's.
    #[test]
    fn the_settings_are_held_under_no_identity_a_plugin_can_have() {
        assert!(!crate::manifest::id_is_well_formed(HOLDER), "`{HOLDER}`");
        assert!(
            crate::manifest::id_is_well_formed("noctua:kernel"),
            "the rule refuses every identity, and so tells nothing of this one"
        );
    }

    /// The world the policy cases are about, which the store keys `home` by.
    fn world() -> nc_authz::MemoryDirectory {
        let pdp = nc_authz::Pdp::embedded("family").expect("the family pack is carried");
        nc_authz::MemoryDirectory::from_world(include_str!("../../../policy/world.json"), &pdp)
            .expect("the world loads")
    }

    /// A store holding `value` as MCP's entry, committed by a unit nobody
    /// dispatched, and the realm it holds it in.
    async fn holding(value: &str) -> (MemoryBackend, nc_ids::OpaqueId) {
        let world = world();
        let realm = world.realm_id("home").expect("home is a realm");
        let store = MemoryBackend::default();
        let unit = UnitRecord {
            unit: nc_ids::Minter::new(
                nc_ids::MachineFlake::from_value(1).expect("a flake"),
                nc_ids::SystemClock,
            )
            .mint()
            .expect("a unit"),
            meta: UnitMeta {
                realm: realm.clone(),
                principal: world
                    .principal(nc_authz::PrincipalKind::User, "ana")
                    .expect("ana is a user")
                    .id()
                    .clone(),
                kind: UnitKind::Step,
                label: "a test's".to_owned(),
                committed_at_ms: 0,
            },
            writes: vec![StagedWrite {
                plugin: HOLDER.to_owned(),
                key: MCP.to_owned(),
                expect: Expect::Anything,
                after: Some(value.to_owned()),
            }],
            blobs: Vec::new(),
            unpinned: Vec::new(),
            notes: Vec::new(),
            marks: Vec::new(),
        };
        let quotas = Quotas {
            entries_per_plugin: 10,
            lowered: BTreeMap::new(),
            blobs: 10,
            writes_per_unit: 10,
            notes_per_unit: 10,
        };
        store
            .commit(&unit, &quotas)
            .await
            .expect("the entry commits");
        (store, realm)
    }

    /// A setting never written is its default, and MCP is off.
    ///
    /// **Off until a person turns it on** (`NOCTUA.md §20.3`), and what the two
    /// texts this module writes are read back as, so a store holding nothing
    /// is the one case that answers the default.
    #[tokio::test]
    async fn a_setting_never_written_is_its_default_and_mcp_is_off() {
        let realm = world().realm_id("home").expect("home is a realm");
        let empty = MemoryBackend::default();
        assert_eq!(
            read(&empty, &realm).await.expect("an empty store is read"),
            Settings { mcp: false }
        );
        for on in [true, false] {
            let (store, realm) = holding(&written(on)).await;
            assert_eq!(
                read(&store, &realm).await.expect("a written entry is read"),
                Settings { mcp: on },
                "{}",
                written(on)
            );
        }
        assert_eq!(written(true), r#"{"on":true}"#);
    }

    /// A setting no writer here wrote is refused, and never read as on or off.
    ///
    /// **Each shape an entry the store keeps could hold**: a bare boolean or
    /// text, `on` of another type or missing, and a field beside it — each
    /// refused as a row the kernel did not write, its text repeated cut and
    /// escaped. The store keeps canonical JSON alone, so no spelling of the two
    /// texts written here is another.
    #[tokio::test]
    async fn a_setting_no_writer_here_wrote_is_refused_and_never_read_as_on_or_off() {
        for value in [
            "true",
            r#"{"on":1}"#,
            r#"{"on":true,"x":1}"#,
            r#"{"on":false,"x":1}"#,
            r#"{"on":null}"#,
            r#""on""#,
            "{}",
        ] {
            let (store, realm) = holding(value).await;
            let refused = read(&store, &realm)
                .await
                .expect_err("an entry nothing here writes");
            assert_eq!(
                (refused.kind(), refused.code()),
                (
                    RefusalKind::Internal,
                    nc_local::LocalError::Unexpected(String::new()).code()
                ),
                "{value}"
            );
            assert!(
                refused
                    .sentence()
                    .contains(&crate::registry::quoted_up_to(value, 64)),
                "{value}: {}",
                refused.sentence()
            );
        }
    }

    /// A set never guesses which way to switch.
    ///
    /// The schema refuses a set without a boolean `on` before the handler
    /// runs; the handler refuses it again rather than read a default.
    #[test]
    fn a_set_with_no_boolean_on_is_refused_rather_than_read_as_off() {
        assert_eq!(asked(&serde_json::json!({"on": true})).ok(), Some(true));
        assert_eq!(asked(&serde_json::json!({"on": false})).ok(), Some(false));
        for args in [
            serde_json::json!({}),
            serde_json::json!({"on": "false"}),
            serde_json::json!({"on": 0}),
        ] {
            let refused = asked(&args).expect_err("no boolean `on`");
            assert_eq!(
                (refused.refusal().kind(), refused.word()),
                (RefusalKind::Invalid, "arguments-invalid"),
                "{args}"
            );
        }
    }

    /// Only the set of MCP writes nothing but its entry.
    ///
    /// Named by the command, never by the namespace: the read writes nothing
    /// at all, and a command of another namespace is not a setting's.
    #[test]
    fn only_mcps_set_writes_only_its_entry() {
        assert!(writes_only_its_entry(MCP_SET));
        for name in [MCP_READ, "settings.other.set", "limits.prune"] {
            assert!(!writes_only_its_entry(name), "`{name}`");
        }
        for which in Which::ALL {
            assert!(!writes_only_its_entry(which.set()), "{which:?}");
            assert!(!writes_only_its_entry(which.read()), "{which:?}");
        }
    }

    /// The declared shortcut settings are the ones this module implements.
    ///
    /// **Both ways**, against `Command::arguments`: each read takes nothing
    /// and each set its one field, the pair decided as `Read` and `Write` on
    /// the platform's `interface` feature and listed among the settings, the
    /// set a change of state and the read none; and each has its handler, the
    /// four settings being every command of theirs the catalog declares.
    #[test]
    fn the_declared_shortcut_settings_are_the_ones_this_module_implements() {
        for which in Which::ALL {
            for (name, reads, action, mutating) in [
                (which.read(), None, "Read", false),
                (which.set(), Some(which.field()), "Write", true),
            ] {
                let declared =
                    nc_catalog::command(name).unwrap_or_else(|| panic!("`{name}` is declared"));
                assert_eq!(
                    declared.arguments,
                    reads.into_iter().collect::<Vec<_>>(),
                    "`{name}`"
                );
                assert_eq!(
                    (
                        declared.action,
                        declared.resource_selector,
                        declared.mutating,
                        declared.categories
                    ),
                    (
                        action,
                        "feature:interface",
                        mutating,
                        &["noctua.settings"][..]
                    ),
                    "`{name}`"
                );
                assert_eq!(
                    matches!(declared.reversibility, nc_catalog::Reversibility::Read),
                    !mutating,
                    "`{name}`"
                );
                assert!(
                    !matches!(
                        declared.reversibility,
                        nc_catalog::Reversibility::Irreversible
                    ),
                    "`{name}` would wait for a confirmation"
                );
            }
            assert_eq!(
                Builtin::named(which.read()),
                Some(Builtin::ShortcutRead(which))
            );
            assert_eq!(
                Builtin::named(which.set()),
                Some(Builtin::ShortcutSet(which))
            );
        }
        let ours: BTreeSet<&str> = Which::ALL
            .into_iter()
            .flat_map(|which| [which.read(), which.set()])
            .collect();
        let declared: BTreeSet<&str> = nc_catalog::COMMANDS
            .iter()
            .map(|command| command.name)
            .filter(|name| name.starts_with("settings.") && ![MCP_READ, MCP_SET].contains(name))
            .collect();
        assert_eq!(ours, declared);
        assert_eq!(
            Which::ALL.map(|which| (which.read(), which.set(), which.field())),
            [
                ("settings.leader.read", "settings.leader.set", "leader"),
                ("settings.chord.read", "settings.chord.set", "chord"),
                ("settings.menu-keys.read", "settings.menu-keys.set", "keys"),
                ("settings.chords.read", "settings.chords.set", "chords"),
            ]
        );
    }

    /// The schema a command declares, as JSON.
    fn schema(name: &str) -> serde_json::Value {
        serde_json::from_str(nc_catalog::command(name).expect("declared").args_schema)
            .expect("a schema")
    }

    /// Every bound the shortcut settings' schemas declare is held to its
    /// source.
    ///
    /// A table holds each key of the vocabulary once, so the menu's keys are
    /// at most as many as the vocabulary and the chords, over two tables,
    /// twice that; a command's name is at most `NAME_MAX` characters; and a
    /// key's and a modifier's name fit the bound their fields declare, which
    /// no longer one would.
    #[test]
    fn every_bound_the_shortcut_schemas_declare_is_held_to_its_source() {
        let keys = Key::ALL.len() as u64;
        let menu = &schema(Which::MenuKeys.set())["properties"]["keys"];
        let chords = &schema(Which::Chords.set())["properties"]["chords"];
        assert_eq!(menu["maxItems"].as_u64(), Some(keys));
        assert_eq!(chords["maxItems"].as_u64(), Some(keys * 2));
        let longest = |names: &mut dyn Iterator<Item = &str>| {
            names.map(str::len).max().expect("a name") as u64
        };
        for entry in [menu, chords] {
            let properties = &entry["items"]["properties"];
            assert_eq!(
                properties[COMMAND]["maxLength"].as_u64(),
                Some(nc_schema::NAME_MAX as u64)
            );
            let bound = properties[KEY]["maxLength"].as_u64().expect("a bound");
            assert!(longest(&mut Key::ALL.iter().map(|key| key.code())) <= bound);
        }
        let word = schema(Which::Chord.set())["properties"][CHORD]["maxLength"]
            .as_u64()
            .expect("a bound");
        let modifiers = [
            Modifier::Control,
            Modifier::Alt,
            Modifier::Shift,
            Modifier::Super,
        ];
        assert!(longest(&mut modifiers.iter().map(|modifier| modifier.word())) <= word);
        // The longest chord the grammar writes: every modifier and the
        // longest key, joined.
        let leader = schema(Which::Leader.set())["properties"]["leader"]["maxLength"]
            .as_u64()
            .expect("a bound");
        let written = modifiers
            .iter()
            .map(|modifier| modifier.word().len() as u64 + 1)
            .sum::<u64>()
            + longest(&mut Key::ALL.iter().map(|key| key.code()));
        assert!(written <= leader, "{written} > {leader}");
    }

    /// The keys a test's settings answer of: a Leader, a chord modifier and
    /// one binding in each table.
    fn some_keys() -> Keys {
        let bound = |texts: &[&str]| -> Vec<nc_schema::keys::Binding> {
            texts
                .iter()
                .map(|text| nc_schema::keys::Binding::parse(text).expect("a binding"))
                .collect()
        };
        Keys {
            leader: Leader::of(Chord::parse("Control+Alt+KeyK").expect("a chord"))
                .expect("a Leader"),
            chord: Modifier::Super,
            tables: nc_schema::keys::tables(
                &[
                    ("a.one", bound(&["Leader KeyB", "Leader+Chord+KeyQ"])),
                    ("a.two", bound(&["Leader KeyA", "Leader+ArrowUp"])),
                ],
                &[],
            )
            .expect("tables"),
        }
    }

    /// A shortcut setting is answered as the one object its set takes whole,
    /// and what the set reads of that object is the setting answered.
    ///
    /// **Each of the four, written out**: the Leader and the chord modifier as
    /// the grammar writes them, the menu's keys as every binding of the
    /// menu's table and no other, and the chords as every binding of the two
    /// tables a held Leader has, each saying which — key by key, as the
    /// vocabulary orders them.
    #[test]
    fn a_shortcut_setting_is_answered_as_the_object_its_set_takes_whole() {
        let keys = some_keys();
        let answers = [
            (Which::Leader, r#"{"leader":"Control+Alt+KeyK"}"#),
            (Which::Chord, r#"{"chord":"Super"}"#),
            (
                Which::MenuKeys,
                r#"{"keys":[{"command":"a.two","key":"KeyA"},{"command":"a.one","key":"KeyB"}]}"#,
            ),
            (
                Which::Chords,
                r#"{"chords":[{"chord":false,"command":"a.two","key":"ArrowUp"},{"chord":true,"command":"a.one","key":"KeyQ"}]}"#,
            ),
        ];
        let shortcuts = [
            Shortcut::Leader(keys.leader.chord()),
            Shortcut::Chord(Modifier::Super),
            Shortcut::Menu(vec![
                ("a.two".to_owned(), Key::KeyA),
                ("a.one".to_owned(), Key::KeyB),
            ]),
            Shortcut::Held(vec![
                ("a.two".to_owned(), Key::ArrowUp, false),
                ("a.one".to_owned(), Key::KeyQ, true),
            ]),
        ];
        for ((which, answer), shortcut) in answers.into_iter().zip(shortcuts) {
            assert_eq!(answered(which, &keys), answer, "{which:?}");
            let args: serde_json::Value = serde_json::from_str(answer).expect("JSON");
            let schema = nc_schema::Schema::compile(
                nc_catalog::command(which.set())
                    .expect("declared")
                    .args_schema,
            )
            .expect("a schema");
            assert!(
                schema.validate(&args).is_ok(),
                "{which:?}: the set refuses what the read answers"
            );
            assert_eq!(
                asked_shortcut(which, &args).expect("read"),
                shortcut,
                "{which:?}"
            );
        }
    }

    /// A shortcut the grammar does not write is refused with
    /// `[NC-CHAN-0025]`, quoting what it read escaped and cut.
    ///
    /// **Every key kept out of a shortcut names no key**: a modifier alone,
    /// a lock key, the numpad, a media key and Esc, and a key spelled any way
    /// but its own; so with a Leader that is no chord, a chord modifier that
    /// is none of the four, and a command no command can be named.
    #[test]
    fn a_shortcut_the_grammar_does_not_write_is_refused_as_reserved() {
        let reserved = |which: Which, args: serde_json::Value, quoted: &str| {
            let failure = asked_shortcut(which, &args).expect_err("refused");
            assert_eq!(
                (
                    failure.refusal().kind(),
                    failure.refusal().code(),
                    failure.word()
                ),
                (
                    RefusalKind::Invalid,
                    ErrorCode::ChannelShortcutKeyReserved,
                    "shortcut-reserved"
                ),
                "{args}"
            );
            let sentence = failure.refusal().sentence();
            assert!(sentence.contains(quoted), "{args}: {sentence}");
            assert!(sentence.starts_with(&format!("`{}` was asked for ", which.set())));
            assert!(
                sentence.ends_with(", so nothing was written and the keys in force stand"),
                "{sentence}"
            );
        };
        for key in [
            "Escape",
            "ShiftLeft",
            "ControlRight",
            "CapsLock",
            "NumLock",
            "Numpad1",
            "AudioVolumeUp",
            "keys",
            "S",
            "Key",
        ] {
            let entry = serde_json::json!({"command": "a.one", "key": key, "chord": false});
            reserved(
                Which::MenuKeys,
                serde_json::json!({ "keys": [entry] }),
                &format!("the key `{key}`"),
            );
            reserved(
                Which::Chords,
                serde_json::json!({ "chords": [entry] }),
                &format!("the key `{key}`"),
            );
        }
        for leader in [
            "Hyper+Space",
            "space",
            "Alt+Escape",
            "Alt + Space",
            "Space+Alt",
        ] {
            reserved(
                Which::Leader,
                serde_json::json!({ "leader": leader }),
                "a Leader that is no chord: ",
            );
        }
        for chord in ["Meta", "shift", "Shift+Alt"] {
            reserved(
                Which::Chord,
                serde_json::json!({ "chord": chord }),
                &format!("the chord modifier `{chord}`"),
            );
        }
        for command in ["A.One", "a one", "", "a..b"] {
            reserved(
                Which::MenuKeys,
                serde_json::json!({"keys": [{"command": command, "key": "KeyA"}]}),
                "which no command can be named",
            );
        }
        let failure = asked_shortcut(
            Which::MenuKeys,
            &serde_json::json!({"keys": [{"command": "a.one", "key": format!("\u{202e}{}", "k".repeat(200))}]}),
        )
        .expect_err("refused");
        let sentence = failure.refusal().sentence();
        assert!(
            !sentence.contains('\u{202e}') && sentence.contains("\\u{202e}"),
            "{sentence}"
        );
        assert!(!sentence.contains(&"k".repeat(QUOTED + 1)), "{sentence}");
    }

    /// Arguments of another shape than a shortcut setting's object are
    /// refused with the arguments' own code, and the menu's keys read no
    /// `chord`.
    #[test]
    fn a_shortcut_of_another_shape_is_refused_as_the_arguments_are() {
        for (which, args) in [
            (Which::Leader, serde_json::json!({})),
            (Which::Leader, serde_json::json!({"leader": 1})),
            (Which::Chord, serde_json::json!({"chord": true})),
            (Which::MenuKeys, serde_json::json!({"keys": {}})),
            (
                Which::MenuKeys,
                serde_json::json!({"keys": [{"key": "KeyA"}]}),
            ),
            (
                Which::MenuKeys,
                serde_json::json!({"keys": [{"command": "a.one"}]}),
            ),
            (
                Which::Chords,
                serde_json::json!({"chords": [{"command": "a.one", "key": "KeyA"}]}),
            ),
        ] {
            let failure = asked_shortcut(which, &args).expect_err("refused");
            assert_eq!(
                (failure.refusal().kind(), failure.refusal().code()),
                (RefusalKind::Invalid, ErrorCode::ValidationArgumentsInvalid),
                "{args}"
            );
        }
        // The menu's keys read no `chord`: a key there is held with nothing.
        assert_eq!(
            asked_shortcut(
                Which::MenuKeys,
                &serde_json::json!({"keys": [{"command": "a.one", "key": "KeyA"}]})
            )
            .expect("read"),
            Shortcut::Menu(vec![("a.one".to_owned(), Key::KeyA)])
        );
    }
}
