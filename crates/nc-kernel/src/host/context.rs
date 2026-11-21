//! `context`: who a call tree runs for, read-only to every plugin.
//!
//! Gated by the `context` capability. Every function asks it first and answers
//! `denied` to a plugin not granted it; to one that is, it answers what the
//! frame of the call running now says — the principal as its opaque
//! identifier, never a name: in a tree acting for an agent, the agent; the
//! actor chain, on whose behalf the tree acts, outermost first, each link's
//! opaque identifier — the person an agent acts for, then each agent between
//! — which is the chain every entry of the tree records, and empty for a
//! principal acting as itself; and whether a person confirmed the call, which
//! is true only for the dispatch at the top of a call tree that redeemed a
//! confirmation the kernel issued for it, never for one nested beneath it or
//! for a callback (`crate::dispatch`), and never in a tree acting for an
//! agent, whatever its frame holds. Where no call tree runs — `activate` —
//! each is refused with `[NC-PLUG-0025]`.
//!
//! **There is no setter in the world, and none here**: the kernel alone writes
//! what a plugin reads (`NOCTUA.md §11.7`).
//!
//! What this answers (`NOCTUA.md §2.4`): a plugin forging who it runs for, or
//! that a person confirmed it. What it does not answer: what a plugin does with
//! the identifier it is told, which names the person to anything it shares it
//! with.

use nc_catalog::Capability;

use super::HostState;
use crate::Environment;
use crate::bindings::KernelError;
use crate::bindings::noctua::kernel::context as world;

impl<E: Environment> world::Host for HostState<E> {
    fn principal(&mut self) -> Result<String, KernelError> {
        self.require(Capability::Context, "principal")?;
        let frame = self.in_a_tree("context#principal")?;
        Ok(frame.principal().id().as_str().to_owned())
    }

    fn actor_chain(&mut self) -> Result<Vec<String>, KernelError> {
        self.require(Capability::Context, "actor-chain")?;
        let frame = self.in_a_tree("context#actor-chain")?;
        Ok(frame
            .chain()
            .iter()
            .map(|link| link.as_str().to_owned())
            .collect())
    }

    fn confirmed_by_human(&mut self) -> Result<bool, KernelError> {
        self.require(Capability::Context, "confirmed-by-human")?;
        let frame = self.in_a_tree("context#confirmed-by-human")?;
        // **Never in an agent's tree**, whatever its frame was handed: no agent
        // holds what the kernel issues, and none is told it was confirmed.
        Ok(frame.confirmed() && frame.acting().may_hold_tokens())
    }
}

#[cfg(test)]
mod tests {
    use crate::bindings::noctua::kernel::context::Host as _;
    use crate::capability::GrantSet;
    use crate::dispatch::{Call, Frame};
    use crate::host::HostState;
    use crate::host::tests::answered_outside_a_tree;
    use nc_catalog::Capability;

    #[test]
    fn every_function_is_denied_without_the_capability_and_refused_outside_a_call_tree() {
        for granted in [false, true] {
            let mut state = HostState::in_memory(if granted {
                GrantSet::of([Capability::Context])
            } else {
                GrantSet::none()
            });
            let answers = [
                state.principal().map(drop),
                state.actor_chain().map(drop),
                state.confirmed_by_human().map(drop),
            ];
            for answer in &answers {
                assert!(answered_outside_a_tree(answer, granted), "{answer:?}");
            }
            assert_eq!(state.reached().len(), answers.len());
        }
    }

    #[test]
    fn a_call_reads_who_its_tree_runs_for_and_whether_a_person_confirmed_it() {
        let mut state = HostState::in_memory(GrantSet::of([Capability::Context]));
        let frame = Frame::for_tests(false, None);
        let ana = frame.principal().id().as_str().to_owned();
        state.enter(frame);
        assert_eq!(state.principal().expect("in a call tree"), ana);
        assert_eq!(
            state.actor_chain().expect("in a call tree"),
            [] as [String; 0]
        );
        assert!(!state.confirmed_by_human().expect("in a call tree"));
        state.leave();
        state.enter(Frame::for_tests(true, None).confirmed_for_tests());
        assert!(state.confirmed_by_human().expect("in a call tree"));
        state.leave();
        state.enter(
            Frame::for_tests(false, None).calling_for_tests(Call::Timer { may_change: true }),
        );
        assert_eq!(
            state.principal().expect("in a callback's call tree"),
            ana,
            "a callback runs for its tree's principal too"
        );
    }

    /// A plugin in an agent's tree reads no confirmation.
    ///
    /// **Whatever its frame holds**: the frame of a tree acting for ana's
    /// assistant is marked confirmed, as no dispatch of an agent's ever makes
    /// one, and the plugin is still told no person confirmed it — beside the
    /// agent as the principal and ana as the one link of its actor chain, in a
    /// command and in a callback alike.
    #[test]
    fn a_plugin_in_an_agents_tree_reads_no_confirmation() {
        let mut state = HostState::in_memory(GrantSet::of([Capability::Context]));
        let person = Frame::<crate::InMemory>::for_tests(false, None).principal();
        for frame in [
            Frame::for_agent_tests(true, None).confirmed_for_tests(),
            Frame::for_agent_tests(false, None).calling_for_tests(Call::Timer { may_change: true }),
        ] {
            let agent = frame.principal();
            state.enter(frame);
            assert_eq!(
                state.principal().expect("in a call tree"),
                agent.id().as_str(),
                "the agent acts"
            );
            assert_ne!(agent.id(), person.id());
            assert_eq!(
                state.actor_chain().expect("in a call tree"),
                [person.id().as_str().to_owned()],
                "on ana's behalf"
            );
            assert!(
                !state.confirmed_by_human().expect("in a call tree"),
                "an agent's tree read a confirmation"
            );
            state.leave();
        }
    }
}
