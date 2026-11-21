//! Who a call tree acts as: a principal acting as itself, or one agent's
//! session for the person who made it (`NOCTUA.md §14.6`).
//!
//! A [`Session`] is opened by the kernel alone ([`crate::Kernel::session`]):
//! the directory names the agent and the person it acts for
//! (`nc_authz::Directory::delegation`), and the chain of whom it acts for is
//! written here from that answer. A call tree acts as an [`Acting`], and every
//! decision of it asks the decision point once for each link — the principal
//! first, then each one it acts for, outermost first — at one moment and one
//! epoch, and stops at the first denial (`crate::dispatch`).
//!
//! # What it answers (`§2.4`)
//!
//! **Who acts is the directory's answer, and the chain is the kernel's.** No
//! caller, no message and no argument names an operator or a link. A session
//! is opened only for an agent its asking person made, and an agent held for
//! another person, one held for no person and one never made are refused in
//! one sentence and one code, so the caller cannot tell them apart.
//!
//! **An agent holds nothing the kernel issues.** A session has no slot for a
//! confirmation or an approval; and an agent named without a session acts
//! alone, unmarked, which the core refuses (`core.an_agent_never_impersonates`)
//! and which redeems nothing it presents either.
//!
//! **A chain is bounded by its type, above the policy.** It holds a person
//! first and agents after, none twice and none of them the acting agent, and
//! at most [`LINKS_MAX`] links; the policy's bound is lower
//! (`core.chain_depth_is_bounded`), so a chain the core refuses is one the
//! kernel still decides and journals whole. Nothing in this build makes a
//! chain longer than one link but the test door.
//!
//! # What it does not answer
//!
//! Whether an agent's rights are within its operator's, which the decision
//! point asks of each link at each decision; and how much a session spends.
//! What an agent's tree puts off — a timer it sets, an event it publishes —
//! is put off as the agent acting alone, which the core refuses when it comes
//! due.

use std::sync::Arc;
use std::time::Instant;

use nc_authz::{Directory as _, EntityRef, Principal, PrincipalKind};
use nc_catalog::ErrorCode;
use nc_ids::OpaqueId;

use crate::dispatch::{Refusal, RefusalKind, within};
use crate::{Environment, Shared};

/// How many links an actor chain holds at most: its operator and every agent
/// it acts through.
///
/// The policy's bound is lower (`core.chain_depth_is_bounded`), and this is
/// the type's, past it, so that a chain the core refuses is one the kernel
/// still decides and journals whole: refusing to record one would erase the
/// evidence.
pub(crate) const LINKS_MAX: usize = 4;

/// The type's bound held past the policy's two links, or a chain the core
/// refuses could not be built to be decided and journaled.
const _: () = assert!(LINKS_MAX > 2);

/// On whose behalf an agent acts, outermost first: the person who operates it,
/// then each agent it acts through. Written by the kernel alone.
///
/// Held by the two constructors a session's chain is built with: the first
/// link is a `User`, every other link an `Agent`, no principal appears twice —
/// nor the acting agent — and there are at most [`LINKS_MAX`] links.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ActorChain {
    /// The person it begins with.
    operator: Principal,
    /// Each agent after the person, outermost first.
    through: Vec<Principal>,
}

impl ActorChain {
    /// The chain of an agent whose operator is `operator`: none unless a person.
    fn operated_by(operator: Principal) -> Option<Self> {
        (operator.kind() == PrincipalKind::User).then(|| Self {
            operator,
            through: Vec::new(),
        })
    }

    /// The same chain through `agent`, one link further in, for `acting`.
    ///
    /// None for a link that is not an agent, one the chain holds already, the
    /// acting agent itself, and a link past [`LINKS_MAX`].
    #[cfg(any(test, feature = "test-support"))]
    fn through(mut self, agent: Principal, acting: &Principal) -> Option<Self> {
        let joins = agent.kind() == PrincipalKind::Agent
            && agent != *acting
            && !self.through.contains(&agent)
            && self.through.len() + 1 < LINKS_MAX;
        joins.then(|| {
            self.through.push(agent);
            self
        })
    }

    /// Every link, outermost first: the person, then each agent.
    pub(crate) fn outermost_first(&self) -> impl Iterator<Item = &Principal> {
        core::iter::once(&self.operator).chain(&self.through)
    }

    /// How many agents stand between the person and the dispatch: the chain's
    /// length, one for an agent acting for its person.
    pub(crate) fn depth(&self) -> u32 {
        u32::try_from(self.through.len().saturating_add(1)).unwrap_or(u32::MAX)
    }

    /// Each link's identifier, outermost first, as the journal records them.
    pub(crate) fn recorded(&self) -> Vec<OpaqueId> {
        self.outermost_first()
            .map(|link| link.id().clone())
            .collect()
    }
}

/// One agent's session: the agent the directory vouched for, and the chain
/// the kernel wrote for it. Opened by [`crate::Kernel::session`] alone.
///
/// **It holds nothing the kernel issues, and has nowhere to hold it**: there is
/// no confirmation and no approval here, so no agent presents either. A clone
/// is the same session.
#[derive(Debug, Clone)]
pub struct Session(Arc<Opened>);

/// What a session is: its agent and the chain it acts for.
#[derive(Debug)]
struct Opened {
    /// The agent, as the directory minted it.
    agent: Principal,
    /// Whom it acts for, outermost first.
    chain: ActorChain,
}

impl Session {
    /// The agent the session acts as.
    #[must_use]
    pub fn agent(&self) -> &Principal {
        &self.0.agent
    }

    /// The person the agent acts for: the first link of its chain.
    #[must_use]
    pub fn operator(&self) -> &Principal {
        &self.0.chain.operator
    }

    /// The session of `agent` acting for `operator` alone, whatever each is.
    ///
    /// What a unit test of a guard builds, so that only the guard stands
    /// between the session and what it asks for.
    #[cfg(test)]
    pub(crate) fn opened_for(agent: Principal, operator: Principal) -> Self {
        Self(Arc::new(Opened {
            agent,
            chain: ActorChain {
                operator,
                through: Vec::new(),
            },
        }))
    }

    /// The same agent, acting through `agents` too, after its operator and
    /// outermost first. **For tests only.**
    ///
    /// Nothing in this build makes an agent act through another — a later
    /// stage's attenuated tokens are the first that will — and this is how
    /// the decision point is shown a chain deeper than one. None for a chain
    /// its type refuses.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    #[must_use]
    pub fn through_for_tests(&self, agents: Vec<Principal>) -> Option<Self> {
        let mut chain = self.0.chain.clone();
        for agent in agents {
            chain = chain.through(agent, &self.0.agent)?;
        }
        Some(Self(Arc::new(Opened {
            agent: self.0.agent.clone(),
            chain,
        })))
    }
}

/// Who a call tree acts as.
#[derive(Debug, Clone)]
pub(crate) enum Acting {
    /// A person or a device deciding as itself.
    ///
    /// An `Agent` here is one named without a session, which acts alone and
    /// unmarked, and which `core.an_agent_never_impersonates` refuses.
    Itself(Principal),
    /// An agent, for the people and agents its session's chain names.
    Agent(Session),
}

impl Acting {
    /// Who acts: the principal every entry of the tree names.
    pub(crate) fn principal(&self) -> &Principal {
        match self {
            Self::Itself(principal) => principal,
            Self::Agent(session) => session.agent(),
        }
    }

    /// Whether an agent acts for someone: what every half of a decision is told.
    pub(crate) const fn via_agent(&self) -> bool {
        matches!(self, Self::Agent(_))
    }

    /// How many agents stand between the person and the dispatch: none for a
    /// principal acting as itself.
    pub(crate) fn depth(&self) -> u32 {
        match self {
            Self::Itself(_) => 0,
            Self::Agent(session) => session.0.chain.depth(),
        }
    }

    /// The chain the journal records: empty for a principal acting as itself.
    pub(crate) fn recorded_chain(&self) -> Vec<OpaqueId> {
        match self {
            Self::Itself(_) => Vec::new(),
            Self::Agent(session) => session.0.chain.recorded(),
        }
    }

    /// Each principal a decision asks, in the order it asks them: the
    /// principal, then the chain, outermost first.
    pub(crate) fn links(&self) -> impl Iterator<Item = &Principal> {
        let chain = match self {
            Self::Itself(_) => None,
            Self::Agent(session) => Some(&session.0.chain),
        };
        core::iter::once(self.principal())
            .chain(chain.into_iter().flat_map(ActorChain::outermost_first))
    }

    /// Whether the tree may redeem what the kernel issued: never an agent's
    /// session, and never an agent acting alone.
    pub(crate) fn may_hold_tokens(&self) -> bool {
        matches!(self, Self::Itself(principal) if principal.kind() != PrincipalKind::Agent)
    }
}

/// Opens the session of `agent` for `person`: the agent and the person it acts
/// for are the directory's answer, and never the caller's.
///
/// # Errors
///
/// `unavailable` with `[NC-POL-0006]` when the directory does not answer in a
/// host call's time; `not-found` with `[NC-AI-0001]`, in one sentence, when no
/// agent `person` made is held under `agent` — none is held, it acts for
/// another person, or it acts for no person.
pub(crate) async fn opened<E: Environment>(
    shared: &Shared<E>,
    agent: &EntityRef,
    person: &Principal,
) -> Result<Session, Refusal> {
    let realm = shared.realm.as_str();
    let delegation = within(Instant::now(), shared.directory.delegation(realm, agent))
        .await
        .map_err(|_| {
            Refusal::new(
                RefusalKind::Unavailable,
                ErrorCode::PolicyDirectoryUnavailable,
                "no session was opened: the directory the decision point reads could not answer"
                    .to_owned(),
            )
        })?;
    let none = || {
        Refusal::new(
            RefusalKind::NotFound,
            ErrorCode::AiNoSuchAgent,
            "no session was opened: no agent the asking person made is held under that \
             identifier"
                .to_owned(),
        )
    };
    // **The answer is held to the question**: an agent of another kind, or
    // under another identifier, is an answer for something else.
    let delegation = delegation
        .filter(|held| {
            held.agent.kind() == PrincipalKind::Agent
                && held.agent.entity() == *agent
                && held.operator == *person
        })
        .ok_or_else(none)?;
    let chain = ActorChain::operated_by(delegation.operator).ok_or_else(none)?;
    Ok(Session(Arc::new(Opened {
        agent: delegation.agent,
        chain,
    })))
}

#[cfg(test)]
mod tests {
    use nc_authz::{Principal, PrincipalKind};

    use super::{Acting, ActorChain, LINKS_MAX, Session};

    /// The world the policy cases are about, with `extra` agents of ana's.
    fn world_with(extra: &[&str]) -> nc_authz::MemoryDirectory {
        let mut world: serde_json::Value =
            serde_json::from_str(include_str!("../../../policy/world.json"))
                .expect("the world is JSON");
        let entities = world.as_array_mut().expect("the world is a list");
        for id in extra {
            entities.push(serde_json::json!({
                "uid": {"type": "Agent", "id": id},
                "attrs": {
                    "operator": {"__entity": {"type": "User", "id": "ana"}},
                    "autonomy": "supervised",
                    "budget_remaining": 1
                },
                "parents": [{"type": "Realm", "id": "home"}]
            }));
        }
        let pdp = nc_authz::Pdp::embedded("family").expect("the family pack is carried");
        nc_authz::MemoryDirectory::from_world(&world.to_string(), &pdp).expect("the world loads")
    }

    /// The principal of `kind` called `id` in `world`.
    fn of(world: &nc_authz::MemoryDirectory, kind: PrincipalKind, id: &str) -> Principal {
        world
            .principal(kind, id)
            .unwrap_or_else(|| panic!("the world holds {id}"))
    }

    #[test]
    fn a_chain_holds_no_link_twice_no_person_past_the_first_and_no_more_than_its_links() {
        let world = world_with(&["a1", "a2", "a3", "a4"]);
        let ana = of(&world, PrincipalKind::User, "ana");
        let bruno = of(&world, PrincipalKind::User, "bruno");
        let laptop = of(&world, PrincipalKind::Device, "ana-laptop");
        let acting = of(&world, PrincipalKind::Agent, "ana-assistant");
        let [a1, a2, a3, a4] =
            ["a1", "a2", "a3", "a4"].map(|id| of(&world, PrincipalKind::Agent, id));
        assert!(
            ActorChain::operated_by(acting.clone()).is_none(),
            "an agent"
        );
        assert!(
            ActorChain::operated_by(laptop.clone()).is_none(),
            "a device"
        );
        let chain = ActorChain::operated_by(ana.clone()).expect("a person");
        assert_eq!(chain.outermost_first().collect::<Vec<_>>(), [&ana]);
        for (refused, why) in [
            (ana.clone(), "the person again"),
            (bruno, "a person past the first"),
            (laptop, "a device"),
            (acting.clone(), "the acting agent"),
        ] {
            assert!(chain.clone().through(refused, &acting).is_none(), "{why}");
        }
        let one = chain.through(a1.clone(), &acting).expect("an agent joins");
        assert!(
            one.clone().through(a1.clone(), &acting).is_none(),
            "a repeat"
        );
        let full = one
            .through(a2.clone(), &acting)
            .and_then(|chain| chain.through(a3.clone(), &acting))
            .expect("agents join up to the bound");
        assert_eq!(
            full.outermost_first().collect::<Vec<_>>(),
            [&ana, &a1, &a2, &a3]
        );
        assert_eq!(full.outermost_first().count(), LINKS_MAX);
        assert!(
            full.through(a4, &acting).is_none(),
            "one link past the bound"
        );
    }

    #[test]
    fn the_depth_is_how_many_agents_stand_between_the_person_and_the_dispatch() {
        let world = world_with(&["a1"]);
        let ana = of(&world, PrincipalKind::User, "ana");
        let assistant = of(&world, PrincipalKind::Agent, "ana-assistant");
        let a1 = of(&world, PrincipalKind::Agent, "a1");
        assert_eq!(Acting::Itself(ana.clone()).depth(), 0);
        let session = Session::opened_for(assistant.clone(), ana.clone());
        assert_eq!(Acting::Agent(session.clone()).depth(), 1);
        let deeper = session
            .through_for_tests(vec![a1.clone()])
            .expect("an agent joins");
        assert_eq!(Acting::Agent(deeper.clone()).depth(), 2);
        assert_eq!(
            Acting::Agent(deeper.clone())
                .links()
                .cloned()
                .collect::<Vec<_>>(),
            [assistant.clone(), ana.clone(), a1.clone()],
            "the principal first, then the chain outermost first"
        );
        assert_eq!(
            Acting::Agent(deeper).recorded_chain(),
            [ana.id().clone(), a1.id().clone()]
        );
        assert_eq!(
            Acting::Itself(ana.clone()).links().collect::<Vec<_>>(),
            [&ana]
        );
        assert_eq!(Acting::Itself(ana).recorded_chain(), Vec::new());
        assert!(session.through_for_tests(vec![assistant]).is_none());
    }

    #[test]
    fn only_a_principal_acting_as_itself_and_no_agent_holds_what_the_kernel_issues() {
        let world = world_with(&[]);
        let ana = of(&world, PrincipalKind::User, "ana");
        let laptop = of(&world, PrincipalKind::Device, "ana-laptop");
        let assistant = of(&world, PrincipalKind::Agent, "ana-assistant");
        let session = Acting::Agent(Session::opened_for(assistant.clone(), ana.clone()));
        for (acting, holds, marked) in [
            (Acting::Itself(ana.clone()), true, false),
            (Acting::Itself(laptop), true, false),
            (Acting::Itself(assistant.clone()), false, false),
            (session.clone(), false, true),
        ] {
            assert_eq!(acting.may_hold_tokens(), holds, "{acting:?}");
            assert_eq!(acting.via_agent(), marked, "{acting:?}");
        }
        assert_eq!(session.principal(), &assistant, "the agent acts");
        let Acting::Agent(session) = session else {
            unreachable!("built as an agent's session")
        };
        assert_eq!(session.operator(), &ana);
    }
}
