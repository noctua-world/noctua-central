//! Timers: a plugin woken after a while, or at an interval, as the person it
//! was set for.
//!
//! **Set within bounds, by a plugin that holds few.** `timers.after` waits at
//! least a millisecond and `timers.every` at least `timer_min_interval_ms`,
//! neither longer than `timer_max_delay_ms`; a tag is at most [`TAG_BYTES`]
//! bytes, and nothing a bound refuses is set (`[NC-VAL-0001]`). A plugin holds
//! at most `timers_per_plugin` at once, the ones a unit holds for it counted
//! (`[NC-PLUG-0017]`). Its identifier is the kernel's, from one counter that
//! never goes back, so no plugin chooses or reuses one; and a cancel naming a
//! timer the plugin does not hold — another's, or none — is not found
//! (`[NC-PLUG-0019]`), the one answer for both.
//!
//! **Held with its unit, like an event.** A timer set where the call tree
//! holds a unit of work is armed when the unit is kept, and a cancel there
//! takes effect then: a unit let go leaves the timers as they were. A command
//! that fails having written, published, set or cancelled dooms the unit, so
//! a caller that goes on past the failure keeps neither the timer it set nor
//! the cancel it held. **And held with its tree where no unit is** — a
//! command that only reads, a callback — armed or cancelled once the tree
//! ends, and not at all when the journal refused the entry of any of its
//! dispatches, so a read whose answer was withheld leaves no timer behind
//! (`crate::units::Later`).
//!
//! **Fired by the pump, as its setter, decided again.** The clock is the
//! kernel's injectable one. The pump fires every timer due, oldest due first,
//! before it delivers events: a one-shot is taken out and a repeating one moved
//! on by its interval from now, before its callback runs, and the decision
//! point decides the authority it was set under again, now — refused, the
//! timer is cancelled. Its `on-timer` runs in a call tree of its own for the
//! principal that set it, never confirmed (`crate::pump`).
//!
//! **Fired where it was set, as far as a change goes.** A timer set where only
//! reading is allowed — in a command that only reads, beneath one, or in an
//! event's callback — fires where only reading is: no command its callback
//! dispatches may change state, and it begins no unit. So putting a change off
//! is no way out of the frame that would have refused it, whose authority the
//! timer carries and is decided again under.
//!
//! **A plugin's timers are its slot's.** They outlive its kills, since a
//! restarted plugin is the same plugin; a quarantined one's are held, not
//! fired, until it is released; and an unloaded one's are cancelled.
//!
//! What this answers (`NOCTUA.md §2.4`): a plugin waking itself without end or
//! in a tight loop, filling the kernel with timers, cancelling another's,
//! running on as a person whose authority was withdrawn, a timer set by a
//! change that was not kept, and a frame that may only read changing state
//! later through a timer it set. What it does not answer: timers across a
//! restart of the kernel itself, which keeps none on disk in this build; a
//! timer that comes due while the pump is not run, which fires once at the
//! next; and a timer set by a command that only reads and then trapped, which
//! is armed when its tree ends and only reads when it fires.

use std::collections::BTreeMap;

use nc_authz::Principal;
use nc_catalog::{ErrorCode, KERNEL};

use crate::dispatch::{Authority, Frame, Refusal, RefusalKind, lock};
use crate::{Environment, PluginSlot, Shared};

/// How many bytes of text a timer's tag may be: what the plugin is handed back
/// when it fires, and kept in the kernel until then.
pub(crate) const TAG_BYTES: usize = 64;

/// One timer, armed or held by a unit.
#[derive(Debug, Clone)]
pub(crate) struct Timer {
    /// The plugin that set it.
    pub(crate) owner: PluginSlot,
    /// What the plugin is handed when it fires.
    pub(crate) tag: String,
    /// When it is next due, on the kernel's clock.
    pub(crate) due_ms: u64,
    /// The interval it repeats at, when it repeats.
    pub(crate) every_ms: Option<u64>,
    /// Who the setting call tree ran for, whom it fires for.
    pub(crate) principal: Principal,
    /// What the setting frame ran under, which is decided again when it fires.
    pub(crate) authority: Authority,
    /// Whether the setting frame let a change run beneath it, which its
    /// callback is held to: a timer set where only reading is allowed fires
    /// where only reading is.
    pub(crate) may_change: bool,
}

/// Refuses a timer of `ms`, repeating when `every`, tagged `tag`, unless every
/// bound holds.
///
/// # Errors
///
/// A refusal, `invalid` with `[NC-VAL-0001]`, naming the
/// bound it is past.
pub(crate) fn judged(ms: u64, every: bool, tag: &str) -> Result<(), Refusal> {
    let least = if every {
        KERNEL.timer_min_interval_ms
    } else {
        1
    };
    let out_of_range = |why: String| {
        Err(Refusal::new(
            RefusalKind::Invalid,
            ErrorCode::ValidationOutOfRange,
            why,
        ))
    };
    if !(least..=KERNEL.timer_max_delay_ms).contains(&ms) {
        return out_of_range(format!(
            "a timer {} {ms} ms, and it may be set from {least} to {} ms",
            if every { "repeating every" } else { "waiting" },
            KERNEL.timer_max_delay_ms
        ));
    }
    if tag.len() > TAG_BYTES {
        return out_of_range(format!(
            "a timer's tag is {} bytes, past the {TAG_BYTES} one may be",
            tag.len()
        ));
    }
    Ok(())
}

/// The refusal of a cancel naming a timer the plugin does not hold.
pub(crate) fn not_held(id: u64) -> Refusal {
    Refusal::new(
        RefusalKind::NotFound,
        ErrorCode::PluginNotFound,
        format!("the plugin holds no timer {id}"),
    )
}

/// The refusal of one more timer than a plugin may hold.
pub(crate) fn no_room() -> Refusal {
    Refusal::new(
        RefusalKind::Unavailable,
        ErrorCode::PluginBoundReached,
        format!(
            "a plugin holds at most {} timers, those units of work hold for it included, and \
             this one would be one more",
            KERNEL.timers_per_plugin
        ),
    )
}

/// Sets a timer from `frame`, of `ms`, repeating when `every`, tagged `tag`,
/// for the principal its tree runs for and under the authority it runs under,
/// answering its identifier.
///
/// Held by the tree's unit when it holds one, and by the tree until it ends
/// when it does not; and where the tree only reads, it is a timer whose
/// callback only reads.
///
/// # Errors
///
/// What [`judged`] refuses, and [`no_room`] when the plugin holds as many
/// timers as it may.
pub(crate) fn set<E: Environment>(
    shared: &Shared<E>,
    frame: &Frame<E>,
    ms: u64,
    every: bool,
    tag: String,
) -> Result<u64, Refusal> {
    judged(ms, every, &tag)?;
    let now_ms = nc_ids::Clock::now_ms(&shared.clock);
    let owner = frame.slot();
    let mut timers = lock(&shared.timers);
    let mut tree = frame.tree();
    let held = timers
        .held_by(owner)
        .saturating_add(tree.timers_held_for(owner));
    if u64::try_from(held).unwrap_or(u64::MAX) >= u64::from(KERNEL.timers_per_plugin) {
        return Err(no_room());
    }
    let id = timers.mint();
    let timer = Timer {
        owner,
        tag,
        due_ms: now_ms.saturating_add(ms),
        every_ms: every.then_some(ms),
        principal: tree.principal().clone(),
        authority: frame.authority().clone(),
        may_change: !tree.reading(),
    };
    tree.later_mut().set_timer(id, timer);
    Ok(id)
}

/// Cancels, from `frame`, the timer `id` its plugin set.
///
/// A timer the tree or its unit holds is taken back at once; an armed one is
/// cancelled when the unit is kept, or when the tree ends where no unit is.
///
/// # Errors
///
/// [`not_held`] for a timer the plugin does not hold — another plugin's, one
/// that fired or was cancelled, or none — and for one the tree or its unit
/// cancels already.
pub(crate) fn cancel<E: Environment>(
    shared: &Shared<E>,
    frame: &Frame<E>,
    id: u64,
) -> Result<(), Refusal> {
    let owner = frame.slot();
    let timers = lock(&shared.timers);
    let mut tree = frame.tree();
    let armed = timers.armed_for(id, owner);
    if tree.timer_taken_back(id, owner) || (armed && tree.later_mut().cancel_timer(id)) {
        Ok(())
    } else {
        Err(not_held(id))
    }
}

/// Every timer armed, by its identifier, and the last identifier handed out.
#[derive(Debug, Default)]
pub(crate) struct Timers {
    armed: BTreeMap<u64, Timer>,
    minted: u64,
}

impl Timers {
    /// The identifier of one more timer: never one handed out before.
    pub(crate) const fn mint(&mut self) -> u64 {
        self.minted = self.minted.saturating_add(1);
        self.minted
    }

    /// How many timers the plugin in `owner` holds armed.
    pub(crate) fn held_by(&self, owner: PluginSlot) -> usize {
        self.armed
            .values()
            .filter(|timer| timer.owner == owner)
            .count()
    }

    /// Whether `id` is armed, and the plugin in `owner`'s.
    pub(crate) fn armed_for(&self, id: u64, owner: PluginSlot) -> bool {
        self.armed
            .get(&id)
            .is_some_and(|timer| timer.owner == owner)
    }

    /// Arms `timer` as `id`.
    pub(crate) fn arm(&mut self, id: u64, timer: Timer) {
        self.armed.insert(id, timer);
    }

    /// Cancels `id`, answering whether it was armed.
    pub(crate) fn cancel(&mut self, id: u64) -> bool {
        self.armed.remove(&id).is_some()
    }

    /// The timer armed as `id`.
    pub(crate) fn get(&self, id: u64) -> Option<&Timer> {
        self.armed.get(&id)
    }

    /// Every timer due at `now_ms`, oldest due first, identifiers breaking a
    /// tie in the order they were set.
    pub(crate) fn due(&self, now_ms: u64) -> Vec<u64> {
        let mut due: Vec<(u64, u64)> = self
            .armed
            .iter()
            .filter(|(_, timer)| timer.due_ms <= now_ms)
            .map(|(id, timer)| (timer.due_ms, *id))
            .collect();
        due.sort_unstable();
        due.into_iter().map(|(_, id)| id).collect()
    }

    /// Fires `id` at `now_ms`, answering the timer as it was armed.
    ///
    /// A one-shot is taken out; a repeating one stays, due again its interval
    /// from now, so a burst of missed intervals fires it once.
    pub(crate) fn fire(&mut self, id: u64, now_ms: u64) -> Option<Timer> {
        let timer = self.armed.get_mut(&id)?;
        let fired = timer.clone();
        match timer.every_ms {
            Some(every) => timer.due_ms = now_ms.saturating_add(every),
            None => {
                self.armed.remove(&id);
            }
        }
        Some(fired)
    }

    /// Cancels every timer the plugin in `owner` holds: one being unloaded.
    pub(crate) fn forget(&mut self, owner: PluginSlot) {
        self.armed.retain(|_, timer| timer.owner != owner);
    }
}

#[cfg(test)]
mod tests {
    use nc_authz::EntityRef;
    use nc_catalog::{ErrorCode, KERNEL};

    use super::{TAG_BYTES, Timer, Timers, judged, no_room, not_held};
    use crate::PluginSlot;
    use crate::dispatch::{Authority, RefusalKind};

    /// A timer of the plugin in `owner`, due at `due_ms`, and repeating
    /// every `every_ms` when that is given.
    fn timer(owner: usize, due_ms: u64, every_ms: Option<u64>) -> Timer {
        let pdp = nc_authz::Pdp::embedded("family").expect("the family pack is carried");
        let world =
            nc_authz::MemoryDirectory::from_world(include_str!("../../../policy/world.json"), &pdp)
                .expect("the world loads");
        Timer {
            owner: PluginSlot(owner),
            tag: "tag".to_owned(),
            due_ms,
            every_ms,
            principal: world
                .principal(nc_authz::PrincipalKind::User, "ana")
                .expect("ana is a user"),
            authority: Authority {
                command: "reference.timer.set".to_owned(),
                action: "Write".to_owned(),
                resource: EntityRef {
                    type_name: "Content".to_owned(),
                    id: "photo".to_owned(),
                },
            },
            may_change: true,
        }
    }

    #[test]
    fn a_timer_is_set_only_within_every_bound_at_both_edges() {
        let max = KERNEL.timer_max_delay_ms;
        let min = KERNEL.timer_min_interval_ms;
        for (ms, every) in [(1, false), (max, false), (min, true), (max, true)] {
            assert!(judged(ms, every, "t").is_ok(), "{ms} {every}");
        }
        for (ms, every) in [
            (0, false),
            (max + 1, false),
            (min - 1, true),
            (max + 1, true),
        ] {
            let refused = judged(ms, every, "t").expect_err("out of range");
            assert_eq!(
                (refused.kind(), refused.code()),
                (RefusalKind::Invalid, ErrorCode::ValidationOutOfRange),
                "{ms} {every}"
            );
        }
        assert!(
            judged(1, false, &"t".repeat(TAG_BYTES)).is_ok(),
            "a tag of the bound"
        );
        let long = judged(1, false, &"t".repeat(TAG_BYTES + 1)).expect_err("past it");
        assert!(long.sentence().contains("tag"), "{}", long.sentence());
        let refused = not_held(7);
        assert_eq!(
            (refused.kind(), refused.code()),
            (RefusalKind::NotFound, ErrorCode::PluginNotFound)
        );
        assert_eq!(no_room().code(), ErrorCode::PluginBoundReached);
    }

    #[test]
    fn an_identifier_is_never_handed_out_twice_and_a_timer_is_its_owners() {
        let mut timers = Timers::default();
        let (one, two) = (timers.mint(), timers.mint());
        assert!(one > 0 && two > one);
        timers.arm(one, timer(0, 10, None));
        timers.arm(two, timer(1, 10, None));
        assert!(timers.armed_for(one, PluginSlot(0)));
        assert!(!timers.armed_for(one, PluginSlot(1)), "another plugin's");
        assert!(!timers.armed_for(99, PluginSlot(0)), "none");
        assert_eq!(timers.held_by(PluginSlot(0)), 1);
        assert!(timers.cancel(one));
        assert!(!timers.cancel(one), "cancelled already");
        assert!(
            timers.mint() > two,
            "a cancelled one's identifier is not reused"
        );
        timers.forget(PluginSlot(1));
        assert_eq!(timers.held_by(PluginSlot(1)), 0);
    }

    #[test]
    fn what_is_due_comes_oldest_first_and_a_repeating_timer_moves_on_from_now() {
        let mut timers = Timers::default();
        let late = timers.mint();
        let early = timers.mint();
        let tied = timers.mint();
        let future = timers.mint();
        timers.arm(late, timer(0, 30, None));
        timers.arm(early, timer(0, 10, Some(1_000)));
        timers.arm(tied, timer(0, 30, None));
        timers.arm(future, timer(0, 31, None));
        assert_eq!(timers.due(30), [early, late, tied]);
        let fired = timers.fire(early, 5_000).expect("armed");
        assert_eq!(fired.due_ms, 10, "answered as it was armed");
        assert_eq!(timers.get(early).map(|timer| timer.due_ms), Some(6_000));
        assert!(timers.fire(late, 30).is_some());
        assert!(timers.get(late).is_none(), "a one-shot fires once");
        assert!(timers.fire(late, 30).is_none());
        assert_eq!(timers.due(30), [tied]);
    }
}
