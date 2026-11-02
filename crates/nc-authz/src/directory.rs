//! What the decision point asks about the world: realms, grants and entities.
//!
//! A [`Directory`] answers three questions for the enforcement point — what a
//! realm selects and pins, which grant a principal holds there, and which
//! entities a question needs — and it answers them for one realm at a time.
//! That is how realm isolation is structural (`NOCTUA.md §10.3`, `§14.3`): a
//! kernel serves one realm, the directory hands out only that realm's entities
//! and grants, and a principal from another realm arrives with no grant and no
//! entity, which the core refuses and [`crate::Pdp::decide`] denies.
//!
//! [`MemoryDirectory`] is the one this stage runs: the world of
//! `policy/world.json`, the grants and epochs a test gives it, and nothing
//! else. The directory over the grants table is Stage 22's.
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
//! **What it does not answer** is whether a grant or an epoch is current:
//! this one holds what it was given, and a directory that falls behind the
//! grants table is what `§14.8`'s epoch is for.

use std::collections::{BTreeMap, HashSet};

use cedar_policy::{Entities, Entity, EntityUid, EvalResult, Schema};
use nc_ids::OpaqueId;

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

/// Where the decision point learns about realms, grants and entities.
///
/// Asynchronous because the directory behind a running kernel reads a
/// database, and not `dyn`-compatible for the reason `nc_journal::Store` gives:
/// `impl Future` buys no boxed allocation per call, and a kernel is generic
/// over the directory it runs with. Every answer is about one realm, and an
/// `Err` is a refusal to decide: the enforcement point denies on one, and
/// never reads it as "no".
pub trait Directory: Send + Sync + 'static {
    /// What `realm` selects and pins, and its epoch.
    ///
    /// # Errors
    ///
    /// Returns [`DirectoryError::UnknownRealm`] for a realm the directory does
    /// not hold, and [`DirectoryError::Unavailable`] when it cannot answer.
    fn realm(&self, realm: &str)
    -> impl Future<Output = Result<RealmFacts, DirectoryError>> + Send;

    /// The grant `principal` holds in `realm`, if any.
    ///
    /// # Errors
    ///
    /// Returns [`DirectoryError::Unavailable`] when the directory cannot
    /// answer.
    fn grant(
        &self,
        realm: &str,
        principal: &EntityRef,
    ) -> impl Future<Output = Result<Option<GrantFacts>, DirectoryError>> + Send;

    /// The entities a question about `principal`, `resource` and `grant` needs.
    ///
    /// Only what `realm` holds: an entity of another realm, or of none, is
    /// left out, and [`crate::Pdp::decide`] denies a question naming it.
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
}

/// The entities one question is decided over, built with the schema.
#[derive(Debug)]
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

    /// The entities named, and each one's ancestors, that `realm` holds.
    fn slice_now(&self, realm: &str, named: &[&EntityRef]) -> Result<Slice, DirectoryError> {
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

    fn grant(
        &self,
        realm: &str,
        principal: &EntityRef,
    ) -> impl Future<Output = Result<Option<GrantFacts>, DirectoryError>> + Send {
        std::future::ready(Ok(self
            .grants
            .get(&(realm.to_owned(), principal.clone()))
            .cloned()))
    }

    fn slice(
        &self,
        realm: &str,
        principal: &EntityRef,
        resource: &EntityRef,
        grant: Option<&EntityRef>,
    ) -> impl Future<Output = Result<Slice, DirectoryError>> + Send {
        let named: Vec<&EntityRef> = [Some(principal), Some(resource), grant]
            .into_iter()
            .flatten()
            .collect();
        std::future::ready(self.slice_now(realm, &named))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::PrincipalKind;

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
