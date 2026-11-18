//! The profile's directory: the world it was started with, and the agents its person made since.
//!
//! An agent is a delegation its person makes (`NOCTUA.md §14.6`): a principal of
//! its own, held under the identifier the machine's one minter gave it, for one
//! operator, with a label, a budget, and grants each scoped to one feature of
//! the realm. [`LocalDirectory`] answers every question about such an agent
//! from what it holds, and every other question from the world it was opened
//! over; a [`Keeper`] keeps each change where the next start reads it back.
//!
//! # What it answers (`§2.4`)
//!
//! **An agent is routed by the text it was held under, and nothing is parsed
//! from it.** A question about an `Agent` whose id is a key of the held map,
//! compared as text, is answered from that map, and any other goes to the
//! world: a lower-case spelling, a label, or the text of a value no change made
//! reaches no agent, and an `OpaqueId` keeps no structure anything reads
//! (`§5.1`). The text is the Crockford spelling of the `Snowflake` the kernel
//! minted, written here once, and the agent's principal is minted here alone.
//!
//! **A grant answers for the one feature it names.** Its scope is compared
//! whole with the resource a question names, never through an entity that
//! resource is in, so an agent granted every feature of the realm holds no
//! grant for the realm's content, which `core.provenance` then refuses.
//!
//! **What the data must never hold is refused when it is written**: an agent
//! whose operator is not a person; a grant given or ended by anyone but the
//! agent's operator; a grant with no time to run, on no feature of the realm,
//! or with a reason or a label it cannot hold; and an agent or a grant past its
//! table's bound, since neither is ever deleted.
//!
//! **Kept before it is held, and an ending held before it is kept.** A new
//! agent or grant is answered only once the keeper took it, so what a start
//! reads back is what was answered; an ending is answered at the very next
//! question even when the keeper refuses it, and the refusal says so.
//!
//! # What it does not answer
//!
//! Whether the person may make a change, which the decision point decides
//! before the change is asked for; whether a grant is current, which Cedar
//! decides against the kernel's clock; and whether an agent's rights are its
//! operator's, which the enforcement point asks of each apart. Changes are
//! serialized by their caller: every one comes from a dispatch, and the kernel
//! runs one call tree at a time, so the lock is held for no keeper's wait.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use nc_catalog::ErrorCode;
use nc_ids::{OpaqueId, Snowflake};

use crate::directory::{
    Delegation, Directory, DirectoryError, GrantFacts, MemoryDirectory, Principal, PrincipalKind,
    RealmFacts, Slice,
};
use crate::{EntityRef, datetime};

/// The autonomy every agent entity carries: the schema requires one, and no policy reads it.
pub const AUTONOMY: &str = "supervised";

/// The entity type a grant's scope is: a feature of the realm.
const FEATURE: &str = "Feature";

/// The entity type a grant is.
const GRANT: &str = "Grant";

/// The entity type a realm is.
const REALM: &str = "Realm";

/// Where every grant to an agent comes from: a person gave it.
const SOURCE: &str = "manual";

/// The most characters an agent's label holds.
const LABEL_MAX: usize = 64;

/// The most characters a grant's reason holds.
const REASON_MAX: usize = 256;

/// How many agents and grants a profile may keep, which whoever opens the directory reads from the catalog.
///
/// The directory is the writer that refuses past them: nothing prunes an agent
/// or a grant, and an ended grant still counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    /// The most agents.
    pub agents: u64,
    /// The most grants, ended ones included.
    pub grants: u64,
}

/// Where a directory's changes are kept, to be read back at the next start.
pub trait Keeper: fmt::Debug + Send + Sync + 'static {
    /// Keeps `kept`, whole or not at all.
    ///
    /// # Errors
    ///
    /// The [`Unkept`] that says why nothing was kept, in its store's code and
    /// in its store's words.
    fn keep(&self, kept: &Kept) -> impl Future<Output = Result<(), Unkept>> + Send;
}

/// One change, as a keeper keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kept {
    /// A new agent.
    Agent(AgentRecord),
    /// A new grant, and the live grant on its scope it ends.
    Grant {
        /// The grant.
        record: GrantRecord,
        /// The grant it ends, at its own beginning and by its own giver.
        ends: Option<Snowflake>,
    },
    /// The end of grants.
    Ended {
        /// The grants ended.
        grants: Vec<Snowflake>,
        /// When, in milliseconds since the Unix epoch.
        at_ms: u64,
        /// Who ended them: the agent's operator.
        by: OpaqueId,
    },
}

/// Why a keeper kept nothing: the store's own code, and its words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unkept {
    /// The code the store refused with.
    pub code: ErrorCode,
    /// Why, in the store's words.
    pub why: String,
}

/// What a keeper kept, which a directory is opened over.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Records {
    /// Every agent kept.
    pub agents: Vec<AgentRecord>,
    /// Every grant kept and not ended.
    pub live: Vec<GrantRecord>,
    /// How many grants are kept, ended ones included.
    pub grants_kept: u64,
}

/// An agent, as it is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRecord {
    /// Its identifier, as the machine's minter gave it.
    pub agent: Snowflake,
    /// The realm it was made in.
    pub realm: OpaqueId,
    /// The person it acts for.
    pub operator: OpaqueId,
    /// What its person calls it.
    pub label: String,
    /// What it may spend in a session.
    pub budget: u32,
}

/// A grant to an agent, as it is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantRecord {
    /// Its identifier, as the machine's minter gave it.
    pub grant: Snowflake,
    /// The agent it is given to.
    pub agent: Snowflake,
    /// The realm it was given in.
    pub realm: OpaqueId,
    /// The feature it is for.
    pub scope: EntityRef,
    /// Who gave it: the agent's operator.
    pub granted_by: OpaqueId,
    /// Why, in its giver's words.
    pub reason: String,
    /// When it begins, in milliseconds since the Unix epoch.
    pub valid_from_ms: u64,
    /// When it ends, in milliseconds since the Unix epoch.
    pub valid_until_ms: u64,
}

/// A change to the agents a directory holds, as the kernel's commands ask for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// A new agent, held for `operator` under the identifier the kernel minted.
    Make {
        /// The identifier the kernel minted for it.
        agent: Snowflake,
        /// The person it acts for: whoever asked for it.
        operator: Principal,
        /// What its person calls it.
        label: String,
        /// What it may spend in a session.
        budget: u32,
    },
    /// A grant to the agent `agent` names, which ends the live grant on its scope.
    Grant {
        /// The identifier the kernel minted for it.
        grant: Snowflake,
        /// The agent, as its caller named it: matched, never parsed.
        agent: String,
        /// The feature it is for.
        scope: EntityRef,
        /// Who gives it: whoever asked for it.
        by: Principal,
        /// Why, in its giver's words.
        reason: String,
        /// When it begins, in milliseconds since the Unix epoch.
        from_ms: u64,
        /// When it ends, in milliseconds since the Unix epoch.
        until_ms: u64,
    },
    /// The end of the agent's live grant on `scope`, or of every one it holds.
    End {
        /// The agent, as its caller named it: matched, never parsed.
        agent: String,
        /// The feature whose grant ends; every one, when none is named.
        scope: Option<EntityRef>,
        /// Who ends it: whoever asked for it.
        by: Principal,
        /// When, in milliseconds since the Unix epoch.
        at_ms: u64,
    },
}

/// What kind of refusal a change met, as whoever asked sorts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// What it names is not there.
    NotFound,
    /// Whoever asked may not make it.
    Denied,
    /// What is held already leaves no room for it.
    Conflict,
    /// It is not a change the data may hold.
    Invalid,
    /// It could not be answered for, or kept.
    Unavailable,
}

/// Why a change was not made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeRefused {
    /// No agent is held under the identifier the change names.
    NoSuchAgent,
    /// The agent's operator is not a person, or the grant is given or ended by someone other than its operator.
    NotTheOperator,
    /// The table the change would add a row to holds as many as its bound.
    Full {
        /// The table: `agents` or `agent_grants`.
        table: &'static str,
    },
    /// A change the data must never hold, and why.
    Malformed(String),
    /// The keeper did not keep it.
    Unkept(Unkept),
    /// The directory could not answer for it.
    Directory(DirectoryError),
}

impl ChangeRefused {
    /// The code a refusal of this change carries.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::NoSuchAgent => ErrorCode::AiNoSuchAgent,
            Self::NotTheOperator => ErrorCode::AiNotTheOperator,
            Self::Full { .. } => ErrorCode::AiAgentsFull,
            Self::Malformed(_) => ErrorCode::ValidationArgumentsInvalid,
            Self::Unkept(unkept) => unkept.code,
            Self::Directory(_) => ErrorCode::PolicyDirectoryUnavailable,
        }
    }

    /// What kind of refusal it is.
    #[must_use]
    pub const fn kind(&self) -> ChangeKind {
        match self {
            Self::NoSuchAgent => ChangeKind::NotFound,
            Self::NotTheOperator => ChangeKind::Denied,
            Self::Full { .. } => ChangeKind::Conflict,
            Self::Malformed(_) => ChangeKind::Invalid,
            Self::Unkept(_) | Self::Directory(_) => ChangeKind::Unavailable,
        }
    }

    /// The word a refusal of this change is recorded under.
    #[must_use]
    pub const fn word(&self) -> &'static str {
        match self {
            Self::NoSuchAgent => "no-such-agent",
            Self::NotTheOperator => "not-the-operator",
            Self::Full { .. } => "agents-full",
            Self::Malformed(_) => "arguments-invalid",
            Self::Unkept(_) => "not-kept",
            Self::Directory(_) => "directory-unavailable",
        }
    }
}

impl fmt::Display for ChangeRefused {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = self.code();
        // Debug-quoted where the words are not this crate's: a store's error.
        match self {
            Self::NoSuchAgent => write!(
                formatter,
                "[{code}] no agent is held under the identifier the change names"
            ),
            Self::NotTheOperator => write!(
                formatter,
                "[{code}] an agent is made for a person, and only that person grants it or ends a grant"
            ),
            Self::Full { table } => write!(
                formatter,
                "[{code}] the profile holds as many rows of {table:?} as its bound allows"
            ),
            Self::Malformed(why) => write!(
                formatter,
                "[{code}] the change is not one the directory may hold: {why:?}"
            ),
            Self::Unkept(unkept) => write!(
                formatter,
                "[{code}] the change was not kept: {:?}",
                unkept.why
            ),
            Self::Directory(error) => write!(formatter, "[{code}] {error}"),
        }
    }
}

impl std::error::Error for ChangeRefused {}

/// An agent a person made, and its live grants, as `agents` lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldAgent {
    /// The agent.
    pub agent: Principal,
    /// What its person calls it.
    pub label: String,
    /// What it may spend in a session.
    pub budget: u32,
    /// Its live grants, in the order they were given.
    pub grants: Vec<HeldGrant>,
}

/// A live grant to an agent, as `agents` lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldGrant {
    /// The grant.
    pub grant: OpaqueId,
    /// The feature it is for.
    pub scope: EntityRef,
    /// Why, in its giver's words.
    pub reason: String,
    /// When it begins, in milliseconds since the Unix epoch.
    pub valid_from_ms: u64,
    /// When it ends, in milliseconds since the Unix epoch.
    pub valid_until_ms: u64,
}

/// The profile's directory: the world it was started with, and the agents its person made since.
#[derive(Debug)]
pub struct LocalDirectory<K: Keeper> {
    /// The world, which answers every question about anything but a held agent.
    world: MemoryDirectory,
    /// The realm the agents are held in, minted when the world was read.
    realm: OpaqueId,
    /// How many agents and grants it may keep.
    bounds: Bounds,
    /// The agents, never held across an await; a lock a panic left behind answers unavailable.
    held: RwLock<Held>,
    /// Where each change is kept.
    keeper: K,
}

/// The agents a directory holds, and how many rows its keeper holds.
#[derive(Debug, Default)]
struct Held {
    /// Each agent, by the text of its identifier: what a question is matched against.
    agents: BTreeMap<String, Delegate>,
    /// How many agents are kept, of every realm.
    agents_kept: u64,
    /// How many grants are kept, of every realm, ended ones included.
    grants_kept: u64,
}

/// An agent held, and its live grants.
#[derive(Debug, Clone)]
struct Delegate {
    /// Its identifier, as the minter gave it.
    made: Snowflake,
    /// Its principal, minted here.
    agent: Principal,
    /// The person it acts for.
    operator: Principal,
    /// What its person calls it.
    label: String,
    /// What it may spend in a session.
    budget: u32,
    /// Its live grants, by the feature each is for.
    live: BTreeMap<EntityRef, Live>,
}

/// A live grant to an agent.
#[derive(Debug, Clone)]
struct Live {
    /// Its identifier, as the minter gave it.
    grant: Snowflake,
    /// Its identifier, as a question names its entity.
    id: OpaqueId,
    /// Who gave it.
    by: OpaqueId,
    /// Why, in its giver's words.
    reason: String,
    /// When it begins.
    from_ms: u64,
    /// When it ends.
    until_ms: u64,
}

impl Live {
    /// The grant, as a question's facts name it.
    fn facts(&self) -> GrantFacts {
        GrantFacts {
            entity: EntityRef {
                type_name: GRANT.to_owned(),
                id: self.id.as_str().to_owned(),
            },
            id: self.id.clone(),
        }
    }
}

impl Delegate {
    /// The agent and its live grants, as `agents` lists them.
    fn listed(&self) -> HeldAgent {
        let mut live: Vec<(&EntityRef, &Live)> = self.live.iter().collect();
        live.sort_by_key(|(_, live)| live.grant);
        HeldAgent {
            agent: self.agent.clone(),
            label: self.label.clone(),
            budget: self.budget,
            grants: live
                .into_iter()
                .map(|(scope, live)| HeldGrant {
                    grant: live.id.clone(),
                    scope: scope.clone(),
                    reason: live.reason.clone(),
                    valid_from_ms: live.from_ms,
                    valid_until_ms: live.until_ms,
                })
                .collect(),
        }
    }
}

/// What a change does once it is allowed, and what its keeper keeps.
struct Planned {
    /// What the held agents become.
    plan: Plan,
    /// What is kept; nothing for an ending that ends nothing.
    kept: Option<Kept>,
}

/// What a change does to the held agents.
enum Plan {
    /// Hold a new agent under the text of its identifier.
    Make {
        /// The text.
        key: String,
        /// The agent.
        delegate: Delegate,
    },
    /// Hold a grant on `scope` for the agent under `key`, in place of its live one there.
    Grant {
        /// The agent's text.
        key: String,
        /// The feature.
        scope: EntityRef,
        /// The grant.
        live: Live,
    },
    /// End the agent's live grants on these features.
    End {
        /// The agent's text.
        key: String,
        /// The features.
        scopes: Vec<EntityRef>,
    },
}

/// Whether `text` is one to `max` characters long and holds none a reader cannot see.
fn holdable(text: &str, max: usize) -> bool {
    (1..=max).contains(&text.chars().count()) && !text.chars().any(nc_schema::unseen)
}

/// A refusal of a change the data must never hold.
fn malformed(why: &str) -> ChangeRefused {
    ChangeRefused::Malformed(why.to_owned())
}

/// The identifier the minter gave, as a question names it.
fn minted(snowflake: Snowflake) -> Result<OpaqueId, DirectoryError> {
    OpaqueId::new(snowflake.to_crockford())
        .map_err(|error| DirectoryError::Malformed(error.to_string()))
}

impl<K: Keeper> LocalDirectory<K> {
    /// The directory over `world` in `realm`, holding what `kept` says was kept.
    ///
    /// Rows of another realm are left out of what it holds, and still count
    /// against its bounds: the tables are the profile's.
    ///
    /// # Errors
    ///
    /// [`DirectoryError::Malformed`] for what this directory would never have
    /// written: more rows than its bounds, an agent twice or one it would have
    /// refused, a grant of an agent `kept` does not hold, one not given by its
    /// agent's operator, on a scope that is not a feature, with no time to run
    /// or a reason it would have refused, and two live grants on one scope.
    pub fn open(
        realm: OpaqueId,
        world: MemoryDirectory,
        kept: Records,
        keeper: K,
        bounds: Bounds,
    ) -> Result<Self, DirectoryError> {
        let refused = |why: &str| DirectoryError::Malformed(format!("what was kept {why}"));
        let agents_kept = u64::try_from(kept.agents.len()).unwrap_or(u64::MAX);
        let live = u64::try_from(kept.live.len()).unwrap_or(u64::MAX);
        if agents_kept > bounds.agents || kept.grants_kept > bounds.grants {
            return Err(refused("holds more rows than its bounds"));
        }
        if live > kept.grants_kept {
            return Err(refused("counts fewer grants than it holds live"));
        }
        let mut agents: BTreeMap<String, Delegate> = BTreeMap::new();
        for record in kept
            .agents
            .into_iter()
            .filter(|record| record.realm == realm)
        {
            if !holdable(&record.label, LABEL_MAX) || record.budget == 0 {
                return Err(refused("holds an agent no change makes"));
            }
            let delegate = Delegate {
                made: record.agent,
                agent: Principal::minted(PrincipalKind::Agent, minted(record.agent)?),
                operator: Principal::minted(PrincipalKind::User, record.operator),
                label: record.label,
                budget: record.budget,
                live: BTreeMap::new(),
            };
            if agents
                .insert(record.agent.to_crockford(), delegate)
                .is_some()
            {
                return Err(refused("holds an agent twice"));
            }
        }
        for record in kept.live.into_iter().filter(|record| record.realm == realm) {
            let Some(delegate) = agents.get_mut(&record.agent.to_crockford()) else {
                return Err(refused("holds a grant of an agent it does not hold"));
            };
            if record.granted_by != *delegate.operator.id()
                || record.scope.type_name != FEATURE
                || record.valid_until_ms <= record.valid_from_ms
                || !holdable(&record.reason, REASON_MAX)
            {
                return Err(refused("holds a grant no change gives"));
            }
            let live = Live {
                grant: record.grant,
                id: minted(record.grant)?,
                by: record.granted_by,
                reason: record.reason,
                from_ms: record.valid_from_ms,
                until_ms: record.valid_until_ms,
            };
            if delegate.live.insert(record.scope, live).is_some() {
                return Err(refused("holds two live grants on one scope"));
            }
        }
        Ok(Self {
            world,
            realm,
            bounds,
            held: RwLock::new(Held {
                agents,
                agents_kept,
                grants_kept: kept.grants_kept,
            }),
            keeper,
        })
    }

    /// The held agents, to read.
    fn read(&self) -> Result<RwLockReadGuard<'_, Held>, DirectoryError> {
        self.held.read().map_err(|_| unanswerable())
    }

    /// The held agents, to change.
    fn write(&self) -> Result<RwLockWriteGuard<'_, Held>, DirectoryError> {
        self.held.write().map_err(|_| unanswerable())
    }

    /// The agent `principal` names, when it is one this directory holds in `realm`.
    ///
    /// Membership of the id's text in the held map, and nothing read out of
    /// the id itself.
    fn runtime<'held>(
        &self,
        held: &'held Held,
        realm: &str,
        principal: &EntityRef,
    ) -> Option<&'held Delegate> {
        if realm != self.realm.as_str() || principal.type_name != PrincipalKind::Agent.type_name() {
            return None;
        }
        held.agents.get(&principal.id)
    }

    /// The agent entity a question names, with its operator and its budget, as Cedar reads it.
    fn agent_entity(&self, delegate: &Delegate) -> serde_json::Value {
        serde_json::json!({
            "uid": { "type": PrincipalKind::Agent.type_name(), "id": delegate.agent.id().as_str() },
            "attrs": {
                "operator": { "__entity": {
                    "type": PrincipalKind::User.type_name(),
                    "id": delegate.operator.id().as_str(),
                } },
                "autonomy": AUTONOMY,
                "budget_remaining": delegate.budget,
            },
            "parents": [{ "type": REALM, "id": self.realm.as_str() }],
        })
    }

    /// The grant entity a question rests on, with its provenance, as Cedar reads it.
    fn grant_entity(&self, live: &Live) -> serde_json::Value {
        let moment =
            |ms: u64| serde_json::json!({ "__extn": { "fn": "datetime", "arg": datetime(ms) } });
        serde_json::json!({
            "uid": { "type": GRANT, "id": live.id.as_str() },
            "attrs": {
                "granted_by": { "__entity": {
                    "type": PrincipalKind::User.type_name(),
                    "id": live.by.as_str(),
                } },
                "reason": live.reason,
                "valid_from": moment(live.from_ms),
                "valid_until": moment(live.until_ms),
                "source": SOURCE,
            },
            "parents": [{ "type": REALM, "id": self.realm.as_str() }],
        })
    }

    /// What `change` does, and what is kept, when it may be made over `held`.
    fn checked(&self, realm: &str, held: &Held, change: &Change) -> Result<Planned, ChangeRefused> {
        self.refused(realm, held, change)?;
        self.planned(held, change)
    }

    /// Why `change` may not be made over `held`, in the order its checks are written.
    fn refused(&self, realm: &str, held: &Held, change: &Change) -> Result<(), ChangeRefused> {
        if realm != self.realm.as_str() {
            return Err(ChangeRefused::Directory(DirectoryError::UnknownRealm(
                realm.to_owned(),
            )));
        }
        match change {
            Change::Make {
                agent,
                operator,
                label,
                budget,
            } => {
                if operator.kind() != PrincipalKind::User {
                    return Err(ChangeRefused::NotTheOperator);
                }
                if held.agents.contains_key(&agent.to_crockford()) {
                    return Err(malformed("an agent is already held under its identifier"));
                }
                if !holdable(label, LABEL_MAX) || *budget == 0 {
                    return Err(malformed(
                        "an agent's label is empty, too long or unseen, or its budget is zero",
                    ));
                }
                if held.agents_kept >= self.bounds.agents {
                    return Err(ChangeRefused::Full { table: "agents" });
                }
            }
            Change::Grant {
                agent,
                scope,
                by,
                reason,
                from_ms,
                until_ms,
                ..
            } => {
                let delegate = held.agents.get(agent).ok_or(ChangeRefused::NoSuchAgent)?;
                if *by != delegate.operator {
                    return Err(ChangeRefused::NotTheOperator);
                }
                if until_ms <= from_ms {
                    return Err(malformed("a grant ends no later than it begins"));
                }
                if scope.type_name != FEATURE || !self.world.holds(realm, scope) {
                    return Err(malformed("a grant's scope is not a feature of the realm"));
                }
                if !holdable(reason, REASON_MAX) {
                    return Err(malformed("a grant's reason is empty, too long or unseen"));
                }
                if held.grants_kept >= self.bounds.grants {
                    return Err(ChangeRefused::Full {
                        table: "agent_grants",
                    });
                }
            }
            Change::End { agent, by, .. } => {
                let delegate = held.agents.get(agent).ok_or(ChangeRefused::NoSuchAgent)?;
                if *by != delegate.operator {
                    return Err(ChangeRefused::NotTheOperator);
                }
            }
        }
        Ok(())
    }

    /// What `change`, already checked over `held`, does to it, and what is kept.
    fn planned(&self, held: &Held, change: &Change) -> Result<Planned, ChangeRefused> {
        let minted = |snowflake| minted(snowflake).map_err(ChangeRefused::Directory);
        match change {
            Change::Make {
                agent,
                operator,
                label,
                budget,
            } => Ok(Planned {
                plan: Plan::Make {
                    key: agent.to_crockford(),
                    delegate: Delegate {
                        made: *agent,
                        agent: Principal::minted(PrincipalKind::Agent, minted(*agent)?),
                        operator: operator.clone(),
                        label: label.clone(),
                        budget: *budget,
                        live: BTreeMap::new(),
                    },
                },
                kept: Some(Kept::Agent(AgentRecord {
                    agent: *agent,
                    realm: self.realm.clone(),
                    operator: operator.id().clone(),
                    label: label.clone(),
                    budget: *budget,
                })),
            }),
            Change::Grant {
                grant,
                agent,
                scope,
                by,
                reason,
                from_ms,
                until_ms,
            } => {
                let delegate = held.agents.get(agent).ok_or(ChangeRefused::NoSuchAgent)?;
                let record = GrantRecord {
                    grant: *grant,
                    agent: delegate.made,
                    realm: self.realm.clone(),
                    scope: scope.clone(),
                    granted_by: by.id().clone(),
                    reason: reason.clone(),
                    valid_from_ms: *from_ms,
                    valid_until_ms: *until_ms,
                };
                let live = Live {
                    grant: *grant,
                    id: minted(*grant)?,
                    by: by.id().clone(),
                    reason: reason.clone(),
                    from_ms: *from_ms,
                    until_ms: *until_ms,
                };
                Ok(Planned {
                    plan: Plan::Grant {
                        key: agent.clone(),
                        scope: scope.clone(),
                        live,
                    },
                    kept: Some(Kept::Grant {
                        record,
                        ends: delegate.live.get(scope).map(|live| live.grant),
                    }),
                })
            }
            Change::End {
                agent,
                scope,
                by,
                at_ms,
            } => {
                let delegate = held.agents.get(agent).ok_or(ChangeRefused::NoSuchAgent)?;
                let mut ending: Vec<(&EntityRef, &Live)> = delegate
                    .live
                    .iter()
                    .filter(|(on, _)| scope.as_ref().is_none_or(|scope| *on == scope))
                    .collect();
                ending.sort_by_key(|(_, live)| live.grant);
                let kept = (!ending.is_empty()).then(|| Kept::Ended {
                    grants: ending.iter().map(|(_, live)| live.grant).collect(),
                    at_ms: *at_ms,
                    by: by.id().clone(),
                });
                Ok(Planned {
                    plan: Plan::End {
                        key: agent.clone(),
                        scopes: ending.into_iter().map(|(on, _)| on.clone()).collect(),
                    },
                    kept,
                })
            }
        }
    }
}

/// Makes `plan` of what `held` holds.
fn apply(held: &mut Held, plan: Plan) {
    match plan {
        Plan::Make { key, delegate } => {
            held.agents.insert(key, delegate);
            held.agents_kept = held.agents_kept.saturating_add(1);
        }
        Plan::Grant { key, scope, live } => {
            if let Some(delegate) = held.agents.get_mut(&key) {
                delegate.live.insert(scope, live);
            }
            held.grants_kept = held.grants_kept.saturating_add(1);
        }
        Plan::End { key, scopes } => {
            if let Some(delegate) = held.agents.get_mut(&key) {
                for scope in &scopes {
                    delegate.live.remove(scope);
                }
            }
        }
    }
}

/// The refusal of a directory whose agents a panic left half-changed.
fn unanswerable() -> DirectoryError {
    DirectoryError::Unavailable("the agents were left half-changed by a panic".to_owned())
}

impl<K: Keeper> Directory for LocalDirectory<K> {
    fn realm(
        &self,
        realm: &str,
    ) -> impl Future<Output = Result<RealmFacts, DirectoryError>> + Send {
        self.world.realm(realm)
    }

    async fn grant(
        &self,
        realm: &str,
        principal: &EntityRef,
        resource: &EntityRef,
    ) -> Result<Option<GrantFacts>, DirectoryError> {
        let held = {
            let held = self.read()?;
            // The scope compared whole, and never through an entity it is in.
            self.runtime(&held, realm, principal)
                .map(|delegate| delegate.live.get(resource).map(Live::facts))
        };
        match held {
            Some(answer) => Ok(answer),
            None => self.world.grant(realm, principal, resource).await,
        }
    }

    async fn slice(
        &self,
        realm: &str,
        principal: &EntityRef,
        resource: &EntityRef,
        grant: Option<&EntityRef>,
    ) -> Result<Slice, DirectoryError> {
        let beside = {
            let held = self.read()?;
            self.runtime(&held, realm, principal).map(|delegate| {
                let rests_on = grant.and_then(|grant| {
                    delegate
                        .live
                        .values()
                        .find(|live| grant.type_name == GRANT && grant.id == live.id.as_str())
                });
                let mut beside = vec![self.agent_entity(delegate)];
                beside.extend(rests_on.map(|live| self.grant_entity(live)));
                // A grant this agent does not hold is the world's to answer for.
                (delegate.operator.entity(), beside, rests_on.is_none())
            })
        };
        let Some((operator, beside, of_the_world)) = beside else {
            return self.world.slice(realm, principal, resource, grant).await;
        };
        let named: Vec<&EntityRef> = [
            Some(resource),
            Some(&operator),
            grant.filter(|_| of_the_world),
        ]
        .into_iter()
        .flatten()
        .collect();
        self.world.slice_beside(realm, &named, beside)
    }

    async fn delegation(
        &self,
        realm: &str,
        agent: &EntityRef,
    ) -> Result<Option<Delegation>, DirectoryError> {
        let held = {
            let held = self.read()?;
            self.runtime(&held, realm, agent)
                .map(|delegate| Delegation {
                    agent: delegate.agent.clone(),
                    operator: delegate.operator.clone(),
                })
        };
        match held {
            Some(delegation) => Ok(Some(delegation)),
            None => self.world.delegation(realm, agent).await,
        }
    }

    fn agents(
        &self,
        realm: &str,
        operator: &EntityRef,
    ) -> impl Future<Output = Result<Vec<HeldAgent>, DirectoryError>> + Send {
        let listed = self.read().map(|held| {
            let mut made: Vec<&Delegate> = held
                .agents
                .values()
                .filter(|delegate| {
                    realm == self.realm.as_str() && delegate.operator.entity() == *operator
                })
                .collect();
            made.sort_by_key(|delegate| delegate.made);
            made.into_iter().map(Delegate::listed).collect()
        });
        std::future::ready(listed)
    }

    fn rehearse(
        &self,
        realm: &str,
        change: &Change,
    ) -> impl Future<Output = Result<(), ChangeRefused>> + Send {
        let checked = self
            .read()
            .map_err(ChangeRefused::Directory)
            .and_then(|held| self.checked(realm, &held, change).map(|_| ()));
        std::future::ready(checked)
    }

    async fn change(&self, realm: &str, change: &Change) -> Result<(), ChangeRefused> {
        if let Change::End { .. } = change {
            // **Ended in memory first**: a person who ends a grant wants the
            // agent stopped now, and the next question is answered without it
            // even when the keeper then refuses the ending.
            let kept = {
                let mut held = self.write().map_err(ChangeRefused::Directory)?;
                let Planned { plan, kept } = self.checked(realm, &held, change)?;
                apply(&mut held, plan);
                kept
            };
            return match kept {
                Some(kept) => self.keeper.keep(&kept).await.map_err(ChangeRefused::Unkept),
                None => Ok(()),
            };
        }
        let Planned { plan, kept } = {
            let held = self.read().map_err(ChangeRefused::Directory)?;
            self.checked(realm, &held, change)?
        };
        // **Kept before it is held**: an agent or a grant the keeper refused
        // is answered nowhere, so a start reads back what was.
        if let Some(kept) = &kept {
            self.keeper
                .keep(kept)
                .await
                .map_err(ChangeRefused::Unkept)?;
        }
        let mut held = self.write().map_err(ChangeRefused::Directory)?;
        apply(&mut held, plan);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
    use std::sync::{Arc, Mutex};

    use nc_catalog::ErrorCode;
    use nc_ids::{MachineFlake, Minter, OpaqueId, Snowflake};

    use super::{
        AUTONOMY, AgentRecord, Bounds, Change, ChangeKind, ChangeRefused, GrantRecord, HeldAgent,
        Keeper, Kept, LocalDirectory, Records, Unkept,
    };
    use crate::{
        Directory as _, DirectoryError, EntityRef, Facts, MemoryDirectory, Pdp, Principal,
        PrincipalKind, Question, sentinel,
    };

    /// The entities every policy case is about.
    const WORLD: &str = include_str!("../../../policy/world.json");

    /// `2026-08-26T00:00:00.000Z`: the moment the pack's cases call now.
    const NOW: u64 = 1_787_702_400_000;

    /// An hour, in milliseconds.
    const HOUR: u64 = 3_600_000;

    /// Bounds no test here reaches.
    const ROOMY: Bounds = Bounds {
        agents: 256,
        grants: 65_536,
    };

    /// A clock that stands at [`NOW`].
    #[derive(Debug)]
    struct Standing;

    impl nc_ids::Clock for Standing {
        fn now_ms(&self) -> u64 {
            NOW
        }

        fn wait_past(&self, _millis: u64) {}
    }

    /// A keeper that records each change it keeps, and refuses each once a test says so.
    #[derive(Debug, Clone, Default)]
    struct Recording {
        kept: Arc<Mutex<Vec<Kept>>>,
        refuses: Arc<AtomicBool>,
    }

    impl Recording {
        /// What it kept, in order.
        fn kept(&self) -> Vec<Kept> {
            self.kept.lock().expect("nothing panics holding it").clone()
        }

        /// Refuses every change from now on, or keeps them again.
        fn refusing(&self, refuses: bool) {
            self.refuses.store(refuses, Ordering::SeqCst);
        }
    }

    impl Keeper for Recording {
        fn keep(&self, kept: &Kept) -> impl Future<Output = Result<(), Unkept>> + Send {
            let answer = if self.refuses.load(Ordering::SeqCst) {
                Err(Unkept {
                    code: ErrorCode::DbUnreachable,
                    why: "the disk is full, as the test made it".to_owned(),
                })
            } else {
                self.kept
                    .lock()
                    .expect("nothing panics holding it")
                    .push(kept.clone());
                Ok(())
            };
            std::future::ready(answer)
        }
    }

    /// An entity of type `type_name` called `id`.
    fn entity(type_name: &str, id: &str) -> EntityRef {
        EntityRef {
            type_name: type_name.to_owned(),
            id: id.to_owned(),
        }
    }

    /// The family pack this build carries.
    fn family() -> Pdp {
        Pdp::embedded("family").expect("the family pack is carried")
    }

    /// The world, with `home` at epoch seven and `g.current` granted to `ana`.
    fn world(pdp: &Pdp) -> MemoryDirectory {
        MemoryDirectory::from_world(WORLD, pdp)
            .expect("the world loads")
            .with_epoch("home", 7)
            .with_grant("home", entity("User", "ana"), "g.current")
            .expect("home holds g.current")
    }

    /// A directory over the world in `home`, keeping into `keeper`, within `bounds`.
    fn local(pdp: &Pdp, keeper: Recording, bounds: Bounds) -> LocalDirectory<Recording> {
        let world = world(pdp);
        let home = world.realm_id("home").expect("home is the world's");
        LocalDirectory::open(home, world, Records::default(), keeper, bounds)
            .expect("nothing kept opens")
    }

    /// The person or principal the world holds of `kind` under `id`.
    fn principal(pdp: &Pdp, kind: PrincipalKind, id: &str) -> Principal {
        world(pdp)
            .principal(kind, id)
            .expect("the world holds the principal")
    }

    /// `count` identifiers, as the machine's minter hands them out, none of them another call's.
    fn mint(count: usize) -> Vec<Snowflake> {
        /// The flake the next call mints with: each call's own.
        static FLAKES: AtomicU16 = AtomicU16::new(1);
        let flake = FLAKES.fetch_add(1, Ordering::SeqCst) % 1_000 + 1;
        let mut minter = Minter::new(MachineFlake::from_value(flake).expect("a flake"), Standing);
        (0..count)
            .map(|_| minter.mint().expect("the minter mints"))
            .collect()
    }

    /// Makes an agent of `operator` under `agent`, as the kernel asks for one.
    async fn make(
        directory: &LocalDirectory<Recording>,
        agent: Snowflake,
        operator: &Principal,
    ) -> Result<(), ChangeRefused> {
        directory
            .change(
                "home",
                &Change::Make {
                    agent,
                    operator: operator.clone(),
                    label: "assistant".to_owned(),
                    budget: 1_000,
                },
            )
            .await
    }

    /// A grant of `scope` for an hour from now to the agent `agent` names, given by `by`.
    fn granting(grant: Snowflake, agent: Snowflake, scope: &str, by: &Principal) -> Change {
        Change::Grant {
            grant,
            agent: agent.to_crockford(),
            scope: entity("Feature", scope),
            by: by.clone(),
            reason: "to name the household's files".to_owned(),
            from_ms: NOW,
            until_ms: NOW + HOUR,
        }
    }

    /// The end, by `by`, of the agent's grant on `scope`, or of every one.
    fn ending(agent: Snowflake, scope: Option<&str>, by: &Principal) -> Change {
        Change::End {
            agent: agent.to_crockford(),
            scope: scope.map(|scope| entity("Feature", scope)),
            by: by.clone(),
            at_ms: NOW + 1,
        }
    }

    /// The agent's entity, as a question names it.
    fn agent(snowflake: Snowflake) -> EntityRef {
        entity("Agent", &snowflake.to_crockford())
    }

    /// What the kernel vouches for an agent acting openly, resting on `grant`, having spent `spent`.
    fn acting(grant: Option<EntityRef>, spent: u64) -> Facts {
        Facts {
            now_ms: NOW,
            via_agent: true,
            confirmed_by_human: false,
            actor_chain_depth: 1,
            policy_epoch: 7,
            quorum_met: false,
            grant,
            budget_spent: spent,
        }
    }

    /// Asks `pdp` about `principal` reading `resource` in `home`, as the kernel asks.
    async fn decided(
        pdp: &Pdp,
        directory: &impl crate::Directory,
        principal: &EntityRef,
        resource: &EntityRef,
        spent: u64,
    ) -> crate::Decision {
        let grant = directory
            .grant("home", principal, resource)
            .await
            .expect("the directory answers")
            .map(|grant| grant.entity);
        let slice = directory
            .slice("home", principal, resource, grant.as_ref())
            .await
            .expect("the directory answers");
        pdp.decide(
            &Question {
                principal,
                action: "Read",
                resource,
                facts: &acting(grant, spent),
            },
            &slice,
        )
    }

    #[tokio::test]
    async fn an_agent_is_held_under_the_identifier_the_minter_gave_and_under_no_text() {
        let pdp = family();
        let directory = local(&pdp, Recording::default(), ROOMY);
        let ana = principal(&pdp, PrincipalKind::User, "ana");
        let [made, _] = mint(2)[..] else {
            unreachable!("two were minted")
        };
        make(&directory, made, &ana).await.expect("made");
        let held = directory
            .delegation("home", &agent(made))
            .await
            .expect("answered")
            .expect("held under the minted identifier");
        assert_eq!(held.agent.kind(), PrincipalKind::Agent);
        assert_eq!(held.agent.id().as_str(), made.to_crockford());
        assert_eq!(held.operator, ana);
        let text = made.to_crockford();
        for unheld in [
            entity("Agent", "assistant"),
            entity("Agent", "ana"),
            entity("Agent", ""),
            entity("Agent", &text.to_lowercase()),
            entity("Agent", &format!("{text} ")),
            entity("Agent", &format!("0{text}")),
            entity("User", &text),
        ] {
            assert_eq!(
                directory.delegation("home", &unheld).await,
                Ok(None),
                "{unheld:?}"
            );
        }
        // The world's own agent is the world's, and never the one held.
        let assistant = directory
            .delegation("home", &entity("Agent", "ana-assistant"))
            .await
            .expect("answered")
            .expect("the world's agent");
        assert_eq!(assistant.agent.id().as_str(), "ana-assistant");
        assert_ne!(assistant.agent, held.agent);
    }

    #[tokio::test]
    async fn a_question_about_an_agent_is_routed_by_the_text_it_was_held_under_and_nothing_parsed_from_it()
     {
        let pdp = family();
        let directory = local(&pdp, Recording::default(), ROOMY);
        let ana = principal(&pdp, PrincipalKind::User, "ana");
        let [made, grant, unmade] = mint(3)[..] else {
            unreachable!("three were minted")
        };
        make(&directory, made, &ana).await.expect("made");
        directory
            .change("home", &granting(grant, made, "naming", &ana))
            .await
            .expect("granted");
        let naming = entity("Feature", "naming");
        assert!(
            directory
                .grant("home", &agent(made), &naming)
                .await
                .expect("answered")
                .is_some(),
            "the text it was held under is answered by what is held"
        );
        // A value no change made, of the same shape, and a spelling that reads
        // back to the held value, both reach the world, which holds neither.
        let lowered = entity("Agent", &made.to_crockford().to_lowercase());
        for unheld in [agent(unmade), lowered] {
            assert_eq!(
                directory.grant("home", &unheld, &naming).await,
                Ok(None),
                "{unheld:?}"
            );
            assert_eq!(directory.delegation("home", &unheld).await, Ok(None));
            let slice = directory
                .slice("home", &unheld, &naming, None)
                .await
                .expect("answered");
            assert!(!slice.holds(&unheld), "{unheld:?}");
        }
        // Asked in a realm it was not made in, the held agent is the world's.
        assert_eq!(directory.delegation("office", &agent(made)).await, Ok(None));
        // A world agent is the world's, whatever its id's shape.
        let assistant = entity("Agent", "ana-assistant");
        let granted = world(&pdp)
            .with_grant("home", assistant.clone(), "g.current")
            .expect("home holds g.current");
        let over = LocalDirectory::open(
            granted.realm_id("home").expect("home"),
            granted,
            Records::default(),
            Recording::default(),
            ROOMY,
        )
        .expect("opens");
        let held = over
            .grant("home", &assistant, &entity("Content", "photo"))
            .await
            .expect("answered")
            .expect("the world's grant, realm-wide");
        assert_eq!(held.id.as_str(), "g.current");
        assert_eq!(
            over.delegation("home", &assistant)
                .await
                .expect("answered")
                .map(|delegation| delegation.operator),
            Some(ana)
        );
    }

    #[tokio::test]
    async fn an_agent_is_delegated_only_for_the_person_who_made_it() {
        let pdp = family();
        let directory = local(&pdp, Recording::default(), ROOMY);
        let (ana, carla) = (
            principal(&pdp, PrincipalKind::User, "ana"),
            principal(&pdp, PrincipalKind::User, "carla"),
        );
        let [anas, carlas] = mint(2)[..] else {
            unreachable!("two were minted")
        };
        make(&directory, anas, &ana).await.expect("made");
        make(&directory, carlas, &carla).await.expect("made");
        let operator = |delegation: Option<crate::Delegation>| delegation.expect("held").operator;
        assert_eq!(
            operator(
                directory
                    .delegation("home", &agent(anas))
                    .await
                    .expect("answered")
            ),
            ana
        );
        // Another person's agent answers its own operator, which is not the
        // asker: whoever binds a session compares the two.
        assert_eq!(
            operator(
                directory
                    .delegation("home", &agent(carlas))
                    .await
                    .expect("answered")
            ),
            carla
        );
        let listed = |agents: Vec<HeldAgent>| -> Vec<String> {
            agents
                .into_iter()
                .map(|held| held.agent.id().as_str().to_owned())
                .collect()
        };
        assert_eq!(
            listed(
                directory
                    .agents("home", &ana.entity())
                    .await
                    .expect("listed")
            ),
            [anas.to_crockford()]
        );
        assert_eq!(
            listed(
                directory
                    .agents("home", &carla.entity())
                    .await
                    .expect("listed")
            ),
            [carlas.to_crockford()]
        );
        assert_eq!(
            directory.agents("office", &ana.entity()).await,
            Ok(Vec::new())
        );
    }

    #[tokio::test]
    async fn only_a_person_makes_an_agent() {
        let pdp = family();
        let keeper = Recording::default();
        let directory = local(&pdp, keeper.clone(), ROOMY);
        for operator in [
            principal(&pdp, PrincipalKind::Device, "ana-laptop"),
            principal(&pdp, PrincipalKind::Agent, "ana-assistant"),
        ] {
            let [made] = mint(1)[..] else {
                unreachable!("one was minted")
            };
            assert_eq!(
                make(&directory, made, &operator).await,
                Err(ChangeRefused::NotTheOperator),
                "{operator:?}"
            );
            assert_eq!(directory.delegation("home", &agent(made)).await, Ok(None));
            assert_eq!(
                directory.agents("home", &operator.entity()).await,
                Ok(Vec::new())
            );
        }
        assert_eq!(keeper.kept(), Vec::new(), "nothing was kept");
    }

    #[tokio::test]
    async fn an_agents_grant_answers_for_the_one_resource_it_names() {
        let pdp = family();
        let directory = local(&pdp, Recording::default(), ROOMY);
        let ana = principal(&pdp, PrincipalKind::User, "ana");
        let [made, grant] = mint(2)[..] else {
            unreachable!("two were minted")
        };
        make(&directory, made, &ana).await.expect("made");
        directory
            .change("home", &granting(grant, made, "naming", &ana))
            .await
            .expect("granted");
        let answered = directory
            .grant("home", &agent(made), &entity("Feature", "naming"))
            .await
            .expect("answered")
            .expect("granted naming");
        assert_eq!(answered.id.as_str(), grant.to_crockford());
        assert_eq!(answered.entity, entity("Grant", &grant.to_crockford()));
        // Not a feature beside it, not the realm the feature is in, and not
        // the realm's content: the scope is compared whole.
        for elsewhere in [
            entity("Feature", "registry"),
            entity("Realm", "home"),
            entity("Content", "notice"),
        ] {
            assert_eq!(
                directory.grant("home", &agent(made), &elsewhere).await,
                Ok(None),
                "{elsewhere:?}"
            );
        }
        // A person's grant is the world's, and realm-wide.
        for anything in [
            entity("Feature", "naming"),
            entity("Feature", "registry"),
            entity("Realm", "home"),
            entity("Content", "notice"),
        ] {
            let held = directory
                .grant("home", &ana.entity(), &anything)
                .await
                .expect("answered")
                .expect("ana's grant");
            assert_eq!(held.id.as_str(), "g.current", "{anything:?}");
        }
    }

    #[tokio::test]
    async fn an_agents_grant_is_its_operators_to_give_and_to_end() {
        let pdp = family();
        let keeper = Recording::default();
        let directory = local(&pdp, keeper.clone(), ROOMY);
        let (ana, carla) = (
            principal(&pdp, PrincipalKind::User, "ana"),
            principal(&pdp, PrincipalKind::User, "carla"),
        );
        let [made, given, taken] = mint(3)[..] else {
            unreachable!("three were minted")
        };
        make(&directory, made, &ana).await.expect("made");
        assert_eq!(
            directory
                .change("home", &granting(taken, made, "naming", &carla))
                .await,
            Err(ChangeRefused::NotTheOperator)
        );
        directory
            .change("home", &granting(given, made, "naming", &ana))
            .await
            .expect("granted by its operator");
        assert_eq!(
            directory
                .change("home", &ending(made, Some("naming"), &carla))
                .await,
            Err(ChangeRefused::NotTheOperator)
        );
        assert_eq!(
            directory.change("home", &ending(made, None, &carla)).await,
            Err(ChangeRefused::NotTheOperator)
        );
        let held = directory
            .grant("home", &agent(made), &entity("Feature", "naming"))
            .await
            .expect("answered")
            .expect("still granted");
        assert_eq!(held.id.as_str(), given.to_crockford());
        assert_eq!(keeper.kept().len(), 2, "the agent and its operator's grant");
    }

    #[tokio::test]
    async fn a_grant_with_no_time_to_run_or_on_no_feature_of_the_realm_is_refused_when_it_is_made()
    {
        let pdp = family();
        let keeper = Recording::default();
        let directory = local(&pdp, keeper.clone(), ROOMY);
        let ana = principal(&pdp, PrincipalKind::User, "ana");
        let [made, grant] = mint(2)[..] else {
            unreachable!("two were minted")
        };
        make(&directory, made, &ana).await.expect("made");
        let regranted = |until_ms: u64, scope: EntityRef, reason: String| Change::Grant {
            grant,
            agent: made.to_crockford(),
            scope,
            by: ana.clone(),
            reason,
            from_ms: NOW,
            until_ms,
        };
        let (naming, why) = (entity("Feature", "naming"), || "a reason".to_owned());
        let refused = [
            (
                "it ends as it begins",
                regranted(NOW, naming.clone(), why()),
            ),
            (
                "it ends before it begins",
                regranted(NOW - 1, naming.clone(), why()),
            ),
            (
                "a feature the world does not hold",
                regranted(NOW + HOUR, entity("Feature", "nowhere"), why()),
            ),
            (
                "the realm's content",
                regranted(NOW + HOUR, entity("Content", "notice"), why()),
            ),
            (
                "the realm itself",
                regranted(NOW + HOUR, entity("Realm", "home"), why()),
            ),
            (
                "no reason",
                regranted(NOW + HOUR, naming.clone(), String::new()),
            ),
            (
                "a reason past its bound",
                regranted(NOW + HOUR, naming.clone(), "a".repeat(257)),
            ),
            (
                "a reason a reader cannot see",
                regranted(NOW + HOUR, naming.clone(), "\u{200b}".to_owned()),
            ),
        ];
        for (what, change) in refused {
            assert!(
                matches!(
                    directory.rehearse("home", &change).await,
                    Err(ChangeRefused::Malformed(_))
                ),
                "{what}: rehearsed"
            );
            assert!(
                matches!(
                    directory.change("home", &change).await,
                    Err(ChangeRefused::Malformed(_))
                ),
                "{what}"
            );
        }
        assert_eq!(
            directory.grant("home", &agent(made), &naming).await,
            Ok(None)
        );
        assert_eq!(keeper.kept().len(), 1, "the agent alone was kept");
        // The control: a minute's grant whose reason is at its bound is held.
        let at_the_bound = regranted(NOW + 1, naming.clone(), "a".repeat(256));
        assert_eq!(directory.rehearse("home", &at_the_bound).await, Ok(()));
        assert_eq!(directory.change("home", &at_the_bound).await, Ok(()));
    }

    #[tokio::test]
    async fn an_agent_with_no_label_no_budget_or_an_identifier_already_held_is_refused_when_it_is_made()
     {
        let pdp = family();
        let keeper = Recording::default();
        let directory = local(&pdp, keeper.clone(), ROOMY);
        let ana = principal(&pdp, PrincipalKind::User, "ana");
        let [made, other] = mint(2)[..] else {
            unreachable!("two were minted")
        };
        make(&directory, made, &ana).await.expect("made");
        let making = |label: &str, budget: u32| Change::Make {
            agent: other,
            operator: ana.clone(),
            label: label.to_owned(),
            budget,
        };
        let refused = [
            ("no label", making("", 10)),
            ("a label past its bound", making(&"a".repeat(65), 10)),
            ("a label a reader cannot see", making("\u{200b}", 10)),
            ("no budget", making("assistant", 0)),
            (
                "an identifier already held",
                Change::Make {
                    agent: made,
                    operator: ana.clone(),
                    label: "again".to_owned(),
                    budget: 10,
                },
            ),
        ];
        for (what, change) in refused {
            assert!(
                matches!(
                    directory.change("home", &change).await,
                    Err(ChangeRefused::Malformed(_))
                ),
                "{what}"
            );
        }
        assert_eq!(keeper.kept().len(), 1, "the first agent alone was kept");
        let listed = directory
            .agents("home", &ana.entity())
            .await
            .expect("listed");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].label, "assistant");
        assert_eq!(
            directory.change("home", &making(&"a".repeat(64), 1)).await,
            Ok(()),
            "the control: a label at its bound and a budget of one"
        );
    }

    #[tokio::test]
    async fn a_new_grant_on_a_scope_ends_the_one_it_replaces() {
        let pdp = family();
        let keeper = Recording::default();
        let directory = local(&pdp, keeper.clone(), ROOMY);
        let ana = principal(&pdp, PrincipalKind::User, "ana");
        let [made, first, beside, second] = mint(4)[..] else {
            unreachable!("four were minted")
        };
        make(&directory, made, &ana).await.expect("made");
        for (grant, scope) in [(first, "naming"), (beside, "registry"), (second, "naming")] {
            directory
                .change("home", &granting(grant, made, scope, &ana))
                .await
                .expect("granted");
        }
        let ends: Vec<Option<Snowflake>> = keeper
            .kept()
            .into_iter()
            .filter_map(|kept| match kept {
                Kept::Grant { ends, .. } => Some(ends),
                _ => None,
            })
            .collect();
        assert_eq!(ends, [None, None, Some(first)]);
        let listed = directory
            .agents("home", &ana.entity())
            .await
            .expect("listed");
        let grants: Vec<(String, String)> = listed[0]
            .grants
            .iter()
            .map(|held| (held.scope.id.clone(), held.grant.as_str().to_owned()))
            .collect();
        assert_eq!(
            grants,
            [
                ("registry".to_owned(), beside.to_crockford()),
                ("naming".to_owned(), second.to_crockford()),
            ]
        );
        let held = directory
            .grant("home", &agent(made), &entity("Feature", "naming"))
            .await
            .expect("answered")
            .expect("granted");
        assert_eq!(held.id.as_str(), second.to_crockford());
    }

    #[tokio::test]
    async fn an_ended_grant_is_not_answered_at_the_next_question() {
        let pdp = family();
        let keeper = Recording::default();
        let directory = local(&pdp, keeper.clone(), ROOMY);
        let ana = principal(&pdp, PrincipalKind::User, "ana");
        let [made, naming, registry, gallery] = mint(4)[..] else {
            unreachable!("four were minted")
        };
        make(&directory, made, &ana).await.expect("made");
        for (grant, scope) in [
            (naming, "naming"),
            (registry, "registry"),
            (gallery, "gallery"),
        ] {
            directory
                .change("home", &granting(grant, made, scope, &ana))
                .await
                .expect("granted");
        }
        let granted = |scope: &'static str| {
            let directory = &directory;
            async move {
                directory
                    .grant("home", &agent(made), &entity("Feature", scope))
                    .await
                    .expect("answered")
                    .is_some()
            }
        };
        directory
            .change("home", &ending(made, Some("naming"), &ana))
            .await
            .expect("ended");
        assert!(!granted("naming").await, "ended at the next question");
        assert!(granted("registry").await, "the scope it names alone");
        assert!(granted("gallery").await);
        directory
            .change("home", &ending(made, None, &ana))
            .await
            .expect("ended");
        assert!(!granted("registry").await);
        assert!(!granted("gallery").await);
        assert_eq!(
            directory
                .agents("home", &ana.entity())
                .await
                .expect("listed")[0]
                .grants,
            Vec::new()
        );
        // Ending what is not live ends nothing, and keeps nothing.
        let before = keeper.kept().len();
        directory
            .change("home", &ending(made, None, &ana))
            .await
            .expect("nothing to end");
        assert_eq!(keeper.kept().len(), before);
        let endings: Vec<Kept> = keeper
            .kept()
            .into_iter()
            .filter(|kept| matches!(kept, Kept::Ended { .. }))
            .collect();
        assert_eq!(
            endings,
            [
                Kept::Ended {
                    grants: vec![naming],
                    at_ms: NOW + 1,
                    by: ana.id().clone(),
                },
                Kept::Ended {
                    grants: vec![registry, gallery],
                    at_ms: NOW + 1,
                    by: ana.id().clone(),
                },
            ]
        );
    }

    #[tokio::test]
    async fn the_directory_refuses_an_agent_and_a_grant_past_its_bounds() {
        let pdp = family();
        let keeper = Recording::default();
        let tight = Bounds {
            agents: 2,
            grants: 2,
        };
        let directory = local(&pdp, keeper.clone(), tight);
        let ana = principal(&pdp, PrincipalKind::User, "ana");
        let [first, second, third, one, two, three] = mint(6)[..] else {
            unreachable!("six were minted")
        };
        make(&directory, first, &ana).await.expect("the first");
        make(&directory, second, &ana).await.expect("the second");
        assert_eq!(
            make(&directory, third, &ana).await,
            Err(ChangeRefused::Full { table: "agents" })
        );
        directory
            .change("home", &granting(one, first, "naming", &ana))
            .await
            .expect("the first");
        // An ended grant is kept, and still counts.
        directory
            .change("home", &ending(first, None, &ana))
            .await
            .expect("ended");
        directory
            .change("home", &granting(two, first, "naming", &ana))
            .await
            .expect("the second");
        assert_eq!(
            directory
                .change("home", &granting(three, first, "registry", &ana))
                .await,
            Err(ChangeRefused::Full {
                table: "agent_grants"
            })
        );
        assert_eq!(directory.delegation("home", &agent(third)).await, Ok(None));
        assert_eq!(
            directory
                .grant("home", &agent(first), &entity("Feature", "registry"))
                .await,
            Ok(None)
        );
    }

    #[test]
    fn each_refusal_of_a_change_is_answered_with_its_own_kind_code_and_word() {
        let unkept = Unkept {
            code: ErrorCode::DbUnreachable,
            why: "down".to_owned(),
        };
        let answered = [
            (
                ChangeRefused::NoSuchAgent,
                ChangeKind::NotFound,
                ErrorCode::AiNoSuchAgent,
                "no-such-agent",
            ),
            (
                ChangeRefused::NotTheOperator,
                ChangeKind::Denied,
                ErrorCode::AiNotTheOperator,
                "not-the-operator",
            ),
            (
                ChangeRefused::Full { table: "agents" },
                ChangeKind::Conflict,
                ErrorCode::AiAgentsFull,
                "agents-full",
            ),
            (
                ChangeRefused::Malformed("why".to_owned()),
                ChangeKind::Invalid,
                ErrorCode::ValidationArgumentsInvalid,
                "arguments-invalid",
            ),
            (
                ChangeRefused::Unkept(unkept),
                ChangeKind::Unavailable,
                ErrorCode::DbUnreachable,
                "not-kept",
            ),
            (
                ChangeRefused::Directory(DirectoryError::Unavailable("down".to_owned())),
                ChangeKind::Unavailable,
                ErrorCode::PolicyDirectoryUnavailable,
                "directory-unavailable",
            ),
        ];
        for (refused, kind, code, word) in answered {
            assert_eq!(refused.kind(), kind, "{refused:?}");
            assert_eq!(refused.code(), code, "{refused:?}");
            assert_eq!(refused.word(), word, "{refused:?}");
            assert!(
                refused.to_string().starts_with(&format!("[{code}] ")),
                "{refused}"
            );
        }
    }

    #[test]
    fn a_refused_change_prints_the_words_it_did_not_write_escaped() {
        let hostile = "\u{1b}[2J";
        for refused in [
            ChangeRefused::Malformed(hostile.to_owned()),
            ChangeRefused::Unkept(Unkept {
                code: ErrorCode::DbUnreachable,
                why: hostile.to_owned(),
            }),
            ChangeRefused::Directory(DirectoryError::Unavailable(hostile.to_owned())),
        ] {
            let printed = refused.to_string();
            assert!(!printed.contains('\u{1b}'), "escaped: {printed}");
            assert!(
                printed.contains("\\u{1b}[2J"),
                "written, not dropped: {printed}"
            );
        }
    }

    #[tokio::test]
    async fn a_policy_reading_an_agents_operator_decides_in_the_kernel_as_in_the_gate() {
        // A rule no pack carries, reading through the agent to its operator.
        const READS_THE_OPERATOR: (&str, &str) = (
            "policy/packs/family/operators.cedar",
            "@id(\"family.an_agent_of_a_suspended_account_acts_on_nothing\")\n\
             forbid (principal is Agent, action, resource)\n\
             when { principal.operator.status == \"suspended\" };\n",
        );
        let pack = &crate::embedded::CARRIED[0];
        let mut files: Vec<(&str, &str)> = crate::embedded::CORE.to_vec();
        files.extend_from_slice(pack.files);
        files.push(READS_THE_OPERATOR);
        let pdp = Pdp::from_sources_for_tests(
            crate::embedded::SCHEMA.1,
            &files,
            pack.profile,
            pack.manifest.1,
        )
        .expect("the pack compiles");
        let directory = local(&pdp, Recording::default(), ROOMY);
        let naming = entity("Feature", "naming");
        let [anas, doras, one, two] = mint(4)[..] else {
            unreachable!("four were minted")
        };
        for (made, grant, operator) in [(anas, one, "ana"), (doras, two, "dora")] {
            let operator = principal(&pdp, PrincipalKind::User, operator);
            make(&directory, made, &operator).await.expect("made");
            directory
                .change("home", &granting(grant, made, "naming", &operator))
                .await
                .expect("granted");
        }
        assert_eq!(
            decided(&pdp, &directory, &agent(doras), &naming, 0)
                .await
                .deciding,
            "family.an_agent_of_a_suspended_account_acts_on_nothing",
            "the rule reads the operator the slice holds"
        );
        assert_eq!(
            decided(&pdp, &directory, &agent(anas), &naming, 0)
                .await
                .deciding,
            sentinel::NO_PERMIT_MATCHED,
            "and decides without an error where it does not fire"
        );
        // The control: the world's agent, its operator left out of the slice,
        // makes the same rule err.
        let assistant = entity("Agent", "ana-assistant");
        let without = world(&pdp)
            .slice_now("home", &[&assistant, &naming])
            .expect("a slice");
        let erred = pdp.decide(
            &Question {
                principal: &assistant,
                action: "Read",
                resource: &naming,
                facts: &acting(None, 0),
            },
            &without,
        );
        assert_eq!(
            erred.deciding,
            "kernel.evaluation_error:family.an_agent_of_a_suspended_account_acts_on_nothing"
        );
    }

    #[tokio::test]
    async fn a_runtime_agents_budget_is_the_one_it_was_made_with() {
        let pdp = family();
        let directory = local(&pdp, Recording::default(), ROOMY);
        let ana = principal(&pdp, PrincipalKind::User, "ana");
        let [made, grant] = mint(2)[..] else {
            unreachable!("two were minted")
        };
        directory
            .change(
                "home",
                &Change::Make {
                    agent: made,
                    operator: ana.clone(),
                    label: "assistant".to_owned(),
                    budget: 3,
                },
            )
            .await
            .expect("made");
        directory
            .change("home", &granting(grant, made, "naming", &ana))
            .await
            .expect("granted");
        let naming = entity("Feature", "naming");
        // No permit names an agent on the feature, so what decides is which
        // rule refuses first: none while budget remains, the budget's after.
        assert_eq!(
            decided(&pdp, &directory, &agent(made), &naming, 2)
                .await
                .deciding,
            sentinel::NO_PERMIT_MATCHED
        );
        assert_eq!(
            decided(&pdp, &directory, &agent(made), &naming, 3)
                .await
                .deciding,
            "core.budget_is_a_permission"
        );
    }

    #[tokio::test]
    async fn a_change_is_held_once_it_is_kept_and_an_ending_before() {
        let pdp = family();
        let keeper = Recording::default();
        let directory = local(&pdp, keeper.clone(), ROOMY);
        let ana = principal(&pdp, PrincipalKind::User, "ana");
        let [unkept, made, refused, granted] = mint(4)[..] else {
            unreachable!("four were minted")
        };
        keeper.refusing(true);
        assert!(matches!(
            make(&directory, unkept, &ana).await,
            Err(ChangeRefused::Unkept(Unkept {
                code: ErrorCode::DbUnreachable,
                ..
            }))
        ));
        assert_eq!(directory.delegation("home", &agent(unkept)).await, Ok(None));
        keeper.refusing(false);
        make(&directory, made, &ana).await.expect("made");
        keeper.refusing(true);
        let naming = entity("Feature", "naming");
        assert!(matches!(
            directory
                .change("home", &granting(refused, made, "naming", &ana))
                .await,
            Err(ChangeRefused::Unkept(_))
        ));
        assert_eq!(
            directory.grant("home", &agent(made), &naming).await,
            Ok(None),
            "a grant not kept is not held"
        );
        keeper.refusing(false);
        directory
            .change("home", &granting(granted, made, "naming", &ana))
            .await
            .expect("granted");
        keeper.refusing(true);
        assert!(matches!(
            directory.change("home", &ending(made, None, &ana)).await,
            Err(ChangeRefused::Unkept(_))
        ));
        assert_eq!(
            directory.grant("home", &agent(made), &naming).await,
            Ok(None),
            "an ending not kept is held all the same"
        );
        assert_eq!(keeper.kept().len(), 2, "the agent and the grant kept");
    }

    /// What a directory's keeper kept, read back as a store hands it to the next start.
    fn read_back(kept: &[Kept]) -> Records {
        let mut records = Records::default();
        for each in kept {
            match each {
                Kept::Agent(agent) => records.agents.push(agent.clone()),
                Kept::Grant { record, ends } => {
                    records.live.retain(|live| Some(live.grant) != *ends);
                    records.live.push(record.clone());
                    records.grants_kept += 1;
                }
                Kept::Ended { grants, .. } => {
                    records.live.retain(|live| !grants.contains(&live.grant));
                }
            }
        }
        records
    }

    /// A directory in which `ana` and `carla` each made an agent and granted and ended grants.
    ///
    /// Answers the directory, what its keeper kept as a store reads it back
    /// with an agent of `office` beside it, and the three agents.
    async fn kept_by_two(pdp: &Pdp) -> (LocalDirectory<Recording>, Records, [Snowflake; 3]) {
        let keeper = Recording::default();
        let directory = local(pdp, keeper.clone(), ROOMY);
        let (ana, carla) = (
            principal(pdp, PrincipalKind::User, "ana"),
            principal(pdp, PrincipalKind::User, "carla"),
        );
        let [anas, carlas, one, two, three, elsewhere] = mint(6)[..] else {
            unreachable!("six were minted")
        };
        make(&directory, anas, &ana).await.expect("made");
        make(&directory, carlas, &carla).await.expect("made");
        for change in [
            granting(one, anas, "naming", &ana),
            granting(two, anas, "naming", &ana),
            granting(three, carlas, "registry", &carla),
            ending(carlas, None, &carla),
        ] {
            directory.change("home", &change).await.expect("changed");
        }
        let mut kept = read_back(&keeper.kept());
        // A row of another realm is the profile's, and not this realm's.
        kept.agents.push(AgentRecord {
            agent: elsewhere,
            realm: OpaqueId::new("office").expect("an identifier"),
            operator: OpaqueId::new("mallory").expect("an identifier"),
            label: "elsewhere".to_owned(),
            budget: 5,
        });
        (directory, kept, [anas, carlas, elsewhere])
    }

    /// A way what was kept is not what this directory writes.
    type Shape = fn(&mut Records, &GrantRecord);

    #[tokio::test]
    async fn a_directory_opened_from_what_was_kept_answers_as_the_one_that_kept_it() {
        let pdp = family();
        let (directory, kept, [anas, carlas, elsewhere]) = kept_by_two(&pdp).await;
        let home = world(&pdp).realm_id("home").expect("home");
        let reopened = LocalDirectory::open(
            home.clone(),
            world(&pdp),
            kept.clone(),
            Recording::default(),
            ROOMY,
        )
        .expect("what was kept opens");
        for person in ["ana", "carla"] {
            let person = entity("User", person);
            assert_eq!(
                reopened.agents("home", &person).await,
                directory.agents("home", &person).await,
                "{person:?}"
            );
        }
        for (made, scope) in [(anas, "naming"), (carlas, "registry")] {
            let scope = entity("Feature", scope);
            assert_eq!(
                reopened.grant("home", &agent(made), &scope).await,
                directory.grant("home", &agent(made), &scope).await
            );
            assert_eq!(
                reopened.delegation("home", &agent(made)).await,
                directory.delegation("home", &agent(made)).await
            );
        }
        for realm in ["home", "office"] {
            assert_eq!(
                reopened.delegation(realm, &agent(elsewhere)).await,
                Ok(None),
                "{realm}"
            );
        }
        // What this directory would never have written is refused.
        let live = kept.live[0].clone();
        let shapes: [(&str, Shape); 3] = [
            ("a grant of an agent it does not hold", |records, live| {
                let mut orphan = live.clone();
                orphan.agent = orphan.grant;
                records.live.push(orphan);
                records.grants_kept += 1;
            }),
            ("two live grants on one scope", |records, live| {
                let mut twice = live.clone();
                twice.grant = live.agent;
                records.live.push(twice);
                records.grants_kept += 1;
            }),
            ("a scope that is not a feature", |records, live| {
                records.live[0].scope = EntityRef {
                    type_name: "Content".to_owned(),
                    id: live.scope.id.clone(),
                };
            }),
        ];
        for (what, shape) in shapes {
            let mut malformed = kept.clone();
            shape(&mut malformed, &live);
            assert!(
                matches!(
                    LocalDirectory::open(
                        home.clone(),
                        world(&pdp),
                        malformed,
                        Recording::default(),
                        ROOMY
                    ),
                    Err(DirectoryError::Malformed(_))
                ),
                "{what}"
            );
        }
    }

    #[tokio::test]
    async fn a_directory_opened_from_what_was_kept_counts_every_row_against_its_bounds() {
        let pdp = family();
        let (_, kept, [anas, ..]) = kept_by_two(&pdp).await;
        let home = world(&pdp).realm_id("home").expect("home");
        // Three agents, the other realm's included, and three grants, the
        // ended ones included: at its bounds, and past them by one.
        let at = Bounds {
            agents: 3,
            grants: 3,
        };
        let full = LocalDirectory::open(
            home.clone(),
            world(&pdp),
            kept.clone(),
            Recording::default(),
            at,
        )
        .expect("at its bounds");
        let ana = principal(&pdp, PrincipalKind::User, "ana");
        let [fourth, four] = mint(2)[..] else {
            unreachable!("two were minted")
        };
        assert_eq!(
            make(&full, fourth, &ana).await,
            Err(ChangeRefused::Full { table: "agents" })
        );
        assert_eq!(
            full.change("home", &granting(four, anas, "registry", &ana))
                .await,
            Err(ChangeRefused::Full {
                table: "agent_grants"
            })
        );
        for under in [
            Bounds {
                agents: 2,
                grants: 3,
            },
            Bounds {
                agents: 3,
                grants: 2,
            },
        ] {
            assert!(
                matches!(
                    LocalDirectory::open(
                        home.clone(),
                        world(&pdp),
                        kept.clone(),
                        Recording::default(),
                        under
                    ),
                    Err(DirectoryError::Malformed(_))
                ),
                "{under:?}"
            );
        }
    }

    #[test]
    fn no_embedded_policy_reads_an_agents_autonomy() {
        let pack = &crate::embedded::CARRIED;
        for (path, text) in crate::embedded::CORE
            .iter()
            .chain(pack.iter().flat_map(|pack| pack.files.iter()))
        {
            assert!(
                !text.contains("autonomy"),
                "{path} reads an agent's autonomy"
            );
        }
    }

    #[tokio::test]
    async fn every_slice_holds_the_agents_operator_beside_the_agent() {
        let pdp = family();
        let directory = local(&pdp, Recording::default(), ROOMY);
        let ana = principal(&pdp, PrincipalKind::User, "ana");
        let [made, grant] = mint(2)[..] else {
            unreachable!("two were minted")
        };
        make(&directory, made, &ana).await.expect("made");
        directory
            .change("home", &granting(grant, made, "naming", &ana))
            .await
            .expect("granted");
        let naming = entity("Feature", "naming");
        let rests_on = entity("Grant", &grant.to_crockford());
        let runtime = directory
            .slice("home", &agent(made), &naming, Some(&rests_on))
            .await
            .expect("answered");
        for held in [
            agent(made),
            ana.entity(),
            naming.clone(),
            rests_on.clone(),
            entity("Realm", "home"),
            entity("Group", "role.guardian"),
        ] {
            assert!(runtime.holds(&held), "{held:?}");
        }
        let uid = crate::uid("Agent", &made.to_crockford()).expect("a uid");
        let held = runtime.entities().get(&uid).expect("the agent");
        assert_eq!(
            held.attr("autonomy").and_then(Result::ok),
            Some(cedar_policy::EvalResult::String(AUTONOMY.to_owned()))
        );
        // A grant the agent does not hold is the world's to answer for.
        let resting = directory
            .slice(
                "home",
                &agent(made),
                &naming,
                Some(&entity("Grant", "g.current")),
            )
            .await
            .expect("answered");
        assert!(resting.holds(&entity("Grant", "g.current")));
        assert!(!resting.holds(&rests_on));
        let assistant = entity("Agent", "ana-assistant");
        let worlds = directory
            .slice("home", &assistant, &naming, None)
            .await
            .expect("answered");
        assert!(worlds.holds(&assistant));
        assert!(worlds.holds(&ana.entity()), "the world agent's operator");
        let person = directory
            .slice("home", &ana.entity(), &naming, None)
            .await
            .expect("answered");
        assert!(!person.holds(&entity("User", "bruno")));
    }

    #[tokio::test]
    async fn a_directory_built_from_a_world_changes_nothing() {
        let pdp = family();
        let world = world(&pdp);
        let ana = principal(&pdp, PrincipalKind::User, "ana");
        let [made] = mint(1)[..] else {
            unreachable!("one was minted")
        };
        let change = Change::Make {
            agent: made,
            operator: ana.clone(),
            label: "assistant".to_owned(),
            budget: 10,
        };
        let refused = |answer: Result<(), ChangeRefused>| {
            assert!(
                matches!(
                    answer,
                    Err(ChangeRefused::Directory(DirectoryError::Unavailable(_)))
                ),
                "{answer:?}"
            );
        };
        refused(world.rehearse("home", &change).await);
        refused(world.change("home", &change).await);
        assert_eq!(world.agents("home", &ana.entity()).await, Ok(Vec::new()));
        assert_eq!(world.delegation("home", &agent(made)).await, Ok(None));
    }

    #[tokio::test]
    async fn a_change_asked_of_another_realm_is_not_made() {
        let pdp = family();
        let keeper = Recording::default();
        let directory = local(&pdp, keeper.clone(), ROOMY);
        let ana = principal(&pdp, PrincipalKind::User, "ana");
        let [made] = mint(1)[..] else {
            unreachable!("one was minted")
        };
        let change = Change::Make {
            agent: made,
            operator: ana.clone(),
            label: "assistant".to_owned(),
            budget: 10,
        };
        assert_eq!(
            directory.change("office", &change).await,
            Err(ChangeRefused::Directory(DirectoryError::UnknownRealm(
                "office".to_owned()
            )))
        );
        assert_eq!(keeper.kept(), Vec::new());
        assert_eq!(directory.delegation("home", &agent(made)).await, Ok(None));
    }
}
