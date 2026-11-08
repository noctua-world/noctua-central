//! Units of work: what a call tree's writes are held in until they are kept,
//! and how they are kept or let go.
//!
//! **A top-level dispatch of a command that changes state is one unit**
//! (`NOCTUA.md §11.5`). The decision point opens it in the journal before the
//! command runs, and every write any plugin makes in that call tree, the
//! nested dispatches included, is staged in the unit's [`Overlay`] rather than
//! written to the store. A read inside the tree sees the overlay over what is
//! committed; a read anywhere else sees only what is committed.
//!
//! **Kept in one order, and let go in one step.** Closing a unit commits its
//! overlay to the store as one transaction, and only then writes the journal's
//! closing bracket — so a store that refuses the commit leaves the journal
//! saying the unit was aborted, and a journal that refuses the close is
//! answered at once by committing the unit's reversal, so what the unit did
//! does not outlive a journal that never said it was kept. A close the journal
//! answers with an error is asked about first, since one that landed before
//! its answer was lost is a unit the journal kept. Aborting writes nothing to
//! the store at all: the overlay is dropped, and the store never held any of
//! it.
//!
//! **And the blobs it made with it.** A blob a command finishes is bytes on
//! disk at once, recorded in the unit (`crate::blobs::Made`), and a row only
//! when the unit commits — pinned, beside a note saying whether a committed
//! unit held it before. A unit let go deletes the bytes it made that nothing
//! else holds; one the journal does not keep is reversed with the pins it put
//! on taken off again, in the reversal's own transaction.
//!
//! **And what left the machine.** A request a command sends that asks its
//! host to change something is noted in the unit before it is sent
//! (`crate::net::note`), and the note is kept with the unit when it commits —
//! what undoing it is told it cannot reach. So is a command that cannot be
//! undone, and one the kernel runs on the realm itself. A unit let go takes
//! the note with it, and the request stays sent.
//!
//! **And what undoing it needs.** A unit keeps a note of each action a
//! command of it was allowed on each resource, which an undo authorizes again,
//! and of each compensating command a plugin registered in it
//! (`compensate-with`), which an undo dispatches (`crate::history`) — and
//! nothing else does: a unit let go runs none of them, and its refusal names
//! each one (`not_run`). A unit that reverses a step is an undo or a redo: it
//! commits the step's images back with the marks that move the step's state
//! and the pins it takes off or puts back, and notes each, so that its own
//! reversal — when the journal does not take its close — puts back all of it,
//! a pin another unit holds by then excepted.
//!
//! **And what it announced and scheduled.** An event published in a unit
//! reaches the event queue only when the unit is kept, and a timer set or
//! cancelled in it is armed or cancelled only then (`crate::events`,
//! `crate::timers`): a unit let go takes them with it, so nothing hears of,
//! or is woken by, a change that was not kept. Each is counted among the
//! unit's changes, as a write is, so a nested command that fails having
//! published, set or cancelled dooms the unit, and a caller that goes on past
//! its failure keeps none of what it held. A call tree that holds no unit
//! holds what it puts off itself ([`Later`]), released when the tree ends and
//! let go when the journal refused the entry of any of its dispatches.
//!
//! **A plugin can doom the unit, and never close it.** A unit a plugin begins
//! inside a command (`commands.begin`) is that command's own, flattened: its
//! commit lets it be, and its abort, or a handle let go or left open when the
//! command returns, dooms the unit of the whole call tree.
//!
//! **Bounded before the store sees it.** A unit stages at most
//! `store_writes`' row bound of keys and `unit_bytes` of keys and values, so a
//! plugin granted `store` cannot make the kernel hold more of its data than
//! that, however large each value it writes. The value a key held before the
//! unit first wrote it is counted beside the one it holds after: it is the
//! image the unit's undo puts back, so a step kept is one its undo can stage,
//! however much it deleted.
//!
//! **A write names what it expects.** A write with no revision creates a key
//! only where none is; a delete with none removes whatever is there; a
//! revision is compared, as the text the store wrote it as, with the one the
//! key holds — inside the unit, the one this unit's own earlier write gave it —
//! and the store checks the same again when the unit commits, so a key another
//! unit changed in between is a conflict and nothing is kept.
//!
//! **And what a stopped process left open, when the kernel opens next.** A
//! process that stops while a command runs leaves its unit open in the journal
//! and nothing of it in the store; one that stops between the store's commit
//! and the journal's close, or whose store refused the reversal of a unit the
//! journal did not take the close of, leaves the unit committed in the store
//! as well. [`recover`], which `Kernel::open` runs before anything else, finds
//! every unit of this realm the journal holds opened and never closed, commits
//! the reversal of what the store kept of it, newest first, and closes it as
//! aborted — and a kernel that cannot does not open.
//!
//! **And nothing prunes a unit the kernel left so while it runs.** A unit
//! whose reversal the store refused is held by the kernel until a prune
//! reverts it first ([`reverted_again`]), and the pruner leaves the units
//! table alone while one stays held: the record a reversal is built from, and
//! the undo a redo's reversal reads, are the units table's, which the pruner
//! deletes from oldest first.
//!
//! **Nor any realm's prune, a unit whose close no kernel has heard.** The
//! kernels of the realms one person belongs to share the file, and the pruner
//! takes the oldest units of all of them; so [`close`] commits a unit held
//! from the pruner in the store, with every unit whose state it moves
//! (`nc_local::LocalStore::commit_held`), and lets go once it hears the
//! journal's close or has reverted the unit, and [`recover`] lets go of the
//! holds of every unit of its realm the journal records as closed. Without the
//! hold, one realm's prune would take the record of a unit another realm's
//! stopped process left open — its recovery then closing it as aborted with its
//! writes kept — and the step of an undo left open, which would keep that
//! realm's kernel from opening.
//!
//! What this answers (`NOCTUA.md §2.4`): half-applied state — a command that
//! fails, a nested command that fails having written, published, set or
//! cancelled, a store or a journal that refuses, and a process that stops in
//! the middle of a unit, each leaves the store, the queue and the timers as
//! they were — effects that survive a failed journal write, a prune taking
//! the record of a unit whose reversal is still owed, and a unit that would
//! make the kernel hold more than its bound. What it does not answer: a
//! journal that answered a close with an error and could not then say whether
//! it took it, whose unit is reverted while the journal may record it as kept
//! — the journal only grows, so the two stay apart, the store's state is what
//! the history acts on, and `kernel.unit_aborted` with the cause
//! `journal_unknown` is the record of the reversal; a reversal the store refuses
//! for good, because a later unit that was kept changed what the unit wrote —
//! which only a kernel that went on after `kernel.revert_failed` lets happen —
//! and which keeps the kernel from opening until somebody repairs it; the
//! units table past its bound while a reversal the store refuses stays owed;
//! and two kernels over one journal, each of which would take the other's
//! open units for ones a stopped process left.

use std::collections::{BTreeMap, BTreeSet};

use nc_catalog::{ErrorCode, KERNEL};
use nc_ids::Snowflake;
use nc_local::{
    BlobCreated, CommitError, Entry, Expect, LocalError, Page, Quotas, Revision, StagedWrite,
    UnitDetail, UnitKind, UnitMeta, UnitRecord, UnitState,
};

use nc_authz::Principal;
use nc_ids::Clock as _;
use nc_journal::{Closing, Outcome, Store as _};

use crate::backend::StoreBackend as _;
use crate::blobs::{self, Hash, Made};
use crate::dispatch::{Cause, Refusal, RefusalKind, Waits};
use crate::events::Published;
use crate::logging::LogSink as _;
use crate::registry::RegisteredCommand;
use crate::timers::Timer;
use crate::ui::Shown;
use crate::{Environment, PluginSlot, Shared};

/// The catalog's `max_rows` for `table`, which every bound below is.
///
/// # Panics
///
/// When `catalog/limits.toml` declares no such table with a `max_rows`, which
/// the compiler refuses for a table that refuses at its limit — so a build of
/// this crate holds one.
fn max_rows(table: &str) -> u64 {
    nc_catalog::TABLES
        .iter()
        .find(|declared| declared.name == table)
        .and_then(|declared| declared.max_rows)
        .unwrap_or_else(|| panic!("`[tables.{table}]` declares the bound its writer refuses at"))
}

/// How many keys one unit may write: `store_writes`' `max_rows`, counted
/// over each unit's rows, which the store refuses past as well.
pub(crate) fn writes_per_unit() -> u64 {
    max_rows("store_writes")
}

/// The bounds a unit's commit is held to, with the plugins whose manifest
/// lowers its quota each holding its own number.
pub(crate) fn quotas(lowered: BTreeMap<String, u64>) -> Quotas {
    Quotas {
        entries_per_plugin: max_rows("store_entries"),
        lowered,
        blobs: max_rows("blobs"),
        writes_per_unit: writes_per_unit(),
        notes_per_unit: max_rows("store_notes"),
    }
}

/// How many notes one unit may keep: `store_notes`' `max_rows`, counted over
/// each unit's rows — one for each blob it makes, each request it sends that
/// leaves the machine, each action it was allowed and each compensation
/// registered in it.
pub(crate) fn blobs_per_unit() -> u64 {
    max_rows("store_notes")
}

/// How many notes a step keeps room for, for its undo: the two an undo keeps
/// beyond the ones that stand for what its step noted — its own action, and
/// that its compensations ran.
///
/// **What makes every step the kernel keeps one it can undo.** An undo notes
/// each pin it takes off, which is a blob its step made, and each action its
/// compensations were allowed, which is at most one for each the step
/// registered; so it keeps at most two notes more than its step did. Kept
/// at `store_notes`' bound, a step's undo would keep one more than the store
/// takes, and be refused every time — its compensations dispatched again before
/// each refusal, and undo reaching only the newest step.
const KEPT_FOR_ITS_UNDO: u64 = 2;

/// How many notes a step may keep: one unit's bound, less what it keeps room
/// for, for its undo ([`KEPT_FOR_ITS_UNDO`]).
pub(crate) fn notes_per_step() -> u64 {
    blobs_per_unit().saturating_sub(KEPT_FOR_ITS_UNDO)
}

/// How many compensations one unit may register: one fewer than the
/// `dispatches_per_tree` of the undo that dispatches them, whose own dispatch
/// is the first of its tree.
pub(crate) fn compensations() -> usize {
    usize::try_from(KERNEL.dispatches_per_tree.saturating_sub(1)).unwrap_or(usize::MAX)
}

/// The unit a call tree's writes belong to.
#[derive(Debug)]
pub(crate) struct Ambient {
    unit: Snowflake,
    overlay: Overlay,
    /// The blobs the unit made, each once, in the order first made.
    made: Vec<Made>,
    /// What left the machine in the unit, each as the note its record keeps,
    /// in the order sent.
    left: Vec<String>,
    /// Each action a command of it was allowed on each resource, once, as the
    /// note its record keeps.
    authorized: Vec<String>,
    /// Each compensation registered in it, as the note its record keeps, in
    /// the order registered.
    compensations: Vec<String>,
    /// What it does when it reverses a step: nothing, for every other unit.
    reversing: Option<Reversing>,
    /// What a person is shown for it in their history, when a plugin began a
    /// unit inside it with a label: the first such label, made safe to show.
    label: Option<String>,
    /// Why it will not be kept, and the cause the log names, when it will not.
    doomed: Option<(Refusal, Cause)>,
    /// What it put off, released once it is kept.
    later: Later,
    /// How long it waits on the journal: as the dispatch that opened it does.
    waits: Waits,
}

/// What a call tree puts off until it is kept: the events published in it,
/// the timers set in it, the armed timers it cancels and what it did to the
/// surfaces.
///
/// **A unit's, and a tree's that holds no unit.** A command that changes
/// state holds what it puts off in its unit, kept with it; a tree that only
/// reads — a top-level read, the dispatches nested beneath it, a callback —
/// holds it in a `Later` of its own, released once the tree has ended and
/// every entry it gave the journal was taken, and dropped otherwise. **Never at
/// once**: a read that armed its timer and queued its event at once would, when
/// the journal refused its entry — its answer withheld — still have its timer
/// fire and its event heard as the person, for a dispatch that is in no entry.
#[derive(Debug, Default)]
pub(crate) struct Later {
    /// The events published, queued once it is released.
    published: Vec<Published>,
    /// The timers set, by the identifier each was answered with, armed once
    /// it is released.
    timers: Vec<(u64, Timer)>,
    /// The armed timers cancelled, cancelled once it is released.
    cancelled: Vec<u64>,
    /// What was done to the surfaces, in the order it was done, shown once
    /// it is released.
    shown: Vec<Shown>,
    /// How often it was told to hold something for later.
    ///
    /// Each event published, timer set, timer taken back, cancel held and
    /// change to a surface counts once, and the count never goes back, so a frame that failed
    /// having held anything is told from one that held nothing, as a write is.
    held: u64,
}

impl Later {
    /// The events it holds, which the event queue counts as already its own.
    pub(crate) fn published(&self) -> &[Published] {
        &self.published
    }

    /// Holds `event` until it is released.
    pub(crate) fn publish(&mut self, event: Published) {
        self.published.push(event);
        self.held = self.held.saturating_add(1);
    }

    /// How many timers it holds for the plugin in `owner`, which its bound
    /// counts as already armed.
    pub(crate) fn timers_of(&self, owner: PluginSlot) -> usize {
        self.timers
            .iter()
            .filter(|(_, timer)| timer.owner == owner)
            .count()
    }

    /// Holds `timer`, set as `id`, until it is released.
    pub(crate) fn set_timer(&mut self, id: u64, timer: Timer) {
        self.timers.push((id, timer));
        self.held = self.held.saturating_add(1);
    }

    /// Takes back the timer `id` the plugin in `owner` set, answering whether
    /// it held one.
    pub(crate) fn unset_timer(&mut self, id: u64, owner: PluginSlot) -> bool {
        let before = self.timers.len();
        self.timers
            .retain(|(held, timer)| !(*held == id && timer.owner == owner));
        let unset = self.timers.len() < before;
        self.held = self.held.saturating_add(u64::from(unset));
        unset
    }

    /// What it holds done to the surfaces, in the order it was done, which
    /// each next change to one is judged after.
    pub(crate) fn surfaces_shown(&self) -> &[Shown] {
        &self.shown
    }

    /// Holds `shown`, a change to a surface, until it is released.
    ///
    /// **A tree rendered into a surface takes the place of the one held for
    /// it, and a close takes away the one held for what it closes.** Release
    /// applies what it holds in order, a surface shows the last tree it was
    /// given, and an identifier is never handed out twice, so what is shown
    /// does not change — and what the kernel keeps for one plugin's renders is
    /// at most a tree per surface it holds, however often it renders. Kept
    /// until the tree ended, every render of a plugin rendering a tree near
    /// `payload_bytes` into one surface in a loop, each render waiting on
    /// nothing and costing it little fuel, would make the kernel hold gigabytes
    /// for it until its tree's deadline.
    pub(crate) fn show(&mut self, shown: Shown) {
        if let Shown::Rendered { owner, id, .. } | Shown::Closed { owner, id } = &shown {
            self.shown.retain(|held| !held.renders_into(*owner, id));
        }
        self.shown.push(shown);
        self.held = self.held.saturating_add(1);
    }

    /// Holds the cancel of the armed timer `id` until it is released,
    /// answering whether it was not held already.
    pub(crate) fn cancel_timer(&mut self, id: u64) -> bool {
        if self.cancelled.contains(&id) {
            return false;
        }
        self.cancelled.push(id);
        self.held = self.held.saturating_add(1);
        true
    }

    /// Releases what it holds: its events reach the event queue, in the order
    /// published, its timers are armed and cancelled as they were set and
    /// cancelled, and what it did to the surfaces is shown, in the order done.
    ///
    /// **No bound is asked again here.** Each event and timer was counted
    /// against its bound as already queued or armed when it was held, and no
    /// other call tree ran in between, so what it holds is what the room was
    /// kept for.
    pub(crate) fn released<E: Environment>(self, shared: &Shared<E>) {
        let mut queue = crate::dispatch::lock(&shared.events);
        for event in self.published {
            queue.push(event);
        }
        drop(queue);
        let mut timers = crate::dispatch::lock(&shared.timers);
        for (id, timer) in self.timers {
            timers.arm(id, timer);
        }
        for id in self.cancelled {
            timers.cancel(id);
        }
        drop(timers);
        let mut surfaces = crate::dispatch::lock(&shared.surfaces);
        for shown in self.shown {
            surfaces.shown(shown);
        }
    }
}

/// What a unit that undoes or redoes a step commits beside its writes, and
/// what is said of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Reversing {
    /// An undo of a step, or a redo of an undo.
    pub(crate) kind: UnitKind,
    /// The step undone or redone, which the log names.
    pub(crate) step: Snowflake,
    /// The units whose state moves with this one, in its own transaction.
    pub(crate) marks: Vec<(Snowflake, UnitState)>,
    /// The blobs whose pin it takes off, each with its size.
    pub(crate) unpinned: Vec<(String, u64)>,
    /// The blobs it pins again.
    pub(crate) repinned: Vec<BlobCreated>,
    /// How many keys it puts back.
    pub(crate) writes: u32,
    /// How many compensating commands it dispatched.
    pub(crate) compensations: u32,
    /// How many of the step's effects had left the machine, and were the
    /// compensations' to answer.
    pub(crate) beyond_reach: u32,
}

impl Reversing {
    /// Whether it is an undo, and not a redo.
    pub(crate) const fn undoes(&self) -> bool {
        matches!(self.kind, UnitKind::Undo { .. })
    }

    /// How many notes [`Reversing::recorded`] writes.
    fn noted(&self) -> usize {
        self.unpinned
            .len()
            .saturating_add(self.repinned.len())
            .saturating_add(usize::from(self.compensations > 0))
    }

    /// The notes its record keeps of what it did: each pin it took off or
    /// put back, and the compensations it dispatched.
    fn recorded(&self) -> Vec<String> {
        let mut notes: Vec<String> = self
            .unpinned
            .iter()
            .map(|(hash, size)| {
                crate::history::Note::BlobUnpinned {
                    hash: hash.clone(),
                    size: *size,
                }
                .written()
            })
            .collect();
        notes.extend(self.repinned.iter().map(|blob| {
            crate::history::Note::BlobRepinned {
                hash: blob.hash.clone(),
            }
            .written()
        }));
        if self.compensations > 0 {
            notes.push(crate::history::Note::CompensationsRan(self.compensations).written());
        }
        notes
    }
}

impl Ambient {
    /// A unit the journal opened as `unit`, holding nothing yet.
    pub(crate) fn new(unit: Snowflake) -> Self {
        Self {
            unit,
            overlay: Overlay::default(),
            made: Vec::new(),
            left: Vec::new(),
            authorized: Vec::new(),
            compensations: Vec::new(),
            reversing: None,
            label: None,
            doomed: None,
            later: Later::default(),
            waits: Waits::TOP,
        }
    }

    /// The same unit, whose close waits on the journal as `waits` says: as
    /// the dispatch nested in a call tree that opened it waits.
    pub(crate) const fn waiting(mut self, waits: Waits) -> Self {
        self.waits = waits;
        self
    }

    /// What the unit put off, kept with it.
    pub(crate) const fn later(&self) -> &Later {
        &self.later
    }

    /// What the unit put off, to put off more.
    pub(crate) const fn later_mut(&mut self) -> &mut Later {
        &mut self.later
    }

    /// Whether the unit made the blob `hash`.
    pub(crate) fn made_blob(&self, hash: &Hash) -> bool {
        self.made.iter().any(|made| made.hash == *hash)
    }

    /// The blobs the unit made.
    pub(crate) fn made(&self) -> &[Made] {
        &self.made
    }

    /// Refuses one more blob when the unit keeps as many notes as it may.
    ///
    /// # Errors
    ///
    /// A refusal with `[NC-PLUG-0017]` at the bound, asked before a writer is
    /// finished so that no bytes are named for a unit that cannot hold them.
    pub(crate) fn room_for_a_blob(&self) -> Result<(), Refusal> {
        let bound = self.room();
        if self.notes() < bound {
            return Ok(());
        }
        Err(Refusal::new(
            RefusalKind::Unavailable,
            ErrorCode::PluginBoundReached,
            format!(
                "a unit keeps at most {bound} note(s) of the blobs it made and the requests that \
                 left the machine, and this blob would be one more"
            ),
        ))
    }

    /// How many notes the unit keeps.
    ///
    /// One for each blob it made, each request that left the machine, each
    /// action it was allowed and each compensation registered in it — and,
    /// for a unit that reverses a step, each its reversal records, counted
    /// from when it is told what it reverses, before its undo dispatches a
    /// compensation.
    fn notes(&self) -> u64 {
        let kept = self
            .made
            .len()
            .saturating_add(self.left.len())
            .saturating_add(self.authorized.len())
            .saturating_add(self.compensations.len())
            .saturating_add(self.reversing.as_ref().map_or(0, Reversing::noted));
        u64::try_from(kept).unwrap_or(u64::MAX)
    }

    /// How many notes the unit may keep: `store_notes`' bound for one unit,
    /// less what a step keeps room for, for its undo ([`KEPT_FOR_ITS_UNDO`]),
    /// when it is not itself a reversal.
    fn room(&self) -> u64 {
        if self.reversing.is_some() {
            blobs_per_unit()
        } else {
            notes_per_step()
        }
    }

    /// Refuses one more note when the unit keeps as many as it may.
    fn room_for(&self, what: &str) -> Result<(), Refusal> {
        let bound = self.room();
        if self.notes() >= bound {
            return Err(Refusal::new(
                RefusalKind::Unavailable,
                ErrorCode::PluginBoundReached,
                format!(
                    "a unit keeps at most {bound} note(s) of what it did, and {what} would be \
                     one more"
                ),
            ));
        }
        Ok(())
    }

    /// Notes that a command of the unit was allowed what `note` says: once,
    /// however often it ran.
    ///
    /// # Errors
    ///
    /// A refusal with `[NC-PLUG-0017]` when the note is a new one and the unit
    /// keeps as many as it may.
    fn authorized(&mut self, note: String) -> Result<(), Refusal> {
        if self.authorized.contains(&note) {
            return Ok(());
        }
        self.room_for("the action a command was allowed")?;
        self.authorized.push(note);
        Ok(())
    }

    /// Notes, once a command of the unit has run, the action it was allowed
    /// as `note` says, and dooms the unit when the note finds no room.
    ///
    /// **Doomed, never kept without it**: an undo authorizes again only what
    /// its step noted, so a unit kept without the note of an action one of its
    /// commands was allowed is one whose undo would put back writes nobody
    /// decided again — and a plugin can fill a unit's notes before it returns,
    /// with compensations or blobs, to make it one.
    pub(crate) fn ran(&mut self, note: String) {
        if let Err(refusal) = self.authorized(note) {
            self.doom(refusal);
        }
    }

    /// Notes, before `command` runs in the unit, that it leaves the machine
    /// when it does: when it cannot be undone, or is the kernel's own change to
    /// the realm (`crate::history::leaves_the_machine`).
    ///
    /// **So that undo stops at the step it becomes**, which its note is what
    /// makes a barrier.
    ///
    /// # Errors
    ///
    /// A refusal with `[NC-PLUG-0017]` when it leaves the machine and the unit
    /// keeps as many notes as it may, and then it must not run: run without
    /// its note, its step would be no barrier, and an undo would reverse the
    /// half of it the kernel can reach.
    pub(crate) fn leaving(&mut self, command: &RegisteredCommand) -> Result<(), Refusal> {
        if crate::history::leaves_the_machine(command) {
            self.left_the_machine(crate::history::left_by(&command.name))
        } else {
            Ok(())
        }
    }

    /// Registers the compensation `note` describes, to be dispatched if the
    /// step the unit becomes is undone.
    ///
    /// **At most one fewer than a call tree may dispatch**, [`compensations`]:
    /// the undo dispatches each one nested in its own tree, which is the
    /// first dispatch of it. A step registering as many as a tree may dispatch
    /// would be accepted, and every undo of it refused at the last compensation
    /// — so undo would never reach past it.
    ///
    /// # Errors
    ///
    /// A refusal with `[NC-PLUG-0017]` when the unit keeps as many notes as it
    /// may, or holds as many compensations.
    pub(crate) fn compensate(&mut self, note: String) -> Result<(), Refusal> {
        self.room_for("this compensation")?;
        let most = compensations();
        if self.compensations.len() >= most {
            return Err(Refusal::new(
                RefusalKind::Unavailable,
                ErrorCode::PluginBoundReached,
                format!(
                    "a unit registers at most {most} compensation(s), one fewer than the \
                     dispatches one call tree may make, which its undo dispatches them in, and \
                     this would be one more"
                ),
            ));
        }
        self.compensations.push(note);
        Ok(())
    }

    /// The command of each compensation registered in the unit, in the order
    /// registered.
    pub(crate) fn compensations_registered(&self) -> Vec<String> {
        self.compensations
            .iter()
            .filter_map(|note| match crate::history::Note::read(note) {
                Ok(crate::history::Note::Compensation { command, .. }) => Some(command),
                _ => None,
            })
            .collect()
    }

    /// Names the unit as a plugin labelled a unit it began inside it, unless
    /// one did first: the text a person is shown for the step.
    ///
    /// **Shown, so made safe to show**: a character `nc_schema::unseen`
    /// answers for becomes U+FFFD and the label is cut, at a character's boundary,
    /// to [`LABEL_BYTES`] — it is text a plugin chose, and the store keeps it
    /// for as long as the step. An empty label names nothing.
    pub(crate) fn labelled(&mut self, label: &str) {
        if self.label.is_some() {
            return;
        }
        let mut shown = String::new();
        for character in label.chars() {
            let character = if nc_schema::unseen(character) {
                char::REPLACEMENT_CHARACTER
            } else {
                character
            };
            if shown.len() + character.len_utf8() > LABEL_BYTES {
                break;
            }
            shown.push(character);
        }
        if !shown.trim().is_empty() {
            self.label = Some(shown);
        }
    }

    /// The label a plugin gave the unit, when one did.
    pub(crate) fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Makes the unit the reversal of a step, as `reversing` says.
    pub(crate) fn reverse(&mut self, reversing: Reversing) {
        self.reversing = Some(reversing);
    }

    /// What the unit reverses, when it reverses a step.
    pub(crate) const fn reversing(&self) -> Option<&Reversing> {
        self.reversing.as_ref()
    }

    /// Takes every blob the unit made itself off the pins its reversal takes
    /// off.
    ///
    /// **An undo whose compensation made the blob its step made holds it**, as
    /// a step does, so the pin stays on — and what the undo answers, journals
    /// and keeps a note of says so, all three reading the reversal after
    /// this. Otherwise the store would refuse such an undo every time, the blob
    /// pinned and unpinned by one unit; and with only the store's record
    /// mended, the undo would answer and journal `reverted.blobs: 1` and note a
    /// pin it never took off.
    pub(crate) fn holds_what_it_made(&mut self) {
        let made = &self.made;
        if let Some(reversing) = &mut self.reversing {
            reversing
                .unpinned
                .retain(|(hash, _)| !made.iter().any(|made| made.hash.as_str() == hash));
        }
    }

    /// Every note its record keeps, in one order.
    ///
    /// The blobs, what left the machine, the actions allowed, the
    /// compensations, and what a reversal did.
    fn notes_kept(&self) -> Vec<String> {
        let mut notes: Vec<String> = self.made.iter().map(blobs::note).collect();
        notes.extend(self.left.iter().cloned());
        notes.extend(self.authorized.iter().cloned());
        notes.extend(self.compensations.iter().cloned());
        if let Some(reversing) = &self.reversing {
            notes.extend(reversing.recorded());
        }
        notes
    }

    /// Notes that the request `note` describes left the machine in the unit,
    /// before it is sent.
    ///
    /// # Errors
    ///
    /// A refusal with `[NC-PLUG-0017]` when the unit keeps as many notes as it
    /// may, and then the request is not sent.
    pub(crate) fn left_the_machine(&mut self, note: String) -> Result<(), Refusal> {
        self.room_for("this request")?;
        self.left.push(note);
        Ok(())
    }

    /// Takes back the newest note `note` of a request that left the machine,
    /// which the transport then refused before it wrote any of it: what was
    /// never sent is nothing an undo cannot reach.
    pub(crate) fn not_sent(&mut self, note: &str) {
        if let Some(at) = self.left.iter().rposition(|left| left == note) {
            self.left.remove(at);
        }
    }

    /// What left the machine in the unit, each as the note its record keeps.
    #[cfg(test)]
    pub(crate) fn left(&self) -> &[String] {
        &self.left
    }

    /// Records the blob `made` as the unit's, unless it made it already: the
    /// first making is the one that says what this machine held before.
    pub(crate) fn make(&mut self, made: Made) {
        if !self.made_blob(&made.hash) {
            self.made.push(made);
        }
    }

    /// How many changes the unit holds — every write staged, every blob made
    /// and everything held for later: what tells a frame that changed
    /// something from one that did not.
    pub(crate) fn changes(&self) -> u64 {
        self.overlay
            .changes()
            .saturating_add(u64::try_from(self.made.len()).unwrap_or(u64::MAX))
            .saturating_add(self.later.held)
    }

    /// The unit's identifier, which its journal brackets carry.
    pub(crate) const fn unit(&self) -> Snowflake {
        self.unit
    }

    /// What the unit staged.
    pub(crate) const fn overlay(&self) -> &Overlay {
        &self.overlay
    }

    /// What the unit staged, to stage more.
    pub(crate) const fn overlay_mut(&mut self) -> &mut Overlay {
        &mut self.overlay
    }

    /// Marks the unit as one that will not be kept, because a command inside
    /// it failed as `cause` says.
    ///
    /// The first cause is the one kept: a unit doomed twice is reported for
    /// what first made it unkeepable, which is the thing to look at.
    pub(crate) fn doom(&mut self, cause: Refusal) {
        self.doom_for(cause, Cause::Nested);
    }

    /// Marks the unit as one that will not be kept, as `refusal` says, for the
    /// log's `cause`: a plugin that aborted a unit it began, or let one go.
    pub(crate) fn doom_for(&mut self, refusal: Refusal, cause: Cause) {
        if self.doomed.is_none() {
            self.doomed = Some((refusal, cause));
        }
    }

    /// Why the unit will not be kept, if it will not.
    pub(crate) fn doomed(&self) -> Option<&Refusal> {
        self.doomed.as_ref().map(|(refusal, _)| refusal)
    }

    /// The cause the log names for a unit that will not be kept.
    pub(crate) fn doomed_for(&self) -> Option<Cause> {
        self.doomed.as_ref().map(|(_, cause)| *cause)
    }
}

/// What a unit staged for one key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InUnit {
    /// Nothing: what the store holds is what the key holds.
    Untouched,
    /// A delete: the key holds nothing, whatever the store holds.
    Deleted,
    /// A value, with the revision its write commits with.
    Written(Entry),
}

/// One key a unit has staged: where it sits among the unit's writes, what
/// the store must hold for it when the unit commits, and what it holds after.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Staged {
    position: u32,
    expect: Expect,
    after: Option<String>,
}

/// The writes a unit has staged, by plugin and key, in the order each key was
/// first written.
///
/// **The order is the revisions'.** The store numbers a unit's writes by
/// their place in the record, so a key's place here, fixed when it is first
/// staged, is the revision its write commits with, and the one a plugin is
/// handed when it writes is the one it reads back once the unit is kept.
///
/// **Bounded in keys and in bytes.** A unit stages at most
/// `writes_per_unit` keys, and at most `unit_bytes` of keys and the values
/// they hold now — a value written over gives back what it held — because
/// the kernel holds all of it until the unit closes, and the count of keys
/// times the largest value is not a bound anyone chose.
///
/// **And the value each key held before it, kept counted.** That is the
/// image the unit's record keeps and its undo stages back, through this same
/// bound: counted by what it wrote alone, a step that deleted or wrote over
/// more than the bound of values would be kept and then refused at every undo,
/// measured, and so undo would never reach past the step a large accidental
/// delete is.
#[derive(Debug, Default)]
pub(crate) struct Overlay {
    order: Vec<(String, String)>,
    staged: BTreeMap<(String, String), Staged>,
    changes: u64,
    bytes: u64,
}

impl Overlay {
    /// What the unit staged for `key` of `plugin`: nothing, a delete, or the
    /// value with the revision its write will commit with.
    pub(crate) fn read(&self, unit: Snowflake, plugin: &str, key: &str) -> InUnit {
        let Some(staged) = self.staged.get(&(plugin.to_owned(), key.to_owned())) else {
            return InUnit::Untouched;
        };
        staged.after.as_ref().map_or(InUnit::Deleted, |value| {
            InUnit::Written(Entry {
                value: value.clone(),
                revision: Revision {
                    unit,
                    n: staged.position,
                },
            })
        })
    }

    /// Stages `after` for `key` of `plugin`, over `committed` — what the store
    /// holds for it now — and answers the revision the write commits with, or
    /// nothing for a delete.
    ///
    /// **What the store must hold is what it held when the unit first touched
    /// the key**, and a later write of the same key keeps that expectation: the
    /// unit's own earlier write is not something the store will see.
    ///
    /// # Errors
    ///
    /// A refusal with `[NC-PLUG-0017]` when the key would be one more than a
    /// unit may write, or the write would take the unit past the bytes it may
    /// stage, what `committed` holds counted for a key it first writes — and
    /// then nothing is staged.
    pub(crate) fn stage(
        &mut self,
        unit: Snowflake,
        plugin: &str,
        key: &str,
        committed: Option<&Entry>,
        after: Option<String>,
    ) -> Result<Option<Revision>, Refusal> {
        let expect = committed.map_or(Expect::Absent, |entry| Expect::Revision(entry.revision));
        let replaced = committed.map_or(0, |entry| bytes_of(&entry.value));
        self.staged_as(unit, (plugin, key), expect, replaced, after)
    }

    /// Stages `after` for `key` of `plugin` as an undo or a redo puts it back:
    /// held, when the unit commits, to the key still holding `holding`, the
    /// value the step being reversed left there, or nothing.
    ///
    /// **By value, not by revision**: the revision the image was taken at has
    /// been replaced by the step being reversed, and what decides whether the
    /// key changed since is what it holds.
    ///
    /// # Errors
    ///
    /// A refusal with `[NC-PLUG-0017]` at the unit's bounds, as [`Self::stage`]
    /// refuses, `holding` counted as what the key held before — the image a
    /// redo stages back, which its step was held to as well.
    pub(crate) fn restore(
        &mut self,
        unit: Snowflake,
        plugin: &str,
        key: &str,
        holding: Option<String>,
        after: Option<String>,
    ) -> Result<(), Refusal> {
        let replaced = holding.as_deref().map_or(0, bytes_of);
        self.staged_as(unit, (plugin, key), Expect::Value(holding), replaced, after)
            .map(drop)
    }

    /// Stages `after` at `at`, expecting `expect` of the store when the key is
    /// first staged, and answers the revision the write commits with.
    ///
    /// `replaced` is the size of what the key held before the unit, counted
    /// only when the unit first stages it.
    fn staged_as(
        &mut self,
        unit: Snowflake,
        (plugin, key): (&str, &str),
        expect: Expect,
        replaced: u64,
        after: Option<String>,
    ) -> Result<Option<Revision>, Refusal> {
        let at = (plugin.to_owned(), key.to_owned());
        let deleting = after.is_none();
        let bytes = self.bytes_with(&at, replaced, after.as_deref());
        if bytes > KERNEL.unit_bytes {
            return Err(Refusal::new(
                RefusalKind::Unavailable,
                ErrorCode::PluginBoundReached,
                format!(
                    "a unit stages at most {} bytes of keys, of the values they hold and of \
                     the ones they held before it, which its undo puts back, and this write \
                     would make it {bytes}",
                    KERNEL.unit_bytes
                ),
            ));
        }
        let position = if let Some(staged) = self.staged.get_mut(&at) {
            staged.after = after;
            staged.position
        } else {
            let bound = writes_per_unit();
            if u64::try_from(self.order.len()).unwrap_or(u64::MAX) >= bound {
                return Err(Refusal::new(
                    RefusalKind::Unavailable,
                    ErrorCode::PluginBoundReached,
                    format!(
                        "a unit writes at most {bound} key(s), and this write would be one more"
                    ),
                ));
            }
            let position = u32::try_from(self.order.len()).map_err(|_| {
                Refusal::new(
                    RefusalKind::Unavailable,
                    ErrorCode::PluginBoundReached,
                    format!("a unit writes at most {} key(s)", u32::MAX),
                )
            })?;
            self.order.push(at.clone());
            self.staged.insert(
                at,
                Staged {
                    position,
                    expect,
                    after,
                },
            );
            position
        };
        self.bytes = bytes;
        self.changes += 1;
        Ok((!deleting).then_some(Revision { unit, n: position }))
    }

    /// The bytes of keys and values the unit would stage, `after` written to `at`
    /// over a value of `replaced` bytes.
    ///
    /// A key is counted once, however often it is written, with what it held
    /// before the unit — `replaced`, counted the first time and kept — and a
    /// value it is written only while the key holds it.
    fn bytes_with(&self, at: &(String, String), replaced: u64, after: Option<&str>) -> u64 {
        let size = |text: &str| u64::try_from(text.len()).unwrap_or(u64::MAX);
        let written = after.map_or(0, size);
        match self.staged.get(at) {
            Some(staged) => self
                .bytes
                .saturating_sub(staged.after.as_deref().map_or(0, size))
                .saturating_add(written),
            None => self
                .bytes
                .saturating_add(size(&at.1))
                .saturating_add(replaced)
                .saturating_add(written),
        }
    }

    /// How many writes have been staged, the same key counted every time:
    /// what tells a frame that wrote from one that did not.
    pub(crate) const fn changes(&self) -> u64 {
        self.changes
    }

    /// The keys of `plugin` beginning with `prefix`, after `cursor` when one is
    /// given, that the unit holds and that it deleted.
    pub(crate) fn listed(
        &self,
        plugin: &str,
        prefix: &str,
        cursor: Option<&str>,
    ) -> (BTreeSet<String>, BTreeSet<String>) {
        let mut held = BTreeSet::new();
        let mut deleted = BTreeSet::new();
        for ((owner, key), staged) in &self.staged {
            if owner != plugin
                || !key.starts_with(prefix)
                || cursor.is_some_and(|cursor| key.as_str() <= cursor)
            {
                continue;
            }
            if staged.after.is_some() {
                held.insert(key.clone());
            } else {
                deleted.insert(key.clone());
            }
        }
        (held, deleted)
    }

    /// Every plugin the unit writes for.
    pub(crate) fn plugins(&self) -> BTreeSet<String> {
        self.order
            .iter()
            .map(|(plugin, _)| plugin.clone())
            .collect()
    }

    /// The unit's writes as the store commits them, in the order each key was
    /// first written.
    pub(crate) fn writes(&self) -> Vec<StagedWrite> {
        self.order
            .iter()
            .filter_map(|at| {
                self.staged.get(at).map(|staged| StagedWrite {
                    plugin: at.0.clone(),
                    key: at.1.clone(),
                    expect: staged.expect.clone(),
                    after: staged.after.clone(),
                })
            })
            .collect()
    }
}

/// How many bytes `text` is, as a unit's bound counts it.
fn bytes_of(text: &str) -> u64 {
    u64::try_from(text.len()).unwrap_or(u64::MAX)
}

/// The page of `limit` keys a plugin is shown: `committed`, read from the
/// store with room for every key the unit deleted, with the keys the unit
/// `held` added and the ones it `deleted` taken out.
///
/// **No key past the committed page is shown while the store holds more.** A
/// key the unit added that sorts after the last committed key read could have
/// committed keys before it that this page never read, so it waits for the
/// page that reads them.
pub(crate) fn merged(
    committed: Page,
    held: BTreeSet<String>,
    deleted: &BTreeSet<String>,
    limit: u32,
) -> Page {
    let limit = usize::try_from(limit.max(1)).unwrap_or(usize::MAX);
    let reached = committed.next.clone();
    let mut keys: BTreeSet<String> = committed.keys.into_iter().collect();
    keys.extend(held);
    let mut keys: Vec<String> = keys
        .into_iter()
        .filter(|key| !deleted.contains(key))
        .filter(|key| reached.as_ref().is_none_or(|last| key <= last))
        .collect();
    let more = keys.len() > limit || reached.is_some();
    keys.truncate(limit);
    let next = if more { keys.last().cloned() } else { None };
    Page { keys, next }
}

/// Whether `token` has the shape of a revision the store writes: a unit in
/// base32 as Crockford writes it, a dot, and the write's place among the
/// unit's.
///
/// **Read for its shape and never decoded.** Decoding is
/// `Snowflake::from_crockford` over text a plugin chose, in a crate that
/// reaches the central log; the revision a key holds is compared as the text
/// it is written as instead, which is all a write needs to know.
pub(crate) fn revision_shaped(token: &str) -> bool {
    /// Crockford's alphabet as `Snowflake::to_crockford` writes it.
    const CROCKFORD: &str = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    token.rsplit_once('.').is_some_and(|(unit, n)| {
        (1..=13).contains(&unit.len())
            && unit.chars().all(|character| CROCKFORD.contains(character))
            && (1..=10).contains(&n.len())
            && n.bytes().all(|byte| byte.is_ascii_digit())
    })
}

/// Whether a write naming `expected` may land over `current`.
///
/// # Errors
///
/// A refusal with `[NC-PLUG-0018]`, the store conflict, when the key does not
/// hold what the write expected — a value where a write that names no revision
/// creates one only where none is, nothing where it named one, or another
/// revision — and `invalid` with `[NC-PLUG-0027]` for text that is not a
/// revision at all, which nothing changing meanwhile could have caused.
pub(crate) fn expected(
    key: &str,
    current: Option<&Entry>,
    expected: Option<&str>,
    creating: bool,
) -> Result<(), Refusal> {
    let conflict = |why: String| {
        Err(Refusal::new(
            RefusalKind::Conflict,
            ErrorCode::PluginStoreConflict,
            format!("the key {key:?} {why}"),
        ))
    };
    match (expected, current) {
        (None, Some(_)) if creating => conflict(
            "holds a value, and a write that names no revision creates a key only where none is"
                .to_owned(),
        ),
        (Some(token), _) if !revision_shaped(token) => Err(Refusal::new(
            RefusalKind::Invalid,
            ErrorCode::PluginNotARevision,
            format!(
                "the write to the key {key:?} names {}, which is not a revision",
                crate::registry::quoted_up_to(token, REVISION_QUOTED)
            ),
        )),
        (Some(token), None) => conflict(format!("holds nothing, and the write expected {token}")),
        (Some(token), Some(held)) if held.revision.to_string() != token => conflict(format!(
            "holds {}, and the write expected {token}",
            held.revision
        )),
        _ => Ok(()),
    }
}

/// How many characters of text a plugin named as a revision a refusal
/// repeats: a revision is at most twenty-four, and anything longer is not one.
const REVISION_QUOTED: usize = 32;

/// What a store's refusal of a unit's commit tells whoever dispatched it.
pub(crate) fn commit_refused(error: &CommitError) -> Refusal {
    match error {
        CommitError::Conflict { plugin, key } => Refusal::new(
            RefusalKind::Conflict,
            ErrorCode::PluginStoreConflict,
            format!(
                "the key {key:?} of `{plugin}` changed after this unit read it, so nothing it \
                 wrote was kept"
            ),
        ),
        CommitError::Quota { plugin } => Refusal::new(
            RefusalKind::Unavailable,
            ErrorCode::PluginBoundReached,
            format!(
                "`{plugin}` would hold more keys than its quota allows, so nothing this unit \
                 wrote was kept"
            ),
        ),
        CommitError::TooManyBlobs => Refusal::new(
            RefusalKind::Unavailable,
            ErrorCode::PluginBoundReached,
            "this machine would hold more blobs than it may, so nothing this unit wrote was kept"
                .to_owned(),
        ),
        CommitError::Invalid(why) => Refusal::new(
            RefusalKind::Internal,
            ErrorCode::KernelUnitAborted,
            format!("the store refused the unit as one no unit can be, so nothing was kept: {why}"),
        ),
        CommitError::Store(failed) => store_failed(failed),
    }
}

/// What a store's refusal of a unit's commit tells whoever dispatched it,
/// for a unit that undoes or redoes a step when `reversing`.
///
/// **A key that changed is the history's conflict for an undo or a redo**
/// (`[NC-KERN-0011]`): what was to be put back was changed while it was, and
/// the caller is told so in the history's words rather than a plugin's.
pub(crate) fn refused_commit(error: &CommitError, reversing: bool) -> Refusal {
    match error {
        CommitError::Conflict { plugin, key } if reversing => Refusal::new(
            RefusalKind::Conflict,
            ErrorCode::KernelUndoConflict,
            format!(
                "the key {key:?} of `{plugin}` changed while it was being put back, so nothing \
                 was undone or redone"
            ),
        ),
        _ => commit_refused(error),
    }
}

/// `refusal`, naming the compensations registered in the unit `ambient`: a
/// unit that is not kept runs none of them, and whoever asked is told which.
///
/// **A compensation runs on undo only**, as the world's `compensate-with`
/// says, and a unit let go is no step anything undoes — so what it registered
/// never runs, and a caller who must set right what the unit sent is told what
/// the plugin meant to set it right with. Its kind and code are kept, since
/// what refused the unit is what the caller acts on; a unit with none
/// registered is refused as it was.
pub(crate) fn not_run(refusal: Refusal, ambient: &Ambient) -> Refusal {
    let registered = ambient.compensations_registered();
    if registered.is_empty() {
        return refusal;
    }
    let named: Vec<String> = registered
        .iter()
        .map(|command| format!("`{command}`"))
        .collect();
    refusal.ending(&format!(
        "; the compensation(s) registered in it did not run: {}",
        named.join(", ")
    ))
}

/// What a store that failed tells whoever was waiting on it.
///
/// It is `unavailable` when the store did not answer, which may pass,
/// and `internal` otherwise, each carrying the store's own code.
pub(crate) fn store_failed(failed: &LocalError) -> Refusal {
    let kind = match failed {
        LocalError::Unavailable(_) => RefusalKind::Unavailable,
        LocalError::Refused(_) | LocalError::Unexpected(_) | LocalError::Migration(_) => {
            RefusalKind::Internal
        }
    };
    let (LocalError::Refused(why)
    | LocalError::Unexpected(why)
    | LocalError::Migration(why)
    | LocalError::Unavailable(why)) = failed;
    Refusal::new(kind, failed.code(), why.clone())
}

/// The record that reverses the committed unit `detail`, as the unit `by`:
/// every key back to what it held before, held to holding what the unit left,
/// the unit marked as undone by it, and each unit in `restored` put back in
/// the state it held before.
///
/// **What the kernel commits when the journal will not take a unit's close.**
/// The two are one transaction, so a reversal that lands leaves the store as
/// it was and the unit's row saying what undid it, and one that does not
/// leaves the unit as it committed for the recovery to find. The pins the unit
/// put on the blobs it made, or put back as a redo, are taken off in the same
/// transaction, as of the reversal, so the sweep deletes their bytes once the
/// grace has passed; and the pins an undo took off are put back.
///
/// A pin the unit put on, or put back as a redo, stays on when it is in
/// `held`, the blobs another committed unit holds now (`still_held`): taking
/// it off would leave that unit's blob to the sweep.
///
/// # Errors
///
/// [`LocalError::Unexpected`] when a note of a blob the unit made, or of one
/// it took the pin off or put back, is not one the kernel writes.
pub(crate) fn reversal(
    detail: &UnitDetail,
    by: Snowflake,
    committed_at_ms: u64,
    restored: Vec<(Snowflake, UnitState)>,
    held: &[String],
) -> Result<UnitRecord, LocalError> {
    let moved = crate::history::pins_moved(&detail.notes)?;
    let mut unpinned = blobs::unpinned_by(&detail.notes)?;
    unpinned.extend(moved.repinned);
    unpinned.retain(|hash| !held.contains(hash));
    let mut marks = vec![(detail.row.unit, UnitState::Undone { by })];
    marks.extend(restored);
    Ok(UnitRecord {
        unit: by,
        meta: UnitMeta {
            realm: detail.row.meta.realm.clone(),
            principal: detail.row.meta.principal.clone(),
            kind: UnitKind::Undo {
                target: detail.row.unit,
            },
            label: REVERSAL_LABEL.to_owned(),
            committed_at_ms,
        },
        writes: detail
            .images
            .iter()
            .map(|image| StagedWrite {
                plugin: image.plugin.clone(),
                key: image.key.clone(),
                expect: Expect::Value(image.after.as_ref().map(|entry| entry.value.clone())),
                after: image.before.as_ref().map(|entry| entry.value.clone()),
            })
            .collect(),
        blobs: moved.unpinned,
        notes: Vec::new(),
        unpinned,
        marks,
    })
}

/// What a person's history shows for a unit the kernel reversed because the
/// journal would not take its close.
pub(crate) const REVERSAL_LABEL: &str = "kernel.revert";

/// How many bytes of the label a plugin gives a unit its step keeps: what a
/// person is shown for it in their history, which a line of a list holds.
pub(crate) const LABEL_BYTES: usize = 128;

/// What the step the unit `ambient` becomes is shown as in a person's history:
/// the label a plugin gave a unit it began inside it, and the command's name
/// otherwise.
///
/// **Never for an undo or a redo**, whose labels say which they are: a
/// compensation an undo dispatches may begin a unit of its own, and its label
/// is not the undo's.
fn shown_as(ambient: &Ambient, command: &str) -> String {
    match (ambient.reversing(), ambient.label()) {
        (None, Some(label)) => label.to_owned(),
        _ => command.to_owned(),
    }
}

/// The blobs the unit `ambient` pins when it commits: each it made, and each
/// a redo pins again.
fn pinned_by(ambient: &Ambient) -> Vec<BlobCreated> {
    let mut pinned: Vec<BlobCreated> = ambient
        .made()
        .iter()
        .map(|made| BlobCreated {
            hash: made.hash.to_string(),
            size: made.size,
        })
        .collect();
    pinned.extend(
        ambient
            .reversing()
            .iter()
            .flat_map(|reversing| reversing.repinned.clone()),
    );
    pinned
}

/// Keeps a top-level dispatch's unit: its overlay committed to the store as
/// one transaction, then its close written to the journal.
///
/// **A close the journal answers with an error may have landed** — a
/// connection lost after the journal's own commit answers just so — so the
/// journal is asked how the unit closed before anything is reverted. Closed
/// committed, the unit is kept and the dispatch answered; closed otherwise or
/// not at all, the unit is reverted; and a journal that cannot say is answered
/// by the reversal too, because a store keeping what the journal may never
/// have recorded is the one outcome a unit exists to prevent.
///
/// # Errors
///
/// What the store refused the commit for (`commit_refused`), with the unit
/// then closed as aborted; and `unavailable`, with the journal's own code,
/// when the journal did not take the close — saying that the unit's reversal
/// was committed at once, or that the store refused it, which is logged as
/// `kernel.revert_failed`, and the unit stays committed in the store, held
/// from the pruner, until the next prune reverts it ([`reverted_again`]) or
/// the recovery on open does. Each names the compensations registered in the
/// unit, none of which runs (`not_run`).
pub(crate) async fn close<E: Environment>(
    shared: &Shared<E>,
    mut ambient: Ambient,
    principal: &Principal,
    label: &str,
) -> Result<(), Refusal> {
    let unit = ambient.unit();
    let overlay = ambient.overlay();
    let reversing = ambient.reversing();
    let record = UnitRecord {
        unit,
        meta: UnitMeta {
            realm: shared.realm.clone(),
            principal: principal.id().clone(),
            kind: reversing.map_or(UnitKind::Step, |reversing| reversing.kind),
            label: shown_as(&ambient, label),
            committed_at_ms: shared.clock.now_ms(),
        },
        writes: overlay.writes(),
        blobs: pinned_by(&ambient),
        notes: ambient.notes_kept(),
        // **Less what the unit made itself**, which its undo took off the list
        // once its compensations ran (`Ambient::holds_what_it_made`).
        unpinned: reversing
            .iter()
            .flat_map(|reversing| reversing.unpinned.iter().map(|(hash, _)| hash.clone()))
            .collect(),
        marks: reversing.map_or_else(Vec::new, |reversing| reversing.marks.clone()),
    };
    let quotas = quotas(shared.lowered(&overlay.plugins()));
    if let Err(refused) = shared.store.commit_held(&record, &quotas).await {
        let_go(shared, &ambient, Cause::Conflict).await;
        return Err(not_run(
            refused_commit(&refused, reversing.is_some()),
            &ambient,
        ));
    }
    let waits = ambient.waits;
    let Some((error, said)) = closed_in_the_journal(shared, unit, waits).await else {
        released(shared, unit).await;
        reversed_in_the_log(shared, reversing);
        kept(shared, &mut ambient);
        return Ok(());
    };
    let unsure = match said {
        Ok(Some(Outcome::UnitCommitted)) => {
            released(shared, unit).await;
            reversed_in_the_log(shared, reversing);
            kept(shared, &mut ambient);
            return Ok(());
        }
        Ok(_) => false,
        Err(_) => true,
    };
    let journal = if unsure {
        format!(
            "the journal answered the close of `{label}`'s unit with an error and could not \
             then say whether it took it"
        )
    } else {
        format!("the journal did not take the close of `{label}`'s unit")
    };
    let sentence = match revert(shared, unit).await {
        Ok(()) => {
            let cause = if unsure {
                Cause::JournalUnknown
            } else {
                Cause::Journal
            };
            released(shared, unit).await;
            abort(shared, unit, cause, waits).await;
            if unsure {
                format!(
                    "{journal}, so what it did was reverted at once, though the journal may \
                     record it as kept"
                )
            } else {
                format!("{journal}, so what it did was reverted at once")
            }
        }
        Err(refused) => {
            // **Held from the pruner** until a prune reverts it first: a unit
            // whose record the pruner took is one the recovery finds nothing
            // of, and closes with its writes left in the store.
            crate::dispatch::lock(&shared.left_committed).insert(unit);
            shared.log.record(&nc_log::events::KernelRevertFailed {
                unit,
                code: refused.code(),
            });
            format!(
                "{journal}, and the store refused the reversal of what it did, so that stays \
                 committed in the store, with the unit open in the journal, until the next \
                 prune or the recovery on open reverts it: {refused}"
            )
        }
    };
    Err(not_run(
        Refusal::new(RefusalKind::Unavailable, error.code(), sentence),
        &ambient,
    ))
}

/// The journal's close of the committed `unit`, waiting as `waits` says:
/// none when it took it, and otherwise what it refused with and what it then
/// said of how the unit closed.
///
/// **A close not answered in time cannot say either**: it may have landed or
/// not, and is reverted as one the journal cannot say it took.
async fn closed_in_the_journal<E: Environment>(
    shared: &Shared<E>,
    unit: Snowflake,
    waits: Waits,
) -> Option<(
    nc_journal::Error,
    Result<Option<Outcome>, nc_journal::Error>,
)> {
    let closed = waits
        .on(async {
            let mut journal = shared.journal.lock().await;
            let Err(error) = journal.close_unit(unit, Closing::Committed).await else {
                return Ok(None);
            };
            Ok(Some((
                error,
                journal.store().unit_closing(&shared.realm, unit).await,
            )))
        })
        .await;
    closed.unwrap_or_else(|unanswered| Some((unanswered.clone(), Err(unanswered))))
}

/// Releases what the unit `ambient` held until it was kept ([`Later::released`]).
fn kept<E: Environment>(shared: &Shared<E>, ambient: &mut Ambient) {
    core::mem::take(&mut ambient.later).released(shared);
}

/// Lets go of what `unit` holds from the pruner (`nc_local::LocalStore::
/// commit_held`): its close was heard, or it was reverted.
///
/// **A release the store refuses leaves the unit held**, which keeps its
/// record from the pruner and the blobs it took the pin off from the sweep —
/// both the way that keeps a person's data — until the recovery the next time
/// a kernel of its realm opens, which lets go of every unit of the realm the
/// journal records as closed.
async fn released<E: Environment>(shared: &Shared<E>, unit: Snowflake) {
    let _held_until_the_next_open = shared.store.release(unit).await;
}

/// Logs that a step was undone or redone, once the unit that did it is kept:
/// nothing, for a unit that reverses nothing.
fn reversed_in_the_log<E: Environment>(shared: &Shared<E>, reversing: Option<&Reversing>) {
    let Some(reversing) = reversing else {
        return;
    };
    shared.log.record(&nc_log::events::KernelStepReversed {
        unit: reversing.step,
        direction: if reversing.undoes() {
            nc_log::variant!("undo")
        } else {
            nc_log::variant!("redo")
        },
        compensations: reversing.compensations,
        beyond_reach: reversing.beyond_reach,
    });
}

/// Lets the unit `ambient` go.
///
/// Nothing it staged reaches the store, the bytes of the blobs it made that
/// nothing else holds are deleted, and the journal is told it was aborted.
pub(crate) async fn let_go<E: Environment>(shared: &Shared<E>, ambient: &Ambient, cause: Cause) {
    blobs::let_go(&*shared.store, &*shared.blobs, ambient.made()).await;
    abort(shared, ambient.unit(), cause, ambient.waits).await;
}

/// Lets a unit go: nothing it staged reaches the store, and the journal is
/// told it was aborted.
///
/// **A journal that does not take the close changes nothing here**: the store
/// never held the unit's writes, and a unit the journal leaves open is closed
/// by the recovery on open.
pub(crate) async fn abort<E: Environment>(
    shared: &Shared<E>,
    unit: Snowflake,
    cause: Cause,
    waits: Waits,
) {
    let _open_until_recovered = waits
        .on(async {
            shared
                .journal
                .lock()
                .await
                .close_unit(unit, Closing::Aborted)
                .await
        })
        .await;
    shared.log.record(&nc_log::events::KernelUnitAborted {
        unit,
        cause: cause.variant(),
    });
}

/// Commits the reversal of `unit`, which the store holds and the journal did
/// not close.
///
/// **Under the catalog's quotas, not a manifest's**, because a reversal leaves
/// each plugin holding what it held before the unit, and a quota lowered since
/// must not keep a person's data in a state the journal never recorded.
///
/// # Errors
///
/// What refused the reversal, with its own code: the store reading the unit
/// back or committing its reversal, the store answering that it holds no unit
/// it has just committed — [`LocalError::Unexpected`] — or the journal
/// minting no identifier for the reversal's own unit; and then the store
/// holds the unit as it committed.
async fn revert<E: Environment>(shared: &Shared<E>, unit: Snowflake) -> Result<(), Refusal> {
    let detail = shared
        .store
        .unit(unit)
        .await
        .map_err(|failed| store_failed(&failed))?
        .ok_or_else(|| {
            store_failed(&LocalError::Unexpected(format!(
                "the store holds no unit {unit}, which it had just committed"
            )))
        })?;
    let restored = restored(shared, &detail).await?;
    let held = still_held(shared, &detail, &restored).await?;
    let by = shared.journal.lock().await.mint_unit().map_err(|error| {
        Refusal::new(
            RefusalKind::Unavailable,
            error.code(),
            "the journal minted no identifier for the reversal's own unit".to_owned(),
        )
    })?;
    let mut record = reversal(&detail, by, shared.clock.now_ms(), restored, &held)
        .map_err(|failed| store_failed(&failed))?;
    // **A pin put back only on a blob the store still holds**: one whose row is
    // gone has no bytes behind it, and a reversal that put it back anyway would
    // insert a pinned row for bytes the sweep had deleted — a blob said to be
    // kept, which nothing would sweep again. Its step names a blob that is not
    // here, which `blobs.locate` then says.
    let mut still_here = Vec::with_capacity(record.blobs.len());
    for blob in record.blobs {
        let row = shared
            .store
            .blob(&blob.hash)
            .await
            .map_err(|failed| store_failed(&failed))?;
        if row.is_some() {
            still_here.push(blob);
        }
    }
    record.blobs = still_here;
    shared
        .store
        .commit(&record, &quotas(BTreeMap::new()))
        .await
        .map(drop)
        .map_err(|refused| commit_refused(&refused))
}

/// The blobs whose pin the reversal of `detail` leaves on: each it made that a
/// committed unit holds now besides it, and each a redo pinned again that one
/// holds besides the step the redo put back — the one `restored` marks undone
/// again.
///
/// **Asked again at the reversal, not taken from the unit's notes.** A unit
/// recorded only the blobs no unit held when it ran, and that answer is as old
/// as the unit: a reversal is the kernel's own repair, run at once or — by a
/// prune, or the recovery on open — after units committed since, and whatever
/// it runs after, a pin is taken off only when no other unit holds the blob.
/// **Asked of every pin, not of a redo's alone**: when a step kept since made
/// the same bytes again, the reversal of the unit left committed before it,
/// asked only of the pins a redo put back, would leave that step's blob to the
/// sweep.
///
/// # Errors
///
/// What the store refused the question with, and [`LocalError::Unexpected`]
/// for a note of a blob made or a pin put back that the kernel did not write;
/// and then nothing is reverted.
async fn still_held<E: Environment>(
    shared: &Shared<E>,
    detail: &UnitDetail,
    restored: &[(Snowflake, UnitState)],
) -> Result<Vec<String>, Refusal> {
    let made = blobs::unpinned_by(&detail.notes).map_err(|failed| store_failed(&failed))?;
    let mut asked: Vec<(String, Snowflake)> = made
        .into_iter()
        .map(|hash| (hash, detail.row.unit))
        .collect();
    let step = restored
        .iter()
        .find_map(|(unit, state)| matches!(state, UnitState::Undone { .. }).then_some(*unit));
    if let Some(step) = step {
        let moved =
            crate::history::pins_moved(&detail.notes).map_err(|failed| store_failed(&failed))?;
        asked.extend(moved.repinned.into_iter().map(|hash| (hash, step)));
    }
    let mut held = Vec::new();
    for (hash, besides) in asked {
        if crate::history::held_elsewhere(shared, &hash, besides)
            .await
            .map_err(|failed| store_failed(&failed))?
        {
            held.push(hash);
        }
    }
    Ok(held)
}

/// The units whose state the committed unit `detail` moved, each with the
/// state it held before: what reversing it puts back.
///
/// **Read from the kinds, which the store keeps typed**: an undo moved its
/// step to undone, and a redo moved its step back to committed and its undo to
/// undone — so reversing an undo commits the step again, and reversing a redo
/// undoes the step again by the same undo, which redo can then reach as
/// before.
///
/// # Errors
///
/// What the store refused the read with, and [`LocalError::Unexpected`], which
/// is `NC-DB-0007`, for a redo whose undo the store does not hold as an undo.
async fn restored<E: Environment>(
    shared: &Shared<E>,
    detail: &UnitDetail,
) -> Result<Vec<(Snowflake, UnitState)>, Refusal> {
    match detail.row.meta.kind {
        UnitKind::Step => Ok(Vec::new()),
        UnitKind::Undo { target } => Ok(vec![(target, UnitState::Committed)]),
        UnitKind::Redo { target: undo } => {
            let read = shared
                .store
                .unit(undo)
                .await
                .map_err(|failed| store_failed(&failed))?;
            match read.map(|undone| undone.row.meta.kind) {
                Some(UnitKind::Undo { target: step }) => Ok(vec![
                    (undo, UnitState::Committed),
                    (step, UnitState::Undone { by: undo }),
                ]),
                _ => Err(store_failed(&LocalError::Unexpected(format!(
                    "the store holds no undo {undo}, which the redo {} reversed",
                    detail.row.unit
                )))),
            }
        }
    }
}

/// Reverts again every unit this kernel left committed in the store and open
/// in the journal, and closes each as aborted once its reversal lands.
///
/// **What a prune asks first** (`crate::builtin`, `crate::pump`). A unit whose
/// close the journal did not take and whose reversal the store refused is held
/// in `Shared::left_committed`, and the pruner deletes the oldest units'
/// records whole — so one it took would be a unit the recovery on open finds
/// nothing of in the store, closing it as aborted with its writes left there,
/// and a redo whose undo it took one the recovery cannot reverse at all. Each
/// is held, as [`recover`] holds a unit it finds open, to the journal's own
/// answer first: one the journal says it closed — a close that landed and
/// whose answer was lost — is no longer held, and is not reverted.
///
/// A unit no longer held is closed in the journal as aborted, logged as
/// `kernel.unit_aborted`; a journal that does not take that close leaves it
/// open for the recovery on open, which finds its reversal in the store and
/// only closes it.
///
/// # Errors
///
/// The first refusal, with its code, and that unit and every one after it
/// stays held for the next prune: the journal could not say how the unit
/// closed, or the store could not read it or refused its reversal again —
/// which `kernel.revert_failed` logged when it was first refused.
pub(crate) async fn reverted_again<E: Environment>(shared: &Shared<E>) -> Result<(), Refusal> {
    let held: Vec<Snowflake> = crate::dispatch::lock(&shared.left_committed)
        .iter()
        .rev()
        .copied()
        .collect();
    for unit in held {
        let closing = shared
            .journal
            .lock()
            .await
            .store()
            .unit_closing(&shared.realm, unit)
            .await
            .map_err(|error| {
                Refusal::new(
                    RefusalKind::Unavailable,
                    error.code(),
                    format!(
                        "the journal could not say how the unit {unit} closed, which the store \
                         holds committed, so it is not reverted"
                    ),
                )
            })?;
        if closing.is_none() {
            let kept = shared
                .store
                .unit(unit)
                .await
                .map_err(|failed| store_failed(&failed))?;
            if kept.is_some_and(|detail| detail.row.state == UnitState::Committed) {
                revert(shared, unit).await.map_err(|refused| {
                    Refusal::new(
                        refused.kind(),
                        refused.code(),
                        format!(
                            "the unit {unit} is committed in the store and open in the journal, \
                             and the store refused its reversal again: {}",
                            refused.sentence()
                        ),
                    )
                })?;
            }
            abort(shared, unit, Cause::Journal, Waits::TOP).await;
        }
        released(shared, unit).await;
        crate::dispatch::lock(&shared.left_committed).remove(&unit);
    }
    Ok(())
}

/// Recovers every unit of this realm a stopped process left open in the
/// journal: what the store kept of it is reverted, and the unit is closed as
/// aborted.
///
/// **What the journal never closed was never kept** (`NOCTUA.md §11.5`). A
/// process stopped while a command ran leaves the unit open and nothing of it
/// in the store, which holds nothing of a unit before its commit; one stopped
/// between the store's commit and the journal's close, or one whose reversal
/// the store refused, leaves the unit committed in the store as well, and its
/// reversal is committed here as the close would have committed it
/// ([`revert`]): its keys put back, the pins it put on taken off and the
/// states it moved restored, a pin another committed unit holds now kept. A
/// unit the store holds reversed already is only closed. Each is logged as
/// `kernel.recovered` with `[NC-KERN-0016]`, saying whether anything was
/// reverted.
///
/// **Asked of the journal's store, then asked again of the journal.** The
/// store names this realm's units with an opening bracket and no closing one
/// (`nc_journal::Store::open_units`), from the columns that say so and never
/// from a read of the history — so a row nothing can decode hides no unit
/// opened after it — and each is held to the journal's own answer of how it
/// closed before anything is done, so a close that landed after the store was
/// asked is never overruled. A store that cannot name one of them — an opening
/// bracket whose unit is not spelled as this code writes one — refuses, and the
/// kernel does not open.
///
/// **What opening costs is the store's.** Every store compares four columns
/// of the entries it visits and decodes none of them, and which entries those
/// are is its own: the local journal's index `(realm, work_unit)` visits the
/// realm's entries inside units; PostgreSQL's journal indexes the realm and
/// the unit apart (`journal_entries_realm`, `journal_entries_work_unit`), and
/// its planner visits every entry of the realm or every entry inside a unit of
/// any realm, whichever it judges fewer; the store in memory visits every
/// entry it holds.
///
/// **Newest first.** A unit's reversal is held to the store holding what the
/// unit left, and a later unit over the same key left something else — which
/// the later unit's own reversal puts back, as it puts back the states it
/// moved. Oldest first, two units over one key would have the second reversal
/// refused as a conflict, and every later open refused the same way.
///
/// # Errors
///
/// The first refusal, with its code, and nothing after it is recovered: the
/// journal could not say which units are open or how one closed, or did not
/// take its close; or the store could not read a unit, or refused its reversal —
/// logged as `kernel.revert_failed` — and then that unit stays as it was.
pub(crate) async fn recover<E: Environment>(shared: &Shared<E>) -> Result<(), Refusal> {
    let open = shared
        .journal
        .lock()
        .await
        .store()
        .open_units(&shared.realm)
        .await
        .map_err(|error| {
            Refusal::new(
                RefusalKind::Unavailable,
                error.code(),
                "the journal could not say which units a stopped process left open in it"
                    .to_owned(),
            )
        })?;
    for unit in open.into_iter().rev() {
        let closing = shared
            .journal
            .lock()
            .await
            .store()
            .unit_closing(&shared.realm, unit)
            .await
            .map_err(|error| {
                Refusal::new(
                    RefusalKind::Unavailable,
                    error.code(),
                    format!("the journal could not say how the unit {unit} closed"),
                )
            })?;
        if closing.is_some() {
            continue;
        }
        let held = shared
            .store
            .unit(unit)
            .await
            .map_err(|failed| store_failed(&failed))?;
        // **Only what this realm kept is reverted**: the realms of one machine
        // share the file, and a unit of another realm under the same number —
        // two journals that did not share one minter (`nc_journal::Journal::
        // sharing`) — is not what this one left open, and reverting it would
        // undo another realm's kept unit.
        let held = held.filter(|detail| detail.row.meta.realm == shared.realm);
        let reverted = held.is_some_and(|detail| detail.row.state == UnitState::Committed);
        if reverted && let Err(refused) = revert(shared, unit).await {
            shared.log.record(&nc_log::events::KernelRevertFailed {
                unit,
                code: refused.code(),
            });
            return Err(Refusal::new(
                refused.kind(),
                refused.code(),
                format!(
                    "the store refused the reversal of the unit {unit}, which a stopped process \
                     left committed in the store and open in the journal: {}",
                    refused.sentence()
                ),
            ));
        }
        let closed = shared
            .journal
            .lock()
            .await
            .close_unit(unit, Closing::Aborted)
            .await;
        if let Err(error) = closed
            && !matches!(error, nc_journal::Error::UnitClosed { .. })
        {
            return Err(Refusal::new(
                RefusalKind::Unavailable,
                error.code(),
                format!(
                    "the journal did not take the close of the unit {unit}, which a stopped \
                     process left open in it"
                ),
            ));
        }
        shared.log.record(&nc_log::events::KernelRecovered {
            unit,
            reverted,
            code: ErrorCode::KernelRecoveredUnit,
        });
    }
    let_go_of_what_closed(shared).await
}

/// Lets go of the holds of every unit of this realm the journal records as
/// closed: one [`recover`] closed just now, one whose close the kernel heard
/// and whose release the store then refused, or a process stopped between the
/// two.
///
/// **A unit the journal holds open stays held**, whether or not the recovery
/// reverted it.
///
/// # Errors
///
/// The first refusal, with its code: the store could not list its holders or
/// read one, or the journal could not say how one closed.
async fn let_go_of_what_closed<E: Environment>(shared: &Shared<E>) -> Result<(), Refusal> {
    let holders = shared
        .store
        .holders()
        .await
        .map_err(|failed| store_failed(&failed))?;
    for holder in holders {
        let detail = shared
            .store
            .unit(holder)
            .await
            .map_err(|failed| store_failed(&failed))?;
        if detail.is_none_or(|detail| detail.row.meta.realm != shared.realm) {
            continue;
        }
        let closing = shared
            .journal
            .lock()
            .await
            .store()
            .unit_closing(&shared.realm, holder)
            .await
            .map_err(|error| {
                Refusal::new(
                    RefusalKind::Unavailable,
                    error.code(),
                    format!("the journal could not say how the unit {holder} closed"),
                )
            })?;
        if closing.is_some() {
            released(shared, holder).await;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use nc_catalog::{ErrorCode, KERNEL};
    use nc_ids::Snowflake;
    use nc_local::{
        CommitError, Entry, Expect, Image, LocalError, Page, Revision, UnitDetail, UnitKind,
        UnitMeta, UnitRow, UnitState,
    };

    use super::{
        Ambient, InUnit, LABEL_BYTES, Overlay, Reversing, blobs_per_unit, commit_refused, expected,
        merged, notes_per_step, quotas, refused_commit, reversal, revision_shaped, writes_per_unit,
    };
    use crate::PluginSlot;
    use crate::blobs::{Hash, Made};
    use crate::dispatch::{Cause, Refusal, RefusalKind};
    use crate::history::Note;
    use crate::timers::Timer;

    /// The bounds a unit's commit is held to are the catalog's, each read by
    /// the name of its own table, and a plugin's lowered quota is its own.
    ///
    /// **Read from the catalog here, and not through the helpers they are built
    /// with**: with the bound on the blobs the store keeps doubled, a test
    /// taking its expectation from the helper it tests would stay green, and
    /// this one, reading the catalog, fails.
    #[test]
    fn a_units_bounds_are_the_catalogs_each_by_its_own_table() {
        let catalog = |table: &str| {
            nc_catalog::TABLES
                .iter()
                .find(|declared| declared.name == table)
                .and_then(|declared| declared.max_rows)
                .unwrap_or_else(|| panic!("`[tables.{table}]` declares a bound"))
        };
        let lowered = std::collections::BTreeMap::from([("noctua:reference".to_owned(), 3)]);
        let held = quotas(lowered.clone());
        assert_eq!(
            (
                held.entries_per_plugin,
                held.blobs,
                held.writes_per_unit,
                held.notes_per_unit,
            ),
            (
                catalog("store_entries"),
                catalog("blobs"),
                catalog("store_writes"),
                catalog("store_notes"),
            )
        );
        assert_eq!(held.lowered, lowered);
        assert_eq!(writes_per_unit(), catalog("store_writes"));
        assert_eq!(blobs_per_unit(), catalog("store_notes"));
    }

    /// A timer the plugin in `owner` set, for `ana`.
    fn a_timer(owner: usize) -> Timer {
        let pdp = nc_authz::Pdp::embedded("family").expect("the family pack is carried");
        Timer {
            owner: PluginSlot(owner),
            tag: "tag".to_owned(),
            due_ms: 10,
            every_ms: None,
            principal: nc_authz::MemoryDirectory::from_world(
                include_str!("../../../policy/world.json"),
                &pdp,
            )
            .expect("the world loads")
            .principal(nc_authz::PrincipalKind::User, "ana")
            .expect("ana is a user"),
            authority: crate::dispatch::Authority {
                command: "reference.timer.set".to_owned(),
                action: "Write".to_owned(),
                resource: nc_authz::EntityRef {
                    type_name: "Content".to_owned(),
                    id: "photo".to_owned(),
                },
            },
            may_change: true,
        }
    }

    #[test]
    fn a_unit_holds_the_timers_set_in_it_by_their_owner_and_each_cancel_once() {
        let (unit, _) = units();
        let mut ambient = Ambient::new(unit);
        ambient.later_mut().set_timer(1, a_timer(0));
        ambient.later_mut().set_timer(2, a_timer(1));
        ambient.later_mut().set_timer(3, a_timer(0));
        assert_eq!(ambient.later().timers_of(PluginSlot(0)), 2);
        assert_eq!(ambient.later().timers_of(PluginSlot(1)), 1);
        assert!(
            !ambient.later_mut().unset_timer(2, PluginSlot(0)),
            "another plugin's timer is not this one's to take back"
        );
        assert!(ambient.later_mut().unset_timer(1, PluginSlot(0)));
        assert!(
            !ambient.later_mut().unset_timer(1, PluginSlot(0)),
            "taken back already"
        );
        assert_eq!(ambient.later().timers_of(PluginSlot(0)), 1);
        assert!(ambient.later_mut().cancel_timer(7));
        assert!(
            !ambient.later_mut().cancel_timer(7),
            "a cancel is held once"
        );
        assert_eq!(ambient.later().cancelled, [7]);
    }

    #[test]
    fn what_a_unit_holds_for_later_is_one_of_its_changes() {
        let (unit, _) = units();
        let mut ambient = Ambient::new(unit);
        let timer = a_timer(0);
        ambient.later_mut().publish(crate::events::Published {
            topic: "reference.noted".to_owned(),
            data: "{}".to_owned(),
            publisher: PluginSlot(0),
            principal: timer.principal.clone(),
            authority: timer.authority.clone(),
        });
        assert_eq!(ambient.changes(), 1, "an event held is a change");
        ambient.later_mut().set_timer(1, timer);
        assert_eq!(ambient.changes(), 2, "a timer held is a change");
        assert!(!ambient.later_mut().unset_timer(1, PluginSlot(1)));
        assert_eq!(ambient.changes(), 2, "nothing was taken back");
        assert!(ambient.later_mut().unset_timer(1, PluginSlot(0)));
        assert_eq!(
            ambient.changes(),
            3,
            "a timer taken back is a change, and the count does not go back"
        );
        assert!(ambient.later_mut().cancel_timer(7));
        assert_eq!(ambient.changes(), 4, "a cancel held is a change");
        assert!(!ambient.later_mut().cancel_timer(7));
        assert_eq!(
            ambient.changes(),
            4,
            "a cancel held already is not held again"
        );
    }

    /// Two units, from one minter.
    fn units() -> (Snowflake, Snowflake) {
        let mut minter = nc_ids::Minter::new(
            nc_ids::MachineFlake::from_value(1).expect("a flake"),
            nc_ids::SystemClock,
        );
        (
            minter.mint().expect("a unit"),
            minter.mint().expect("a unit"),
        )
    }

    /// An entry holding `value`, written by `unit` as its `n`th write.
    fn entry(value: &str, unit: Snowflake, n: u32) -> Entry {
        Entry {
            value: value.to_owned(),
            revision: Revision { unit, n },
        }
    }

    #[test]
    fn a_staged_key_is_read_back_with_the_revision_its_write_commits_with() {
        let (unit, earlier) = units();
        let mut overlay = Overlay::default();
        assert_eq!(
            overlay.read(unit, "p", "a"),
            InUnit::Untouched,
            "nothing staged"
        );
        let first = overlay
            .stage(unit, "p", "a", None, Some("1".to_owned()))
            .expect("within the bound");
        let second = overlay
            .stage(
                unit,
                "p",
                "b",
                Some(&entry("2", earlier, 0)),
                Some("3".to_owned()),
            )
            .expect("within the bound");
        assert_eq!(first, Some(Revision { unit, n: 0 }));
        assert_eq!(second, Some(Revision { unit, n: 1 }));
        assert_eq!(
            overlay.read(unit, "p", "b"),
            InUnit::Written(entry("3", unit, 1))
        );
        assert_eq!(
            overlay.read(unit, "q", "b"),
            InUnit::Untouched,
            "another plugin's key"
        );
        let deleted = overlay
            .stage(unit, "p", "a", None, None)
            .expect("within the bound");
        assert_eq!(deleted, None, "a delete has no revision");
        assert_eq!(overlay.read(unit, "p", "a"), InUnit::Deleted);
        assert_eq!(overlay.changes(), 3);
        let writes = overlay.writes();
        assert_eq!(
            writes
                .iter()
                .map(|write| (
                    write.key.as_str(),
                    write.expect.clone(),
                    write.after.clone()
                ))
                .collect::<Vec<_>>(),
            [
                ("a", Expect::Absent, None),
                (
                    "b",
                    Expect::Revision(Revision {
                        unit: earlier,
                        n: 0
                    }),
                    Some("3".to_owned())
                ),
            ],
            "each key once, in the order first written, expecting what the store held then"
        );
        assert_eq!(overlay.plugins(), BTreeSet::from(["p".to_owned()]));
    }

    #[test]
    fn a_unit_refuses_a_key_past_the_writes_one_unit_records() {
        let (unit, _) = units();
        let mut overlay = Overlay::default();
        let bound = writes_per_unit();
        for at in 0..bound {
            overlay
                .stage(unit, "p", &format!("k{at}"), None, Some("1".to_owned()))
                .expect("within the bound");
        }
        let refused = overlay
            .stage(unit, "p", "one-more", None, Some("1".to_owned()))
            .expect_err("past the bound");
        assert_eq!(refused.code(), ErrorCode::PluginBoundReached);
        assert_eq!(refused.kind(), RefusalKind::Unavailable);
        assert!(
            overlay
                .stage(unit, "p", "k0", None, Some("2".to_owned()))
                .is_ok(),
            "a key already staged is not one more"
        );
    }

    /// A unit stages up to `unit_bytes` of keys and values and not a byte
    /// more, a value written over gives back what it held, and a write refused
    /// stages nothing.
    ///
    /// Bounded by the count of keys alone, one unit could make the kernel hold
    /// 80 MiB for a plugin whose own memory is capped at 64 MiB: this is the
    /// bound in bytes.
    #[test]
    fn a_unit_stages_no_more_bytes_than_the_catalog_allows() {
        let (unit, _) = units();
        let bound = usize::try_from(KERNEL.unit_bytes).expect("fits");
        let mut overlay = Overlay::default();
        overlay
            .stage(unit, "p", "k", None, Some("v".repeat(bound - 1)))
            .expect("a key and its value of exactly the bound");
        assert_eq!(overlay.bytes, KERNEL.unit_bytes);
        let refused = overlay
            .stage(unit, "p", "j", None, Some(String::new()))
            .expect_err("one byte past the bound: the key alone");
        assert_eq!(refused.code(), ErrorCode::PluginBoundReached);
        assert_eq!(refused.kind(), RefusalKind::Unavailable);
        assert_eq!(overlay.read(unit, "p", "j"), InUnit::Untouched);
        assert_eq!(overlay.changes(), 1, "a refused write staged something");
        overlay
            .stage(unit, "p", "k", None, Some("v".repeat(bound - 2)))
            .expect("a value written over gives back the room it took");
        overlay
            .stage(unit, "p", "j", None, Some(String::new()))
            .expect("the byte given back is room for the key");
        assert_eq!(overlay.bytes, KERNEL.unit_bytes);
        overlay
            .stage(unit, "p", "k", None, None)
            .expect("a delete gives back its value");
        assert_eq!(overlay.bytes, 2, "the two keys, and no value");
    }

    /// A key a unit first writes counts what it held before against the
    /// unit's bytes, and keeps it counted, so the undo of a unit kept fits.
    ///
    /// With its key names alone counted, a step that deleted twenty values of
    /// about a megabyte would be kept, and its undo refused at every attempt:
    /// the before images an undo stages would be past the bound the step was
    /// never held to, so undo would never reach past it. An undo or a redo
    /// counts what each key holds as it puts it back, the same images the other
    /// way round.
    #[test]
    fn a_unit_counts_what_each_key_held_before_it_so_that_its_undo_fits() {
        let (unit, earlier) = units();
        let value = "v".repeat(1_000_000);
        let held = entry(&value, earlier, 0);
        let one = u64::try_from(value.len() + "k00".len()).expect("fits");
        let fits = KERNEL.unit_bytes / one;
        assert!(fits > 1, "the bound holds more than one such key");
        let mut step = Overlay::default();
        for n in 0..fits {
            step.stage(unit, "p", &format!("k{n:02}"), Some(&held), None)
                .expect("a delete within the bound");
        }
        let refused = step
            .stage(unit, "p", &format!("k{fits:02}"), Some(&held), None)
            .expect_err("one delete more than its undo could stage");
        assert_eq!(refused.code(), ErrorCode::PluginBoundReached);
        assert_eq!(refused.kind(), RefusalKind::Unavailable);
        let staged = usize::try_from(fits).expect("fits");
        assert_eq!(
            step.writes().len(),
            staged,
            "a refused delete staged nothing"
        );
        let before = step.bytes;
        step.stage(unit, "p", "k00", None, Some("1".to_owned()))
            .expect("a key staged already is no new key");
        assert_eq!(
            step.bytes,
            before + 1,
            "a key written again keeps what it held before counted"
        );

        let mut undo = Overlay::default();
        for write in step.writes() {
            undo.restore(
                unit,
                &write.plugin,
                &write.key,
                write.after.clone(),
                Some(value.clone()),
            )
            .expect("the undo of a step kept fits in a unit");
        }
        assert_eq!(
            undo.bytes, step.bytes,
            "the same images, the other way round"
        );

        let mut redo = Overlay::default();
        for n in 0..fits {
            redo.restore(unit, "p", &format!("k{n:02}"), Some(value.clone()), None)
                .expect("within the bound");
        }
        let refused = redo
            .restore(unit, "p", &format!("k{fits:02}"), Some(value.clone()), None)
            .expect_err("what a key holds as it is put back is counted");
        assert_eq!(refused.code(), ErrorCode::PluginBoundReached);
    }

    #[test]
    fn a_unit_is_doomed_for_the_first_cause_only() {
        let (unit, _) = units();
        let mut ambient = Ambient::new(unit);
        assert!(ambient.doomed().is_none());
        ambient.doom(crate::dispatch::Refusal::new(
            RefusalKind::Internal,
            ErrorCode::PluginTrapped,
            "first".to_owned(),
        ));
        ambient.doom(crate::dispatch::Refusal::new(
            RefusalKind::Internal,
            ErrorCode::PluginMalformedResult,
            "second".to_owned(),
        ));
        assert_eq!(
            ambient.doomed().map(crate::dispatch::Refusal::code),
            Some(ErrorCode::PluginTrapped)
        );
        assert_eq!(ambient.unit(), unit);
    }

    #[test]
    fn a_revision_is_judged_by_its_shape_and_never_decoded() {
        let (unit, _) = units();
        assert!(revision_shaped(&Revision { unit, n: 7 }.to_string()));
        for refused in [
            "",
            ".",
            "7",
            "ABC.",
            ".7",
            "abc.7",
            "ABCI.7",
            "AB.7x",
            "ABCDEFGHJKMNPQ.7",
            "AB.12345678901",
        ] {
            assert!(!revision_shaped(refused), "{refused:?}");
        }
    }

    #[test]
    fn a_write_lands_only_over_what_it_expected() {
        let (unit, _) = units();
        let held = entry("1", unit, 2);
        let named = held.revision.to_string();
        assert!(expected("k", None, None, true).is_ok(), "a key created");
        assert!(expected("k", Some(&held), None, false).is_ok(), "deleted");
        assert!(expected("k", Some(&held), Some(&named), true).is_ok());
        let conflicts = [
            expected("k", Some(&held), None, true),
            expected("k", None, Some(&named), true),
            expected(
                "k",
                Some(&held),
                Some(&Revision { unit, n: 3 }.to_string()),
                true,
            ),
        ];
        for refused in conflicts {
            let refused = refused.expect_err("a conflict");
            assert_eq!(refused.kind(), RefusalKind::Conflict, "{refused}");
            assert_eq!(refused.code(), ErrorCode::PluginStoreConflict);
        }
        // **Not the conflict's code, `[NC-PLUG-0018]`**, whose description —
        // what the write expected had changed — is untrue of a text that is no
        // revision, and whose message tells a person to try again.
        let malformed =
            expected("k", Some(&held), Some("yesterday"), true).expect_err("not a revision");
        assert_eq!(
            (malformed.kind(), malformed.code()),
            (RefusalKind::Invalid, ErrorCode::PluginNotARevision)
        );
        let long = "x".repeat(10_000);
        assert!(
            expected("k", None, Some(&long), true)
                .expect_err("not a revision")
                .to_string()
                .len()
                < 200,
            "text a plugin named is cut before it is repeated"
        );
    }

    #[test]
    fn a_page_shows_what_the_unit_holds_over_what_is_committed() {
        let page = |keys: &[&str], next: Option<&str>| Page {
            keys: keys.iter().map(|key| (*key).to_owned()).collect(),
            next: next.map(str::to_owned),
        };
        let set = |keys: &[&str]| keys.iter().map(|key| (*key).to_owned()).collect();
        // The store holds a, c and e and nothing after; the unit added b and f
        // and deleted c.
        let whole = merged(
            page(&["a", "c", "e"], None),
            set(&["b", "f"]),
            &set(&["c"]),
            10,
        );
        assert_eq!(whole, page(&["a", "b", "e", "f"], None));
        let cut = merged(
            page(&["a", "c", "e"], None),
            set(&["b", "f"]),
            &set(&["c"]),
            2,
        );
        assert_eq!(
            cut,
            page(&["a", "b"], Some("b")),
            "a page of two says more follow"
        );
        // The store holds more after e: f waits, because a key between e and
        // f may be committed and was not read.
        let held_back = merged(
            page(&["a", "c", "e"], Some("e")),
            set(&["b", "f"]),
            &set(&[]),
            10,
        );
        assert_eq!(held_back, page(&["a", "b", "c", "e"], Some("e")));
        assert_eq!(
            merged(page(&[], None), set(&[]), &set(&[]), 0),
            page(&[], None),
            "a limit of zero over nothing is an empty last page"
        );
    }

    #[test]
    fn every_refusal_of_a_commit_carries_the_code_of_what_refused_it() {
        for (error, kind, code) in [
            (
                CommitError::Conflict {
                    plugin: "p".to_owned(),
                    key: "k".to_owned(),
                },
                RefusalKind::Conflict,
                ErrorCode::PluginStoreConflict,
            ),
            (
                CommitError::Quota {
                    plugin: "p".to_owned(),
                },
                RefusalKind::Unavailable,
                ErrorCode::PluginBoundReached,
            ),
            (
                CommitError::TooManyBlobs,
                RefusalKind::Unavailable,
                ErrorCode::PluginBoundReached,
            ),
            (
                CommitError::Invalid("a key twice".to_owned()),
                RefusalKind::Internal,
                ErrorCode::KernelUnitAborted,
            ),
            (
                CommitError::Store(LocalError::Unavailable("busy".to_owned())),
                RefusalKind::Unavailable,
                LocalError::Unavailable(String::new()).code(),
            ),
            (
                CommitError::Store(LocalError::Unexpected("a row".to_owned())),
                RefusalKind::Internal,
                LocalError::Unexpected(String::new()).code(),
            ),
        ] {
            let refused = commit_refused(&error);
            assert_eq!((refused.kind(), refused.code()), (kind, code), "{error:?}");
        }
    }

    #[test]
    fn a_reversal_puts_every_key_back_and_marks_what_it_reversed() {
        let (step, by) = units();
        let realm = nc_ids::OpaqueId::new("home").expect("an id"); // allow-logging: OpaqueId::new of a literal this test writes
        let detail = UnitDetail {
            row: UnitRow {
                unit: step,
                meta: UnitMeta {
                    realm: realm.clone(),
                    principal: realm,
                    kind: UnitKind::Step,
                    label: "reference.note.put".to_owned(),
                    committed_at_ms: 1,
                },
                state: UnitState::Committed,
            },
            images: vec![
                Image {
                    plugin: "p".to_owned(),
                    key: "made".to_owned(),
                    before: None,
                    after: Some(entry("1", step, 0)),
                },
                Image {
                    plugin: "p".to_owned(),
                    key: "changed".to_owned(),
                    before: Some(entry("0", by, 3)),
                    after: Some(entry("2", step, 1)),
                },
            ],
            notes: Vec::new(),
        };
        let reversed =
            reversal(&detail, by, 9, Vec::new(), &[]).expect("the notes are the kernel's");
        assert_eq!(reversed.unit, by);
        assert_eq!(reversed.meta.kind, UnitKind::Undo { target: step });
        // The literal a person's history shows, and not the constant: a test
        // reading the constant back restates what it is meant to hold.
        assert_eq!(reversed.meta.label, "kernel.revert");
        assert_eq!(reversed.meta.committed_at_ms, 9);
        assert_eq!(reversed.marks, [(step, UnitState::Undone { by })]);
        assert_eq!(
            reversed
                .writes
                .iter()
                .map(|write| (
                    write.key.as_str(),
                    write.expect.clone(),
                    write.after.clone()
                ))
                .collect::<Vec<_>>(),
            [
                ("made", Expect::Value(Some("1".to_owned())), None),
                (
                    "changed",
                    Expect::Value(Some("2".to_owned())),
                    Some("0".to_owned())
                ),
            ]
        );
    }

    /// A refusal, for a test that dooms a unit.
    fn refusal(sentence: &str) -> Refusal {
        Refusal::new(
            RefusalKind::Internal,
            ErrorCode::KernelUnitAborted,
            sentence.to_owned(),
        )
    }

    #[test]
    fn a_unit_is_doomed_for_the_cause_first_given_and_a_failure_inside_it_is_nested() {
        let (unit, _) = units();
        let mut ambient = Ambient::new(unit);
        assert_eq!(ambient.doomed_for(), None);
        ambient.doom_for(refusal("dropped"), Cause::Dropped);
        ambient.doom(refusal("a nested failure"));
        assert_eq!(ambient.doomed_for(), Some(Cause::Dropped));
        assert_eq!(ambient.doomed().map(Refusal::sentence), Some("dropped"));
        let mut failed = Ambient::new(unit);
        failed.doom(refusal("a nested failure"));
        assert_eq!(failed.doomed_for(), Some(Cause::Nested));
    }

    /// An action allowed is noted once however often it ran, a compensation
    /// in the order registered, and every note a unit keeps takes room from
    /// one bound.
    #[test]
    fn a_unit_notes_each_action_once_each_compensation_in_order_and_no_more_than_its_bound() {
        let (unit, _) = units();
        let mut ambient = Ambient::new(unit);
        ambient.authorized("a".to_owned()).expect("room");
        ambient
            .authorized("a".to_owned())
            .expect("the same note again");
        ambient.compensate("second".to_owned()).expect("room");
        ambient.compensate("first".to_owned()).expect("room");
        ambient.left_the_machine("sent".to_owned()).expect("room");
        assert_eq!(
            ambient.notes_kept(),
            ["sent", "a", "second", "first"],
            "what left, the actions allowed, and the compensations in the order registered"
        );
        let bound = usize::try_from(notes_per_step()).expect("fits");
        for n in ambient.notes_kept().len()..bound {
            ambient
                .authorized(format!("action {n}"))
                .expect("within the bound");
        }
        for refused in [
            ambient.authorized("one more".to_owned()),
            ambient.compensate("one more".to_owned()),
            ambient.left_the_machine("one more".to_owned()),
        ] {
            let refused = refused.expect_err("past the bound");
            assert_eq!(refused.code(), ErrorCode::PluginBoundReached, "{refused}");
        }
        assert!(
            ambient.authorized("a".to_owned()).is_ok(),
            "an action noted already takes no more room"
        );
        assert!(
            ambient.room_for_a_blob().is_err(),
            "a blob shares the bound"
        );
    }

    /// A step holding as many notes as one may: the action `kept` first, as a
    /// command that ran was allowed it, then as many compensations as a unit
    /// registers, then other actions allowed.
    fn filled(kept: &str) -> Ambient {
        let (unit, _) = units();
        let mut ambient = Ambient::new(unit);
        ambient.ran(kept.to_owned());
        let bound = usize::try_from(notes_per_step()).expect("fits");
        for n in 1..bound {
            if n <= super::compensations() {
                ambient
                    .compensate(format!("compensation {n}"))
                    .expect("within the bound");
            } else {
                ambient
                    .authorized(format!("action {n}"))
                    .expect("within the bound");
            }
        }
        ambient
    }

    /// A unit registers no more compensations than its undo can dispatch: one
    /// fewer than a call tree may, the undo's own dispatch being the first.
    ///
    /// **Not by the notes alone**: bounded only by the notes a unit keeps, ten
    /// times more, a step could register more than any undo of it could run —
    /// refused at the last one every time it was asked, which would leave undo
    /// stuck at that step for good.
    #[test]
    fn a_unit_registers_no_more_compensations_than_its_undo_can_dispatch() {
        let (unit, _) = units();
        let mut ambient = Ambient::new(unit);
        let most = usize::try_from(KERNEL.dispatches_per_tree).expect("fits") - 1;
        assert!(
            most < usize::try_from(blobs_per_unit()).expect("fits"),
            "the notes' bound refuses first, and this would hold nothing"
        );
        for n in 0..most {
            ambient
                .compensate(format!("compensation {n}"))
                .expect("as many as an undo dispatches");
        }
        let refused = ambient
            .compensate("one more".to_owned())
            .expect_err("one more than an undo's call tree may dispatch");
        assert_eq!(
            (refused.kind(), refused.code()),
            (RefusalKind::Unavailable, ErrorCode::PluginBoundReached),
            "{refused}"
        );
        assert_eq!(
            ambient.notes_kept().len(),
            most,
            "the one refused is not kept"
        );
    }

    /// Inside one unit a key's revision names the key, not each value it
    /// takes: every write of it there is handed the one revision it commits
    /// with, so a write naming it lands over whatever the unit staged since —
    /// and in another unit the same position is another revision.
    ///
    /// **The narrower claim, pinned**: the store's "a revision is never reused"
    /// does not cover a key written twice in one unit — a call tree is one
    /// person's, run one command at a time, and what a revision tells apart is
    /// another tree's write.
    #[test]
    fn inside_a_unit_one_revision_covers_every_value_a_key_takes() {
        let (unit, other) = units();
        let mut overlay = Overlay::default();
        let first = overlay
            .stage(unit, "p", "a", None, Some("1".to_owned()))
            .expect("within the bound")
            .expect("a revision");
        let second = overlay
            .stage(unit, "p", "a", None, Some("2".to_owned()))
            .expect("within the bound")
            .expect("a revision");
        assert_eq!(first, second, "one key, one unit, one revision");
        let InUnit::Written(held) = overlay.read(unit, "p", "a") else {
            panic!("the key is written in the unit");
        };
        assert_eq!(held.value, "2");
        assert!(expected("a", Some(&held), Some(&first.to_string()), false).is_ok());
        let elsewhere = Revision {
            unit: other,
            n: first.n,
        };
        assert!(
            expected("a", Some(&held), Some(&elsewhere.to_string()), false).is_err(),
            "the same position in another unit is another revision"
        );
    }

    /// A command that ran, allowed an action its unit has no room to note,
    /// dooms the unit with the bound's code, and one noted already does not.
    ///
    /// The doom is held here: let go of, a unit whose notes a plugin had filled
    /// with compensations would be kept without the note of what its command
    /// was allowed, so an undo of it would authorize nothing again before
    /// putting its writes back.
    #[test]
    fn a_unit_with_no_room_to_note_what_a_command_was_allowed_is_doomed() {
        let mut ambient = filled("allowed");
        assert!(ambient.doomed().is_none(), "a unit within its bound");
        assert_eq!(
            ambient.notes_kept().first().map(String::as_str),
            Some("allowed")
        );
        ambient.ran("allowed".to_owned());
        assert!(
            ambient.doomed().is_none(),
            "an action noted already takes no room"
        );
        ambient.ran("allowed elsewhere".to_owned());
        let doomed = ambient.doomed().expect("doomed at the bound");
        assert_eq!(doomed.code(), ErrorCode::PluginBoundReached, "{doomed}");
        assert_eq!(doomed.kind(), RefusalKind::Unavailable);
        assert_eq!(ambient.doomed_for(), Some(Cause::Nested));
        assert!(
            !ambient
                .notes_kept()
                .contains(&"allowed elsewhere".to_owned()),
            "a note past the bound is kept"
        );
    }

    /// A command that leaves the machine is noted before it runs, and refused
    /// by a unit with no room left for the note, which then notes nothing of
    /// it; a command that does not leave is never noted.
    ///
    /// No dispatch of this build reaches the refusal — such a command runs only
    /// at the top of its tree, its unit empty — so this is where it is held.
    #[test]
    fn a_command_that_leaves_the_machine_is_refused_by_a_unit_with_no_room_to_note_it() {
        let listed = crate::registry::Registry::from_catalog().listed();
        let of = |name: &str| {
            listed
                .iter()
                .find(|command| command.name == name)
                .cloned()
                .unwrap_or_else(|| panic!("the catalog declares `{name}`"))
        };
        let (unit, _) = units();
        let mut ambient = Ambient::new(unit);
        for staying in ["journal.verify", "kernel.undo", "kernel.redo"] {
            ambient
                .leaving(&of(staying))
                .unwrap_or_else(|refused| panic!("`{staying}`: {refused}"));
        }
        assert!(ambient.left().is_empty(), "{:?}", ambient.left());
        ambient
            .leaving(&of("limits.prune"))
            .expect("room for its note");
        assert_eq!(ambient.left(), [crate::history::left_by("limits.prune")]);

        let mut full = filled("allowed");
        let refused = full
            .leaving(&of("limits.prune"))
            .expect_err("no room for its note");
        assert_eq!(refused.code(), ErrorCode::PluginBoundReached, "{refused}");
        assert_eq!(refused.kind(), RefusalKind::Unavailable);
        let got = full.left();
        assert!(got.is_empty(), "a command refused is noted: {got:?}");
        full.leaving(&of("journal.verify"))
            .expect("a command that does not leave needs no room");
    }

    /// What an undo or a redo did is noted with it: each pin it moved, and
    /// how many compensations it dispatched.
    #[test]
    fn a_reversing_unit_notes_the_pins_it_moved_and_its_compensations() {
        let (unit, step) = units();
        let mut ambient = Ambient::new(unit);
        let hash: String = core::iter::repeat_n('a', 64).collect();
        ambient.reverse(Reversing {
            kind: UnitKind::Undo { target: step },
            step,
            marks: vec![(step, UnitState::Undone { by: unit })],
            unpinned: vec![(hash.clone(), 3)],
            repinned: vec![nc_local::BlobCreated {
                hash: hash.clone(),
                size: 3,
            }],
            writes: 1,
            compensations: 2,
            beyond_reach: 1,
        });
        assert_eq!(
            ambient.notes_kept(),
            [
                Note::BlobUnpinned {
                    hash: hash.clone(),
                    size: 3
                }
                .written(),
                Note::BlobRepinned { hash }.written(),
                Note::CompensationsRan(2).written(),
            ]
        );
        assert!(ambient.reversing().is_some_and(Reversing::undoes));
    }

    /// The undo of the fullest step the kernel keeps keeps no more notes than
    /// the store takes of one unit: whatever number of blobs a step with a
    /// compensation makes, through the kernel's own checks, the step either is
    /// not kept or its undo — the pins it takes off, that its compensation ran,
    /// the compensation's action and its own — fits.
    ///
    /// **What it holds against**: a step kept at the bound whose undo records
    /// one note more, which the store would refuse every time, its
    /// compensations dispatched again before each refusal.
    #[test]
    fn the_undo_of_every_step_the_kernel_keeps_notes_no_more_than_the_store_takes() {
        let (unit, step) = units();
        let bound = blobs_per_unit();
        let made = |n: u64| Hash::of(&n.to_le_bytes());
        let mut kept_any = false;
        for blobs in bound.saturating_sub(4)..=bound {
            let mut kept = Ambient::new(step);
            kept.compensate("a compensation".to_owned()).expect("room");
            let mut made_all = true;
            for n in 0..blobs {
                if kept.room_for_a_blob().is_err() {
                    made_all = false;
                    break;
                }
                kept.make(Made {
                    hash: made(n),
                    size: 7,
                    kept_before: false,
                    pinned_before: false,
                });
            }
            kept.ran("the action the step's command was allowed".to_owned());
            if !made_all || kept.doomed().is_some() {
                continue;
            }
            kept_any = true;
            assert!(
                u64::try_from(kept.notes_kept().len()).expect("fits") <= bound,
                "{blobs}: a step kept past the store's bound"
            );
            let mut undo = Ambient::new(unit);
            undo.reverse(Reversing {
                kind: UnitKind::Undo { target: step },
                step,
                marks: vec![(step, UnitState::Undone { by: unit })],
                unpinned: (0..blobs).map(|n| (made(n).to_string(), 7)).collect(),
                repinned: Vec::new(),
                writes: 0,
                compensations: 1,
                beyond_reach: 0,
            });
            undo.ran("the action the compensation was allowed".to_owned());
            undo.ran("the undo's own action".to_owned());
            assert!(
                undo.doomed().is_none(),
                "{blobs}: the undo of a kept step is kept"
            );
            assert!(
                u64::try_from(undo.notes_kept().len()).expect("fits") <= bound,
                "{blobs}: the undo of a step the kernel kept records {} notes, and the store \
                 takes at most {bound}",
                undo.notes_kept().len()
            );
        }
        assert!(
            kept_any,
            "no step near the bound was kept, so nothing was asked"
        );
    }

    /// A reversal's notes count against its unit's bound from when it is told
    /// what it reverses, so a compensation that would note past the bound is
    /// refused as it runs, rather than the whole undo by the store after it.
    #[test]
    fn a_reversals_notes_count_against_its_unit_before_it_is_kept() {
        let (unit, step) = units();
        let bound = blobs_per_unit();
        let mut undo = Ambient::new(unit);
        undo.reverse(Reversing {
            kind: UnitKind::Undo { target: step },
            step,
            marks: Vec::new(),
            unpinned: (0..bound - 1)
                .map(|n| (Hash::of(&n.to_le_bytes()).to_string(), 7))
                .collect(),
            repinned: Vec::new(),
            writes: 0,
            compensations: 1,
            beyond_reach: 0,
        });
        assert!(
            undo.room_for_a_blob().is_err(),
            "the reversal's notes fill the unit"
        );
        undo.ran("an action".to_owned());
        assert!(undo.doomed().is_some(), "no room to note an action left");
        assert_eq!(
            undo.reversing().map(Reversing::noted),
            undo.reversing().map(|reversing| reversing.recorded().len()),
            "what is counted is what is recorded"
        );
    }

    #[test]
    fn a_key_put_back_is_held_to_the_value_the_step_left() {
        let (unit, earlier) = units();
        let mut overlay = Overlay::default();
        overlay
            .restore(
                unit,
                "p",
                "k",
                Some("left".to_owned()),
                Some("before".to_owned()),
            )
            .expect("within the bound");
        overlay
            .stage(
                unit,
                "p",
                "k",
                Some(&entry("x", earlier, 0)),
                Some("again".to_owned()),
            )
            .expect("within the bound");
        assert_eq!(
            overlay
                .writes()
                .iter()
                .map(|write| (write.expect.clone(), write.after.clone()))
                .collect::<Vec<_>>(),
            [(
                Expect::Value(Some("left".to_owned())),
                Some("again".to_owned())
            )],
            "held to the value, as first staged, and holding what was written last"
        );
    }

    #[test]
    fn a_key_changed_under_an_undo_or_a_redo_is_the_historys_conflict() {
        let conflict = CommitError::Conflict {
            plugin: "p".to_owned(),
            key: "k".to_owned(),
        };
        let reversing = refused_commit(&conflict, true);
        assert_eq!(
            (reversing.kind(), reversing.code()),
            (RefusalKind::Conflict, ErrorCode::KernelUndoConflict)
        );
        let stepping = refused_commit(&conflict, false);
        assert_eq!(stepping.code(), ErrorCode::PluginStoreConflict);
        assert_eq!(
            refused_commit(&CommitError::TooManyBlobs, true).code(),
            ErrorCode::PluginBoundReached,
            "only a conflict is the history's"
        );
    }

    /// The kernel's reversal of an undo commits its step again and pins back
    /// what it took the pin off; of a redo, takes off the pins it put back.
    #[test]
    fn a_reversal_of_an_undo_or_a_redo_puts_back_the_marks_and_pins_it_moved() {
        let (undo, by) = units();
        let realm = nc_ids::OpaqueId::new("home").expect("an id"); // allow-logging: OpaqueId::new of a literal this test writes
        let (a, b): (String, String) = (
            core::iter::repeat_n('a', 64).collect(),
            core::iter::repeat_n('b', 64).collect(),
        );
        let detail = |kind, notes| UnitDetail {
            row: UnitRow {
                unit: undo,
                meta: UnitMeta {
                    realm: realm.clone(),
                    principal: realm.clone(),
                    kind,
                    label: "kernel.undo".to_owned(),
                    committed_at_ms: 1,
                },
                state: UnitState::Committed,
            },
            images: Vec::new(),
            notes,
        };
        let undone = detail(
            UnitKind::Undo { target: by },
            vec![
                Note::BlobUnpinned {
                    hash: a.clone(),
                    size: 3,
                }
                .written(),
            ],
        );
        let reversed = reversal(&undone, by, 9, vec![(by, UnitState::Committed)], &[])
            .expect("the notes are the kernel's");
        assert_eq!(
            reversed.marks,
            [(undo, UnitState::Undone { by }), (by, UnitState::Committed)]
        );
        assert_eq!(reversed.blobs, [nc_local::BlobCreated { hash: a, size: 3 }]);
        assert_eq!(reversed.unpinned, [] as [String; 0]);
        let c: String = core::iter::repeat_n('c', 64).collect();
        let redone = detail(
            UnitKind::Redo { target: by },
            vec![
                Note::BlobRepinned { hash: b.clone() }.written(),
                Note::BlobRepinned { hash: c.clone() }.written(),
            ],
        );
        let reversed =
            reversal(&redone, by, 9, Vec::new(), &[c]).expect("the notes are the kernel's");
        assert_eq!(
            reversed.unpinned,
            [b],
            "a pin another unit holds is taken off by a redo's reversal"
        );
        assert_eq!(reversed.blobs, [] as [nc_local::BlobCreated; 0]);
        let unreadable = detail(UnitKind::Step, vec![r#"{"note":"unknown"}"#.to_owned()]);
        assert!(matches!(
            reversal(&unreadable, by, 9, Vec::new(), &[]),
            Err(LocalError::Unexpected(_))
        ));
    }

    /// The first label a plugin gives a unit names its step, made safe to show
    /// and cut at a character's boundary; an empty one names nothing.
    #[test]
    fn a_step_is_shown_as_the_first_label_a_plugin_gave_it_made_safe_to_show() {
        let (unit, _) = units();
        let mut ambient = Ambient::new(unit);
        ambient.labelled("  ");
        assert_eq!(ambient.label(), None, "an empty label names nothing");
        ambient.labelled("Import\u{202e} album\u{3164}");
        ambient.labelled("A second label");
        assert_eq!(
            ambient.label(),
            Some("Import\u{fffd} album\u{fffd}"),
            "the first label, a character that shows nothing replaced"
        );
        let mut long = Ambient::new(unit);
        long.labelled(&"é".repeat(LABEL_BYTES));
        assert_eq!(
            long.label().map(str::len),
            Some(LABEL_BYTES),
            "cut to the bound at a character's boundary"
        );
        assert_eq!(
            super::shown_as(&ambient, "reference.unit.commit"),
            "Import\u{fffd} album\u{fffd}"
        );
        assert_eq!(
            super::shown_as(&Ambient::new(unit), "reference.note.put"),
            "reference.note.put"
        );
        let (step, _) = units();
        ambient.reverse(Reversing {
            kind: UnitKind::Undo { target: step },
            step,
            marks: Vec::new(),
            unpinned: Vec::new(),
            repinned: Vec::new(),
            writes: 0,
            compensations: 0,
            beyond_reach: 0,
        });
        assert_eq!(
            super::shown_as(&ambient, "kernel.undo"),
            "kernel.undo",
            "an undo is shown as one, whatever its compensations labelled"
        );
    }
}
