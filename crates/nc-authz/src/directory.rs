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
//! else. The directory over the grants table is Stage 22's. A profile's
//! `world.json` is read by [`LocalWorld::parse`] into the realm and the person
//! a daemon decides as, the grant that person holds and the realm's epoch, and
//! that directory over the rest.
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

    use super::{Directory as _, DirectoryError, LocalWorld, PrincipalKind};

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
        let grant = directory
            .grant("home", &ana)
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
            directory.grant("home", &bruno).await.expect("answered"),
            None,
            "a grant the world names is its person's alone"
        );
        let ungranted = LocalWorld::parse(&written("home", "ana"), &pdp)
            .expect("the world reads")
            .into_directory();
        assert_eq!(ungranted.grant("home", &ana).await.expect("answered"), None);
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
