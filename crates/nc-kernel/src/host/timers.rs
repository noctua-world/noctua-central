//! `timers`: being woken after a while, or at an interval.
//!
//! Gated by `timers`. Every function asks it first and answers `denied` to a
//! plugin not granted it; to one that is, it sets or cancels a timer under the
//! frame of the call running now — refused with `[NC-PLUG-0025]` where none
//! runs — within the bounds `crate::timers` documents, held by the call tree's
//! unit until it is kept, or by the tree until it ends where no unit is. A
//! timer fires through the plugin's
//! own `on-timer` export, called by the kernel's pump.
//!
//! What this answers (`NOCTUA.md §2.4`): a plugin setting timers past its
//! bound or tighter than the catalog allows, and cancelling another's. What it
//! does not answer: what a plugin does when it is woken, which its callback's
//! call tree holds it to.

use nc_catalog::Capability;

use super::{HostState, refused};
use crate::Environment;
use crate::bindings::KernelError;
use crate::bindings::noctua::kernel::timers as world;

impl<E: Environment> HostState<E> {
    /// Sets a timer for `function`, of `ms`, repeating when `every`, and
    /// tagged `tag`.
    fn timer(
        &self,
        function: &'static str,
        ms: u64,
        every: bool,
        tag: String,
    ) -> Result<u64, KernelError> {
        let (frame, kernel) = self.acting(function)?;
        crate::timers::set(&kernel, frame, ms, every, tag).map_err(|refusal| refused(&refusal))
    }
}

#[expect(
    clippy::unused_async_trait_impl,
    reason = "the world declares these async, and a timer is set and cancelled in the kernel's \
              memory: none of them waits on anything"
)]
impl<E: Environment> world::Host for HostState<E> {
    async fn after(&mut self, ms: u64, tag: String) -> Result<u64, KernelError> {
        self.require(Capability::Timers, "after")?;
        self.timer("timers#after", ms, false, tag)
    }

    async fn every(&mut self, ms: u64, tag: String) -> Result<u64, KernelError> {
        self.require(Capability::Timers, "every")?;
        self.timer("timers#every", ms, true, tag)
    }

    async fn cancel(&mut self, id: u64) -> Result<(), KernelError> {
        self.require(Capability::Timers, "cancel")?;
        let (frame, kernel) = self.acting("timers#cancel")?;
        crate::timers::cancel(&kernel, frame, id).map_err(|refusal| refused(&refusal))
    }
}

#[cfg(test)]
mod tests {
    use crate::bindings::noctua::kernel::timers::Host as _;
    use crate::capability::GrantSet;
    use crate::host::HostState;
    use crate::host::tests::answered_outside_a_tree;
    use nc_catalog::Capability;

    #[tokio::test]
    async fn every_function_is_denied_without_the_capability_and_refused_outside_a_call_tree() {
        for granted in [false, true] {
            let mut state = HostState::in_memory(if granted {
                GrantSet::of([Capability::Timers])
            } else {
                GrantSet::none()
            });
            let answers = [
                state.after(1_000, "t".to_owned()).await.map(drop),
                state.every(1_000, "t".to_owned()).await.map(drop),
                state.cancel(0).await,
            ];
            for answer in &answers {
                assert!(answered_outside_a_tree(answer, granted), "{answer:?}");
            }
            assert_eq!(state.reached().len(), answers.len());
        }
    }
}
