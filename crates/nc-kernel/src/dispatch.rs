//! The decision point: `commands.dispatch`, the one door every command goes
//! through (`NOCTUA.md §11.7`, `§14.2`).
//!
//! **In one order, and each refusal recorded where it is made.** A dispatch is
//! held to the call tree's budgets — how deep it nests, how long the tree has
//! run, how many commands the tree has dispatched — then to the shape of its
//! name, to the registry, to the tree's frames (a command that changes state is
//! never run beneath one that only reads), to the bound on a payload, to
//! canonical JSON and to its command's schema, and its resource is taken from
//! its arguments by the command's selector. Only then is it authorized: the
//! decision point must have been built, the directory must answer for the
//! realm, the realm must be one whose pack this kernel carries, the grant and
//! the entities are the directory's, and what the kernel vouches for — the
//! time, the epoch, whether a human confirmed it and whether a quorum approved
//! it — is the kernel's alone. A command that cannot be undone is refused
//! after Cedar allows it unless a confirmation the kernel issued is redeemed
//! for it. **The plugin runs after all of that or not at all.**
//!
//! **Every dispatch is journaled**: a refusal, a read, a failure and every
//! dispatch in a tree, each as one entry naming the principal, the action, the
//! command and the entity, what was decided and by which policy or sentinel,
//! the pack and the epoch it was decided under, the grant it rested on and the
//! unit it ran in. A top-level dispatch of a command that changes state opens
//! its unit in the journal before the plugin runs, so a journal that cannot be
//! written means the plugin never runs; one that takes the opening and not the
//! entry, or the entry and not the close, means nothing the unit did is kept
//! (`crate::units`). A refusal stands whether or not the journal took it, and
//! is logged either way.
//!
//! **A nested dispatch is the tree's, and runs as its principal.** A plugin
//! dispatching from inside a command reaches this same door with the tree's
//! principal and inside the tree's unit; it is never confirmed and never
//! approved, because a confirmation and an approval are issued for one
//! dispatch at the top of a tree and redeemed there or not at all. A plugin
//! dispatching into itself while it runs is refused rather than trapped; one
//! asking past its tree's count of dispatches is told the count is spent, and
//! trapped rather than told again when it asks on.
//! Undo and redo run only at the top of a tree, and are refused nested in
//! another command (`crate::history`). **A callback the pump calls is a tree
//! of its own** — a timer fired or an event heard, for the principal it was
//! set or published for — whose frame is beneath every command it dispatches:
//! each is nested, never confirmed, and beneath an event's callback, which only
//! reads, no command may change state (`crate::pump`).
//!
//! **What an undo needs is noted as the tree runs.** Every command that
//! changes state and ran leaves a note in the unit of the action it was
//! allowed on its resource, which an undo authorizes again, and a command that
//! cannot be undone or that the kernel runs on the realm itself leaves one,
//! before it runs, of having left the machine, which an undo stops at.
//!
//! **A denial tells the caller its code and not the policy that decided.** The
//! journal records the deciding `@id` or sentinel either way; the caller is
//! told that it was denied and why in the kernel's words, because a policy that
//! reads the resource fires by what the resource holds, so its `@id` would tell
//! a caller whether an entity they cannot read exists.
//!
//! What this answers (`NOCTUA.md §2.4`): a mutation without authorization — no
//! plugin code runs before the decision, and none runs when the decision point,
//! the directory or the journal cannot be asked; a forged principal or
//! confirmation — the principal is the caller's and a nested dispatch's is the
//! tree's, and a confirmation or an approval is the kernel's, bound to one
//! principal, one command and its arguments, used once and only before it
//! expires; malformed input, refused before the plugin runs; and a call tree
//! that never ends, by depth, deadline and count — every dispatch asked for
//! counting, refused or not, and a plugin asking past the count once it was
//! told trapped, so one that asks again whatever it is told is stopped rather
//! than journaled without end, and one that only crossed the count others
//! spent is told and harmed no further; and a plugin held past its tree's time
//! by a journal that does not answer the dispatch it asked for, whose every
//! wait there is held as a host call is ([`Waits`]). What it does not answer: a
//! plugin that acts on another resource than the one it was authorized for
//! inside its own namespace, which is the plugin's code, and a tree whose
//! commands each stay within the bounds while the kernel's own work between
//! them is what takes the time.

use core::fmt;
use core::future::Future;
use core::pin::Pin;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::{Duration, Instant};

use nc_authz::{Directory as _, EntityRef, Facts, Principal, Question, sentinel};
use nc_canon::Value;
use nc_catalog::{ErrorCode, KERNEL};
use nc_ids::{Clock as _, OpaqueId, Snowflake};
use nc_journal::{Outcome, Record};
use nc_schema::descriptor::Reversibility;

use crate::events::Published;
use crate::logging::LogSink as _;
use crate::registry::{Admitted, Owner};
use crate::units::{self, Ambient, Later};
use crate::{Environment, PluginSlot, Shared, history, supervisor};

/// Who a dispatch is for: the principal a directory vouched for, and what the
/// kernel issued them for this dispatch.
#[derive(Debug)]
pub struct Caller {
    principal: Principal,
    confirmation: Option<Confirmation>,
    approval: Option<Approval>,
}

impl Caller {
    /// A caller who is `principal`, holding nothing the kernel issued.
    #[must_use]
    pub const fn new(principal: Principal) -> Self {
        Self {
            principal,
            confirmation: None,
            approval: None,
        }
    }

    /// The same caller, holding a human's `confirmation` of one dispatch.
    #[must_use]
    pub fn confirmed(mut self, confirmation: Confirmation) -> Self {
        self.confirmation = Some(confirmation);
        self
    }

    /// The same caller, holding a quorum's `approval` of one dispatch.
    #[must_use]
    pub fn approved(mut self, approval: Approval) -> Self {
        self.approval = Some(approval);
        self
    }

    /// Who the dispatch is for.
    #[must_use]
    pub const fn principal(&self) -> &Principal {
        &self.principal
    }

    /// The same caller again, for the task its dispatch runs on.
    ///
    /// What it holds is a serial the kernel honours once, so the copy is worth
    /// nothing the first was not.
    pub(crate) fn again(&self) -> Self {
        Self {
            principal: self.principal.clone(),
            confirmation: self.confirmation.as_ref().map(|held| Confirmation {
                serial: held.serial,
            }),
            approval: self.approval.as_ref().map(|held| Approval {
                serial: held.serial,
            }),
        }
    }
}

/// A human's confirmation of one dispatch, which only the kernel issues.
///
/// **Bound, once, for a while.** It names the principal, the command — by its
/// name and by the identity of the plugin that held it when it was issued —
/// and the BLAKE3 of the arguments' canonical text it was issued for, and the
/// kernel redeems it only for that dispatch, at the top of a call tree, within
/// `confirmation_ttl_ms` of issuing it — and only once: the kernel keeps what
/// it issued, and a confirmation it has redeemed is one it no longer holds.
/// Nothing outside the kernel can make one, and copying one copies a number the
/// kernel stops honouring the first time it is used.
#[derive(Debug, PartialEq, Eq)]
pub struct Confirmation {
    serial: u64,
}

/// A quorum's approval of one dispatch, which only the kernel issues, bound and
/// redeemed as a [`Confirmation`] is.
///
/// What issues one for real is the approval flow, a later stage's; until
/// then the kernel issues it through a door a test reaches, and a quorum
/// is met only by one it issued.
#[derive(Debug, PartialEq, Eq)]
pub struct Approval {
    serial: u64,
}

impl Confirmation {
    /// The confirmation the kernel issued as `serial`.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) const fn issued(serial: u64) -> Self {
        Self { serial }
    }
}

impl Approval {
    /// The approval the kernel issued as `serial`.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) const fn issued(serial: u64) -> Self {
        Self { serial }
    }
}

/// Which of the two the kernel issued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Token {
    /// A human confirmed it.
    Confirmation,
    /// A quorum approved it.
    Approval,
}

/// What one confirmation or approval was issued for.
///
/// **The whole principal, its kind with its identifier**: Cedar holds a user
/// and a device of one identifier for two principals, and with only the
/// identifier bound a confirmation issued to the one would be redeemed by the
/// other.
///
/// **And the command by who holds it, not only by its name**: a namespace an
/// unloaded plugin left is any identity's to take, and a command declared
/// there under the same name is that plugin's code. Bound by the name alone, a
/// person's confirmation of one plugin's command that cannot be undone would be
/// redeemed, after an unload and a load, running another's — the bound a
/// compensation is held to (`[NC-KERN-0023]`, `crate::history`).
#[derive(Debug)]
struct Binding {
    token: Token,
    principal: Principal,
    name: String,
    /// The identity of the plugin that held the command when it was issued:
    /// none for a command of the kernel's own, and for a name nothing held
    /// then — which no later load makes one of the kernel's, so a token
    /// naming none is redeemed for a command of the kernel's own alone.
    holder: Option<String>,
    args: [u8; 32],
    at_ms: u64,
    /// When it was issued on the monotonic clock, which no change of the wall
    /// clock moves.
    issued: Instant,
}

/// Every confirmation and approval the kernel has issued and not redeemed.
///
/// **Bounded by the time they live**: each issue forgets every one past its
/// `confirmation_ttl_ms`, so what a kernel holds is what it issued within that
/// time.
///
/// **Issued by the doors alone in this build.** The interface a person
/// confirms in and the flow that collects a quorum are later stages'; until
/// they exist a shipped kernel issues nothing, so it redeems nothing, and
/// a command that cannot be undone is denied to everyone — closed, not
/// open.
#[derive(Debug, Default)]
pub(crate) struct Issued {
    #[cfg(any(test, feature = "test-support"))]
    next: u64,
    outstanding: BTreeMap<u64, Binding>,
}

impl Issued {
    /// Issues a `token` for `principal` to dispatch `command` — its name, and
    /// the identity of the plugin holding it, none for the kernel's own — with
    /// `args`, the arguments' canonical text, now.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn issue(
        &mut self,
        token: Token,
        principal: &Principal,
        (name, holder): (&str, Option<&str>),
        args: &str,
        now_ms: u64,
    ) -> u64 {
        self.outstanding
            .retain(|_, binding| !expired(binding.at_ms, binding.issued, now_ms));
        self.next += 1;
        self.outstanding.insert(
            self.next,
            Binding {
                token,
                principal: principal.clone(),
                name: name.to_owned(),
                holder: holder.map(str::to_owned),
                args: digest(args),
                at_ms: now_ms,
                issued: Instant::now(),
            },
        );
        self.next
    }

    /// Whether `serial` is a `token` issued for exactly this dispatch — this
    /// principal, this `command` by its name and by the plugin holding it now,
    /// these arguments — and not yet expired: held still, until the decision
    /// of the dispatch it is presented with uses it up ([`used`](Self::used)).
    ///
    /// **One that does not match is kept**, so a caller presenting a
    /// confirmation with another command does not lose the one it holds; one
    /// that expired is forgotten.
    fn bound(
        &mut self,
        token: Token,
        serial: u64,
        principal: &Principal,
        (name, holder): (&str, Option<&str>),
        args: &str,
        now_ms: u64,
    ) -> bool {
        let Some(binding) = self.outstanding.get(&serial) else {
            return false;
        };
        if expired(binding.at_ms, binding.issued, now_ms) {
            self.outstanding.remove(&serial);
            return false;
        }
        binding.token == token
            && binding.principal == *principal
            && binding.name == name
            && binding.holder.as_deref() == holder
            && binding.args == digest(args)
    }

    /// Forgets `serial`, which a dispatch the kernel decided spent: it is
    /// redeemed, and held no longer.
    fn used(&mut self, serial: u64) {
        self.outstanding.remove(&serial);
    }
}

/// Whether something issued at `at_ms` on the kernel's clock, and at `issued`
/// on the monotonic one, is past its time at `now_ms`.
///
/// **On both clocks, and a wall clock behind the issue is past it.** The
/// kernel's clock is the wall clock, which steps back, and a confirmation
/// measured on it alone, with a saturating difference, would live an hour
/// longer for every hour the clock stepped back: every reading before the issue
/// would count as no time at all. The monotonic clock bounds its life whatever
/// the wall clock does, and a reading before the issue is one the kernel cannot
/// place, so it is refused rather than taken as fresh.
fn expired(at_ms: u64, issued: Instant, now_ms: u64) -> bool {
    let ttl = KERNEL.confirmation_ttl_ms;
    now_ms < at_ms || now_ms - at_ms > ttl || issued.elapsed() > Duration::from_millis(ttl)
}

/// The BLAKE3 of a dispatch's arguments, as the canonical text they were
/// validated, forwarded and hashed as.
fn digest(args: &str) -> [u8; 32] {
    *blake3::hash(args.as_bytes()).as_bytes()
}

/// Why a dispatch was not answered: the world's case, the registry's code and
/// a sentence saying what was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    kind: RefusalKind,
    code: ErrorCode,
    sentence: String,
    traps: bool,
}

impl Refusal {
    /// A refusal of `kind`, carrying `code`, said as `sentence`.
    pub(crate) const fn new(kind: RefusalKind, code: ErrorCode, sentence: String) -> Self {
        Self {
            kind,
            code,
            sentence,
            traps: false,
        }
    }

    /// The same refusal, made to a plugin that asked from inside a command by
    /// trapping it rather than as a value it reads.
    const fn trapping(mut self) -> Self {
        self.traps = true;
        self
    }

    /// Whether the plugin that asked is trapped rather than told: what a
    /// dispatch past its tree's count is answered with, once the plugin asking
    /// was told the count is spent.
    pub(crate) const fn traps(&self) -> bool {
        self.traps
    }

    /// The same refusal, its sentence ending with `clause`.
    pub(crate) fn ending(mut self, clause: &str) -> Self {
        self.sentence.push_str(clause);
        self
    }

    /// The world's case it is answered as.
    #[must_use]
    pub const fn kind(&self) -> RefusalKind {
        self.kind
    }

    /// The registry code it carries.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        self.code
    }

    /// What was refused, and why, without the code.
    #[must_use]
    pub fn sentence(&self) -> &str {
        &self.sentence
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "[{}] {}", self.code, self.sentence)
    }
}

impl std::error::Error for Refusal {}

/// The cases of the world's `error`, which a refusal is answered as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusalKind {
    /// Not allowed.
    Denied,
    /// Not there.
    NotFound,
    /// Not what was expected.
    Invalid,
    /// Written against something that changed.
    Conflict,
    /// Not reachable right now.
    Unavailable,
    /// The kernel's own failure, or a plugin's.
    Internal,
}

impl RefusalKind {
    /// The world's word for the case.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Denied => "denied",
            Self::NotFound => "not-found",
            Self::Invalid => "invalid",
            Self::Conflict => "conflict",
            Self::Unavailable => "unavailable",
            Self::Internal => "internal",
        }
    }
}

/// Why the decision point turned a dispatch away, as the journal names the
/// kernel's own decisions and the log names every refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reason {
    UnknownCommand,
    MalformedName,
    MalformedArguments,
    Denied,
    Unconfirmed,
    ReadOnly,
    TopLevelOnly,
    TooDeep,
    Deadline,
    TooManyDispatches,
    Reentrant,
    Quarantined,
    JournalUnavailable,
}

impl Reason {
    /// What the journal records as having decided, when the kernel did: a
    /// sentinel beginning with `kernel.`, which no policy's `@id` may.
    const fn sentinel(self) -> &'static str {
        match self {
            Self::UnknownCommand => "kernel.unknown_command",
            Self::MalformedName => "kernel.malformed_name",
            Self::MalformedArguments => "kernel.malformed_arguments",
            Self::Denied => sentinel::NO_PERMIT_MATCHED,
            Self::Unconfirmed => "kernel.irreversible_needs_confirmation",
            Self::ReadOnly => "kernel.read_only",
            Self::TopLevelOnly => "kernel.top_level_only",
            Self::TooDeep => "kernel.too_deep",
            Self::Deadline => "kernel.deadline",
            Self::TooManyDispatches => "kernel.too_many_dispatches",
            Self::Reentrant => "kernel.reentrant",
            Self::Quarantined => "kernel.quarantined",
            Self::JournalUnavailable => "kernel.journal_unavailable",
        }
    }

    /// The variant the log names it as.
    fn variant(self) -> nc_log::Variant {
        match self {
            Self::UnknownCommand => nc_log::variant!("unknown_command"),
            Self::MalformedName => nc_log::variant!("malformed_name"),
            Self::MalformedArguments => nc_log::variant!("malformed_arguments"),
            Self::Denied => nc_log::variant!("denied"),
            Self::Unconfirmed => nc_log::variant!("unconfirmed"),
            Self::ReadOnly => nc_log::variant!("read_only"),
            Self::TopLevelOnly => nc_log::variant!("top_level_only"),
            Self::TooDeep => nc_log::variant!("too_deep"),
            Self::Deadline => nc_log::variant!("deadline"),
            Self::TooManyDispatches => nc_log::variant!("too_many_dispatches"),
            Self::Reentrant => nc_log::variant!("reentrant"),
            Self::Quarantined => nc_log::variant!("quarantined"),
            Self::JournalUnavailable => nc_log::variant!("journal_unavailable"),
        }
    }
}

/// One top-level dispatch and everything it dispatched: who it runs for, until
/// when, how much it has dispatched, the frames running now and the unit its
/// writes are staged in.
///
/// **Its deadline is kept on two clocks, set at the same moment.** A dispatch
/// asked for in the tree is refused past `deadline_ms`, on the kernel's clock,
/// which a test moves; a call running in it is stopped `dispatch_deadline_ms`
/// after `began`, on the monotonic clock the epoch is measured on, which no
/// change of the wall clock moves.
#[derive(Debug)]
pub(crate) struct CallTree {
    principal: Principal,
    deadline_ms: u64,
    began: Instant,
    dispatches: u32,
    /// Each plugin told, by a refusal it read, that the tree's count of
    /// dispatches is spent: at most one entry for each plugin loaded.
    told: Vec<PluginSlot>,
    /// Each frame running now, outermost first, as whether a command
    /// dispatched beneath it may change state.
    frames: Vec<bool>,
    unit: Option<Ambient>,
    /// What the tree put off while it held no unit, released once it ends.
    put_off: Later,
    /// Whether the journal refused the entry of a dispatch of the tree, so
    /// that nothing it put off is released.
    entry_refused: bool,
}

impl CallTree {
    /// A tree for `principal`, begun at `now_ms`, with nothing dispatched yet.
    pub(crate) fn new(principal: Principal, now_ms: u64) -> Self {
        Self {
            principal,
            deadline_ms: now_ms.saturating_add(KERNEL.dispatch_deadline_ms),
            began: Instant::now(),
            dispatches: 0,
            told: Vec::new(),
            frames: Vec::new(),
            unit: None,
            put_off: Later::default(),
            entry_refused: false,
        }
    }

    /// Where the tree holds what it puts off now: in its unit, kept with it,
    /// when it holds one, and in its own [`Later`] when it does not.
    pub(crate) const fn later_mut(&mut self) -> &mut Later {
        match &mut self.unit {
            Some(unit) => unit.later_mut(),
            None => &mut self.put_off,
        }
    }

    /// Every event the tree holds for the queue: its own, and its unit's.
    pub(crate) fn held_events(&self) -> impl Iterator<Item = &Published> {
        self.put_off.published().iter().chain(
            self.unit
                .iter()
                .flat_map(|unit| unit.later().published().iter()),
        )
    }

    /// What the tree holds done to the surfaces and has not released: its
    /// own, then its unit's — the order the two are released in.
    pub(crate) fn surfaces_held(&self) -> Vec<&crate::ui::Shown> {
        self.put_off
            .surfaces_shown()
            .iter()
            .chain(
                self.unit
                    .iter()
                    .flat_map(|unit| unit.later().surfaces_shown().iter()),
            )
            .collect()
    }

    /// How many timers the tree holds for the plugin in `owner`: its own,
    /// and its unit's.
    pub(crate) fn timers_held_for(&self, owner: PluginSlot) -> usize {
        self.put_off.timers_of(owner).saturating_add(
            self.unit
                .as_ref()
                .map_or(0, |unit| unit.later().timers_of(owner)),
        )
    }

    /// Takes back the timer `id` the plugin in `owner` set in the tree, held
    /// by the tree or by its unit, answering whether either held one.
    pub(crate) fn timer_taken_back(&mut self, id: u64, owner: PluginSlot) -> bool {
        self.put_off.unset_timer(id, owner)
            || self
                .unit
                .as_mut()
                .is_some_and(|unit| unit.later_mut().unset_timer(id, owner))
    }

    /// What the tree put off while it held no unit, taken out of it as it
    /// ends: none when the journal refused the entry of any of its
    /// dispatches, whose answer was withheld and whose effects go with it.
    pub(crate) fn put_off_taken(&mut self) -> Option<Later> {
        let put_off = core::mem::take(&mut self.put_off);
        (!self.entry_refused).then_some(put_off)
    }

    /// When the tree began, on the monotonic clock.
    pub(crate) const fn began(&self) -> Instant {
        self.began
    }

    /// Who the tree runs for.
    pub(crate) const fn principal(&self) -> &Principal {
        &self.principal
    }

    /// Opens the frame of a callback the kernel calls as the tree's top:
    /// every command it dispatches is nested beneath it.
    pub(crate) fn calls_back(&mut self, call: Call) {
        self.frames.push(call.lets_changes_beneath());
    }

    /// The unit the tree's writes are staged in, taken out of it to be kept
    /// or let go.
    pub(crate) const fn unit_taken(&mut self) -> Option<Ambient> {
        self.unit.take()
    }

    /// Makes `unit` the tree's: the one a timer's callback begins.
    pub(crate) fn begins(&mut self, unit: Ambient) {
        self.unit = Some(unit);
    }

    /// The unit the tree's writes are staged in, if it has one.
    pub(crate) const fn unit(&self) -> Option<&Ambient> {
        self.unit.as_ref()
    }

    /// The unit the tree's writes are staged in, to stage one.
    pub(crate) const fn unit_mut(&mut self) -> Option<&mut Ambient> {
        self.unit.as_mut()
    }

    /// Spends one dispatch at `depth`, `now_ms`, asked for by the plugin in
    /// `requester` — none for the kernel's own: every one asked for, whether it
    /// runs or is refused.
    ///
    /// **A refusal spends the count as a dispatch that runs does**, and a
    /// plugin that asks past the count once it was told the count is spent is
    /// trapped rather than told again. A plugin that asks again whatever it is
    /// told — too deep, past the deadline, past the count — is otherwise
    /// refused as often as its fuel lets it ask, each refusal one more entry in
    /// the journal; so it is stopped at the tree's count, killed, and the unit
    /// it ran in with it.
    ///
    /// **Told first, because the count is the tree's and not the requester's.**
    /// Every plugin of a tree spends it, so the dispatch that happens to cross
    /// it is no measure of who spent it: trapped at once, a peer dispatching
    /// once for each of a relay's calls would be killed at the count the relay
    /// spent, and quarantined after `restart_budget` such trees, while the
    /// relay, which stops at the refusal it reads, would never be killed. Told
    /// once, the peer answers its caller and harms nothing; the plugin that
    /// asks on is the one stopped. So the journal holds at most two refusals
    /// past the count for each plugin of the tree: the one it was told, and the
    /// one it was trapped for.
    ///
    /// # Errors
    ///
    /// The refusal, and its reason, when the tree has been asked for
    /// `dispatches_per_tree` dispatches already — which traps a plugin told
    /// so before — when the dispatch nests deeper than `dispatch_depth`, and
    /// when the tree is past its deadline.
    fn spend(
        &mut self,
        depth: usize,
        now_ms: u64,
        requester: Option<PluginSlot>,
    ) -> Result<(), (Refusal, Reason)> {
        self.dispatches = self.dispatches.saturating_add(1);
        if self.dispatches > KERNEL.dispatches_per_tree {
            let refusal = Refusal::new(
                RefusalKind::Denied,
                ErrorCode::KernelTooManyDispatches,
                format!(
                    "the call tree has been asked for the {} dispatches one tree may, refused \
                     or not",
                    KERNEL.dispatches_per_tree
                ),
            );
            let refusal = if self.told_of_the_count(requester) {
                refusal.trapping()
            } else {
                refusal
            };
            return Err((refusal, Reason::TooManyDispatches));
        }
        if depth > usize::try_from(KERNEL.dispatch_depth).unwrap_or(usize::MAX) {
            return Err((
                Refusal::new(
                    RefusalKind::Denied,
                    ErrorCode::KernelTooDeep,
                    format!(
                        "a dispatch at depth {depth} is deeper than the {} a call tree may \
                         nest",
                        KERNEL.dispatch_depth
                    ),
                ),
                Reason::TooDeep,
            ));
        }
        if now_ms >= self.deadline_ms {
            return Err((
                Refusal::new(
                    RefusalKind::Unavailable,
                    ErrorCode::KernelDeadline,
                    format!(
                        "the call tree has run past its {} ms",
                        KERNEL.dispatch_deadline_ms
                    ),
                ),
                Reason::Deadline,
            ));
        }
        Ok(())
    }

    /// Whether the plugin in `requester` was told already, in this tree, that
    /// its count of dispatches is spent — and, when it was not, that it is
    /// told now. The kernel asking for its own is never one told.
    fn told_of_the_count(&mut self, requester: Option<PluginSlot>) -> bool {
        let Some(slot) = requester else {
            return false;
        };
        if self.told.contains(&slot) {
            return true;
        }
        self.told.push(slot);
        false
    }

    /// Whether a frame running in the tree now only reads: no command that
    /// changes state may run beneath it, and a timer set there fires where
    /// only reading is.
    pub(crate) fn reading(&self) -> bool {
        self.frames.iter().any(|mutating| !mutating)
    }
}

/// The tree's lock, whoever last held it: nothing panics holding it.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What a frame runs: one of the plugin's commands, or one of the two
/// callbacks the kernel calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Call {
    /// A command dispatched to the plugin, which may change state when it is
    /// declared `mutating`.
    Command {
        /// Whether the command is declared to change state.
        mutating: bool,
    },
    /// `on-timer`: a timer the plugin set, fired. It writes nothing itself,
    /// and may dispatch commands that change state and group them in a unit
    /// of its own, when the timer was set where a change was allowed.
    Timer {
        /// Whether the frame that set the timer let a change run beneath it:
        /// one set where only reading was allowed fires where only reading is.
        may_change: bool,
    },
    /// `on-event`: an event the plugin subscribed to, delivered. It only
    /// reads, and no command beneath it may change state.
    Event,
}

impl Call {
    /// Whether a command dispatched beneath it may change state: not beneath a
    /// command that only reads, nor beneath an event, nor beneath a timer
    /// set where only reading was allowed.
    const fn lets_changes_beneath(self) -> bool {
        match self {
            Self::Command { mutating } => mutating,
            Self::Timer { may_change } => may_change,
            Self::Event => false,
        }
    }
}

/// What a frame runs under: the command a principal was allowed, the action
/// it was allowed and the resource it was allowed it on.
///
/// **Captured where something is put off, and decided again when it comes
/// due.** A timer a frame sets and an event it publishes carry the authority
/// of the frame that set or published it, and the decision point decides it
/// again, now, before the callback runs (`NOCTUA.md §14.2`): a grant expired
/// since, or a resource no longer the principal's, stops the callback rather
/// than letting it run on what was once allowed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Authority {
    /// The command whose authorization this is.
    pub(crate) command: String,
    /// The Cedar action it was allowed.
    pub(crate) action: String,
    /// The resource it was allowed it on.
    pub(crate) resource: EntityRef,
}

/// What a plugin's store holds while one of its commands or callbacks runs:
/// the tree it runs in, what it runs, the plugin running it, the authority it
/// runs under, whether a person confirmed it, and the kernel a dispatch from
/// it reaches.
///
/// **Set for the call and taken back after it**, so a plugin's store holds no
/// tree between calls, and `activate` — which runs with none — may read what
/// is committed and do nothing else.
pub(crate) struct Frame<E: Environment> {
    kernel: Weak<Shared<E>>,
    tree: Arc<Mutex<CallTree>>,
    slot: PluginSlot,
    call: Call,
    authority: Authority,
    confirmed: bool,
}

impl<E: Environment> fmt::Debug for Frame<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Frame")
            .field("tree", &self.tree)
            .field("slot", &self.slot)
            .field("call", &self.call)
            .field("authority", &self.authority)
            .field("confirmed", &self.confirmed)
            .finish_non_exhaustive()
    }
}

impl<E: Environment> Frame<E> {
    /// A frame of `tree`, under `kernel`, in which the plugin in `slot` runs
    /// its `call` under `authority`, confirmed by a person when `confirmed`.
    pub(crate) const fn new(
        kernel: Weak<Shared<E>>,
        tree: Arc<Mutex<CallTree>>,
        slot: PluginSlot,
        call: Call,
        authority: Authority,
        confirmed: bool,
    ) -> Self {
        Self {
            kernel,
            tree,
            slot,
            call,
            authority,
            confirmed,
        }
    }

    /// Whether the call running may change state: a command declared to.
    pub(crate) const fn mutating(&self) -> bool {
        matches!(self.call, Call::Command { mutating: true })
    }

    /// What the frame runs.
    pub(crate) const fn call(&self) -> Call {
        self.call
    }

    /// The plugin running it.
    pub(crate) const fn slot(&self) -> PluginSlot {
        self.slot
    }

    /// The authority it runs under, which a timer it sets and an event it
    /// publishes carry.
    pub(crate) const fn authority(&self) -> &Authority {
        &self.authority
    }

    /// Whether a person confirmed what runs: only the top of a call tree
    /// whose dispatch redeemed a confirmation the kernel issued for it.
    pub(crate) const fn confirmed(&self) -> bool {
        self.confirmed
    }

    /// Who the tree runs for.
    pub(crate) fn principal(&self) -> Principal {
        self.tree().principal.clone()
    }

    /// The kernel the command runs under: none once it is gone, which no
    /// frame outlives, and none in a frame a host function's own test made.
    pub(crate) fn kernel(&self) -> Option<Arc<Shared<E>>> {
        self.kernel.upgrade()
    }

    /// The tree the command runs in.
    pub(crate) fn tree(&self) -> MutexGuard<'_, CallTree> {
        lock(&self.tree)
    }

    /// When the tree the command runs in began, on the monotonic clock.
    pub(crate) fn tree_began(&self) -> Instant {
        self.tree().began()
    }

    /// Dispatches `name` with `args` from this frame, as the tree's principal.
    ///
    /// **On the runtime, not on the calling plugin's stack.** A host function
    /// runs on the fiber wasmtime gives the plugin's call, and Cedar measures
    /// the stack it has left with `stacker`, which reads the thread's bounds
    /// and not the fiber's: measured, every decision made there would be
    /// refused as `kernel.request_invalid`, Cedar's own check finding no room.
    /// So the nested dispatch is a task of its own, which the frame waits on.
    ///
    /// # Errors
    ///
    /// Whatever the decision point refuses — a dispatch past the tree's count,
    /// once the plugin was told the count is spent, as a refusal that
    /// [`Refusal::traps`], which the host function answers by trapping the
    /// plugin — and `internal` with `[NC-KERN-0008]`
    /// when the kernel the frame ran under is gone, which no frame outlives, or
    /// the task the dispatch ran in did not finish.
    pub(crate) async fn dispatch(&self, name: &str, args: &str) -> Result<String, Refusal> {
        let gone = |why: &str| {
            Refusal::new(
                RefusalKind::Internal,
                ErrorCode::KernelUnitAborted,
                format!("`{name}` was dispatched from a command and could not run: {why}"),
            )
        };
        let Some(kernel) = self.kernel.upgrade() else {
            return Err(gone("the kernel that command ran under is gone"));
        };
        let (tree, requester) = (Arc::clone(&self.tree), Some(self.slot));
        let (owned, given) = (name.to_owned(), args.to_owned());
        tokio::spawn(async move { nested(&kernel, &tree, &owned, &given, requester).await })
            .await
            .unwrap_or_else(|_| Err(gone("the task it ran in did not finish")))
    }
}

/// Dispatches `name` with `args` for `caller` as the top of a call tree, on a
/// task of its own.
///
/// **It runs to its end whoever stops waiting for it**, for the reasons
/// [`crate::Kernel::dispatch`] gives.
///
/// # Errors
///
/// Whatever [`top`] refuses, and `internal` with `[NC-KERN-0008]` when the
/// task did not finish, as [`joined`] says.
pub(crate) async fn detached<E: Environment>(
    shared: &Arc<Shared<E>>,
    caller: &Caller,
    name: &str,
    args: &str,
) -> Result<String, Refusal> {
    let (shared, caller) = (Arc::clone(shared), caller.again());
    let (owned, given) = (name.to_owned(), args.to_owned());
    let tree = async move { top(&shared, &caller, &owned, &given).await };
    joined(name, tokio::spawn(tree).await)
}

/// What the task a top-level dispatch of `name` ran on answered.
///
/// # Errors
///
/// The dispatch's own refusal, and `internal` with `[NC-KERN-0008]` when the
/// task did not finish — it panicked, or the runtime stopped it, which only
/// its shutting down does.
fn joined(
    name: &str,
    joined: Result<Result<String, Refusal>, tokio::task::JoinError>,
) -> Result<String, Refusal> {
    joined.unwrap_or_else(|_| {
        Err(Refusal::new(
            RefusalKind::Internal,
            ErrorCode::KernelUnitAborted,
            format!("`{name}` could not run: the task it ran in did not finish"),
        ))
    })
}

#[cfg(test)]
impl<E: Environment> Frame<E> {
    /// A frame under no kernel, for a host function's own tests.
    ///
    /// Of a tree run for `ana` of the world the policy cases are about, holding
    /// the unit `unit` when there is one: those tests reach the tree, and never
    /// the door.
    pub(crate) fn for_tests(mutating: bool, unit: Option<Snowflake>) -> Self {
        let pdp = nc_authz::Pdp::embedded("family").expect("the family pack is carried");
        let directory =
            nc_authz::MemoryDirectory::from_world(include_str!("../../../policy/world.json"), &pdp)
                .expect("the world loads");
        let principal = directory
            .principal(nc_authz::PrincipalKind::User, "ana")
            .expect("ana is a user of the world");
        let mut tree = CallTree::new(principal, 0);
        tree.unit = unit.map(Ambient::new);
        Self::new(
            Weak::new(),
            Arc::new(Mutex::new(tree)),
            PluginSlot(0),
            Call::Command { mutating },
            Authority {
                command: "tests.door".to_owned(),
                action: "Read".to_owned(),
                resource: EntityRef {
                    type_name: "Content".to_owned(),
                    id: "photo".to_owned(),
                },
            },
            false,
        )
    }

    /// The frame, running `call` rather than the command it was made for.
    pub(crate) const fn calling_for_tests(mut self, call: Call) -> Self {
        self.call = call;
        self
    }

    /// The frame, of a dispatch a person confirmed.
    pub(crate) const fn confirmed_for_tests(mut self) -> Self {
        self.confirmed = true;
        self
    }

    /// The frame, of a tree that began at `began` rather than when it was
    /// made.
    pub(crate) fn begun_for_tests(self, began: Instant) -> Self {
        self.tree().began = began;
        self
    }

    /// The tree the frame runs in, which a test reads after the frame is let
    /// go.
    pub(crate) fn tree_for_tests(&self) -> Arc<Mutex<CallTree>> {
        Arc::clone(&self.tree)
    }
}

/// Dispatches `name` with `args` for `caller`, as the top of a call tree.
///
/// **One tree at a time.** Every top-level dispatch takes the kernel's bus
/// first, so two trees never interleave their units — running trees side by
/// side is not built; a nested dispatch is inside the tree that holds it, and
/// never asks for it.
pub(crate) async fn top<E: Environment>(
    shared: &Arc<Shared<E>>,
    caller: &Caller,
    name: &str,
    args: &str,
) -> Result<String, Refusal> {
    let _one_tree_at_a_time = shared.bus.lock().await;
    supervisor::started_again(shared).await;
    let tree = Arc::new(Mutex::new(CallTree::new(
        caller.principal.clone(),
        shared.clock.now_ms(),
    )));
    let answered = decide(shared, &tree, Some(caller), name, args, None).await;
    let put_off = lock(&tree).put_off_taken();
    if let Some(later) = put_off {
        later.released(shared);
    }
    answered
}

/// Dispatches `name` with `args` from inside `tree`, as its principal, asked
/// for by the plugin in `requester` — none when the kernel asks for its own,
/// as an undo dispatching a compensation does.
///
/// Boxed, because a nested dispatch runs a plugin whose command may dispatch
/// again, and a future cannot hold itself.
pub(crate) fn nested<'a, E: Environment>(
    shared: &'a Arc<Shared<E>>,
    tree: &'a Arc<Mutex<CallTree>>,
    name: &'a str,
    args: &'a str,
    requester: Option<PluginSlot>,
) -> Pin<Box<dyn Future<Output = Result<String, Refusal>> + Send + 'a>> {
    Box::pin(decide(shared, tree, None, name, args, requester))
}

/// One dispatch through every step of the decision point, in order.
async fn decide<E: Environment>(
    shared: &Arc<Shared<E>>,
    tree: &Arc<Mutex<CallTree>>,
    caller: Option<&Caller>,
    name: &str,
    args: &str,
    requester: Option<PluginSlot>,
) -> Result<String, Refusal> {
    let began = lock(tree).began();
    let waits = Waits::of(caller.is_none(), began);
    let asked = asked(shared, tree, name, args, requester, waits).await?;
    let authorized = match authorize(shared, &asked, caller, began).await {
        Ok(authorized) => authorized,
        Err(turned) => return Err(turned_away(shared, tree, *turned, waits).await),
    };
    run(shared, tree, &asked, &authorized).await
}

/// A dispatch that passed every step before authorization: the command, its
/// arguments as canonical text and as the schema read them, its resource, and
/// where in the tree it was asked.
struct Asked {
    name: String,
    command: Admitted,
    canonical: String,
    json: serde_json::Value,
    resource: EntityRef,
    depth: usize,
    principal: Principal,
    waits: Waits,
}

/// What the decision point decided a dispatch under, beside the policies.
#[derive(Debug, Clone)]
struct Decided {
    pack: String,
    epoch: u64,
    grant: Option<OpaqueId>,
}

/// An allowed dispatch: what decided it, under what, and what the caller was
/// issued for it.
struct Authorized {
    deciding: String,
    decided: Decided,
    held: history::Held,
}

/// A dispatch about to be refused: the refusal, and everything its journal
/// entry and its log line say.
struct Turned {
    refusal: Refusal,
    reason: Reason,
    mutating: bool,
    action: String,
    resource: Value,
    result: Outcome,
    deciding: String,
    decided: Option<Decided>,
}

impl Turned {
    /// A refusal the kernel decided before the command's own action was
    /// known, recorded under `kernel.dispatch`.
    fn early(refusal: Refusal, reason: Reason, resource: Value) -> Self {
        Self {
            refusal,
            reason,
            mutating: false,
            action: DISPATCH.to_owned(),
            resource,
            result: Outcome::Denied,
            deciding: reason.sentinel().to_owned(),
            decided: None,
        }
    }

    /// A refusal of a registered command, decided by `deciding`.
    fn of(
        command: &Admitted,
        refusal: Refusal,
        reason: Reason,
        resource: Value,
        deciding: String,
        decided: Option<Decided>,
    ) -> Self {
        Self {
            refusal,
            reason,
            mutating: command.listed().mutating,
            action: command.listed().action.clone(),
            resource,
            result: Outcome::Denied,
            deciding,
            decided,
        }
    }
}

/// The action a dispatch is recorded under before its command is known.
const DISPATCH: &str = "kernel.dispatch";

/// The resource a dispatch is recorded with: the command, the entity it acts
/// on when it is known, and the failure when it failed.
///
/// **A name is recorded only when it is a command's name**: a malformed one is
/// text the caller chose, and the journal keeps it for ever.
fn resource_of(name: &str, entity: Option<&EntityRef>, failure: Option<&str>) -> Value {
    if !nc_schema::descriptor::command_name_is_well_formed(name) {
        return Value::object([("refused", Value::text("malformed-name"))]);
    }
    let mut fields = BTreeMap::new();
    fields.insert("command".to_owned(), Value::text(name));
    if let Some(entity) = entity {
        fields.insert(
            "entity".to_owned(),
            Value::object([
                ("type", Value::text(entity.type_name.clone())),
                ("id", Value::text(entity.id.clone())),
            ]),
        );
    }
    if let Some(failure) = failure {
        fields.insert("failure".to_owned(), Value::text(failure));
    }
    Value::Object(fields)
}

/// Journals `turned` as the tree's, waiting as `waits` says, logs it, and
/// answers its refusal.
///
/// **The refusal stands whether or not the journal took it**: a journal that
/// is down denies nothing more and allows nothing more, and the log line is
/// written either way.
async fn turned_away<E: Environment>(
    shared: &Shared<E>,
    tree: &Arc<Mutex<CallTree>>,
    turned: Turned,
    waits: Waits,
) -> Refusal {
    let (principal, unit) = {
        let tree = lock(tree);
        (tree.principal.clone(), tree.unit().map(Ambient::unit))
    };
    let record = Record {
        realm: shared.realm.clone(),
        principal: principal.id().clone(),
        actor_chain: Vec::new(),
        action: turned.action,
        resource: turned.resource,
        result: turned.result,
        deciding_policy: Some(turned.deciding),
        policy_pack: turned.decided.as_ref().map(|decided| decided.pack.clone()),
        policy_epoch: turned.decided.as_ref().map(|decided| decided.epoch),
        grant_used: turned.decided.and_then(|decided| decided.grant),
        work_unit: unit,
        recorded_at: shared.clock.now_ms(),
    };
    let _refused_either_way = waits
        .on(async { shared.journal.lock().await.record(record).await })
        .await;
    refused_in_the_log(shared, &turned.refusal, turned.reason, turned.mutating);
    turned.refusal
}

/// Writes the log line a refusal is, with no word of what was asked.
fn refused_in_the_log<E: Environment>(
    shared: &Shared<E>,
    refusal: &Refusal,
    reason: Reason,
    mutating: bool,
) {
    shared.log.record(&nc_log::events::KernelDispatchRefused {
        code: refusal.code(),
        reason: reason.variant(),
        mutating,
    });
}

/// Steps one to seven: the budgets, the name, the registry, the frames, the
/// payload, the schema and the resource — of a dispatch the plugin in
/// `requester` asked for, or the kernel for none.
///
/// # Errors
///
/// What the first step that refused answered, journaled and logged — the
/// refusal the caller is told.
async fn asked<E: Environment>(
    shared: &Shared<E>,
    tree: &Arc<Mutex<CallTree>>,
    name: &str,
    args: &str,
    requester: Option<PluginSlot>,
    waits: Waits,
) -> Result<Asked, Refusal> {
    let (principal, depth, spent) = {
        let mut tree = lock(tree);
        let depth = tree.frames.len();
        let spent = tree.spend(depth, shared.clock.now_ms(), requester);
        (tree.principal.clone(), depth, spent)
    };
    if let Err((refusal, reason)) = spent {
        let resource = resource_of(name, None, None);
        let mut turned = Turned::early(refusal, reason, resource);
        // **Logged as the command it would have been**, when the registry
        // holds one by that name: a refused write is the one a person may be
        // waiting on, which is what the log's `mutating` says. Without this,
        // every write refused for its tree's depth, deadline or count would be
        // logged as a read.
        turned.mutating = nc_schema::descriptor::command_name_is_well_formed(name)
            && shared
                .command(name)
                .is_some_and(|command| command.listed().mutating);
        return Err(turned_away(shared, tree, turned, waits).await);
    }
    let command = match registered(shared, tree, name, depth) {
        Ok(command) => command,
        Err(turned) => return Err(turned_away(shared, tree, *turned, waits).await),
    };
    match understood(shared, &command, name, args) {
        Ok((canonical, json, resource)) => Ok(Asked {
            name: name.to_owned(),
            command,
            canonical,
            json,
            resource,
            depth,
            principal,
            waits,
        }),
        Err(refusal) => {
            let resource = resource_of(name, None, None);
            Err(turned_away(
                shared,
                tree,
                Turned::of(
                    &command,
                    refusal,
                    Reason::MalformedArguments,
                    resource,
                    Reason::MalformedArguments.sentinel().to_owned(),
                    None,
                ),
                waits,
            )
            .await)
        }
    }
}

/// Steps two to four: a name in the shape of one, a command the registry
/// holds, and a frame it may run under — undo and redo at the top of a tree
/// only.
fn registered<E: Environment>(
    shared: &Shared<E>,
    tree: &Arc<Mutex<CallTree>>,
    name: &str,
    depth: usize,
) -> Result<Admitted, Box<Turned>> {
    if !nc_schema::descriptor::command_name_is_well_formed(name) {
        return Err(Box::new(Turned::early(
            Refusal::new(
                RefusalKind::Invalid,
                ErrorCode::KernelMalformedName,
                "the name dispatched is not a command's: lower-case words joined by dots, at \
                 most 96 characters"
                    .to_owned(),
            ),
            Reason::MalformedName,
            resource_of(name, None, None),
        )));
    }
    let Some(command) = shared.command(name) else {
        return Err(Box::new(Turned::early(
            Refusal::new(
                RefusalKind::NotFound,
                ErrorCode::KernelUnknownCommand,
                format!("no command `{name}` is registered"),
            ),
            Reason::UnknownCommand,
            resource_of(name, None, None),
        )));
    };
    if depth > 0 && history::is_history(name) {
        let refusal = Refusal::new(
            RefusalKind::Denied,
            ErrorCode::KernelTopLevelOnly,
            format!(
                "`{name}` runs only at the top of a call tree, and it was dispatched from inside \
                 a command"
            ),
        );
        return Err(Box::new(Turned::of(
            &command,
            refusal,
            Reason::TopLevelOnly,
            resource_of(name, None, None),
            Reason::TopLevelOnly.sentinel().to_owned(),
            None,
        )));
    }
    if command.listed().mutating && lock(tree).reading() {
        let refusal = Refusal::new(
            RefusalKind::Denied,
            ErrorCode::KernelReadOnly,
            format!(
                "`{name}` changes state, and it was dispatched from inside a command that only \
                 reads"
            ),
        );
        return Err(Box::new(Turned::of(
            &command,
            refusal,
            Reason::ReadOnly,
            resource_of(name, None, None),
            Reason::ReadOnly.sentinel().to_owned(),
            None,
        )));
    }
    Ok(command)
}

/// Steps five to seven: the payload's bound, canonical JSON, the schema and
/// the resource — answered as the arguments' canonical text, the value the
/// schema read, and the entity the selector took.
///
/// **What is validated is what is forwarded and hashed**: the plugin is handed
/// the canonical text, the schema read that same text, and a confirmation is
/// bound to its digest.
pub(crate) fn understood<E: Environment>(
    shared: &Shared<E>,
    command: &Admitted,
    name: &str,
    args: &str,
) -> Result<(String, serde_json::Value, EntityRef), Refusal> {
    if u64::try_from(args.len()).unwrap_or(u64::MAX) > KERNEL.payload_bytes {
        return Err(Refusal::new(
            RefusalKind::Invalid,
            ErrorCode::ValidationOutOfRange,
            format!(
                "the arguments of `{name}` are {} bytes, past the {} a payload may be",
                args.len(),
                KERNEL.payload_bytes
            ),
        ));
    }
    let value = nc_canon::parse(args).map_err(|refused| {
        Refusal::new(
            RefusalKind::Invalid,
            refused.code(),
            format!("the arguments of `{name}` are not canonical JSON: {refused}"),
        )
    })?;
    let canonical = value.to_canonical();
    let json: serde_json::Value = serde_json::from_str(&canonical).map_err(|refused| {
        Refusal::new(
            RefusalKind::Internal,
            ErrorCode::CanonUnparsable,
            format!(
                "the canonical text of the arguments of `{name}` does not read back: {refused}"
            ),
        )
    })?;
    let judged = command.judged();
    if let Err(violations) = judged.schema.validate(&json) {
        let more = violations.len().saturating_sub(1);
        let first = violations
            .first()
            .map_or_else(String::new, ToString::to_string);
        return Err(Refusal::new(
            RefusalKind::Invalid,
            ErrorCode::ValidationArgumentsInvalid,
            format!(
                "the arguments of `{name}` do not fit its schema: {first}{}",
                if more == 0 {
                    String::new()
                } else {
                    format!(", and {more} more")
                }
            ),
        ));
    }
    let resource = judged
        .selector
        .extract(shared.realm.as_str(), &json)
        .map_err(|refused| {
            Refusal::new(
                RefusalKind::Invalid,
                ErrorCode::ValidationResourceIdMalformed,
                format!("the resource of `{name}` is not one: {refused}"),
            )
        })?;
    Ok((canonical, json, resource))
}

/// How long a dispatch waits on the journal: as long as it takes at the top of
/// its call tree, and as a host call is held beneath it.
///
/// **A dispatch nested in a tree is waited on from inside the tree's time.**
/// A plugin waits on it inside `commands.dispatch`, a host call no epoch
/// reaches, and an undo waits on the compensations it dispatches; so each wait
/// of a nested dispatch on the journal — its refusal's entry, the opening of a
/// unit of its own, its entry, the close of that unit, a `journal.verify` — is
/// held to `host_call_deadline_ms`, or to what the tree has left when that is
/// nearer, as [`within`] holds the directory, and one that runs out is the
/// journal not answering: the dispatch is refused, and nothing it changed is
/// kept. Unbounded, a timer's callback whose dispatch opened its unit on a
/// journal that stalled would wait past its tree's deadline, the plugin never
/// killed for it, and the pump and the bus held for as long as the journal
/// stalled.
/// **The top of a tree waits as long as the journal takes**: its waiter is the
/// kernel's caller, which may stop waiting without stopping it, and its entry
/// is the record that the tree ran.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Waits {
    /// When the tree began, for a dispatch nested in it: none at its top.
    nested_in: Option<Instant>,
}

impl Waits {
    /// The waits of the top of a call tree, which last as long as the journal
    /// takes.
    pub(crate) const TOP: Self = Self { nested_in: None };

    /// The waits of a dispatch in the tree that began at `began`: nested in it
    /// when `nested`, and its top otherwise.
    const fn of(nested: bool, began: Instant) -> Self {
        Self {
            nested_in: if nested { Some(began) } else { None },
        }
    }

    /// What the journal answers `wait`, held as these waits are held.
    ///
    /// # Errors
    ///
    /// What the journal refused, and [`nc_journal::Error::Unavailable`] when
    /// it did not answer in time — what it was doing is dropped where it
    /// stood, which a journal append survives: the entry landed whole or not
    /// at all, as a journal that stopped answering would leave it.
    pub(crate) async fn on<T>(
        self,
        wait: impl Future<Output = Result<T, nc_journal::Error>>,
    ) -> Result<T, nc_journal::Error> {
        let Some(began) = self.nested_in else {
            return wait.await;
        };
        let deadline =
            Duration::from_millis(KERNEL.host_call_deadline_ms).min(supervisor::tree_left(began));
        tokio::time::timeout(deadline, wait)
            .await
            .unwrap_or_else(|_| {
                Err(nc_journal::Error::Unavailable(format!(
                    "it did not answer within the {} ms a dispatch nested in a call tree waits",
                    deadline.as_millis()
                )))
            })
    }
}

/// The directory's answer, or its refusal, within the deadline a host call is
/// held to — or what the call tree that began at `began` has left of its own,
/// when that is nearer, as a host call is held (`supervisor::in_time`).
async fn within<T>(
    began: Instant,
    asked: impl Future<Output = Result<T, nc_authz::DirectoryError>>,
) -> Result<T, nc_authz::DirectoryError> {
    let deadline =
        Duration::from_millis(KERNEL.host_call_deadline_ms).min(supervisor::tree_left(began));
    tokio::time::timeout(deadline, asked)
        .await
        .unwrap_or_else(|_| {
            Err(nc_authz::DirectoryError::Unavailable(format!(
                "it did not answer within {} ms",
                deadline.as_millis()
            )))
        })
}

/// Step eight: the decision.
///
/// # Errors
///
/// The refusal, as one about to be journaled: what [`decision`] denied, and a
/// command that cannot be undone, allowed and not confirmed, with the code
/// `[NC-KERN-0006]` and the sentinel
/// `kernel.irreversible_needs_confirmation` — which the journal records as
/// what decided it.
async fn authorize<E: Environment>(
    shared: &Shared<E>,
    asked: &Asked,
    caller: Option<&Caller>,
    began: Instant,
) -> Result<Authorized, Box<Turned>> {
    let (deciding, decided, held) = decision(
        shared,
        began,
        &asked.principal,
        &asked.command.listed().action,
        &asked.resource,
        || redeemed(shared, caller, asked),
    )
    .await
    .map_err(|denied| {
        denial(
            asked,
            denied.code,
            &denied.deciding,
            denied.why,
            denied.decided,
        )
    })?;
    if asked.command.listed().reversibility == Reversibility::Irreversible && !held.confirmed {
        let mut turned = denial(
            asked,
            ErrorCode::KernelUnconfirmed,
            Reason::Unconfirmed.sentinel(),
            "it cannot be undone, and no confirmation the kernel issued for it was redeemed",
            Some(decided),
        );
        turned.reason = Reason::Unconfirmed;
        return Err(turned);
    }
    Ok(Authorized {
        deciding,
        decided,
        held,
    })
}

/// A question the decision point denied.
///
/// What its refusal is made of: its code, what decided it, the kernel's words
/// for why, and what it was decided under when it got that far.
struct Denial {
    code: ErrorCode,
    deciding: String,
    why: &'static str,
    decided: Option<Decided>,
}

/// Whether `principal` may do `action` on `resource` now: what step eight
/// decides of a dispatch, and of an action an undo or a redo authorizes again.
///
/// In order, each refusing what follows: the decision point must have been
/// built, the directory must answer for the realm, the realm must pin a pack
/// this kernel carries, and Cedar decides with the grant and the entities the
/// directory holds — each question asked within a host call's deadline, and
/// within what the call tree that began at `began` has left — and with the
/// facts the kernel vouches for, `held` among them: asked once every question
/// of the directory is answered, so a confirmation is never spent on a
/// dispatch the kernel could not decide. **Not before the directory has
/// answered**: with the entities asked for after it is redeemed, a directory
/// that failed there would take a person's single-use confirmation with the
/// undecided dispatch. **Nor before Cedar's request is built**: Cedar builds
/// its request after `held` is asked, and a request it refuses — an epoch past
/// what its schema holds — is a decision that could not be evaluated
/// (`[NC-POL-0005]`). So `held` answers the serials it found bound beside what
/// they vouch for, and those are spent only once the decision is evaluated,
/// allowed or denied.
///
/// # Errors
///
/// The [`Denial`]: no decision point, `kernel.pdp_unavailable` with the pack's
/// own code; a directory that did not answer, `kernel.directory_unavailable`
/// with `[NC-POL-0006]`; a realm whose profile or pin is not this kernel's
/// pack, `kernel.pack_not_carried` with `[NC-POL-0004]`; and Cedar's denial,
/// with the policies that decided or `kernel.no_permit_matched`.
async fn decision<E: Environment>(
    shared: &Shared<E>,
    began: Instant,
    principal: &Principal,
    action: &str,
    resource: &EntityRef,
    held: impl FnOnce() -> (history::Held, Vec<u64>),
) -> Result<(String, Decided, history::Held), Denial> {
    let unavailable = |_: nc_authz::DirectoryError| Denial {
        code: ErrorCode::PolicyDirectoryUnavailable,
        deciding: sentinel::DIRECTORY_UNAVAILABLE.to_owned(),
        why: "the directory the decision point reads could not answer",
        decided: None,
    };
    let pdp = shared.pdp.as_ref().map_err(|unbuilt| Denial {
        code: unbuilt.code(),
        deciding: sentinel::PDP_UNAVAILABLE.to_owned(),
        why: "the decision point could not be built, so nothing is allowed",
        decided: None,
    })?;
    let realm = shared.realm.as_str();
    let facts_of_the_realm = within(began, shared.directory.realm(realm))
        .await
        .map_err(unavailable)?;
    let decided = |grant| Decided {
        pack: pdp.pack().to_owned(),
        epoch: facts_of_the_realm.epoch,
        grant,
    };
    if !pdp.carries(&facts_of_the_realm) {
        return Err(Denial {
            code: ErrorCode::PolicyPackNotCarried,
            deciding: sentinel::PACK_NOT_CARRIED.to_owned(),
            why: "the realm is held to a pack this kernel does not carry",
            decided: Some(decided(None)),
        });
    }
    let entity = principal.entity();
    let grant = within(began, shared.directory.grant(realm, &entity))
        .await
        .map_err(unavailable)?;
    let holding = grant.as_ref().map(|grant| &grant.entity);
    let slice = within(
        began,
        shared.directory.slice(realm, &entity, resource, holding),
    )
    .await
    .map_err(unavailable)?;
    let (held, bound) = held();
    let facts = Facts {
        now_ms: shared.clock.now_ms(),
        via_agent: false,
        confirmed_by_human: held.confirmed,
        actor_chain_depth: 0,
        policy_epoch: facts_of_the_realm.epoch,
        quorum_met: held.quorum_met,
        grant: grant.as_ref().map(|grant| grant.entity.clone()),
    };
    let decision = pdp.decide(
        &Question {
            principal: &entity,
            action,
            resource,
            facts: &facts,
        },
        &slice,
    );
    if decision.code != Some(ErrorCode::PolicyEvaluationFailed) {
        let mut issued = lock(&shared.issued);
        for serial in bound {
            issued.used(serial);
        }
    }
    let decided = decided(grant.map(|grant| grant.id));
    if !decision.allowed {
        return Err(Denial {
            code: decision.code.unwrap_or(ErrorCode::PolicyDenied),
            deciding: decision.deciding,
            why: "the decision point did not allow it, for this principal, on this resource",
            decided: Some(decided),
        });
    }
    Ok((decision.deciding, decided, held))
}

/// Authorizes `authority` again, now, for `principal` holding `held`: an
/// action a step being undone or redone was allowed, or the authority a timer
/// was set or an event published under, as it comes due.
///
/// **The decision a dispatch meets, with the facts of now** (`NOCTUA.md
/// §14.9`, invariant 6): a grant expired since, an entity no longer theirs or
/// a quorum no longer met refuses what was put off. A denial is journaled in
/// the tree's unit as the decision point journals any — the action, the
/// command and resource it was allowed for, and what decided it — and logged,
/// and the caller is told its code and never the policy: `again` says what was
/// being authorized again, after the command's name.
///
/// # Errors
///
/// The denial, `denied` with its code, as [`decision`] lists
/// them.
pub(crate) async fn reauthorized<E: Environment>(
    shared: &Shared<E>,
    tree: &Arc<Mutex<CallTree>>,
    principal: &Principal,
    authority: &Authority,
    held: history::Held,
    again: &str,
) -> Result<(), Refusal> {
    let Authority {
        command,
        action,
        resource,
    } = authority;
    let began = lock(tree).began();
    let Err(denied) = decision(shared, began, principal, action, resource, || {
        (held, Vec::new())
    })
    .await
    else {
        return Ok(());
    };
    let turned = Turned {
        refusal: Refusal::new(
            RefusalKind::Denied,
            denied.code,
            format!("`{command}` was denied, {again}: {}", denied.why),
        ),
        reason: Reason::Denied,
        // **As the command it names declares**, not `true` for every one, which
        // would log a timer a read set, refused when it fires, as a refused
        // write. A command no longer registered is told by its action.
        mutating: shared
            .command(command)
            .map_or(action != "Read", |admitted| admitted.listed().mutating),
        action: action.clone(),
        resource: resource_of(command, Some(resource), None),
        result: Outcome::Denied,
        deciding: denied.deciding,
        decided: denied.decided,
    };
    Err(turned_away(shared, tree, turned, Waits::TOP).await)
}

/// The denial of `asked` with `code`, decided by `deciding`, telling the
/// caller `why` — and never `deciding` itself.
fn denial(
    asked: &Asked,
    code: ErrorCode,
    deciding: &str,
    why: &str,
    decided: Option<Decided>,
) -> Box<Turned> {
    Box::new(Turned::of(
        &asked.command,
        Refusal::new(
            RefusalKind::Denied,
            code,
            format!("`{}` was denied: {why}", asked.name),
        ),
        Reason::Denied,
        resource_of(&asked.name, Some(&asked.resource), None),
        deciding.to_owned(),
        decided,
    ))
}

/// Whether the caller's confirmation and approval were issued for this
/// dispatch, and the serial of each one that was, which the decision spends.
///
/// **Only at the top of a tree.** A nested dispatch is never confirmed and
/// never approved, whatever the caller holds: what a human confirmed is the one
/// dispatch they saw.
fn redeemed<E: Environment>(
    shared: &Shared<E>,
    caller: Option<&Caller>,
    asked: &Asked,
) -> (history::Held, Vec<u64>) {
    let Some(caller) = caller.filter(|_| asked.depth == 0) else {
        return (history::Held::default(), Vec::new());
    };
    let now_ms = shared.clock.now_ms();
    let principal = &caller.principal;
    let holder = history::owner_of(shared, &asked.command);
    let mut issued = lock(&shared.issued);
    let mut spent = Vec::new();
    let mut bound = |token, serial| {
        let bound = issued.bound(
            token,
            serial,
            principal,
            (&asked.name, holder.as_deref()),
            &asked.canonical,
            now_ms,
        );
        if bound {
            spent.push(serial);
        }
        bound
    };
    let confirmed = caller
        .confirmation
        .as_ref()
        .is_some_and(|confirmation| bound(Token::Confirmation, confirmation.serial));
    let quorum_met = caller
        .approval
        .as_ref()
        .is_some_and(|approval| bound(Token::Approval, approval.serial));
    let held = history::Held {
        confirmed,
        quorum_met,
    };
    (held, spent)
}

/// Why an authorized dispatch did not answer.
///
/// The refusal, the word its entry records it under, and whether the unit it
/// ran in cannot be kept whatever its caller does.
#[derive(Debug)]
pub(crate) struct Failure {
    refusal: Refusal,
    word: &'static str,
    kills: bool,
}

impl Failure {
    /// The failure `refusal`, recorded under `word`, which leaves the unit to
    /// its caller unless the command changed something.
    pub(crate) const fn new(refusal: Refusal, word: &'static str) -> Self {
        Self {
            refusal,
            word,
            kills: false,
        }
    }

    /// The refusal it is answered with, for a handler's own tests.
    #[cfg(test)]
    pub(crate) const fn refusal(&self) -> &Refusal {
        &self.refusal
    }
}

/// What running an authorized command came to.
enum Executed {
    /// It ran, and answered or failed.
    Ran(Result<String, Failure>),
    /// It ran once its entry had landed, journaled before it — what no unit
    /// can hold is done only once it is journaled — and answered or failed.
    Entered(Result<String, Failure>),
    /// The journal did not take the entry written before it, so it never ran:
    /// the refusal, and why its unit is not kept.
    Withheld((Refusal, Cause)),
    /// Its plugin is running already, further up this tree.
    Busy,
}

/// Why a unit was aborted, as the log names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cause {
    /// Its command refused, trapped or answered what is not a payload.
    Failed,
    /// A command dispatched inside it did, having written.
    Nested,
    /// A plugin aborted a unit it had begun inside it.
    Explicit,
    /// A plugin let a unit it had begun inside it go without committing it,
    /// or returned with it open.
    Dropped,
    /// The store refused its commit.
    Conflict,
    /// The journal did not take its entry or its close.
    Journal,
    /// The journal answered its close with an error and could not then say
    /// whether it took it.
    JournalUnknown,
}

impl Cause {
    /// The variant the log names it as.
    pub(crate) fn variant(self) -> nc_log::Variant {
        match self {
            Self::Failed => nc_log::variant!("failed"),
            Self::Nested => nc_log::variant!("nested"),
            Self::Explicit => nc_log::variant!("explicit"),
            Self::Dropped => nc_log::variant!("dropped"),
            Self::Conflict => nc_log::variant!("conflict"),
            Self::Journal => nc_log::variant!("journal"),
            Self::JournalUnknown => nc_log::variant!("journal_unknown"),
        }
    }
}

/// Steps nine to thirteen: the plugin's state, the unit, the command, the
/// entry and the close.
async fn run<E: Environment>(
    shared: &Arc<Shared<E>>,
    tree: &Arc<Mutex<CallTree>>,
    asked: &Asked,
    authorized: &Authorized,
) -> Result<String, Refusal> {
    if let Owner::Plugin(slot) = asked.command.listed().owner
        && shared.stopped(slot)
    {
        let failure = quarantined(&asked.name);
        refused_in_the_log(
            shared,
            &failure.refusal,
            Reason::Quarantined,
            asked.command.listed().mutating,
        );
        return journaled(shared, tree, asked, authorized, Err(failure))
            .await
            .map_err(|(refusal, _)| refusal);
    }
    // **A command that changes state where the tree holds no unit opens
    // one**: the top of a tree, and a dispatch from a timer's callback that
    // began none, each of which is a step of its own.
    let opens = asked.command.listed().mutating && lock(tree).unit().is_none();
    let opened = if opens {
        Some(opened(shared, tree, asked).await?)
    } else {
        None
    };
    let outcome = executed(shared, tree, asked, authorized).await;
    match opened {
        Some(unit) => closed(shared, tree, asked, unit, outcome).await,
        None => outcome.map_err(|(refusal, _)| refusal),
    }
}

/// Step ten: the unit a top-level dispatch of a command that changes state
/// runs in, opened in the journal before the command runs — and the unit of a
/// step of its own a dispatch from a timer's callback that began none opens,
/// which waits on the journal as that nested dispatch does ([`Waits`]).
///
/// # Errors
///
/// `unavailable`, with the journal's own code, when the journal does not take
/// the opening, or does not answer in time — and then the command never runs.
async fn opened<E: Environment>(
    shared: &Shared<E>,
    tree: &Arc<Mutex<CallTree>>,
    asked: &Asked,
) -> Result<Snowflake, Refusal> {
    let opened = asked
        .waits
        .on(async {
            shared
                .journal
                .lock()
                .await
                .open_unit(asked.name.clone())
                .await
        })
        .await;
    match opened {
        Ok(unit) => {
            lock(tree).unit = Some(Ambient::new(unit).waiting(asked.waits));
            Ok(unit)
        }
        Err(error) => {
            let refusal = Refusal::new(
                RefusalKind::Unavailable,
                error.code(),
                format!(
                    "the journal did not take the opening of `{}`'s unit, so it did not run",
                    asked.name
                ),
            );
            refused_in_the_log(shared, &refusal, Reason::JournalUnavailable, true);
            Err(refusal)
        }
    }
}

/// Steps eleven and twelve: the command, run in a frame of its own, and its
/// entry in the journal.
///
/// **A command that fails having written, published, set or cancelled dooms
/// the unit it ran in**, and so does one that trapped: a nested command's
/// writes, events and timers are the tree's, and what a failed one staged or
/// held for later cannot be kept. One that fails having changed nothing
/// leaves the unit as it was, and its caller decides.
async fn executed<E: Environment>(
    shared: &Arc<Shared<E>>,
    tree: &Arc<Mutex<CallTree>>,
    asked: &Asked,
    authorized: &Authorized,
) -> Result<String, (Refusal, Cause)> {
    let mutating = asked.command.listed().mutating;
    let (before, noted) = {
        let mut tree = lock(tree);
        tree.frames.push(mutating);
        let noted = tree
            .unit_mut()
            .map_or(Ok(()), |unit| unit.leaving(asked.command.listed()));
        (tree.unit().map(Ambient::changes), noted)
    };
    let executed = match (noted, asked.command.listed().owner) {
        // **Reached by no dispatch of this build**, and held where it is
        // decided (`Ambient::leaving`'s own test): a command that leaves the
        // machine runs only at the top of its tree, where its unit holds no
        // note yet — nested, one that cannot be undone is never confirmed and
        // the kernel's change to the realm never approved, so either is denied
        // before it gets here.
        (Err(refusal), _) => Executed::Ran(Err(Failure::new(refusal, "bound-reached"))),
        (Ok(()), Owner::Kernel) => builtin(shared, tree, asked, authorized).await,
        (Ok(()), Owner::Plugin(slot)) => plugin(shared, tree, slot, asked, authorized).await,
    };
    {
        let mut tree = lock(tree);
        tree.frames.pop();
        if let Some(unit) = tree.unit_mut() {
            match &executed {
                Executed::Ran(Err(failure)) | Executed::Entered(Err(failure)) => {
                    let wrote = before != Some(unit.changes());
                    if failure.kills || wrote {
                        unit.doom(failure.refusal.clone());
                    }
                }
                Executed::Ran(Ok(_)) | Executed::Entered(Ok(_)) if mutating => {
                    let note = history::Note::Authorized {
                        action: asked.command.listed().action.clone(),
                        entity: asked.resource.clone(),
                    };
                    unit.ran(note.written());
                }
                Executed::Ran(Ok(_))
                | Executed::Entered(Ok(_))
                | Executed::Withheld(_)
                | Executed::Busy => {}
            }
        }
    }
    let ran = match executed {
        Executed::Ran(ran) => ran,
        Executed::Entered(ran) => return ran.map_err(|failure| (failure.refusal, Cause::Failed)),
        Executed::Withheld(refused) => return Err(refused),
        Executed::Busy => {
            let refusal = Refusal::new(
                RefusalKind::Denied,
                ErrorCode::KernelReentrant,
                format!(
                    "`{}` is a command of a plugin running already further up this call tree, \
                     which may not be entered again until it returns",
                    asked.name
                ),
            );
            let turned = Turned::of(
                &asked.command,
                refusal,
                Reason::Reentrant,
                resource_of(&asked.name, Some(&asked.resource), None),
                Reason::Reentrant.sentinel().to_owned(),
                Some(authorized.decided.clone()),
            );
            return Err((
                turned_away(shared, tree, turned, asked.waits).await,
                Cause::Failed,
            ));
        }
    };
    journaled(shared, tree, asked, authorized, ran).await
}

/// Step twelve: the entry of an authorized dispatch, and the answer it lets
/// through.
///
/// # Errors
///
/// The command's own failure; and `unavailable`, with the journal's code, when
/// the journal does not take the entry, or does not answer in time ([`Waits`])
/// — the answer is then withheld, and the unit the dispatch ran in, if any, is
/// doomed.
async fn journaled<E: Environment>(
    shared: &Shared<E>,
    tree: &Arc<Mutex<CallTree>>,
    asked: &Asked,
    authorized: &Authorized,
    ran: Result<String, Failure>,
) -> Result<String, (Refusal, Cause)> {
    let failure = ran.as_ref().err().map(|failure| failure.word);
    let mut resource = resource_of(&asked.name, Some(&asked.resource), failure);
    let reversed = if ran.is_ok() && history::is_history(&asked.name) {
        lock(tree)
            .unit()
            .and_then(Ambient::reversing)
            .map(history::recorded_of)
    } else {
        None
    };
    if let (Some(reversed), Value::Object(fields)) = (reversed, &mut resource) {
        fields.extend(reversed);
    }
    let record = Record {
        realm: shared.realm.clone(),
        principal: asked.principal.id().clone(),
        actor_chain: Vec::new(),
        action: asked.command.listed().action.clone(),
        resource,
        result: if ran.is_ok() {
            Outcome::Allowed
        } else {
            Outcome::Failed
        },
        deciding_policy: Some(authorized.deciding.clone()),
        policy_pack: Some(authorized.decided.pack.clone()),
        policy_epoch: Some(authorized.decided.epoch),
        grant_used: authorized.decided.grant.clone(),
        work_unit: lock(tree).unit().map(Ambient::unit),
        recorded_at: shared.clock.now_ms(),
    };
    let entered = asked
        .waits
        .on(async { shared.journal.lock().await.record(record).await })
        .await;
    if let Err(error) = entered {
        let refusal = Refusal::new(
            RefusalKind::Unavailable,
            error.code(),
            format!(
                "the journal did not take the entry of `{}`, so its answer is withheld",
                asked.name
            ),
        );
        {
            let mut tree = lock(tree);
            tree.entry_refused = true;
            if let Some(unit) = tree.unit_mut() {
                unit.doom(refusal.clone());
            }
        }
        refused_in_the_log(
            shared,
            &refusal,
            Reason::JournalUnavailable,
            asked.command.listed().mutating,
        );
        return Err((refusal, Cause::Journal));
    }
    ran.map_err(|failure| (failure.refusal, Cause::Failed))
}

/// Step thirteen: the unit of a top-level dispatch, kept or let go.
///
/// # Errors
///
/// The command's own refusal when it failed; `[NC-KERN-0008]`, in the case of
/// what doomed it, when a command inside it failed having written or trapped;
/// and whatever closing the unit refuses (`crate::units`). A unit let go names
/// the compensations registered in it, none of which runs (`units::not_run`).
async fn closed<E: Environment>(
    shared: &Shared<E>,
    tree: &Arc<Mutex<CallTree>>,
    asked: &Asked,
    unit: Snowflake,
    outcome: Result<String, (Refusal, Cause)>,
) -> Result<String, Refusal> {
    let ambient = lock(tree).unit.take();
    let Some(ambient) = ambient else {
        units::abort(shared, unit, Cause::Failed, asked.waits).await;
        return Err(Refusal::new(
            RefusalKind::Internal,
            ErrorCode::KernelUnitAborted,
            format!("`{}` lost the unit it ran in", asked.name),
        ));
    };
    match (outcome, ambient.doomed()) {
        (Err((refusal, cause)), _) => {
            units::let_go(shared, &ambient, cause).await;
            Err(units::not_run(refusal, &ambient))
        }
        (Ok(_), Some(doomed)) => {
            let doomed_for = ambient.doomed_for().unwrap_or(Cause::Nested);
            let because = match doomed_for {
                Cause::Explicit | Cause::Dropped => "because a plugin let a unit it began go",
                _ => "because a command inside it failed",
            };
            let refusal = Refusal::new(
                doomed.kind(),
                ErrorCode::KernelUnitAborted,
                format!(
                    "`{}` answered, and its unit was not kept, {because}: {doomed}",
                    asked.name
                ),
            );
            units::let_go(shared, &ambient, doomed_for).await;
            Err(units::not_run(refusal, &ambient))
        }
        (Ok(answer), None) => units::close(shared, ambient, &asked.principal, &asked.name)
            .await
            .map(|()| answer),
    }
}

/// The failure of a dispatch to a plugin killed in this call tree when its
/// time was up, which the next call tree starts again.
fn not_back_yet(name: &str) -> Failure {
    Failure {
        refusal: Refusal::new(
            RefusalKind::Unavailable,
            ErrorCode::KernelDeadline,
            format!(
                "`{name}` is a command of a plugin killed when its call tree's time was up: \
                 it is started again before the next call tree runs"
            ),
        ),
        word: "not-back-yet",
        kills: false,
    }
}

/// The failure of a dispatch to a plugin that is quarantined.
fn quarantined(name: &str) -> Failure {
    Failure {
        refusal: Refusal::new(
            RefusalKind::Unavailable,
            ErrorCode::PluginQuarantined,
            format!(
                "`{name}` is a command of a plugin that is quarantined: it is not run until it \
                 is released"
            ),
        ),
        word: "quarantined",
        kills: false,
    }
}

/// How many characters of what a plugin said, or of what stopped it, a
/// refusal repeats.
const QUOTED: usize = 256;

/// Runs a command the kernel itself implements, in the tree's own unit:
/// verifying the journal, pruning the local tables, and the history's undo
/// and redo (`crate::builtin`).
///
/// **A command the registry gives the kernel has a handler**, which
/// `crate::builtin`'s test holds both ways; one without would be answered as
/// a command nothing runs, and never as one that ran.
async fn builtin<E: Environment>(
    shared: &Arc<Shared<E>>,
    tree: &Arc<Mutex<CallTree>>,
    asked: &Asked,
    authorized: &Authorized,
) -> Executed {
    use crate::builtin::{self, Builtin};

    Executed::Ran(match Builtin::named(&asked.name) {
        Some(Builtin::Verify) => builtin::verified(shared, &asked.json, asked.waits).await,
        Some(Builtin::Prune) => {
            return pruned_once_journaled(shared, tree, asked, authorized).await;
        }
        Some(Builtin::Undo) => history::undo(shared, tree, &asked.principal, authorized.held).await,
        Some(Builtin::Redo) => history::redo(shared, tree, &asked.principal, authorized.held).await,
        None => Err(Failure::new(
            Refusal::new(
                RefusalKind::NotFound,
                ErrorCode::KernelUnknownCommand,
                format!(
                    "`{}` is registered as the kernel's, and the kernel runs no such command",
                    asked.name
                ),
            ),
            "unknown-command",
        )),
    })
}

/// `limits.prune`, which deletes nothing until its entry has landed.
///
/// **What no unit can hold is journaled before it is done.** Every other
/// command's changes are staged in its unit and go with it when the journal
/// does not take its entry; the pruner's deletions are outside every unit.
/// So a prune its rehearsal did not refuse (`crate::builtin::pruned`) is
/// journaled as allowed first, and the pruner deletes only then: a journal
/// that does not take that entry leaves every row where it was, the answer
/// withheld as any refused entry's is. What the entry records is the
/// decision; whether the run then finished is what the unit's close says.
async fn pruned_once_journaled<E: Environment>(
    shared: &Arc<Shared<E>>,
    tree: &Arc<Mutex<CallTree>>,
    asked: &Asked,
    authorized: &Authorized,
) -> Executed {
    match crate::builtin::pruned(shared, &asked.json).await {
        Err(failure) => Executed::Ran(Err(failure)),
        Ok(crate::builtin::Prune::Answered(answer)) => Executed::Ran(Ok(answer)),
        Ok(crate::builtin::Prune::Deletes(target, run)) => {
            match journaled(shared, tree, asked, authorized, Ok(String::new())).await {
                Ok(_) => Executed::Entered(crate::builtin::deleted(target, run).await),
                Err(refused) => Executed::Withheld(refused),
            }
        }
    }
}

/// Runs a plugin's command, in a frame of the tree, in the plugin's own store.
///
/// **Checked out for the call**: the store is taken out of its slot while the
/// command runs, so a dispatch into the same plugin further down the tree finds
/// the slot empty and is refused, and a kill leaves nothing in the slot to
/// reuse. A call that ends in an error kills the plugin — its store is dropped
/// — and the supervisor starts it again or quarantines it (`crate::supervisor`).
async fn plugin<E: Environment>(
    shared: &Arc<Shared<E>>,
    tree: &Arc<Mutex<CallTree>>,
    slot: PluginSlot,
    asked: &Asked,
    authorized: &Authorized,
) -> Executed {
    let (mut instance, limits) = match shared.check_out(slot) {
        Ok(checked_out) => checked_out,
        Err(supervisor::Out::Busy) => return Executed::Busy,
        Err(supervisor::Out::Stopped) => return Executed::Ran(Err(quarantined(&asked.name))),
        Err(supervisor::Out::Killed) => return Executed::Ran(Err(not_back_yet(&asked.name))),
    };
    instance.enter(Frame::new(
        Arc::downgrade(shared),
        Arc::clone(tree),
        slot,
        Call::Command {
            mutating: asked.command.listed().mutating,
        },
        Authority {
            command: asked.name.clone(),
            action: asked.command.listed().action.clone(),
            resource: asked.resource.clone(),
        },
        authorized.held.confirmed,
    ));
    let called = crate::bindings::run(&mut instance, &asked.name, &asked.canonical, &limits).await;
    left(shared, slot, &mut instance);
    match called {
        Ok(answer) => {
            shared.check_in(slot, instance);
            Executed::Ran(answered(&asked.name, answer))
        }
        Err(stopped) => {
            let kill = supervisor::Kill::of(&stopped);
            let why = crate::registry::quoted_up_to(&stopped.root_cause().to_string(), QUOTED);
            drop(instance);
            let began = lock(tree).began();
            let back = supervisor::killed(shared, slot, kill, began).await;
            Executed::Ran(Err(killed(&asked.name, kill, &why, back)))
        }
    }
}

/// Takes the frame back from the plugin in `slot` once a call into it has
/// returned — a command, a callback, or `activate`, which runs in no frame.
///
/// It logs each capability the plugin was refused, each code a host call of
/// it was refused with for where it ran or whose name it used, and each reason
/// a fetch of it was refused for, the first time each was, once.
pub(crate) fn left<E: Environment>(
    shared: &Shared<E>,
    slot: PluginSlot,
    instance: &mut crate::bindings::Instance<E>,
) {
    for denial in instance.leave() {
        shared.log.record(&nc_log::events::KernelCapabilityDenied {
            slot: logged_slot(slot),
            capability: crate::logging::capability(denial.capability()),
            code: denial.code(),
        });
    }
    for code in instance.calls_refused() {
        shared.log.record(&nc_log::events::KernelHostCallRefused {
            slot: logged_slot(slot),
            code,
        });
    }
    for (reason, code) in instance.fetches_refused() {
        shared.log.record(&nc_log::events::NetRefused {
            slot: logged_slot(slot),
            reason: reason.variant(),
            code,
        });
    }
}

/// The number a slot is logged as.
pub(crate) fn logged_slot(slot: PluginSlot) -> u32 {
    u32::try_from(slot.index()).unwrap_or(u32::MAX)
}

/// The failure of a command whose plugin was killed for `kill`, said as
/// whether it came back.
///
/// **In the kill's own code when it did, or will before the next call tree
/// runs**, and `[NC-PLUG-0007]` when it is quarantined: the code is what a
/// caller decides by, and a plugin that is not run again is not one worth
/// retrying.
fn killed(name: &str, kill: supervisor::Kill, why: &str, back: supervisor::Back) -> Failure {
    let (code, back) = match back {
        supervisor::Back::Restarted => (kill.code(), "was killed and restarted"),
        supervisor::Back::Later => (
            kill.code(),
            "was killed, and is started again before the next call tree runs",
        ),
        supervisor::Back::Quarantined => (
            ErrorCode::PluginQuarantined,
            "was killed and is quarantined: it is not run again until it is released",
        ),
    };
    Failure {
        refusal: Refusal::new(
            RefusalKind::Internal,
            code,
            format!("`{name}` {}, and its plugin {back}: {why}", kill.said()),
        ),
        word: kill.word(),
        kills: true,
    }
}

/// What a plugin's command answered, held to being a payload: canonical JSON
/// within `payload_bytes`.
fn answered(name: &str, answer: Result<String, (RefusalKind, String)>) -> Result<String, Failure> {
    match answer {
        Ok(text) => {
            let canonical = nc_canon::parse(&text).is_ok_and(|value| value.to_canonical() == text);
            if canonical && u64::try_from(text.len()).unwrap_or(u64::MAX) <= KERNEL.payload_bytes {
                Ok(text)
            } else {
                Err(Failure {
                    refusal: Refusal::new(
                        RefusalKind::Internal,
                        ErrorCode::PluginMalformedResult,
                        format!(
                            "`{name}` answered {} byte(s) that are not a payload: canonical \
                             JSON of at most {} bytes",
                            text.len(),
                            KERNEL.payload_bytes
                        ),
                    ),
                    word: "malformed-result",
                    kills: false,
                })
            }
        }
        Err((kind, why)) => Err(Failure {
            refusal: Refusal::new(
                kind,
                ErrorCode::PluginCommandRefused,
                format!(
                    "`{name}` refused, saying {}",
                    crate::registry::quoted_up_to(&why, QUOTED)
                ),
            ),
            word: "refused",
            kills: false,
        }),
    }
}

#[cfg(test)]
mod tests {
    use nc_catalog::{ErrorCode, KERNEL};

    use nc_authz::{Principal, PrincipalKind};

    use super::{Issued, Reason, RefusalKind, Token, answered, digest, expired, resource_of};

    /// The user `ana`, the user `bruno` and the device `ana`: of the world the
    /// policy cases are about, with a device of ana's own identifier added.
    fn principals() -> (Principal, Principal, Principal) {
        let mut world: serde_json::Value =
            serde_json::from_str(include_str!("../../../policy/world.json"))
                .expect("the world is JSON");
        world
            .as_array_mut()
            .expect("the world is a list")
            .push(serde_json::json!({
                "uid": {"type": "Device", "id": "ana"},
                "attrs": {
                    "owner": {"__entity": {"type": "User", "id": "ana"}},
                    "node_key": "0011223344556677",
                    "attested": true
                },
                "parents": [{"type": "Realm", "id": "home"}]
            }));
        let pdp = nc_authz::Pdp::embedded("family").expect("the family pack is carried");
        let directory = nc_authz::MemoryDirectory::from_world(&world.to_string(), &pdp)
            .expect("the world loads");
        let of = |kind, name| {
            directory
                .principal(kind, name)
                .expect("the world holds the principal")
        };
        (
            of(PrincipalKind::User, "ana"),
            of(PrincipalKind::User, "bruno"),
            of(PrincipalKind::Device, "ana"),
        )
    }

    #[test]
    fn a_token_is_redeemed_once_for_the_dispatch_it_was_issued_for() {
        let mut issued = Issued::default();
        let (ana, bruno, anas_device) = principals();
        assert_eq!(ana.id(), anas_device.id(), "one identifier, two principals");
        let held = ("a.do", Some("noctua:a"));
        let serial = issued.issue(Token::Confirmation, &ana, held, r#"{"x":1}"#, 1_000);
        for (token, principal, command, args) in [
            (Token::Approval, &ana, held, r#"{"x":1}"#),
            (Token::Confirmation, &bruno, held, r#"{"x":1}"#),
            (Token::Confirmation, &anas_device, held, r#"{"x":1}"#),
            (
                Token::Confirmation,
                &ana,
                ("a.undo", Some("noctua:a")),
                r#"{"x":1}"#,
            ),
            (Token::Confirmation, &ana, held, r#"{"x":2}"#),
            // The same name, held now by another identity, or by none.
            (
                Token::Confirmation,
                &ana,
                ("a.do", Some("noctua:b")),
                r#"{"x":1}"#,
            ),
            (Token::Confirmation, &ana, ("a.do", None), r#"{"x":1}"#),
        ] {
            assert!(
                !issued.bound(token, serial, principal, command, args, 1_001),
                "{token:?} {principal:?} {command:?} {args}"
            );
        }
        let its_own = |issued: &mut Issued, now_ms| {
            issued.bound(
                Token::Confirmation,
                serial,
                &ana,
                held,
                r#"{"x":1}"#,
                now_ms,
            )
        };
        assert!(
            its_own(&mut issued, 1_001),
            "a token presented with another dispatch is kept for its own"
        );
        assert!(
            its_own(&mut issued, 1_001),
            "a token bound is held until a decision spends it"
        );
        issued.used(serial);
        assert!(!its_own(&mut issued, 1_002), "redeemed twice");
        assert!(!issued.bound(Token::Confirmation, 99, &ana, held, "{}", 1_002));
    }

    /// A question the decision point asks the directory is held to what the
    /// call tree has left when that is nearer than a host call's deadline.
    ///
    /// Held to a host call's deadline alone, the directory would outlast a tree
    /// near its end.
    #[tokio::test(start_paused = true)]
    async fn a_question_of_the_directory_is_held_to_what_its_tree_has_left() {
        let left = std::time::Duration::from_secs(1);
        let began = std::time::Instant::now()
            .checked_sub(
                std::time::Duration::from_millis(KERNEL.dispatch_deadline_ms).saturating_sub(left),
            )
            .expect("the machine has run longer than a call tree may");
        let started = tokio::time::Instant::now();
        let asked = super::within(
            began,
            core::future::pending::<Result<(), nc_authz::DirectoryError>>(),
        )
        .await;
        assert!(
            matches!(asked, Err(nc_authz::DirectoryError::Unavailable(_))),
            "{asked:?}"
        );
        assert!(
            started.elapsed() <= left,
            "the directory was waited on {:?}, past the {left:?} its tree had left",
            started.elapsed()
        );
    }

    /// A token's life is held on the monotonic clock as well as the kernel's,
    /// and a kernel's clock that reads before its issue reads it as expired.
    ///
    /// **Not on the wall clock alone**: measured on it alone, with a saturating
    /// difference, a confirmation issued before the clock steps back an hour
    /// would be redeemed `confirmation_ttl_ms` and more after it, every reading
    /// before the issue counting as no time at all.
    #[test]
    fn a_token_lives_no_longer_than_its_time_whatever_the_wall_clock_does() {
        let ttl = KERNEL.confirmation_ttl_ms;
        let (at, now) = (3_600_000 + 5_000, std::time::Instant::now());
        assert!(
            expired(at, now, at - 3_600_000 + ttl + 1),
            "a clock stepped back an hour, and run on past the token's life"
        );
        assert!(expired(at, now, at - 1), "a clock one millisecond behind");
        let long_ago = now
            .checked_sub(std::time::Duration::from_millis(ttl + 1))
            .expect("the machine has run longer than a token lives");
        assert!(
            expired(at, long_ago, at),
            "a token issued longer ago than it lives, on a wall clock that stood still"
        );
        let mut issued = Issued::default();
        let (ana, _, _) = principals();
        let serial = issued.issue(Token::Confirmation, &ana, ("a.do", None), "{}", at);
        assert!(!issued.bound(
            Token::Confirmation,
            serial,
            &ana,
            ("a.do", None),
            "{}",
            at - 3_600_000 + ttl + 1
        ));
    }

    #[test]
    fn a_token_past_its_time_is_forgotten_rather_than_redeemed() {
        let mut issued = Issued::default();
        let (ana, _, _) = principals();
        let at = 5_000;
        let serial = issued.issue(Token::Approval, &ana, ("a.do", None), "{}", at);
        let now = std::time::Instant::now();
        assert!(!expired(at, now, at + KERNEL.confirmation_ttl_ms));
        assert!(expired(at, now, at + KERNEL.confirmation_ttl_ms + 1));
        assert!(
            !issued.bound(
                Token::Approval,
                serial,
                &ana,
                ("a.do", None),
                "{}",
                at + KERNEL.confirmation_ttl_ms + 1
            ),
            "redeemed after its time"
        );
        let kept = issued.issue(Token::Approval, &ana, ("a.do", None), "{}", at);
        let later = issued.issue(
            Token::Approval,
            &ana,
            ("a.do", None),
            "{}",
            at + KERNEL.confirmation_ttl_ms + 1,
        );
        assert!(
            !issued.outstanding.contains_key(&kept),
            "what expired is forgotten when the next is issued"
        );
        assert!(issued.outstanding.contains_key(&later));
        assert_ne!(digest("{}"), digest("{ }"));
    }

    #[test]
    fn a_malformed_name_is_never_written_into_the_journal() {
        let named = resource_of("a.b", None, Some("refused")).to_canonical();
        assert_eq!(named, r#"{"command":"a.b","failure":"refused"}"#);
        let refused = resource_of("A Name a person chose", None, None).to_canonical();
        assert_eq!(refused, r#"{"refused":"malformed-name"}"#);
    }

    #[test]
    fn every_sentinel_the_kernel_decides_with_begins_with_the_prefix_no_policy_may_use() {
        for reason in [
            Reason::UnknownCommand,
            Reason::MalformedName,
            Reason::MalformedArguments,
            Reason::Denied,
            Reason::Unconfirmed,
            Reason::ReadOnly,
            Reason::TopLevelOnly,
            Reason::TooDeep,
            Reason::Deadline,
            Reason::TooManyDispatches,
            Reason::Reentrant,
            Reason::Quarantined,
            Reason::JournalUnavailable,
        ] {
            assert!(reason.sentinel().starts_with("kernel."), "{reason:?}");
        }
    }

    #[test]
    fn an_answer_is_a_payload_or_a_failure_of_its_plugin() {
        assert_eq!(
            answered("a.b", Ok(r#"{"a":1}"#.to_owned())).ok().as_deref(),
            Some(r#"{"a":1}"#)
        );
        for text in [r#"{"b":1,"a":2}"#, "not json", "{ }"] {
            let failed = answered("a.b", Ok(text.to_owned())).expect_err(text);
            assert_eq!(
                failed.refusal.code(),
                ErrorCode::PluginMalformedResult,
                "{text}"
            );
            assert!(!failed.kills);
        }
        let large = format!(
            "\"{}\"",
            "a".repeat(usize::try_from(KERNEL.payload_bytes).expect("fits"))
        );
        assert!(answered("a.b", Ok(large)).is_err(), "past the bound");
        let refused = answered("a.b", Err((RefusalKind::Conflict, "x".repeat(1_000))))
            .expect_err("the plugin refused");
        assert_eq!(refused.refusal.kind(), RefusalKind::Conflict);
        assert_eq!(refused.refusal.code(), ErrorCode::PluginCommandRefused);
        assert!(
            refused.refusal.sentence().len() < 400,
            "what a plugin said is cut before it is repeated"
        );
    }
}
