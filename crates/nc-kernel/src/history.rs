//! Undo and redo: a person's own history, stepped back and forward through
//! the decision point (`NOCTUA.md §11.4`, `§11.5`).
//!
//! **Two built-in commands, and no history interface.** `kernel.undo` and
//! `kernel.redo` are dispatched like any other command
//! (`catalog/commands.toml`): decided by Cedar as `Write` on the realm,
//! journaled, and run inside a unit of their own the decision point opened —
//! at the top of a call tree only, since nested in another command either is
//! refused (`[NC-KERN-0017]`). Neither takes an argument: which step is the
//! kernel's to find, and no identifier crosses the boundary.
//!
//! **What undo takes back is what the kernel keeps, and no more.** A step is
//! one top-level unit of work, kept in the local store with the before and
//! after image of every key it wrote and a note of everything else it did
//! (`crate::units`). Undo finds the newest step of the person asking still
//! committed, among their newest `undo_history` steps of this realm — steps
//! alone, so the undos and redos between them push none out of reach — and:
//!
//! 1. refuses a step whose effects left the machine — a request that asked its
//!    host to change something, a command that cannot be undone, a command the
//!    kernel runs on the realm itself — when no compensation was registered in
//!    it: it is a barrier, and undo does not reach past it (`[NC-KERN-0012]`);
//! 2. asks the journal how the step's unit closed, and refuses one the journal
//!    does not record as kept (`[NC-KERN-0022]`): the store keeps the images,
//!    and the journal is what says the step happened, since reversal images
//!    never enter the hash chain;
//! 3. authorizes again, now, every action the step was allowed on every
//!    resource — its own command's, and every command's dispatched inside it —
//!    as the person asking and with what they hold now, and refuses at the
//!    first denial, journaled with what decided it;
//! 4. holds every key to the value the step left there, and refuses when one
//!    changed since (`[NC-KERN-0011]`) — before anything is staged or sent;
//! 5. stages every key back to its before image, and takes the pin off every
//!    blob the step made that no other committed unit holds now — one that
//!    noted making it, or the one that first made its row — so the sweep
//!    deletes its bytes once `blob_orphan_grace_ms` has passed unless a redo
//!    pins it again;
//! 6. dispatches the compensations registered in the step, the last one first,
//!    nested in the undo's own unit, as the person asking, through the
//!    decision point and never confirmed, refusing the undo when one fails or
//!    answers having let the undo's unit go, before the next one runs
//!    (`[NC-KERN-0014]`) — and dispatches none of them, refusing the undo
//!    (`[NC-KERN-0023]`), when the command of one is held now by another
//!    plugin than the one that held it when it was registered, or by none,
//!    since a namespace an unloaded plugin left is any identity's to take —
//!    and none either (`[NC-KERN-0024]`) when one names another plugin's
//!    command and the plugin that registered it is no longer loaded granted
//!    `commands.dispatch`, which registering it asked;
//! 7. and is kept as one unit, which marks the step undone in the same
//!    transaction.
//!
//! **Redo puts back the newest undo, until there is new work.** Redo finds the
//! person's newest undo still kept whose step it undid, among their newest
//! `undo_history` units of every kind, unless a step of theirs was committed
//! after it — new work ends the way back — and refuses it
//! when the undo dispatched compensations (`[NC-KERN-0013]`), whose effects
//! cannot be undone in turn. It asks the journal about both units, authorizes
//! the step's actions again now, holds every key to what the undo left, pins
//! again every blob the step made that it finds unpinned — whoever's undo took
//! the pin off, and refusing when one was swept meanwhile (`[NC-KERN-0019]`) —
//! and is kept as one unit marking the step committed and the undo undone. A reversal the kernel made itself, because the journal did
//! not take a unit's close, is nobody's undo, and redo never puts one back.
//!
//! **Compensations are registered, judged and kept by the unit**: a plugin
//! names one with `compensate-with` inside the command whose unit it is — one
//! of its own, unless it is granted `commands.dispatch` as well — and the
//! command must be registered, change state, be none the core holds to a
//! person's confirmation or a quorum's approval on every resource — one that
//! cannot be undone, one that deletes — and not be one of the kernel's own
//! (`[NC-KERN-0021]`), and its arguments are held to its schema then
//! (`compensation`), its note naming the identity of the plugin that holds the
//! command. What is refused there is what no undo could ever run, a
//! compensation being dispatched nested, never confirmed and never approved;
//! the rest is decided when the undo dispatches it, as the person asking and
//! with what they hold then — **a pack's rule that asks a quorum of some
//! resources among it**: the family pack's
//! `family.giving_out_encrypted_content_needs_quorum` asks one of a `Share` or
//! a `Grant` of private content, so a compensation that shares the diary is
//! registered and refused at every undo while the diary stays private
//! (`[NC-KERN-0014]`), its step one no undo reaches past. The registration does
//! not refuse every compensation needing a quorum's approval: only one the
//! core's rules hold to it on every resource.
//!
//! What this answers (`NOCTUA.md §2.4`): undo used as escalation — only a
//! person's own steps, only with the authority they hold now, held to what
//! each key holds, and compensations dispatched through the decision point
//! like any command; a step that left the machine reverted as though it had
//! not, which a barrier answers; a history the audit log never saw, since
//! every target is verified against the journal; and a blob another unit still
//! holds losing its bytes. What it does not answer: what a step set going — a
//! timer it armed stays armed and an event it published stays delivered or
//! queued, and its undo's `beyond_reach` counts only what left the machine —
//! since undo reverses what a step kept; a compensation that ran
//! before one that failed, whose effect outside the machine stays; a
//! compensation that does not in fact set right what its step did, which is
//! its plugin's to get right; a unit forgotten by the history's bound, which no
//! longer counts as holding the blobs it made — nothing forgets a person's
//! units in this build, and a unit the pruner took is read as holding what it
//! made for good; a compensation a pack's rule refuses on the resource it
//! names, which bars its step's undo while the rule holds; an undo whose
//! compensations write more than the bytes its
//! step left room for in one unit — the step was held to its own images, which
//! the undo stages back first — or note more than the notes a step keeps free
//! for its undo (`crate::units`), and is refused as their failure; how far back
//! a redo reaches, which is the person's newest
//! `undo_history` units of every kind, so a run of more than half that many
//! redos leaves the undos older than it out of reach, their steps staying
//! undone; and an undo or a redo whose close the journal does not take, which
//! is reverted as any unit is, the requests its compensations sent staying
//! sent.

use std::sync::{Arc, Mutex};

use nc_authz::{EntityRef, Principal};
use nc_canon::Value;
use nc_catalog::{ErrorCode, KERNEL};
use nc_ids::Snowflake;
use nc_journal::{Outcome, Store as _};
use nc_local::{BlobCreated, Image, LocalError, UnitDetail, UnitKind, UnitRow, UnitState};
use nc_schema::descriptor::Reversibility;

use crate::backend::StoreBackend as _;
use crate::blobs::{self, BlobStore as _, Hash};
use crate::dispatch::{self, CallTree, Failure, Refusal, RefusalKind};
use crate::units::{self, Reversing};
use crate::{Environment, Shared};

/// The command that undoes the newest step of the person who asks.
pub(crate) const UNDO: &str = "kernel.undo";

/// The command that redoes the newest step the person who asks undid.
pub(crate) const REDO: &str = "kernel.redo";

/// Whether `name` is undo or redo, which run only at the top of a call tree.
pub(crate) fn is_history(name: &str) -> bool {
    name == UNDO || name == REDO
}

/// What the person asking was issued for this one dispatch, beside their
/// grant: what an action authorized again is decided with.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Held {
    /// A human confirmed this dispatch.
    pub(crate) confirmed: bool,
    /// A quorum approved this dispatch.
    pub(crate) quorum_met: bool,
}

/// One note a unit keeps, as the kernel writes it and reads it back.
///
/// **Canonical JSON, and the kind is its `note` field.** The store keeps notes
/// as text it never reads (`nc_local`); this is the one reader, and a note of
/// a kind it does not know, or of a known kind in another shape, is a row the
/// kernel did not write, refused as `NC-DB-0007`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Note {
    /// A command of the unit was allowed `action` on `entity`.
    Authorized {
        /// The Cedar action.
        action: String,
        /// The resource it was allowed on.
        entity: EntityRef,
    },
    /// A compensation registered in the unit: `command`, with `args`, of the
    /// plugin `owner`, registered by the plugin `by`.
    Compensation {
        /// The command dispatched if the step is undone.
        command: String,
        /// Its arguments, which its schema held when it was registered.
        args: Value,
        /// The identity of the plugin that held `command` when it was
        /// registered: an undo dispatches it only while that plugin holds it.
        owner: String,
        /// The identity of the plugin that registered it: an undo dispatches
        /// another plugin's command for it only while it is loaded granted
        /// `commands.dispatch`, as it had to be to register one.
        by: String,
    },
    /// Something the unit did left the machine.
    ///
    /// A request that asks its host to change something, or a command that
    /// cannot be undone or that the kernel runs on the realm itself.
    LeftTheMachine,
    /// A blob the unit made, and whether a committed unit pinned it before.
    BlobCreated {
        /// Its hash.
        hash: String,
        /// Whether a committed unit held it before this one made it.
        pinned_before: bool,
    },
    /// An undo took the pin off a blob of `size` bytes.
    BlobUnpinned {
        /// Its hash.
        hash: String,
        /// Its size, which pinning it again records.
        size: u64,
    },
    /// A redo pinned a blob again.
    BlobRepinned {
        /// Its hash.
        hash: String,
    },
    /// An undo dispatched this many compensations.
    CompensationsRan(u32),
}

/// The `note` word of each kind the kernel writes here.
const AUTHORIZED: &str = "authorized";
/// See [`AUTHORIZED`].
const COMPENSATION: &str = "compensation";
/// See [`AUTHORIZED`].
const BLOB_UNPINNED: &str = "blob_unpinned";
/// See [`AUTHORIZED`].
const BLOB_REPINNED: &str = "blob_repinned";
/// See [`AUTHORIZED`].
const COMPENSATIONS_RAN: &str = "compensations_ran";

impl Note {
    /// The note as its unit's record keeps it.
    ///
    /// A blob made and a request sent are written where they happen
    /// (`crate::blobs::note`, `crate::net::note`), and here they are only
    /// read.
    pub(crate) fn written(&self) -> String {
        let kind = |word: &str| ("note", Value::text(word));
        let counted = |n: u64| Value::Integer(i64::try_from(n).unwrap_or(i64::MAX));
        match self {
            Self::Authorized { action, entity } => Value::object([
                ("action", Value::text(action.clone())),
                (
                    "entity",
                    Value::object([
                        ("id", Value::text(entity.id.clone())),
                        ("type", Value::text(entity.type_name.clone())),
                    ]),
                ),
                kind(AUTHORIZED),
            ]),
            Self::Compensation {
                command,
                args,
                owner,
                by,
            } => Value::object([
                ("args", args.clone()),
                ("by", Value::text(by.clone())),
                ("command", Value::text(command.clone())),
                kind(COMPENSATION),
                ("owner", Value::text(owner.clone())),
            ]),
            Self::LeftTheMachine => Value::object([kind(crate::net::LEFT_THE_MACHINE)]),
            Self::BlobCreated {
                hash,
                pinned_before,
            } => return blobs::noted(hash, *pinned_before),
            Self::BlobUnpinned { hash, size } => Value::object([
                ("hash", Value::text(hash.clone())),
                kind(BLOB_UNPINNED),
                ("size", counted(*size)),
            ]),
            Self::BlobRepinned { hash } => {
                Value::object([("hash", Value::text(hash.clone())), kind(BLOB_REPINNED)])
            }
            Self::CompensationsRan(count) => Value::object([
                ("compensations", counted(u64::from(*count))),
                kind(COMPENSATIONS_RAN),
            ]),
        }
        .to_canonical()
    }

    /// The note `written` is.
    ///
    /// # Errors
    ///
    /// [`LocalError::Unexpected`] for text that is not a note the kernel
    /// writes: not canonical JSON, of a kind it does not know, or of a known
    /// kind in another shape.
    pub(crate) fn read(written: &str) -> Result<Self, LocalError> {
        let unexpected = || {
            LocalError::Unexpected(format!(
                "a unit's note is not one the kernel writes: {}",
                crate::registry::quoted_up_to(written, NOTE_QUOTED)
            ))
        };
        let mut fields = nc_canon::parse(written)
            .ok()
            .and_then(Value::into_object)
            .ok_or_else(unexpected)?;
        let kind = fields
            .remove("note")
            .and_then(Value::into_text)
            .ok_or_else(unexpected)?;
        let text = |fields: &mut std::collections::BTreeMap<String, Value>, name: &str| {
            fields.remove(name).and_then(Value::into_text)
        };
        let whole =
            |fields: &mut std::collections::BTreeMap<String, Value>, name: &str| match fields
                .remove(name)
            {
                Some(Value::Integer(n)) => u64::try_from(n).ok(),
                _ => None,
            };
        let note = match kind.as_str() {
            AUTHORIZED => {
                let action = text(&mut fields, "action");
                let entity = fields
                    .remove("entity")
                    .and_then(Value::into_object)
                    .and_then(|mut entity| {
                        let id = text(&mut entity, "id")?;
                        let type_name = text(&mut entity, "type")?;
                        entity.is_empty().then_some(EntityRef { type_name, id })
                    });
                action
                    .zip(entity)
                    .map(|(action, entity)| Self::Authorized { action, entity })
            }
            COMPENSATION => text(&mut fields, "command")
                .zip(fields.remove("args"))
                .zip(text(&mut fields, "owner"))
                .zip(text(&mut fields, "by"))
                .map(|(((command, args), owner), by)| Self::Compensation {
                    command,
                    args,
                    owner,
                    by,
                }),
            crate::net::LEFT_THE_MACHINE => {
                // The request's host and method, or the command's name: what
                // a person is shown, and nothing undo decides by.
                fields.clear();
                Some(Self::LeftTheMachine)
            }
            BLOB_UNPINNED => text(&mut fields, "hash")
                .filter(|hash| Hash::parse(hash).is_some())
                .zip(whole(&mut fields, "size"))
                .map(|(hash, size)| Self::BlobUnpinned { hash, size }),
            BLOB_REPINNED => text(&mut fields, "hash")
                .filter(|hash| Hash::parse(hash).is_some())
                .map(|hash| Self::BlobRepinned { hash }),
            COMPENSATIONS_RAN => whole(&mut fields, "compensations")
                .and_then(|count| u32::try_from(count).ok())
                .map(Self::CompensationsRan),
            _ => blobs::created(written)?.map(|(hash, pinned_before)| {
                fields.clear();
                Self::BlobCreated {
                    hash,
                    pinned_before,
                }
            }),
        };
        match note {
            Some(note) if fields.is_empty() => Ok(note),
            _ => Err(unexpected()),
        }
    }
}

/// How many characters of a note a refusal repeats.
const NOTE_QUOTED: usize = 128;

/// The note a command that cannot be undone, or one the kernel runs on the
/// realm itself, leaves in its unit before it runs: `{"command":…,"note":…}`.
pub(crate) fn left_by(command: &str) -> String {
    Value::object([
        ("command", Value::text(command)),
        ("note", Value::text(crate::net::LEFT_THE_MACHINE)),
    ])
    .to_canonical()
}

/// Whether running `command` leaves the machine.
///
/// It does as a request that changes something elsewhere does when the
/// command cannot be undone, and when the kernel owns it and it changes
/// state — `limits.prune` — but for undo and redo, which are the history
/// itself.
pub(crate) fn leaves_the_machine(command: &crate::registry::RegisteredCommand) -> bool {
    command.reversibility == Reversibility::Irreversible
        || (command.owner == crate::registry::Owner::Kernel
            && command.mutating
            && !is_history(&command.name))
}

/// Everything a unit's notes say, read whole.
#[derive(Debug, Default)]
struct Read {
    /// Each action allowed, on each resource.
    authorized: Vec<(String, EntityRef)>,
    /// Each compensation, as its command, its arguments' canonical text, the
    /// plugin that held the command and the plugin that registered it, in the
    /// order registered.
    compensations: Vec<(String, String, String, String)>,
    /// How many effects left the machine.
    left: u32,
    /// Each blob made, and whether a committed unit held it before.
    made: Vec<(String, bool)>,
    /// Whether an undo dispatched compensations.
    compensated: bool,
}

impl Read {
    /// What `notes` say.
    ///
    /// # Errors
    ///
    /// [`LocalError::Unexpected`] for a note the kernel did not write.
    fn of(notes: &[String]) -> Result<Self, LocalError> {
        let mut read = Self::default();
        for note in notes {
            match Note::read(note)? {
                Note::Authorized { action, entity } => read.authorized.push((action, entity)),
                Note::Compensation {
                    command,
                    args,
                    owner,
                    by,
                } => read
                    .compensations
                    .push((command, args.to_canonical(), owner, by)),
                Note::LeftTheMachine => read.left = read.left.saturating_add(1),
                Note::BlobCreated {
                    hash,
                    pinned_before,
                } => read.made.push((hash, pinned_before)),
                // The pins a reversal moved, which only the kernel's own
                // reversal of it reads back (`pins_moved`): a redo pins again
                // what the step made, whatever its undo took off.
                Note::BlobUnpinned { .. } | Note::BlobRepinned { .. } => {}
                Note::CompensationsRan(count) => read.compensated |= count > 0,
            }
        }
        Ok(read)
    }
}

/// The pins a committed unit moved, as its notes say.
///
/// The blobs whose pin an undo took off, which its reversal pins again, and
/// the blobs a redo pinned again, whose pin its reversal takes off.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Moved {
    /// The blobs to pin again.
    pub(crate) unpinned: Vec<BlobCreated>,
    /// The blobs to take the pin off.
    pub(crate) repinned: Vec<String>,
}

/// The pins the unit whose notes are `notes` moved.
///
/// # Errors
///
/// [`LocalError::Unexpected`] for a note the kernel did not write.
pub(crate) fn pins_moved(notes: &[String]) -> Result<Moved, LocalError> {
    let mut moved = Moved::default();
    for note in notes {
        match Note::read(note)? {
            Note::BlobUnpinned { hash, size } => moved.unpinned.push(BlobCreated { hash, size }),
            Note::BlobRepinned { hash } => moved.repinned.push(hash),
            _ => {}
        }
    }
    Ok(moved)
}

/// The action the core asks a quorum's approval of, on any resource
/// (`core.destructive_needs_quorum`, `policy/core/quorum.cedar`).
///
/// **The core's rule, read here as what a compensation can never meet**: a
/// compensation is dispatched nested, and a nested dispatch carries no
/// approval, so one that deletes is denied every time the undo needing it is
/// asked. `a_compensation_that_could_never_run_is_refused_when_it_is_registered`
/// dispatches a nested deletion and holds its denial to that rule, so a core
/// that stops asking it fails there.
const DELETE: &str = "Delete";

/// The note of the compensation `name` with `args`, as a plugin registers
/// it, once it is shown to be one an undo can dispatch.
///
/// # Errors
///
/// `invalid` with `[NC-KERN-0002]` for a name no command can have; `not-found`
/// with `[NC-KERN-0001]` for a command nothing registered; `invalid` with
/// `[NC-KERN-0021]` for undo or redo and every other command of the kernel's
/// own, a command that changes nothing, one that cannot be undone and one that
/// deletes, which a compensation — nested, never confirmed and never approved —
/// could never run by the core's rules, on any resource, or would run on the
/// realm itself (a pack's rule that reads the resource is the undo's to meet);
/// and the decision point's
/// refusal of the arguments, as a dispatch of them would meet it: past the
/// payload's bound, not canonical, or not what the command's schema or
/// selector reads.
pub(crate) fn compensation<E: Environment>(
    shared: &Shared<E>,
    by: &str,
    name: &str,
    args: &str,
) -> Result<String, Refusal> {
    let cannot = |why: String| {
        Refusal::new(
            RefusalKind::Invalid,
            ErrorCode::KernelCompensationRefused,
            why,
        )
    };
    if !nc_schema::descriptor::command_name_is_well_formed(name) {
        return Err(Refusal::new(
            RefusalKind::Invalid,
            ErrorCode::KernelMalformedName,
            "the compensation names something that is not a command's name".to_owned(),
        ));
    }
    if is_history(name) {
        return Err(cannot(format!(
            "`{name}` cannot compensate a step: undo and redo run only at the top of a call tree"
        )));
    }
    let Some(command) = shared.command(name) else {
        return Err(Refusal::new(
            RefusalKind::NotFound,
            ErrorCode::KernelUnknownCommand,
            format!("no command `{name}` is registered to compensate with"),
        ));
    };
    if command.listed().owner == crate::registry::Owner::Kernel {
        return Err(cannot(format!(
            "`{name}` is one of the kernel's own, which work on the realm itself and not on \
             what a plugin's step did"
        )));
    }
    if !command.listed().mutating {
        return Err(cannot(format!(
            "`{name}` changes nothing, so it cannot set a step right"
        )));
    }
    if command.listed().reversibility == Reversibility::Irreversible {
        return Err(cannot(format!(
            "`{name}` cannot be undone and needs a person's confirmation, which a compensation \
             never has"
        )));
    }
    if command.listed().action == DELETE {
        return Err(cannot(format!(
            "`{name}` deletes, and the core asks a quorum's approval of every deletion, which a \
             compensation never has"
        )));
    }
    let Some(owner) = owner_of(shared, &command) else {
        return Err(Refusal::new(
            RefusalKind::NotFound,
            ErrorCode::KernelUnknownCommand,
            format!("no plugin holds `{name}` to compensate with"),
        ));
    };
    let (canonical, _, _) = dispatch::understood(shared, &command, name, args)?;
    let args = nc_canon::parse(&canonical).map_err(|refused| {
        Refusal::new(
            RefusalKind::Internal,
            refused.code(),
            format!("the canonical text of `{name}`'s arguments does not read back: {refused}"),
        )
    })?;
    Ok(Note::Compensation {
        command: name.to_owned(),
        args,
        owner,
        by: by.to_owned(),
    }
    .written())
}

/// The identity of the plugin that holds `command`: none for the kernel's.
pub(crate) fn owner_of<E: Environment>(
    shared: &Shared<E>,
    command: &crate::registry::Admitted,
) -> Option<String> {
    match command.listed().owner {
        crate::registry::Owner::Plugin(slot) => shared.plugin_of(slot),
        crate::registry::Owner::Kernel => None,
    }
}

/// Refuses the undo of a step one of whose `compensations` names a command
/// the plugin that held it when it was registered does not hold now, or
/// another plugin's command for a plugin no longer loaded granted
/// `commands.dispatch`.
///
/// **Asked of every compensation before any is dispatched**, so none of them
/// reaches outside the machine for an undo that cannot run them all.
///
/// **And of the plugin that registered it, as it is now.** Registering
/// another plugin's command asks `commands.dispatch`, because the undo
/// dispatches it as the person; a plugin loaded again without the grant would
/// otherwise have it dispatched all the same, and the undo would then also lift
/// the barrier the step's request would have been.
///
/// # Errors
///
/// `denied` with `[NC-KERN-0023]`, naming the first command held otherwise,
/// and with `[NC-KERN-0024]`, naming the first registered by a plugin no
/// longer granted to dispatch it.
fn held_as_registered<E: Environment>(
    shared: &Shared<E>,
    compensations: &[(String, String, String, String)],
) -> Result<(), Failure> {
    let ungranted = compensations.iter().find(|(_, _, owner, by)| {
        owner != by && !shared.grants(by, nc_catalog::Capability::CommandsDispatch)
    });
    if let Some((command, _, _, _)) = ungranted {
        return Err(refused(
            RefusalKind::Denied,
            ErrorCode::KernelCompensationUngranted,
            format!(
                "the compensation `{command}` the step registered is another plugin's command, \
                 and the plugin that registered it is no longer loaded granted \
                 `commands.dispatch`, so none was dispatched and nothing was undone"
            ),
            "compensation-ungranted",
        ));
    }
    let moved = compensations.iter().find(|(command, _, owner, _)| {
        shared
            .command(command)
            .and_then(|admitted| owner_of(shared, &admitted))
            .as_deref()
            != Some(owner.as_str())
    });
    match moved {
        Some((command, _, _, _)) => Err(refused(
            RefusalKind::Denied,
            ErrorCode::KernelCompensationMoved,
            format!(
                "the compensation `{command}` the step registered is no longer a command of \
                 the plugin that registered it, so none was dispatched and nothing was undone"
            ),
            "compensation-moved",
        )),
        None => Ok(()),
    }
}

/// A refusal of the history's, recorded in its entry under `word`.
fn refused(kind: RefusalKind, code: ErrorCode, sentence: String, word: &'static str) -> Failure {
    Failure::new(Refusal::new(kind, code, sentence), word)
}

/// The store's refusal, recorded under `store-refused`.
fn store_failed(failed: &LocalError) -> Failure {
    Failure::new(units::store_failed(failed), "store-refused")
}

/// The person's newest steps in this realm, as far back as an undo looks.
///
/// **Steps, and not every unit**: counted among their undos and redos, a step
/// redone and undone often enough would fall past the bound and be nothing to
/// undo, though it is the person's newest.
async fn steps_of<E: Environment>(
    shared: &Shared<E>,
    principal: &Principal,
) -> Result<Vec<UnitRow>, Failure> {
    shared
        .store
        .steps(&shared.realm, principal.id(), KERNEL.undo_history)
        .await
        .map_err(|failed| store_failed(&failed))
}

/// The person's newest units in this realm, steps, undos and redos alike, as
/// far back as a redo looks.
async fn units_of<E: Environment>(
    shared: &Shared<E>,
    principal: &Principal,
) -> Result<Vec<UnitRow>, Failure> {
    shared
        .store
        .units(&shared.realm, principal.id(), KERNEL.undo_history)
        .await
        .map_err(|failed| store_failed(&failed))
}

/// The committed unit `unit`, whole.
async fn detail_of<E: Environment>(
    shared: &Shared<E>,
    unit: Snowflake,
) -> Result<UnitDetail, Failure> {
    shared
        .store
        .unit(unit)
        .await
        .map_err(|failed| store_failed(&failed))?
        .ok_or_else(|| {
            store_failed(&LocalError::Unexpected(format!(
                "the store lists the unit {unit} and holds none"
            )))
        })
}

/// What `notes` say, or the store's refusal of a note the kernel did not
/// write.
fn read(notes: &[String]) -> Result<Read, Failure> {
    Read::of(notes).map_err(|failed| store_failed(&failed))
}

/// Refuses unless the journal records every one of `units` as kept.
///
/// # Errors
///
/// `unavailable` with the journal's own code when it cannot say, and
/// `internal` with `[NC-KERN-0022]` when it says a unit was not kept, or
/// says nothing of it.
async fn recorded<E: Environment>(shared: &Shared<E>, units: &[Snowflake]) -> Result<(), Failure> {
    let journal = shared.journal.lock().await;
    for unit in units {
        match journal.store().unit_closing(&shared.realm, *unit).await {
            Ok(Some(Outcome::UnitCommitted)) => {}
            Ok(_) => {
                return Err(refused(
                    RefusalKind::Internal,
                    ErrorCode::KernelStepUnrecorded,
                    format!(
                        "the journal does not record the unit {unit} as kept, so it is neither \
                         undone nor redone: the history reverses only what the audit log kept"
                    ),
                    "unrecorded",
                ));
            }
            Err(error) => {
                return Err(refused(
                    RefusalKind::Unavailable,
                    error.code(),
                    "the journal could not say how the step's unit closed, so nothing was \
                     undone or redone"
                        .to_owned(),
                    "journal-unreadable",
                ));
            }
        }
    }
    Ok(())
}

/// Authorizes again, now, every action in `authorized`: as `principal`,
/// holding `held`.
async fn authorized_now<E: Environment>(
    shared: &Arc<Shared<E>>,
    tree: &Arc<Mutex<CallTree>>,
    name: &str,
    principal: &Principal,
    authorized: &[(String, EntityRef)],
    held: Held,
) -> Result<(), Failure> {
    for (action, entity) in authorized {
        let authority = dispatch::Authority {
            command: name.to_owned(),
            action: action.clone(),
            resource: entity.clone(),
        };
        dispatch::reauthorized(
            shared,
            tree,
            principal,
            &authority,
            held,
            "what the step changed being authorized again now",
        )
        .await
        .map_err(|refusal| Failure::new(refusal, "denied-now"))?;
    }
    Ok(())
}

/// Refuses unless every key of `images` holds its after image, committed.
///
/// That is what the unit being reversed left there — a step, or the undo a
/// redo reverses.
async fn held_as_left<E: Environment>(shared: &Shared<E>, images: &[Image]) -> Result<(), Failure> {
    for image in images {
        let held = shared
            .store
            .get(&shared.realm, &image.plugin, &image.key)
            .await
            .map_err(|failed| store_failed(&failed))?;
        let expected = image.after.as_ref().map(|entry| entry.value.as_str());
        if held.as_ref().map(|entry| entry.value.as_str()) != expected {
            return Err(refused(
                RefusalKind::Conflict,
                ErrorCode::KernelUndoConflict,
                format!(
                    "the key {:?} of `{}` has been changed since, so nothing was undone or redone",
                    crate::registry::quoted_up_to(&image.key, KEY_QUOTED),
                    image.plugin
                ),
                "conflict",
            ));
        }
    }
    Ok(())
}

/// How many characters of a key a refusal repeats.
const KEY_QUOTED: usize = 64;

/// Stages each key of `images` back to its before image in the tree's unit,
/// held to its after image when the unit commits, and answers the unit and
/// how many keys it put back.
///
/// # Errors
///
/// `internal` with `[NC-KERN-0008]` when the tree holds no unit, which the
/// decision point opened for the command; and the unit's own bounds.
fn staged(tree: &Arc<Mutex<CallTree>>, images: &[Image]) -> Result<(Snowflake, u32), Failure> {
    let mut tree = dispatch::lock(tree);
    let unit = tree.unit_mut().ok_or_else(|| {
        refused(
            RefusalKind::Internal,
            ErrorCode::KernelUnitAborted,
            "the history ran outside the unit the decision point opened for it".to_owned(),
            "unit-lost",
        )
    })?;
    let id = unit.unit();
    for image in images {
        let (holding, after) = (
            value_of(image.after.as_ref()),
            value_of(image.before.as_ref()),
        );
        unit.overlay_mut()
            .restore(id, &image.plugin, &image.key, holding, after)
            .map_err(|refusal| Failure::new(refusal, "bound-reached"))?;
    }
    Ok((id, u32::try_from(images.len()).unwrap_or(u32::MAX)))
}

/// The value an image's entry holds.
fn value_of(entry: Option<&nc_local::Entry>) -> Option<String> {
    entry.map(|entry| entry.value.clone())
}

/// Makes the tree's unit the reversal `reversing` says, once it is whole.
fn reversed(tree: &Arc<Mutex<CallTree>>, reversing: Reversing) {
    if let Some(unit) = dispatch::lock(tree).unit_mut() {
        unit.reverse(reversing);
    }
}

/// What an undo or a redo answers, and what its journal entry adds to its
/// resource: the step, what was put back of it, and what was beyond reach.
pub(crate) fn answer_of(reversing: &Reversing) -> String {
    let mut fields = recorded_of(reversing);
    fields.insert(
        "beyond_reach".to_owned(),
        Value::Integer(i64::from(reversing.beyond_reach)),
    );
    Value::Object(fields).to_canonical()
}

/// What an undo's or a redo's journal entry adds to its resource.
pub(crate) fn recorded_of(reversing: &Reversing) -> std::collections::BTreeMap<String, Value> {
    let blobs = if reversing.undoes() {
        reversing.unpinned.len()
    } else {
        reversing.repinned.len()
    };
    std::collections::BTreeMap::from([
        (
            "target".to_owned(),
            Value::text(reversing.step.to_crockford()),
        ),
        (
            "reverted".to_owned(),
            Value::object([
                (
                    "blobs",
                    Value::Integer(i64::try_from(blobs).unwrap_or(i64::MAX)),
                ),
                (
                    "compensations",
                    Value::Integer(i64::from(reversing.compensations)),
                ),
                ("writes", Value::Integer(i64::from(reversing.writes))),
            ]),
        ),
    ])
}

/// `kernel.undo`: the newest step of `principal` still committed, reversed in
/// the unit of the tree it runs in.
///
/// # Errors
///
/// A [`Failure`] the decision point journals, as the module header lists the
/// refusals in order: nothing to undo, a barrier, a step the journal does not
/// record, an action denied now, a key changed since, a compensation that
/// failed — and what the store or the journal refused.
pub(crate) async fn undo<E: Environment>(
    shared: &Arc<Shared<E>>,
    tree: &Arc<Mutex<CallTree>>,
    principal: &Principal,
    held: Held,
) -> Result<String, Failure> {
    let rows = steps_of(shared, principal).await?;
    let Some(step) = rows
        .iter()
        .find(|row| row.meta.kind == UnitKind::Step && row.state == UnitState::Committed)
    else {
        return Err(refused(
            RefusalKind::NotFound,
            ErrorCode::KernelNothingToUndo,
            "no step of this person's is left to undo".to_owned(),
            "nothing-to-undo",
        ));
    };
    let detail = detail_of(shared, step.unit).await?;
    let notes = read(&detail.notes)?;
    if notes.left > 0 && notes.compensations.is_empty() {
        return Err(refused(
            RefusalKind::Denied,
            ErrorCode::KernelUndoBarrier,
            format!(
                "the newest step did {} thing(s) that left the machine or cannot be undone, \
                 with nothing registered to reverse them, and undo does not reach past it",
                notes.left
            ),
            "barrier",
        ));
    }
    recorded(shared, &[step.unit]).await?;
    authorized_now(shared, tree, UNDO, principal, &notes.authorized, held).await?;
    held_as_left(shared, &detail.images).await?;
    let unpinned = unpinned(shared, step.unit, &notes.made).await?;
    let (unit, writes) = staged(tree, &detail.images)?;
    let reversing = Reversing {
        kind: UnitKind::Undo { target: step.unit },
        step: step.unit,
        marks: vec![(step.unit, UnitState::Undone { by: unit })],
        unpinned,
        repinned: Vec::new(),
        writes,
        compensations: u32::try_from(notes.compensations.len()).unwrap_or(u32::MAX),
        beyond_reach: notes.left,
    };
    held_as_registered(shared, &notes.compensations)?;
    // **Its notes counted before any compensation runs**, so a compensation
    // that would note past what the undo's unit may keep is refused as it
    // runs, rather than the store refusing the undo after every compensation
    // was dispatched.
    let unheld = answer_of(&reversing);
    reversed(tree, reversing);
    for (command, args, _, _) in notes.compensations.iter().rev() {
        dispatch::nested(shared, tree, command, args, None)
            .await
            .map_err(|refusal| {
                refused(
                    refusal.kind(),
                    ErrorCode::KernelCompensationFailed,
                    format!(
                        "the compensation `{command}` the step registered failed, so nothing \
                         was undone: {refusal}"
                    ),
                    "compensation-failed",
                )
            })?;
        // **A compensation that answers may still have let the undo's unit
        // go** — a unit it began and dropped dooms the tree's — and the rest
        // must not run for an undo that is not kept.
        let doomed = dispatch::lock(tree)
            .unit()
            .and_then(units::Ambient::doomed)
            .cloned();
        if let Some(doomed) = doomed {
            return Err(refused(
                doomed.kind(),
                ErrorCode::KernelCompensationFailed,
                format!(
                    "the compensation `{command}` the step registered let the undo's unit go, \
                     so nothing was undone: {doomed}"
                ),
                "compensation-failed",
            ));
        }
    }
    // **What the undo did, read once its compensations ran**: a blob one of
    // them made is the undo's own, and its pin stays on.
    Ok(dispatch::lock(tree)
        .unit_mut()
        .and_then(|unit| {
            unit.holds_what_it_made();
            unit.reversing().map(answer_of)
        })
        .unwrap_or(unheld))
}

/// The blobs of `made` whose pin the undo of `step` takes off, each with its
/// size: those no other committed unit holds now.
///
/// **Held is asked of the units, not of what was held before.** A unit that
/// made the same bytes and is still committed — a step, or an undo whose
/// compensation made them — holds the blob, and a blob held before the step
/// is held now only while its holder is: the unit that first made its row,
/// still committed, holds it whether or not it kept a note of it. Asked of the
/// note alone, a blob two people's steps made would keep its pin once both
/// were undone, and the sweep would never take it.
///
/// **And a holder the pruner took holds it for good.** The pruner deletes a
/// unit's notes with it, and a unit it took is one no undo reaches any more,
/// so what it made stays: otherwise the bytes of such a step would be swept
/// once a later step that made them again was undone, nothing being left to
/// say who held them.
async fn unpinned<E: Environment>(
    shared: &Shared<E>,
    step: Snowflake,
    made: &[(String, bool)],
) -> Result<Vec<(String, u64)>, Failure> {
    let mut unpinned = Vec::new();
    for (hash, pinned_before) in made {
        let held_elsewhere = held_elsewhere(shared, hash, step)
            .await
            .map_err(|failed| store_failed(&failed))?;
        if held_elsewhere {
            continue;
        }
        let row = shared
            .store
            .blob(hash)
            .await
            .map_err(|failed| store_failed(&failed))?;
        let Some(row) = row else {
            continue;
        };
        if *pinned_before && holds(shared, row.created_unit).await? {
            continue;
        }
        unpinned.push((hash.clone(), row.size));
    }
    Ok(unpinned)
}

/// Whether a committed unit other than `except` holds a note of the blob
/// it made of `hash`, as a unit holding it before notes it or as one that
/// did not.
///
/// # Errors
///
/// What the store refused the question with, which is a refusal to act and
/// never a "no".
pub(crate) async fn held_elsewhere<E: Environment>(
    shared: &Shared<E>,
    hash: &str,
    except: Snowflake,
) -> Result<bool, LocalError> {
    for noted in [blobs::noted(hash, false), blobs::noted(hash, true)] {
        if shared.store.noted_elsewhere(&noted, except).await? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Whether `unit` still holds what it made: the store holds it committed, or
/// holds it no more — the pruner took it, beyond every undo, so what it made
/// stays.
async fn holds<E: Environment>(shared: &Shared<E>, unit: Snowflake) -> Result<bool, Failure> {
    Ok(shared
        .store
        .unit(unit)
        .await
        .map_err(|failed| store_failed(&failed))?
        .is_none_or(|held| held.row.state == UnitState::Committed))
}

/// `kernel.redo`: the newest undo of `principal` still kept, reversed in the
/// unit of the tree it runs in.
///
/// # Errors
///
/// A [`Failure`] the decision point journals, as the module header lists the
/// refusals in order: nothing to redo, an undo that dispatched compensations,
/// a unit the journal does not record, an action denied now, a key changed
/// since, a blob swept since — and what the store or the journal refused.
pub(crate) async fn redo<E: Environment>(
    shared: &Arc<Shared<E>>,
    tree: &Arc<Mutex<CallTree>>,
    principal: &Principal,
    held: Held,
) -> Result<String, Failure> {
    let rows = units_of(shared, principal).await?;
    let Some((undo, step)) = newest_undo(shared, &rows).await? else {
        return Err(refused(
            RefusalKind::NotFound,
            ErrorCode::KernelNothingToRedo,
            "no step this person undid is left to redo".to_owned(),
            "nothing-to-redo",
        ));
    };
    let undone = read(&undo.notes)?;
    if undone.compensated {
        return Err(refused(
            RefusalKind::Denied,
            ErrorCode::KernelRedoCompensated,
            "undoing the newest step dispatched its compensations, which cannot be taken back, \
             so it is not redone"
                .to_owned(),
            "compensated",
        ));
    }
    recorded(shared, &[step.row.unit, undo.row.unit]).await?;
    let notes = read(&step.notes)?;
    authorized_now(shared, tree, REDO, principal, &notes.authorized, held).await?;
    held_as_left(shared, &undo.images).await?;
    let repinned = repinned(shared, &notes.made).await?;
    let (unit, writes) = staged(tree, &undo.images)?;
    let reversing = Reversing {
        kind: UnitKind::Redo {
            target: undo.row.unit,
        },
        step: step.row.unit,
        marks: vec![
            (step.row.unit, UnitState::Committed),
            (undo.row.unit, UnitState::Undone { by: unit }),
        ],
        unpinned: Vec::new(),
        repinned,
        writes,
        compensations: 0,
        beyond_reach: notes.left,
    };
    let answer = answer_of(&reversing);
    reversed(tree, reversing);
    Ok(answer)
}

/// The newest undo among `rows` a redo can reverse, and the step it undid:
/// none when a step committed after it comes first, which is new work, or
/// when the pruner took the step it undid.
///
/// **Only a person's undo**: a reversal the kernel made because the journal
/// did not take a unit's close is labelled apart (`units::REVERSAL_LABEL`) and
/// passed over, as are a redo and an undo already redone — the one undo still
/// committed is the one its step is undone by, both moving in one
/// transaction.
async fn newest_undo<E: Environment>(
    shared: &Shared<E>,
    rows: &[UnitRow],
) -> Result<Option<(UnitDetail, UnitDetail)>, Failure> {
    for row in rows {
        match row.meta.kind {
            UnitKind::Step if row.state == UnitState::Committed => return Ok(None),
            UnitKind::Undo { target }
                if row.state == UnitState::Committed && row.meta.label != units::REVERSAL_LABEL =>
            {
                // **A step the pruner took is beyond reach**, as the migration
                // says a target that is gone is read — never answered as a
                // store holding a row it did not write.
                let Some(step) = shared
                    .store
                    .unit(target)
                    .await
                    .map_err(|failed| store_failed(&failed))?
                else {
                    return Ok(None);
                };
                return Ok(Some((detail_of(shared, row.unit).await?, step)));
            }
            _ => {}
        }
    }
    Ok(None)
}

/// The blobs of `made`, the ones the step a redo puts back made, that the
/// redo pins again: each whose row is unpinned now, as the unit records it.
///
/// **Every blob the step made, and not only those its undo took the pin
/// off**: the unit that held a blob when the step was undone may be undone
/// itself by the redo, and the step put back holds the blob again. A blob
/// pinned now is held already, and is not one this redo pinned — so the
/// redo's reversal, which takes off what it records, leaves that pin. Asked of
/// the undo's notes alone, a step redone after another person's undo would be
/// committed with its blob unpinned, and the sweep would delete the bytes.
///
/// # Errors
///
/// `not-found` with `[NC-KERN-0019]` when the store no longer holds one's row
/// or the blobs' store its bytes: the sweep took it, or something did that a
/// redo cannot put back.
async fn repinned<E: Environment>(
    shared: &Shared<E>,
    made: &[(String, bool)],
) -> Result<Vec<BlobCreated>, Failure> {
    let mut repinned = Vec::new();
    for (hash, _) in made {
        let row = shared
            .store
            .blob(hash)
            .await
            .map_err(|failed| store_failed(&failed))?;
        let bytes = match Hash::parse(hash) {
            Some(parsed) => shared
                .blobs
                .size(&parsed)
                .await
                .map_err(|failed| Failure::new(failed.refusal(), "blobs-refused"))?,
            None => None,
        };
        if row.is_none() || bytes.is_none() {
            return Err(refused(
                RefusalKind::NotFound,
                ErrorCode::KernelRedoBlobSwept,
                format!(
                    "the blob `{hash}` the step made has been swept since it was undone, so it \
                     is not redone"
                ),
                "blob-swept",
            ));
        }
        if let Some(row) = row.filter(|row| !row.pinned) {
            repinned.push(BlobCreated {
                hash: hash.clone(),
                size: row.size,
            });
        }
    }
    Ok(repinned)
}

#[cfg(test)]
mod tests {
    use nc_authz::EntityRef;
    use nc_canon::Value;
    use nc_local::LocalError;

    use nc_schema::descriptor::Reversibility;

    use super::{Moved, Note, Read, leaves_the_machine, left_by, pins_moved};

    /// A hash, of `fill` sixty-four times.
    fn hash(fill: char) -> String {
        core::iter::repeat_n(fill, 64).collect()
    }

    #[test]
    fn every_note_the_history_writes_is_read_back_as_itself() {
        let notes = [
            Note::Authorized {
                action: "Write".to_owned(),
                entity: EntityRef {
                    type_name: "Content".to_owned(),
                    id: "photo".to_owned(),
                },
            },
            Note::Compensation {
                command: "reference.note.put".to_owned(),
                args: Value::object([("note", Value::text("photo"))]),
                owner: "noctua:reference".to_owned(),
                by: "noctua:reference-peer".to_owned(),
            },
            Note::LeftTheMachine,
            Note::BlobCreated {
                hash: hash('a'),
                pinned_before: true,
            },
            Note::BlobUnpinned {
                hash: hash('b'),
                size: 15,
            },
            Note::BlobRepinned { hash: hash('c') },
            Note::CompensationsRan(2),
        ];
        for note in notes {
            let written = note.written();
            assert_eq!(Note::read(&written), Ok(note.clone()), "{written}");
            assert_eq!(
                nc_canon::parse(&written).map(|value| value.to_canonical()),
                Ok(written.clone()),
                "written as canonical JSON"
            );
        }
        assert_eq!(
            Note::Authorized {
                action: "Write".to_owned(),
                entity: EntityRef {
                    type_name: "Content".to_owned(),
                    id: "photo".to_owned(),
                },
            }
            .written(),
            r#"{"action":"Write","entity":{"id":"photo","type":"Content"},"note":"authorized"}"#
        );
        assert_eq!(
            Note::read(&left_by("limits.prune")),
            Ok(Note::LeftTheMachine),
            "a command's note is one that left the machine"
        );
        assert_eq!(
            Note::read(r#"{"host":"example.com","method":"POST","note":"left_the_machine"}"#),
            Ok(Note::LeftTheMachine),
            "and so is a request's"
        );
    }

    #[test]
    fn a_note_the_kernel_did_not_write_is_refused_as_not_its_own() {
        for written in [
            "not json",
            "1",
            r#"{"note":"something_else"}"#,
            r#"{"action":"Write","note":"authorized"}"#,
            r#"{"action":"Write","entity":{"id":"photo"},"note":"authorized"}"#,
            r#"{"action":"Write","entity":{"id":"p","type":"C","x":1},"note":"authorized"}"#,
            r#"{"command":"a.b","note":"compensation"}"#,
            // **Whole but for the field beside its own**, so that field is what
            // refuses it. A case lacking `by` as well would be refused for
            // that, and a reader keeping a field beside its own then decided by
            // nothing.
            r#"{"args":{},"by":"p:r","command":"a.b","extra":1,"note":"compensation","owner":"p:q"}"#,
            r#"{"compensations":1,"extra":1,"note":"compensations_ran"}"#,
            r#"{"args":{},"command":"a.b","note":"compensation"}"#,
            r#"{"args":{},"command":"a.b","note":"compensation","owner":1}"#,
            r#"{"args":{},"command":"a.b","note":"compensation","owner":"p:q"}"#,
            r#"{"args":{},"by":1,"command":"a.b","note":"compensation","owner":"p:q"}"#,
            r#"{"hash":"x","note":"blob_unpinned","size":1}"#,
            r#"{"hash":"x","note":"blob_repinned"}"#,
            r#"{"compensations":-1,"note":"compensations_ran"}"#,
            r#"{"hash":"x","note":"blob_created","pinned_before":false}"#,
        ] {
            assert!(
                matches!(Note::read(written), Err(LocalError::Unexpected(_))),
                "{written} was read as {:?}",
                Note::read(written)
            );
        }
    }

    #[test]
    fn a_steps_notes_are_read_whole_in_the_order_written() {
        let compensation = |n: i64| {
            Note::Compensation {
                command: "a.b".to_owned(),
                args: Value::object([("n", Value::Integer(n))]),
                owner: "p:q".to_owned(),
                by: "p:r".to_owned(),
            }
            .written()
        };
        let notes = [
            compensation(1),
            Note::LeftTheMachine.written(),
            compensation(2),
            Note::LeftTheMachine.written(),
            Note::CompensationsRan(0).written(),
        ];
        let read = Read::of(&notes).expect("the kernel's notes");
        assert_eq!(
            read.compensations,
            [
                (
                    "a.b".to_owned(),
                    r#"{"n":1}"#.to_owned(),
                    "p:q".to_owned(),
                    "p:r".to_owned()
                ),
                (
                    "a.b".to_owned(),
                    r#"{"n":2}"#.to_owned(),
                    "p:q".to_owned(),
                    "p:r".to_owned()
                )
            ]
        );
        assert_eq!(read.left, 2);
        assert!(!read.compensated, "no compensation ran");
        assert!(
            Read::of(&[Note::CompensationsRan(1).written()])
                .expect("the kernel's notes")
                .compensated
        );
    }

    /// Undo and redo are declared as this module runs them: no argument — which
    /// step is the kernel's to find — `Write` on the realm, changing state.
    ///
    /// **Both ways**, as `journal.verify`'s and `limits.prune`'s declarations
    /// are held to what implements them: an argument the catalog gains is one
    /// nothing here would read.
    #[test]
    fn the_declared_undo_and_redo_are_the_ones_this_module_implements() {
        for name in [super::UNDO, super::REDO] {
            let declared =
                nc_catalog::command(name).unwrap_or_else(|| panic!("`{name}` is declared"));
            assert!(
                declared.arguments.is_empty(),
                "`{name}` declares {:?}, and takes no argument",
                declared.arguments
            );
            assert_eq!(
                (
                    declared.action,
                    declared.resource_selector,
                    declared.mutating
                ),
                ("Write", "realm", true),
                "`{name}`"
            );
            assert!(
                matches!(declared.reversibility, nc_catalog::Reversibility::Write),
                "`{name}`: {:?}",
                declared.reversibility
            );
        }
    }

    /// A command that cannot be undone leaves the machine.
    ///
    /// So does one the kernel owns that changes state — `limits.prune` — but
    /// not undo and redo, a read, or a plugin's command that can be undone.
    #[test]
    fn what_leaves_the_machine_is_what_cannot_be_undone_or_the_kernels_own_change() {
        let listed = crate::registry::Registry::from_catalog().listed();
        let of = |name: &str| {
            listed
                .iter()
                .find(|command| command.name == name)
                .cloned()
                .unwrap_or_else(|| panic!("`{name}` is registered"))
        };
        assert!(leaves_the_machine(&of("limits.prune")));
        for name in ["journal.verify", "kernel.undo", "kernel.redo"] {
            assert!(!leaves_the_machine(&of(name)), "`{name}`");
        }
        let mut plugins = of("kernel.undo");
        plugins.name = "reference.note.put".to_owned();
        plugins.owner = crate::registry::Owner::Plugin(crate::PluginSlot(0));
        assert!(
            !leaves_the_machine(&plugins),
            "a plugin's command that can be undone"
        );
        plugins.reversibility = Reversibility::Irreversible;
        assert!(
            leaves_the_machine(&plugins),
            "a plugin's command that cannot be undone"
        );
    }

    #[test]
    fn the_pins_a_unit_moved_are_read_from_its_notes() {
        let notes = [
            Note::BlobUnpinned {
                hash: hash('a'),
                size: 3,
            }
            .written(),
            Note::BlobRepinned { hash: hash('b') }.written(),
            Note::LeftTheMachine.written(),
        ];
        assert_eq!(
            pins_moved(&notes),
            Ok(Moved {
                unpinned: vec![nc_local::BlobCreated {
                    hash: hash('a'),
                    size: 3
                }],
                repinned: vec![hash('b')],
            })
        );
    }
}
