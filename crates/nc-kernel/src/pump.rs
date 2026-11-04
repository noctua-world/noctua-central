//! The pump: what the kernel does between call trees — the timers that came
//! due, the events queued, and the local tables held to their bounds.
//!
//! **In one order, one tree at a time.** [`crate::Kernel::pump`] takes the bus
//! every top-level dispatch takes, so nothing it runs interleaves with a
//! command, and does three things in turn: it fires every timer due, oldest
//! due first (`crate::timers`); it delivers the events that were queued when it
//! began, oldest first, each once (`crate::events`) — so what a callback
//! publishes while it runs waits for the next pump; and it runs the one
//! pruner over the local database, which removes rows only from a table past
//! its bound (`NOCTUA.md §15.8`) — once every unit this kernel left committed
//! in the store and open in the journal is reverted, and not while one stays.
//! It answers [`Pumped`], what each came to.
//!
//! **A callback is a call tree of its own, decided before it runs.** Each
//! timer fired and each event heard by each plugin that subscribed runs in a
//! tree of its own, for the principal the timer was set for or the event
//! published for, never confirmed: the decision point first decides again the
//! authority the timer was set or the event published under, and a denial,
//! journaled, cancels the timer or drops the event rather than calling anyone.
//! The callback's frame is beneath every command it dispatches, so each is a
//! nested dispatch, through the decision point — never `kernel.undo` or
//! `kernel.redo`, never confirmed — and an event's callback only reads, so no
//! command beneath it may change state; nor may one beneath the callback of a
//! timer set where only reading was allowed. Any other timer's callback may:
//! each command that changes state is a step of its own, or of the unit the
//! callback began, kept when the callback returns unless something in it
//! failed.
//!
//! **Supervised like any call.** A plugin that traps, spins, burns its fuel or
//! outgrows its memory in a callback is killed and started again, or
//! quarantined, as it is in a command (`crate::supervisor`), and the unit its
//! callback began is let go. A plugin a call tree killed with no time left to
//! start it again in is started first — by the pump before anything else, and
//! before each callback. A plugin that is quarantined is not called: its
//! due timers are held until it is released, and an event it would hear is
//! dropped and counted — and that is asked after the start before its callback,
//! so a plugin that start quarantined is held from it too. Asked before the
//! start, a start that failed would leave the one-shot timer spent and counted
//! as fired, and the event as delivered.
//!
//! **To its end, whoever stops waiting.** The pump runs on a task of its own,
//! as a top-level dispatch does: a caller that stops waiting for it loses the
//! answer, and leaves no plugin checked out and no unit open.
//!
//! What this answers (`NOCTUA.md §2.4`): stale authorization — a timer or an
//! event acting on what was once allowed — a callback mutating as a person
//! without the decision point, a callback that crashes or hangs, and a local
//! table that grows past its bound. What it does not answer: when the pump is
//! run, which is the daemon's (`NOCTUA-PLAN.md`'s Stage 13), and a store held
//! in memory, which has no database to prune.

use std::sync::{Arc, Mutex};

use nc_authz::Principal;
use nc_catalog::ErrorCode;
use nc_ids::Clock as _;

use crate::dispatch::{self, Authority, Call, CallTree, Cause, Frame, lock};
use crate::history::Held;
use crate::supervisor::{self, Kill};
use crate::{Environment, PluginSlot, Shared, units};

/// What one pump came to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Pumped {
    /// Timers whose callback was called.
    pub fired: u32,
    /// Events heard: each callback a subscriber was called with.
    pub delivered: u32,
    /// Timers cancelled and events dropped because the decision point refused
    /// the authority they were set or published under, now.
    pub refused: u32,
    /// Timers due and held, not fired, because their plugin is quarantined.
    pub suspended: u32,
    /// Events a quarantined subscriber would have heard, dropped.
    pub dropped: u32,
    /// Callbacks that did not end well: killed, or whose unit was not kept.
    pub failed: u32,
    /// Rows the pruner removed from the local database.
    pub pruned: u64,
    /// The code the pruner refused a table with, when it refused one.
    ///
    /// **Or what refused a reversal still owed**, when the pruner was not run
    /// because a unit the journal holds open is committed in the store.
    pub prune_refused: Option<ErrorCode>,
}

/// What a callback is called with.
#[derive(Debug, Clone)]
enum Callback {
    /// A timer, fired: its tag, and whether its callback may change state.
    Timer {
        /// What the plugin is handed.
        tag: String,
        /// Whether the timer was set where a change was allowed.
        may_change: bool,
    },
    /// An event, heard: its topic and its data.
    Event(String, String),
}

/// Pumps once, under the bus, on a task of its own.
pub(crate) async fn pumped<E: Environment>(shared: &Arc<Shared<E>>) -> Pumped {
    let shared = Arc::clone(shared);
    let pumping = async move {
        let _between_trees = shared.bus.lock().await;
        supervisor::started_again(&shared).await;
        let queued = lock(&shared.events).len();
        let mut pumped = Pumped::default();
        fired(&shared, &mut pumped).await;
        delivered(&shared, queued, &mut pumped).await;
        pruned(&shared, &mut pumped).await;
        pumped
    };
    tokio::spawn(pumping).await.unwrap_or_default()
}

/// Fires every timer due now, oldest due first.
async fn fired<E: Environment>(shared: &Arc<Shared<E>>, pumped: &mut Pumped) {
    let now_ms = shared.clock.now_ms();
    let due = lock(&shared.timers).due(now_ms);
    for id in due {
        let owner = match lock(&shared.timers).get(id) {
            Some(timer) => timer.owner,
            // Cancelled by a callback this pump called before it.
            None => continue,
        };
        supervisor::started_again(shared).await;
        if shared.stopped(owner) {
            pumped.suspended = pumped.suspended.saturating_add(1);
            continue;
        }
        let Some(timer) = lock(&shared.timers).fire(id, now_ms) else {
            continue;
        };
        let tree = tree_for(shared, &timer.principal);
        let again = dispatch::reauthorized(
            shared,
            &tree,
            &timer.principal,
            &timer.authority,
            Held::default(),
            "firing a timer it set being authorized again now",
        )
        .await;
        if again.is_err() {
            lock(&shared.timers).cancel(id);
            pumped.refused = pumped.refused.saturating_add(1);
            continue;
        }
        pumped.fired = pumped.fired.saturating_add(1);
        let callback = Callback::Timer {
            tag: timer.tag,
            may_change: timer.may_change,
        };
        if !called_back(shared, owner, &timer.principal, &timer.authority, callback).await {
            pumped.failed = pumped.failed.saturating_add(1);
        }
    }
}

/// Delivers the `queued` oldest events, each once, to every plugin that
/// subscribed to its topic.
async fn delivered<E: Environment>(shared: &Arc<Shared<E>>, queued: usize, pumped: &mut Pumped) {
    for _ in 0..queued {
        let Some(event) = lock(&shared.events).next() else {
            break;
        };
        let subscribers = shared.subscribers(&event.topic);
        if subscribers.is_empty() {
            continue;
        }
        let tree = tree_for(shared, &event.principal);
        let decided = dispatch::reauthorized(
            shared,
            &tree,
            &event.principal,
            &event.authority,
            Held::default(),
            "delivering an event it published being authorized again now",
        )
        .await;
        if decided.is_err() {
            pumped.refused = pumped.refused.saturating_add(1);
            continue;
        }
        for subscriber in subscribers {
            supervisor::started_again(shared).await;
            if shared.stopped(subscriber) {
                pumped.dropped = pumped.dropped.saturating_add(1);
                continue;
            }
            pumped.delivered = pumped.delivered.saturating_add(1);
            let callback = Callback::Event(event.topic.clone(), event.data.clone());
            let ended_well = called_back(
                shared,
                subscriber,
                &event.principal,
                &event.authority,
                callback,
            )
            .await;
            if !ended_well {
                pumped.failed = pumped.failed.saturating_add(1);
            }
        }
    }
}

/// Runs the one pruner over the local database, when the store has one.
///
/// **Once the units this kernel left are reverted** (`crate::units`), and not
/// at all while one stays, the code of what refused its reversal said instead:
/// the pruner takes the oldest units' records, and a unit committed in the
/// store and open in the journal is reverted from its own.
async fn pruned<E: Environment>(shared: &Shared<E>, pumped: &mut Pumped) {
    use crate::backend::StoreBackend as _;

    let Some(mut target) = shared.store.pruner() else {
        return;
    };
    if let Err(held) = units::reverted_again(shared).await {
        pumped.prune_refused = Some(held.code());
        return;
    }
    match nc_prune::run(&mut target, nc_prune::Request::every()).await {
        Ok(tables) => {
            pumped.pruned = tables.iter().map(|table| table.removed).sum();
            pumped.prune_refused = tables.iter().find_map(nc_prune::Pruned::code);
        }
        Err(why) => pumped.prune_refused = Some(code_of(&why)),
    }
}

/// The code the pruner's refusal `why` opens on, as every one of its
/// refusals does, and `ErrorCode::DbPruneRefused` for one that does not.
pub(crate) fn code_of(why: &str) -> ErrorCode {
    ErrorCode::ALL
        .iter()
        .copied()
        .find(|code| why.starts_with(&format!("[{}]", code.code())))
        .unwrap_or(ErrorCode::DbPruneRefused)
}

/// A call tree for `principal`, begun now, which a callback runs in and a
/// refusal to run one is journaled in.
fn tree_for<E: Environment>(shared: &Shared<E>, principal: &Principal) -> Arc<Mutex<CallTree>> {
    Arc::new(Mutex::new(CallTree::new(
        principal.clone(),
        shared.clock.now_ms(),
    )))
}

/// Calls `callback` of the plugin in `slot`, as `principal` under `authority`.
///
/// In a call tree of its own, answering whether it ended well: it ran to its
/// end, and the unit it began, if any, was kept. **Its caller starts what a
/// call tree killed first**, and asks then whether the plugin is stopped.
async fn called_back<E: Environment>(
    shared: &Arc<Shared<E>>,
    slot: PluginSlot,
    principal: &Principal,
    authority: &Authority,
    callback: Callback,
) -> bool {
    let Ok((mut instance, limits)) = shared.check_out(slot) else {
        return false;
    };
    let call = match callback {
        Callback::Timer { may_change, .. } => Call::Timer { may_change },
        Callback::Event(..) => Call::Event,
    };
    let tree = tree_for(shared, principal);
    lock(&tree).calls_back(call);
    instance.enter(Frame::new(
        Arc::downgrade(shared),
        Arc::clone(&tree),
        slot,
        call,
        authority.clone(),
        false,
    ));
    let called = match &callback {
        Callback::Timer { tag, .. } => crate::bindings::on_timer(&mut instance, tag, &limits).await,
        Callback::Event(topic, data) => {
            crate::bindings::on_event(&mut instance, topic, data, &limits).await
        }
    };
    dispatch::left(shared, slot, &mut instance);
    let ran = match called {
        Ok(()) => {
            shared.check_in(slot, instance);
            true
        }
        Err(stopped) => {
            let kill = Kill::of(&stopped);
            drop(instance);
            let began = lock(&tree).began();
            supervisor::killed(shared, slot, kill, began).await;
            false
        }
    };
    let put_off = lock(&tree).put_off_taken();
    if let Some(later) = put_off {
        later.released(shared);
    }
    let unit = lock(&tree).unit_taken();
    let Some(ambient) = unit else {
        return ran;
    };
    match (ran, ambient.doomed_for()) {
        (true, None) => {
            let label = shared.plugin_of(slot).unwrap_or_default();
            units::close(shared, ambient, principal, &label)
                .await
                .is_ok()
        }
        (true, Some(cause)) => {
            units::let_go(shared, &ambient, cause).await;
            false
        }
        (false, _) => {
            units::let_go(shared, &ambient, Cause::Failed).await;
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use nc_catalog::ErrorCode;

    use super::code_of;

    #[test]
    fn a_refusal_of_the_pruner_is_read_as_the_code_it_opens_on() {
        for code in [ErrorCode::DbTableNotBounded, ErrorCode::DbUnreachable] {
            assert_eq!(code_of(&format!("[{code}] the table refused")), code);
        }
        assert_eq!(
            code_of("a sentence that names no code"),
            ErrorCode::DbPruneRefused,
            "a refusal is never read as no refusal"
        );
    }
}
