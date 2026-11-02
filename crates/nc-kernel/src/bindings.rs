//! The world, as wasmtime's `bindgen!` generates it for the host.
//!
//! **Private, and nothing generated leaves it.** The generated types document
//! what the WIT documents and no more — `variant error` and two of its cases
//! carry no doc comment — so re-exporting one puts `missing_docs` on this crate
//! for text it does not own. Whatever the rest of the kernel needs from here
//! goes out as a function of this module, in the kernel's own types.
//!
//! **`forbid(unsafe_code)` holds, and what it covers is written down.** The
//! expansion contains `unsafe` — `TypedFunc::new_unchecked`, once per guest
//! export — and rustc does not report `unsafe_code` inside an external macro's
//! expansion, so this crate compiles under the workspace's `forbid`. What
//! `forbid` means in this repository is *no `unsafe` written here*; the
//! expansion is wasmtime's, as wasmtime's internals are. Measured in a scratch
//! crate carrying this workspace's lints: with `WASMTIME_DEBUG_BINDGEN=1` the
//! generated source is included as a local file and the same build fails with
//! five `unsafe_code` errors. Never judge the lints with it set.
//!
//! **A refusal is a value, and only a fault is a trap.** Every host function
//! that returns `result<_, error>` is generated to return
//! `Result<_, KernelError>`, and `convert_error` decides per error:
//! [`KernelError::Refused`] reaches the plugin as the world's `error` value and
//! the instance lives on, [`KernelError::Trap`] traps it. Under plain
//! `trappable` a `?` on a refusal is an outer `Err` — measured, it trapped, and
//! a trap poisons the plugin's whole `Store`, so a capability check written the
//! obvious way would have killed the plugin it was refusing.
//!
//! `log` and `context` are generated synchronous: both are answered from what
//! the call already holds and never await, and an `async fn` that never awaits
//! is what clippy's `unused_async_trait_impl` refuses.
//!
//! What this answers (`NOCTUA.md §2.4`): a host function cannot hand a plugin a
//! refusal as a trap by accident, which would turn every `denied` into a kill.
//! What it does not answer: which functions a plugin may call, which is the
//! capability each host function asks for first (`crate::host`).

wasmtime::component::bindgen!({
    world: "plugin",
    path: "wit",
    imports: {
        "noctua:kernel/log": trappable,
        "noctua:kernel/context": trappable,
        default: async | trappable,
    },
    exports: { default: async },
    trappable_error_type: { "noctua:kernel/types.error" => crate::bindings::KernelError },
});

use noctua::kernel::types;

use crate::Environment;
use crate::engine::{Compiled, Engine};
use crate::host::HostState;
use crate::limits::Limits;
use crate::registry::Declared;

/// What a host function answers when it does not answer `Ok`.
#[derive(Debug)]
#[expect(
    unreachable_pub,
    reason = "every generated host trait is declared `pub` and names this type, so it must \
              be declared `pub` as well (E0446); the module holding both is private, which \
              is what keeps it out of the crate's surface"
)]
pub enum KernelError {
    /// Reaches the plugin as the world's `error` value; the instance lives on.
    Refused(types::Error),
    /// Traps: the instance is killed, and its `Store` with it.
    Trap(wasmtime::Error),
}

impl From<types::Error> for KernelError {
    fn from(refusal: types::Error) -> Self {
        Self::Refused(refusal)
    }
}

impl From<wasmtime::Error> for KernelError {
    fn from(fault: wasmtime::Error) -> Self {
        Self::Trap(fault)
    }
}

/// `component`, linked against every host function of the world and type
/// checked before anything runs.
///
/// **Every import is the host's, and only the world's.** Each function of the
/// world is linked to its implementation in `crate::host`, whose first
/// statement asks for its capability, so a plugin granted nothing reaches the
/// host and is told `denied` rather than failing to link. A component that
/// imports anything the world does not declare is refused here, because no
/// definition of it exists.
pub(crate) fn linked<E: Environment>(
    engine: &Engine,
    component: &Compiled,
) -> wasmtime::Result<PluginPre<HostState<E>>> {
    use wasmtime::component::HasSelf;

    let mut linker = wasmtime::component::Linker::<HostState<E>>::new(engine.wasmtime());
    Plugin::add_to_linker::<HostState<E>, HasSelf<HostState<E>>>(&mut linker, |state| state)?;
    PluginPre::new(linker.instantiate_pre(component.wasmtime())?)
}

/// A store holding `state`, whose memory and tables grow against the guard
/// it carries, given `fuel` and an epoch deadline `ticks` away.
///
/// **Its epoch counts the plugin's own running.** The call hook tells the
/// state each time a call crosses into the host and back, and reaching the
/// deadline asks the state whether the plugin has run its share of it or
/// spent the rest waiting on the host, which is held to a deadline of its own,
/// and whether the call tree it runs in is past its deadline, which kills it
/// as the tree's (`crate::supervisor`).
fn store_for<E: Environment>(
    engine: &Engine,
    state: HostState<E>,
    fuel: u64,
    ticks: u64,
) -> wasmtime::Result<wasmtime::Store<HostState<E>>> {
    let mut store = wasmtime::Store::new(engine.wasmtime(), state);
    store.limiter(|state| state.guard());
    store.call_hook(|mut store, hook| {
        store.data_mut().running().crossed(hook);
        Ok(())
    });
    store.epoch_deadline_callback(|mut store| store.data_mut().running().reached());
    begun(&mut store, fuel, ticks)?;
    Ok(store)
}

/// A call begins in `store`: given `fuel`, and `ticks` of the epoch counted
/// from now, within what its call tree has left.
///
/// **The tree's start is the frame's**, which the store holds once a command
/// is entered, so a dispatch nested late in a tree is first stopped where the
/// tree ends; `activate` runs in no tree and is held from its own start.
fn begun<E: Environment>(
    store: &mut wasmtime::Store<HostState<E>>,
    fuel: u64,
    ticks: u64,
) -> wasmtime::Result<()> {
    store.set_fuel(fuel)?;
    let tree = store.data().tree_began();
    let first = store.data_mut().running().began(ticks, tree);
    store.set_epoch_deadline(first);
    Ok(())
}

/// What a plugin declared at `activate`: its commands, in the kernel's own
/// types, and the topics it would hear.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Declaration {
    /// Every command, in the order declared.
    pub(crate) commands: Vec<Declared>,
    /// Every topic pattern, in the order declared.
    pub(crate) subscriptions: Vec<String>,
}

/// A plugin instantiated and activated: its store and the instance in it,
/// and what it declared.
pub(crate) struct Activated<E: Environment> {
    /// The store and the instance every later call goes through.
    pub(crate) instance: Instance<E>,
    /// What `activate` declared.
    pub(crate) declaration: Declaration,
}

/// One plugin, instantiated: the store it lives in and the instance in it.
pub(crate) struct Instance<E: Environment> {
    store: wasmtime::Store<HostState<E>>,
    plugin: Plugin,
}

impl<E: Environment> Instance<E> {
    /// Hands the plugin's store the frame of the command it is about to run.
    pub(crate) fn enter(&mut self, frame: crate::dispatch::Frame<E>) {
        self.store.data_mut().enter(frame);
    }

    /// Takes the frame back once the command has run, and answers every
    /// function the plugin was refused for the first time while it ran.
    pub(crate) fn leave(&mut self) -> Vec<crate::capability::Denial> {
        self.store.data_mut().leave()
    }

    /// Every reason a fetch of the plugin was refused for since the kernel
    /// last asked, each once.
    pub(crate) fn fetches_refused(&mut self) -> Vec<(crate::net::Reason, nc_catalog::ErrorCode)> {
        self.store.data_mut().fetches_refused()
    }

    /// Every code a host call of the plugin was refused with for where it ran
    /// or whose name it used since the kernel last asked, each once.
    pub(crate) fn calls_refused(&mut self) -> Vec<nc_catalog::ErrorCode> {
        self.store.data_mut().calls_refused()
    }
}

/// A call into `instance` begins, given the fuel one dispatch is and an
/// epoch deadline the plugin's ticks away: a command's, or a callback's,
/// which is held to what a dispatch is.
fn dispatched<E: Environment>(instance: &mut Instance<E>, limits: &Limits) -> wasmtime::Result<()> {
    begun(
        &mut instance.store,
        limits.fuel_per_dispatch(),
        limits.epoch_deadline_ticks(),
    )
}

/// Runs the command `name` with `args` in `instance`, given the fuel one
/// dispatch is and an epoch deadline the plugin's ticks away.
///
/// Answers what the plugin answered — its result, or its refusal as the
/// world's case and its text — or what stopped it, which is a trap: the
/// store is then poisoned and is never entered again.
pub(crate) async fn run<E: Environment>(
    instance: &mut Instance<E>,
    name: &str,
    args: &str,
    limits: &Limits,
) -> wasmtime::Result<Result<String, (crate::RefusalKind, String)>> {
    dispatched(instance, limits)?;
    let answered = instance
        .plugin
        .noctua_kernel_guest()
        .call_execute(&mut instance.store, name, &args.to_owned())
        .await?;
    Ok(answered.map_err(refusal_of))
}

/// Calls the plugin's `on-timer` in `instance` with `tag`, on what one
/// dispatch is given.
///
/// Answers nothing, as the world declares it, or what stopped it, which is a
/// trap: the store is then poisoned and is never entered again.
pub(crate) async fn on_timer<E: Environment>(
    instance: &mut Instance<E>,
    tag: &str,
    limits: &Limits,
) -> wasmtime::Result<()> {
    dispatched(instance, limits)?;
    instance
        .plugin
        .noctua_kernel_guest()
        .call_on_timer(&mut instance.store, tag)
        .await
}

/// Calls the plugin's `on-event` in `instance` with `topic` and `data`, on
/// what one dispatch is given.
///
/// Answers as [`on_timer`] does: nothing, or the trap that
/// stopped it.
pub(crate) async fn on_event<E: Environment>(
    instance: &mut Instance<E>,
    topic: &str,
    data: &str,
    limits: &Limits,
) -> wasmtime::Result<()> {
    dispatched(instance, limits)?;
    instance
        .plugin
        .noctua_kernel_guest()
        .call_on_event(&mut instance.store, topic, &data.to_owned())
        .await
}

/// The world's `error` a plugin answered a command with, as the kernel's case
/// and the plugin's text.
fn refusal_of(refusal: types::Error) -> (crate::RefusalKind, String) {
    use crate::RefusalKind;

    match refusal {
        types::Error::Denied(why) => (RefusalKind::Denied, why),
        types::Error::NotFound(why) => (RefusalKind::NotFound, why),
        types::Error::Invalid(why) => (RefusalKind::Invalid, why),
        types::Error::Conflict(why) => (RefusalKind::Conflict, why),
        types::Error::Unavailable(why) => (RefusalKind::Unavailable, why),
        types::Error::Internal(why) => (RefusalKind::Internal, why),
    }
}

/// Why a plugin did not activate.
#[derive(Debug)]
pub(crate) enum NotActivated {
    /// Stopped by wasmtime: a trap, the fuel or the memory bound, while it was
    /// instantiated or while `activate` ran.
    Failed(wasmtime::Error),
    /// `activate` answered with the world's `error`.
    ///
    /// The text is the plugin's and is kept whole, so whatever repeats it is
    /// what bounds it and makes what a reader cannot see visible.
    Refused {
        /// The case it answered with, in the world's own spelling.
        case: &'static str,
        /// What the plugin said.
        why: String,
    },
}

/// The world's `error` a plugin answered `activate` with, as its case and its
/// text.
fn refused(refusal: types::Error) -> NotActivated {
    let (case, why) = match refusal {
        types::Error::Denied(why) => ("denied", why),
        types::Error::NotFound(why) => ("not-found", why),
        types::Error::Invalid(why) => ("invalid", why),
        types::Error::Conflict(why) => ("conflict", why),
        types::Error::Unavailable(why) => ("unavailable", why),
        types::Error::Internal(why) => ("internal", why),
    };
    NotActivated::Refused { case, why }
}

/// `pre` instantiated in a fresh store holding `state`, and asked to
/// activate.
///
/// **`activate` runs on the activation's fuel**, the catalog's
/// `fuel_per_activate` and not what a dispatch is given, under the plugin's
/// own epoch deadline. Nothing a plugin can do in it writes: it runs in no call
/// tree, so the store's writes, a blob and a dispatch are refused as from a
/// frame that may only read, and an event, a timer, a surface and the context
/// as from where nobody is acted for.
pub(crate) async fn activated<E: Environment>(
    engine: &Engine,
    pre: &PluginPre<HostState<E>>,
    state: HostState<E>,
    limits: &Limits,
) -> Result<Activated<E>, NotActivated> {
    let mut store = store_for(
        engine,
        state,
        nc_catalog::KERNEL.fuel_per_activate,
        limits.epoch_deadline_ticks(),
    )
    .map_err(NotActivated::Failed)?;
    let plugin = pre
        .instantiate_async(&mut store)
        .await
        .map_err(NotActivated::Failed)?;
    let manifest = plugin
        .noctua_kernel_guest()
        .call_activate(&mut store)
        .await
        .map_err(NotActivated::Failed)?
        .map_err(refused)?;
    Ok(Activated {
        instance: Instance { store, plugin },
        declaration: Declaration {
            commands: manifest.commands.into_iter().map(declared).collect(),
            subscriptions: manifest.subscriptions,
        },
    })
}

/// Instantiates `component` granted nothing, over an empty store held in
/// memory, and calls `activate`.
///
/// Answers the declared commands, every field of each, or the plugin's own
/// refusal as its case and its text.
#[cfg(any(test, feature = "test-support"))]
pub(crate) async fn activate(
    engine: &Engine,
    component: &Compiled,
) -> wasmtime::Result<Result<Vec<crate::DeclaredForTests>, String>> {
    let pre = linked::<crate::InMemory>(engine, component)?;
    let state = HostState::in_memory(crate::capability::GrantSet::none());
    match activated(engine, &pre, state, &Limits::catalog()).await {
        Ok(activated) => Ok(Ok(activated
            .declaration
            .commands
            .into_iter()
            .map(crate::DeclaredForTests::from)
            .collect())),
        Err(NotActivated::Refused { case, why }) => Ok(Err(format!("{case}: {why}"))),
        Err(NotActivated::Failed(error)) => Err(error),
    }
}

/// Instantiates `component` holding `grants`, over an empty store held in
/// memory, and calls `execute(name, args)`.
///
/// Answers what the plugin answered — its result, or its refusal in its
/// `Debug` form — with every host function it reached, every function it was
/// refused with how many times, and every line it logged, read out of its
/// store afterwards.
#[cfg(any(test, feature = "test-support"))]
pub(crate) async fn execute(
    engine: &Engine,
    component: &Compiled,
    grants: &crate::capability::GrantSet,
    name: &str,
    args: &str,
) -> wasmtime::Result<crate::ExecutedForTests> {
    let pre = linked::<crate::InMemory>(engine, component)?;
    let mut store = store_for(
        engine,
        HostState::in_memory(grants.clone()),
        nc_catalog::KERNEL.fuel_per_dispatch,
        nc_catalog::KERNEL.epoch_deadline_ticks,
    )?;
    let plugin = pre.instantiate_async(&mut store).await?;
    let answer = plugin
        .noctua_kernel_guest()
        .call_execute(&mut store, name, &args.to_owned())
        .await?
        .map_err(|refusal| format!("{refusal:?}"));
    let state = store.data();
    Ok(crate::ExecutedForTests {
        answer,
        reached: state.reached(),
        denied: state.denials().to_vec(),
        log: state
            .ring()
            .read()
            .lines()
            .iter()
            .map(|line| (line.level().word().to_owned(), line.text().to_owned()))
            .collect(),
    })
}

/// One declared command, out of the generated record and into the kernel's.
///
/// **Destructured, so a field the world adds is a compile error here** rather
/// than a field no test reads: this answered `command.name` alone, and the
/// other six crossed the boundary for nothing to look at.
fn declared(command: noctua::kernel::commands::Descriptor) -> Declared {
    use nc_schema::descriptor::Reversibility as Kernel;
    use noctua::kernel::commands::{Descriptor, Reversibility};

    let Descriptor {
        name,
        title,
        args_schema,
        mutating,
        action,
        resource_selector,
        reversibility,
    } = command;
    Declared {
        name,
        title,
        args_schema,
        mutating,
        action,
        resource_selector,
        reversibility: match reversibility {
            Reversibility::Read => Kernel::Read,
            Reversibility::Write => Kernel::Write,
            Reversibility::Irreversible => Kernel::Irreversible,
        },
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{KernelError, activated, linked, on_event, on_timer, run, types};
    use crate::capability::GrantSet;
    use crate::dispatch::Frame;
    use crate::engine::Engine;
    use crate::host::HostState;
    use crate::limits::{Limits, Lowering};
    use crate::supervisor::Kill;
    use nc_catalog::KERNEL;
    use types::Host as _;

    /// The catalog gives `activate` less fuel than a dispatch, which is what lets
    /// the fuel left after it say which of the two it ran on.
    const _: () = assert!(
        KERNEL.fuel_per_activate < KERNEL.fuel_per_dispatch,
        "the catalog gives activate less fuel than a dispatch, or what is left after it \
         cannot say which of the two it was given"
    );

    /// `activate` runs on the activation's fuel, and not on what a dispatch is
    /// given.
    ///
    /// **What is left is what tells the two apart.** The operator that gave
    /// `activate` no fuel at all proved only that some is given: handed
    /// `fuel_per_dispatch` instead, four times as much, every suite stayed
    /// green — measured — because the reference plugin activates well within
    /// either, so the fuel left after it is the one number that can say which.
    #[tokio::test]
    async fn activate_runs_on_the_activations_fuel_and_not_on_a_dispatchs() {
        let engine = Engine::new().expect("the engine's configuration is valid");
        let compiled = engine
            .compile(&crate::built::component("reference"))
            .expect("the reference plugin compiles");
        let pre = linked::<crate::InMemory>(&engine, &compiled).expect("it links");
        let activated = activated(
            &engine,
            &pre,
            HostState::in_memory(GrantSet::none()),
            &Limits::catalog(),
        )
        .await
        .expect("the reference plugin activates");
        let left = activated
            .instance
            .store
            .get_fuel()
            .expect("the engine meters fuel");
        assert!(
            left > 0 && left < KERNEL.fuel_per_activate,
            "activate left {left} of its fuel, and was given {} — anything above that is \
             a dispatch's",
            KERNEL.fuel_per_activate
        );
    }

    /// `activate` runs on the epoch ticks its manifest lowers, and not on the
    /// catalog's.
    ///
    /// The share the store was given is what is read: the reference plugin
    /// activates in far less than either, so no kill could tell them apart.
    /// **A verifier found it held by nothing**: `activate` given the catalog's
    /// ticks, a hundred times as many, left every suite green.
    #[tokio::test]
    async fn activate_runs_on_the_epoch_ticks_its_manifest_lowers() {
        /// The ticks the manifest lowers every call to.
        const TICKS: u64 = 5;
        /// Fewer than the catalog's, or the two cannot be told apart.
        const _: () = assert!(
            TICKS < KERNEL.epoch_deadline_ticks,
            "a lowering the same as the catalog's says nothing about which was given"
        );
        let engine = Engine::new().expect("the engine's configuration is valid");
        let compiled = engine
            .compile(&crate::built::component("reference"))
            .expect("the reference plugin compiles");
        let pre = linked::<crate::InMemory>(&engine, &compiled).expect("it links");
        let lowered = Limits::catalog()
            .lowered(&Lowering {
                epoch_deadline_ticks: Some(TICKS),
                ..Lowering::default()
            })
            .expect("a lowering within the catalog");
        let mut activated = activated(
            &engine,
            &pre,
            HostState::in_memory(GrantSet::none()),
            &lowered,
        )
        .await
        .expect("the reference plugin activates");
        let share = activated.instance.store.data_mut().running().share();
        assert_eq!(
            share,
            Duration::from_millis(KERNEL.epoch_tick_ms * TICKS),
            "activate was given {share:?} of the epoch, and its manifest leaves it {TICKS} \
             ticks of {} ms",
            KERNEL.epoch_tick_ms
        );
    }

    /// A dispatch runs on the fuel its manifest lowers, and not on the
    /// catalog's.
    ///
    /// **What is left is what tells the two apart.** A verifier handed every
    /// dispatch the catalog's fuel and every suite stayed green: the plugin
    /// that burns ran out of the catalog's two billion as well, before its
    /// epoch did, so its kill was the fuel's either way.
    #[tokio::test]
    async fn a_dispatch_runs_on_the_fuel_its_manifest_lowers() {
        /// The fuel the manifest lowers every dispatch to.
        const FUEL: u64 = 10_000_000;
        /// Less than the catalog's, or the two cannot be told apart.
        const _: () = assert!(
            FUEL < KERNEL.fuel_per_dispatch,
            "a lowering the same as the catalog's says nothing about which was given"
        );
        let engine = Engine::new().expect("the engine's configuration is valid");
        let compiled = engine
            .compile(&crate::built::component("reference"))
            .expect("the reference plugin compiles");
        let pre = linked::<crate::InMemory>(&engine, &compiled).expect("it links");
        let mut activated = activated(
            &engine,
            &pre,
            HostState::in_memory(GrantSet::none()),
            &Limits::catalog(),
        )
        .await
        .expect("the reference plugin activates");
        let lowered = Limits::catalog()
            .lowered(&Lowering {
                fuel_per_dispatch: Some(FUEL),
                ..Lowering::default()
            })
            .expect("a lowering within the catalog");
        activated.instance.enter(Frame::for_tests(false, None));
        let answered = run(
            &mut activated.instance,
            "reference.echo",
            r#"{"text":"hello"}"#,
            &lowered,
        )
        .await
        .expect("the command runs to its end");
        assert_eq!(answered, Ok("hello".to_owned()));
        let left = activated
            .instance
            .store
            .get_fuel()
            .expect("the engine meters fuel");
        assert!(
            left > 0 && left < FUEL,
            "a dispatch left {left} of the {FUEL} its manifest gives it"
        );
    }

    /// A callback runs on the fuel its manifest lowers, as a dispatch does,
    /// and not on what the plugin's last call left it.
    ///
    /// **Written by a reviewer (wave 5, isolation).** With the reset of the
    /// budget taken out of `on_timer` and `on_event` — fuel, epoch share and
    /// the tree a call is held to — every suite of this crate stayed green: a
    /// callback ran on the half a billion units `activate` left, and on the
    /// epoch deadline and the tree of the plugin's last call.
    #[tokio::test]
    async fn a_callback_runs_on_the_fuel_its_manifest_lowers_and_not_on_what_was_left() {
        /// The fuel the manifest lowers every dispatch and callback to.
        const FUEL: u64 = 10_000_000;
        let engine = Engine::new().expect("the engine's configuration is valid");
        let compiled = engine
            .compile(&crate::built::component("reference"))
            .expect("the reference plugin compiles");
        let pre = linked::<crate::InMemory>(&engine, &compiled).expect("it links");
        let mut activated = activated(
            &engine,
            &pre,
            HostState::in_memory(GrantSet::none()),
            &Limits::catalog(),
        )
        .await
        .expect("the reference plugin activates");
        let lowered = Limits::catalog()
            .lowered(&Lowering {
                fuel_per_dispatch: Some(FUEL),
                ..Lowering::default()
            })
            .expect("a lowering within the catalog");
        for event in [false, true] {
            // **Refilled past the bound before each**, so a callback that kept
            // what the one before it left is told from one given its own: run
            // second on the first one's remainder, an event's callback that
            // was given nothing passed.
            activated
                .instance
                .store
                .set_fuel(FUEL.saturating_mul(50))
                .expect("the engine meters fuel");
            let called = if event {
                on_event(&mut activated.instance, "peer.fired", "{}", &lowered).await
            } else {
                on_timer(&mut activated.instance, "log", &lowered).await
            };
            called.expect("the callback runs to its end");
            let left = activated
                .instance
                .store
                .get_fuel()
                .expect("the engine meters fuel");
            assert!(
                left > 0 && left < FUEL,
                "a callback (an event's: {event}) left {left} of the {FUEL} its manifest gives it"
            );
        }
    }

    /// A call in a call tree past its deadline is stopped as the tree's, at
    /// once, whatever share of the epoch its plugin has left.
    ///
    /// The plugin spins on the catalog's share, seconds of it, in a tree that
    /// began a whole deadline ago: held from the call's own start instead of
    /// the tree's, it spins until its fuel or its share runs out, and is
    /// stopped as either of those.
    #[tokio::test]
    async fn a_call_in_a_tree_past_its_deadline_is_stopped_as_the_trees() {
        let engine = Engine::new().expect("the engine's configuration is valid");
        let _ticker = engine.ticker().expect("the ticker starts");
        let compiled = engine
            .compile(&crate::built::component("reference"))
            .expect("the reference plugin compiles");
        let pre = linked::<crate::InMemory>(&engine, &compiled).expect("it links");
        let mut activated = activated(
            &engine,
            &pre,
            HostState::in_memory(GrantSet::none()),
            &Limits::catalog(),
        )
        .await
        .expect("the reference plugin activates");
        let long_ago = Instant::now()
            .checked_sub(Duration::from_millis(KERNEL.dispatch_deadline_ms))
            .expect("the machine has run longer than a call tree may");
        activated
            .instance
            .enter(Frame::for_tests(false, None).begun_for_tests(long_ago));
        let started = Instant::now();
        let stopped = run(
            &mut activated.instance,
            "reference.abuse.spin",
            r#"{"note":"photo"}"#,
            &Limits::catalog(),
        )
        .await
        .expect_err("a plugin that spins answers nothing");
        assert_eq!(Kill::of(&stopped), Kill::TreeDeadline, "{stopped:?}");
        let share = Duration::from_millis(KERNEL.epoch_tick_ms * KERNEL.epoch_deadline_ticks);
        assert!(
            started.elapsed() < share,
            "stopped after {:?}, not at its tree's deadline",
            started.elapsed()
        );
    }

    /// A call in a call tree past its deadline sends no request, and is
    /// stopped as the tree's at once, though it calls the host before it
    /// spins at all.
    ///
    /// The plugin reaches `net#fetch` before its first epoch check, against a
    /// transport that holds every request until the test is over — so a
    /// request sent waits out its host call's deadline. **A reviewer found
    /// a real bug with it**: the host call was held to its own deadline
    /// alone, and the epoch callback that reads the tree's runs only while the
    /// plugin runs — so the request was handed to the transport, and the call
    /// ran for as long as the transport took.
    #[tokio::test]
    async fn a_call_in_a_tree_past_its_deadline_sends_no_request_and_is_stopped_at_once() {
        let engine = Engine::new().expect("the engine's configuration is valid");
        let _ticker = engine.ticker().expect("the ticker starts");
        let compiled = engine
            .compile(&crate::built::component("reference"))
            .expect("the reference plugin compiles");
        let pre = linked::<crate::InMemory>(&engine, &compiled).expect("it links");
        let transport = std::sync::Arc::new(crate::net::Scripted::default());
        let state = HostState::<crate::InMemory>::new(
            GrantSet::of([nc_catalog::Capability::Net]),
            (
                HostState::<crate::InMemory>::home(),
                "tests:door".to_owned(),
            ),
            vec!["example.com".to_owned()],
            std::sync::Arc::new(crate::backend::MemoryBackend::default()),
            std::sync::Arc::new(crate::blobs::MemoryBlobs::default()),
            std::sync::Arc::clone(&transport),
            crate::limits::MemoryGuard::new(&Limits::catalog()),
            crate::host::log::SharedRing::default(),
        );
        let mut activated = activated(&engine, &pre, state, &Limits::catalog())
            .await
            .expect("the reference plugin activates");
        let long_ago = Instant::now()
            .checked_sub(Duration::from_millis(KERNEL.dispatch_deadline_ms))
            .expect("the machine has run longer than a call tree may");
        activated
            .instance
            .enter(Frame::for_tests(false, None).begun_for_tests(long_ago));
        let stall = transport.stall();
        let started = Instant::now();
        let stopped = run(
            &mut activated.instance,
            "reference.abuse.fetch",
            r#"{"note":"photo","url":"https://example.com/"}"#,
            &Limits::catalog(),
        )
        .await;
        let elapsed = started.elapsed();
        drop(stall);
        let sent = transport.asked().len();
        let share = Duration::from_millis(KERNEL.epoch_tick_ms * KERNEL.epoch_deadline_ticks);
        assert!(
            sent == 0 && elapsed < share,
            "a call in a tree past its deadline sent {sent} request(s) and ran {elapsed:?} \
             before it was stopped as {:?}",
            stopped.as_ref().map_err(Kill::of)
        );
        assert_eq!(
            stopped.as_ref().map_err(Kill::of).err(),
            Some(Kill::TreeDeadline),
            "{stopped:?}"
        );
    }

    #[test]
    fn a_refusal_reaches_the_plugin_as_a_value() {
        let refusal = types::Error::Denied("not granted".to_owned());
        let converted =
            HostState::in_memory(GrantSet::none()).convert_error(KernelError::from(refusal));
        assert!(
            matches!(converted, Ok(types::Error::Denied(ref why)) if why == "not granted"),
            "a refusal became a trap, and a trap kills the plugin it was refusing: \
             {converted:?}"
        );
    }

    #[test]
    fn a_fault_traps_rather_than_reaching_the_plugin() {
        let fault = wasmtime::Error::msg("the host overran its deadline");
        let converted =
            HostState::in_memory(GrantSet::none()).convert_error(KernelError::from(fault));
        assert!(
            converted.is_err_and(|trap| trap.to_string() == "the host overran its deadline"),
            "a fault was handed to the plugin as an ordinary refusal"
        );
    }
}
