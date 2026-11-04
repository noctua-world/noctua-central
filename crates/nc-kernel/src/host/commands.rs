//! `commands`: dispatching a command, and grouping a step's changes in a unit.
//!
//! Two capabilities divide the interface (`catalog/capabilities.toml`):
//! `commands.dispatch` grants `dispatch`, and `commands.begin` grants `begin`
//! and every method of the `unit` it hands out. Every function asks its
//! capability first and answers `denied` to a plugin not granted it — and a
//! compensation naming a command not the plugin's own asks for
//! `commands.dispatch` as well.
//!
//! **`dispatch` is the decision point**, reached from inside the command
//! running now: the same steps a top-level dispatch goes through, as the call
//! tree's principal and inside the tree's unit, never confirmed and never
//! approved (`crate::dispatch`), and the one host function not held to
//! `host_call_deadline_ms` as a whole: the command it reaches runs under its
//! tree's deadline, and each wait of the decision point on the journal is held
//! to the nearer of the two (`crate::dispatch::Waits`), past which the plugin
//! is told the journal did not answer. From
//! `activate`, where no command runs and so
//! no one is there to run it as, it is refused with `[NC-PLUG-0024]`. What it
//! refuses reaches the plugin as the world's `error`, in the refusal's case and
//! carrying its code, and the plugin lives on — except a dispatch past its
//! call tree's count (`[NC-KERN-0020]`) once the plugin asking was told the
//! count is spent, which traps it: one that would ask again whatever it is
//! told can only be stopped, and one told once is told, not killed.
//!
//! **A unit `begin` hands out is flattened into the tree's** (`NOCTUA.md
//! §11.5`). A command that changes state already runs in the unit its call
//! tree's top-level dispatch opened, which every command dispatched in the
//! tree joins, in any plugin; so `begin` there opens nothing, and the first
//! `label` a plugin gives one is what a person is shown for the step — made
//! safe to show and cut to a bound — where it would otherwise be the
//! top-level command's name. `commit` is
//! then the plugin saying its part is whole, and lets the unit be; `abort`, a
//! handle let go without `commit`, and one still open when the command that
//! began it returns each doom the unit of the whole tree, which is then not
//! kept (`[NC-KERN-0008]`, logged `explicit` or `dropped`). A handle outlives
//! nothing: what it was begun in ends with its command, and one used after is
//! refused as ended (`[NC-PLUG-0028]`), never as a unit aborted. **In a
//! timer's callback the first `begin` opens a unit of
//! the callback's own**, in the journal — labelled by the plugin's identity,
//! never by the text it gave, which the journal would keep for ever — and the
//! rest flatten into it: every command the callback dispatches after it joins
//! it, and it is kept when the callback returns, or let go as a command's is.
//! `begin` is refused anywhere else — `activate`, an event's callback, a
//! command that only reads and the callback of a timer set where only reading
//! was allowed run in no unit of work (`[NC-PLUG-0024]`) — and at
//! `unit_handles` units held at once (`[NC-PLUG-0017]`).
//!
//! **`compensate-with` registers what an undo dispatches**, in the tree's
//! unit, once the command it names is shown to be one an undo can run
//! (`crate::history::compensation`): registered, changing state, held by the
//! core's rules to no person's confirmation and no quorum's approval — so not
//! one that cannot be undone or that deletes — not one of the kernel's own,
//! its arguments held to its schema; a pack's rule that reads the resource is
//! decided when the undo dispatches it. **And the plugin's own,
//! unless it is granted `commands.dispatch` too**: the undo dispatches it as
//! the person, so naming another's command is a dispatch put off until then,
//! refused with `[NC-PLUG-0001]` before anything else of that command is
//! judged. Nothing runs it now: an abort does not, and an undo of the step
//! does, nested, through the decision point — and only while the plugin that
//! held the command when it was registered holds it still, whoever takes an
//! unloaded plugin's namespace meanwhile (`crate::history`).
//!
//! **`unit.abort` has no `error` to refuse with**, so it refuses by trapping,
//! which only a handle the kernel did not hand out meets — the capability it
//! answers to is the one `begin` asked for. A handle this kernel never issued
//! traps every method; a drop of one is nothing to let go.
//!
//! What this answers (`NOCTUA.md §2.4`): half-applied state — a plugin that
//! stops part-way, or drops what it began, leaves its tree's unit unkept — a
//! plugin that holds the kernel's memory by beginning without end, and one not
//! granted `commands.dispatch` having another plugin's command dispatched as
//! the person by naming it as a compensation. What it does not answer: a
//! plugin that commits a group whose work it got wrong, which is its own code;
//! a compensation that does not set right what its step did; and which
//! commands are registered, which a plugin not granted `commands.dispatch`
//! learns from being refused one rather than told it is unknown.

use std::collections::BTreeMap;

use nc_catalog::{Capability, ErrorCode, KERNEL};
use wasmtime::component::Resource;

use super::{HostState, refused};
use crate::bindings::KernelError;
use crate::bindings::noctua::kernel::commands as world;
use crate::dispatch::{Call, Cause, Refusal};
use crate::{Environment, RefusalKind};

/// Where a unit `begin` hands out belongs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Begun {
    /// Inside the unit the call tree holds already.
    Flattened,
    /// In a unit of its own, which the timer's callback that began it holds.
    Owned,
}

/// How a unit handle a plugin holds stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Held {
    /// Begun, and neither committed nor let go.
    Open,
    /// Committed: its part of the tree's unit is whole.
    Committed,
}

/// Every unit handle one plugin holds inside the command running now, and the
/// next handle to hand out.
///
/// **Bounded by the catalog and cleared with each command**: a handle is a
/// record in host memory, outside the linear memory the plugin's own bound
/// counts, so at most `unit_handles` are held at once, and none outlives the
/// command that began it.
#[derive(Debug, Default)]
pub(crate) struct Units {
    held: BTreeMap<u32, Held>,
    next: u32,
}

/// What a method found a handle to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Found {
    /// Held, as it stands.
    Held(Held),
    /// Handed out by this kernel, and ended: let go, aborted, or begun in a
    /// command that has returned.
    Ended,
    /// Never handed out by this kernel.
    NeverIssued,
}

impl Units {
    /// A handle for one more unit.
    ///
    /// # Errors
    ///
    /// A refusal with `[NC-PLUG-0017]` when the plugin holds as many as it
    /// may.
    fn begun(&mut self) -> Result<u32, Refusal> {
        if u64::try_from(self.held.len()).unwrap_or(u64::MAX) >= u64::from(KERNEL.unit_handles) {
            return Err(Refusal::new(
                RefusalKind::Unavailable,
                ErrorCode::PluginBoundReached,
                format!(
                    "a plugin holds at most {} units begun inside one command, and this one \
                     would be one more",
                    KERNEL.unit_handles
                ),
            ));
        }
        let handle = self.next;
        self.next = self.next.wrapping_add(1);
        self.held.insert(handle, Held::Open);
        Ok(handle)
    }

    /// What `handle` is.
    fn found(&self, handle: u32) -> Found {
        match self.held.get(&handle) {
            Some(held) => Found::Held(*held),
            None if handle < self.next => Found::Ended,
            None => Found::NeverIssued,
        }
    }

    /// Lets `handle` go, answering whether it was still open.
    fn ended(&mut self, handle: u32) -> bool {
        self.held.remove(&handle) == Some(Held::Open)
    }

    /// Ends every handle at the end of a command, answering whether any was
    /// still open.
    pub(crate) fn ended_with_the_command(&mut self) -> bool {
        let open = self.held.values().any(|held| *held == Held::Open);
        self.held.clear();
        open
    }
}

/// The refusal of a method on a handle this kernel never issued: a trap,
/// which only a fault of the host could bring about.
fn not_issued(function: &str) -> wasmtime::Error {
    wasmtime::Error::msg(format!(
        "`commands#{function}` was called on a unit this kernel did not issue"
    ))
}

/// The refusal of a method on a handle that has ended.
///
/// **Under a code of its own, and not `[NC-KERN-0008]`**, which says a unit
/// was aborted: the handle of a unit that was kept ends with the command that
/// began it too, and nothing was aborted then.
fn ended(function: &str) -> KernelError {
    refused(&Refusal::new(
        RefusalKind::Invalid,
        ErrorCode::PluginUnitEnded,
        format!(
            "`commands#{function}` was called on a unit that has ended: let go, aborted, or begun \
             in a command that has returned"
        ),
    ))
}

impl<E: Environment> HostState<E> {
    /// Where a unit's `begin` belongs.
    ///
    /// Flattened into the unit of a command that changes state, or of a timer's
    /// callback that began one already, and owned by a timer's callback that
    /// has none — a timer set where a change was allowed, for one set where
    /// only reading was runs in no unit of work.
    fn begins_here(&self) -> Result<Begun, Refusal> {
        match self.frame() {
            Some(frame) if frame.mutating() && frame.tree().unit().is_some() => {
                Ok(Begun::Flattened)
            }
            Some(frame) if matches!(frame.call(), Call::Timer { may_change: true }) => {
                if frame.tree().unit().is_some() {
                    Ok(Begun::Flattened)
                } else {
                    Ok(Begun::Owned)
                }
            }
            _ => Err(Refusal::new(
                RefusalKind::Denied,
                ErrorCode::PluginReadOnlyFrame,
                "a unit is begun only inside a command that changes state or a timer's \
                 callback: `activate`, an event's callback, a command that only reads and the \
                 callback of a timer set where only reading was allowed run in no unit of work"
                    .to_owned(),
            )),
        }
    }

    /// Opens, in the journal, the unit of the timer's callback running now,
    /// labelled by the plugin's identity, and makes it the call tree's.
    ///
    /// # Errors
    ///
    /// `unavailable` with the journal's own code when it does not take the
    /// opening, and the trap of a journal that does not answer within the host
    /// call's deadline.
    async fn owned(&self) -> Result<(), KernelError> {
        let (frame, kernel) = self.acting("commands#begin")?;
        let label = self.plugin().to_owned();
        let opening = async {
            kernel
                .journal
                .lock()
                .await
                .open_unit(label)
                .await
                .map_err(|error| {
                    refused(&Refusal::new(
                        RefusalKind::Unavailable,
                        error.code(),
                        "the journal did not take the opening of the timer's unit, so nothing \
                         was begun"
                            .to_owned(),
                    ))
                })
        };
        let unit = crate::supervisor::in_time("commands#begin", self.left(), opening).await?;
        frame.tree().begins(crate::units::Ambient::new(unit));
        Ok(())
    }

    /// Dooms the unit of the call tree of the command running now, for the
    /// log's `cause` and as `why` says: a plugin aborted a unit, or let one go.
    pub(crate) fn doom_the_tree(&self, why: &str, cause: Cause) {
        if let Some(frame) = self.frame()
            && let Some(unit) = frame.tree().unit_mut()
        {
            unit.doom_for(
                Refusal::new(
                    RefusalKind::Internal,
                    ErrorCode::KernelUnitAborted,
                    why.to_owned(),
                ),
                cause,
            );
        }
    }
}

impl<E: Environment> world::Host for HostState<E> {
    async fn begin(&mut self, label: String) -> Result<Resource<world::Unit>, KernelError> {
        self.require(Capability::CommandsBegin, "begin")?;
        let begun = self
            .begins_here()
            .map_err(|refusal| self.refused_here("commands#begin", &refusal))?;
        let handle = self.units.begun().map_err(|refusal| refused(&refusal))?;
        if begun == Begun::Owned
            && let Err(refusal) = self.owned().await
        {
            self.units.ended(handle);
            return Err(refusal);
        }
        if let Some(frame) = self.frame()
            && let Some(unit) = frame.tree().unit_mut()
        {
            unit.labelled(&label);
        }
        Ok(Resource::new_own(handle))
    }

    async fn dispatch(&mut self, name: String, args: String) -> Result<String, KernelError> {
        self.require(Capability::CommandsDispatch, "dispatch")?;
        let Some(frame) = self.frame() else {
            return Err(self.refused_here(
                "commands#dispatch",
                &Refusal::new(
                    RefusalKind::Denied,
                    ErrorCode::PluginReadOnlyFrame,
                    "a dispatch from `activate` is refused: no command runs there, so there is no \
                 one to run it as"
                        .to_owned(),
                ),
            ));
        };
        frame
            .dispatch(&name, &args)
            .await
            .map_err(|refusal| refused(&refusal))
    }
}

#[expect(
    clippy::unused_async_trait_impl,
    reason = "the world declares these async, and each answers from what this store and the \
              call tree hold: a compensation is judged against the registry, never run"
)]
impl<E: Environment> world::HostUnit for HostState<E> {
    async fn compensate_with(
        &mut self,
        unit: Resource<world::Unit>,
        name: String,
        args: String,
    ) -> Result<(), KernelError> {
        const FUNCTION: &str = "[method]unit.compensate-with";
        self.require(Capability::CommandsBegin, FUNCTION)?;
        match self.units.found(unit.rep()) {
            Found::Held(_) => {}
            Found::Ended => return Err(ended(FUNCTION)),
            Found::NeverIssued => return Err(KernelError::Trap(not_issued(FUNCTION))),
        }
        let kernel = self.frame().and_then(crate::dispatch::Frame::kernel);
        let Some(kernel) = kernel else {
            return Err(refused(&Refusal::new(
                RefusalKind::Internal,
                ErrorCode::KernelUnitAborted,
                "the compensation was not registered: the kernel the command ran under is gone"
                    .to_owned(),
            )));
        };
        // **Another's command is a dispatch, put off until the undo.** The undo
        // dispatches it as the person, which is what `commands.dispatch`
        // grants: without it a plugin names only its own commands, and is
        // told no more of another's than that it is registered.
        if kernel.foreign_to(&name, &self.plugin) {
            self.require(Capability::CommandsDispatch, FUNCTION)?;
        }
        let note = crate::history::compensation(&kernel, &self.plugin, &name, &args)
            .map_err(|refusal| refused(&refusal))?;
        let registered = self
            .frame()
            .and_then(|frame| frame.tree().unit_mut().map(|unit| unit.compensate(note)));
        match registered {
            Some(answer) => answer.map_err(|refusal| refused(&refusal)),
            None => Err(ended(FUNCTION)),
        }
    }

    async fn commit(&mut self, unit: Resource<world::Unit>) -> Result<(), KernelError> {
        self.require(Capability::CommandsBegin, "[method]unit.commit")?;
        match self.units.found(unit.rep()) {
            Found::Held(_) => {
                self.units.held.insert(unit.rep(), Held::Committed);
                Ok(())
            }
            Found::Ended => Err(ended("[method]unit.commit")),
            Found::NeverIssued => Err(KernelError::Trap(not_issued("[method]unit.commit"))),
        }
    }

    async fn abort(&mut self, unit: Resource<world::Unit>) -> wasmtime::Result<()> {
        self.require_or_trap(Capability::CommandsBegin, "[method]unit.abort")?;
        if self.units.found(unit.rep()) == Found::NeverIssued {
            return Err(not_issued("[method]unit.abort"));
        }
        if self.units.ended(unit.rep()) {
            self.doom_the_tree(
                "a plugin aborted a unit it began inside this call tree",
                Cause::Explicit,
            );
        }
        Ok(())
    }

    async fn drop(&mut self, unit: Resource<world::Unit>) -> wasmtime::Result<()> {
        self.release(Capability::CommandsBegin)?;
        if self.units.ended(unit.rep()) {
            self.doom_the_tree(
                "a plugin let a unit it began inside this call tree go without committing it",
                Cause::Dropped,
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use wasmtime::component::Resource;

    use crate::InMemory;
    use crate::bindings::KernelError;
    use crate::bindings::noctua::kernel::commands::{Host as _, HostUnit as _};
    use crate::bindings::noctua::kernel::types;
    use crate::capability::GrantSet;
    use crate::dispatch::{Call, Cause, Frame};
    use crate::host::HostState;
    use nc_catalog::{Capability, ErrorCode, KERNEL};

    /// Whether `answer` is a denial opening on `code`.
    fn denied_with<T: core::fmt::Debug>(answer: &Result<T, KernelError>, code: ErrorCode) -> bool {
        matches!(
            answer,
            Err(KernelError::Refused(types::Error::Denied(why)))
                if why.starts_with(&format!("[{code}] "))
        )
    }

    /// Whether `answer` is a refusal of `kind`'s case opening on `code`.
    fn refused_as<T: core::fmt::Debug>(
        answer: &Result<T, KernelError>,
        kind: fn(&types::Error) -> Option<&String>,
        code: ErrorCode,
    ) -> bool {
        matches!(answer, Err(KernelError::Refused(error))
            if kind(error).is_some_and(|why| why.starts_with(&format!("[{code}] "))))
    }

    /// The sentence of an `invalid`.
    fn invalid(error: &types::Error) -> Option<&String> {
        match error {
            types::Error::Invalid(why) => Some(why),
            _ => None,
        }
    }

    /// The sentence of an `unavailable`.
    fn unavailable(error: &types::Error) -> Option<&String> {
        match error {
            types::Error::Unavailable(why) => Some(why),
            _ => None,
        }
    }

    /// The sentence of an `internal`.
    fn internal(error: &types::Error) -> Option<&String> {
        match error {
            types::Error::Internal(why) => Some(why),
            _ => None,
        }
    }

    /// Whether `answer` is the trap of a handle this kernel never issued.
    fn never_issued<T: core::fmt::Debug>(answer: &Result<T, KernelError>) -> bool {
        matches!(answer, Err(KernelError::Trap(trap)) if trap.to_string().contains("did not issue"))
    }

    /// A store granted both capabilities, running a command that changes
    /// state in a tree holding a unit, and that tree.
    fn in_a_change() -> (
        HostState<InMemory>,
        std::sync::Arc<std::sync::Mutex<crate::dispatch::CallTree>>,
    ) {
        let mut state = HostState::in_memory(GrantSet::of([
            Capability::CommandsDispatch,
            Capability::CommandsBegin,
        ]));
        let frame = Frame::for_tests(true, Some(unit()));
        let tree = frame.tree_for_tests();
        state.enter(frame);
        (state, tree)
    }

    /// A unit, for a tree to hold.
    fn unit() -> nc_ids::Snowflake {
        nc_ids::Minter::new(
            nc_ids::MachineFlake::from_value(1).expect("a flake"),
            nc_ids::SystemClock,
        )
        .mint()
        .expect("a unit")
    }

    /// The cause the tree's unit is doomed for, if it is.
    fn doomed(tree: &std::sync::Mutex<crate::dispatch::CallTree>) -> Option<Cause> {
        crate::dispatch::lock(tree)
            .unit()
            .and_then(crate::units::Ambient::doomed_for)
    }

    #[tokio::test]
    async fn every_function_is_denied_without_its_capability_and_runs_only_in_a_command() {
        for granted in [false, true] {
            let mut state = HostState::in_memory(if granted {
                GrantSet::of([Capability::CommandsDispatch, Capability::CommandsBegin])
            } else {
                GrantSet::none()
            });
            let outside = if granted {
                ErrorCode::PluginReadOnlyFrame
            } else {
                ErrorCode::PluginCapabilityDenied
            };
            let dispatched = state
                .dispatch("reference.note.get".to_owned(), "{}".to_owned())
                .await;
            assert!(denied_with(&dispatched, outside), "{dispatched:?}");
            let begun = state.begin("a step".to_owned()).await;
            assert!(denied_with(&begun, outside), "{begun:?}");
            let compensated = state
                .compensate_with(Resource::new_own(0), "a.b".to_owned(), "{}".to_owned())
                .await;
            let committed = state.commit(Resource::new_own(0)).await;
            for answer in [compensated, committed] {
                if granted {
                    assert!(never_issued(&answer), "{answer:?}");
                } else {
                    assert!(
                        denied_with(&answer, ErrorCode::PluginCapabilityDenied),
                        "{answer:?}"
                    );
                }
            }
            assert_eq!(state.reached().len(), 4);
        }
    }

    #[tokio::test]
    async fn dispatch_and_begin_answer_to_two_capabilities() {
        let mut dispatching = HostState::in_memory(GrantSet::of([Capability::CommandsDispatch]));
        dispatching.enter(Frame::for_tests(true, Some(unit())));
        assert!(denied_with(
            &dispatching.begin("a step".to_owned()).await,
            ErrorCode::PluginCapabilityDenied
        ));
        let mut beginning = HostState::in_memory(GrantSet::of([Capability::CommandsBegin]));
        beginning.enter(Frame::for_tests(true, Some(unit())));
        assert!(denied_with(
            &beginning.dispatch("a.b".to_owned(), "{}".to_owned()).await,
            ErrorCode::PluginCapabilityDenied
        ));
        assert!(beginning.begin("a step".to_owned()).await.is_ok());
    }

    /// A dispatch from a frame whose kernel is gone is refused as the unit's
    /// loss, never run.
    #[tokio::test]
    async fn a_dispatch_from_a_frame_no_kernel_holds_is_refused() {
        let mut state = HostState::in_memory(GrantSet::of([Capability::CommandsDispatch]));
        state.enter(Frame::for_tests(true, None));
        let answer = state
            .dispatch("reference.note.get".to_owned(), "{}".to_owned())
            .await;
        assert!(
            refused_as(&answer, internal, ErrorCode::KernelUnitAborted),
            "{answer:?}"
        );
    }

    /// A unit is begun inside a command that changes state, in its tree's
    /// unit, and nowhere else.
    #[tokio::test]
    async fn a_unit_is_begun_only_inside_a_command_that_changes_state() {
        let grants = GrantSet::of([Capability::CommandsBegin]);
        let mut nowhere = HostState::in_memory(grants.clone());
        let reading = |unit| Frame::for_tests(false, unit);
        for frame in [
            None,
            Some(reading(Some(unit()))),
            Some(Frame::for_tests(true, None)),
        ] {
            let mut state = HostState::in_memory(grants.clone());
            let named = format!("{frame:?}");
            if let Some(frame) = frame {
                state.enter(frame);
            }
            let begun = state.begin("a step".to_owned()).await;
            assert!(
                denied_with(&begun, ErrorCode::PluginReadOnlyFrame),
                "{named}: {begun:?}"
            );
        }
        nowhere.enter(Frame::for_tests(true, Some(unit())));
        assert!(nowhere.begin("a step".to_owned()).await.is_ok());
    }

    /// A callback that only reads begins no unit: an event's, and a timer's
    /// set where only reading was allowed.
    ///
    /// **The control is a timer set where a change was allowed**, which gets
    /// past the frame rule to open a unit of its own, and is refused there
    /// only because no kernel stands behind a test's frame — the answer a
    /// callback that only reads would be given too, were that rule not asked
    /// first.
    #[tokio::test]
    async fn a_callback_that_only_reads_begins_no_unit() {
        for (call, reads) in [
            (Call::Event, true),
            (Call::Timer { may_change: false }, true),
            (Call::Timer { may_change: true }, false),
        ] {
            let mut state = HostState::in_memory(GrantSet::of([Capability::CommandsBegin]));
            state.enter(Frame::for_tests(false, None).calling_for_tests(call));
            let begun = state.begin("a step".to_owned()).await;
            if reads {
                assert!(
                    denied_with(&begun, ErrorCode::PluginReadOnlyFrame),
                    "{call:?}: {begun:?}"
                );
            } else {
                assert!(
                    refused_as(&begun, internal, ErrorCode::KernelUnitAborted),
                    "{call:?}: {begun:?}"
                );
            }
        }
    }

    /// A timer's callback whose tree holds a unit begins into that unit, and
    /// opens no other: a callback runs in at most one.
    ///
    /// **A second unit would take the first one's place** in the tree, and the
    /// first would then be neither kept nor let go — its writes lost, its unit
    /// in the journal left open until the recovery on open. Opening one asks
    /// for the kernel a test's frame stands without, so a `begin` that tried
    /// would be refused here.
    #[tokio::test]
    async fn a_timers_callback_begins_into_the_unit_its_tree_holds_already() {
        let held = unit();
        let mut state = HostState::in_memory(GrantSet::of([Capability::CommandsBegin]));
        let frame =
            Frame::for_tests(false, Some(held)).calling_for_tests(Call::Timer { may_change: true });
        let tree = frame.tree_for_tests();
        state.enter(frame);
        let begun = state.begin("again".to_owned()).await;
        assert!(begun.is_ok(), "flattened into the tree's unit: {begun:?}");
        assert_eq!(
            crate::dispatch::lock(&tree)
                .unit()
                .map(crate::units::Ambient::unit),
            Some(held),
            "the unit the tree held is the one it holds"
        );
    }

    /// A unit committed leaves the tree's unit to be kept.
    ///
    /// An abort, a handle let go and one left open at the command's end each
    /// doom it, for a cause of their own.
    #[tokio::test]
    async fn a_committed_unit_is_kept_and_an_abort_a_drop_or_a_return_dooms_the_tree() {
        let (mut state, tree) = in_a_change();
        let first = state.begin("kept".to_owned()).await.expect("begun");
        let rep = first.rep();
        assert!(state.commit(Resource::new_own(rep)).await.is_ok());
        assert!(
            state.commit(Resource::new_own(rep)).await.is_ok(),
            "committed twice"
        );
        state
            .abort(Resource::new_own(rep))
            .await
            .expect("an abort after the commit is nothing");
        crate::bindings::noctua::kernel::commands::HostUnit::drop(&mut state, first)
            .await
            .expect("let go");
        assert_eq!(
            doomed(&tree),
            None,
            "a unit committed and let go dooms nothing"
        );

        let dropped = state.begin("dropped".to_owned()).await.expect("begun");
        crate::bindings::noctua::kernel::commands::HostUnit::drop(&mut state, dropped)
            .await
            .expect("let go");
        assert_eq!(doomed(&tree), Some(Cause::Dropped));

        let (mut state, tree) = in_a_change();
        let aborted = state.begin("aborted".to_owned()).await.expect("begun");
        let rep = aborted.rep();
        state.abort(aborted).await.expect("aborted");
        assert_eq!(doomed(&tree), Some(Cause::Explicit));
        let after = state.commit(Resource::new_own(rep)).await;
        assert!(
            refused_as(&after, invalid, ErrorCode::PluginUnitEnded),
            "a unit aborted is ended: {after:?}"
        );

        let (mut state, tree) = in_a_change();
        let open = state.begin("left open".to_owned()).await.expect("begun");
        let rep = open.rep();
        let _denials = state.leave();
        assert_eq!(doomed(&tree), Some(Cause::Dropped), "returned with it open");
        state.enter(Frame::for_tests(true, Some(unit())));
        for answer in [
            state.commit(Resource::new_own(rep)).await,
            state
                .compensate_with(Resource::new_own(rep), "a.b".to_owned(), "{}".to_owned())
                .await,
        ] {
            assert!(
                refused_as(&answer, invalid, ErrorCode::PluginUnitEnded),
                "a unit begun in a command that returned is ended: {answer:?}"
            );
        }

        let (mut state, tree) = in_a_change();
        let committed = state.begin("committed".to_owned()).await.expect("begun");
        let rep = committed.rep();
        state.commit(committed).await.expect("committed");
        let _denials = state.leave();
        assert_eq!(doomed(&tree), None, "returned with it committed");
        // **The handle of a unit that was kept ends with its command too**, and
        // a later command calling on it is told the handle ended — never that a
        // unit was aborted, which would be false, the kept unit and the asking
        // command's own both standing.
        let frame = Frame::for_tests(true, Some(unit()));
        let asking = frame.tree_for_tests();
        state.enter(frame);
        let used_again = state.commit(Resource::new_own(rep)).await;
        assert!(
            refused_as(&used_again, invalid, ErrorCode::PluginUnitEnded),
            "the handle of a unit kept, used in a later command: {used_again:?}"
        );
        assert_eq!(
            doomed(&asking),
            None,
            "the asking command's unit is not aborted"
        );
    }

    /// A plugin holds at most `unit_handles` units at once, and one let go is
    /// room for the next.
    #[tokio::test]
    async fn a_plugin_holds_no_more_units_than_the_catalog_allows() {
        let (mut state, _tree) = in_a_change();
        let mut held = Vec::new();
        for _ in 0..KERNEL.unit_handles {
            held.push(
                state
                    .begin("one".to_owned())
                    .await
                    .expect("within the bound"),
            );
        }
        let more = state.begin("one more".to_owned()).await;
        assert!(
            refused_as(&more, unavailable, ErrorCode::PluginBoundReached),
            "{more:?}"
        );
        let first = held.remove(0);
        state
            .commit(Resource::new_own(first.rep()))
            .await
            .expect("committed");
        crate::bindings::noctua::kernel::commands::HostUnit::drop(&mut state, first)
            .await
            .expect("let go");
        assert!(state.begin("again".to_owned()).await.is_ok());
    }

    /// A compensation registered where the kernel is gone is refused as the
    /// unit's loss.
    #[tokio::test]
    async fn a_compensation_where_no_kernel_holds_the_command_is_refused() {
        let (mut state, _tree) = in_a_change();
        let unit = state.begin("a step".to_owned()).await.expect("begun");
        let answer = state
            .compensate_with(unit, "a.b".to_owned(), "{}".to_owned())
            .await;
        assert!(
            refused_as(&answer, internal, ErrorCode::KernelUnitAborted),
            "{answer:?}"
        );
    }

    #[tokio::test]
    async fn abort_traps_and_a_drop_needs_its_capability() {
        let mut none = HostState::in_memory(GrantSet::none());
        // **Refused for the capability, not for the handle.** Both are traps
        // here, so only the sentence tells an `abort` that asked from one that
        // did not.
        let abort = none.abort(Resource::new_own(0)).await;
        assert!(
            abort
                .as_ref()
                .is_err_and(|trap| trap.to_string().contains("was not granted")),
            "{abort:?}"
        );
        assert_eq!(none.reached().len(), 1, "the refused `abort` was recorded");
        assert!(
            crate::bindings::noctua::kernel::commands::HostUnit::drop(
                &mut none,
                Resource::new_own(0)
            )
            .await
            .is_err()
        );
        let mut granted = HostState::in_memory(GrantSet::of([Capability::CommandsBegin]));
        assert!(
            granted
                .abort(Resource::new_own(0))
                .await
                .is_err_and(|trap| trap.to_string().contains("did not issue"))
        );
        assert!(
            crate::bindings::noctua::kernel::commands::HostUnit::drop(
                &mut granted,
                Resource::new_own(0)
            )
            .await
            .is_ok(),
            "a handle never issued is nothing to let go"
        );
    }

    /// A plugin's store granted `granted`, running the callback of a timer set
    /// where a change was allowed, in a call tree of `kernel`'s for `ana` that
    /// began at `began` and is within its deadline on the kernel's clock.
    fn in_a_timers_callback(
        kernel: &crate::Kernel<InMemory>,
        ana: nc_authz::Principal,
        granted: Capability,
        began: std::time::Instant,
    ) -> HostState<InMemory> {
        let now_ms = nc_ids::Clock::now_ms(&nc_ids::SystemClock);
        let tree = std::sync::Arc::new(std::sync::Mutex::new(crate::dispatch::CallTree::new(
            ana, now_ms,
        )));
        let mut state = HostState::in_memory(GrantSet::of([granted]));
        state.enter(
            Frame::new(
                std::sync::Arc::downgrade(&kernel.shared),
                tree,
                crate::PluginSlot(0),
                Call::Timer { may_change: true },
                crate::dispatch::Authority {
                    command: "tests.door".to_owned(),
                    action: "Write".to_owned(),
                    resource: nc_authz::EntityRef {
                        type_name: "Content".to_owned(),
                        id: "photo".to_owned(),
                    },
                },
                false,
            )
            .begun_for_tests(began),
        );
        state
    }

    /// A timer's `begin` whose journal never answers traps at the host call's
    /// deadline, rather than holding the pump that called it for ever.
    ///
    /// The journal's lock is held by the test, so the opening of the
    /// callback's own unit waits on it; the clock is paused, so the deadline
    /// passes the moment nothing else can run, and the outer bound turns a
    /// missing deadline into a failure instead of a test that never ends.
    /// **The deadline is held in `owned`**: with it taken out, this times out
    /// at twice the host call's deadline and fails.
    #[tokio::test(start_paused = true)]
    async fn a_timers_begin_whose_journal_never_answers_traps_at_the_deadline() {
        let (kernel, ana) = crate::host::tests::a_kernel_for_ana().await;
        let mut state = in_a_timers_callback(
            &kernel,
            ana,
            Capability::CommandsBegin,
            std::time::Instant::now(),
        );
        let _the_journal_answers_nobody = kernel.shared.journal.lock().await;
        let begun = tokio::time::timeout(
            std::time::Duration::from_millis(KERNEL.host_call_deadline_ms * 2),
            state.begin("a step".to_owned()),
        )
        .await
        .expect("a timer's `begin` waited on the journal past twice the host call's deadline");
        assert!(
            matches!(&begun, Err(KernelError::Trap(trap))
                if trap.to_string().contains("`commands#begin`")),
            "{begun:?}"
        );
    }

    /// A dispatch from a plugin, whose journal never answers, waits on it no
    /// longer than a host call may wait, and the plugin reads its refusal.
    ///
    /// Beside the test above: `commands.dispatch` is the host call no deadline
    /// of its own holds, so what holds it is each wait of the dispatch nested
    /// in the tree. The command is unknown, so the dispatch is refused at the
    /// registry, and that refusal is journaled before it is answered, on a
    /// journal whose lock the test holds. **The wait has a bound**: with none,
    /// the plugin would not be stopped when its tree's time is up — a timer's
    /// callback would hold the pump and the bus while the journal stalled.
    #[tokio::test(start_paused = true)]
    async fn a_dispatch_whose_journal_never_answers_waits_no_longer_than_a_host_call() {
        let (kernel, ana) = crate::host::tests::a_kernel_for_ana().await;
        let mut state = in_a_timers_callback(
            &kernel,
            ana,
            Capability::CommandsDispatch,
            std::time::Instant::now(),
        );
        let _the_journal_answers_nobody = kernel.shared.journal.lock().await;
        let started = tokio::time::Instant::now();
        let answered = tokio::time::timeout(
            std::time::Duration::from_millis(KERNEL.host_call_deadline_ms * 2),
            state.dispatch("tests.anything".to_owned(), "{}".to_owned()),
        )
        .await
        .expect("a dispatch waited on the journal past twice the host call's deadline");
        let waited = started.elapsed();
        assert!(
            refused_as(
                &answered,
                |error| match error {
                    types::Error::NotFound(why) => Some(why),
                    _ => None,
                },
                ErrorCode::KernelUnknownCommand
            ),
            "{answered:?}"
        );
        assert!(
            waited >= std::time::Duration::from_millis(KERNEL.host_call_deadline_ms),
            "the journal was given {waited:?}, less than a host call's deadline"
        );
    }

    /// A dispatch from a call tree with no time left, whose journal never
    /// answers, is answered at once: what the tree has left is nearer than a
    /// host call's deadline. A dispatch still waiting on the journal when
    /// that deadline comes fails it.
    #[tokio::test(start_paused = true)]
    async fn a_dispatch_from_a_tree_with_no_time_left_is_answered_though_the_journal_never_is() {
        let (kernel, ana) = crate::host::tests::a_kernel_for_ana().await;
        let long_ago = std::time::Instant::now()
            .checked_sub(std::time::Duration::from_millis(
                KERNEL.dispatch_deadline_ms,
            ))
            .expect("the machine has run longer than a call tree may");
        let mut state = in_a_timers_callback(&kernel, ana, Capability::CommandsDispatch, long_ago);
        let _the_journal_answers_nobody = kernel.shared.journal.lock().await;
        let started = tokio::time::Instant::now();
        let answered = tokio::time::timeout(
            std::time::Duration::from_millis(KERNEL.host_call_deadline_ms * 2),
            state.dispatch("tests.anything".to_owned(), "{}".to_owned()),
        )
        .await
        .expect("a dispatch from a tree with no time left waited on the journal");
        assert!(answered.is_err(), "{answered:?}");
        assert!(
            started.elapsed() < std::time::Duration::from_millis(KERNEL.host_call_deadline_ms),
            "a tree with no time left waited {:?} on the journal",
            started.elapsed()
        );
    }
}
