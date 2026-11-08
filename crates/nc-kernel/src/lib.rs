//! The local kernel: the host of the plugin world (`NOCTUA.md §11`).
//!
//! [`world`] names the world the kernel hosts and pins its bytes, a private
//! `bindings` module is that world as wasmtime generates it for the host, and
//! [`engine`] is the runtime every plugin is compiled for — fuel and epoch
//! interruption switched on. A plugin's [`manifest`] names the [`capability`]
//! grants it asks for and the [`limits`] it lowers, and every host function of
//! the world asks for its capability before anything else, answering `denied`
//! to a plugin not granted it. A [`Kernel`] is opened over an [`Environment`]
//! — the [`backend`] its plugins' data is kept in, the journal, the clock, the
//! directory its decisions read and the [`logging`] sink its events go to —
//! once every unit a stopped process left open in the journal is recovered,
//! what the store kept of it reverted ([`Kernel::open`]),
//! and [`Kernel::load`] compiles a plugin, gives it a store of its own whose
//! memory grows against the manifest's bound, runs its `activate` and admits
//! what it declares into the [`registry`], or refuses it naming every problem.
//!
//! **[`Kernel::dispatch`] is the one door to every command**, a plugin's or the
//! kernel's own, and `commands.dispatch` is the same door from inside a
//! command: the decision point authorizes it with Cedar before anything runs,
//! journals it, and runs a command that changes state inside a unit of work
//! whose writes are kept whole or not at all. A plugin that traps, spins,
//! burns its fuel, outgrows its memory or waits on a host call past its
//! deadline is killed and started again, and quarantined until it is released
//! when it crashes too often (`crate::supervisor`, [`Kernel::plugin_status`],
//! [`Kernel::release`]). A plugin streams [`blobs`] in and out of a directory of
//! their own, each kept with the unit that made it or let go with it, and
//! [`Kernel::sweep_blobs`] deletes what no unit holds any more. A plugin
//! reaches the network through [`net`]: HTTPS to the names its manifest
//! declares, judged before anything is sent, through a [`net::Transport`] that
//! dials only public addresses and follows no redirect. A person steps back and
//! forward through their own history with `kernel.undo` and `kernel.redo`,
//! dispatched through the same door (`crate::history`): what a step kept in the
//! store and the blobs it made are put back, what it was allowed is authorized
//! again, its compensations are dispatched, and a step that left the machine
//! with none is one undo does not pass. A plugin groups what it does with
//! `commands.begin`, flattened into the unit of the command it runs in. The
//! kernel's own commands — the journal's chain verified, the local tables
//! pruned — run through the same door, as a guardian's (`crate::builtin`). A plugin
//! announces what happened in its own namespace, and an event published in a
//! unit is queued only when the unit is kept; it sets timers, and is woken as
//! the person it set them for; it describes what it shows in surfaces, in a
//! closed set of nodes whose interactions name registered commands; it reads
//! who its call runs for; and it writes to a log of its own, which never
//! reaches the central one ([`Kernel::plugin_log`]). [`Kernel::pump`] fires the
//! timers due and delivers the events queued, each callback a call tree of its
//! own, decided again before it runs, and holds the local database's tables to
//! their bounds; [`Kernel::surfaces`] lists what every plugin shows, and
//! [`Kernel::unload`] takes a plugin out with its commands, timers and
//! surfaces.
//!
//! **What this answers (`NOCTUA.md §2.4`).** A plugin is compiled from bytes on
//! every load and never read back as native code, so a file on disk cannot hand
//! the kernel machine code (see [`engine::Engine::compile`]); the world a plugin
//! links against is the reference world byte for byte (see [`world`]); no host
//! function runs for a plugin that was not granted its capability, which it
//! learns as a refusal it can read rather than as a crash; a command a plugin
//! declares is registered only when it can be authorized, in a namespace that
//! plugin alone holds; and no command runs before it is authorized and every
//! one is journaled before its answer is returned, with what it changed never
//! surviving a journal that did not take it, nor a process stopped before the
//! journal took its close — but for the cases `crate::units` names as not
//! answered, a unit opened past an entry of the journal the recovery cannot
//! read among them; a plugin that crashes, hangs or
//! exhausts its budget is stopped without harm to any other, each in a store
//! of its own; a blob is said to be on this machine only when it is, and
//! never to be on another; a plugin's request reaches a public address of a
//! name its manifest declares, or nothing; an undo reverses only the person's
//! own steps, with the authority they hold now; and a timer or an event runs
//! its callback only under an authority the decision point grants again when
//! it comes due. **What it does not answer:**
//! whether a plugin acts on the resource it was authorized for inside its own
//! namespace, which is the plugin's code, and timing between plugins, which
//! share one machine.
#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
// Every doctest of this crate forbids `unsafe` as the crate does: a
// doctest is a crate of its own, outside the root's `forbid`.
#![doc(test(attr(forbid(unsafe_code))))]

pub mod backend;
mod bindings;
pub mod blobs;
mod built;
mod builtin;
pub mod capability;
mod dispatch;
pub mod engine;
mod events;
mod history;
mod host;
pub mod limits;
pub mod logging;
pub mod manifest;
pub mod net;
mod pump;
pub mod registry;
mod supervisor;
mod timers;
mod ui;
mod units;
pub mod world;

pub use dispatch::{Approval, Caller, Confirmation, Refusal, RefusalKind};
pub use host::log::{LogLevel, LogLine, PluginLog};
pub use pump::Pumped;
pub use supervisor::{Kill, PluginStatus};
pub use ui::{Surface, SurfaceKind};

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use nc_catalog::ErrorCode;
use nc_ids::OpaqueId;

use crate::manifest::PluginManifest;

/// What a kernel runs on that is not the kernel's to choose.
///
/// Where its plugins' data is kept, where its history is written, what time
/// it is, who may do what, and where its events go.
pub trait Environment: Send + Sync + 'static {
    /// The store every plugin's data is kept in: `nc_local::LocalStore` in a
    /// daemon.
    type Store: backend::StoreBackend;
    /// Where the bytes of the blobs this machine holds are kept:
    /// [`blobs::DirectoryBlobs`] in a daemon.
    type Blobs: blobs::BlobStore;
    /// The journal's store: the realm's history, hash-chained.
    type Journal: nc_journal::Store + fmt::Debug + Send + Sync + 'static;
    /// The clock every decision, entry and unit is timed by.
    type Clock: nc_ids::Clock + fmt::Debug + Clone + Send + Sync + 'static;
    /// Where the decision point reads realms, grants and entities from.
    type Directory: nc_authz::Directory + fmt::Debug;
    /// Where the kernel's events go: `nc_log::Log` in a daemon.
    type Log: logging::LogSink;
    /// What sends a plugin's request once the kernel has judged it:
    /// [`net::UreqTransport`] in a daemon.
    type Transport: net::Transport;
}

/// What a kernel is opened with.
pub struct KernelConfig<E: Environment> {
    /// The store every plugin's data is kept in.
    pub store: E::Store,
    /// Where the bytes of the blobs this machine holds are kept; which unit
    /// made each, and whether one still holds it, is the store's.
    pub blobs: E::Blobs,
    /// The decision point, or why the pack it is built from could not be.
    ///
    /// **A kernel opens either way, and refuses what it cannot judge**: a
    /// plugin is not loaded while there is no decision point to hold its
    /// commands' actions to, and a dispatch is denied, each refusal carrying
    /// the pack's own code.
    pub pdp: Result<nc_authz::Pdp, nc_authz::PdpError>,
    /// The journal of the one realm this kernel serves.
    ///
    /// **Its realm is the kernel's**: every decision is asked about it, every
    /// unit the store keeps is recorded under it, and a kernel serves one realm
    /// — a principal of another holds no grant in this one.
    pub journal: nc_journal::Journal<E::Journal, E::Clock>,
    /// Where the decision point reads realms, grants and entities from.
    pub directory: E::Directory,
    /// The clock every decision, entry and unit is timed by.
    pub clock: E::Clock,
    /// Where the kernel's events go.
    pub log: E::Log,
    /// What sends a plugin's request once the kernel has judged it.
    pub transport: E::Transport,
}

impl<E: Environment> fmt::Debug for KernelConfig<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("KernelConfig")
            .field("store", &self.store)
            .field("blobs", &self.blobs)
            .field("pdp", &self.pdp)
            .field("journal", &self.journal)
            .field("directory", &self.directory)
            .field("clock", &self.clock)
            .field("log", &self.log)
            .field("transport", &self.transport)
            .finish()
    }
}

/// The local kernel: the plugins it loaded, and the door every command goes
/// through.
pub struct Kernel<E: Environment> {
    shared: Arc<Shared<E>>,
}

/// Everything a kernel holds, shared with the plugins' stores while a command
/// runs, so a dispatch from inside one reaches the same door.
pub(crate) struct Shared<E: Environment> {
    pub(crate) engine: engine::Engine,
    pub(crate) store: Arc<E::Store>,
    pub(crate) blobs: Arc<E::Blobs>,
    /// What every plugin's judged request is sent with.
    pub(crate) transport: Arc<E::Transport>,
    pub(crate) pdp: Result<nc_authz::Pdp, nc_authz::PdpError>,
    plugins: Mutex<Plugins<E>>,
    pub(crate) journal: tokio::sync::Mutex<nc_journal::Journal<E::Journal, E::Clock>>,
    pub(crate) directory: E::Directory,
    pub(crate) clock: E::Clock,
    pub(crate) log: E::Log,
    pub(crate) realm: OpaqueId,
    /// What every top-level dispatch takes first, so one call tree runs at a
    /// time.
    pub(crate) bus: tokio::sync::Mutex<()>,
    /// Every confirmation and approval issued and not yet redeemed.
    pub(crate) issued: Mutex<dispatch::Issued>,
    /// The events published and waiting for the pump.
    pub(crate) events: Mutex<events::Queue>,
    /// Every timer armed.
    pub(crate) timers: Mutex<timers::Timers>,
    /// Every surface open, and the tree each shows.
    pub(crate) surfaces: Mutex<ui::Surfaces>,
    /// Every unit this kernel left committed in the store and open in the
    /// journal — its close not taken, its reversal refused — until a prune
    /// reverts it first (`crate::units::reverted_again`): what the pruner may
    /// not take.
    ///
    /// **Held in memory, and enough so**: a kernel that stops loses it, and
    /// the recovery of the next one to open finds every such unit open in the
    /// journal before anything prunes.
    pub(crate) left_committed: Mutex<BTreeSet<nc_ids::Snowflake>>,
    /// The thread that advances the engine's epoch, for as long as the kernel
    /// is open.
    ticker: engine::Ticker,
}

/// Every plugin a kernel loaded, and the registry their commands are in.
///
/// **Behind a lock never held across an await.** A load takes it to see the
/// plugin's identity and namespace are free, lets go while the plugin is
/// compiled and activated, and takes it again to judge and register what it
/// declared — asking the same question again first, since another load may
/// have finished in between. A dispatch takes it to look a command up and to
/// take a plugin's store out of its slot, and again to put it back.
struct Plugins<E: Environment> {
    registry: registry::Registry,
    loaded: Vec<Loaded<E>>,
}

/// One loaded plugin: its manifest, what it is instantiated from, what it
/// declared, its store when it is not running, its kills, and its own log,
/// which every instance of it writes to.
struct Loaded<E: Environment> {
    manifest: PluginManifest,
    pre: bindings::PluginPre<host::HostState<E>>,
    declaration: bindings::Declaration,
    slot: supervisor::Slot<E>,
    supervision: supervisor::Supervision,
    log: host::log::SharedRing,
}

impl<E: Environment> fmt::Debug for Kernel<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let plugins = self.shared.plugins();
        formatter
            .debug_struct("Kernel")
            .field("store", &self.shared.store)
            .field("pdp", &self.shared.pdp)
            .field("realm", &self.shared.realm)
            .field("ticks", &self.shared.ticker.advanced())
            .field(
                "loaded",
                &plugins
                    .loaded
                    .iter()
                    .map(|loaded| loaded.manifest.id())
                    .collect::<Vec<_>>(),
            )
            .field("registry", &plugins.registry)
            .finish_non_exhaustive()
    }
}

impl<E: Environment> Kernel<E> {
    /// Opens a kernel over `config`, with no plugin loaded and the catalog's
    /// own commands registered, once every unit a stopped process left open is
    /// recovered.
    ///
    /// **Recovered before anything runs** (`crate::units`): every unit of
    /// this realm the journal holds opened and never closed is closed as
    /// aborted, what the store kept of it — a process stopped between the
    /// store's commit and the journal's close — reverted first, newest first,
    /// and each is logged as `kernel.recovered` with `[NC-KERN-0016]`. So a
    /// kernel opens over a store holding nothing of a unit it found open, and it
    /// finds every one the journal's store names as open
    /// (`nc_journal::Store::open_units`), which no entry the journal cannot
    /// decode hides. The cases `crate::units` names as not answered are not
    /// closed here.
    ///
    /// # Errors
    ///
    /// [`OpenError::Engine`] when wasmtime refuses the engine's configuration,
    /// and [`OpenError::Unrecovered`] when a unit left open could not be
    /// recovered: the journal could not say which units are open or would not
    /// take a close,
    /// or the store could not be read or refused its reversal, which is logged
    /// as `kernel.revert_failed`. Nothing runs over a store holding what the
    /// journal never kept, so the kernel does not open, and opening it again
    /// tries again.
    pub async fn open(config: KernelConfig<E>) -> Result<Self, OpenError> {
        let realm = config.journal.realm().clone();
        let engine = engine::Engine::new().map_err(OpenError::Engine)?;
        let ticker = engine.ticker().map_err(OpenError::Engine)?;
        let kernel = Self {
            shared: Arc::new(Shared {
                engine,
                store: Arc::new(config.store),
                blobs: Arc::new(config.blobs),
                transport: Arc::new(config.transport),
                pdp: config.pdp,
                plugins: Mutex::new(Plugins {
                    registry: registry::Registry::from_catalog(),
                    loaded: Vec::new(),
                }),
                journal: tokio::sync::Mutex::new(config.journal),
                directory: config.directory,
                clock: config.clock,
                log: config.log,
                realm,
                bus: tokio::sync::Mutex::new(()),
                issued: Mutex::new(dispatch::Issued::default()),
                events: Mutex::new(events::Queue::default()),
                timers: Mutex::new(timers::Timers::default()),
                surfaces: Mutex::new(ui::Surfaces::default()),
                left_committed: Mutex::new(BTreeSet::new()),
                ticker,
            }),
        };
        units::recover(&kernel.shared)
            .await
            .map_err(OpenError::Unrecovered)?;
        Ok(kernel)
    }

    /// Every command this kernel can dispatch, in name order: its own and every
    /// loaded plugin's.
    #[must_use]
    pub fn registry(&self) -> Vec<registry::RegisteredCommand> {
        self.shared.plugins().registry.listed()
    }

    /// Dispatches the command `name` with `args` for `caller`, as the top of a
    /// call tree, and answers its result.
    ///
    /// Through the decision point, every step of it in order (see the module
    /// `dispatch`'s header): nothing runs unless it is authorized, everything
    /// is journaled, and a command that changes state runs inside a unit kept
    /// whole or not at all.
    ///
    /// **A caller that stops waiting does not stop the dispatch.** The tree
    /// runs on a task of its own, to its end. One stopped part-way leaves its
    /// unit open in the journal, which then opens no other, and its plugin's
    /// store dropped in the middle of a call — measured, with the tree awaited
    /// here, every later dispatch to that plugin would be refused as a dispatch
    /// into itself, for as long as the kernel runs. What a caller who gave up
    /// loses is the answer; the journal still has it.
    ///
    /// # Errors
    ///
    /// A [`Refusal`], in the world's case and with its code: what the decision
    /// point refused and why, what the command's plugin answered with when it
    /// refused, trapped or answered what is not a payload, and what the store or
    /// the journal refused when the unit was closed.
    pub async fn dispatch(
        &self,
        caller: &Caller,
        name: &str,
        args: &str,
    ) -> Result<String, Refusal> {
        dispatch::detached(&self.shared, caller, name, args).await
    }

    /// Loads the plugin `manifest` describes from `component`'s bytes.
    ///
    /// In order: the plugin's identity and namespace must be free, and its
    /// namespace not one the kernel keeps; the bytes are compiled, here, and
    /// linked against the world; the plugin is instantiated in a store of its
    /// own, holding the grants its manifest names, its memory held to the
    /// manifest's bound, and its `activate` is run on the activation's fuel;
    /// and what it declared is judged and registered as one — every command or
    /// none. Its store reads the plugin's own keys of this kernel's store, so a
    /// plugin granted `store` can read in `activate` what it should declare.
    ///
    /// # Errors
    ///
    /// A [`LoadError`] naming every problem, each with its code: the decision
    /// point's own when it could not be built; `[NC-PLUG-0022]` for an identity
    /// or a namespace another loaded plugin holds, or a namespace the kernel
    /// keeps; `[NC-PLUG-0011]` for bytes that are not a component this kernel
    /// compiles, or that do not link against the world, what wasmtime said of
    /// them repeated cut to a bound; `[NC-PLUG-0021]` for a plugin that
    /// trapped, ran out of fuel, outgrew its memory or refused while it was
    /// instantiated or activated, its own refusal repeated cut to a bound; and
    /// every problem `activate`'s declarations have —
    /// `[NC-PLUG-0008]`, `[NC-PLUG-0009]` and `[NC-KERN-0015]`, as the
    /// [`registry`] module lists them.
    pub async fn load(
        &self,
        manifest: PluginManifest,
        component: &[u8],
    ) -> Result<PluginSlot, LoadError> {
        let plugin = manifest.id().to_owned();
        let pdp = self.shared.pdp.as_ref().map_err(|unbuilt| {
            LoadError::one(
                unbuilt.code(),
                format!(
                    "`{plugin}` was not loaded, because the decision point its commands are \
                     judged against could not be built: {unbuilt}"
                ),
            )
        })?;
        self.shared.plugins().vacant(&manifest)?;
        let compiled = self.shared.engine.compile(component).map_err(|refused| {
            LoadError::one(
                ErrorCode::PluginComponentInvalid,
                format!(
                    "`{plugin}` is not a component this kernel compiles: {}",
                    registry::quoted_up_to(&refused.to_string(), REFUSAL_QUOTED)
                ),
            )
        })?;
        let pre = bindings::linked::<E>(&self.shared.engine, &compiled).map_err(|refused| {
            LoadError::one(
                ErrorCode::PluginComponentInvalid,
                format!(
                    "`{plugin}` does not link against the world `plugin`: {}",
                    registry::quoted_up_to(&format!("{refused:#}"), REFUSAL_QUOTED)
                ),
            )
        })?;
        let log = host::log::SharedRing::default();
        let state = host::HostState::for_plugin(&manifest, &self.shared, Arc::clone(&log));
        let mut activated =
            bindings::activated(&self.shared.engine, &pre, state, manifest.limits())
                .await
                .map_err(|failed| {
                    LoadError::one(
                        ErrorCode::PluginActivationFailed,
                        match failed {
                            bindings::NotActivated::Failed(error) => {
                                format!("`{plugin}` did not activate: {}", error.root_cause())
                            }
                            bindings::NotActivated::Refused { case, why } => format!(
                                "`{plugin}` refused to activate, answering `{case}` with {}",
                                registry::quoted_up_to(&why, REFUSAL_QUOTED)
                            ),
                        },
                    )
                })?;
        let mut plugins = self.shared.plugins();
        plugins.vacant(&manifest).map_err(LoadError::raced)?;
        let slot = PluginSlot(plugins.loaded.len());
        let admitted = plugins
            .registry
            .admit(
                &manifest,
                slot,
                &activated.declaration.commands,
                &activated.declaration.subscriptions,
                pdp,
            )
            .map_err(|problems| LoadError { problems })?;
        plugins.registry.register(admitted);
        // **What `activate` was refused is logged once the plugin has a slot to
        // be named by**, as what a command is refused is once it returns. Left
        // for later, a capability the plugin was denied in `activate` would be
        // logged on the return of a later, unrelated call, or never, for a
        // plugin nothing dispatches.
        dispatch::left(&self.shared, slot, &mut activated.instance);
        plugins.loaded.push(Loaded {
            manifest,
            pre,
            declaration: activated.declaration,
            slot: supervisor::Slot::Ready(activated.instance),
            supervision: supervisor::Supervision::default(),
            log,
        });
        Ok(slot)
    }

    /// Fires every timer due, delivers every event queued, and holds the
    /// local database's tables to their bounds, answering what each came to.
    ///
    /// **Between call trees, in that order** (`crate::pump`): it takes the bus
    /// every top-level dispatch takes, fires the timers due first, then
    /// delivers the events that were queued when it began — so what a callback
    /// publishes waits for the next pump — and runs the one pruner last, once
    /// every unit this kernel left committed in the store and open in the
    /// journal is reverted, and not while one stays. Each
    /// timer and each event is a callback in a call tree of its own, for the
    /// principal it was set or published for, the authority it was set or
    /// published under decided again before it runs. A daemon runs it from time
    /// to time; nothing here does on its own, and it runs to its end whoever
    /// stops waiting for it.
    pub async fn pump(&self) -> Pumped {
        pump::pumped(&self.shared).await
    }

    /// What the kernel keeps of the log of the plugin in `slot`, if this kernel
    /// loaded one there: its newest lines, and how many older ones went.
    ///
    /// **The plugin's own text, kept here and nowhere else**: it never
    /// reaches the central log, whose type wall keeps text out (`NOCTUA.md
    /// §15.4`), and it outlives the plugin's kills, the ring being the slot's.
    #[must_use]
    pub fn plugin_log(&self, slot: PluginSlot) -> Option<PluginLog> {
        let plugins = self.shared.plugins();
        let loaded = plugins.loaded.get(slot.index())?;
        let read = dispatch::lock(&loaded.log).read();
        Some(read)
    }

    /// Every surface a plugin holds open, with the last tree kept for each.
    ///
    /// In the order their identifiers sort: what a renderer, Stage 16's,
    /// draws.
    #[must_use]
    pub fn surfaces(&self) -> Vec<Surface> {
        dispatch::lock(&self.shared.surfaces).listed()
    }

    /// Unloads the plugin in `slot`, answering whether one was loaded there.
    ///
    /// Between call trees, as a pump is: its commands leave the registry, its
    /// timers are cancelled, its surfaces closed, the events it published and
    /// nobody has heard yet let go, and its instance dropped. What it kept in
    /// the store stays the plugin's, under its identity, which a plugin of the
    /// same identity loaded later reads — and so do the compensations its
    /// commands were registered as, which an undo dispatches only to a plugin
    /// of that identity, whoever takes its namespace meanwhile; the slot is
    /// never used again, and [`Kernel::plugin_status`] knows nothing of it.
    pub async fn unload(&self, slot: PluginSlot) -> bool {
        let _no_tree_runs = self.shared.bus.lock().await;
        let unloaded = {
            let mut plugins = self.shared.plugins();
            let held = plugins
                .loaded
                .get_mut(slot.index())
                .filter(|loaded| !matches!(loaded.slot, supervisor::Slot::Unloaded));
            match held {
                Some(loaded) => {
                    loaded.slot = supervisor::Slot::Unloaded;
                    plugins.registry.forget(slot);
                    true
                }
                None => false,
            }
        };
        if unloaded {
            dispatch::lock(&self.shared.timers).forget(slot);
            dispatch::lock(&self.shared.surfaces).forget(slot);
            dispatch::lock(&self.shared.events).forget(slot);
        }
        unloaded
    }

    /// How the plugin in `slot` stands, if this kernel loaded one there.
    ///
    /// Which instance of it runs, how often it was killed and why last, and
    /// whether it is quarantined.
    #[must_use]
    pub fn plugin_status(&self, slot: PluginSlot) -> Option<PluginStatus> {
        self.shared.status(slot)
    }

    /// Releases the plugin in `slot`: forgets every kill it had, and starts it
    /// again when it is quarantined.
    ///
    /// A slot this kernel loaded no plugin in holds nothing to release, and is
    /// answered as released. So is a plugin another release is starting
    /// already: that one starts it, nothing is dispatched to it until it has,
    /// and [`Kernel::plugin_status`] says whether it came back.
    ///
    /// **A caller that stops waiting does not stop the release**, which runs
    /// on a task of its own to its end, as a dispatch does: a release claims
    /// the plugin before it starts it, and one stopped part-way would leave
    /// it claimed for good.
    ///
    /// # Errors
    ///
    /// A [`Refusal`], `unavailable` with `[NC-PLUG-0007]`, when the plugin was
    /// quarantined and does not come back as it was — it does not activate, or
    /// declares otherwise than it was registered — or the task the release ran
    /// in did not finish, and so stays quarantined.
    pub async fn release(&self, slot: PluginSlot) -> Result<(), Refusal> {
        supervisor::released(&self.shared, slot).await
    }

    /// Sweeps the blobs, and answers how many of each kind went.
    ///
    /// It deletes the ones no committed unit has held for their
    /// `blob_orphan_grace_ms`, row and bytes, the bytes no row names and every
    /// writing no writer holds — bytes only once nobody has named them for
    /// that grace, so another realm's kernel over the same bytes keeps what
    /// its units in flight made.
    ///
    /// **Between call trees, never during one.** It takes the bus every
    /// top-level dispatch takes, because the blobs a unit makes are bytes no
    /// row names until the unit commits, and a sweep inside a tree would take
    /// them for orphans. A daemon runs it from time to time; nothing here does
    /// on its own.
    ///
    /// # Errors
    ///
    /// A [`Refusal`] with the code of what refused — the store, with its own,
    /// or the blobs' bytes, `[NC-BLOB-0004]` — and what was swept before it
    /// stays swept.
    pub async fn sweep_blobs(&self) -> Result<blobs::Swept, Refusal> {
        use nc_ids::Clock as _;

        let _between_trees = self.shared.bus.lock().await;
        blobs::sweep(
            &*self.shared.store,
            &*self.shared.blobs,
            self.shared.clock.now_ms(),
        )
        .await
    }

    /// A human's confirmation of `principal` dispatching `name` with `args`,
    /// as the kernel issues one. **For tests only.**
    ///
    /// What issues one for real is the interface a person confirms in, a later
    /// stage's; this door is how a suite holds one, and it is the kernel's own
    /// issuing — bound, single-use and timed exactly as that one will be.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    #[must_use]
    pub fn confirmation_for_tests(
        &self,
        principal: &nc_authz::Principal,
        name: &str,
        args: &str,
    ) -> Confirmation {
        Confirmation::issued(self.issued_for_tests(
            dispatch::Token::Confirmation,
            principal,
            name,
            args,
        ))
    }

    /// A quorum's approval of `principal` dispatching `name` with `args`, as
    /// the kernel issues one. **For tests only.**
    ///
    /// The approval flow that collects a quorum is a later stage's; until it
    /// exists a quorum is met only by an approval this door issued.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    #[must_use]
    pub fn approval_for_tests(
        &self,
        principal: &nc_authz::Principal,
        name: &str,
        args: &str,
    ) -> Approval {
        Approval::issued(self.issued_for_tests(dispatch::Token::Approval, principal, name, args))
    }

    /// Every entry of this kernel's journal, in order. **For tests only.**
    ///
    /// What a suite holds the decision point to is what it wrote, and the
    /// journal is the kernel's to hold: this reads it through the journal's
    /// own store, whichever store that is.
    ///
    /// # Errors
    ///
    /// What the journal's store refused the read with, when it
    /// refused it.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub async fn journal_for_tests(&self) -> Result<Vec<nc_journal::Entry>, nc_journal::Error> {
        use nc_journal::Store as _;

        let journal = self.shared.journal.lock().await;
        Ok(journal.store().from(0).await?.entries)
    }

    /// Issues `token` for `principal` to dispatch `name` with `args`, read as
    /// canonical JSON when they are.
    #[cfg(any(test, feature = "test-support"))]
    fn issued_for_tests(
        &self,
        token: dispatch::Token,
        principal: &nc_authz::Principal,
        name: &str,
        args: &str,
    ) -> u64 {
        use nc_ids::Clock as _;

        let canonical =
            nc_canon::parse(args).map_or_else(|_| args.to_owned(), |value| value.to_canonical());
        let holder = self
            .shared
            .command(name)
            .and_then(|admitted| history::owner_of(&self.shared, &admitted));
        dispatch::lock(&self.shared.issued).issue(
            token,
            principal,
            (name, holder.as_deref()),
            &canonical,
            self.shared.clock.now_ms(),
        )
    }
}

impl<E: Environment> Shared<E> {
    /// The plugins and the registry, whoever last held them.
    ///
    /// Nothing panics while holding the lock, and a load changes them only
    /// once it has judged everything.
    fn plugins(&self) -> MutexGuard<'_, Plugins<E>> {
        self.plugins.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The command registered as `name`, if one is.
    pub(crate) fn command(&self, name: &str) -> Option<registry::Admitted> {
        self.plugins().registry.lookup(name)
    }

    /// Whether the command registered as `name` is not the plugin `plugin`'s
    /// own: the kernel's, or one another plugin declared.
    ///
    /// A name nothing registered is nobody's, and so not foreign: what it
    /// names is refused as unknown wherever it is asked for.
    pub(crate) fn foreign_to(&self, name: &str, plugin: &str) -> bool {
        let plugins = self.plugins();
        plugins
            .registry
            .lookup(name)
            .is_some_and(|command| match command.listed().owner {
                registry::Owner::Kernel => true,
                registry::Owner::Plugin(slot) => plugins
                    .loaded
                    .get(slot.index())
                    .is_none_or(|loaded| loaded.manifest.id() != plugin),
            })
    }

    /// The quota each of `plugins` is held to by its manifest, by identity.
    pub(crate) fn lowered(&self, plugins: &BTreeSet<String>) -> BTreeMap<String, u64> {
        self.plugins()
            .loaded
            .iter()
            .filter(|loaded| plugins.contains(loaded.manifest.id()))
            .map(|loaded| {
                (
                    loaded.manifest.id().to_owned(),
                    loaded.manifest.limits().store_entries(),
                )
            })
            .collect()
    }
}

impl<E: Environment> Shared<E> {
    /// The plugins whose activation subscribed to a pattern hearing `topic`.
    ///
    /// In the order they were loaded: every one loaded, quarantined or not, and
    /// none unloaded.
    pub(crate) fn subscribers(&self, topic: &str) -> Vec<PluginSlot> {
        self.plugins()
            .loaded
            .iter()
            .enumerate()
            .filter(|(_, loaded)| {
                !loaded.unloaded()
                    && loaded
                        .declaration
                        .subscriptions
                        .iter()
                        .any(|pattern| events::hears(pattern, topic))
            })
            .map(|(at, _)| PluginSlot(at))
            .collect()
    }

    /// The namespace of the plugin in `slot`, as its manifest names it.
    pub(crate) fn namespace_of(&self, slot: PluginSlot) -> Option<String> {
        self.plugins()
            .loaded
            .get(slot.index())
            .map(|loaded| loaded.manifest.namespace().to_owned())
    }

    /// Whether a plugin of the identity `plugin` is loaded, and its manifest
    /// grants `capability`.
    pub(crate) fn grants(&self, plugin: &str, capability: nc_catalog::Capability) -> bool {
        self.plugins().loaded.iter().any(|loaded| {
            !loaded.unloaded()
                && loaded.manifest.id() == plugin
                && loaded.manifest.grants().holds(capability)
        })
    }

    /// The identity of the plugin in `slot`, as its manifest names it.
    pub(crate) fn plugin_of(&self, slot: PluginSlot) -> Option<String> {
        self.plugins()
            .loaded
            .get(slot.index())
            .map(|loaded| loaded.manifest.id().to_owned())
    }
}

impl<E: Environment> Loaded<E> {
    /// Whether the plugin was unloaded, which nothing runs again.
    const fn unloaded(&self) -> bool {
        matches!(self.slot, supervisor::Slot::Unloaded)
    }
}

impl<E: Environment> Plugins<E> {
    /// Refuses `manifest` when its namespace is one the kernel keeps, or its
    /// identity or its namespace is a loaded plugin's.
    ///
    /// **One namespace, one plugin.** Its store is the plugin's by identity and
    /// its commands are the namespace's, so a second plugin under the same
    /// namespace could declare commands in the first one's name. The refusal is
    /// the plugin's own code, `[NC-PLUG-0022]`, and not the one a command's name
    /// is refused with: nothing was declared yet, and a refusal that said a
    /// command was would send its reader to look for one.
    fn vacant(&self, manifest: &PluginManifest) -> Result<(), LoadError> {
        let (plugin, namespace) = (manifest.id(), manifest.namespace());
        let mut held = self.loaded.iter().filter(|loaded| !loaded.unloaded());
        let taken = if registry::RESERVED.contains(&namespace) {
            format!("asks for the namespace `{namespace}.`, which the kernel keeps for its own")
        } else if held.clone().any(|loaded| loaded.manifest.id() == plugin) {
            "is loaded already".to_owned()
        } else if let Some(holder) = held.find(|loaded| loaded.manifest.namespace() == namespace) {
            format!(
                "asks for the namespace `{namespace}.`, which `{}` holds",
                holder.manifest.id()
            )
        } else {
            return Ok(());
        };
        Err(LoadError::one(
            ErrorCode::PluginIdentityTaken,
            format!("`{plugin}` {taken}"),
        ))
    }
}

/// How many characters of a sentence a load's refusal repeats: a plugin's own
/// refusal to activate, or what wasmtime said of bytes it would not compile or
/// link.
///
/// A sentence, where the registry holds an action, an entity type or a topic
/// to sixty-four and repeats a command's name only when it is well formed, and
/// bounded for the same reason: the plugin wrote it, and the refusal is the
/// kernel's — measured, unbounded, a plugin seeded to refuse with a million
/// characters would be repeated whole, a sentence of 1,000,157 characters.
/// wasmtime's sentence quotes the component's own names, and a component
/// importing a function named with 100,000 characters would be refused in one
/// of 100,185.
const REFUSAL_QUOTED: usize = 256;

/// Where a loaded plugin sits in its kernel: its place in the order plugins
/// were loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PluginSlot(usize);

impl PluginSlot {
    /// Its place in the order plugins were loaded, from zero.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }
}

/// Why a plugin was not loaded: every problem found, each with its own code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadError {
    problems: Vec<LoadProblem>,
}

impl LoadError {
    /// A refusal for one reason.
    fn one(code: ErrorCode, sentence: String) -> Self {
        Self {
            problems: vec![LoadProblem::new(code, sentence)],
        }
    }

    /// Every problem, in the order found.
    #[must_use]
    pub fn problems(&self) -> &[LoadProblem] {
        &self.problems
    }

    /// The same refusal, met once the plugin had activated: a load that
    /// finished first took what this one asked for.
    fn raced(self) -> Self {
        Self {
            problems: self
                .problems
                .into_iter()
                .map(|problem| LoadProblem {
                    sentence: format!(
                        "{} — by a load that finished while this one activated",
                        problem.sentence
                    ),
                    ..problem
                })
                .collect(),
        }
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (at, problem) in self.problems.iter().enumerate() {
            if at > 0 {
                formatter.write_str("; ")?;
            }
            write!(formatter, "{problem}")?;
        }
        Ok(())
    }
}

impl std::error::Error for LoadError {}

/// One reason a plugin was not loaded: its code, and the sentence naming what
/// was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadProblem {
    code: ErrorCode,
    sentence: String,
}

impl LoadProblem {
    /// A problem carrying `code`, said as `sentence`.
    pub(crate) const fn new(code: ErrorCode, sentence: String) -> Self {
        Self { code, sentence }
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

impl fmt::Display for LoadProblem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "[{}] {}", self.code, self.sentence)
    }
}

/// Why a kernel did not open.
#[derive(Debug)]
pub enum OpenError {
    /// The engine's configuration, which wasmtime refused.
    Engine(engine::EngineError),
    /// A unit a stopped process left open was not recovered, so what the
    /// store kept of it may still be there: the refusal says what refused.
    Unrecovered(Refusal),
}

impl fmt::Display for OpenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Engine(refused) => write!(formatter, "{refused}"),
            Self::Unrecovered(refusal) => write!(
                formatter,
                "the kernel did not open, because a unit a stopped process left open was not \
                 recovered: {refusal}"
            ),
        }
    }
}

impl std::error::Error for OpenError {}

/// The environment the test doors and this crate's own tests run a plugin in:
/// an empty store held in memory.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug)]
pub(crate) struct InMemory;

#[cfg(any(test, feature = "test-support"))]
impl Environment for InMemory {
    type Store = backend::MemoryBackend;
    type Blobs = blobs::MemoryBlobs;
    type Journal = nc_journal::InMemory;
    type Clock = nc_ids::SystemClock;
    type Directory = nc_authz::MemoryDirectory;
    type Log = logging::Captured;
    type Transport = net::Scripted;
}

/// The component built from `plugins/<name>`, once it is shown to be the one
/// its sources build today. **For tests only.**
///
/// The kernel's suites load a plugin through this and nothing else. `cargo
/// xtask build --plugins` writes the component beside the list of every file it
/// was built from, and this reads the list back and panics — naming the command
/// that fixes it — on a component that is missing or stale, rather than let a
/// suite run against a plugin nobody wrote.
///
/// # Panics
///
/// When the component is missing, or any file it was built from has changed,
/// gone or been joined by another since.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
#[must_use]
pub fn component_for_tests(name: &str) -> Vec<u8> {
    built::component(name)
}

/// One command as a plugin declared it at `activate`: every field of the
/// world's `descriptor` record. **For tests only.**
///
/// The kernel's own type, because the generated record stays inside the
/// private `bindings` module, which says why. **Every field, not the name.** A
/// suite that held only the names would pass a component whose adapter
/// declared its read as mutating, under an empty action and with the other
/// reversibility — measured, all three planted in
/// `plugins/reference/src/adapter.rs` and the component built — because no
/// host test reads the adapter, which is compiled for `wasm32` alone.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredForTests {
    /// Its name, namespaced by the plugin.
    pub name: String,
    /// The label a person sees.
    pub title: String,
    /// The JSON Schema of its arguments, as the plugin wrote it.
    pub args_schema: String,
    /// Whether it mutates state.
    pub mutating: bool,
    /// The Cedar action it is decided under.
    pub action: String,
    /// How the resource is taken from its arguments.
    pub resource_selector: String,
    /// `read`, `write` or `irreversible`: the world's own spelling of the case.
    pub reversibility: &'static str,
}

#[cfg(any(test, feature = "test-support"))]
impl From<registry::Declared> for DeclaredForTests {
    fn from(declared: registry::Declared) -> Self {
        use nc_schema::descriptor::Reversibility;

        let registry::Declared {
            name,
            title,
            args_schema,
            mutating,
            action,
            resource_selector,
            reversibility,
        } = declared;
        Self {
            name,
            title,
            args_schema,
            mutating,
            action,
            resource_selector,
            reversibility: match reversibility {
                Reversibility::Read => "read",
                Reversibility::Write => "write",
                Reversibility::Irreversible => "irreversible",
            },
        }
    }
}

/// Instantiates `component` and calls its `activate`, answering every command
/// it declares, field for field. **For tests only.**
///
/// What a component declares as it declares it, outside any kernel and before
/// the registry judges it: `crates/nc-kernel/tests/the_reference_plugin_activates.rs`
/// hands it the component `cargo xtask build --plugins` wrote, and holds what
/// comes back to the plugin's own table field for field — which `Kernel::load`
/// cannot show, since it registers what it admits and refuses the rest.
/// **Granted nothing**, over an empty store in memory: every host function of
/// the world is linked, and each answers `denied`, so a plugin that reaches for
/// the host during `activate` is told no rather than killed, and the link
/// itself is the proof that the world it was built against is the world this
/// kernel hosts.
///
/// # Errors
///
/// Returns what wasmtime refused — the link, the instantiation or the call — as
/// a sentence, and the plugin's own `error` value as its case and its text.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub async fn activate_for_tests(
    engine: &engine::Engine,
    component: &engine::Compiled,
) -> Result<Vec<DeclaredForTests>, String> {
    bindings::activate(engine, component)
        .await
        .map_err(|error| format!("{error:?}"))?
}

/// What one command answered, run by `execute_for_tests`, and what it reached
/// on the way. **For tests only.**
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutedForTests {
    /// The command's result, or the world's `error` it refused with, written
    /// in its `Debug` form.
    pub answer: Result<String, String>,
    /// Every host function the plugin reached, in order.
    pub reached: Vec<capability::Linked>,
    /// Every function the plugin was refused, once, in the order each was
    /// first refused, with how many times it was.
    pub denied: Vec<(capability::Denial, u64)>,
    /// Every line the plugin logged, as its level's word and its text.
    pub log: Vec<(String, String)>,
}

/// Instantiates `component` holding `grants` and runs one of its commands,
/// for tests only.
///
/// The coverage test (`crates/nc-kernel/tests/capabilities.rs`) runs the
/// reference plugin's probe through it, over an empty store in memory, and
/// reads back every host function the probe reached. **A door, and nothing
/// else may be one**: it runs a command with no decision point in front of it
/// and in no call tree, which in a shipped kernel would be a second way to
/// every mutation — so it exists behind `test-support` alone, and a host
/// function that needs a call tree answers it as it answers `activate`.
///
/// # Errors
///
/// Returns what wasmtime refused — the link, the instantiation or the call,
/// a trap included — as a sentence.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub async fn execute_for_tests(
    engine: &engine::Engine,
    component: &engine::Compiled,
    grants: &capability::GrantSet,
    name: &str,
    args: &str,
) -> Result<ExecutedForTests, String> {
    bindings::execute(engine, component, grants, name, args)
        .await
        .map_err(|error| format!("{error:?}"))
}

#[cfg(test)]
mod tests {
    use super::{LoadError, LoadProblem, PluginSlot};
    use nc_catalog::ErrorCode;

    /// A refusal of several problems says every one, each opening on its own
    /// code, in the order found.
    #[test]
    fn a_refusal_names_every_problem_each_opening_on_its_own_code() {
        let refused = LoadError {
            problems: vec![
                LoadProblem::new(
                    ErrorCode::PluginDescriptorMissingField,
                    "`notes.a` of `noctua:notes` declares no action".to_owned(),
                ),
                LoadProblem::new(
                    ErrorCode::KernelNameTaken,
                    "`kernel.b` of `noctua:notes` is in the namespace `kernel.`".to_owned(),
                ),
            ],
        };
        assert_eq!(
            refused.to_string(),
            format!(
                "[{}] `notes.a` of `noctua:notes` declares no action; [{}] `kernel.b` of \
                 `noctua:notes` is in the namespace `kernel.`",
                ErrorCode::PluginDescriptorMissingField,
                ErrorCode::KernelNameTaken
            )
        );
        assert_eq!(refused.problems()[1].code(), ErrorCode::KernelNameTaken);
        assert_eq!(PluginSlot(4).index(), 4);
    }
}
