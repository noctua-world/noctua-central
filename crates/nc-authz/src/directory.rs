//! What the decision point asks about the world: realms, grants, entities and agents.
//!
//! A [`Directory`] answers seven questions for the enforcement point, four of
//! them about agents. The three every decision asks are what a realm selects
//! and pins, which grant a principal holds there for the resource a question
//! names, and which entities a question needs. The four about agents are which
//! person an agent acts for, which agents a person made, and whether a change to
//! them would be made, and making it. It answers them for one realm at a time.
//! That is how realm isolation is structural (`NOCTUA.md §10.3`, `§14.3`): a
//! kernel serves one realm, the directory hands out only that realm's entities
//! and grants, and a principal from another realm arrives with no grant and no
//! entity, which the core refuses and [`crate::Pdp::decide`] denies.
//!
//! [`MemoryDirectory`] is the world of `policy/world.json`, the grants and
//! epochs a test gives it, and nothing else: a grant it holds is realm-wide,
//! the agents it answers for are the world's own, and it changes nothing. The
//! directory over the grants table is Stage 22's. A profile's `world.json` is read by
//! [`LocalWorld::parse`] into the realm and the person a daemon decides as, the
//! grant that person holds and the realm's epoch, and that directory over the
//! rest. [`crate::LocalDirectory`] layers the agents a profile's person makes
//! over it (`NOCTUA.md §14.6`).
//!
//! # What it answers (`§2.4`)
//!
//! **A slice is built with the schema**, so an entity it hands out has the
//! attributes the policies were validated against, one the schema refuses
//! never reaches Cedar, and every action arrives with the groups it is in:
//! action entities come only from the schema, and without them a `forbid` on
//! `action in` a group never fires. **A principal is minted here**, only for an entity the
//! world holds under an identifier `nc_ids::OpaqueId` accepts, so the kernel
//! receives principals typed and never mints one — it reaches the central log,
//! and the log's wall refuses minting there.
//!
//! **An agent's operator is in every slice beside it**, so a policy reading
//! `principal.operator` decides in the kernel as the gate decides it over the
//! whole world, where a slice holding the agent alone would make it err.
//!
//! **What it does not answer** is whether a grant or an epoch is current:
//! this one holds what it was given, and a directory that falls behind the
//! grants table is what `§14.8`'s epoch is for. Nor whether an agent's rights
//! are its operator's: the enforcement point asks for each of them apart.

use std::collections::{BTreeMap, HashSet};

use cedar_policy::{Entities, Entity, EntityUid, EvalResult, Schema};
use nc_ids::OpaqueId;

use crate::agents::{Change, ChangeRefused, HeldAgent};
use crate::{EntityRef, Pdp, uid};

/// The entity type a realm is.
const REALM: &str = "Realm";

/// The entity type a grant is.
const GRANT: &str = "Grant";

/// What a realm selects and pins, and the epoch its authorization state is at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealmFacts {
    /// The profile that selects a pack: `family`.
    pub profile: String,
    /// The pack the realm pins: `family@1`.
    pub pinned_pack: String,
    /// The realm's policy epoch; zero until one is set, which the core refuses.
    pub epoch: u64,
}

/// A grant a principal holds in a realm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantFacts {
    /// The `Grant` entity, as a question's facts name it.
    pub entity: EntityRef,
    /// Its identifier, as the journal records it.
    pub id: OpaqueId,
}

/// The kind of principal a question can name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrincipalKind {
    /// A person.
    User,
    /// An installed local backend (`NOCTUA.md §14.5`).
    Device,
    /// An AI agent, with an identity of its own (`§14.6`).
    Agent,
}

impl PrincipalKind {
    /// The Cedar entity type this kind of principal is.
    #[must_use]
    pub const fn type_name(&self) -> &str {
        match self {
            Self::User => "User",
            Self::Device => "Device",
            Self::Agent => "Agent",
        }
    }
}

/// A principal a directory vouched for: its kind and its identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Principal {
    /// What kind it is.
    kind: PrincipalKind,
    /// Its identifier.
    id: OpaqueId,
}

impl Principal {
    /// The principal of `kind` under `id`, minted in this crate and nowhere else.
    pub(crate) const fn minted(kind: PrincipalKind, id: OpaqueId) -> Self {
        Self { kind, id }
    }

    /// What kind of principal it is.
    #[must_use]
    pub const fn kind(&self) -> PrincipalKind {
        self.kind
    }

    /// Its identifier.
    #[must_use]
    pub const fn id(&self) -> &OpaqueId {
        &self.id
    }

    /// The entity a question names it by.
    #[must_use]
    pub fn entity(&self) -> EntityRef {
        EntityRef {
            type_name: self.kind.type_name().to_owned(),
            id: self.id.as_str().to_owned(),
        }
    }
}

/// An agent the directory holds and the person it acts for, both minted here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delegation {
    /// The agent, as a question names it.
    pub agent: Principal,
    /// Its operator: a `User` of the realm.
    pub operator: Principal,
}

/// Why a directory did not answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectoryError {
    /// The directory could not be reached, or did not answer in time.
    Unavailable(String),
    /// The directory holds no realm of this identifier.
    UnknownRealm(String),
    /// What the directory was built from, or asked to hold, is refused.
    Malformed(String),
}

impl core::fmt::Display for DirectoryError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Debug-quoted: each is text from outside, a realm id or a world.
        match self {
            Self::Unavailable(why) => write!(formatter, "the directory did not answer: {why:?}"),
            Self::UnknownRealm(realm) => {
                write!(formatter, "the directory holds no realm {realm:?}")
            }
            Self::Malformed(why) => write!(
                formatter,
                "the directory refused what it was given: {why:?}"
            ),
        }
    }
}

impl std::error::Error for DirectoryError {}

/// Where the decision point learns about realms, grants, entities and agents.
///
/// Asynchronous because the directory behind a running kernel reads a
/// database, and not `dyn`-compatible for the reason `nc_journal::Store` gives:
/// `impl Future` buys no boxed allocation per call, and a kernel is generic
/// over the directory it runs with. Every answer is about one realm, and an
/// `Err` is a refusal to decide: the enforcement point denies on one, and
/// never reads it as "no". **No question has a default answer**, so a
/// directory wrapping another that forgot one does not compile, where a
/// default would answer for its agents silently.
pub trait Directory: Send + Sync + 'static {
    /// What `realm` selects and pins, and its epoch.
    ///
    /// # Errors
    ///
    /// Returns [`DirectoryError::UnknownRealm`] for a realm the directory does
    /// not hold, and [`DirectoryError::Unavailable`] when it cannot answer.
    fn realm(&self, realm: &str)
    -> impl Future<Output = Result<RealmFacts, DirectoryError>> + Send;

    /// The grant `principal` holds in `realm` for a question about `resource`, if any.
    ///
    /// A person's grant is realm-wide, whatever the resource. An agent's is
    /// the live one whose scope is `resource` itself, compared whole: never
    /// one on an entity `resource` is in, so an agent granted a feature holds
    /// nothing for the content of the realm.
    ///
    /// # Errors
    ///
    /// Returns [`DirectoryError::Unavailable`] when the directory cannot
    /// answer.
    fn grant(
        &self,
        realm: &str,
        principal: &EntityRef,
        resource: &EntityRef,
    ) -> impl Future<Output = Result<Option<GrantFacts>, DirectoryError>> + Send;

    /// The entities a question about `principal`, `resource` and `grant` needs.
    ///
    /// Only what `realm` holds: an entity of another realm, or of none, is
    /// left out, and [`crate::Pdp::decide`] denies a question naming it. An
    /// agent's slice holds its operator beside it.
    ///
    /// # Errors
    ///
    /// Returns [`DirectoryError::Unavailable`] when the directory cannot answer,
    /// and [`DirectoryError::Malformed`] when what it holds is refused by the
    /// schema.
    fn slice(
        &self,
        realm: &str,
        principal: &EntityRef,
        resource: &EntityRef,
        grant: Option<&EntityRef>,
    ) -> impl Future<Output = Result<Slice, DirectoryError>> + Send;

    /// The agent `agent` names and the person it acts for, when the realm holds both.
    ///
    /// None for an entity that is not an agent of `realm`, and for one whose
    /// operator is not a person of it. Both are minted here, from what the
    /// directory holds, and never from the text `agent` carries.
    ///
    /// # Errors
    ///
    /// Returns [`DirectoryError::Unavailable`] when the directory cannot
    /// answer.
    fn delegation(
        &self,
        realm: &str,
        agent: &EntityRef,
    ) -> impl Future<Output = Result<Option<Delegation>, DirectoryError>> + Send;

    /// The agents `operator` made in `realm`, with their live grants, in the order they were made.
    ///
    /// A world's agents are never listed: they were not made by a person.
    ///
    /// # Errors
    ///
    /// Returns [`DirectoryError::Unavailable`] when the directory cannot
    /// answer.
    fn agents(
        &self,
        realm: &str,
        operator: &EntityRef,
    ) -> impl Future<Output = Result<Vec<HeldAgent>, DirectoryError>> + Send;

    /// Whether `change` would be made now, changing nothing.
    ///
    /// # Errors
    ///
    /// The [`ChangeRefused`] the change itself would be refused with, when
    /// it is asked for now.
    fn rehearse(
        &self,
        realm: &str,
        change: &Change,
    ) -> impl Future<Output = Result<(), ChangeRefused>> + Send;

    /// Makes `change`: kept before it is held, except an ending, which is held first.
    ///
    /// # Errors
    ///
    /// Each [`ChangeRefused`]: a change this directory does not make, one
    /// the data it holds must never hold, and one its keeper did not keep.
    fn change(
        &self,
        realm: &str,
        change: &Change,
    ) -> impl Future<Output = Result<(), ChangeRefused>> + Send;
}

/// The entities one question is decided over, built with the schema.
#[derive(Debug, Clone)]
pub struct Slice(Entities);

impl Slice {
    /// Whether the slice holds `entity`.
    #[must_use]
    pub fn holds(&self, entity: &EntityRef) -> bool {
        uid(&entity.type_name, &entity.id).is_some_and(|uid| self.0.get(&uid).is_some())
    }

    /// The entities, for Cedar's authorizer.
    pub(crate) const fn entities(&self) -> &Entities {
        &self.0
    }

    /// The same slice with `entity` in place of the one it held under its uid, loaded with `schema`.
    ///
    /// Nothing when the schema refuses the entity, which the decision point
    /// then denies.
    pub(crate) fn replacing(&self, entity: Entity, schema: &Schema) -> Option<Self> {
        self.0
            .clone()
            .upsert_entities([entity], Some(schema))
            .ok()
            .map(Self)
    }
}

/// A directory over a world held in memory, with the grants and epochs it is given.
#[derive(Debug)]
pub struct MemoryDirectory {
    /// The schema every slice is built with: the decision point's own.
    schema: Schema,
    /// Every entity of the world, loaded with that schema.
    world: Entities,
    /// The grant each principal holds, by realm.
    grants: BTreeMap<(String, EntityRef), GrantFacts>,
    /// Each realm's epoch.
    epochs: BTreeMap<String, u64>,
}

impl MemoryDirectory {
    /// A directory over `world`, Cedar's JSON entity list, loaded with the decision point's schema.
    ///
    /// It holds no grant and every realm's epoch is zero until
    /// [`Self::with_grant`] and [`Self::with_epoch`] say otherwise, so a
    /// directory nobody configured denies everything.
    ///
    /// # Errors
    ///
    /// Returns [`DirectoryError::Malformed`] for a world the schema does not
    /// load.
    pub fn from_world(world: &str, pdp: &Pdp) -> Result<Self, DirectoryError> {
        let schema = pdp.compiled.schema().clone();
        let world = Entities::from_json_str(world, Some(&schema))
            .map_err(|error| DirectoryError::Malformed(error.to_string()))?;
        Ok(Self {
            schema,
            world,
            grants: BTreeMap::new(),
            epochs: BTreeMap::new(),
        })
    }

    /// The same directory, with `principal` holding the grant `grant_id` in `realm`.
    ///
    /// # Errors
    ///
    /// Returns [`DirectoryError::Malformed`] for a grant identifier
    /// `nc_ids::OpaqueId` refuses, and for a grant the world does not hold in
    /// that realm — a test naming a grant that is not there would be denied for
    /// a reason it never meant to test.
    pub fn with_grant(
        mut self,
        realm: &str,
        principal: EntityRef,
        grant_id: &str,
    ) -> Result<Self, DirectoryError> {
        let id = OpaqueId::new(grant_id)
            .map_err(|error| DirectoryError::Malformed(error.to_string()))?;
        let entity = EntityRef {
            type_name: GRANT.to_owned(),
            id: grant_id.to_owned(),
        };
        let held = uid(GRANT, grant_id)
            .zip(uid(REALM, realm))
            .is_some_and(|(grant, realm)| self.holds_in(&grant, &realm));
        if !held {
            return Err(DirectoryError::Malformed(format!(
                "the realm {realm:?} holds no grant {grant_id:?}"
            )));
        }
        self.grants
            .insert((realm.to_owned(), principal), GrantFacts { entity, id });
        Ok(self)
    }

    /// The same directory, with `realm` at `epoch`.
    #[must_use]
    pub fn with_epoch(mut self, realm: &str, epoch: u64) -> Self {
        self.epochs.insert(realm.to_owned(), epoch);
        self
    }

    /// The principal of `kind` called `id`, when the world holds one and `id` is an identifier.
    #[must_use]
    pub fn principal(&self, kind: PrincipalKind, id: &str) -> Option<Principal> {
        let minted = OpaqueId::new(id).ok()?;
        self.world.get(&uid(kind.type_name(), id)?)?;
        Some(Principal { kind, id: minted })
    }

    /// The realm called `id`, when the world holds one and `id` is an identifier.
    #[must_use]
    pub fn realm_id(&self, id: &str) -> Option<OpaqueId> {
        let minted = OpaqueId::new(id).ok()?;
        self.world.get(&uid(REALM, id)?)?;
        Some(minted)
    }

    /// Whether the world holds `entity` inside `realm`: the realm itself, or one of its descendants.
    fn holds_in(&self, entity: &EntityUid, realm: &EntityUid) -> bool {
        self.world.get(entity).is_some()
            && (entity == realm || self.world.is_ancestor_of(realm, entity))
    }

    /// What `realm` selects and pins, read from the world.
    fn realm_now(&self, realm: &str) -> Result<RealmFacts, DirectoryError> {
        let facts = uid(REALM, realm)
            .and_then(|uid| self.world.get(&uid))
            .and_then(|entity| {
                Some(RealmFacts {
                    profile: text_of(entity, "profile")?,
                    pinned_pack: text_of(entity, "policy_pack")?,
                    epoch: self.epochs.get(realm).copied().unwrap_or(0),
                })
            });
        facts.ok_or_else(|| DirectoryError::UnknownRealm(realm.to_owned()))
    }

    /// Whether the world holds `entity` inside `realm`, as a question names both.
    pub(crate) fn holds(&self, realm: &str, entity: &EntityRef) -> bool {
        uid(&entity.type_name, &entity.id)
            .zip(uid(REALM, realm))
            .is_some_and(|(entity, realm)| self.holds_in(&entity, &realm))
    }

    /// The entity a world agent's `operator` names, read from the world and never from `agent`'s text.
    ///
    /// None for an entity that is not an agent the world holds, and for one
    /// whose operator is not an entity.
    fn operator_named(&self, agent: &EntityRef) -> Option<EntityRef> {
        if agent.type_name != PrincipalKind::Agent.type_name() {
            return None;
        }
        let held = self.world.get(&uid(&agent.type_name, &agent.id)?)?;
        let Some(Ok(EvalResult::EntityUid(operator))) = held.attr("operator") else {
            return None;
        };
        Some(EntityRef {
            type_name: operator.type_name().to_string(),
            id: operator.id().unescaped().to_owned(),
        })
    }

    /// The agent the world holds in `realm` under `agent`, and the person of the realm its entity names.
    fn delegation_now(&self, realm: &str, agent: &EntityRef) -> Option<Delegation> {
        let operator = self.operator_named(agent)?;
        if operator.type_name != PrincipalKind::User.type_name()
            || !self.holds(realm, agent)
            || !self.holds(realm, &operator)
        {
            return None;
        }
        Some(Delegation {
            agent: self.principal(PrincipalKind::Agent, &agent.id)?,
            operator: self.principal(PrincipalKind::User, &operator.id)?,
        })
    }

    /// The entities named and their ancestors that `realm` holds, and `beside` as well.
    ///
    /// `beside` is Cedar's JSON for entities the world does not hold — an
    /// agent a person made, and its grant — loaded with the same schema, so one
    /// the schema refuses is refused here as the world's would be, and one
    /// already in the slice is refused rather than either of the two chosen.
    pub(crate) fn slice_beside(
        &self,
        realm: &str,
        named: &[&EntityRef],
        beside: Vec<serde_json::Value>,
    ) -> Result<Slice, DirectoryError> {
        let malformed = |error: cedar_policy::entities_errors::EntitiesError| {
            DirectoryError::Malformed(error.to_string())
        };
        let entities = beside
            .into_iter()
            .map(|entity| Entity::from_json_value(entity, Some(&self.schema)).map_err(malformed))
            .collect::<Result<Vec<Entity>, DirectoryError>>()?;
        let Slice(held) = self.slice_now(realm, named)?;
        held.add_entities(entities, Some(&self.schema))
            .map(Slice)
            .map_err(malformed)
    }

    /// The entities named, and each one's ancestors, that `realm` holds.
    pub(crate) fn slice_now(
        &self,
        realm: &str,
        named: &[&EntityRef],
    ) -> Result<Slice, DirectoryError> {
        let mut taken: HashSet<EntityUid> = HashSet::new();
        let mut entities: Vec<Entity> = Vec::new();
        let realm = uid(REALM, realm);
        for entity in named {
            let Some(uid) = uid(&entity.type_name, &entity.id) else {
                continue;
            };
            let ancestors = self.world.ancestors(&uid).into_iter().flatten();
            for each in std::iter::once(&uid).chain(ancestors) {
                if realm
                    .as_ref()
                    .is_some_and(|realm| self.holds_in(each, realm))
                    && let Some(found) = self.world.get(each)
                    && taken.insert(each.clone())
                {
                    entities.push(found.clone());
                }
            }
        }
        Entities::from_entities(entities, Some(&self.schema))
            .map(Slice)
            .map_err(|error| DirectoryError::Malformed(error.to_string()))
    }
}

/// What a profile decides as: one realm, the person every dispatch is decided
/// for, the grant that person holds there, the realm's epoch, and the world of
/// entities around them (`NOCTUA.md §5.13`).
///
/// **One canonical document**, the profile's `world.json`:
/// `{"epoch":…,"grant":…,"person":…,"realm":…,"world":[…]}`, the world
/// written as Cedar's JSON entity list and loaded with the decision point's
/// schema, and `grant` the one key it may leave out. The realm and the person
/// are minted here, as every principal is, and only for entities the world
/// holds — the person inside that realm — so a profile cannot name a principal
/// it has no entity for, and the daemon, which reaches the central log, mints
/// neither. The grant is one of the world's `Grant` entities inside that realm,
/// held by the person as [`MemoryDirectory::with_grant`] holds one, and the
/// epoch a whole number above zero, as [`MemoryDirectory::with_epoch`] sets
/// it: until the directory over the grants table (Stage 22), the world file is
/// where a profile's grant comes from, read at each start, and a world naming
/// none is a person who holds none, every dispatch of whom is denied.
#[derive(Debug)]
pub struct LocalWorld {
    realm: OpaqueId,
    person: Principal,
    directory: MemoryDirectory,
}

/// The keys a world file holds, and no other; every one but `grant` is
/// required.
const WORLD_KEYS: [&str; 5] = ["epoch", "grant", "person", "realm", "world"];

impl LocalWorld {
    /// The world `text` says, read as the decision point `pdp` reads entities.
    ///
    /// # Errors
    ///
    /// [`DirectoryError::Malformed`] for text that is not one canonical JSON
    /// object holding the keys a world file holds and nothing else, a realm or
    /// a person that is not text, an epoch that is not a whole number above
    /// zero, a grant that is not text, a world that is not a list or that the
    /// schema does not load, a realm the world does not hold, a person it does
    /// not hold inside that realm, and a grant it does not hold inside it.
    pub fn parse(text: &str, pdp: &Pdp) -> Result<Self, DirectoryError> {
        let malformed = |why: String| DirectoryError::Malformed(why);
        let value = nc_canon::parse(text)
            .map_err(|error| malformed(format!("a world file is not JSON this reads: {error}")))?;
        // **Written one way**: the installer writes it canonically, so a file
        // in any other spelling is one something else wrote.
        if value.to_canonical() != text {
            return Err(malformed(
                "a world file is not written canonically, as the installer writes one".to_owned(),
            ));
        }
        let nc_canon::Value::Object(fields) = &value else {
            return Err(malformed("a world file is not one object".to_owned()));
        };
        if let Some(key) = fields
            .keys()
            .find(|key| !WORLD_KEYS.contains(&key.as_str()))
        {
            return Err(malformed(format!(
                "a world file holds the key {key:?}, which it does not read"
            )));
        }
        let named = |key: &str| match fields.get(key) {
            Some(nc_canon::Value::Text(name)) => Ok(name.as_str()),
            _ => Err(malformed(format!(
                "a world file's {key:?} is missing or not text"
            ))),
        };
        let (realm, person) = (named("realm")?, named("person")?);
        // **Above zero**, because the core refuses every decision at an epoch
        // of zero (`core.fail_closed`): a world at zero would name a grant
        // nothing could use.
        let epoch = match fields.get("epoch") {
            Some(nc_canon::Value::Integer(epoch)) => {
                u64::try_from(*epoch).ok().filter(|epoch| *epoch > 0)
            }
            _ => None,
        }
        .ok_or_else(|| {
            malformed(
                "a world file's \"epoch\" is missing or not a whole number above zero".to_owned(),
            )
        })?;
        let grant = match fields.get("grant") {
            None => None,
            Some(nc_canon::Value::Text(grant)) => Some(grant.as_str()),
            Some(_) => return Err(malformed("a world file's \"grant\" is not text".to_owned())),
        };
        let Some(world @ nc_canon::Value::Array(_)) = fields.get("world") else {
            return Err(malformed(
                "a world file's \"world\" is missing or not a list of entities".to_owned(),
            ));
        };
        let directory =
            MemoryDirectory::from_world(&world.to_canonical(), pdp)?.with_epoch(realm, epoch);
        let minted = directory
            .realm_id(realm)
            .ok_or_else(|| malformed(format!("the world holds no realm {realm:?}")))?;
        let inside = uid(PrincipalKind::User.type_name(), person)
            .zip(uid(REALM, realm))
            .is_some_and(|(user, realm)| directory.holds_in(&user, &realm));
        let principal = directory
            .principal(PrincipalKind::User, person)
            .filter(|_| inside)
            .ok_or_else(|| {
                malformed(format!(
                    "the world holds no person {person:?} inside the realm {realm:?}"
                ))
            })?;
        let directory = match grant {
            Some(grant) => directory.with_grant(realm, principal.entity(), grant)?,
            None => directory,
        };
        Ok(Self {
            realm: minted,
            person: principal,
            directory,
        })
    }

    /// The realm every dispatch is decided in.
    #[must_use]
    pub const fn realm(&self) -> &OpaqueId {
        &self.realm
    }

    /// The person every dispatch is decided as.
    #[must_use]
    pub const fn person(&self) -> &Principal {
        &self.person
    }

    /// The directory over the world's entities, which a kernel is opened with.
    #[must_use]
    pub fn into_directory(self) -> MemoryDirectory {
        self.directory
    }
}

/// A string attribute of an entity, or nothing when it has none of that name and type.
fn text_of(entity: &Entity, attribute: &str) -> Option<String> {
    match entity.attr(attribute) {
        Some(Ok(EvalResult::String(text))) => Some(text),
        _ => None,
    }
}

impl Directory for MemoryDirectory {
    fn realm(
        &self,
        realm: &str,
    ) -> impl Future<Output = Result<RealmFacts, DirectoryError>> + Send {
        std::future::ready(self.realm_now(realm))
    }

    // A world's grant is its principal's in the realm, whatever the resource.
    fn grant(
        &self,
        realm: &str,
        principal: &EntityRef,
        _resource: &EntityRef,
    ) -> impl Future<Output = Result<Option<GrantFacts>, DirectoryError>> + Send {
        std::future::ready(Ok(self
            .grants
            .get(&(realm.to_owned(), principal.clone()))
            .cloned()))
    }

    // A world agent's operator joins the slice here and not in `slice_now`,
    // which every slice is built by.
    fn slice(
        &self,
        realm: &str,
        principal: &EntityRef,
        resource: &EntityRef,
        grant: Option<&EntityRef>,
    ) -> impl Future<Output = Result<Slice, DirectoryError>> + Send {
        let operator = self.operator_named(principal);
        let named: Vec<&EntityRef> = [Some(principal), Some(resource), grant, operator.as_ref()]
            .into_iter()
            .flatten()
            .collect();
        std::future::ready(self.slice_now(realm, &named))
    }

    fn delegation(
        &self,
        realm: &str,
        agent: &EntityRef,
    ) -> impl Future<Output = Result<Option<Delegation>, DirectoryError>> + Send {
        std::future::ready(Ok(self.delegation_now(realm, agent)))
    }

    // A world's agents were written into it, and no person made them.
    fn agents(
        &self,
        _realm: &str,
        _operator: &EntityRef,
    ) -> impl Future<Output = Result<Vec<HeldAgent>, DirectoryError>> + Send {
        std::future::ready(Ok(Vec::new()))
    }

    fn rehearse(
        &self,
        _realm: &str,
        _change: &Change,
    ) -> impl Future<Output = Result<(), ChangeRefused>> + Send {
        std::future::ready(Err(holds_what_it_was_given()))
    }

    fn change(
        &self,
        _realm: &str,
        _change: &Change,
    ) -> impl Future<Output = Result<(), ChangeRefused>> + Send {
        std::future::ready(Err(holds_what_it_was_given()))
    }
}

/// The refusal of every change asked of a world: it holds what it was given.
fn holds_what_it_was_given() -> ChangeRefused {
    ChangeRefused::Directory(DirectoryError::Unavailable(
        "a directory built from a world holds what it was given".to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{Directory as _, DirectoryError, LocalWorld, MemoryDirectory, PrincipalKind};

    /// The entities every policy case is about, which a development profile's
    /// world is written from.
    const WORLD: &str = include_str!("../../../policy/world.json");

    /// A world file as the installer writes one, canonical.
    ///
    /// It holds the realm, the person, the epoch the kernel's suites give the
    /// realm `home`, a grant when one is named, and the entities.
    fn granted(realm: &str, person: &str, grant: Option<&str>) -> String {
        let world = nc_canon::parse(WORLD).expect("the policy world is JSON");
        let mut fields = std::collections::BTreeMap::new();
        fields.insert("realm".to_owned(), nc_canon::Value::Text(realm.to_owned()));
        fields.insert(
            "person".to_owned(),
            nc_canon::Value::Text(person.to_owned()),
        );
        fields.insert("epoch".to_owned(), nc_canon::Value::Integer(7));
        if let Some(grant) = grant {
            fields.insert("grant".to_owned(), nc_canon::Value::text(grant));
        }
        fields.insert("world".to_owned(), world);
        nc_canon::Value::Object(fields).to_canonical()
    }

    /// A world file naming no grant.
    fn written(realm: &str, person: &str) -> String {
        granted(realm, person, None)
    }

    fn refused(text: &str) -> String {
        let pdp = crate::Pdp::embedded("family").expect("the family pack is carried");
        match LocalWorld::parse(text, &pdp) {
            Err(DirectoryError::Malformed(why)) => why,
            other => panic!("a world file was not refused as malformed: {other:?}"),
        }
    }

    /// A world file names its realm and its person, each minted for an entity
    /// the world holds, and the directory over the rest.
    #[test]
    fn a_world_file_names_the_realm_and_the_person_it_decides_as() {
        let pdp = crate::Pdp::embedded("family").expect("the family pack is carried");
        let world = LocalWorld::parse(&written("home", "ana"), &pdp).expect("the world reads");
        assert_eq!(world.realm().as_str(), "home");
        assert_eq!(world.person().kind(), PrincipalKind::User);
        assert_eq!(world.person().id().as_str(), "ana");
        let directory = world.into_directory();
        assert!(directory.principal(PrincipalKind::User, "bruno").is_some());
    }

    /// A world file's grant is the one its person holds in its realm, and its
    /// epoch the realm's; a world naming no grant is a person who holds none.
    #[tokio::test]
    async fn a_world_files_grant_is_its_persons_and_its_epoch_its_realms() {
        let pdp = crate::Pdp::embedded("family").expect("the family pack is carried");
        let ana = crate::EntityRef {
            type_name: "User".to_owned(),
            id: "ana".to_owned(),
        };
        let world = LocalWorld::parse(&granted("home", "ana", Some("g.by_policy")), &pdp)
            .expect("the world reads");
        let directory = world.into_directory();
        let realm = directory.realm("home").await.expect("the realm is held");
        assert_eq!(realm.epoch, 7);
        let home = crate::EntityRef {
            type_name: "Realm".to_owned(),
            id: "home".to_owned(),
        };
        let grant = directory
            .grant("home", &ana, &home)
            .await
            .expect("answered")
            .expect("ana holds the grant the world names");
        assert_eq!(grant.id.as_str(), "g.by_policy");
        assert_eq!(grant.entity.type_name, "Grant");
        let bruno = crate::EntityRef {
            type_name: "User".to_owned(),
            id: "bruno".to_owned(),
        };
        assert_eq!(
            directory
                .grant("home", &bruno, &home)
                .await
                .expect("answered"),
            None,
            "a grant the world names is its person's alone"
        );
        let ungranted = LocalWorld::parse(&written("home", "ana"), &pdp)
            .expect("the world reads")
            .into_directory();
        assert_eq!(
            ungranted
                .grant("home", &ana, &home)
                .await
                .expect("answered"),
            None
        );
        assert_eq!(
            ungranted
                .realm("home")
                .await
                .expect("the realm is held")
                .epoch,
            7
        );
    }

    /// Each way a world file is not one this reads is refused, by what it is.
    #[test]
    fn a_world_file_that_is_not_one_this_reads_is_refused() {
        let good = written("home", "ana");
        assert!(refused("not json").contains("not JSON"));
        assert!(refused(&format!(" {good}")).contains("canonically"));
        assert!(refused("[]").contains("not one object"));
        let extra = good.replacen("{\"epoch\"", "{\"also\":\"g.current\",\"epoch\"", 1);
        assert!(refused(&extra).contains("\"also\""), "{}", refused(&extra));
        let no_realm = good.replacen(",\"realm\":\"home\"", "", 1);
        assert!(refused(&no_realm).contains("\"realm\" is missing"));
        let numbered = good.replacen("\"person\":\"ana\"", "\"person\":7", 1);
        assert!(refused(&numbered).contains("\"person\" is missing or not text"));
        assert!(
            refused("{\"epoch\":7,\"person\":\"ana\",\"realm\":\"home\",\"world\":{}}")
                .contains("not a list")
        );
        // A list the schema does not load is the schema's refusal, by Cedar's
        // own words, and none of this reader's.
        let unloaded =
            refused("{\"epoch\":7,\"person\":\"ana\",\"realm\":\"home\",\"world\":[{\"uid\":1}]}");
        assert!(
            !unloaded.starts_with("a world file") && !unloaded.starts_with("the world holds"),
            "{unloaded}"
        );
        assert!(refused(&written("nowhere", "ana")).contains("no realm \"nowhere\""));
        assert!(refused(&written("home", "zed")).contains("no person \"zed\""));
        // **A person of another realm is not this realm's**: `mallory` is the
        // world's, in `office`, and a profile deciding in `home` as her would
        // decide as a principal the realm does not hold.
        assert!(refused(&written("home", "mallory")).contains("inside the realm \"home\""));
    }

    /// A world file's epoch and grant are refused unless the realm holds them.
    ///
    /// An epoch that is missing, zero, below zero or not a number, a grant that
    /// is not text, and a grant the world does not hold inside the realm are
    /// each refused, by what they are.
    #[test]
    fn a_world_files_epoch_and_grant_are_refused_unless_the_realm_holds_them() {
        let good = written("home", "ana");
        for epoch in ["", "\"epoch\":0,", "\"epoch\":-7,", "\"epoch\":\"7\","] {
            let text = good.replacen("\"epoch\":7,", epoch, 1);
            assert!(
                refused(&text).contains("\"epoch\" is missing or not a whole number above zero"),
                "{epoch}: {}",
                refused(&text)
            );
        }
        let numbered = good.replacen("{\"epoch\":7,", "{\"epoch\":7,\"grant\":7,", 1);
        assert!(refused(&numbered).contains("\"grant\" is not text"));
        // `g.not_yet_by_policy` is the world's, in no realm; `nobody` is not
        // the world's at all.
        for grant in ["g.not_yet_by_policy", "nobody"] {
            assert!(
                refused(&granted("home", "ana", Some(grant)))
                    .contains(&format!("holds no grant \"{grant}\"")),
                "{grant}"
            );
        }
    }

    #[tokio::test]
    async fn a_world_agents_operator_is_the_person_its_entity_names_and_none_else() {
        // Two agents the policy world does not hold: one of `home` acting for
        // a person of `office`, and one of `office` acting for a person of
        // `home`. Neither has an operator of its own realm.
        let mut world: serde_json::Value = serde_json::from_str(WORLD).expect("the world is JSON");
        let entities = world.as_array_mut().expect("a list of entities");
        for (agent, realm, operator) in [("stray", "home", "mallory"), ("offsite", "office", "ana")]
        {
            entities.push(serde_json::json!({
                "uid": { "type": "Agent", "id": agent },
                "attrs": {
                    "operator": { "__entity": { "type": "User", "id": operator } },
                    "autonomy": "supervised",
                    "budget_remaining": 10,
                },
                "parents": [{ "type": "Realm", "id": realm }],
            }));
        }
        let pdp = crate::Pdp::embedded("family").expect("the family pack is carried");
        let directory =
            MemoryDirectory::from_world(&world.to_string(), &pdp).expect("the world loads");
        let named = |type_name: &str, id: &str| crate::EntityRef {
            type_name: type_name.to_owned(),
            id: id.to_owned(),
        };
        let delegation = directory
            .delegation("home", &named("Agent", "ana-assistant"))
            .await
            .expect("answered")
            .expect("the world's agent in its realm");
        assert_eq!(delegation.agent.kind(), PrincipalKind::Agent);
        assert_eq!(delegation.agent.id().as_str(), "ana-assistant");
        assert_eq!(delegation.operator.kind(), PrincipalKind::User);
        assert_eq!(delegation.operator.id().as_str(), "ana");
        for (realm, unanswered) in [
            ("office", named("Agent", "ana-assistant")),
            ("home", named("User", "ana")),
            ("home", named("Device", "ana-laptop")),
            ("home", named("Agent", "ghost")),
            ("home", named("Agent", "stray")),
            ("office", named("Agent", "offsite")),
            ("home", named("Agent", "offsite")),
        ] {
            assert_eq!(
                directory.delegation(realm, &unanswered).await,
                Ok(None),
                "{realm} {unanswered:?}"
            );
        }
    }

    #[test]
    fn every_kind_of_principal_is_a_principal_type_the_schema_declares() {
        let pdp = crate::Pdp::embedded("family").expect("the family pack is carried");
        let declared: BTreeSet<String> = pdp
            .compiled
            .schema()
            .principals()
            .map(ToString::to_string)
            .collect();
        let kinds: BTreeSet<String> = [
            PrincipalKind::User,
            PrincipalKind::Device,
            PrincipalKind::Agent,
        ]
        .iter()
        .map(|kind| kind.type_name().to_owned())
        .collect();
        assert_eq!(kinds, declared);
    }
}
