//! `events`: announcing that something happened.
//!
//! Gated by `events.publish`. `publish` asks it first and answers `denied` to
//! a plugin not granted it; to one that is, it publishes under the frame of
//! the call running now — refused with `[NC-PLUG-0025]` where none runs — a
//! topic of the plugin's own namespace and a payload of data, held by the call
//! tree's unit until it is kept or queued at once, within the queue's bounds
//! (`crate::events`). Hearing an event is `events.subscribe`, which grants no
//! function: the kernel calls `on-event`, which the plugin exports, for every
//! topic its activation subscribed to.
//!
//! What this answers (`NOCTUA.md §2.4`): a plugin announcing in another's name
//! or past what the kernel holds for it. What it does not answer: what a
//! subscriber makes of what it hears.

use nc_catalog::Capability;

use super::{HostState, refused};
use crate::Environment;
use crate::bindings::KernelError;
use crate::bindings::noctua::kernel::events as world;
use crate::events::Published;

#[expect(
    clippy::unused_async_trait_impl,
    reason = "the world declares this async, and publishing holds the event in the call tree's \
              unit or queues it, in the kernel's memory: it waits on nothing"
)]
impl<E: Environment> world::Host for HostState<E> {
    async fn publish(&mut self, topic: String, data: String) -> Result<(), KernelError> {
        self.require(Capability::EventsPublish, "publish")?;
        let (frame, kernel) = self.acting("events#publish")?;
        let namespace = kernel.namespace_of(frame.slot()).unwrap_or_default();
        crate::events::judged(&topic, &namespace)
            .map_err(|refusal| self.refused_here("events#publish", &refusal))?;
        let data = crate::events::payload(&data).map_err(|refusal| refused(&refusal))?;
        let event = Published {
            topic,
            data,
            publisher: frame.slot(),
            principal: frame.principal(),
            authority: frame.authority().clone(),
        };
        crate::events::publish(&kernel, &mut frame.tree(), event)
            .map_err(|refusal| refused(&refusal))
    }
}

#[cfg(test)]
mod tests {
    use crate::bindings::noctua::kernel::events::Host as _;
    use crate::capability::GrantSet;
    use crate::host::HostState;
    use crate::host::tests::answered_outside_a_tree;
    use nc_catalog::Capability;

    #[tokio::test]
    async fn publishing_is_denied_without_the_capability_and_refused_outside_a_call_tree() {
        for granted in [false, true] {
            let mut state = HostState::in_memory(if granted {
                GrantSet::of([Capability::EventsPublish])
            } else {
                GrantSet::of([Capability::EventsSubscribe])
            });
            let answer = state
                .publish("reference.saved".to_owned(), "{}".to_owned())
                .await;
            assert!(answered_outside_a_tree(&answer, granted), "{answer:?}");
        }
    }
}
