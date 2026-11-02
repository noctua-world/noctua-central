//! The host half of the world: what one plugin's store holds, and the door
//! every host function goes through first.
//!
//! **One [`HostState`] per plugin store**, holding the plugin's grants, its
//! identity — the one its keys are kept under, bound from its manifest and
//! never from anything the plugin says — the domains its manifest declares,
//! the store its data is in, the transport its requests are sent with, the
//! guard its memory grows against, the record of every host function it
//! reached, the denials it was answered — once per function refused, with how
//! many times — the reasons its fetches were refused for, once each, the codes
//! its calls were refused with for where they ran, once each, and its own log. Each interface of the world is implemented in a module of
//! its own below, against the private `bindings`, generic over the kernel's
//! [`Environment`], and every function's first statement is one of four:
//!
//! - [`HostState::require`], for a function whose result carries the world's
//!   `error`: a capability not granted is answered `denied` as that value;
//! - [`HostState::require_or_trap`], for the two that carry none —
//!   `reader.size` and `unit.abort` — which refuses by trapping, and is safe
//!   only because each is reachable through a handle that `blobs.open` or
//!   `commands.begin` hands out, under the same capability: a plugin denied it
//!   never holds one to call them on;
//! - [`HostState::release`], for a resource's drop, which is not a function of
//!   the world and is reachable the same way;
//! - [`HostState::ungated`], for `log`, which `catalog/capabilities.toml` gives
//!   every plugin and which only records that it was reached.
//!
//! **Every function of the world is implemented.** A plugin granted a
//! capability is answered by what the function does, or refused for what it
//! asked — never told the function is not there.
//!
//! **What runs a command or a callback is a frame of its call tree**, which
//! the kernel hands the store before the call and takes back after it: the
//! tree's principal, its unit of work, what the frame runs — a command, which
//! may change state, or the `on-timer` or `on-event` the kernel calls — the
//! plugin running it, the authority it runs under, and whether a person
//! confirmed it. The store's writes, a dispatch and a unit begun read it, and
//! with none — in `activate`, or through a test door — each is refused with
//! `[NC-PLUG-0024]`; the context, an event, a timer and a surface each act for
//! the person the tree runs for, and with none are refused with
//! `[NC-PLUG-0025]`; a unit begun and still open when the frame is taken back
//! dooms the tree's unit. Each of the two, and an event published under a topic
//! not the plugin's (`[NC-PLUG-0026]`), is recorded through
//! [`HostState::refused_here`] and logged as `kernel.host_call_refused` once the
//! call returns, as a capability denied is: once for each function and code.
//!
//! **A host function that waits is held to a deadline.** Every one that awaits
//! anything — the store's four, the blobs' that read the disk, the fetch that
//! waits on the network and the `begin` of a timer's callback, which waits on
//! the journal — runs its body through
//! `supervisor::in_time` under its `interface#function` name, and one that has
//! not answered within `host_call_deadline_ms`, or within what its call tree
//! has left when that is nearer, traps the plugin that asked, which kills it;
//! one in a tree with nothing left is never started. `commands.dispatch` alone
//! is not: the command it reaches runs under its tree's deadline, and the
//! decision point holds each of its waits on the journal to the nearer of the
//! two (`crate::dispatch::Waits`).
//!
//! **What a store keeps of a plugin is bounded by the world, not by the
//! plugin.** It lives in host memory for the plugin's lifetime, outside the
//! linear memory `memory_bytes` bounds, so nothing a plugin repeats may grow
//! it: a function reached is a member of a set, a function refused is one
//! entry with a count beside it, a fetch refused is its reason, kept once, a
//! function refused for where it ran is kept once with its code, and the log is
//! a ring. A [`Denial`] is kept only by a host function, under its
//! own name, so the denials hold at most one entry per function a capability
//! grants (`nc_catalog::CAPABILITIES`), however many times a plugin is
//! refused, and the refused fetches at most one per reason `net.refused` names.
//!
//! What this answers (`NOCTUA.md §2.4`): no host function can run for a
//! plugin before its capability is checked, a refusal never kills the plugin
//! it refuses, and refusing it costs the kernel no memory per call. What it
//! does not answer: which principal a call runs for, and whether it is
//! authorized — that is the decision point's (`crate::dispatch`), which the
//! frame a command runs in carries here.

mod blobs;
mod commands;
mod context;
mod events;
pub(crate) mod log;
mod net;
mod store;
mod timers;
mod ui;

use std::collections::BTreeSet;
use std::sync::Arc;

use nc_catalog::{Capability, ErrorCode};
use nc_ids::OpaqueId;

use self::log::SharedRing;
use crate::bindings::KernelError;
use crate::bindings::noctua::kernel::types;
use crate::blobs::BlobStore;
use crate::capability::{Denial, GrantSet, Linked};
use crate::dispatch::{Frame, Refusal};
use crate::limits::MemoryGuard;
use crate::manifest::PluginManifest;
use crate::supervisor::Running;
use crate::{Environment, RefusalKind, Shared};

/// What one plugin's store holds.
#[derive(Debug)]
pub(crate) struct HostState<E: Environment> {
    grants: GrantSet,
    /// The realm the kernel serves, whose keys the plugin's are.
    realm: OpaqueId,
    plugin: String,
    /// The names its manifest declares under `[net] domains`.
    domains: Vec<String>,
    store: Arc<E::Store>,
    /// Where the bytes of the blobs are kept.
    blobs: Arc<E::Blobs>,
    /// The blob readers and writers the plugin holds open.
    handles: blobs::Handles<<E::Blobs as BlobStore>::Writer>,
    /// What its judged requests are sent with.
    transport: Arc<E::Transport>,
    guard: MemoryGuard,
    reached: BTreeSet<Linked>,
    denials: Vec<(Denial, u64)>,
    /// How many of `denials` the kernel has been handed already.
    logged: usize,
    /// Every reason a fetch was refused for, once, with the code it was
    /// refused with the first time, in the order first refused.
    fetches_refused: Vec<(crate::net::Reason, ErrorCode)>,
    /// How many of `fetches_refused` the kernel has been handed already.
    fetches_logged: usize,
    /// Every function refused for where it ran or whose name it used, rather
    /// than for a capability — `[NC-PLUG-0024]`, `[NC-PLUG-0025]` and
    /// `[NC-PLUG-0026]` — once with each code, in the order first refused.
    ///
    /// Behind a lock because the host functions that refuse so read the store
    /// through a shared borrow.
    calls_refused: std::sync::Mutex<Vec<(&'static str, ErrorCode)>>,
    /// How many of `calls_refused` the kernel has been handed already.
    calls_logged: usize,
    /// The plugin's own log, which the kernel holds for every instance of it.
    ring: SharedRing,
    /// The units it began inside the command running now.
    units: commands::Units,
    /// The frame of the command running now, and nothing between commands.
    frame: Option<Frame<E>>,
    /// When the call tree a restart inside one began: what `activate` is held
    /// to, as a call of that tree, where no frame is; none at a load, a
    /// release and a restart between call trees.
    within: Option<std::time::Instant>,
    /// How long the call running now has run, its own running apart from the
    /// host's.
    running: Running,
}

impl<E: Environment> HostState<E> {
    /// What the plugin `plugin` of `realm` holds before it has reached
    /// anything.
    ///
    /// It is granted `grants` and fetches from `domains`, its data is in
    /// `store`, its realm's keys, and its blobs' bytes in `blobs`, its
    /// requests are sent with
    /// `transport`, its memory grows against `guard` and its log lines go
    /// to `ring`: the caller decides each of them, and the plugin none.
    #[expect(
        clippy::too_many_arguments,
        reason = "each is one thing the plugin is held to or kept in, and the caller decides \
                  every one: bundling them would be a struct built only to be taken apart here"
    )]
    pub(crate) fn new(
        grants: GrantSet,
        (realm, plugin): (OpaqueId, String),
        domains: Vec<String>,
        store: Arc<E::Store>,
        blobs: Arc<E::Blobs>,
        transport: Arc<E::Transport>,
        guard: MemoryGuard,
        ring: SharedRing,
    ) -> Self {
        Self {
            grants,
            realm,
            plugin,
            domains,
            store,
            blobs,
            handles: blobs::Handles::default(),
            transport,
            guard,
            reached: BTreeSet::new(),
            denials: Vec::new(),
            logged: 0,
            fetches_refused: Vec::new(),
            fetches_logged: 0,
            calls_refused: std::sync::Mutex::new(Vec::new()),
            calls_logged: 0,
            ring,
            units: commands::Units::default(),
            frame: None,
            within: None,
            running: Running::default(),
        }
    }

    /// The same state, whose `activate` runs as a call of the call tree that
    /// began at `within`, held to what it has left — none for one in no tree.
    pub(crate) const fn within(mut self, within: Option<std::time::Instant>) -> Self {
        self.within = within;
        self
    }

    /// What the plugin `manifest` describes holds, under the kernel `shared`:
    /// the grants and the domains its manifest names, the kernel's store, blobs
    /// and transport, a memory held to the manifest's bound, and `ring`, the
    /// log the kernel keeps for the plugin.
    ///
    /// **One constructor for its load and every restart**, so the instance a
    /// kill brings back is held to what the first one was: the restart built
    /// its state apart, and handing it every grant or the catalog's memory
    /// left every suite green.
    pub(crate) fn for_plugin(
        manifest: &PluginManifest,
        shared: &Shared<E>,
        ring: SharedRing,
    ) -> Self {
        Self::new(
            manifest.grants().clone(),
            (shared.realm.clone(), manifest.id().to_owned()),
            manifest.domains().to_vec(),
            Arc::clone(&shared.store),
            Arc::clone(&shared.blobs),
            Arc::clone(&shared.transport),
            MemoryGuard::new(manifest.limits()),
            ring,
        )
    }

    /// Takes the frame of the command about to run.
    pub(crate) fn enter(&mut self, frame: Frame<E>) {
        self.frame = Some(frame);
    }

    /// Lets go of the frame once the command has run, and answers every
    /// function refused for the first time since the last call.
    ///
    /// **One line per function, never one per call**: a denial is kept once,
    /// with a count beside it, and only a function refused for the first time
    /// is handed over, so what the log is given is bounded by the world's
    /// functions however often a plugin repeats a refused call.
    ///
    /// **And a unit begun in the command ends with it**: one still open dooms
    /// the tree's unit, as a handle let go without a commit does.
    pub(crate) fn leave(&mut self) -> Vec<Denial> {
        if self.units.ended_with_the_command() {
            self.doom_the_tree(
                "a plugin returned from a command with a unit it began in it still open",
                crate::dispatch::Cause::Dropped,
            );
        }
        self.frame = None;
        let fresh = self
            .denials
            .iter()
            .skip(self.logged)
            .map(|(denial, _)| *denial)
            .collect();
        self.logged = self.denials.len();
        fresh
    }

    /// Every reason a fetch was refused for since the kernel was last handed
    /// them, each once, with its code.
    ///
    /// **One line per reason, never one per request**, as a denial is one per
    /// function: what the log is given is bounded by the reasons `net.refused`
    /// names, however often a plugin repeats a refused fetch.
    pub(crate) fn fetches_refused(&mut self) -> Vec<(crate::net::Reason, ErrorCode)> {
        let fresh = self
            .fetches_refused
            .iter()
            .skip(self.fetches_logged)
            .copied()
            .collect();
        self.fetches_logged = self.fetches_refused.len();
        fresh
    }

    /// The code of each function refused for where it ran or whose name it
    /// used since the kernel was last handed them, once for each function and
    /// code.
    ///
    /// **One line per function, never one per call**, as a denial is: what
    /// the log is given is bounded by the world's functions and the three
    /// codes, however often a plugin repeats a refused call.
    pub(crate) fn calls_refused(&mut self) -> Vec<ErrorCode> {
        let refused = self
            .calls_refused
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let fresh = refused
            .iter()
            .skip(self.calls_logged)
            .map(|(_, code)| *code)
            .collect();
        self.calls_logged = refused.len();
        fresh
    }

    /// Records that `function`, as `interface#function` names it, was refused
    /// as `refusal` says — for where it ran or whose name it used — once for
    /// that function and code, and answers what the plugin is told.
    ///
    /// **A reviewer found a real bug with it**: a write where nothing may
    /// change, an act for a person where no call tree runs and an event under
    /// another plugin's topic were each answered and recorded nowhere, where
    /// a capability denied and a fetch refused are each logged — and every
    /// authorization denial is recorded (`NOCTUA.md §15.5`).
    pub(crate) fn refused_here(&self, function: &'static str, refusal: &Refusal) -> KernelError {
        let seen = (function, refusal.code());
        let mut refused = crate::dispatch::lock(&self.calls_refused);
        if !refused.contains(&seen) {
            refused.push(seen);
        }
        drop(refused);
        self::refused(refusal)
    }

    /// Records that a fetch was refused as `refused` says, once for its
    /// reason, and answers what the plugin is told.
    pub(crate) fn refused_fetch(&mut self, refused: &crate::net::Refused) -> KernelError {
        if let Some(reason) = refused.reason
            && !self.fetches_refused.iter().any(|(seen, _)| *seen == reason)
        {
            self.fetches_refused.push((reason, refused.refusal.code()));
        }
        self::refused(&refused.refusal)
    }

    /// The frame of the command running now: none in `activate`, and none
    /// between commands.
    pub(crate) const fn frame(&self) -> Option<&Frame<E>> {
        self.frame.as_ref()
    }

    /// When the call tree of the command running now began: none while the
    /// plugin activates in no tree — at a load, a release, or a restart
    /// between call trees — and the tree's own while a restart inside one
    /// activates it.
    pub(crate) fn tree_began(&self) -> Option<std::time::Instant> {
        self.frame.as_ref().map(Frame::tree_began).or(self.within)
    }

    /// The guard the plugin's memory grows against, which wasmtime asks before
    /// every growth.
    pub(crate) const fn guard(&mut self) -> &mut MemoryGuard {
        &mut self.guard
    }

    /// How long the call running now has run, which wasmtime's hooks tell
    /// and its epoch deadline asks.
    pub(crate) const fn running(&mut self) -> &mut Running {
        &mut self.running
    }

    /// What the call tree of the call running now has left of its deadline,
    /// which a host call that waits is held to besides its own
    /// (`supervisor::in_time`).
    pub(crate) fn left(&self) -> Option<std::time::Duration> {
        self.running.left()
    }

    /// Records that `function` of `capability`'s interface was reached, and
    /// answers whether it may run.
    ///
    /// The first statement of every host function whose result carries the
    /// world's `error`. A refusal is counted against the function's own entry,
    /// which the first refusal of it opens: the record grows with the
    /// functions refused, never with the calls.
    ///
    /// # Errors
    ///
    /// Returns the [`Denial`] as the world's `denied` value, which reaches the
    /// plugin as a refusal it reads: the instance lives on.
    pub(crate) fn require(
        &mut self,
        capability: Capability,
        function: &'static str,
    ) -> Result<(), KernelError> {
        self.reached.insert(Linked {
            interface: capability.spec().interface,
            function,
            capability: Some(capability),
        });
        self.grants.check(capability, function).map_err(|denial| {
            match self.denials.iter_mut().find(|(seen, _)| *seen == denial) {
                Some((_, times)) => *times = times.saturating_add(1),
                None => self.denials.push((denial, 1)),
            }
            KernelError::Refused(types::Error::Denied(denial.to_string()))
        })
    }

    /// The same check as [`HostState::require`], for a function whose result
    /// carries no `error`.
    ///
    /// # Errors
    ///
    /// Traps with the denial's sentence, which kills the plugin: there is no
    /// value to refuse with. Reachable only for a handle a constructor of the
    /// same capability handed out, so a plugin meets it only by holding a
    /// handle it could not have been given.
    pub(crate) fn require_or_trap(
        &mut self,
        capability: Capability,
        function: &'static str,
    ) -> wasmtime::Result<()> {
        self.require(capability, function)
            .map_err(|refused| match refused {
                KernelError::Refused(error) => wasmtime::Error::msg(format!("{error:?}")),
                KernelError::Trap(fault) => fault,
            })
    }

    /// The first statement of a resource's drop: the handle's capability is
    /// still held.
    ///
    /// A drop is not a function of the world, so it is not recorded as reached.
    ///
    /// # Errors
    ///
    /// Traps when `capability` is not granted, which a handle cannot be held
    /// without.
    pub(crate) fn release(&self, capability: Capability) -> wasmtime::Result<()> {
        self.grants
            .check(capability, "[resource-drop]")
            .map_err(|denial| wasmtime::Error::msg(denial.to_string()))
    }

    /// Records that `function` of `interface`, which every plugin is given, was
    /// reached.
    pub(crate) fn ungated(&mut self, interface: &'static str, function: &'static str) {
        self.reached.insert(Linked {
            interface,
            function,
            capability: None,
        });
    }

    /// The frame of the call running now, and the kernel it runs under, for a
    /// host function that acts for the person the call tree runs for:
    /// `function`, as the world names it.
    ///
    /// # Errors
    ///
    /// `denied` with `[NC-PLUG-0025]` where no call tree runs — `activate`,
    /// or a test door — and `internal` with `[NC-KERN-0008]` when the kernel
    /// the frame ran under is gone, which no frame outlives.
    pub(crate) fn acting(
        &self,
        function: &'static str,
    ) -> Result<(&Frame<E>, Arc<Shared<E>>), KernelError> {
        let frame = self.in_a_tree(function)?;
        let Some(kernel) = frame.kernel() else {
            return Err(refused(&Refusal::new(
                RefusalKind::Internal,
                ErrorCode::KernelUnitAborted,
                format!("`{function}` could not run: the kernel the call ran under is gone"),
            )));
        };
        Ok((frame, kernel))
    }

    /// The frame of the call running now, for a host function that reads who
    /// the call tree runs for: `function`, as the world names it.
    ///
    /// # Errors
    ///
    /// `denied` with `[NC-PLUG-0025]` where no call tree runs — `activate`, or
    /// a test door.
    pub(crate) fn in_a_tree(&self, function: &'static str) -> Result<&Frame<E>, KernelError> {
        self.frame().ok_or_else(|| {
            self.refused_here(function, &Refusal::new(
                RefusalKind::Denied,
                ErrorCode::PluginOutsideACallTree,
                format!(
                    "`{function}` acts for the person a call tree runs for, and none runs here: \
                     `activate` runs in no call tree"
                ),
            ))
        })
    }

    /// The plugin's identity, as its manifest names it.
    pub(crate) fn plugin(&self) -> &str {
        &self.plugin
    }

    /// Every host function the plugin reached, in order.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn reached(&self) -> Vec<Linked> {
        self.reached.iter().copied().collect()
    }

    /// Every function the plugin was refused, once, in the order each was
    /// first refused, with how many times it was.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn denials(&self) -> &[(Denial, u64)] {
        &self.denials
    }

    /// The plugin's own log.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn ring(&self) -> std::sync::MutexGuard<'_, log::Ring> {
        crate::dispatch::lock(&self.ring)
    }
}

/// The state a test runs a plugin in, without a kernel around it.
#[cfg(any(test, feature = "test-support"))]
impl HostState<crate::InMemory> {
    /// What a plugin granted `grants` holds when its data is an empty store
    /// in memory and its bounds are the catalog's — the state a test door
    /// runs a plugin in, and the one this module's tests call through.
    ///
    /// It fetches from `example.com`, the one name the reference plugin's
    /// probe asks for, through a transport that answers every request with an
    /// empty `200`.
    pub(crate) fn in_memory(grants: GrantSet) -> Self {
        Self::new(
            grants,
            (Self::home(), "tests:door".to_owned()),
            vec!["example.com".to_owned()],
            Arc::new(crate::backend::MemoryBackend::default()),
            Arc::new(crate::blobs::MemoryBlobs::default()),
            Arc::new(crate::net::Scripted::default()),
            MemoryGuard::new(&crate::limits::Limits::catalog()),
            SharedRing::default(),
        )
    }

    /// The realm a test runs a plugin in: `home`, of the world the policy
    /// cases are about.
    pub(crate) fn home() -> OpaqueId {
        let pdp = nc_authz::Pdp::embedded("family").expect("the family pack is carried");
        nc_authz::MemoryDirectory::from_world(include_str!("../../../policy/world.json"), &pdp)
            .expect("the world loads")
            .realm_id("home")
            .expect("home is a realm of the world")
    }
}

/// A refusal of the kernel's, as the world's `error` a plugin reads: the
/// same case, and the code and sentence as its text.
///
/// **Or as a trap, for a refusal that [`Refusal::traps`]** — a dispatch past
/// a call tree's count, asked for by a plugin already told the count is spent
/// — carrying the same text: the plugin is killed rather than told again, so
/// one that would ask again whatever it is told cannot.
pub(crate) fn refused(refusal: &Refusal) -> KernelError {
    let said = refusal.to_string();
    if refusal.traps() {
        return KernelError::Trap(wasmtime::Error::msg(said));
    }
    KernelError::Refused(match refusal.kind() {
        RefusalKind::Denied => types::Error::Denied(said),
        RefusalKind::NotFound => types::Error::NotFound(said),
        RefusalKind::Invalid => types::Error::Invalid(said),
        RefusalKind::Conflict => types::Error::Conflict(said),
        RefusalKind::Unavailable => types::Error::Unavailable(said),
        RefusalKind::Internal => types::Error::Internal(said),
    })
}

impl<E: Environment> types::Host for HostState<E> {
    fn convert_error(&mut self, error: KernelError) -> wasmtime::Result<types::Error> {
        match error {
            KernelError::Refused(refusal) => Ok(refusal),
            KernelError::Trap(fault) => Err(fault),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::HostState;
    use crate::InMemory;
    use crate::bindings::KernelError;
    use crate::bindings::noctua::kernel::types;
    use crate::capability::{GrantSet, Linked};
    use nc_catalog::{Capability, ErrorCode};

    /// A kernel over the store, the blobs and the journal in memory, deciding
    /// with the family pack over the world the policy cases are about, and
    /// `ana`, a user of that world.
    pub(crate) async fn a_kernel_for_ana() -> (crate::Kernel<InMemory>, nc_authz::Principal) {
        let pack = nc_authz::Pdp::embedded("family").expect("the family pack is carried");
        let world = nc_authz::MemoryDirectory::from_world(
            include_str!("../../../policy/world.json"),
            &pack,
        )
        .expect("the world loads");
        let realm = world.realm_id("home").expect("home is a realm");
        let ana = world
            .principal(nc_authz::PrincipalKind::User, "ana")
            .expect("ana is a user of the world");
        let kernel = crate::Kernel::open(crate::KernelConfig {
            store: crate::backend::MemoryBackend::default(),
            blobs: crate::blobs::MemoryBlobs::default(),
            pdp: Ok(pack),
            journal: nc_journal::Journal::new(
                nc_journal::InMemory::default(),
                realm,
                ana.id().clone(),
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
        (kernel, ana)
    }

    /// Whether `answer` is what a host function that acts for a person
    /// answers outside any call tree: `denied`, with `[NC-PLUG-0001]` when its
    /// capability is not granted and `[NC-PLUG-0025]` when it is.
    ///
    /// The one reading every such interface's tests share, so each module's
    /// case is the list of its functions and nothing else.
    pub(crate) fn answered_outside_a_tree(answer: &Result<(), KernelError>, granted: bool) -> bool {
        if !granted {
            return denied_its_capability(answer);
        }
        matches!(
            answer,
            Err(KernelError::Refused(types::Error::Denied(why)))
                if why.starts_with(&format!("[{}] ", ErrorCode::PluginOutsideACallTree))
        )
    }

    /// Whether `answer` is the denial a function answers a plugin not granted
    /// its capability: `denied`, with `[NC-PLUG-0001]`.
    pub(crate) fn denied_its_capability(answer: &Result<(), KernelError>) -> bool {
        matches!(
            answer,
            Err(KernelError::Refused(types::Error::Denied(why)))
                if why.starts_with(&format!("[{}] ", ErrorCode::PluginCapabilityDenied))
        )
    }

    #[test]
    fn a_function_not_granted_is_refused_as_a_value_and_recorded() {
        let mut state = HostState::in_memory(GrantSet::none());
        let refused = state.require(Capability::Timers, "after");
        assert!(
            matches!(
                refused,
                Err(KernelError::Refused(types::Error::Denied(ref why)))
                    if why.starts_with(&format!("[{}] ", ErrorCode::PluginCapabilityDenied))
                        && why.contains("`timers#after`")
            ),
            "{refused:?}"
        );
        assert_eq!(
            state.reached(),
            [Linked {
                interface: "timers",
                function: "after",
                capability: Some(Capability::Timers),
            }]
        );
        assert_eq!(state.denials().len(), 1);
        assert_eq!(state.denials()[0].0.capability(), Capability::Timers);
        assert_eq!(state.denials()[0].1, 1, "refused once");
    }

    /// A million refusals keep one entry per function refused, not one per
    /// call.
    ///
    /// **A review found this record a list of every refused call:**
    /// `timers::cancel` called five million times inside one dispatch's fuel
    /// left five million denials in the plugin's store, in host memory that
    /// `memory_bytes` does not count, kept for the plugin's lifetime. Every
    /// function a capability grants is refused here in turn, so the record
    /// reaches its whole bound and no further, and the counts add up to every
    /// call.
    #[test]
    fn a_million_refusals_keep_one_entry_per_function_refused() {
        let calls = 1_000_000_usize;
        let functions: Vec<(Capability, &'static str)> = nc_catalog::CAPABILITIES
            .iter()
            .flat_map(|spec| {
                spec.functions
                    .iter()
                    .map(move |function| (spec.capability, *function))
            })
            .collect();
        assert!(
            functions.len() > nc_catalog::CAPABILITIES.len(),
            "{} function(s) over {} capabilities: a catalog this read part of",
            functions.len(),
            nc_catalog::CAPABILITIES.len()
        );
        let mut state = HostState::in_memory(GrantSet::none());
        // The bound is asked after every call, so a record that grows with the
        // calls fails at the first entry past it rather than after a million.
        for (capability, function) in functions.iter().cycle().take(calls) {
            assert!(state.require(*capability, function).is_err());
            assert!(
                state.denials().len() <= functions.len(),
                "{} entries kept for {} functions refused",
                state.denials().len(),
                functions.len()
            );
        }
        assert_eq!(
            state.denials().len(),
            functions.len(),
            "{calls} refusals kept {} entries, and the capabilities grant {} functions",
            state.denials().len(),
            functions.len()
        );
        assert_eq!(
            state.denials().iter().map(|(_, times)| *times).sum::<u64>(),
            u64::try_from(calls).expect("the count fits"),
            "every refusal is counted"
        );
        assert_eq!(
            state
                .denials()
                .iter()
                .map(|(denial, _)| (denial.capability(), denial.function()))
                .collect::<Vec<_>>(),
            functions,
            "each function is kept once, in the order it was first refused"
        );
    }

    #[test]
    fn a_function_granted_runs_and_is_recorded_without_a_denial() {
        let mut state = HostState::in_memory(GrantSet::of([Capability::Timers]));
        assert!(state.require(Capability::Timers, "after").is_ok());
        assert_eq!(state.reached().len(), 1);
        assert_eq!(state.denials(), []);
    }

    #[test]
    fn a_function_with_no_error_to_refuse_with_traps_rather_than_answering() {
        let mut state = HostState::in_memory(GrantSet::none());
        let trapped = state
            .require_or_trap(Capability::BlobsRead, "[method]reader.size")
            .expect_err("nothing is granted");
        assert!(
            trapped.to_string().contains("`blobs#[method]reader.size`"),
            "{trapped}"
        );
        assert!(
            HostState::in_memory(GrantSet::of([Capability::BlobsRead]))
                .require_or_trap(Capability::BlobsRead, "[method]reader.size")
                .is_ok()
        );
    }

    #[test]
    fn a_handle_is_released_only_under_the_capability_that_issued_it() {
        assert!(
            HostState::in_memory(GrantSet::none())
                .release(Capability::CommandsBegin)
                .is_err()
        );
        let granted = HostState::in_memory(GrantSet::of([Capability::CommandsBegin]));
        assert!(granted.release(Capability::CommandsBegin).is_ok());
        let got = granted.reached();
        assert!(
            got.is_empty(),
            "a drop is not a function of the world: {got:?}"
        );
    }

    #[test]
    fn an_interface_every_plugin_is_given_is_recorded_with_no_capability() {
        let mut state = HostState::in_memory(GrantSet::none());
        state.ungated("log", "write");
        assert_eq!(
            state.reached(),
            [Linked {
                interface: "log",
                function: "write",
                capability: None,
            }]
        );
        assert_eq!(state.denials(), []);
    }

    /// Every host function refused for where it ran is recorded with its
    /// code, once however often it is refused, and handed over once.
    ///
    /// Every gated function called where no call tree runs, with every
    /// capability granted: each of [`IN_A_COMMAND`] is recorded with
    /// `[NC-PLUG-0024]` and each of [`IN_A_CALL_TREE`] with `[NC-PLUG-0025]`,
    /// and nothing else is — so a refusal that forgot to record itself is
    /// named, and one recorded under another function's name too. Calling
    /// them all again records and hands over nothing: what the log is given
    /// is bounded by the functions, not the calls. A topic outside the
    /// plugin's namespace, `[NC-PLUG-0026]`, needs a call tree, and
    /// `the_rest_of_the_world.rs` refuses one there.
    #[tokio::test]
    async fn every_call_refused_for_where_it_ran_is_recorded_once_with_its_code() {
        let mut state = HostState::in_memory(GrantSet::every());
        every_gated_function(&mut state).await;
        let recorded = |state: &HostState<InMemory>| -> BTreeSet<(String, ErrorCode)> {
            crate::dispatch::lock(&state.calls_refused)
                .iter()
                .map(|(function, code)| ((*function).to_owned(), *code))
                .collect()
        };
        let expected: BTreeSet<(String, ErrorCode)> = IN_A_COMMAND
            .iter()
            .map(|named| (named, ErrorCode::PluginReadOnlyFrame))
            .chain(
                IN_A_CALL_TREE
                    .iter()
                    .map(|named| (named, ErrorCode::PluginOutsideACallTree)),
            )
            .map(|((interface, function), code)| (format!("{interface}#{function}"), code))
            .collect();
        assert_eq!(recorded(&state), expected);
        assert_eq!(
            state.calls_refused().len(),
            expected.len(),
            "each function was handed over once"
        );
        every_gated_function(&mut state).await;
        assert_eq!(recorded(&state).len(), expected.len());
        assert_eq!(
            state.calls_refused(),
            [],
            "a function handed over once was handed over again"
        );
    }

    /// Each host function asks for the capability the catalog gives it, by
    /// the name the catalog gives it, and for no other.
    ///
    /// **Granted one capability at a time**, because a store granted two of an
    /// interface's capabilities cannot tell which of them a function asked for.
    /// A review planted `unit.commit` asking for `commands.dispatch` and
    /// `reader.seek` recording itself as `reader.sek`, and every test of this
    /// crate passed: each interface's own test granted both of its capabilities
    /// together or counted what was reached, and the coverage suite leaves out
    /// every method, which no plugin can reach before a constructor is built.
    /// Here the first plant answers the wrong refusal under both grants, and
    /// either plant leaves what was reached apart from the catalog.
    ///
    /// A function granted answers as it is built to: [`BUILT`] names each
    /// that answers, [`IN_A_COMMAND`] the ones that refuse to run outside a
    /// command, as these tests call them, [`IN_A_CALL_TREE`] the ones that act
    /// for a person and refuse to run outside a call tree, [`REFUSED_AS_BUILT`]
    /// the ones whose answer here is a refusal of their own, and
    /// [`ON_A_HANDLE`] the methods called on a handle no constructor handed
    /// out — and every granted function is in exactly one of them.
    #[tokio::test]
    async fn every_host_function_asks_for_the_capability_the_catalog_names_and_no_other() {
        let catalog: BTreeSet<Linked> = nc_catalog::CAPABILITIES
            .iter()
            .flat_map(|spec| {
                spec.functions.iter().map(move |function| Linked {
                    interface: spec.interface,
                    function,
                    capability: Some(spec.capability),
                })
            })
            .collect();
        for spec in nc_catalog::CAPABILITIES {
            let mut state = HostState::in_memory(GrantSet::of([spec.capability]));
            let answered = every_gated_function(&mut state).await;
            assert_eq!(
                answered
                    .iter()
                    .map(|(interface, function, _)| (interface.as_str(), function.as_str()))
                    .collect::<BTreeSet<_>>(),
                catalog
                    .iter()
                    .map(|linked| (linked.interface, linked.function))
                    .collect::<BTreeSet<_>>(),
                "the calls below are not every function the catalog grants"
            );
            for (interface, function, answer) in &answered {
                let granted =
                    interface == spec.interface && spec.functions.contains(&function.as_str());
                let named = (interface.as_str(), function.as_str());
                let refused_as_built = REFUSED_AS_BUILT
                    .iter()
                    .find(|(function, _)| *function == named)
                    .map(|(_, code)| *code);
                let expected = if !granted {
                    Answer::Denied
                } else if IN_A_COMMAND.contains(&named) {
                    Answer::OutsideACommand
                } else if let Some(code) = refused_as_built {
                    Answer::Unavailable(Some(code))
                } else if ON_A_HANDLE.contains(&named) {
                    Answer::NotIssued
                } else if IN_A_CALL_TREE.contains(&named) {
                    Answer::OutsideATree
                } else if BUILT.contains(&named) {
                    Answer::Answered
                } else {
                    panic!(
                        "`{interface}#{function}` is granted and in no list of what a granted \
                         function answers"
                    )
                };
                assert_eq!(
                    *answer, expected,
                    "`{interface}#{function}` with only `{}` granted",
                    spec.name
                );
            }
            let reached: BTreeSet<Linked> = state.reached().into_iter().collect();
            assert!(
                reached == catalog,
                "with `{}` granted, what was reached is not every function under the \
                 capability the catalog gives it — reached and not the catalog's: {:?}; \
                 the catalog's and not reached: {:?}",
                spec.name,
                reached.difference(&catalog).collect::<Vec<_>>(),
                catalog.difference(&reached).collect::<Vec<_>>()
            );
            for (resource, released) in every_drop(&mut state).await {
                let issued = spec
                    .functions
                    .iter()
                    .any(|function| function.starts_with(&format!("[method]{resource}.")));
                assert_eq!(
                    released, issued,
                    "a `{resource}` dropped with only `{}` granted",
                    spec.name
                );
            }
        }
    }

    /// The functions of the world this build implements, by interface.
    ///
    /// Each answers a plugin granted its capability with what it was asked
    /// for, which the empty store in memory these tests run on makes an
    /// answer without an error.
    const BUILT: [(&str, &str); 4] = [
        ("store", "get"),
        ("store", "list"),
        ("blobs", "locate"),
        ("net", "fetch"),
    ];

    /// The functions of the world this build implements that run only in the
    /// frame of a command that changes state.
    ///
    /// Called with no command running, as every test here calls them, each
    /// refuses with `[NC-PLUG-0024]` as a value: a write needs a unit to be
    /// staged in, and a dispatch needs a call tree to run in.
    const IN_A_COMMAND: [(&str, &str); 7] = [
        ("store", "put"),
        ("store", "delete"),
        ("commands", "dispatch"),
        ("commands", "begin"),
        ("blobs", "create"),
        ("blobs", "[method]writer.write"),
        ("blobs", "[method]writer.finish"),
    ];

    /// The functions that act for the person a call tree runs for.
    ///
    /// Called with no call tree running, as every test here calls them, each
    /// refuses with `[NC-PLUG-0025]` as a value: there is nobody to act for.
    const IN_A_CALL_TREE: [(&str, &str); 10] = [
        ("context", "principal"),
        ("context", "actor-chain"),
        ("context", "confirmed-by-human"),
        ("events", "publish"),
        ("timers", "after"),
        ("timers", "every"),
        ("timers", "cancel"),
        ("ui", "open-surface"),
        ("ui", "close-surface"),
        ("ui", "render"),
    ];

    /// The functions this build implements that refuse these tests as unavailable.
    ///
    /// Each answers with a code of its own: `open` of a blob this machine does
    /// not hold, and `announce`, which no peer hears.
    const REFUSED_AS_BUILT: [((&str, &str), ErrorCode); 2] = [
        (("blobs", "open"), ErrorCode::BlobNotLocal),
        (("blobs", "announce"), ErrorCode::BlobNoPeers),
    ];

    /// The methods this build implements that trap on a handle never issued.
    ///
    /// Called on a handle this kernel did not issue, as these tests call them,
    /// each traps: no constructor here hands one out.
    const ON_A_HANDLE: [(&str, &str); 6] = [
        ("blobs", "[method]reader.size"),
        ("blobs", "[method]reader.read"),
        ("blobs", "[method]reader.seek"),
        ("commands", "[method]unit.compensate-with"),
        ("commands", "[method]unit.commit"),
        ("commands", "[method]unit.abort"),
    ];

    /// What one host function answered.
    #[derive(Debug, PartialEq, Eq)]
    enum Answer {
        /// `[NC-PLUG-0001]`: the capability it asked for is not granted.
        Denied,
        /// `[NC-PLUG-0025]`: it is granted, and acts for a person no call tree
        /// runs for here.
        OutsideATree,
        /// It is granted and built, and answered without an error.
        Answered,
        /// `[NC-PLUG-0024]`: it is granted and built, and runs only inside a
        /// command that changes state.
        OutsideACommand,
        /// `unavailable`, with the code its sentence opens on.
        Unavailable(Option<ErrorCode>),
        /// A trap: the method was called on a handle this kernel did not issue.
        NotIssued,
        /// None of those, as it was answered.
        Other(String),
    }

    /// The answer of a function whose result carries the world's `error`.
    fn as_a_value(answer: Result<(), KernelError>) -> Answer {
        match answer {
            Err(KernelError::Refused(types::Error::Denied(why)))
                if why.starts_with(&format!("[{}] ", ErrorCode::PluginCapabilityDenied)) =>
            {
                Answer::Denied
            }
            Err(KernelError::Refused(types::Error::Denied(why)))
                if why.starts_with(&format!("[{}] ", ErrorCode::PluginOutsideACallTree)) =>
            {
                Answer::OutsideATree
            }
            Err(KernelError::Refused(types::Error::Denied(why)))
                if why.starts_with(&format!("[{}] ", ErrorCode::PluginReadOnlyFrame)) =>
            {
                Answer::OutsideACommand
            }
            Err(KernelError::Refused(types::Error::Unavailable(why))) => {
                Answer::Unavailable(code_of(&why))
            }
            Err(KernelError::Trap(fault)) if fault.to_string().contains("did not issue") => {
                Answer::NotIssued
            }
            Ok(()) => Answer::Answered,
            other => Answer::Other(format!("{other:?}")),
        }
    }

    /// The code a refusal's sentence opens on, when it opens on one.
    fn code_of(why: &str) -> Option<ErrorCode> {
        why.strip_prefix('[')
            .and_then(|rest| rest.split_once(']'))
            .and_then(|(code, _)| ErrorCode::from_code(code))
    }

    /// The answer of a function whose result carries none, which refuses by
    /// trapping with the same sentences.
    fn as_a_trap(answer: wasmtime::Result<()>) -> Answer {
        match answer.map_err(|trap| trap.to_string()) {
            Err(why) if why.contains(&format!("[{}] ", ErrorCode::PluginCapabilityDenied)) => {
                Answer::Denied
            }
            Err(why) if why.contains("did not issue") => Answer::NotIssued,
            other => Answer::Other(format!("{other:?}")),
        }
    }

    /// Every function of every gated interface, called once, with what each
    /// answered under the name the catalog gives it.
    ///
    /// Written out because each call is a method of a generated trait, which
    /// nothing enumerates; the test above holds these names to the catalog
    /// both ways, so a function left out or named wrongly fails it.
    async fn every_gated_function(
        state: &mut HostState<InMemory>,
    ) -> Vec<(String, String, Answer)> {
        let mut answered = every_function_with_a_handle(state).await;
        answered.extend(every_function_without_one(state).await);
        answered
    }

    /// The calls of [`every_gated_function`] into `blobs` and `commands`, held
    /// apart from the others at clippy's hundred-line bound: the two
    /// interfaces whose methods are a handle's.
    async fn every_function_with_a_handle(
        state: &mut HostState<InMemory>,
    ) -> Vec<(String, String, Answer)> {
        use crate::bindings::noctua::kernel::blobs::{Host as _, HostReader as _, HostWriter as _};
        use crate::bindings::noctua::kernel::commands::{Host as _, HostUnit as _};
        use wasmtime::component::Resource;

        let hash = "0".repeat(64);
        let text = |text: &str| text.to_owned();
        owned(vec![
            (
                "blobs",
                "locate",
                as_a_value(state.locate(hash.clone()).await.map(drop)),
            ),
            (
                "blobs",
                "open",
                as_a_value(state.open(hash.clone()).await.map(drop)),
            ),
            (
                "blobs",
                "[method]reader.size",
                as_a_trap(state.size(Resource::new_own(0)).await.map(drop)),
            ),
            (
                "blobs",
                "[method]reader.read",
                as_a_value(state.read(Resource::new_own(0), 1).await.map(drop)),
            ),
            (
                "blobs",
                "[method]reader.seek",
                as_a_value(state.seek(Resource::new_own(0), 0).await),
            ),
            (
                "blobs",
                "create",
                as_a_value(state.create().await.map(drop)),
            ),
            ("blobs", "announce", as_a_value(state.announce(hash).await)),
            (
                "blobs",
                "[method]writer.write",
                as_a_value(state.write(Resource::new_own(0), vec![0]).await),
            ),
            (
                "blobs",
                "[method]writer.finish",
                as_a_value(state.finish(Resource::new_own(0)).await.map(drop)),
            ),
            (
                "commands",
                "begin",
                as_a_value(state.begin(text("a step")).await.map(drop)),
            ),
            (
                "commands",
                "dispatch",
                as_a_value(state.dispatch(text("a.b"), text("{}")).await.map(drop)),
            ),
            (
                "commands",
                "[method]unit.compensate-with",
                as_a_value(
                    state
                        .compensate_with(Resource::new_own(0), text("a.b"), text("{}"))
                        .await,
                ),
            ),
            (
                "commands",
                "[method]unit.commit",
                as_a_value(state.commit(Resource::new_own(0)).await),
            ),
            (
                "commands",
                "[method]unit.abort",
                as_a_trap(state.abort(Resource::new_own(0)).await),
            ),
        ])
    }

    /// The calls of [`every_gated_function`] into every other gated
    /// interface, none of which hands out a handle.
    async fn every_function_without_one(
        state: &mut HostState<InMemory>,
    ) -> Vec<(String, String, Answer)> {
        use crate::bindings::noctua::kernel::context::Host as _;
        use crate::bindings::noctua::kernel::events::Host as _;
        use crate::bindings::noctua::kernel::net::{Host as _, Request};
        use crate::bindings::noctua::kernel::store::Host as _;
        use crate::bindings::noctua::kernel::timers::Host as _;
        use crate::bindings::noctua::kernel::ui::{Host as _, SurfaceKind};

        let request = Request {
            method: "GET".to_owned(),
            url: "https://example.com/".to_owned(),
            headers: Vec::new(),
            body: None,
        };
        let text = |text: &str| text.to_owned();
        owned(vec![
            (
                "context",
                "principal",
                as_a_value(state.principal().map(drop)),
            ),
            (
                "context",
                "actor-chain",
                as_a_value(state.actor_chain().map(drop)),
            ),
            (
                "context",
                "confirmed-by-human",
                as_a_value(state.confirmed_by_human().map(drop)),
            ),
            (
                "events",
                "publish",
                as_a_value(state.publish(text("a.b"), text("{}")).await),
            ),
            (
                "net",
                "fetch",
                as_a_value(state.fetch(request).await.map(drop)),
            ),
            (
                "store",
                "get",
                as_a_value(state.get(text("k")).await.map(drop)),
            ),
            (
                "store",
                "put",
                as_a_value(state.put(text("k"), text("{}"), None).await.map(drop)),
            ),
            (
                "store",
                "delete",
                as_a_value(state.delete(text("k"), None).await),
            ),
            (
                "store",
                "list",
                as_a_value(state.list(String::new(), 1, None).await.map(drop)),
            ),
            (
                "timers",
                "after",
                as_a_value(state.after(1_000, text("t")).await.map(drop)),
            ),
            (
                "timers",
                "every",
                as_a_value(state.every(1_000, text("t")).await.map(drop)),
            ),
            ("timers", "cancel", as_a_value(state.cancel(0).await)),
            (
                "ui",
                "open-surface",
                as_a_value(
                    state
                        .open_surface(SurfaceKind::Panel, text("Notes"))
                        .await
                        .map(drop),
                ),
            ),
            (
                "ui",
                "close-surface",
                as_a_value(state.close_surface(text("s")).await),
            ),
            (
                "ui",
                "render",
                as_a_value(state.render(text("s"), text("{}")).await),
            ),
        ])
    }

    /// The calls as the test reads them, each name its own `String`.
    fn owned(calls: Vec<(&str, &str, Answer)>) -> Vec<(String, String, Answer)> {
        calls
            .into_iter()
            .map(|(interface, function, answer)| {
                (interface.to_owned(), function.to_owned(), answer)
            })
            .collect()
    }

    /// Whether each resource's drop released it, by the resource's name in the
    /// world.
    async fn every_drop(state: &mut HostState<InMemory>) -> [(String, bool); 3] {
        use crate::bindings::noctua::kernel::{blobs, commands};
        use wasmtime::component::Resource;

        [
            (
                "reader".to_owned(),
                blobs::HostReader::drop(&mut *state, Resource::new_own(0))
                    .await
                    .is_ok(),
            ),
            (
                "writer".to_owned(),
                blobs::HostWriter::drop(&mut *state, Resource::new_own(0))
                    .await
                    .is_ok(),
            ),
            (
                "unit".to_owned(),
                commands::HostUnit::drop(&mut *state, Resource::new_own(0))
                    .await
                    .is_ok(),
            ),
        ]
    }
}
