//! The decision point this build embeds, asked about the world through a directory.
//!
//! Every question here goes the way the kernel's will: the realm's facts, the
//! pack it pins held to the one carried, the principal's grant, the facts, the
//! slice, and then [`Pdp::decide`]. What each answer names is asserted as well
//! as what it allows, because the journal records the name (`NOCTUA.md §14.9`)
//! and a denial for the wrong reason is a test that proves nothing.

use std::path::Path;

use nc_authz::{
    Decision, Directory as _, DirectoryError, EntityRef, Facts, MemoryDirectory, Pdp, PdpError,
    PrincipalKind, Question, RealmFacts, sentinel,
};
use nc_catalog::ErrorCode;

/// The world every case in `policy/` is decided against.
const WORLD: &str = include_str!("../../../policy/world.json");

/// The schema, as the policies were validated against it.
const SCHEMA: &str = include_str!("../../../policy/schema.cedarschema");

/// `2026-08-26T00:00:00.000Z`: the moment the pack's cases call now.
const NOW: u64 = 1_787_702_400_000;

/// The epoch `home` is at in every directory here.
const EPOCH: u64 = 7;

/// The family pack's manifest, for a decision point built from other text.
const MANIFEST: &str = "version = 1\n";

/// An entity of type `type_name` called `id`.
fn entity(type_name: &str, id: &str) -> EntityRef {
    EntityRef {
        type_name: type_name.to_owned(),
        id: id.to_owned(),
    }
}

/// The decision point this build carries for `family`.
fn family() -> Pdp {
    Pdp::embedded("family").expect("the family pack is carried and compiles")
}

/// A directory over the world, with `home` at [`EPOCH`] and a current grant for four members.
fn directory(pdp: &Pdp) -> MemoryDirectory {
    let mut directory = MemoryDirectory::from_world(WORLD, pdp)
        .expect("the world loads with the schema")
        .with_epoch("home", EPOCH);
    for member in ["ana", "bruno", "carla", "dora"] {
        directory = directory
            .with_grant("home", entity("User", member), "g.current")
            .expect("home holds g.current");
    }
    directory
}

/// What the kernel vouches for at [`NOW`], resting on `grant`.
fn facts(grant: Option<EntityRef>) -> Facts {
    Facts {
        now_ms: NOW,
        via_agent: false,
        confirmed_by_human: false,
        actor_chain_depth: 0,
        policy_epoch: EPOCH,
        quorum_met: false,
        grant,
    }
}

/// Asks `pdp` about `principal` doing `action` to `resource` in `home`, as the kernel does.
async fn ask(
    pdp: &Pdp,
    directory: &MemoryDirectory,
    principal: &EntityRef,
    action: &str,
    resource: &EntityRef,
) -> Decision {
    ask_vouching(pdp, directory, principal, action, resource, |_| {}).await
}

/// A change to what the kernel vouches for, made before the question is put.
type Vouch = fn(&mut Facts);

/// Asks as [`ask`] does, once `vouch` has changed what the kernel vouches for.
async fn ask_vouching(
    pdp: &Pdp,
    directory: &MemoryDirectory,
    principal: &EntityRef,
    action: &str,
    resource: &EntityRef,
    vouch: Vouch,
) -> Decision {
    let realm = directory.realm("home").await.expect("home is in the world");
    assert!(pdp.carries(&realm), "home pins the pack this build carries");
    let grant = directory
        .grant("home", principal)
        .await
        .expect("the directory answers");
    let mut facts = facts(grant.map(|grant| grant.entity));
    facts.policy_epoch = realm.epoch;
    vouch(&mut facts);
    let slice = directory
        .slice("home", principal, resource, facts.grant.as_ref())
        .await
        .expect("the directory answers");
    pdp.decide(
        &Question {
            principal,
            action,
            resource,
            facts: &facts,
        },
        &slice,
    )
}

/// A denial, named and coded.
fn denied(deciding: &str, code: ErrorCode) -> Decision {
    Decision {
        allowed: false,
        deciding: deciding.to_owned(),
        code: Some(code),
    }
}

/// An allow, named by the permits that decided it.
fn allowed(deciding: &str) -> Decision {
    Decision {
        allowed: true,
        deciding: deciding.to_owned(),
        code: None,
    }
}

#[tokio::test]
async fn an_owner_writing_their_own_content_is_allowed_by_the_permit_that_names_it() {
    let pdp = family();
    let decision = ask(
        &pdp,
        &directory(&pdp),
        &entity("User", "ana"),
        "Write",
        &entity("Content", "photo"),
    )
    .await;
    assert_eq!(
        decision,
        Decision {
            allowed: true,
            deciding: "family.an_owner_governs_their_own_content".to_owned(),
            code: None,
        }
    );
}

#[tokio::test]
async fn a_request_a_forbid_refuses_is_denied_naming_that_forbid() {
    let pdp = family();
    let decision = ask(
        &pdp,
        &directory(&pdp),
        &entity("User", "dora"),
        "Write",
        &entity("Content", "postcard"),
    )
    .await;
    assert_eq!(
        decision,
        denied(
            "family.a_frozen_account_acts_on_nothing",
            ErrorCode::PolicyDenied
        )
    );
}

#[tokio::test]
async fn every_forbid_that_fired_is_named_in_order_and_joined() {
    // `hugo` holds no grant and a status the pack does not know: two floors.
    let pdp = family();
    let decision = ask(
        &pdp,
        &directory(&pdp),
        &entity("User", "hugo"),
        "Read",
        &entity("Content", "notice"),
    )
    .await;
    assert_eq!(
        decision,
        denied(
            "core.provenance,family.a_frozen_account_acts_on_nothing",
            ErrorCode::PolicyDenied
        )
    );
}

#[tokio::test]
async fn a_request_nothing_permits_is_denied_as_no_permit_matched() {
    let pdp = family();
    let decision = ask(
        &pdp,
        &directory(&pdp),
        &entity("User", "bruno"),
        "Write",
        &entity("Content", "photo"),
    )
    .await;
    assert_eq!(
        decision,
        denied(sentinel::NO_PERMIT_MATCHED, ErrorCode::PolicyDenied)
    );
}

#[tokio::test]
async fn every_fact_the_kernel_vouches_for_reaches_the_rule_that_reads_it() {
    // Each fact is flipped alone against its control, and both answers are
    // named: a fact the context drops or writes as a constant turns one answer
    // of its pair into the other. With `quorum_met` written as `true`, the
    // first row's delete, which nobody approved, is allowed.
    let pdp = family();
    let directory = directory(&pdp)
        .with_grant("home", entity("Agent", "ana-assistant"), "g.current")
        .expect("home holds g.current");
    let (ana, assistant) = (entity("User", "ana"), entity("Agent", "ana-assistant"));
    let owner = allowed("family.an_owner_governs_their_own_content");
    let refused = |deciding: &str| denied(deciding, ErrorCode::PolicyDenied);
    let asked: [(&str, &EntityRef, &str, Vouch, Decision); 8] = [
        (
            "a delete with no quorum",
            &ana,
            "Delete",
            |_| {},
            refused("core.destructive_needs_quorum"),
        ),
        (
            "a delete a quorum approved",
            &ana,
            "Delete",
            |facts| facts.quorum_met = true,
            owner.clone(),
        ),
        (
            "a chain at its bound",
            &ana,
            "Write",
            |facts| facts.actor_chain_depth = 2,
            owner.clone(),
        ),
        (
            "a chain past its bound",
            &ana,
            "Write",
            |facts| facts.actor_chain_depth = 3,
            refused("core.chain_depth_is_bounded"),
        ),
        (
            "an agent acting as if alone",
            &assistant,
            "Read",
            |_| {},
            refused("core.an_agent_never_impersonates"),
        ),
        (
            "an agent acting as one",
            &assistant,
            "Read",
            |facts| facts.via_agent = true,
            allowed("family.a_member_reads_shared_content"),
        ),
        (
            "an agent's delete no human confirmed",
            &ana,
            "Delete",
            |facts| {
                facts.quorum_met = true;
                facts.via_agent = true;
            },
            refused("core.irreversible_needs_a_human"),
        ),
        (
            "an agent's delete a human confirmed",
            &ana,
            "Delete",
            |facts| {
                facts.quorum_met = true;
                facts.via_agent = true;
                facts.confirmed_by_human = true;
            },
            owner,
        ),
    ];
    let photo = entity("Content", "photo");
    for (what, principal, action, vouch, expected) in asked {
        let decision = ask_vouching(&pdp, &directory, principal, action, &photo, vouch).await;
        assert_eq!(decision, expected, "{what}");
    }
}

#[tokio::test]
async fn an_entity_the_directory_does_not_hold_is_denied_exactly_as_one_nothing_permits() {
    // Asked of Cedar, the owner permit reads `resource.owner` of an entity that
    // is not there and errs — and a denial named for that error would tell the
    // caller the entity does not exist.
    let pdp = family();
    let directory = directory(&pdp);
    let ana = entity("User", "ana");
    let unknown = ask(&pdp, &directory, &ana, "Read", &entity("Content", "ghost")).await;
    let refused = ask(
        &pdp,
        &directory,
        &entity("User", "bruno"),
        "Write",
        &entity("Content", "photo"),
    )
    .await;
    assert_eq!(unknown, refused);
    assert_eq!(
        unknown,
        denied(sentinel::NO_PERMIT_MATCHED, ErrorCode::PolicyDenied)
    );
}

#[tokio::test]
async fn a_missing_resource_is_named_as_a_held_one_by_a_forbid_that_never_reads_it() {
    // The same principal and the same verb, on a resource the directory holds
    // and on one it does not. A missing one denied as `kernel.no_permit_matched`
    // before Cedar is asked, while the held one names the forbid that fired,
    // would tell any member asking for a delete without a quorum whether a
    // content id exists.
    let pdp = family();
    let directory = directory(&pdp);
    let floors = "core.provenance,family.a_frozen_account_acts_on_nothing";
    let asked = [
        (
            "ana",
            "Delete",
            ("Content", "photo"),
            ("Content", "ghost"),
            "core.destructive_needs_quorum",
        ),
        (
            "ana",
            "Delete",
            ("Content", "vault"),
            ("Content", "no-such-id"),
            "core.destructive_needs_quorum",
        ),
        (
            "elias",
            "Read",
            ("Content", "notice"),
            ("Content", "ghost"),
            floors,
        ),
        (
            "hugo",
            "Read",
            ("User", "bruno"),
            ("User", "nobody"),
            floors,
        ),
        (
            "dora",
            "Write",
            ("Content", "postcard"),
            ("Content", "ghost"),
            "family.a_frozen_account_acts_on_nothing",
        ),
    ];
    for (principal, action, held, missing, forbids) in asked {
        let principal = entity("User", principal);
        let named = denied(forbids, ErrorCode::PolicyDenied);
        let (held, missing) = (entity(held.0, held.1), entity(missing.0, missing.1));
        let there = ask(&pdp, &directory, &principal, action, &held).await;
        assert_eq!(there, named, "the control: {principal:?} {action} {held:?}");
        let absent = ask(&pdp, &directory, &principal, action, &missing).await;
        assert_eq!(absent, there, "{principal:?} {action} {missing:?}");
    }
}

#[tokio::test]
async fn an_entity_of_another_realm_is_denied_even_where_a_permit_reads_nothing_of_it() {
    // The guardian's permit to grant reads no attribute of its resource, so
    // Cedar alone would let `ana` grant on `mallory`, who is `office`'s.
    let pdp = family();
    let directory = directory(&pdp);
    let ana = entity("User", "ana");
    let at_home = ask(&pdp, &directory, &ana, "Grant", &entity("User", "bruno")).await;
    assert_eq!(
        at_home,
        Decision {
            allowed: true,
            deciding: "family.a_guardian_administers_the_realm".to_owned(),
            code: None,
        },
        "the control: the same permit allows inside the realm"
    );
    let elsewhere = ask(&pdp, &directory, &ana, "Grant", &entity("User", "mallory")).await;
    assert_eq!(
        elsewhere,
        denied(sentinel::NO_PERMIT_MATCHED, ErrorCode::PolicyDenied)
    );
    let outsider = ask(
        &pdp,
        &directory,
        &entity("User", "mallory"),
        "Read",
        &entity("Content", "notice"),
    )
    .await;
    assert_eq!(
        outsider,
        denied(sentinel::NO_PERMIT_MATCHED, ErrorCode::PolicyDenied)
    );
    // The two questions `policy/packs/family/tests/permissions.toml` pins as
    // Cedar's `allow` over the whole world, asked through `home`'s slice:
    // `policy/core/README.md` cites this test as what denies them.
    for (principal, action, resource) in [
        (
            entity("User", "mallory"),
            "Read",
            entity("Content", "photo"),
        ),
        (entity("User", "ana"), "Revoke", entity("Realm", "office")),
    ] {
        assert_eq!(
            ask(&pdp, &directory, &principal, action, &resource).await,
            denied(sentinel::NO_PERMIT_MATCHED, ErrorCode::PolicyDenied),
            "{principal:?} {action} {resource:?}"
        );
    }
}

/// A schema with one action group: `Write` is a member of `Mutate`, and `Read` is not.
///
/// The shipped schema declares no group, so nothing it can ask tells a slice
/// built with the schema from one built without it. The context is the one
/// [`Facts`] writes, so a request here is refused for nothing but its policies.
const GROUPED: &str = r#"entity Realm;
entity User in [Realm];
entity Content in [Realm];
entity Grant in [Realm];
type Vouched = {
  "now": datetime,
  "via_agent": Bool,
  "confirmed_by_human": Bool,
  "actor_chain_depth": Long,
  "policy_epoch": Long,
  "quorum_met": Bool,
  "grant"?: Grant,
};
action "Mutate";
action "Write" in ["Mutate"]
  appliesTo { principal: [User], resource: [Content], context: Vouched };
action "Read"
  appliesTo { principal: [User], resource: [Content], context: Vouched };
"#;

/// The world [`GROUPED`] is asked about: one realm, one member, one thing she acts on.
const GROUPED_WORLD: &str = r#"[
    {"uid": {"type": "Realm", "id": "home"}, "attrs": {}, "parents": []},
    {"uid": {"type": "User", "id": "ana"}, "attrs": {},
     "parents": [{"type": "Realm", "id": "home"}]},
    {"uid": {"type": "Content", "id": "note"}, "attrs": {},
     "parents": [{"type": "Realm", "id": "home"}]}
]"#;

/// A permit for every action, so only a `forbid` can refuse one.
const ANYONE_ACTS: (&str, &str) = (
    "policy/packs/family/anyone.cedar",
    "@id(\"family.anyone_acts\")\npermit (principal, action, resource);\n",
);

/// A floor over the `Mutate` group, which names its members only through the group.
const NOTHING_MUTATES: (&str, &str) = (
    "policy/packs/family/nothing_mutates.cedar",
    "// nc:narrows: `action in` — the group is the rule: whatever mutates, and nothing else.\n\
     @id(\"family.nothing_mutates\")\n\
     forbid (principal, action in Action::\"Mutate\", resource);\n",
);

/// What a decision point over [`GROUPED`] answers for `ana` doing `action` to `note` in `home`.
///
/// Asked through a slice, as the kernel asks, because the slice is what is
/// under test: what it holds is all Cedar knows of the world.
async fn in_the_grouped_world(pdp: &Pdp, action: &str) -> Decision {
    let directory = MemoryDirectory::from_world(GROUPED_WORLD, pdp).expect("the world loads");
    let (ana, note) = (entity("User", "ana"), entity("Content", "note"));
    let slice = directory
        .slice("home", &ana, &note, None)
        .await
        .expect("the directory answers");
    pdp.decide(
        &Question {
            principal: &ana,
            action,
            resource: &note,
            facts: &facts(None),
        },
        &slice,
    )
}

#[tokio::test]
async fn a_forbid_on_an_action_group_fires_through_a_slice() {
    // Cedar reads `Action::"Write" in Action::"Mutate"` from the entities it is
    // handed, and action entities come only from the schema — so a slice built
    // without it holds no group, the floor never fires and the permit beside it
    // allows. The shipped schema declares no group, so the shipped pack cannot
    // show whether `slice_now` hands Cedar the schema's action entities —
    // written `Entities::from_entities(entities, None)`, it would not — and
    // this test builds a grouped schema that tells the two apart.
    let permit_only = Pdp::from_sources_for_tests(GROUPED, &[ANYONE_ACTS], "family", MANIFEST)
        .expect("the permit compiles against the grouped schema");
    assert_eq!(
        in_the_grouped_world(&permit_only, "Write").await,
        allowed("family.anyone_acts"),
        "the control: without the floor, the permit allows through the slice"
    );
    let floored =
        Pdp::from_sources_for_tests(GROUPED, &[ANYONE_ACTS, NOTHING_MUTATES], "family", MANIFEST)
            .expect("the floor compiles against the grouped schema");
    assert_eq!(
        in_the_grouped_world(&floored, "Write").await,
        denied("family.nothing_mutates", ErrorCode::PolicyDenied),
        "a member of the group is refused by the floor over it"
    );
    assert_eq!(
        in_the_grouped_world(&floored, "Read").await,
        allowed("family.anyone_acts"),
        "an action outside the group is not refused by it"
    );
}

/// The shipped core and family pack, with `extra` beside them, as a test decision point.
fn family_with(extra: &[(&str, &str)]) -> Result<Pdp, PdpError> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut texts: Vec<(String, String)> = Vec::new();
    for directory in ["policy/core", "policy/packs/family"] {
        let mut paths: Vec<_> = std::fs::read_dir(root.join(directory))
            .expect("a policy directory")
            .map(|entry| entry.expect("an entry").path())
            .filter(|path| path.extension().is_some_and(|kind| kind == "cedar"))
            .collect();
        paths.sort();
        for path in paths {
            let name = path.file_name().expect("a file").to_string_lossy();
            texts.push((
                format!("{directory}/{name}"),
                std::fs::read_to_string(&path).expect("a readable policy"),
            ));
        }
    }
    let mut files: Vec<(&str, &str)> = texts
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    files.extend_from_slice(extra);
    Pdp::from_sources_for_tests(SCHEMA, &files, "family", MANIFEST)
}

/// A `forbid` that errs for every epoch above zero: the product overflows a `Long`.
const ERRS: (&str, &str) = (
    "policy/packs/family/errs.cedar",
    "@id(\"family.a_rule_that_errs\")\n\
     forbid (principal, action, resource)\n\
     when { context.policy_epoch * 9223372036854775807 > 0 };\n",
);

#[tokio::test]
async fn an_evaluation_error_denies_even_when_a_permit_matched() {
    let (ana, photo) = (entity("User", "ana"), entity("Content", "photo"));
    let without = family_with(&[]).expect("the shipped pack compiles");
    let allowed = ask(&without, &directory(&without), &ana, "Write", &photo).await;
    assert!(allowed.allowed, "the control: the permit matches");

    // Cedar skips the erring `forbid` and answers `Allow` from the permit.
    let erring = family_with(&[ERRS]).expect("a rule that errs still compiles");
    let decision = ask(&erring, &directory(&erring), &ana, "Write", &photo).await;
    assert_eq!(
        decision,
        denied(
            "kernel.evaluation_error:family.a_rule_that_errs",
            ErrorCode::PolicyEvaluationFailed
        )
    );
}

#[tokio::test]
async fn an_evaluation_error_denies_by_its_name_even_when_cedar_denied() {
    // The other half of the rule: Cedar denies too, so the verdict is the same
    // and only the name tells the error from a forbid or from no permit. With
    // the rule held on `Allow` alone, an erring rule on a denied request is
    // journaled as `kernel.no_permit_matched`.
    let without = family_with(&[]).expect("the shipped pack compiles");
    let erring = family_with(&[ERRS]).expect("a rule that errs still compiles");
    let (plain, with_it) = (directory(&without), directory(&erring));
    // Nothing permits the first, and a forbid fires on the second.
    let asked = [
        (("bruno", "photo"), sentinel::NO_PERMIT_MATCHED),
        (
            ("dora", "postcard"),
            "family.a_frozen_account_acts_on_nothing",
        ),
    ];
    for ((principal, resource), control) in asked {
        let (principal, resource) = (entity("User", principal), entity("Content", resource));
        assert_eq!(
            ask(&without, &plain, &principal, "Write", &resource).await,
            denied(control, ErrorCode::PolicyDenied),
            "the control: {principal:?} is denied without the rule that errs"
        );
        assert_eq!(
            ask(&erring, &with_it, &principal, "Write", &resource).await,
            denied(
                "kernel.evaluation_error:family.a_rule_that_errs",
                ErrorCode::PolicyEvaluationFailed
            ),
            "{principal:?} {resource:?}"
        );
    }
}

#[tokio::test]
async fn a_request_the_schema_refuses_is_denied_before_any_policy_runs() {
    let pdp = family();
    let directory = directory(&pdp);
    let ana = entity("User", "ana");
    let photo = entity("Content", "photo");
    let invalid = denied(sentinel::REQUEST_INVALID, ErrorCode::PolicyEvaluationFailed);
    // An action the schema does not declare, a resource and a principal of a
    // type no action applies to, and a type Cedar cannot name.
    let asked = [
        (ana.clone(), "Launch", photo.clone()),
        (ana.clone(), "Read", entity("Folder", "household")),
        (entity("Group", "role.guardian"), "Read", photo.clone()),
        (ana.clone(), "Read", entity("Content::", "photo")),
    ];
    for (principal, action, resource) in asked {
        let decision = ask(&pdp, &directory, &principal, action, &resource).await;
        assert_eq!(decision, invalid, "{principal:?} {action} {resource:?}");
    }
}

#[tokio::test]
async fn facts_the_schema_cannot_hold_are_an_invalid_request_and_never_a_wrapped_value() {
    let pdp = family();
    let directory = directory(&pdp);
    let (ana, photo) = (entity("User", "ana"), entity("Content", "photo"));
    let slice = directory
        .slice("home", &ana, &photo, None)
        .await
        .expect("the directory answers");
    let current = Some(entity("Grant", "g.current"));
    let mut past_a_long = facts(current.clone());
    past_a_long.policy_epoch = u64::MAX;
    let mut past_a_datetime = facts(current.clone());
    past_a_datetime.now_ms = u64::MAX;
    let mut a_user_for_a_grant = facts(current);
    a_user_for_a_grant.grant = Some(entity("User", "bruno"));
    for facts in [past_a_long, past_a_datetime, a_user_for_a_grant] {
        let question = Question {
            principal: &ana,
            action: "Write",
            resource: &photo,
            facts: &facts,
        };
        assert_eq!(
            pdp.decide(&question, &slice),
            denied(sentinel::REQUEST_INVALID, ErrorCode::PolicyEvaluationFailed),
            "{facts:?}"
        );
    }
}

#[tokio::test]
async fn the_moment_a_grant_expires_is_the_moment_the_facts_write() {
    // `g.expiring_now` ends at 2026-08-26T00:00:00Z, exactly `NOW`.
    let pdp = family();
    let directory = MemoryDirectory::from_world(WORLD, &pdp)
        .expect("the world loads")
        .with_epoch("home", EPOCH)
        .with_grant("home", entity("User", "ana"), "g.expiring_now")
        .expect("home holds g.expiring_now");
    let (ana, photo) = (entity("User", "ana"), entity("Content", "photo"));
    let grant = Some(entity("Grant", "g.expiring_now"));
    let slice = directory
        .slice("home", &ana, &photo, grant.as_ref())
        .await
        .expect("the directory answers");
    let at = |now_ms: u64| {
        let mut facts = facts(grant.clone());
        facts.now_ms = now_ms;
        pdp.decide(
            &Question {
                principal: &ana,
                action: "Write",
                resource: &photo,
                facts: &facts,
            },
            &slice,
        )
    };
    assert!(
        at(NOW - 1).allowed,
        "a millisecond before it ends, it holds"
    );
    assert_eq!(
        at(NOW),
        denied("core.grant_not_expired", ErrorCode::PolicyDenied)
    );
}

#[tokio::test]
async fn a_realm_whose_pack_this_build_does_not_carry_is_refused() {
    for profile in ["org", "collective", "", "Family"] {
        let refused = Pdp::embedded(profile).expect_err("no such pack is carried");
        assert_eq!(refused, PdpError::ProfileNotCarried(profile.to_owned()));
        assert_eq!(refused.code(), ErrorCode::PolicyPackNotCarried);
    }
    let pdp = family();
    let office = directory(&pdp)
        .realm("office")
        .await
        .expect("office is in the world");
    assert!(!pdp.carries(&office), "office pins org@0.1.0: {office:?}");
    let realm = |profile: &str, pinned: &str| RealmFacts {
        profile: profile.to_owned(),
        pinned_pack: pinned.to_owned(),
        epoch: EPOCH,
    };
    // The pin alone is not the realm's choice: a collective pinning the family
    // pack names rules it was never reviewed against.
    assert!(!pdp.carries(&realm("collective", "family@1")));
    assert!(!pdp.carries(&realm("family", "family@2")));
    assert!(!pdp.carries(&realm("family", "family@1 ")));
    assert!(pdp.carries(&realm("family", "family@1")));
}

/// The checks a refusal names, sorted, each once.
fn checks(refused: &PdpError) -> Vec<&str> {
    let mut named: Vec<&str> = match refused {
        PdpError::Unreadable(problems)
        | PdpError::Invalid(problems)
        | PdpError::RuleBroken(problems) => problems.iter().map(|problem| problem.check).collect(),
        PdpError::ProfileNotCarried(_) => Vec::new(),
    };
    named.sort_unstable();
    named.dedup();
    named
}

/// A pack file holding `text`, under the family pack's directory.
const fn in_the_pack(text: &'static str) -> [(&'static str, &'static str); 1] {
    [("policy/packs/family/extra.cedar", text)]
}

#[test]
fn each_refusal_of_a_pack_carries_the_code_its_check_means() {
    // One pack per check, each named with the code that check means. One code
    // for every refusal of a parsed pack would log an `@id` in the reserved
    // prefix as NC-POL-0002, which the ledger records as a pack Cedar's
    // validator refuses: a schema failure.
    let unreadable = ErrorCode::PolicyUnreadable;
    let (invalid, broken) = (ErrorCode::PolicyInvalid, ErrorCode::PolicyRuleBroken);
    let refusals = [
        (
            "policy-malformed",
            family_with(&in_the_pack("permit (principal,")),
            unreadable,
        ),
        (
            "schema-malformed",
            Pdp::from_sources_for_tests("entity", &[ERRS], "family", MANIFEST),
            unreadable,
        ),
        (
            "pack-manifest-malformed",
            Pdp::from_sources_for_tests(SCHEMA, &[ERRS], "family", "version = \"one\"\n"),
            unreadable,
        ),
        (
            "pack-invalid",
            family_with(&in_the_pack(
                "@id(\"family.reads_what_is_not_there\")\n\
                 permit (principal, action, resource) when { principal.shoe_size > 40 };\n",
            )),
            invalid,
        ),
        (
            "pack-warns",
            family_with(&in_the_pack(
                "@id(\"family.never_fires\")\n\
                 permit (principal, action, resource) when { false };\n",
            )),
            invalid,
        ),
        (
            "policy-id-reserved",
            family_with(&in_the_pack(
                "@id(\"kernel.no_permit_matched\")\npermit (principal, action, resource);\n",
            )),
            broken,
        ),
        (
            "policy-id-malformed",
            family_with(&in_the_pack(
                "@id(\"Family.Shouting\")\npermit (principal, action, resource);\n",
            )),
            broken,
        ),
        (
            "duplicate-policy-id",
            family_with(&in_the_pack(
                "@id(\"family.an_owner_governs_their_own_content\")\n\
                 permit (principal, action, resource);\n",
            )),
            broken,
        ),
        (
            "policy-without-an-id",
            family_with(&in_the_pack("permit (principal, action, resource);\n")),
            broken,
        ),
        (
            "policy-is-not-in-force",
            family_with(&in_the_pack(
                "@id(\"family.a_template\")\n\
                 permit (principal == ?principal, action, resource);\n",
            )),
            broken,
        ),
        (
            "core-narrows-by-entity-type",
            family_with(&[(
                "policy/core/narrowed.cedar",
                "@id(\"core.narrowed\")\n\
                 forbid (principal == User::\"ana\", action, resource);\n",
            )]),
            broken,
        ),
        (
            "empty-pack",
            Pdp::from_sources_for_tests(SCHEMA, &[], "family", MANIFEST),
            broken,
        ),
    ];
    for (check, built, code) in refusals {
        let refused = built.expect_err(check);
        assert_eq!(checks(&refused), [check], "{refused}");
        assert_eq!(refused.code(), code, "{check}: {refused}");
    }
}

#[test]
fn a_refusal_prints_its_code_and_what_was_refused() {
    let refused = family_with(&[("policy/packs/family/broken.cedar", "permit (")])
        .expect_err("it does not parse");
    let printed = refused.to_string();
    let unreadable = format!("[{}]", ErrorCode::PolicyUnreadable);
    assert!(printed.starts_with(&unreadable), "{printed}");
    assert!(
        printed.contains("policy/packs/family/broken.cedar"),
        "{printed}"
    );
    let not_carried = Pdp::embedded("org\u{1b}[2J")
        .expect_err("not carried")
        .to_string();
    let uncarried = format!("[{}]", ErrorCode::PolicyPackNotCarried);
    assert!(not_carried.starts_with(&uncarried), "{not_carried}");
    assert!(
        !not_carried.contains('\u{1b}'),
        "the profile is escaped: {not_carried}"
    );
}

#[test]
fn the_embedded_pack_holds_the_policies_the_gate_loads_for_the_core_and_the_family() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let loaded = nc_policy::load(&root).expect("the gate's pack loads");
    let mut expected = loaded.ids_under("policy/core/");
    expected.extend(loaded.ids_under("policy/packs/family/"));
    expected.sort();
    let pdp = family();
    assert_eq!(pdp.ids(), expected);
    // A floor over both halves, so an empty pair cannot agree.
    assert!(loaded.ids_under("policy/core/").len() > 1);
    assert!(loaded.ids_under("policy/packs/family/").len() > 1);

    let version = loaded.versions().get("family").copied();
    let pinned = version.map(|version| nc_policy::manifest::Manifest::new(version).pin("family"));
    assert_eq!(Some(pdp.pack()), pinned.as_deref());
    assert_eq!(
        loaded.pins().get("home").map(String::as_str),
        Some(pdp.pack())
    );
    assert_eq!(pdp.profile(), "family");
}

#[test]
fn the_decision_point_reads_the_actions_and_resources_the_schema_declares() {
    let pdp = family();
    assert_eq!(
        pdp.actions(),
        ["Delete", "Grant", "Read", "Revoke", "Share", "Write"]
    );
    assert_eq!(
        pdp.resources_for("Read"),
        ["Content", "Feature", "Realm", "User"]
    );
    for undeclared in ["Launch", "read", "", "Re\"ad"] {
        assert!(pdp.resources_for(undeclared).is_empty(), "{undeclared}");
    }
}

#[tokio::test]
async fn a_realm_is_read_from_the_world_with_the_epoch_it_was_given() {
    let pdp = family();
    let configured = directory(&pdp);
    assert_eq!(
        configured.realm("home").await,
        Ok(RealmFacts {
            profile: "family".to_owned(),
            pinned_pack: "family@1".to_owned(),
            epoch: EPOCH,
        })
    );
    // Nobody set office's epoch, so it is zero, which the core refuses.
    assert_eq!(
        configured.realm("office").await.map(|office| office.epoch),
        Ok(0)
    );
    let bare = MemoryDirectory::from_world(WORLD, &pdp).expect("the world loads");
    assert_eq!(bare.realm("home").await.map(|home| home.epoch), Ok(0));
}

#[tokio::test]
async fn a_realm_the_world_does_not_hold_is_unknown() {
    let pdp = family();
    let directory = directory(&pdp);
    for realm in ["nowhere", "", "ana", "Home"] {
        assert_eq!(
            directory.realm(realm).await,
            Err(DirectoryError::UnknownRealm(realm.to_owned()))
        );
    }
}

#[tokio::test]
async fn an_unconfigured_realm_denies_everything_by_its_epoch() {
    let pdp = family();
    let bare = MemoryDirectory::from_world(WORLD, &pdp)
        .expect("the world loads")
        .with_grant("home", entity("User", "ana"), "g.current")
        .expect("home holds g.current");
    let decision = ask(
        &pdp,
        &bare,
        &entity("User", "ana"),
        "Write",
        &entity("Content", "photo"),
    )
    .await;
    assert_eq!(
        decision,
        denied("core.fail_closed", ErrorCode::PolicyDenied)
    );
}

#[tokio::test]
async fn a_grant_is_held_per_realm_and_per_principal() {
    let pdp = family();
    let directory = directory(&pdp);
    let ana = entity("User", "ana");
    let held = directory
        .grant("home", &ana)
        .await
        .expect("the directory answers")
        .expect("ana holds a grant in home");
    assert_eq!(held.entity, entity("Grant", "g.current"));
    assert_eq!(held.id.as_str(), "g.current");
    assert_eq!(directory.grant("office", &ana).await, Ok(None));
    assert_eq!(
        directory.grant("home", &entity("User", "elias")).await,
        Ok(None)
    );
    assert_eq!(
        directory.grant("home", &entity("Device", "ana")).await,
        Ok(None)
    );
}

#[test]
fn a_grant_is_refused_when_malformed_or_not_held_by_the_realm() {
    let pdp = family();
    let ana = entity("User", "ana");
    for (realm, grant) in [
        ("home", "g current"),
        ("home", ""),
        ("home", "g.nowhere"),
        // In the world and in no realm.
        ("home", "g.not_yet_by_policy"),
        // Home's, asked of office.
        ("office", "g.current"),
        ("nowhere", "g.current"),
    ] {
        let refused = MemoryDirectory::from_world(WORLD, &pdp)
            .expect("the world loads")
            .with_grant(realm, ana.clone(), grant)
            .expect_err("refused");
        assert!(
            matches!(refused, DirectoryError::Malformed(_)),
            "{realm} {grant}: {refused}"
        );
    }
    // Held by its realm, so the identifier rule alone refuses it: every id
    // above is also absent from the world, and an id minted leniently —
    // `g_spaced` in the journal for `g spaced` in Cedar — would pass.
    let refused = MemoryDirectory::from_world(SPACED, &pdp)
        .expect("the world loads")
        .with_grant("home town", entity("User", "ana bob"), "g spaced")
        .expect_err("a space is not in an identifier");
    let unwritable = nc_ids::OpaqueId::new("g spaced").expect_err("the identifier rule refuses it");
    assert_eq!(refused, DirectoryError::Malformed(unwritable.to_string()));
}

#[tokio::test]
async fn a_slice_holds_what_was_named_and_its_ancestors_in_the_realm_and_nothing_else() {
    let pdp = family();
    let directory = directory(&pdp);
    let (ana, photo) = (entity("User", "ana"), entity("Content", "photo"));
    let grant = entity("Grant", "g.current");
    let slice = directory
        .slice("home", &ana, &photo, Some(&grant))
        .await
        .expect("the directory answers");
    for held in [
        ana,
        photo,
        grant,
        entity("Group", "role.guardian"),
        entity("Realm", "home"),
        entity("Folder", "household"),
        entity("Feature", "gallery"),
    ] {
        assert!(slice.holds(&held), "{held:?} is in the slice");
    }
    for absent in [
        entity("User", "bruno"),
        entity("Content", "notice"),
        entity("Realm", "office"),
        entity("Content::", "photo"),
    ] {
        assert!(!slice.holds(&absent), "{absent:?} is not in the slice");
    }
    let across = directory
        .slice(
            "home",
            &entity("User", "mallory"),
            &entity("Realm", "office"),
            None,
        )
        .await
        .expect("the directory answers");
    assert!(!across.holds(&entity("User", "mallory")));
    assert!(!across.holds(&entity("Realm", "office")));
    let nowhere = directory
        .slice(
            "nowhere",
            &entity("User", "ana"),
            &entity("Realm", "home"),
            None,
        )
        .await
        .expect("the directory answers");
    assert!(!nowhere.holds(&entity("User", "ana")));
}

#[test]
fn a_world_the_schema_refuses_is_refused() {
    let pdp = family();
    let statusless = r#"[{"uid": {"type": "User", "id": "ana"},
        "attrs": {"birth_date": {"__extn": {"fn": "datetime", "arg": "1988-04-12"}}},
        "parents": []}]"#;
    for world in ["not json", "{}", statusless] {
        let refused = MemoryDirectory::from_world(world, &pdp).expect_err("refused");
        assert!(matches!(refused, DirectoryError::Malformed(_)), "{refused}");
    }
}

/// A world holding entities whose ids Cedar accepts and `nc_ids::OpaqueId` refuses.
const SPACED: &str = r#"[
    {"uid": {"type": "Realm", "id": "home town"},
     "attrs": {"profile": "family", "policy_pack": "family@1"}, "parents": []},
    {"uid": {"type": "User", "id": "ana bob"},
     "attrs": {"birth_date": {"__extn": {"fn": "datetime", "arg": "1988-04-12"}},
               "status": "active"},
     "parents": [{"type": "Realm", "id": "home town"}]},
    {"uid": {"type": "Grant", "id": "g spaced"},
     "attrs": {"granted_by": {"__entity": {"type": "User", "id": "ana bob"}},
               "reason": "held by its realm under an id the identifier rule refuses",
               "valid_from": {"__extn": {"fn": "datetime", "arg": "2026-01-01"}},
               "valid_until": {"__extn": {"fn": "datetime", "arg": "2027-01-01"}},
               "source": "manual"},
     "parents": [{"type": "Realm", "id": "home town"}]}
]"#;

#[tokio::test]
async fn a_principal_is_minted_only_for_an_entity_the_world_holds_under_a_well_formed_id() {
    let pdp = family();
    let directory = directory(&pdp);
    let ana = directory
        .principal(PrincipalKind::User, "ana")
        .expect("ana is a user in the world");
    assert_eq!(ana.kind(), PrincipalKind::User);
    assert_eq!(ana.id().as_str(), "ana");
    assert_eq!(ana.entity(), entity("User", "ana"));
    let laptop = directory
        .principal(PrincipalKind::Device, "ana-laptop")
        .expect("a device");
    assert_eq!(laptop.entity(), entity("Device", "ana-laptop"));
    let assistant = directory
        .principal(PrincipalKind::Agent, "ana-assistant")
        .expect("an agent");
    assert_eq!(assistant.entity(), entity("Agent", "ana-assistant"));
    for (kind, id) in [
        (PrincipalKind::User, "ghost"),
        (PrincipalKind::Device, "ana"),
        (PrincipalKind::Agent, "ana-laptop"),
        (PrincipalKind::User, ""),
    ] {
        assert!(directory.principal(kind, id).is_none(), "{kind:?} {id}");
    }
    assert_eq!(
        directory.realm_id("home").map(|id| id.as_str().to_owned()),
        Some("home".to_owned())
    );
    assert!(directory.realm_id("nowhere").is_none());
    assert!(directory.realm_id("ana").is_none());

    // The entity is there, and only the identifier rule refuses it.
    let spaced = MemoryDirectory::from_world(SPACED, &pdp).expect("the world loads");
    let odd = entity("User", "ana bob");
    let slice = spaced
        .slice("home town", &odd, &entity("Realm", "home town"), None)
        .await
        .expect("the directory answers");
    assert!(slice.holds(&odd), "the world holds it");
    assert!(spaced.principal(PrincipalKind::User, "ana bob").is_none());
    assert!(spaced.realm_id("home town").is_none());
    assert!(
        spaced.realm("home town").await.is_ok(),
        "the realm is there"
    );
}

#[test]
fn the_directory_answers_on_any_thread() {
    fn sendable<T: Send>(_: T) {}
    let pdp = family();
    let directory = directory(&pdp);
    let ana = entity("User", "ana");
    sendable(directory.realm("home"));
    sendable(directory.grant("home", &ana));
    sendable(directory.slice("home", &ana, &ana, None));
}

#[test]
fn a_directory_refusal_prints_what_it_was_given_escaped() {
    // Every variant, because each carries text from outside: a realm id, the
    // schema's complaint about a world, a database's error. With only the
    // realm's escaping asserted, the other two could print raw.
    let hostile = "\u{1b}[2J";
    let printed = [
        DirectoryError::Unavailable(hostile.to_owned()),
        DirectoryError::UnknownRealm(hostile.to_owned()),
        DirectoryError::Malformed(hostile.to_owned()),
    ]
    .map(|error| error.to_string());
    assert!(printed[0].contains("did not answer"), "{}", printed[0]);
    assert!(printed[1].contains("holds no realm"), "{}", printed[1]);
    assert!(printed[2].contains("refused"), "{}", printed[2]);
    for each in &printed {
        assert!(!each.contains('\u{1b}'), "escaped: {each}");
        assert!(each.contains("\\u{1b}[2J"), "written, not dropped: {each}");
    }
}
