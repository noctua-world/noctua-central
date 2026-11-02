//! What becomes of a plugin that is stopped: why it was, whether it runs
//! again, and what holds each call into it to its time.
//!
//! **One store per plugin, and a stopped one is never entered again.** A trap,
//! a run past the fuel or the epoch, a memory growth the guard refused and a
//! host call past its deadline each end the call with an error, and wasmtime
//! then refuses every later entry into that store — any instance in it — as
//! `CannotEnterComponent`. So the store is dropped, which is the kill, and
//! the plugin is instantiated again in a fresh one from what it was first
//! instantiated from and activated again: a plugin that declares anything
//! other than what was registered, or does not activate, is not run again.
//! That restart is a call of the call tree the kill was in, held to what the
//! tree has left; one the tree has no time left for waits for the next call
//! tree, which starts it before it runs anything — so no tree outlives its
//! deadline by the time a plugin on its stack takes to come back.
//! While a command runs, the store is out of its [`Slot`], so a dispatch into
//! the same plugin from further down the tree finds the slot empty and is
//! refused as reentrant (`[NC-KERN-0003]`) without a trap.
//!
//! **Six causes, each with its own code** ([`Kill`]): a trap of the plugin's
//! own (`[NC-PLUG-0002]`), its fuel (`[NC-PLUG-0003]`), its epoch deadline
//! (`[NC-PLUG-0004]`), its memory (`[NC-PLUG-0005]`), a host call that did
//! not answer within `host_call_deadline_ms` (`[NC-PLUG-0006]`) and the
//! deadline of the call tree it ran in (`[NC-KERN-0007]`, the code a tree
//! past its deadline is refused with). The memory guard's refusal and the two
//! deadlines are the kernel's own error types and are not wasmtime traps, so
//! they are asked for first, and the traps after them.
//!
//! **Restarted within a budget.** `restart_budget` kills inside
//! `restart_window_ms` quarantine a plugin instead of restarting it: every
//! dispatch to it is `unavailable` (`[NC-PLUG-0007]`) until it is released,
//! and releasing it forgives every kill it had and starts it again — once,
//! however many releases arrive while it starts, with nothing dispatched to it
//! until it is back, and to its end whoever stops waiting for it. A kill at the call tree's deadline is not one of
//! them: the host spends that time as much as the plugin does, and a plugin is
//! not switched off because its store was slow, so it is restarted every time.
//!
//! **The epoch bounds what the plugin runs, not what the host answers.** A
//! call is given `epoch_deadline_ticks` ticks of the plugin's own running, and
//! the time it spends inside host calls is counted apart and given back — a
//! host call is held to a deadline of its own, and a plugin waiting on a slow
//! answer is not the plugin that spins. The whole call, host calls included,
//! is still held to its call tree's `dispatch_deadline_ms`, counted from when
//! the tree began, so a plugin that calls the host without end is stopped as
//! well, and every plugin a dispatch nested in the tree reaches stops when the
//! tree does — the decision point holds that dispatch's waits on the journal
//! (`crate::dispatch::Waits`); a call in no tree — `activate` — is held to
//! the same span from its own start.
//!
//! What this answers (`NOCTUA.md §2.4`): a plugin that crashes, spins,
//! computes, grows or waits without end is stopped, its unit of work is not
//! kept, and it harms no other plugin, which has a store, an instance and a
//! memory of its own; and one that crashes again and again is stopped for
//! good rather than restarted for ever. What it does not answer: the work a
//! host call was doing when its deadline passed, which goes on in the kernel
//! until it finishes and is then dropped; a plugin that calls the host
//! without end inside its fuel, which is killed at its tree's deadline each
//! time it is dispatched and never quarantined for it; and a plugin that
//! stays within every bound and is merely slow. What a quarantined plugin
//! would have been called back for is `crate::pump`'s: its due timers are
//! held until it is released, and an event it would hear is dropped and
//! counted.

use core::fmt;
use core::future::Future;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use nc_catalog::{ErrorCode, KERNEL};
use nc_ids::Clock as _;

use crate::bindings::{self, KernelError};
use crate::dispatch::{Refusal, logged_slot};
use crate::limits::MemoryRefused;
use crate::logging::LogSink as _;
use crate::{Environment, PluginSlot, RefusalKind, Shared};

/// Why a plugin was killed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kill {
    /// It trapped: a panic, an `unreachable`, its stack, or anything else the
    /// plugin did.
    Trap,
    /// It ran out of the fuel one call is given.
    Fuel,
    /// It ran past the epoch deadline one call is given.
    Epoch,
    /// Its memory asked to grow past the bound it is held to.
    Memory,
    /// A host function it called did not answer within `host_call_deadline_ms`.
    HostCallDeadline,
    /// It ran past the deadline of the call tree it ran in, whatever of the
    /// time the host took.
    TreeDeadline,
}

impl Kill {
    /// What stopped the call that ended in `error`.
    ///
    /// The kernel's own errors first — the memory guard's, the host call's
    /// deadline and the call tree's, none of which is a trap of wasmtime's —
    /// then the traps that are the fuel's and the epoch's, and anything else
    /// is the plugin's own trap.
    pub(crate) fn of(error: &wasmtime::Error) -> Self {
        if error.downcast_ref::<MemoryRefused>().is_some() {
            return Self::Memory;
        }
        if error.downcast_ref::<HostCallDeadline>().is_some() {
            return Self::HostCallDeadline;
        }
        if error.downcast_ref::<TreeDeadline>().is_some() {
            return Self::TreeDeadline;
        }
        match error.downcast_ref::<wasmtime::Trap>() {
            Some(wasmtime::Trap::OutOfFuel) => Self::Fuel,
            Some(wasmtime::Trap::Interrupt) => Self::Epoch,
            _ => Self::Trap,
        }
    }

    /// The code a dispatch the kill stopped is refused with, when the plugin
    /// was started again.
    ///
    /// The call tree's deadline is the tree's code, the one a dispatch asked
    /// for past it is refused with: what ran out is the tree's time, which
    /// every plugin in it shares.
    #[must_use]
    pub const fn code(self) -> ErrorCode {
        match self {
            Self::Trap => ErrorCode::PluginTrapped,
            Self::Fuel => ErrorCode::PluginOutOfFuel,
            Self::Epoch => ErrorCode::PluginEpochDeadline,
            Self::Memory => ErrorCode::PluginMemoryLimit,
            Self::HostCallDeadline => ErrorCode::PluginHostCallDeadline,
            Self::TreeDeadline => ErrorCode::KernelDeadline,
        }
    }

    /// What the plugin did, as a refusal says it.
    pub(crate) const fn said(self) -> &'static str {
        match self {
            Self::Trap => "trapped",
            Self::Fuel => "ran out of fuel",
            Self::Epoch => "ran past its epoch deadline",
            Self::Memory => "outgrew its memory",
            Self::HostCallDeadline => "waited on a host call past its deadline",
            Self::TreeDeadline => "ran past its call tree's deadline",
        }
    }

    /// The word a dispatch's journal entry records the failure as.
    pub(crate) const fn word(self) -> &'static str {
        match self {
            Self::Trap => "trapped",
            Self::Fuel => "out-of-fuel",
            Self::Epoch => "epoch-deadline",
            Self::Memory => "memory-limit",
            Self::HostCallDeadline => "host-call-deadline",
            Self::TreeDeadline => "tree-deadline",
        }
    }

    /// The variant `kernel.plugin_killed` names it as.
    fn variant(self) -> nc_log::Variant {
        match self {
            Self::Trap => nc_log::variant!("trap"),
            Self::Fuel => nc_log::variant!("fuel"),
            Self::Epoch => nc_log::variant!("epoch"),
            Self::Memory => nc_log::variant!("memory"),
            Self::HostCallDeadline => nc_log::variant!("deadline"),
            Self::TreeDeadline => nc_log::variant!("tree"),
        }
    }
}

/// How a loaded plugin stands: the instance it runs in, how often it was
/// killed, and whether it runs at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginStatus {
    generation: u64,
    crashes: u32,
    quarantined: bool,
    last_kill: Option<Kill>,
}

impl PluginStatus {
    /// Which instance of the plugin runs now: 0 for the one its load
    /// activated, and one more each time it was started again.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// How many times it was killed since it was loaded or last released.
    #[must_use]
    pub const fn crashes(&self) -> u32 {
        self.crashes
    }

    /// Whether it is not run until it is released.
    #[must_use]
    pub const fn quarantined(&self) -> bool {
        self.quarantined
    }

    /// Why it was last killed, if it ever was.
    #[must_use]
    pub const fn last_kill(&self) -> Option<Kill> {
        self.last_kill
    }
}

/// Where a loaded plugin's store is.
pub(crate) enum Slot<E: Environment> {
    /// In its slot, ready to run a command.
    Ready(bindings::Instance<E>),
    /// Out of it, running a command — or being started again after a kill in
    /// the command it ran.
    Busy,
    /// Gone: the plugin is quarantined until it is released.
    Stopped,
    /// Gone, and being started again by a release.
    ///
    /// Nothing runs it, and no other release starts it, until that one has
    /// finished: the release that claimed the slot is the one that puts an
    /// instance in it, or leaves it stopped.
    Starting,
    /// Gone for good: the plugin was unloaded, and nothing is run in the slot
    /// again — a release finds nothing to start, and one starting it already
    /// leaves the slot as the unload did.
    Unloaded,
    /// Gone: killed in a call tree that had no time left to start it again
    /// in, and started again before the next call tree runs anything
    /// ([`started_again`]). It is not quarantined, and nothing runs it
    /// meanwhile.
    Killed,
}

/// Why a plugin's store could not be taken out of its slot.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Out {
    /// It is running a command already.
    Busy,
    /// It is not run until it is released, nor while a release starts it.
    Stopped,
    /// It was killed when its call tree's time was up, and is started again
    /// before the next call tree runs.
    Killed,
}

/// What became of a plugin after a kill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Back {
    /// It was started again, and runs.
    Restarted,
    /// It is quarantined until it is released.
    Quarantined,
    /// It is started again before the next call tree runs: the one it was
    /// killed in had no time left to start it in.
    Later,
}

/// What the kernel remembers of a plugin's kills.
///
/// **Bounded by the budget**: the kills inside the window are kept to decide
/// the next, and a plugin that reaches the budget is quarantined and killed no
/// more until it is released, which forgets them.
#[derive(Debug, Default)]
pub(crate) struct Supervision {
    generation: u64,
    crashes: u32,
    recent: VecDeque<u64>,
    last: Option<Kill>,
}

impl Supervision {
    /// Records a kill of `kill` at `now_ms`, and answers whether it spends
    /// the budget: the `restart_budget`-th kill inside `restart_window_ms`.
    ///
    /// **A kill at the call tree's deadline is counted and spends nothing.**
    /// The tree's time is spent by the host as much as by the plugin — a slow
    /// store, the directory, the journal, the kernel's own work between
    /// commands — and every plugin of the tree is stopped by it, so a plugin
    /// is not quarantined for a tree that ran long: it is killed, because its
    /// store cannot be entered again, and started again every time.
    fn counted(&mut self, kill: Kill, now_ms: u64) -> bool {
        self.crashes = self.crashes.saturating_add(1);
        self.last = Some(kill);
        if kill == Kill::TreeDeadline {
            return false;
        }
        while self
            .recent
            .front()
            .is_some_and(|at| now_ms.saturating_sub(*at) >= KERNEL.restart_window_ms)
        {
            self.recent.pop_front();
        }
        self.recent.push_back(now_ms);
        self.recent.len() >= usize::try_from(KERNEL.restart_budget).unwrap_or(usize::MAX)
    }

    /// Forgets every kill, as a release does.
    fn forgiven(&mut self) {
        self.crashes = 0;
        self.recent.clear();
    }
}

impl<E: Environment> Shared<E> {
    /// Takes the store of the plugin in `slot` out of it, with the bounds the
    /// plugin is held to.
    ///
    /// **It looks before it takes**, so a slot holding no store is left as it
    /// was. Taken first and put back, a quarantined slot is one a missed put
    /// back leaves busy for good: every dispatch refused as reentrant, and a
    /// release only forgiving it.
    pub(crate) fn check_out(
        &self,
        slot: PluginSlot,
    ) -> Result<(bindings::Instance<E>, crate::limits::Limits), Out> {
        let mut plugins = self.plugins();
        let Some(loaded) = plugins.loaded.get_mut(slot.index()) else {
            return Err(Out::Stopped);
        };
        match loaded.slot {
            Slot::Ready(_) => {}
            Slot::Busy => return Err(Out::Busy),
            Slot::Stopped | Slot::Starting | Slot::Unloaded => return Err(Out::Stopped),
            Slot::Killed => return Err(Out::Killed),
        }
        let Slot::Ready(instance) = core::mem::replace(&mut loaded.slot, Slot::Busy) else {
            return Err(Out::Busy);
        };
        Ok((instance, *loaded.manifest.limits()))
    }

    /// Puts the store of the plugin in `slot` back.
    pub(crate) fn check_in(&self, slot: PluginSlot, instance: bindings::Instance<E>) {
        if let Some(loaded) = self.plugins().loaded.get_mut(slot.index()) {
            loaded.slot = Slot::Ready(instance);
        }
    }

    /// Whether the plugin in `slot` is not run until it is released, or until
    /// the release starting it again has.
    pub(crate) fn stopped(&self, slot: PluginSlot) -> bool {
        self.plugins()
            .loaded
            .get(slot.index())
            .is_none_or(|loaded| {
                matches!(loaded.slot, Slot::Stopped | Slot::Starting | Slot::Unloaded)
            })
    }

    /// How the plugin in `slot` stands, if this kernel loaded one there.
    ///
    /// A plugin a release is starting again is quarantined still, until it is
    /// back.
    pub(crate) fn status(&self, slot: PluginSlot) -> Option<PluginStatus> {
        let plugins = self.plugins();
        let loaded = plugins
            .loaded
            .get(slot.index())
            .filter(|loaded| !matches!(loaded.slot, Slot::Unloaded))?;
        Some(PluginStatus {
            generation: loaded.supervision.generation,
            crashes: loaded.supervision.crashes,
            quarantined: matches!(loaded.slot, Slot::Stopped | Slot::Starting),
            last_kill: loaded.supervision.last,
        })
    }
}

/// Records that the plugin in `slot` was killed for `kill`, in the call tree
/// that began at `within`, and starts it again or quarantines it.
///
/// Its store is dropped already; the kill that spends the budget quarantines
/// it, and the answer is what became of it. The restart is held to what the
/// tree has left ([`restart`]).
pub(crate) async fn killed<E: Environment>(
    shared: &Shared<E>,
    slot: PluginSlot,
    kill: Kill,
    within: Instant,
) -> Back {
    shared.log.record(&nc_log::events::KernelPluginKilled {
        slot: logged_slot(slot),
        cause: kill.variant(),
        code: kill.code(),
    });
    let now_ms = shared.clock.now_ms();
    let spent = {
        let mut plugins = shared.plugins();
        let Some(loaded) = plugins.loaded.get_mut(slot.index()) else {
            return Back::Quarantined;
        };
        let spent = loaded.supervision.counted(kill, now_ms);
        if spent {
            loaded.slot = Slot::Stopped;
        }
        spent
    };
    if spent {
        logged_as_quarantined(shared, slot);
        return Back::Quarantined;
    }
    restart(shared, slot, Some(within)).await
}

/// Starts again every plugin a call tree killed with no time left to start
/// it in, each held to its own span as a load is: what a top-level dispatch
/// and each callback the pump calls do first, under the bus.
pub(crate) async fn started_again<E: Environment>(shared: &Shared<E>) {
    let killed: Vec<PluginSlot> = shared
        .plugins()
        .loaded
        .iter()
        .enumerate()
        .filter(|(_, loaded)| matches!(loaded.slot, Slot::Killed))
        .map(|(index, _)| PluginSlot(index))
        .collect();
    for slot in killed {
        restart(shared, slot, None).await;
    }
}

/// Releases the plugin in `slot`, as [`release`] does, on a task of its own.
///
/// **It runs to its end whoever stops waiting for it**, as a top-level
/// dispatch and a pump do. A release claims the slot before it starts the
/// plugin, and one dropped while the plugin activated left the slot claimed
/// for good — measured, every later release answered as released and started
/// nothing, and every dispatch to the plugin was refused as a quarantined
/// one's for as long as the kernel ran.
///
/// # Errors
///
/// What [`release`] refuses, and `unavailable` with `[NC-PLUG-0007]` when the
/// task did not finish — it panicked, or the runtime stopped it — so the
/// plugin is not said to be back.
pub(crate) async fn released<E: Environment>(
    shared: &Arc<Shared<E>>,
    slot: PluginSlot,
) -> Result<(), Refusal> {
    let shared = Arc::clone(shared);
    tokio::spawn(async move { release(&shared, slot).await })
        .await
        .unwrap_or_else(|_| {
            Err(Refusal::new(
                RefusalKind::Unavailable,
                ErrorCode::PluginQuarantined,
                "the release did not finish, so the plugin is not said to be back".to_owned(),
            ))
        })
}

/// Forgives every kill of the plugin in `slot`, and starts it again when it
/// is quarantined.
///
/// A slot this kernel loaded no plugin in holds nothing to release, and nor
/// does one unloaded, even while the release was starting it.
///
/// **The release claims the slot before it starts the plugin**, under the
/// lock it looked under, and holds it while the plugin is instantiated and
/// activated. A release that finds it claimed leaves it to the one that
/// claimed it, so two releases at once start the plugin once; and nothing is
/// dispatched to it meanwhile, so an instance that comes back is never put
/// over one a command has taken out to run — which would let a dispatch from
/// inside that command into a second instance of the same plugin.
///
/// # Errors
///
/// `unavailable`, with `[NC-PLUG-0007]`, when the plugin was quarantined and
/// does not come back as it was: it stays quarantined.
pub(crate) async fn release<E: Environment>(
    shared: &Shared<E>,
    slot: PluginSlot,
) -> Result<(), Refusal> {
    let plugin = {
        let mut plugins = shared.plugins();
        let Some(loaded) = plugins.loaded.get_mut(slot.index()) else {
            return Ok(());
        };
        loaded.supervision.forgiven();
        if !matches!(loaded.slot, Slot::Stopped) {
            return Ok(());
        }
        loaded.slot = Slot::Starting;
        loaded.manifest.id().to_owned()
    };
    // **Unloaded while it was started**: nothing is left to release, which is
    // what a slot holding no plugin is answered.
    if restart(shared, slot, None).await == Back::Restarted || shared.status(slot).is_none() {
        return Ok(());
    }
    Err(Refusal::new(
        RefusalKind::Unavailable,
        ErrorCode::PluginQuarantined,
        format!("`{plugin}` was released and did not come back as it was, so it stays quarantined"),
    ))
}

/// Instantiates the plugin in `slot` again from what it was first
/// instantiated from, and activates it.
///
/// **As it was, or not at all.** A plugin that does not activate again, or
/// that declares anything other than what was registered, is quarantined: its
/// commands stay registered, and every dispatch to one is refused as the
/// plugin's. One that comes back is the next generation of itself.
///
/// What it writes into the slot it writes as the slot's holder: a kill's
/// dispatch holds it busy, a release holds it starting, and the next call
/// tree holds it killed.
///
/// **Held to what the call tree it runs in has left**, for one a kill inside
/// a tree started at `within` asks for — none for a release and for the next
/// call tree, each held to its own span as a load is. Its `activate` is a call
/// of that tree: a reviewer found a tree of thirty seconds that ran
/// forty-nine, the restart's one read of its store waited on in full, and one
/// of sixty-eight with two plugins on its stack. A plugin whose `activate` the
/// tree's deadline stopped — one the tree had no time left for among them,
/// which is stopped at its first host call or tick — is left
/// [`Slot::Killed`], for the next call tree to start ([`started_again`]): that
/// is not a plugin that did not come back as it was, so it is not
/// quarantined.
async fn restart<E: Environment>(
    shared: &Shared<E>,
    slot: PluginSlot,
    within: Option<Instant>,
) -> Back {
    let started = Instant::now();
    let again = {
        let plugins = shared.plugins();
        plugins.loaded.get(slot.index()).map(|loaded| {
            let manifest = &loaded.manifest;
            let state =
                crate::host::HostState::for_plugin(manifest, shared, Arc::clone(&loaded.log))
                    .within(within);
            (
                loaded.pre.clone(),
                state,
                *manifest.limits(),
                loaded.declaration.clone(),
            )
        })
    };
    let Some((pre, state, limits, declaration)) = again else {
        return Back::Quarantined;
    };
    let mut activated = bindings::activated(&shared.engine, &pre, state, &limits).await;
    // What `activate` was refused, logged as it returns, as at a load —
    // whether or not the instance that came back is kept.
    if let Ok(back) = &mut activated {
        crate::dispatch::left(shared, slot, &mut back.instance);
    }
    let back = {
        let mut plugins = shared.plugins();
        let Some(loaded) = plugins.loaded.get_mut(slot.index()) else {
            return Back::Quarantined;
        };
        match activated {
            // **An unload while a release started it is the last word**: the
            // instance that came back is dropped, and the slot stays as the
            // unload left it.
            _ if matches!(loaded.slot, Slot::Unloaded) => return Back::Quarantined,
            Ok(activated) if activated.declaration == declaration => {
                loaded.slot = Slot::Ready(activated.instance);
                loaded.supervision.generation = loaded.supervision.generation.saturating_add(1);
                Back::Restarted
            }
            Err(bindings::NotActivated::Failed(error))
                if within.is_some() && error.downcast_ref::<TreeDeadline>().is_some() =>
            {
                loaded.slot = Slot::Killed;
                Back::Later
            }
            Ok(_) | Err(_) => {
                loaded.slot = Slot::Stopped;
                Back::Quarantined
            }
        }
    };
    match back {
        Back::Restarted => shared.log.record(&nc_log::events::KernelPluginRestarted {
            slot: logged_slot(slot),
            took: started.elapsed(),
        }),
        Back::Quarantined => logged_as_quarantined(shared, slot),
        Back::Later => {}
    }
    back
}

/// Logs that the plugin in `slot` is quarantined.
fn logged_as_quarantined<E: Environment>(shared: &Shared<E>, slot: PluginSlot) {
    shared.log.record(&nc_log::events::KernelPluginQuarantined {
        slot: logged_slot(slot),
        code: ErrorCode::PluginQuarantined,
    });
}

/// A host function that did not answer within `host_call_deadline_ms`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HostCallDeadline {
    function: &'static str,
}

impl fmt::Display for HostCallDeadline {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "the host call `{}` did not answer within its {} ms",
            self.function, KERNEL.host_call_deadline_ms
        )
    }
}

impl std::error::Error for HostCallDeadline {}

/// `call`, the body of the host function `function`, held to its deadline.
///
/// The deadline is the nearer of the catalog's `host_call_deadline_ms` and
/// `left`, what the call tree the call runs in has left of its own
/// (`Running::left`) — none for a call no plugin's call is running, which is
/// held to the host call's alone.
///
/// **Its expiry is a trap**, which kills the plugin that asked: a call it
/// waits on for ever is a plugin that holds its call tree for ever. What the
/// host was doing is dropped where it stood, and anything that must survive
/// that is the kernel's to keep outside the call. Every host function that
/// waits on anything goes through this, but `commands.dispatch`, whose command
/// runs under its call tree's deadline and whose waits on the journal the
/// decision point holds to the nearer of the two (`crate::dispatch::Waits`).
///
/// **A tree's time bounds its host calls too.** The epoch callback that
/// stops a tree past its deadline runs only while the plugin runs, so a host
/// call held to its own deadline alone ran on past the tree's — measured, a
/// fetch handed to the transport in a tree whose time was up, and a tree of
/// thirty seconds that ran forty-two. So the tree's limit, when it is the
/// nearer, traps as the tree's ([`TreeDeadline`]), and a call in a tree with
/// nothing left is never started: nothing of it reaches a store, the blobs or
/// the transport.
///
/// # Errors
///
/// What `call` answers, and as a trap [`HostCallDeadline`] once the host
/// call's deadline passes, or [`TreeDeadline`] once the tree's does.
pub(crate) async fn in_time<T>(
    function: &'static str,
    left: Option<Duration>,
    call: impl Future<Output = Result<T, KernelError>>,
) -> Result<T, KernelError> {
    let own = Duration::from_millis(KERNEL.host_call_deadline_ms);
    let tree = left.filter(|left| *left <= own);
    let expired = || {
        KernelError::Trap(tree.map_or_else(
            || wasmtime::Error::new(HostCallDeadline { function }),
            |_| wasmtime::Error::new(TreeDeadline),
        ))
    };
    if tree.is_some_and(|left| left.is_zero()) {
        return Err(expired());
    }
    tokio::time::timeout(tree.unwrap_or(own), call)
        .await
        .unwrap_or_else(|_| Err(expired()))
}

/// What a call tree that began at `began` has left of its
/// `dispatch_deadline_ms`, now: nothing once the deadline has passed.
pub(crate) fn tree_left(began: Instant) -> Duration {
    Duration::from_millis(KERNEL.dispatch_deadline_ms).saturating_sub(began.elapsed())
}

/// A call that ran past the deadline of the call tree it ran in.
///
/// The epoch deadline's callback answers it, as an error of its own rather
/// than the epoch's interrupt, so the kill is told apart from a plugin that
/// spun past its own share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TreeDeadline;

impl fmt::Display for TreeDeadline {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "the call ran past the {} ms its call tree may take, counted from when the \
             tree began",
            KERNEL.dispatch_deadline_ms
        )
    }
}

impl std::error::Error for TreeDeadline {}

/// How long one call into a plugin has run, its own running apart from the
/// host's answering, and how long the call tree it runs in has.
///
/// **Set at the start of every call**, and read when the call's epoch
/// deadline is reached: the store's call hook adds every stretch spent inside
/// the host to what the host is counted, and the deadline's callback gives a
/// plugin that has not run its own share yet the ticks that remain — never
/// past the deadline of its call tree, which is counted from the tree's start
/// and not the call's, so a dispatch nested late in a tree has only what the
/// tree has left.
#[derive(Debug, Default)]
pub(crate) struct Running {
    share: Duration,
    began: Option<Instant>,
    tree: Option<Instant>,
    waited: Duration,
    entered: Option<Instant>,
}

impl Running {
    /// A call begins, given `ticks` of the epoch to run for.
    ///
    /// In a call tree that began at `tree` — none for a call in no tree, which
    /// is held from its own start — and it answers the ticks its first
    /// deadline is away: the share, or what the tree has left when that is
    /// nearer.
    pub(crate) fn began(&mut self, ticks: u64, tree: Option<Instant>) -> u64 {
        let now = Instant::now();
        let tree = tree.unwrap_or(now);
        *self = Self {
            share: tick().saturating_mul(u32::try_from(ticks).unwrap_or(u32::MAX)),
            began: Some(now),
            tree: Some(tree),
            waited: Duration::ZERO,
            entered: None,
        };
        match verdict(
            Duration::ZERO,
            now.saturating_duration_since(tree),
            self.share,
        ) {
            Verdict::Run(ticks) => ticks,
            Verdict::Share | Verdict::Whole => 1,
        }
    }

    /// The call crossed between the plugin and the host as `hook` says.
    ///
    /// **Two states, the host's and the plugin's**, as wasmtime splits its
    /// hooks: returning from the plugin or calling the host begins a stretch
    /// of the host's, and calling the plugin or returning to it ends one. The
    /// plugin's own allocator, which the host calls inside a host call to hand
    /// it a string or a list, is the plugin running, and the epoch check at
    /// its entry calls the host again: each is a crossing of its own, so the
    /// wait before it stays the host's.
    pub(crate) fn crossed(&mut self, hook: wasmtime::CallHook) {
        if hook.entering_host() {
            self.entered = Some(Instant::now());
        } else if let Some(entered) = self.entered.take() {
            self.waited = self.waited.saturating_add(entered.elapsed());
        }
    }

    /// The call reached its epoch deadline: what to do about it.
    ///
    /// # Errors
    ///
    /// [`TreeDeadline`] once the call tree is past its deadline, which kills
    /// the plugin as the tree's; a plugin past its own share is interrupted,
    /// which is the epoch's.
    pub(crate) fn reached(&self) -> wasmtime::Result<wasmtime::UpdateDeadline> {
        let (Some(began), Some(tree)) = (self.began, self.tree) else {
            return Ok(wasmtime::UpdateDeadline::Interrupt);
        };
        let waited = self.waited.saturating_add(
            self.entered
                .map_or(Duration::ZERO, |entered| entered.elapsed()),
        );
        match verdict(
            began.elapsed().saturating_sub(waited),
            tree.elapsed(),
            self.share,
        ) {
            Verdict::Run(ticks) => Ok(wasmtime::UpdateDeadline::Continue(ticks)),
            Verdict::Share => Ok(wasmtime::UpdateDeadline::Interrupt),
            Verdict::Whole => Err(wasmtime::Error::new(TreeDeadline)),
        }
    }

    /// What the call tree of the call running now has left of its deadline,
    /// which each host call it makes is held to as well as its own: none
    /// while no call has begun.
    ///
    /// Of the tree the call began in, or of the call itself for one in no
    /// tree — `activate`, held to the same span from its own start.
    pub(crate) fn left(&self) -> Option<Duration> {
        self.tree.map(tree_left)
    }

    /// The share of the epoch the call running now was given, as a duration.
    ///
    /// Read by the bindings' tests, which hold each call to the ticks its
    /// plugin's manifest leaves it.
    #[cfg(test)]
    pub(crate) const fn share(&self) -> Duration {
        self.share
    }
}

/// The length of one tick of the epoch.
fn tick() -> Duration {
    Duration::from_millis(KERNEL.epoch_tick_ms)
}

/// What a call that reached its epoch deadline is told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// It runs on, for this many ticks.
    Run(u64),
    /// It has run its own share of the epoch.
    Share,
    /// Its call tree is past its deadline.
    Whole,
}

/// Whether a call whose plugin has `ran` of its own, in a call tree that has
/// run for `tree`, may run on when its plugin's share of the epoch is
/// `share`.
///
/// It may when the plugin has run less than its share and the tree less than
/// `dispatch_deadline_ms`: then for as many ticks as the nearer of the two
/// leaves, and at least one.
fn verdict(ran: Duration, tree: Duration, share: Duration) -> Verdict {
    let whole = Duration::from_millis(KERNEL.dispatch_deadline_ms);
    if ran >= share {
        return Verdict::Share;
    }
    if tree >= whole {
        return Verdict::Whole;
    }
    let left = share.saturating_sub(ran).min(whole.saturating_sub(tree));
    let ticks = left.as_millis().div_ceil(tick().as_millis().max(1));
    Verdict::Run(u64::try_from(ticks).unwrap_or(u64::MAX).max(1))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{
        HostCallDeadline, Kill, Out, Running, Slot, Supervision, TreeDeadline, Verdict, in_time,
        tick, verdict,
    };
    use crate::bindings::KernelError;
    use crate::limits::{Limits, Lowering, MemoryGuard};
    use nc_catalog::{ErrorCode, KERNEL};
    use wasmtime::CallHook;
    use wasmtime::ResourceLimiter as _;

    /// How many ticks `update` gives, or `None` when it interrupts or fails.
    fn ticks(update: &wasmtime::Result<wasmtime::UpdateDeadline>) -> Option<u64> {
        match update {
            Ok(wasmtime::UpdateDeadline::Continue(ticks)) => Some(*ticks),
            _ => None,
        }
    }

    /// A whole call tree's deadline.
    fn whole() -> Duration {
        Duration::from_millis(KERNEL.dispatch_deadline_ms)
    }

    /// The instant `ago` before now.
    fn before_now(ago: Duration) -> Instant {
        Instant::now()
            .checked_sub(ago)
            .expect("the machine has run longer than a call tree may")
    }

    /// Each cause of a kill is told apart, the kernel's own errors before the
    /// traps they would otherwise be read as.
    #[test]
    fn every_cause_of_a_kill_is_told_apart_and_carries_its_own_code() {
        let mut guard = MemoryGuard::new(
            &Limits::catalog()
                .lowered(&Lowering {
                    memory_bytes: Some(65_536),
                    ..Lowering::default()
                })
                .expect("a lowering within the catalog"),
        );
        let refused = guard
            .memory_growing(0, 65_537, None)
            .expect_err("past the bound");
        let deadline = wasmtime::Error::new(HostCallDeadline {
            function: "store#get",
        });
        let tree = wasmtime::Error::new(TreeDeadline);
        for (error, kill, code) in [
            (
                wasmtime::Error::new(wasmtime::Trap::UnreachableCodeReached),
                Kill::Trap,
                ErrorCode::PluginTrapped,
            ),
            (
                wasmtime::Error::new(wasmtime::Trap::StackOverflow),
                Kill::Trap,
                ErrorCode::PluginTrapped,
            ),
            (
                wasmtime::Error::msg("anything else"),
                Kill::Trap,
                ErrorCode::PluginTrapped,
            ),
            (
                wasmtime::Error::new(wasmtime::Trap::OutOfFuel),
                Kill::Fuel,
                ErrorCode::PluginOutOfFuel,
            ),
            (
                wasmtime::Error::new(wasmtime::Trap::Interrupt),
                Kill::Epoch,
                ErrorCode::PluginEpochDeadline,
            ),
            (
                refused.context("while it ran"),
                Kill::Memory,
                ErrorCode::PluginMemoryLimit,
            ),
            (
                deadline.context("while it ran"),
                Kill::HostCallDeadline,
                ErrorCode::PluginHostCallDeadline,
            ),
            (
                tree.context("while it ran"),
                Kill::TreeDeadline,
                ErrorCode::KernelDeadline,
            ),
        ] {
            assert_eq!(Kill::of(&error), kill, "{error:?}");
            assert_eq!(kill.code(), code);
        }
    }

    /// Each kill is journaled, logged and said in words of its own.
    ///
    /// The causes it is logged under are the ones the catalog's
    /// `kernel.plugin_killed` names, read from the catalog. **A verifier found
    /// the two deadlines' words held by nothing**: the journal could record
    /// either as `trapped`, and the log the tree's as `trap`, with every suite
    /// green.
    #[test]
    fn every_kill_is_journaled_logged_and_said_in_words_of_its_own() {
        let killed = include_str!("../../../catalog/log-events.toml")
            .split("[[event]]")
            .find(|event| event.contains("name = \"kernel.plugin_killed\""))
            .expect("the catalog declares `kernel.plugin_killed`");
        let every = [
            (Kill::Trap, "trapped", "trap", "trapped"),
            (Kill::Fuel, "out-of-fuel", "fuel", "ran out of fuel"),
            (
                Kill::Epoch,
                "epoch-deadline",
                "epoch",
                "ran past its epoch deadline",
            ),
            (Kill::Memory, "memory-limit", "memory", "outgrew its memory"),
            (
                Kill::HostCallDeadline,
                "host-call-deadline",
                "deadline",
                "waited on a host call past its deadline",
            ),
            (
                Kill::TreeDeadline,
                "tree-deadline",
                "tree",
                "ran past its call tree's deadline",
            ),
        ];
        for (kill, word, cause, said) in every {
            assert_eq!(kill.word(), word, "{kill:?}");
            assert_eq!(kill.variant().get(), cause, "{kill:?}");
            assert_eq!(kill.said(), said, "{kill:?}");
            assert!(
                killed.contains(&format!("`{cause}`")),
                "`kernel.plugin_killed` does not name the cause `{cause}`: {killed}"
            );
        }
        let words: std::collections::BTreeSet<_> =
            every.iter().map(|(_, word, _, _)| *word).collect();
        let causes: std::collections::BTreeSet<_> =
            every.iter().map(|(_, _, cause, _)| *cause).collect();
        assert_eq!(words.len(), every.len(), "two kills are journaled alike");
        assert_eq!(causes.len(), every.len(), "two kills are logged alike");
    }

    /// The budget is spent by the `restart_budget`-th kill inside the window,
    /// a kill older than the window is forgotten, and a release forgets them
    /// all.
    #[test]
    fn the_budget_is_spent_inside_the_window_and_a_release_forgets_it() {
        let mut supervision = Supervision::default();
        let budget = KERNEL.restart_budget;
        assert!(budget > 1, "a budget of one quarantines at the first kill");
        for at in 1..budget {
            assert!(
                !supervision.counted(Kill::Trap, u64::from(at)),
                "kill {at} of {budget} spent it"
            );
        }
        let later = KERNEL.restart_window_ms + 1;
        assert!(
            !supervision.counted(Kill::Fuel, later),
            "a kill outside the window counted against it"
        );
        assert_eq!(supervision.crashes, budget, "every kill is counted");
        assert_eq!(supervision.last, Some(Kill::Fuel));
        for at in 1..budget - 1 {
            assert!(!supervision.counted(Kill::Trap, later + u64::from(at)));
        }
        assert!(
            supervision.counted(Kill::Trap, later + u64::from(budget)),
            "the budget's last kill inside the window did not spend it"
        );
        supervision.forgiven();
        assert_eq!(supervision.crashes, 0);
        assert!(
            !supervision.counted(Kill::Trap, later + u64::from(budget) + 1),
            "a released plugin was held to the kills it had before"
        );
    }

    /// A kill at the call tree's deadline spends none of the budget, however
    /// many fall inside the window.
    ///
    /// Each is counted as a kill, and the plugin's own kills after them are
    /// held to the whole budget still. **A verifier found it spent**: a plugin
    /// whose store answered every read inside its deadline, but slowly, was
    /// quarantined at its third write.
    #[test]
    fn a_kill_for_the_trees_deadline_spends_no_budget() {
        let mut supervision = Supervision::default();
        let budget = KERNEL.restart_budget;
        for at in 0..=budget {
            assert!(
                !supervision.counted(Kill::TreeDeadline, u64::from(at)),
                "kill {at} at the tree's deadline spent the budget"
            );
        }
        assert_eq!(supervision.crashes, budget + 1, "every kill is counted");
        assert_eq!(supervision.last, Some(Kill::TreeDeadline));
        for at in 1..budget {
            assert!(
                !supervision.counted(Kill::Trap, u64::from(budget + at)),
                "the tree's kills were held against the plugin's own"
            );
        }
        assert!(
            supervision.counted(Kill::Trap, u64::from(budget * 2)),
            "the plugin's own kills were not held to the budget"
        );
    }

    /// A plugin that has not run its share runs on, whatever the host took of
    /// the call, and one that has is interrupted.
    #[test]
    fn the_epoch_counts_what_the_plugin_ran_and_not_what_the_host_answered() {
        let share = tick() * 5;
        assert_eq!(
            verdict(tick() * 2, tick() * 50, share),
            Verdict::Run(3),
            "two ticks run of five, the rest of the call spent in the host"
        );
        assert_eq!(verdict(tick() * 5, tick() * 5, share), Verdict::Share);
        assert_eq!(verdict(tick() * 10, tick() * 50, share), Verdict::Share);
        assert_eq!(
            verdict(share.saturating_sub(Duration::from_micros(1)), share, share),
            Verdict::Run(1),
            "a share not yet spent is at least one more tick"
        );
    }

    /// No call outlives its call tree's deadline, however much of it the host
    /// answered and however little the plugin ran.
    #[test]
    fn no_call_runs_past_a_call_trees_deadline_whatever_the_host_took() {
        let share = tick() * 5;
        assert_eq!(verdict(tick(), whole(), share), Verdict::Whole);
        assert_eq!(
            verdict(tick() * 2, whole().saturating_sub(tick() * 2), share),
            Verdict::Run(2),
            "the nearer of the two bounds is the one given"
        );
    }

    /// The share a call is given is its ticks, counted from when it began,
    /// with the host's stretches set apart.
    #[test]
    fn a_call_is_given_its_ticks_and_the_host_is_counted_apart() {
        let mut running = Running::default();
        assert!(
            ticks(&running.reached()).is_none(),
            "a call that never began has no share"
        );
        assert_eq!(
            running.began(KERNEL.epoch_deadline_ticks, None),
            KERNEL.epoch_deadline_ticks,
            "a call in no tree is first stopped at its share"
        );
        assert_eq!(
            running.share,
            tick() * u32::try_from(KERNEL.epoch_deadline_ticks).expect("fits")
        );
        running.crossed(CallHook::CallingHost);
        std::thread::sleep(Duration::from_millis(2));
        running.crossed(CallHook::ReturningFromHost);
        assert!(
            running.waited >= Duration::from_millis(2),
            "{:?}",
            running.waited
        );
        assert!(running.entered.is_none());
        assert!(
            ticks(&running.reached()).is_some(),
            "the share is not spent"
        );
    }

    /// What a person is told of a plugin killed, a unit aborted, an answer
    /// the kernel cannot read or an undo whose compensation failed promises
    /// only what the kernel reverses — what was changed in Noctua — and says
    /// that what was sent outside this computer may have arrived, in every
    /// locale (`NOCTUA.md §11.5`: never widen the claim).
    ///
    /// **A reviewer found every one of them saying nothing was kept** and to
    /// try again, after a `POST` the kernel's own documents say may have
    /// reached its host, so a person retrying sent it twice. Every way a
    /// plugin is killed is matched here, so a new one is a message judged too.
    #[test]
    fn a_kill_or_an_abort_promises_a_person_only_what_noctua_reverses() {
        use nc_catalog::i18n::{Locale, message};
        let kills = [
            Kill::Trap,
            Kill::Fuel,
            Kill::Epoch,
            Kill::Memory,
            Kill::HostCallDeadline,
            Kill::TreeDeadline,
        ];
        for kill in kills {
            match kill {
                Kill::Trap
                | Kill::Fuel
                | Kill::Epoch
                | Kill::Memory
                | Kill::HostCallDeadline
                | Kill::TreeDeadline => {}
            }
        }
        let codes = kills.iter().map(|kill| kill.code()).chain([
            ErrorCode::KernelUnitAborted,
            ErrorCode::KernelCompensationFailed,
            ErrorCode::PluginMalformedResult,
        ]);
        let mut judged = 0_usize;
        for code in codes {
            for (locale, beyond) in [
                (Locale::En, "outside this computer"),
                (Locale::PtBR, "fora deste computador"),
            ] {
                let said = message(code.message_key(), locale)
                    .unwrap_or_else(|| panic!("{code} has no message in {locale:?}"));
                assert!(
                    said.contains(beyond),
                    "{code} tells a person in {locale:?} more than the kernel reverses: {said}"
                );
                judged += 1;
            }
        }
        assert_eq!(judged, 18, "nine codes, each in both locales");
    }

    /// A call is given its own share of the epoch, and none of the time the
    /// host answered a call before it.
    ///
    /// **Written by a reviewer (wave 5, isolation).** With the host's account
    /// carried from one call into the next, every suite of this crate stayed
    /// green: a plugin that waited on the host in earlier calls ran that much
    /// longer than its share in a later one — up to its tree's deadline, the
    /// kill that spends no budget.
    #[test]
    fn a_call_is_given_none_of_the_host_time_of_the_call_before_it() {
        let mut running = Running::default();
        running.began(1, None);
        running.crossed(CallHook::CallingHost);
        std::thread::sleep(tick() * 4);
        running.crossed(CallHook::ReturningFromHost);
        running.began(1, None);
        std::thread::sleep(tick() * 2);
        assert!(
            matches!(running.reached(), Ok(wasmtime::UpdateDeadline::Interrupt)),
            "a call that ran twice its share of one tick was let run on, the host's wait of \
             the call before it counted as its own: {:?}",
            running.waited
        );
    }

    /// The plugin's allocator, run inside a host call to hand it what the host
    /// answers, is the plugin's own running, and the wait before it stays the
    /// host's.
    ///
    /// The crossings are wasmtime's around a host call that answers a string:
    /// into the host, a wait, into the plugin's allocator, and into the host
    /// again at the epoch check at its entry, where the deadline is asked.
    /// **This case found a real bug**: an account that only a call into the
    /// host opened was opened again at that check, and the wait was counted
    /// as the plugin's.
    #[test]
    fn the_plugins_allocator_inside_a_host_call_leaves_the_wait_to_the_host() {
        let mut running = Running::default();
        running.began(KERNEL.epoch_deadline_ticks, None);
        let wait = Duration::from_millis(20);
        running.crossed(CallHook::CallingHost);
        std::thread::sleep(wait);
        running.crossed(CallHook::CallingWasm);
        running.crossed(CallHook::CallingHost);
        assert!(
            running.waited >= wait,
            "the wait before the allocator was counted as the plugin's: {:?}",
            running.waited
        );
        running.crossed(CallHook::ReturningFromHost);
        running.crossed(CallHook::ReturningFromWasm);
        assert!(running.entered.is_some(), "the host's stretch goes on");
        running.crossed(CallHook::ReturningFromHost);
        assert!(running.entered.is_none());
        assert!(running.waited >= wait, "{:?}", running.waited);
    }

    /// A call begun late in its call tree is given only what the tree has
    /// left, and one in a tree past its deadline is stopped as the tree's.
    #[test]
    fn a_call_late_in_its_tree_is_given_what_the_tree_has_left_and_no_more() {
        let mut running = Running::default();
        let first = running.began(
            KERNEL.epoch_deadline_ticks,
            Some(before_now(whole().saturating_sub(tick() * 2))),
        );
        assert!(
            (1..=2).contains(&first),
            "a call two ticks before its tree's deadline was first stopped {first} ticks away"
        );
        let first = running.began(KERNEL.epoch_deadline_ticks, Some(before_now(whole())));
        assert_eq!(
            first, 1,
            "a tree past its deadline gives one tick to stop in"
        );
        let Err(stopped) = running.reached() else {
            panic!("a call past its tree's deadline runs on");
        };
        assert_eq!(
            stopped.downcast_ref::<TreeDeadline>(),
            Some(&TreeDeadline),
            "{stopped}"
        );
        assert_eq!(Kill::of(&stopped), Kill::TreeDeadline);
    }

    /// A host call past its deadline is a trap carrying the deadline, and one
    /// within it is what it answered.
    #[tokio::test(start_paused = true)]
    async fn a_host_call_past_its_deadline_traps_and_one_within_it_answers() {
        let answered = in_time("store#get", None, async { Ok::<_, KernelError>(7) }).await;
        assert!(matches!(answered, Ok(7)), "{answered:?}");
        let refused = in_time("store#get", None, async {
            Err::<(), _>(KernelError::Refused(
                crate::bindings::noctua::kernel::types::Error::Denied("no".to_owned()),
            ))
        })
        .await;
        assert!(
            matches!(refused, Err(KernelError::Refused(_))),
            "{refused:?}"
        );
        let started = tokio::time::Instant::now();
        let waited = in_time(
            "store#get",
            Some(Duration::from_millis(KERNEL.host_call_deadline_ms + 1)),
            core::future::pending::<Result<(), KernelError>>(),
        )
        .await;
        let Err(KernelError::Trap(trap)) = waited else {
            panic!("a call that never answers was answered: {waited:?}");
        };
        assert_eq!(
            trap.downcast_ref::<HostCallDeadline>(),
            Some(&HostCallDeadline {
                function: "store#get"
            })
        );
        assert!(trap.to_string().contains("`store#get`"), "{trap}");
        assert_eq!(
            started.elapsed(),
            Duration::from_millis(KERNEL.host_call_deadline_ms),
            "the deadline is the catalog's"
        );
    }

    /// A host call is held to what its call tree has left when that is the
    /// nearer deadline, trapping as the tree's, and one in a tree with nothing
    /// left is never started.
    ///
    /// **A reviewer found a real bug with this**: the call was held to its own
    /// deadline alone, so a fetch in a tree whose time was up was handed to
    /// the transport and waited on for as long as the host call's deadline
    /// allowed.
    #[tokio::test(start_paused = true)]
    async fn a_host_call_is_held_to_what_its_tree_has_left() {
        let left = Duration::from_millis(KERNEL.host_call_deadline_ms / 2);
        let started = tokio::time::Instant::now();
        let waited = in_time(
            "net#fetch",
            Some(left),
            core::future::pending::<Result<(), KernelError>>(),
        )
        .await;
        let Err(KernelError::Trap(trap)) = waited else {
            panic!("a call that never answers was answered: {waited:?}");
        };
        assert_eq!(trap.downcast_ref::<TreeDeadline>(), Some(&TreeDeadline));
        assert_eq!(started.elapsed(), left, "the tree's time is the nearer");
        let mut started_at_all = false;
        let never = in_time("net#fetch", Some(Duration::ZERO), async {
            started_at_all = true;
            Ok::<_, KernelError>(())
        })
        .await;
        assert!(
            !started_at_all
                && matches!(&never, Err(KernelError::Trap(trap)) if trap.downcast_ref::<TreeDeadline>().is_some()),
            "a call in a tree with no time left was started, or answered {never:?}"
        );
    }

    /// A kernel with its plugins' data in memory and the reference plugin
    /// loaded, and the slot it was loaded in.
    async fn loaded() -> (crate::Kernel<crate::InMemory>, crate::PluginSlot) {
        let pack = nc_authz::Pdp::embedded("family").expect("the family pack is carried");
        let world = nc_authz::MemoryDirectory::from_world(
            include_str!("../../../policy/world.json"),
            &pack,
        )
        .expect("the world loads");
        let realm = world.realm_id("home").expect("home is a realm");
        let keeper = world
            .principal(nc_authz::PrincipalKind::User, "ana")
            .expect("ana is a user of the world")
            .id()
            .clone();
        let kernel = crate::Kernel::open(crate::KernelConfig {
            store: crate::backend::MemoryBackend::default(),
            blobs: crate::blobs::MemoryBlobs::default(),
            pdp: Ok(pack),
            journal: nc_journal::Journal::new(
                nc_journal::InMemory::default(),
                realm,
                keeper,
                nc_ids::Minter::new(
                    nc_ids::MachineFlake::from_value(3).expect("a flake"),
                    nc_ids::SystemClock,
                ),
            ),
            directory: world,
            clock: nc_ids::SystemClock,
            log: crate::logging::Captured::default(),
            transport: crate::net::Scripted::default(),
        })
        .await
        .expect("the kernel opens");
        let manifest = crate::manifest::PluginManifest::parse(
            "id = \"noctua:reference\"\nnamespace = \"reference\"\ncapabilities = [\"store\"]\n",
        )
        .expect("a manifest the kernel reads");
        let slot = kernel
            .load(manifest, &crate::built::component("reference"))
            .await
            .unwrap_or_else(|refused| panic!("the reference plugin loads: {refused}"));
        (kernel, slot)
    }

    /// A check-out leaves a slot that holds no store as it was, however often
    /// one is asked for.
    ///
    /// Quarantined, being started again by a release, running already, or
    /// unloaded: each is answered as what it is, and stands as it stood — an
    /// unloaded plugin with no status at all. **A verifier
    /// found the quarantined case held by nothing**: the slot was written busy
    /// and then put back, and with the put-back deleted it stayed busy for
    /// good with every suite green, because the decision point asks whether a
    /// plugin is stopped before it ever checks one out.
    #[tokio::test]
    async fn a_check_out_leaves_a_slot_that_holds_no_store_as_it_was() {
        let (kernel, slot) = loaded().await;
        let shared = &kernel.shared;
        assert!(
            shared.check_out(slot).is_ok(),
            "the plugin it loaded is ready to run"
        );
        for (state, word, out, quarantined, stopped) in [
            (Slot::Stopped, "quarantined", Out::Stopped, Some(true), true),
            (
                Slot::Starting,
                "being started again",
                Out::Stopped,
                Some(true),
                true,
            ),
            (Slot::Busy, "running", Out::Busy, Some(false), false),
            (Slot::Unloaded, "unloaded", Out::Stopped, None, true),
        ] {
            shared.plugins().loaded[slot.index()].slot = state;
            for asked in 1..=2 {
                assert_eq!(
                    shared.check_out(slot).err().as_ref(),
                    Some(&out),
                    "check-out {asked} of a plugin {word}"
                );
                assert_eq!(
                    shared.status(slot).map(|status| status.quarantined()),
                    quarantined,
                    "a plugin {word}, after check-out {asked}"
                );
                assert_eq!(
                    shared.stopped(slot),
                    stopped,
                    "a plugin {word}, after check-out {asked}"
                );
            }
        }
    }
}
