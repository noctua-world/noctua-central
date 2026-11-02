//! The policy pack: loading it, validating it, and running its cases.
//!
//! **This is neither an enforcement point nor the decision point.** The
//! enforcement point is `commands.dispatch` and the gateway's middleware,
//! asking `nc-authz` (`NOCTUA.md §14.2`). What this crate does is hold the
//! pack to its own rules before it can reach a deployment: it parses, it
//! validates against the schema transcribed from
//! `reference/identity-and-authorization.md §4`, it runs every case through
//! Cedar's own authorizer, and it refuses a policy that has no denial test.
//!
//! **And it is the one door a decision point reaches a policy through.**
//! [`compile`] parses, keys and validates a set of policy files against the
//! schema, refusing everything [`load`] refuses in the policies themselves — an
//! `@id` under [`RESERVED_PREFIX`] included — and hands back what a decision
//! point evaluates with, the schema and the policy set. `load` compiles every
//! pack at once, for the gate; a decision point compiles the core and its
//! realm's pack. What only the gate holds — where each policy file sits, the
//! world, each pack's [`manifest`] and the pins the world writes against it —
//! stays in `load`, and the cases stay in [`run`] and [`coverage`].
//!
//! # Why a denial test is not just a test
//!
//! `§14.9`'s seventh invariant: **every policy has a denial test, not only a
//! permission test. A policy without a negative test does not enter the
//! monorepo.** Cedar denies by default, so `decision = "deny"` on its own is
//! true of an empty pack — it is the purest form of the `§19.5` vacuous
//! assertion, and a suite full of them would report a floor that is not there.
//!
//! So a denial case says **why**, and the runner proves the reason:
//!
//! * A case denied by a `forbid` names it in `because`. The runner re-runs the
//!   same request with **that policy removed** and requires the answer to become
//!   `allow`. If it does not, the case was passing for some other reason and
//!   proves nothing about the policy it names.
//! * A case denied because nothing permitted it names the permit it falls just
//!   outside of, in `no_permit_matched`. The runner re-runs it with **every
//!   `forbid` removed** and requires it to stay denied — which is what makes it
//!   a case about a missing permit rather than about a rule that fired.
//!
//! Neither check can be satisfied by a case that does nothing.

#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
// Every doctest of this crate forbids `unsafe` as the crate does: a
// doctest is a crate of its own, outside the root's `forbid`.
#![doc(test(attr(forbid(unsafe_code))))]
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::str::FromStr as _;

use cedar_policy::{
    Authorizer, Context, Decision, Effect, Entities, EntityUid, Policy, PolicyId, PolicySet,
    Request, Schema, ValidationMode, Validator,
};

pub mod cases;
pub mod manifest;
mod problem;
mod value;

pub use cases::{Case, Decision as Expected};
pub use problem::Problem;

/// Where the pack lives, relative to the repository root.
const POLICY: &str = "policy";

/// The schema's file, inside [`POLICY`].
const SCHEMA: &str = "schema.cedarschema";

/// The prefix no policy's `@id` may begin with: it is the decision point's own.
///
/// A journal entry names what decided (`NOCTUA.md §14.9`) — a policy's `@id`,
/// or, when no policy decided, a sentinel the decision point writes itself:
/// nothing permitted the request, the pack could not be loaded, the directory
/// did not answer. Both land in the same column, so a pack rule called
/// `kernel.pdp_unavailable` would make an `allow` it decided read, to whoever
/// audits the journal, exactly like the fail-closed refusal of that name.
/// [`compile`] refuses the prefix, which keeps the two sets apart by
/// construction rather than by everybody remembering.
///
/// **What it does not answer on its own** is an id that only *looks* like one:
/// the prefix is compared as bytes. [`compile`] also holds every id to one
/// shape, lower-case ASCII words joined by dots, which refuses another case,
/// surrounding whitespace and a letter from another script. What is left is a
/// look-alike inside that alphabet, `kerne1.` for `kernel.`: another string,
/// which the journal records as written.
pub const RESERVED_PREFIX: &str = "kernel.";

/// The loaded pack: its schema, its policies, and the world its cases run in.
#[derive(Debug)]
pub struct Pack {
    schema: Schema,
    policies: PolicySet,
    entities: Entities,
    /// Every policy's effect, by id, so a case can be told what kind it names.
    effects: BTreeMap<String, Effect>,
    /// Which file each policy came from, for a diagnostic that can be acted on.
    origins: BTreeMap<String, String>,
    /// `world.json` as it was read, so a guard's absent branch can be built.
    ///
    /// Every attribute a `!(x has y) ||` guard names is declared **required** by
    /// the schema, so the one request that takes the guard's other branch cannot
    /// be loaded with the schema on. Keeping the text lets the gate strip one
    /// attribute and re-parse with no schema — which is the decision point the
    /// guard exists for.
    world: String,
    /// What the packs' manifests declare and what the world's realms pin.
    carried: manifest::Carried,
}

impl Pack {
    /// The version each pack declares in its `pack.toml`, by profile.
    ///
    /// Only a pack that holds a policy has one, so `org` and `collective`,
    /// which hold none, are absent rather than versioned.
    #[must_use]
    pub const fn versions(&self) -> &BTreeMap<String, u32> {
        &self.carried.versions
    }

    /// Every realm held to the pack its profile selects, by id, with its pin.
    ///
    /// Each pin has already been held to that pack's version. A realm whose
    /// profile selects no carried pack is not here: its pin names its own
    /// profile, and there was nothing on disk to hold the version to.
    #[must_use]
    pub const fn pins(&self) -> &BTreeMap<String, String> {
        &self.carried.pins
    }

    /// Every policy id in the pack, in order.
    #[must_use]
    pub fn ids(&self) -> Vec<String> {
        self.effects.keys().cloned().collect()
    }

    /// How many policies it holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.effects.len()
    }

    /// Whether it holds none, which is never right.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    /// Every policy read from a directory, by id.
    ///
    /// **Called from outside this crate, and decided there.** `nc-authz`'s
    /// `the_embedded_pack_holds_the_policies_the_gate_loads_for_the_core_and_the_family`
    /// narrows what `load` read from the whole tree to the core and one pack
    /// with it, and holds the ids a decision point embeds to what is left — so
    /// gutting this fails that suite, and no test in this crate notices. It is
    /// the walk [`Self::effects_under`] makes, with the verdicts dropped.
    ///
    /// **The gate's own rule does not run through it.** *"Every policy here is
    /// a `forbid`"* reads [`Self::effects_under`], which answers the count and
    /// the verdict in one walk so the two cannot be given different prefixes.
    #[must_use]
    pub fn ids_under(&self, under: &str) -> Vec<String> {
        self.effects_under(under)
            .into_iter()
            .map(|(id, _)| id)
            .collect()
    }

    /// Every policy from a directory, by id, with whether it permits.
    ///
    /// **One question over one list, because two were two prefixes.** The core's
    /// *"every policy here is a `forbid`"* was decided by `ids_under` for the
    /// count and `permits_under` for the verdict, each given the prefix
    /// separately — so either could be pointed somewhere else on its own.
    /// Measured on 2026-09-10: `permits_under` aimed at `policy/core/tests/`
    /// left the step byte-identical — *11 core policy(ies) are forbid*, over
    /// nothing examined — and `ids_under` ignoring its prefix left it printing
    /// 15. A caller that reads the effects of the very list it counted
    /// cannot have one half aimed elsewhere.
    #[must_use]
    pub fn effects_under(&self, under: &str) -> Vec<(String, bool)> {
        let mut found: Vec<(String, bool)> = self
            .origins
            .iter()
            .filter(|(_, origin)| origin.starts_with(under))
            .map(|(id, _)| (id.clone(), self.effects.get(id) == Some(&Effect::Permit)))
            .collect();
        found.sort();
        found
    }

    /// Every policy from a directory whose effect is `permit`, by id.
    ///
    /// **A sentence `policy/core/README.md` states and nothing held.** That file
    /// says *"Every policy here is a `forbid`"*, and the argument beside it is
    /// the whole design: Cedar denies by default, so an invariant expressed as a
    /// `permit` is one that can only ever widen what is allowed. Measured on
    /// 2026-09-09, the sentence changed to `permit` in the file: the whole gate
    /// green, every number byte-identical to a clean run — nothing reads that
    /// file for whether what it says is true, and its own header is where a
    /// reader learns what the core is.
    ///
    /// Answered from the parsed policies rather than from their text, for the
    /// reason [`Self::actions`] is: what a file appears to say and what Cedar
    /// loaded from it are two things.
    ///
    /// `under` is matched as a path prefix against the origin each policy was
    /// read from, so `policy/core/` names the core and says nothing about the
    /// packs — where three `permit`s live on purpose.
    ///
    /// **Nothing calls this, and gutting it leaves the whole suite green**,
    /// which is what a function nothing calls does. The gate it was written for
    /// reads [`Self::effects_under`] instead, for the reason that function's
    /// own doc gives. It is kept as the shape a caller outside this crate would
    /// reach for: a surface, not the mechanism.
    #[must_use]
    pub fn permits_under(&self, under: &str) -> Vec<String> {
        self.effects
            .iter()
            .filter(|(id, effect)| {
                **effect == Effect::Permit
                    && self
                        .origins
                        .get(*id)
                        .is_some_and(|origin| origin.starts_with(under))
            })
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// The entity types the schema lets `action` apply to, as resources.
    ///
    /// **The coordinate the command comparison did not read.** Both shipped
    /// commands select the realm as their resource and the schema declared no
    /// action that applies to a `Realm`, so the request either of them builds
    /// was one Cedar's own schema refuses — measured by appending a case with
    /// `resource = 'Realm::"home"'`: *"is not a request the schema allows:
    /// resource type `Realm` is not valid for `Action::\"Read\"`"*, while the
    /// step over the same tree printed `ok 2 command(s) name an action the
    /// schema declares`. Read from the parsed schema rather than from its text,
    /// for the reason [`Self::actions`] is.
    #[must_use]
    pub fn resources_for(&self, action: &str) -> Vec<String> {
        let Ok(uid) = EntityUid::from_str(&format!("Action::\"{action}\"")) else {
            return Vec::new();
        };
        let mut named: Vec<String> = self
            .schema
            .resources_for_action(&uid)
            .into_iter()
            .flatten()
            .map(ToString::to_string)
            .collect();
        named.sort();
        named
    }

    /// Every action name the schema declares, sorted.
    ///
    /// Taken from the parsed schema rather than from the file's text, so what a
    /// caller checks a name against is the same set the validator holds. The
    /// caller is `cargo xtask check`, which reads `catalog/commands.toml`'s
    /// `action` field: a command naming an action the schema has never heard of
    /// is a command whose authorization request cannot be built, and until this
    /// existed nothing compared the two sides at all.
    #[must_use]
    pub fn actions(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .schema
            .actions()
            .map(|action| action.id().escaped().to_string())
            .collect();
        names.sort();
        names
    }
}

/// What Cedar's validator says about the pack, errors and warnings both.
///
/// Lifted out of [`load`], which crossed clippy's line limit.
///
/// **Only the errors were read.** `validation_warnings()` did not appear
/// anywhere in this tree, and it is where Cedar puts `ImpossiblePolicy` — "this
/// policy expression evaluates to false for all valid requests", which is a
/// `forbid` that cannot fire and reads exactly like one that can — along with
/// `BidiCharsInString`, `BidiCharsInIdentifier`, `MixedScriptIdentifier` and
/// `ConfusableIdentifier`.
fn what_the_validator_says(schema: &Schema, policies: &PolicySet, problems: &mut Vec<Problem>) {
    // Validation last, so a pack that failed to parse is not also reported as
    // invalid — one defect, one diagnostic.
    let result = Validator::new(schema.clone()).validate(policies, ValidationMode::default());
    for error in result.validation_errors() {
        problems.push(Problem {
            file: format!("{POLICY}/"),
            line: 0,
            what: format!("the pack does not validate: {error}"),
            fix: "a policy that mentions an attribute the schema does not have is one \
                  that can never match, and Cedar will not tell you at run time"
                .to_owned(),
            check: "pack-invalid",
        });
    }
    // **And the warnings, which were dropped on the floor.**
    // `validation_errors()` was the only side read, and the other side is
    // where Cedar puts `ImpossiblePolicy` — "this policy expression
    // evaluates to false for all valid requests", which is a `forbid` that
    // cannot fire and reads exactly like one that can — along with
    // `BidiCharsInString`, `BidiCharsInIdentifier`, `MixedScriptIdentifier`
    // and `ConfusableIdentifier`. `grep -rn validation_warnings` over this
    // tree came back empty: the string did not appear in it.
    //
    // A failure and not a `detail`, for the reason every other rule here
    // gives: a policy pack is the one artefact where a rule that decides
    // nothing must not be indistinguishable from one that decides
    // everything. `nc-catalog-compiler` refuses a bidi override in a
    // *message*; this is the file that says who may read it.
    for warning in result.validation_warnings() {
        problems.push(Problem {
            file: format!("{POLICY}/"),
            line: 0,
            what: format!("the pack validates with a warning: {warning}"),
            fix: "Cedar warns where a policy is well-formed and cannot do what it \
                  appears to — an expression false for every valid request, or an \
                  identifier whose spelling is not what it looks like. Fix the \
                  policy, or say in the file why the warning is the intended \
                  shape"
                .to_owned(),
            check: "pack-warns",
        });
    }
}

/// A policy set compiled against its schema: parsed, keyed and validated.
///
/// **What a decision point loads, and all it needs.** [`load`] holds the whole
/// tree to its rules — the world, the manifests, where each policy file sits —
/// and [`run`] and [`coverage`] hold it to its cases, and a decision point
/// needs none of that: it needs the schema every request is built against and
/// the policies that answer it. Both come out of [`compile`], which is the one
/// function either of them reaches a policy through, so a rule `compile`
/// refuses cannot be loaded by the other door either: one that does not parse
/// or is a template, an `@id` missing, reserved or another policy's, a core
/// floor narrowing itself, and anything Cedar's validator errs or warns about.
///
/// **What only the gate holds is not asked again here.** A rule with no denial
/// case, one no written list names, a policy in no pack, a world or a manifest
/// out of step — `compile` accepts a set with any of them, so a decision point
/// is to load only files the gate has passed, and holding the list it embeds
/// to what [`load`] reads is how it knows it does.
#[derive(Debug)]
pub struct Compiled {
    schema: Schema,
    policies: PolicySet,
    /// Every policy's effect, by id.
    effects: BTreeMap<String, Effect>,
    /// Which file each policy came from.
    origins: BTreeMap<String, String>,
}

impl Compiled {
    /// The schema every request is to be built against.
    ///
    /// Passing it is what makes a request missing a context attribute a
    /// refusal before any policy runs, which `policy/core/README.md` asks of
    /// the decision point.
    #[must_use]
    pub const fn schema(&self) -> &Schema {
        &self.schema
    }

    /// The policies, each keyed by its `@id`.
    ///
    /// A journal entry records that `@id` as the name of the policy which
    /// decided.
    #[must_use]
    pub const fn policies(&self) -> &PolicySet {
        &self.policies
    }

    /// Every policy id, in order.
    #[must_use]
    pub fn ids(&self) -> Vec<String> {
        self.effects.keys().cloned().collect()
    }
}

/// Compiles a policy set against a schema, as a decision point loads one.
///
/// `schema` is the text of `policy/schema.cedarschema`, and each file is its
/// path relative to the repository root beside its text. The path is part of
/// the rule: a policy under `policy/core/` is held to the core's — it may not
/// narrow itself by entity type — and every other `forbid` to a pack floor's.
///
/// [`load`] compiles every `.cedar` file under `policy/`, because the gate
/// holds every pack at once. A decision point compiles the core and the one pack
/// its realm's profile selects (`NOCTUA.md §14.4`), through this same function.
///
/// # Errors
///
/// Returns every problem it found: a schema that will not parse; a policy that
/// will not parse, that is a template, that carries no `@id`, whose `@id` begins
/// with [`RESERVED_PREFIX`], is not lower-case words joined by dots, does not begin
/// with the pack its file is in or is another policy's, or that is a floor
/// narrowing itself; a set holding no policy at
/// all; and — only once all of that is clean, so that one defect is one
/// diagnostic — every error and warning Cedar's validator reports against the
/// schema.
pub fn compile(schema: &str, files: &[(String, String)]) -> Result<Compiled, Vec<Problem>> {
    let parsed = schema_of(schema)?;
    let mut problems = Vec::new();
    let (policies, effects, origins) = assemble(schema, files, &mut problems);
    if policies.policies().count() == 0 {
        problems.push(Problem {
            file: format!("{POLICY}/core/"),
            line: 0,
            what: "the pack holds no policies at all".to_owned(),
            fix: "the invariant core is what every pack shares (§14.9). A pack that \
                  loaded nothing would pass every test below, because Cedar denies by \
                  default and a denial proves nothing on its own"
                .to_owned(),
            check: "empty-pack",
        });
    }

    // Validation last, so a pack that failed to parse is not also reported as
    // invalid — one defect, one diagnostic.
    if problems.is_empty() {
        what_the_validator_says(&parsed, &policies, &mut problems);
    }

    if problems.is_empty() {
        Ok(Compiled {
            schema: parsed,
            policies,
            effects,
            origins,
        })
    } else {
        Err(problems)
    }
}

/// Reads the pack under `root`, validates it, and returns it.
///
/// The policies are [`compile`]d — every `.cedar` file under `policy/`, since
/// the gate holds every pack at once — and around them the tree is held to what
/// a decision point never sees: the world the cases are decided against, and
/// each pack's [`manifest`] with the pins the world's realms write against it.
///
/// **Validation does not wait for the world.** It runs inside [`compile`] once
/// the policies themselves are clean, so a tree with a broken world *and* a
/// policy the schema refuses reports both: two defects, two diagnostics.
///
/// # Errors
///
/// Returns every problem it found: a schema that cannot be read or will not
/// parse, a policy that cannot be read, everything [`compile`] refuses, a world
/// that cannot be read or does not load, and everything the manifest gate
/// refuses.
pub fn load(root: &Path) -> Result<Pack, Vec<Problem>> {
    let schema_path = Path::new(POLICY).join(SCHEMA);
    let source = std::fs::read_to_string(root.join(&schema_path)).map_err(|error| {
        vec![Problem {
            file: path_of(&schema_path),
            line: 0,
            what: format!("the schema could not be read: {error}"),
            fix: "the schema is `reference/identity-and-authorization.md §4` transcribed, \
                  and that file outranks NOCTUA.md (§0.4). Without it nothing can be \
                  validated and every policy is only syntax"
                .to_owned(),
            check: "schema-unreadable",
        }]
    })?;
    // Parsed here as well as inside `compile`, because the world is loaded
    // against it — and a schema that does not parse stops everything here, as
    // it always has.
    let schema = schema_of(&source)?;

    let mut problems = Vec::new();
    let files = read_policies(root, &mut problems);
    let compiled = match compile(&source, &files) {
        Ok(compiled) => Some(compiled),
        Err(mut refused) => {
            problems.append(&mut refused);
            None
        }
    };

    let world = root.join(POLICY).join("world.json");
    let mut world_text = String::new();
    let entities = match std::fs::read_to_string(&world) {
        Ok(text) => match Entities::from_json_str(&text, Some(&schema)) {
            Ok(entities) => {
                world_text = text;
                entities
            }
            Err(error) => {
                problems.push(Problem {
                    file: format!("{POLICY}/world.json"),
                    line: 0,
                    what: format!("the world does not load: {error}"),
                    fix: "every case is about these people, devices and grants; entities \
                          that do not match the schema make every case below untrustworthy"
                        .to_owned(),
                    check: "world-malformed",
                });
                Entities::empty()
            }
        },
        Err(error) => {
            problems.push(Problem {
                file: format!("{POLICY}/world.json"),
                line: 0,
                what: format!("the world could not be read: {error}"),
                fix: "the cases have nothing to run against without it".to_owned(),
                check: "world-unreadable",
            });
            Entities::empty()
        }
    };
    let carried = manifest::check(root, &entities, &mut problems);

    match compiled {
        Some(compiled) if problems.is_empty() => Ok(Pack {
            schema: compiled.schema,
            policies: compiled.policies,
            entities,
            effects: compiled.effects,
            origins: compiled.origins,
            world: world_text,
            carried,
        }),
        _ => Err(problems),
    }
}

/// The schema's text, parsed, or the one problem saying it would not parse.
///
/// Both doors take it — [`load`] for the world it loads, [`compile`] for the
/// policies it validates — so both refuse it in the same words.
fn schema_of(source: &str) -> Result<Schema, Vec<Problem>> {
    Schema::from_cedarschema_str(source)
        .map(|(schema, _)| schema)
        .map_err(|error| {
            vec![Problem {
                file: path_of(&Path::new(POLICY).join(SCHEMA)),
                line: 0,
                what: format!("the schema does not parse: {error}"),
                fix: "every entity, attribute and action here appears in \
                      reference/identity-and-authorization.md §4. Correct the transcription \
                      rather than the reference"
                    .to_owned(),
                check: "schema-malformed",
            }]
        })
}

/// Every policy file under `policy/`, with its path and its text.
///
/// The path is relative to `root`. Split out of `load` for its line budget, and
/// it is the right seam: this is about reading files, and [`assemble`] is about
/// what they say once read.
fn read_policies(root: &Path, problems: &mut Vec<Problem>) -> Vec<(String, String)> {
    let mut read = Vec::new();
    for path in policy_files(&root.join(POLICY)) {
        let relative = crate::relative_to(root, &path);
        let Ok(text) = std::fs::read_to_string(&path) else {
            problems.push(Problem {
                file: relative,
                line: 0,
                what: "the policy could not be read".to_owned(),
                fix: "a policy that cannot be read is a rule that will not be enforced \
                      and will not be reported as absent"
                    .to_owned(),
                check: "policy-unreadable",
            });
            continue;
        };
        read.push((relative, text));
    }
    read
}

/// Every policy in `files`, re-keyed by its `@id`.
///
/// Everything here is about what the files say; everything left in
/// [`compile`] is about what the set means once it is assembled.
fn assemble(
    schema_source: &str,
    files: &[(String, String)],
    problems: &mut Vec<Problem>,
) -> (
    PolicySet,
    BTreeMap<String, Effect>,
    BTreeMap<String, String>,
) {
    let mut policies = PolicySet::new();
    let mut effects = BTreeMap::new();
    let mut origins = BTreeMap::new();
    for (relative, text) in files {
        let parsed = match PolicySet::from_str(text) {
            Ok(parsed) => parsed,
            Err(error) => {
                problems.push(Problem {
                    file: relative.clone(),
                    line: 0,
                    what: format!("the policy does not parse: {error}"),
                    fix: "Cedar's own parser is what runs in production, so a policy it \
                          cannot read is a policy that is not in force"
                        .to_owned(),
                    check: "policy-malformed",
                });
                continue;
            }
        };
        every_statement_is_a_policy(text, relative, &parsed, problems);
        for policy in parsed.policies() {
            // **Re-keyed by its `@id`.** `§14.9` requires each journal entry to
            // record which policy decided, and Cedar's own ids are positional —
            // `policy0`, `policy1` — so an entry written today would name a
            // different rule the moment a file was added above it.
            let Some(name) = policy.annotation("id") else {
                problems.push(Problem {
                    file: relative.clone(),
                    // **The file, not a line.** It reported the first `forbid`
                    // in the file, so removing the second `@id` in a two-policy
                    // file sent a reader to the *other* policy — one that is
                    // perfectly well formed. Cedar keeps no span, and
                    // `PolicySet` does not iterate in file order, so there is no
                    // honest line to give: naming the file is right, and naming
                    // the wrong rule is worse than naming none.
                    line: 0,
                    what: "a policy carries no `@id`".to_owned(),
                    fix: "the journal records which policy decided (§14.9), and without \
                          an annotation Cedar names it by position — a number that changes \
                          when a file is added. Give it a stable `@id(\"pack.rule\")`"
                        .to_owned(),
                    check: "policy-without-an-id",
                });
                continue;
            };
            if let Some(refused) = refused_id(name, relative, text) {
                problems.push(refused);
                continue;
            }
            if effects.contains_key(name) {
                problems.push(Problem {
                    file: relative.clone(),
                    line: problem::line_of(text, name).max(1),
                    what: format!("`{name}` is the id of more than one policy"),
                    fix: "an id is what a journal entry names, so two policies sharing \
                          one make an audit record ambiguous for as long as the logs live"
                        .to_owned(),
                    check: "duplicate-policy-id",
                });
                continue;
            }
            // Infallible: `PolicyId`'s `FromStr` cannot fail, and saying so is
            // better than an `else` branch nothing can reach.
            let id = PolicyId::new(name);
            // **A core invariant applies to everyone, or it is not one.**
            // `unless { … || principal is Agent }` on `core.provenance` — an
            // agent acting with no grant at all — and `|| resource is Feature`
            // on the quorum rule were both added with the whole suite green,
            // because every case in the pack used a `User` principal and a
            // `Content` or `User` resource. Covering the space case by case
            // cannot close that: the schema declares three principal types and
            // three resource types, and a permit for every pair is a product
            // decision nobody has made.
            //
            // So the rule is structural: a policy under `policy/core/` does not
            // narrow itself by entity type. A later invariant that genuinely
            // needs to says so with the marker, in the file, where a reader
            // meets it.
            report_narrowings(policy, schema_source, text, name, relative, problems);
            effects.insert(name.to_owned(), policy.effect());
            origins.insert(name.to_owned(), relative.clone());
            if policies.add(policy.clone().new_id(id)).is_err() {
                problems.push(Problem {
                    file: relative.clone(),
                    line: 1,
                    what: format!("`{name}` could not be added to the pack"),
                    fix: "two policies cannot share an id".to_owned(),
                    check: "duplicate-policy-id",
                });
            }
        }
    }

    (policies, effects, origins)
}

/// Why the `@id` `name` of a policy in `relative`, whose text is `text`, is
/// refused: one under the reserved prefix, one of another shape than
/// lower-case words joined by dots, or one whose first word is not its file's
/// pack — asked in that order, and none for an id that is none of them.
fn refused_id(name: &str, relative: &str, text: &str) -> Option<Problem> {
    // **Before anything else is asked of the id.** A reserved one is not
    // a duplicate or a narrowing to be weighed: it is a name the journal
    // cannot tell from a verdict the decision point wrote itself.
    if name.starts_with(RESERVED_PREFIX) {
        return Some(Problem {
            file: relative.to_owned(),
            line: problem::line_of(text, name).max(1),
            what: format!(
                "`{name}` begins with `{RESERVED_PREFIX}`, the prefix the decision \
                 point names its own verdicts with"
            ),
            fix: "a journal entry names the policy that decided, or a sentinel the \
                  decision point writes when none did — the pack could not load, \
                  nothing permitted the request. Both are one column, so a policy \
                  under this prefix could decide an `allow` that reads like a \
                  fail-closed refusal. Name it under its own pack's prefix"
                .to_owned(),
            check: "policy-id-reserved",
        });
    }
    // **And every other id has one shape**, because the prefix is
    // compared as bytes: `Kernel.no_permit_matched`, the sentinel with a
    // space in front, or with a Cyrillic `е`, is another string that a
    // view which folds case, trims or draws the glyph shows as the
    // decision point's own verdict.
    if !id_is_well_formed(name) {
        return Some(Problem {
            file: relative.to_owned(),
            line: problem::line_of(text, name).max(1),
            what: format!("`{name}` is not lower-case words joined by dots"),
            fix: "a journal entry names the policy that decided in the column that \
                  holds the decision point's own `kernel.` sentinels. An id of \
                  lower-case ASCII letters, digits and `_`, each word begun by a \
                  letter, cannot differ from one of those only in case, whitespace or \
                  script. Name it `<pack>.<rule>`"
                .to_owned(),
            check: "policy-id-malformed",
        });
    }
    // **And its first word is the file's pack.** The id rule said
    // `<pack>.<rule>` and held only the alphabet, so a reviewer
    // compiled a family file's `permit` named `core.…` beside one named
    // `org.…`: the journal's column of what decided then names an
    // invariant every pack holds for a rule one pack wrote.
    if let Some(pack) = pack_of(relative)
        && name.split('.').next() != Some(pack)
    {
        return Some(Problem {
            file: relative.to_owned(),
            line: problem::line_of(text, name).max(1),
            what: format!(
                "`{name}` is written in the pack `{pack}`, and its first word names \
                 another"
            ),
            fix: format!(
                "an id is `<pack>.<rule>`, its first word the pack its file is in: a \
                 journal entry names the policy that decided, and an auditor reads \
                 from it whose rule that was — the core's, every pack's, or one \
                 pack's. Name it `{pack}.<rule>`"
            ),
            check: "policy-id-outside-its-pack",
        });
    }
    None
}

/// The pack a policy file at `relative` is written in: `core` under
/// `policy/core/`, the profile under `policy/packs/<profile>/`, and none for a
/// file anywhere else.
fn pack_of(relative: &str) -> Option<&str> {
    if relative.starts_with("policy/core/") {
        return Some("core");
    }
    let (profile, file) = relative.strip_prefix("policy/packs/")?.split_once('/')?;
    (!profile.is_empty() && !file.is_empty()).then_some(profile)
}

/// Whether `id` is lower-case words joined by dots: `family.a_rule`.
///
/// ```text
/// id   = word 1*("." word)
/// word = [a-z] *[a-z0-9_]
/// ```
///
/// At least two words, the pack's and the rule's — and that the first is the
/// pack the file is in is [`pack_of`]'s, asked beside it. It answers an id that reads
/// as a sentinel only once case is folded, whitespace trimmed or a letter from
/// another script drawn; it does not answer a look-alike inside this
/// alphabet, `kerne1.` for `kernel.`, which is another string and a
/// reviewer's to catch.
fn id_is_well_formed(id: &str) -> bool {
    id.split('.').count() >= 2
        && id.split('.').all(|word| {
            let mut characters = word.chars();
            characters
                .next()
                .is_some_and(|first| first.is_ascii_lowercase())
                && characters
                    .all(|rest| rest.is_ascii_lowercase() || rest.is_ascii_digit() || rest == '_')
        })
}

/// Every statement in one file becomes a policy, or the file is refused.
///
/// `PolicySet::from_str` parses a Cedar *template* happily and `policies()`
/// yields nothing for it, so a Cedar file in `policy/core/` written as a
/// template was read as in force and was in force nowhere: no `@id` requirement,
/// no narrowing check, no denial test, and the policy count did not move — so
/// `§19.3`'s floor could not see it either. A core invariant present on disk and
/// enforced by nothing is the worst shape a policy directory can be in.
///
/// **Asked of Cedar's parse, not of the file's text.** This counted
/// `text.matches("@id(\"")` — the four-byte literal — against
/// `policies().count()`, and was wrong in both directions, each by one
/// character. Measured on 2026-09-08:
///
/// ```text
/// @id ("family.a_guest_never_deletes")   on a template, in a new pack file
///   -> check passed; sixteen rules on disk, fifteen counted, and the sixteenth
///      in force nowhere. `@id("…")` without the space is caught at once.
///
/// // a comment above the annotation mentioning `@id("core.fail_closed")`
///   -> policy/core/fail_closed.cedar:1 refused, `load` returns `Err`, and
///      `xtask::policy::check` returns before every later policy check —
///      32 of the 36 tests in criterion 11's harness failing on the wrong one.
/// ```
///
/// `fail_closed.cedar` already writes ``**Every policy carries an `@id`.**`` one
/// character from tripping it. What the paragraph above actually claims is that
/// no statement here is a *template*, and Cedar answers that itself: a policy
/// set holds policies and templates, and `templates()` is the second half.
/// Neither a space nor a comment can move it.
fn every_statement_is_a_policy(
    text: &str,
    relative: &str,
    parsed: &PolicySet,
    problems: &mut Vec<Problem>,
) {
    // The text is still read, for the message alone: a reader of the refusal
    // wants to know how many statements they wrote.
    let declared = text.matches("@id").count();
    let templates = parsed.templates().count();
    if templates == 0 {
        return;
    }
    problems.push(Problem {
        file: relative.to_owned(),
        line: 1,
        what: format!(
            "Cedar reads {templates} template(s) here, of {declared} statement(s) \
             annotated in the file"
        ),
        fix: "a statement Cedar parses but does not return as a policy is enforced by \
              nothing — a Cedar *template* is the usual way to write one by accident, and \
              it takes `?principal` or `?resource`. Every statement in `policy/` is a \
              policy, and a template belongs to a pack that links it rather than to the \
              invariant core"
            .to_owned(),
        check: "policy-is-not-in-force",
    });
}

/// Every way one core policy turns itself off, reported.
///
/// A policy outside `policy/core/` narrows itself all the time — that is what a
/// pack is for — so only the core is checked.
fn report_narrowings(
    policy: &Policy,
    schema: &str,
    text: &str,
    name: &str,
    relative: &str,
    problems: &mut Vec<Problem>,
) {
    // **A pack's `forbid` is a floor, and a floor may not choose whose.** This
    // read `policy/core/` only, and the mutation loop beside it proposes
    // conjuncts rather than scopes — so a pack's `forbid` fell between them,
    // and narrowing its scope to one named principal moved no number anywhere.
    // Measured against the shipped pack:
    // `family.giving_out_encrypted_content_needs_quorum` — whose own paragraph
    // calls handing out encrypted content "the most irreversible thing this
    // schema can express" — scoped to `principal == User::"ana"` left
    // `40 case(s) … decide as written`, `15 policy(ies) each have a denial
    // test` and `check passed`, while a decision probe went from `Deny` to
    // `Allow` for a *different* user's private content, with no quorum. Its
    // denial case names ana, so removing the policy is still noticed and
    // nothing else was.
    //
    // **Per policy, written in the file — not per kind of constraint.** This
    // read `principal` alone, on the argument that a pack rule says which
    // actions and which resources it is about and reporting those would refuse
    // it the right to have a subject. `policy/core/README.md` records that exact
    // argument being tried for the core and being wrong: *"An action constraint
    // used to be exempt wholesale, and that was a hole … `core.provenance`
    // rescoped to `action == Action::\"Read\"` gave every `Write`, `Delete`,
    // `Share`, `Grant` and `Revoke` a pass."* One rule written twice with the
    // second copy narrower is this tree's commonest defect, and this was it.
    //
    // Measured on the shipped pack: `family.giving_out_encrypted_content_needs_quorum`
    // rescoped from `resource` to `resource in Folder::\"household\"` left every
    // gate number byte-identical — 7 core files, 15 policies, 40 cases, 15
    // denial tests, `check passed` — while restricted content in any *other*
    // folder was handed out with no quorum at all. Folders are user-created and
    // the shipped world has one, which is the only reason no case noticed.
    //
    // So every axis is reported and the exemption is the written one: a
    // narrowing a policy is entitled to carries `nc:narrows:` naming the
    // constraint, the same hatch the core uses and the same census counts it.
    let core = relative.starts_with("policy/core/");
    let a_pack_floor = !core && policy.effect() == Effect::Forbid;
    if !core && !a_pack_floor {
        return;
    }
    let rendered = policy.to_string();
    for (where_, narrowing, target) in narrows_itself(policy, schema) {
        // **A type test is a rule having a subject; naming an entity is taking
        // something out from under the floor.** The operator is what tells them
        // apart, which this function already knows — `is in` is split from `is`
        // ten lines up for exactly that reason. `resource is Content` says what
        // kind of thing the rule governs and every pack rule needs to;
        // `resource in Folder::"household"`, `principal == Agent::"ana"` and
        // `resource == Content::"c-1"` each pick out particular entities and
        // leave the rest of the class uncovered.
        //
        // Reported per *operator* rather than per axis, so the axis argument
        // this paragraph used to make cannot come back: it is why a `resource`
        // rescope walked through, and refusing the axis wholesale instead would
        // refuse `forbid (principal is Agent, …)`, which is an ordinary pack
        // rule with a subject. Only for a pack's floor — a core invariant holds
        // for every type too, and says so in its own branch.
        //
        // **In the scope, or in a condition beside a read of the same subject,
        // and nowhere else.** The exemption reached every type test in a `when`,
        // and the mutation loops never drop a disjunct. So the frozen-account
        // floor with `principal.status == "suspended"` rewritten as
        // `(principal.status == "suspended" && resource is Content)` printed
        // the policy block byte-identical and `check passed`, and a suspended
        // account had its undo and redo back. That type test says what the
        // floor is about for one disjunct only, which takes the other kinds of
        // resource out from under the floor. A type test in a condition stays
        // excused where it guards a read, as in
        // `resource is Content && resource.sensitivity`: the read would error
        // on another kind of resource, and Cedar skips a policy whose condition
        // errors. That is the shape `guards_a_dereference` recognises. A type
        // test that guards nothing belongs in the scope, on the policy's first
        // line.
        //
        // **What it still does not see:** a type test on a subject the policy
        // reads *elsewhere* is excused wherever it stands, because the question
        // is asked of the whole rendering. The cases are what stand behind
        // that: each floor's are written on every kind of resource a permit
        // reaches.
        if a_pack_floor
            && a_type_test(&narrowing)
            && (where_ == Narrowing::Scope || guards_a_dereference(&narrowing, &rendered))
        {
            continue;
        }
        if where_ == Narrowing::Scope
            && crate::references_marked(text, name, &constraint_of(&narrowing), &target)
        {
            continue;
        }
        problems.push(Problem {
            file: relative.to_owned(),
            line: problem::line_of(text, name).max(1),
            what: if core {
                format!("`{name}` is a core invariant and turns itself off for {narrowing}")
            } else {
                // **A pack's `forbid` is not a core invariant**, and saying so
                // sent a reader to `policy/core/` for a rule that is not there.
                format!("`{name}` is a pack's floor and leaves part of its own class uncovered, by {narrowing}")
            },
            fix: "a core invariant holds for every principal, every action and every \
                  resource the schema declares — that is what makes it a floor rather than \
                  a rule one pack can step around. A constraint in the *scope* can be \
                  excused with `nc:narrows: <reason>` in a comment above it, and **the \
                  reason has to quote the constraint it excuses, backticks and all** — \
                  a reason for `principal is` is a different rule from one for \
                  `principal ==`, and naming only the subject covered both. When the \
                  constraint names an entity, the quote either stops at the operator or \
                  names every entity it names — all of a list, not one verb of it: a \
                  reason arguing about `Device` is not a reason \
                  about `principal is Agent`. Anywhere on the marker's own line, not \
                  only at its start — and on that line, because a paragraph under a \
                  marker quoting a constraint to warn against it excused that very \
                  constraint. One written about the \
                  action used to cover a `principal` constraint added later, which turned \
                  invariant 5 off for every agent. A narrowing in a `when` or an `unless` \
                  cannot be excused at all: that is where an exemption hides. A pack's \
                  floor may say what kind of thing it is about in its scope, and in a \
                  condition only beside the read of that subject's attribute it guards; \
                  a type test that guards nothing goes on the policy's first line"
                .to_owned(),
            check: "core-narrows-by-entity-type",
        });
    }
}

/// The scope constraint a narrowing message is about: the subject and its
/// operator.
///
/// The message is built by `narrows_itself` as ``\`principal ==\` in its scope``,
/// so what sits inside the backticks is the constraint — read back rather than
/// threaded through, because the message is what a reader sees and the two
/// cannot then disagree.
///
/// **The subject alone is not the constraint, and this returned the subject.**
/// `principal is Agent` is a type test — the delegation shape `§14.6` allows —
/// and `principal == Agent::"ana-assistant"` is invariant 1 holding for exactly
/// one entity in the world. Both are "the principal", so one marker excused the
/// other: measured, `core.an_agent_never_impersonates` rescoped to a single
/// agent left the reason it already carries intact and `check passed`.
/// `widenings` runs on permits only, so nothing else looks. What tells the two
/// apart is the operator, which is why it is part of what a reason must name.
fn constraint_of(narrowing: &str) -> String {
    narrowing
        .trim_start_matches('`')
        .split('`')
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned()
}

/// The entities a scope constraint names, as a reader would write them.
///
/// `principal is Agent` names `Agent`; `principal == Agent::"ana-assistant"` and
/// `principal in Realm::"home"` name the literal. Empty when the node names
/// none, which is every shape where the operator is the whole constraint.
///
/// **And a set is a name too, which is where this was blind.** Cedar renders
/// `action in [Action::"Grant", Action::"Revoke"]` with an `entities` array and
/// no `entity` at all, so this answered the empty string for it — and the rule
/// downstream then had exactly one arm left, the one that accepts a reason
/// stopping at the operator. Measured on 2026-09-11: the family pack's marker
/// rewritten from `` `action in` `` to the list its policy actually names was
/// **refused**, by a `fix` telling the author to name the entity they had just
/// named. Every shipped marker passes through the short form, so nothing in the
/// tree had ever exercised the other arm.
///
/// Several names come back separated by spaces, which is how the caller reads
/// them: it splits on whitespace and asks that each one be quoted.
fn the_entity_named(node: Option<&serde_json::Value>) -> String {
    let Some(node) = node else {
        return String::new();
    };
    if let Some(kind) = node.get("entity_type").and_then(|kind| kind.as_str()) {
        return kind.to_owned();
    }
    if let Some(entities) = node.get("entities").and_then(|set| set.as_array()) {
        let named: Vec<String> = entities
            .iter()
            .filter_map(|one| {
                match (
                    one.get("type").and_then(|kind| kind.as_str()),
                    one.get("id").and_then(|id| id.as_str()),
                ) {
                    (Some(kind), Some(id)) => Some(format!("{kind}::\"{id}\"")),
                    _ => None,
                }
            })
            .collect();
        if !named.is_empty() {
            return named.join(" ");
        }
    }
    let Some(entity) = node.get("entity") else {
        return String::new();
    };
    match (
        entity.get("type").and_then(|kind| kind.as_str()),
        entity.get("id").and_then(|id| id.as_str()),
    ) {
        (Some(kind), Some(id)) => format!("{kind}::\"{id}\""),
        _ => String::new(),
    }
}

/// The test a core policy narrows itself with, if it has one.
///
/// # Read from the AST, not from the rendered text
///
/// This matched strings in `policy.to_string()`, and every fix it took was
/// another spelling: a string literal that happened to contain `principal is a`,
/// a schema comment that made the attribute rule vacuous, `X has attr` as a type
/// test in different words. The last one was the tell — the check could always
/// be defeated by writing the same thing another way, and `unless {
/// context.via_agent }` did it with one token that is neither `is` nor `has`.
///
/// Cedar renders a policy to an EST — a real syntax tree — through
/// [`Policy::to_json`]. Reading that removes the whole class: a string literal
/// is a `Value` node and cannot be mistaken for an operator, a comment is not in
/// the tree at all, and a shape this does not know about is a node kind rather
/// than a spelling it failed to anticipate.
///
/// # What counts as narrowing
///
/// A core invariant is a floor: it holds for every principal and every resource
/// the schema declares, or it is not one. Four shapes turn it off for a class of
/// requests, and each is reported by the name a reader would use:
///
/// * a **constrained scope** — `forbid (principal == Agent::"x", …)`;
/// * an **`is`** against an entity type;
/// * a **`has`** against an attribute that not every entity type declares, which
///   is an `is` in different words: `Agent` is the only type with
///   `budget_remaining`, so `principal has budget_remaining` is an exact test for
///   "is an Agent";
/// * an **`==` or `in`** against an entity literal;
///
/// A **context** condition is not one of them, and `narrowing_in` says why.
fn narrows_itself(policy: &Policy, schema: &str) -> Vec<(Narrowing, String, String)> {
    let Ok(json) = policy.to_json() else {
        return Vec::new();
    };
    let mut found = Vec::new();
    // **The conditions first, and the order is load-bearing.** A policy with a
    // marked scope constraint and an unmarked condition reported the scope, was
    // excused, and the condition was never looked at.
    if let Some(conditions) = json.get("conditions").and_then(|node| node.as_array()) {
        for condition in conditions {
            if let Some(body) = condition.get("body") {
                for narrowing in narrowing_in(body, schema) {
                    found.push((Narrowing::Condition, narrowing, String::new()));
                }
            }
        }
    }
    // **Every scope subject, not the first one that is constrained.** This
    // returned at the first non-`All` of the three, so a marker excusing that
    // one left the other two unexamined — measured, eight of nine scope
    // constraints added across the marked core policies went unreported, and
    // `forbid (principal, action == Action::"Delete", resource is Content)`
    // means deleting a `User` or a `Feature` needs no quorum. That is the
    // file's own worked example, moved from the condition to the scope.
    for subject in ["principal", "action", "resource"] {
        let Some(op) = json
            .get(subject)
            .and_then(|node| node.get("op"))
            .and_then(|op| op.as_str())
        else {
            continue;
        };
        if op != "All" {
            // **`principal is X in Y` renders as `op: "is"` with an `in`
            // beside it**, and reading only `op` collapsed it onto the plain
            // type test — so `principal is Agent in Realm::"home"` was excused
            // by the shipped marker written for `principal is Agent`, and the
            // invariant held in one realm and was absent in every other one a
            // deployment creates. `policy/world.json` has two realms and every
            // agent lives in one of them, so no case could see it either. The
            // operator is what tells a rule from a tenant scoping, and both
            // words of it are the operator.
            let combined = json
                .get(subject)
                .is_some_and(|node| node.get("in").is_some());
            let op = if combined && op == "is" { "is in" } else { op };
            // **And the entity the constraint names**, which is the whole of
            // what a type test *is*. A marker's reason may quote the subject and
            // the operator and then argue about a different type entirely —
            // measured, a marker whose reason quoted `principal is Device` excused
            // `core.budget_is_a_permission`, whose scope is `principal is
            // Agent`, with the whole policy suite green and `Device` naming a
            // type that appears nowhere in that file.
            found.push((
                Narrowing::Scope,
                format!("`{subject} {op}` in its scope"),
                the_entity_named(json.get(subject)),
            ));
        }
    }
    found
}

/// Whether a narrowing is a type test rather than a named entity.
///
/// `` `resource is` `` in a scope and `` `resource is Content` `` in a condition
/// are both a rule saying what kind of thing it is about. `` `resource in` ``,
/// `` `resource ==` `` and `` `principal is in` `` each name an entity, and a
/// floor that names one leaves every other member of the class uncovered.
///
/// `is in` is deliberately not a type test: `principal is Agent in Realm::"home"`
/// renders as an `is` with an `in` beside it, and reading it as the former is how
/// an invariant came to hold in one realm and be absent in every other.
fn a_type_test(narrowing: &str) -> bool {
    let inside = narrowing.trim_start_matches('`');
    let inside = inside.split('`').next().unwrap_or_default();
    let mut words = inside.split_whitespace();
    words.next();
    // Every word after the operator, not a substring of the whole: the scope
    // form is `principal is in` with nothing after it, so a `" in "` with two
    // spaces never matched and the tenant scoping read as a plain type test.
    words.next() == Some("is") && !words.any(|word| word == "in")
}

/// The marker that excuses a core policy's scope constraint.
///
/// A `pub const` because the census in `xtask/src/check.rs` counts every gate's
/// escape hatch and this one was not among them: switching off a core Cedar
/// invariant with a marker moved no number anywhere, in the one line whose
/// stated job is "one per gate that has one". It had already been wrong once,
/// the same way, by omitting `nc:bounded:`.
pub const NARROWS: &str = "nc:narrows:";

/// Where a narrowing came from, which decides whether the marker can excuse it.
///
/// **`nc:narrows:` excuses a scope constraint and never a condition.** The
/// marker used to excuse the whole policy, so writing one for
/// `core.destructive_needs_quorum`'s `action == Action::"Delete"` — a real,
/// documented proxy — also excused a `|| resource is Feature` smuggled into its
/// condition, which is one person removing a platform capability alone. A scope
/// constraint is visible on the policy's first line and is what somebody means
/// when they say a rule is about one kind of thing; a condition that turns the
/// rule off for a subset is the shape this check exists for, and nothing about
/// it should be excusable by a comment written for something else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Narrowing {
    /// `forbid (principal == …, …)` — on the policy's own first line.
    Scope,
    /// `when`/`unless` — where an exemption hides.
    Condition,
}

/// The narrowing in one EST node, if there is one.
///
/// Every child is walked, in any position: a test on a scope variable narrows
/// the rule wherever in an expression it sits, so there is nothing here that
/// needs to know whether a node is being read as a value or as a condition.
fn narrowing_in(node: &serde_json::Value, schema: &str) -> Vec<String> {
    /// The scope variables this rule is about.
    ///
    /// **`context` is deliberately not among them**, and that is a limit worth
    /// stating rather than an oversight. A context condition is how every one of
    /// these invariants is written — `core.provenance` is
    /// `forbid … unless { context has grant }` — and a check that called that a
    /// narrowing would be reporting the invariant for doing exactly what it
    /// exists to do. Whether a particular context escape is the rule's own
    /// definition or a way around it is a question about meaning, and nothing
    /// structural separates `unless { context has grant }` from
    /// `unless { context.via_agent }`. `policy/core/README.md` records it as a
    /// limit of this gate rather than leaving it to be discovered.
    const SUBJECTS: [&str; 3] = ["principal", "action", "resource"];

    /// Operators whose operands are whole conditions rather than values.
    ///
    /// Gathering "a subject somewhere, an entity somewhere" across one of these
    /// fired on `principal == resource || (context has grant &&
    /// context.grant.granted_by == User::"ana")` — two halves the gate accepts
    /// individually — and named `&&` as the offending operator, pointing at the
    /// wrong node. Their children are judged on their own by the descent below.
    const COMBINATORS: [&str; 4] = ["&&", "||", "!", "if-then-else"];

    // `?` is safe on these two: there are no children to descend into when the
    // node is not an object with a single operator key.
    let Some((kind, body)) = node.as_object().and_then(|node| node.iter().next()) else {
        return Vec::new();
    };
    // **Whichever side *mentions* a scope variable, not whichever side *is*
    // one.** This read `body["left"]["Var"]` and gave up with `?` when the left
    // operand was anything else — which returned from the whole function, so the
    // child walk below never ran either. Both halves of that were defeatable by
    // one wrapper: `(if true then principal else principal) is Agent` is the
    // same test as `principal is Agent`, and it loaded clean. Descending would
    // not have been enough on its own — the `is` lives *above* the wrapper, and
    // what is underneath is a bare `Var` that narrows nothing by itself.
    let subject = |at: &str| {
        body.get(at)
            .and_then(|side| mentions_a_subject(side, &SUBJECTS))
    };
    // **A negated `has` is a fail-closed guard *in the disjunction it was
    // written for*, and a narrowing anywhere else.**
    // `!(principal has budget_remaining) || principal.budget_remaining <= 0`
    // makes the `forbid` apply to *every* principal, including one whose type
    // does not declare the attribute — which is what a floor has to do when the
    // decision point forgets the schema. Cedar skips a policy whose condition
    // *errors*, so a bare `principal.budget_remaining` is not merely unguarded:
    // a request without the attribute is allowed by the rest of the pack.
    // Measured without the schema passed to the authorizer:
    //
    // ```text
    // with the schema:    refused: expected the record to have `policy_epoch`
    // without the schema: Allow   errors: ["… record does not have the attribute …"]
    // ```
    //
    // **This excused the shape and not the use of it, and that was a hole big
    // enough to turn an invariant off.** The early return fired on the `!` node
    // wherever it stood, so `when { !(principal has node_key) }` — written
    // alone, as a whole condition — read as a fail-closed guard while saying the
    // exact opposite: `node_key` is declared only by `Device`, so it is
    // `principal is not a Device` in different words, and appending it to
    // `core.fail_closed` left invariant 1 off for every principal but a device
    // with the whole gate green and the policy block byte-identical. Three more
    // spellings did the same to invariants 2, 4 and 5 — the last of them
    // `!(resource has tier)` on `core.quorum`, which is verbatim the worked
    // example `policy/core/README.md` names as the reason this gate exists.
    // The README already claimed the shape was caught; it was prose stating the
    // inverse of the code.
    //
    // So the excuse now belongs to the `||`, which is where both halves are
    // visible. The guard is skipped and **the other half is still judged**,
    // because it is an ordinary predicate and a narrowing hidden there would
    // otherwise ride in behind the guard. A negated `has` standing anywhere else
    // falls through to the descent below, which reaches the `has` inside it and
    // reports it — the same verdict the unnegated spelling gets, which is right,
    // because they are the same test.
    if kind == "||"
        && let Some(left) = body.get("left")
        && let Some(right) = body.get("right")
    {
        let beside_a_guard = |guard: &serde_json::Value, other: &'_ serde_json::Value| {
            the_attribute_a_negated_has_tests(guard, &SUBJECTS)
                .is_some_and(|attribute| dereferences(other, &attribute))
        };
        if beside_a_guard(left, right) {
            return narrowing_in(right, schema);
        }
        if beside_a_guard(right, left) {
            return narrowing_in(left, schema);
        }
    }
    match kind.as_str() {
        "is" => {
            if let Some(named) = subject("left")
                && let Some(entity_type) = body.get("entity_type").and_then(|kind| kind.as_str())
            {
                // **And the `in` beside it, which the scope reader splits and
                // this one dropped.** `principal is Agent in Realm::"home"`
                // renders as an `is` with an `in` hanging off it; the scope
                // reader has carried a paragraph about that since a shipped
                // marker written for `principal is Agent` excused it and an
                // invariant held in one realm and was absent in every other.
                // Here the same shape was rendered as a bare type test — and
                // once a pack's floor was allowed to be a type test, that made
                // `resource is Content in Folder::"household"` in a `when`
                // invisible. Measured on the shipped floor: every `ok` line
                // byte-identical, no case able to see it because the world has
                // one folder, and the moment a deployment creates a second the
                // floor stops holding for what is in it.
                let combined = body.get("in").is_some();
                let operator = if combined { "is in" } else { "is" };
                return vec![format!("`{named} {operator} {entity_type}`")];
            }
        }
        "has" => {
            if let Some(named) = subject("left")
                && let Some(attribute) = body.get("attr").and_then(|attr| attr.as_str())
                && !declared_by_every_entity(schema, attribute)
            {
                return vec![format!("`{named} has {attribute}`")];
            }
        }
        _ => {}
    }
    // **Any operator that puts a scope variable and an entity literal together
    // is a membership test**, whatever it is called. This matched `==` and `in`
    // by name and descended only into *object* children — and a `Set`'s body is
    // a JSON array, so `[Agent::"a"].contains(principal)` was invisible. It is
    // the same test as `principal == Agent::"a"`, which the gate's own doc lists
    // as one it catches, and it turned invariant 1 off for `Delete`, `Grant` and
    // `Revoke` with the whole policy section green. `containsAny` and
    // `containsAll` are the same shape again.
    //
    // Naming operators was the mistake, twice over: the first version named four
    // and missed three, and a Cedar release adding a fourth membership operator
    // would have re-opened it. What matters is what the operands are.
    // **Not the boolean combinators**, for the reason `COMBINATORS` gives.
    if !COMBINATORS.contains(&kind.as_str())
        && let Some(operands) = body.as_object()
    {
        let mut mentions = None;
        let mut entity = false;
        for operand in operands.values() {
            if mentions.is_none() {
                mentions = mentions_a_subject(operand, &SUBJECTS);
            }
            entity = entity || names_an_entity(operand);
        }
        if let Some(named) = mentions
            && entity
        {
            return vec![format!("`{named} {kind}` an entity literal")];
        }
    }
    // And every child, **arrays included**: a `Set`'s body is an array, so
    // descending only into objects stopped the walk at every set boundary. A
    // narrowing wrapped in `&&`, `!`, `if-then-else`, a set literal, or anything
    // Cedar adds later is still a narrowing.
    // **Every child, not the first that answers.** This returned at the first
    // one, so a condition written `resource is Content && resource == Content::"x"`
    // reported only the type test — and once a pack's floor was allowed to *be* a
    // type test, the rest of the condition was never examined at all. Measured on
    // the shipped floor: `resource.sensitivity == "restricted"` replaced by
    // `resource == Content::"diary"` left every `ok` line byte-identical and
    // handed a guardian somebody else's private content with no quorum. The same
    // constraint written one conjunct *earlier* was reported — the verdict
    // depended on the order the conjuncts happened to be in.
    let mut found = Vec::new();
    for child in children(body) {
        found.extend(narrowing_in(child, schema));
    }
    found
}

/// The attribute a `!(subject has attr)` node tests, when a node is that.
///
/// Written as its own reader because the shape has to be recognised exactly:
/// `narrowing_in` used to excuse *any* `!` over a `has`, wherever it stood, and
/// the header there records what that cost. A `!` over anything else, a `has` on
/// something that is not a scope subject, and a `!` over a `has` nested inside a
/// larger expression all answer `None` — the last because what is excused is one
/// operand of a `||`, and a guard buried deeper is not that operand.
fn the_attribute_a_negated_has_tests(
    node: &serde_json::Value,
    subjects: &[&str],
) -> Option<String> {
    let (kind, body) = node.as_object()?.iter().next()?;
    if kind != "!" {
        return None;
    }
    let operand = body.get("arg").or_else(|| body.get("inner"))?;
    let (inner, fields) = operand.as_object()?.iter().next()?;
    if inner != "has" {
        return None;
    }
    mentions_a_subject(fields.get("left")?, subjects)?;
    fields
        .get("attr")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
}

/// Whether a node reads `attribute` off something, anywhere inside it.
///
/// **An attribute *access*, not a `has`.** Both spell an `attr` field in the
/// EST, and accepting either would excuse `!(principal has node_key) ||
/// (resource has node_key)` — two type tests wearing a guard's clothes. What
/// makes the disjunction a floor is that the half beside the guard *uses* the
/// value the guard proved is there.
fn dereferences(node: &serde_json::Value, attribute: &str) -> bool {
    if let Some(fields) = node.as_object()
        && let Some((kind, body)) = fields.iter().next()
        && kind == "."
        && body
            .get("attr")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|named| named == attribute)
    {
        return true;
    }
    children(node)
        .iter()
        .any(|child| dereferences(child, attribute))
}

/// Every child of an EST node, whether it is held in an object or an array.
fn children(node: &serde_json::Value) -> Vec<&serde_json::Value> {
    match node {
        serde_json::Value::Object(fields) => fields.values().collect(),
        serde_json::Value::Array(items) => items.iter().collect(),
        _ => Vec::new(),
    }
}

/// Whether an EST node is, or contains, an entity literal.
///
/// **Anywhere inside it**, because `principal in [Group::"role.guardian"]` is
/// the same test as `principal in Group::"role.guardian"` and the square
/// brackets put the literal one level down, inside a `Set`. That pair of
/// brackets turned the rule off on the shipped schema.
fn names_an_entity(node: &serde_json::Value) -> bool {
    if node.get("__entity").is_some() {
        return true;
    }
    match node {
        serde_json::Value::Object(fields) => fields.values().any(names_an_entity),
        serde_json::Value::Array(items) => items.iter().any(names_an_entity),
        _ => false,
    }
}

/// The first scope variable named anywhere inside an EST node.
///
/// A wrapper does not change what a test is about: `(if true then principal else
/// principal)`, `principal.owner` and a `Set` holding one all still name the
/// principal, and a check that only recognised a bare `Var` was defeated by any
/// of them.
fn mentions_a_subject(node: &serde_json::Value, subjects: &[&str]) -> Option<String> {
    if let Some(named) = node.get("Var").and_then(|var| var.as_str())
        && subjects.contains(&named)
    {
        return Some(named.to_owned());
    }
    match node {
        serde_json::Value::Object(fields) => fields
            .values()
            .find_map(|child| mentions_a_subject(child, subjects)),
        serde_json::Value::Array(items) => items
            .iter()
            .find_map(|child| mentions_a_subject(child, subjects)),
        _ => None,
    }
}

/// Cedar or schema source with its `//` comments removed.
fn without_line_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether every entity type in the schema declares `attribute`.
///
/// An attribute only some types have is a test for those types. One every type
/// has — there are none today — would be an ordinary condition.
fn declared_by_every_entity(schema: &str, attribute: &str) -> bool {
    let (types, declaring) = walk_entities(schema, attribute);
    types > 0 && declaring == types
}

/// How many entity types the schema declares.
///
/// Split out of [`declared_by_every_entity`] so a test can compare it with the
/// file: the walk counted three of the schema's own sentences as types, and the
/// only symptom was a rule that never returned `true` — a miscount that makes
/// the check vacuous is invisible from the check's own answer.
#[cfg(test)]
fn entity_types(schema: &str) -> usize {
    walk_entities(schema, "").0
}

/// The entity types in the schema, and how many declare `attribute`.
///
/// **Each declaration ends at its own `;`.** This looked for the next `{` and
/// treated whatever it found as the entity's attribute block — so
/// `entity Group in [Realm];`, which has none, borrowed the *next* entity's
/// block and then skipped past it: the shipped schema declares nine types and
/// the walk saw seven. Two of them were invisible to a rule about what every
/// type declares.
///
/// Comments are stripped here rather than by the caller, because a schema
/// arriving with them was the first version of the same defect — three of the
/// schema's own sentences were counted as types.
fn walk_entities(schema: &str, attribute: &str) -> (usize, usize) {
    let schema = without_line_comments(schema);
    let mut types = 0_usize;
    let mut declaring = 0_usize;
    let mut rest = schema.as_str();
    while let Some(at) = rest.find("entity ") {
        rest = &rest[at + "entity ".len()..];
        types += 1;
        let open = rest.find('{');
        let ends = rest.find(';');
        // A declaration with no block: `entity Group in [Realm];`. The `;` comes
        // first, or there is no `{` left in the file at all.
        let Some(open) = open.filter(|open| ends.is_none_or(|ends| *open < ends)) else {
            match ends {
                Some(ends) => rest = &rest[ends..],
                None => break,
            }
            continue;
        };
        let Some(close) = rest[open..].find("};").map(|close| open + close) else {
            break;
        };
        let block = &rest[open..close];
        // `"attr":` or `"attr"?:` — the optional marker is part of the name's
        // punctuation, not of the name.
        if block.contains(&format!("\"{attribute}\":"))
            || block.contains(&format!("\"{attribute}\"?:"))
        {
            declaring += 1;
        }
        rest = &rest[close..];
    }
    (types, declaring)
}

/// Whether one backtick-quoted span of `reason` opens with `constraint`.
///
/// The span's first word must be the subject, and every remaining word of the
/// constraint must follow it in order — so `` `principal is` `` and
/// `` `principal is Agent` `` both satisfy the constraint `principal is`, while
/// `` `principal ==` `` satisfies neither and `` `principal is Agent` ``
/// satisfies neither `principal ==` nor `principal is in`.
///
/// **In order, and not merely present**: an operator is two words for
/// `principal is X in Y` and one for the rest, and a reason that names the
/// entity between them is the reason an author actually writes.
fn quotes_the_constraint(reason: &str, constraint: &str, target: &str) -> bool {
    let wanted: Vec<String> = constraint
        .to_lowercase()
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    let Some((subject, operator)) = wanted.split_first() else {
        return false;
    };
    let Some(first) = operator.first() else {
        return false;
    };
    let folded = reason.to_lowercase();
    let target = target.to_lowercase();
    // **Every quoted span, and each judged whole.** Reading only the span the
    // reason opens with was the answer to a reason that quotes the thing it
    // warns *against* — `core.no_self_escalation`'s paragraph quotes
    // `action == Action::"Read"` to say what once went wrong there — and it made
    // the rule wrong the other way at once: an author who names the type before
    // the constraint, which is how the sentence reads best, was refused by a
    // `fix` telling them to quote what they had just quoted. What separates the
    // two is not *where* the span sits but *what it says*, which is the rule
    // below: the subject, the operator, and — when the constraint names an
    // entity — that entity and no other.
    //
    // That was not the whole of it, and the same paragraph proved it: this rule
    // reads what it is given, and `references_marked` was giving it the whole
    // comment run, so the warning four lines under the marker still matched on
    // its own terms. Where is now the caller's question — it hands over the
    // marker's line — and what remains here is what a span says. Neither half
    // is sufficient alone.
    folded.split('`').skip(1).step_by(2).any(|span| {
        let words: Vec<&str> = span.split_whitespace().collect();
        if words.first() != Some(&subject.as_str()) || words.get(1) != Some(&first.as_str()) {
            return false;
        }
        // **The operator's second word, and its absence.** Cedar renders
        // `principal is X in Y` as `is` with an `in` beside it, so the
        // constraint is two words — and `rest.any(…)` scanned forward, which let
        // the fuller quote for *that* shape also satisfy the one-word `principal
        // in`. A policy legitimately scoped to a realm and then rescoped to a
        // group would keep its marker. So the second word must be there when the
        // constraint has one, and must **not** be there when it does not.
        let carries_the_second = words[2..].contains(&"in");
        let operator_matches = match operator.get(1) {
            Some(second) => words[2..].contains(&second.as_str()),
            None => first != "is" || !carries_the_second,
        };
        if !operator_matches {
            return false;
        }
        // **And the entity, which is what a type test is.** `principal is` is
        // the shape; `principal is Agent` is the rule. A reason arguing about
        // `Device` excused a policy narrowed to `Agent` — a type appearing
        // nowhere in that file — because everything past the operator was
        // unread. So a span that stops at the operator is the constraint named
        // in the shortest form there is, and a span that goes on has to go on
        // with the entity this policy actually names.
        //
        // **Every name it names, and a list names several.** `target` is one
        // name for `principal is Agent` and two for
        // `action in [Action::"Grant", Action::"Revoke"]`, and a reason that
        // quotes the constraint as the policy writes it has to carry all of
        // them — quoting one verb of a two-verb list is a reason for a
        // constraint this policy does not have. Extra words stay legal: the
        // `is in` shape renders as `principal is Agent in Realm::"home"`, whose
        // realm is past the entity and was always allowed to be there.
        //
        // Brackets and commas are punctuation of the list and not part of a
        // name, so they are trimmed before the comparison. Without that, the
        // only spelling a reader would ever write — the list copied off the
        // policy's own first line — is the one spelling refused.
        let rest: Vec<&str> = words[2..]
            .iter()
            .copied()
            .map(|word| word.trim_matches(|char| "[],".contains(char)))
            .filter(|word| !word.is_empty())
            .filter(|word| Some(*word) != operator.get(1).map(String::as_str))
            .collect();
        //
        // **And no more than it names, where more is a different rule.** The
        // test above is a subset test, and a subset accepts a reason naming a
        // *larger* list than the policy has — so a marker written honestly for
        // `action in [Action::"Grant", Action::"Revoke"]` went on excusing that
        // policy after somebody rescoped it to `[Action::"Grant"]`. Measured on
        // 2026-09-11: invariant 4 narrowed that way produced no
        // `core-narrows-by-entity-type` at all, with the marker still naming the
        // verb the invariant had just stopped covering. That is the drift a
        // marker per constraint exists to catch, and it is the same sentence as
        // the one-verb direction, which was refused.
        //
        // The exception is the type test, and it is why the subset rule was
        // written: Cedar renders `principal is Agent in Realm::"home"` as an
        // `is` with an `in` beside it, so the realm is an extra word of one
        // constraint rather than a second entity of the same kind. An `is` may
        // carry more; an `==` or an `in` is exactly the entities it names.
        let named: Vec<&str> = target.split_whitespace().collect();
        let no_more = first == "is" || rest.iter().all(|word| named.contains(word));
        rest.is_empty()
            || (!named.is_empty() && named.iter().all(|one| rest.contains(one)) && no_more)
    })
}

/// Whether a comment above `name` in `text` marks it as a deliberate exception.
///
/// **The annotation, not the first place the file writes it.** The anchor was
/// `text.find("@id(\"<name>\")")` — the first *text* occurrence — so ordinary
/// prose naming the policy's own id moved it, and both directions were live.
/// Measured on 2026-09-08, two comment lines added under
/// `policy/core/no_self_escalation.cedar`'s header, saying that every journal
/// entry it writes records its own `@id`, with the policy and its existing
/// marker untouched:
///
/// ```text
/// error policy/core/no_self_escalation.cedar:3 — `core.no_self_escalation` is a
///       core invariant and turns itself off for `action in` in its scope
/// ```
///
/// — reported against the *comment*, telling the author to write the marker they
/// had already written, and `load` then returns `Err` so every later step of the
/// policy gate returns early. Every file in `policy/core/` writes prose about
/// `@id`; four of them carry the sentence *"Every policy carries an `@id`"*, one
/// character from tripping it.
///
/// And the other way: the marker moved to the top of the file with such a
/// sentence under it left `check passed` while a `forbid` narrowed to two
/// actions carried its excuse thirty lines away, behind the header — which is
/// the opposite of what the hatch's own argument rests on, that a constraint in
/// the scope is where a reader meets it.
///
/// So the anchor is the last `@id("<name>")` that *begins a line*, which is
/// where an annotation is written and where prose is not.
/// `every_statement_is_a_policy` records this exact class costing a run once
/// already, and the repair went in there and not here. The same anchor bug was
/// in a second place — `the_written_numbers_are_the_numbers` counted every
/// `@id(` in a file, so the same comment made a one-rule file hold two — and it
/// is repaired the same way.
///
/// **What this does not close, said plainly.** The marker may still sit anywhere
/// in the contiguous comment run above the annotation, and in a file whose whole
/// header is comments that run is the whole file: moved to the top with a
/// sentence under it, a marker thirty lines from the constraint it excuses is
/// accepted. Bounding it would need a rule about *how close* is close enough,
/// and this file has none — the honest half is that the anchor is now the
/// annotation, so ordinary prose can no longer move it in either direction, and
/// what remains is a placement question rather than a reader that can be fooled
/// by a sentence.
fn references_marked(text: &str, name: &str, constraint: &str, target: &str) -> bool {
    let annotation = format!("@id(\"{name}\")");
    let opens_a_line = |at: &usize| {
        text[..*at]
            .rsplit_once('\n')
            .is_none_or(|(_, last)| last.trim().is_empty())
    };
    let written: Vec<usize> = text
        .match_indices(&annotation)
        .map(|(at, _)| at)
        .filter(opens_a_line)
        .collect();
    // **And no line-opening annotation is "not marked", not "look anywhere".**
    // This fell through to `text.find(&annotation)` — the first *text*
    // occurrence, which is precisely the anchor the paragraph above records
    // removing, and which ordinary prose naming a policy's own id can move in
    // either direction. A silent fall-through that fails *open*, and decided by
    // nothing: every `@id(` this pack ships opens its line, so the clause was
    // never run and deleting it left both suites green.
    //
    // Fail closed. A policy whose annotation this reader cannot anchor is a
    // policy with no marker, which is what `§2.4` asks of a check that cannot
    // be performed.
    let Some(at) = written.last().copied() else {
        return false;
    };
    text[..at]
        .lines()
        .rev()
        .take_while(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("//") || trimmed.is_empty()
        })
        // **In reading order, and each marker's reason is its own.** The lines
        // were collected with `.rev()` and joined, so everything *above* the
        // marker landed *after* it and was read as part of its reason —
        // `nc:narrows: reasons` was excused for `principal` by a sentence
        // written eight lines earlier about something else. A reason is the
        // text of the marker, and of the comment lines that follow it.
        //
        // **Joined with the newlines kept**, because the line the marker is
        // written on is load-bearing below and a space would erase it.
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n")
        .split(NARROWS)
        .skip(1)
        .any(|reason| {
            // **The reason has to name what it excuses.** The marker was per
            // *policy*, so one written honestly about
            // `action == Action::"Delete"` silently covered a
            // `principal is User` added later — measured turning invariant 5 off
            // for every `Agent` and `Device`, with `check passed`. `limits.rs`
            // fixed exactly this class for its own hatch: "a marker excuses one
            // table … it excused both, which is how an unbounded table rode in
            // beside a genuinely bounded one".
            //
            // **And the operator with it, spelled inside backticks.** The
            // word alone let a reason written for `principal is Agent` excuse
            // `principal == Agent::"ana-assistant"` — a type test and a single
            // named entity are both "the principal", and only the second turns
            // invariant 1 off for everything else. Backticks because prose
            // reaches this too: *"the action in its scope"* contains `action
            // in`, and a rule a sentence satisfies by accident is the gate
            // shape this repository refuses everywhere else.
            //
            // **What is asked of the quoted span is that it *begins* with the
            // constraint, not that it equals it.** Requiring equality refused
            // ``principal is Agent`` — the constraint as the author reads it on
            // the policy's first line, and a strictly more informative reason —
            // with a diagnostic that told them to quote the constraint they had
            // just quoted. Too strong is the other way to be wrong, and a `fix`
            // that restates the failure is what `NOCTUA.md §15.1` is against.
            //
            // The reason is prose and is never otherwise parsed; this asks only
            // that it quote the constraint it is about, which is the least a
            // reason can do and still say which one it excuses.
            // **And the quote is on the marker's own line.** The reason runs
            // to the end of the comment run, and one paragraph in this pack
            // quotes a constraint in order to warn *against* it:
            // `core.no_self_escalation`'s marker records, four lines below
            // itself, that `core.provenance` was once rescoped to
            // `action == Action::"Read"`. That sentence then excused exactly
            // that rescoping of the policy it sits on — measured on 2026-09-11,
            // invariant 4 narrowed to `action == Action::"Read"` produced no
            // `core-narrows-by-entity-type` at all, `load()` succeeded, and only
            // the two cases in `policy/core/tests/` were left holding it.
            //
            // Every marker this pack ships names its constraint on the marker's
            // line, which is where an excuse belongs: beside the thing it
            // excuses, not four lines away inside a story about another policy.
            // The lines under it are still the reason a person reads; they are
            // no longer where the gate looks for the quote.
            //
            // **What this must not refuse** is the ordering the rule above was
            // widened for — an author who names the type before the constraint,
            // ``a rule about `principal is Agent` ``. That is one line, so both
            // spans are still on it. Nor is it the earlier repair coming back:
            // that one read the *first* span on the line and refused the rest;
            // this reads every span on the line and refuses the lines below.
            // `split_once` rather than `split(…).next()`: `str::split` always
            // yields one item, so the fallback arm of an `unwrap_or` there is
            // decided by nothing — a clause this tree refuses everywhere else.
            // Both arms here are driven: a marker with lines under it takes the
            // `Some` arm, and one written on the last comment line of the run
            // takes the `None` arm, which is the whole reason.
            let named = reason.split_once('\n').map_or(reason, |(first, _)| first);
            !named.trim().is_empty() && quotes_the_constraint(named, constraint, target)
        })
}

/// Runs every case against the pack.
///
/// Returns the problems, how many cases ran — so a `tests/` directory that lost
/// its files is visible rather than green (`§19.3`) — and how many clauses each
/// policy has pinned by a case, keyed by policy id.
///
/// **The third is a map and was a total.** One number over thirteen policies is
/// a count where the property is a set: a policy gaining a pinned conjunct pays
/// for another losing one, and the number written down in `xtask/src/policy.rs`
/// never moves.
///
/// # Errors
///
/// Returns a problem per case that did not do what it said it would.
pub fn run(pack: &Pack, cases: &[Case]) -> (Vec<Problem>, usize, BTreeMap<String, usize>) {
    let authorizer = Authorizer::new();
    let mut problems = Vec::new();
    let mut ran = 0_usize;
    for case in cases {
        let Some(request) = request_for(pack, case, &mut problems) else {
            continue;
        };
        if names_nobody(pack, case, &mut problems) {
            continue;
        }
        let answer = authorizer.is_authorized(&request, &pack.policies, &pack.entities);
        let got = answer.decision();
        let expected = match case.decision {
            Expected::Allow => Decision::Allow,
            Expected::Deny => Decision::Deny,
        };
        if got != expected {
            problems.push(Problem {
                file: case.file.clone(),
                line: case.line,
                what: format!(
                    "`{}` expected {} and Cedar answered {got:?}",
                    case.name,
                    case.decision.word()
                ),
                fix: "the pack and the case disagree, and one of them is what a real \
                      request will meet. Decide which before changing either"
                    .to_owned(),
                check: "wrong-decision",
            });
            continue;
        }

        let reasons: BTreeSet<String> = answer
            .diagnostics()
            .reason()
            .map(ToString::to_string)
            .collect();
        if let Some(named) = &case.because {
            if !reasons.contains(named) {
                problems.push(Problem {
                    file: case.file.clone(),
                    line: case.line,
                    what: format!(
                        "`{}` says {named} decided it, and what decided it was {reasons:?}",
                        case.name
                    ),
                    fix: "the journal records which policy decided (§14.9), so a case that \
                          names the wrong one is a case that would not notice the right one \
                          being deleted"
                        .to_owned(),
                    check: "wrong-policy",
                });
                continue;
            }
            if case.decision == Expected::Deny {
                check_the_denial_is_not_free(pack, case, named, &authorizer, &mut problems);
            }
        }
        if let Some(permit) = &case.no_permit_matched {
            check_nothing_permitted_it(pack, case, permit, &authorizer, &mut problems);
        }

        // **The last statement a verdict reaches, not the first of the body.**
        // `ran` sat directly under the `request_for` guard, above the
        // authorization call and above every verdict below it — so three lines
        // here, `if case.file.ends_with("chain_depth.toml") { continue; }`, left
        // `40 case(s) in 8 file(s) decide as written` byte-identical while a case
        // naming the wrong policy went unjudged. `xtask/canaries/README.md` lists
        // `asked`, `read` and `judged` as repaired for exactly this; `ran` was not
        // among them, and `run` is the door `xtask/src/policy.rs` calls.
        ran += 1;
    }
    let pinned = check_every_permit_constraint_is_tested(pack, cases, &authorizer, &mut problems);
    (problems, ran, pinned)
}

/// A denial a `forbid` caused must stop being a denial when that `forbid` goes.
///
/// Without this, every denial case is satisfied by Cedar's default and the whole
/// suite is the `§19.5` failure: an assertion that cannot fail, reporting a floor
/// nobody has stood on.
fn check_the_denial_is_not_free(
    pack: &Pack,
    case: &Case,
    named: &str,
    authorizer: &Authorizer,
    problems: &mut Vec<Problem>,
) {
    let Some(without) = pack_without(&pack.policies, |id| id == named) else {
        return;
    };
    let Ok(request) = build_request(pack, case) else {
        return;
    };
    let answer = authorizer.is_authorized(&request, &without, &pack.entities);
    if answer.decision() == Decision::Deny {
        problems.push(Problem {
            file: case.file.clone(),
            line: case.line,
            what: format!(
                "`{}` is refused with {named} removed, so it does not test {named}",
                case.name
            ),
            fix: "Cedar denies by default, so a request nothing permits is denied whatever \
                  the core says — and a case like this passes against an empty pack. Give \
                  the request a permit it would otherwise satisfy, so that removing the \
                  forbid changes the answer"
                .to_owned(),
            check: "denial-proves-nothing",
        });
    }
}

/// A case about a missing permit must still be denied with every `forbid` gone.
fn check_nothing_permitted_it(
    pack: &Pack,
    case: &Case,
    permit: &str,
    authorizer: &Authorizer,
    problems: &mut Vec<Problem>,
) {
    match pack.effects.get(permit) {
        Some(Effect::Permit) => {}
        Some(Effect::Forbid) => {
            problems.push(Problem {
                file: case.file.clone(),
                line: case.line,
                what: format!("`{permit}` is a forbid, and `no_permit_matched` names permits"),
                fix: "a denial caused by a rule names it in `because`; `no_permit_matched` \
                      is for a request that falls outside a permit and is refused because \
                      nothing allowed it"
                    .to_owned(),
                check: "not-a-permit",
            });
            return;
        }
        None => {
            problems.push(Problem {
                file: case.file.clone(),
                line: case.line,
                what: format!("`{permit}` is not a policy in this pack"),
                fix: "the name has to match a policy's `@id`, or the case is about a rule \
                      that does not exist and will pass for ever"
                    .to_owned(),
                check: "unknown-policy",
            });
            return;
        }
    }
    let Some(permits_only) = pack_without(&pack.policies, |id| {
        matches!(pack.effects.get(id), Some(Effect::Forbid))
    }) else {
        return;
    };
    let Ok(request) = build_request(pack, case) else {
        return;
    };
    let answer = authorizer.is_authorized(&request, &permits_only, &pack.entities);
    if answer.decision() == Decision::Allow {
        problems.push(Problem {
            file: case.file.clone(),
            line: case.line,
            what: format!(
                "`{}` is permitted once the forbids are removed, so it was refused by a \
                 rule and not by a missing permit",
                case.name
            ),
            fix: "name the forbid in `because` instead. `no_permit_matched` is for the \
                  boundary of a permit, and using it for a rule hides which rule fired"
                .to_owned(),
            check: "denial-was-not-a-missing-permit",
        });
    }
}

/// A case claiming the action is the boundary is refused if it is not.
///
/// Split out because [`coverage`] crossed a hundred lines when the claim gained
/// a check. What it asks is one question: run the same request with a verb the
/// permit allows, and it must be *permitted*. If it is still denied, something
/// other than the action refuses it and the flag buys a pairing it has not
/// earned.
fn the_flag_means_what_it_says(
    pack: &Pack,
    case: &Case,
    cases: &[Case],
    permit: &str,
    authorizer: &Authorizer,
    problems: &mut Vec<Problem>,
) {
    // **And the flag has to be true of the case that sets it.** It is a
    // per-case opt-out of the pairing rule, and nothing asked whether the
    // action really is what puts the request outside the permit: a case
    // could set it and be denied by the resource, the context or a `when`
    // clause, with the flag buying it a pairing it did not earn. Measured —
    // the guardian boundary case retargeted at a principal who is not a
    // guardian, with an action the permit *does* allow, kept the flag, kept
    // its name, and `boundary-without-a-control` stayed silent.
    //
    // What the claim means, in one question: run the same request with a
    // verb the permit allows, and it must be *permitted*. If it is still
    // denied, something other than the action is refusing it and the flag
    // is a lie.
    if !case.the_action_is_the_boundary {
        return;
    }
    // **Every matching control, not whichever one is loaded first.** This was
    // `cases.iter().find(…)`, and `cases::load` sorts by path — so every
    // `policy/core/tests/*.toml` precedes a pack's, and a control written in
    // `policy/packs/family/tests/permissions.toml` was never the one selected.
    // The claim was judged against one arbitrarily-chosen case, which is why an
    // attempt to write a case for the principal conjunct below reported a
    // different check entirely and proved nothing.
    //
    // The flag claims one thing — swap the verb for one the permit allows and
    // the request is permitted — and a claim of that shape is false as soon as
    // *any* allowed verb still leaves it denied.
    let mut refused = None;
    for control in cases.iter().filter(|other| {
        other.decision == Expected::Allow
            && other.because.as_deref() == Some(permit)
            && entity_type(&other.resource) == entity_type(&case.resource)
            && entity_type(&other.principal) == entity_type(&case.principal)
    }) {
        let Ok(request) = build_request(
            pack,
            &Case {
                action: control.action.clone(),
                ..case.clone()
            },
        ) else {
            continue;
        };
        if authorizer
            .is_authorized(&request, &pack.policies, &pack.entities)
            .decision()
            == cedar_policy::Decision::Deny
        {
            refused = Some(control);
            break;
        }
    }
    if let Some(control) = refused {
        problems.push(Problem {
            file: case.file.clone(),
            line: case.line,
            what: format!(
                "`{}` says the action is what puts it outside {permit}, and it is \
                 refused with `{}` too — a verb {permit} allows",
                case.name, control.action
            ),
            fix: "`the_action_is_the_boundary` claims one thing: swap the verb for \
                  one the permit allows and the request is permitted. Here it is \
                  still denied, so the resource, the principal or a condition is \
                  what refuses it — and the flag buys this case a pairing it has \
                  not earned. Drop the flag and pair it on the same action, or \
                  write the case that is really about the verb"
                .to_owned(),
            check: "boundary-is-not-the-action",
        });
    }
}

/// A case claiming the principal is the boundary is refused if it is not.
///
/// [`the_flag_means_what_it_says`] one field along, and asked of the controls
/// the pairing found rather than of a lookup of its own: each is an `allow`
/// naming the same permit on the same action and the same *kind* of resource.
/// Ask this case again as some control's principal, and it must be
/// *permitted*. If it is denied as every one of them, the resource, the context
/// or a condition is what refuses it, and the flag buys a pairing across
/// principal types it has not earned.
///
/// **Some control, not each.** A control shares this case's resource type and
/// not its resource, nor its context, and the request asked again keeps both
/// of this case's own — so a control the permit allows on another realm is
/// refused on this one by membership, which says nothing about who is asking.
fn the_principal_is_what_refuses_it(
    pack: &Pack,
    case: &Case,
    controls: &[&Case],
    permit: &str,
    authorizer: &Authorizer,
    problems: &mut Vec<Problem>,
) {
    if !case.the_principal_is_the_boundary {
        return;
    }
    // **Some principal the permit allows is permitted here, and that is the
    // whole claim.** This asked the opposite quantifier — refused as *any*
    // control's principal, the flag was false — and was wrong the moment a
    // control stood on another resource. Measured by a reviewer: one correct
    // `allow`, `mallory Write Realm::"office"`, reported both shipped cases
    // about the history permit's type test, which no rule had changed. None
    // permitted is reported against the first, as the one to read.
    let permitted = controls.iter().any(|control| {
        build_request(
            pack,
            &Case {
                principal: control.principal.clone(),
                ..case.clone()
            },
        )
        .is_ok_and(|request| {
            authorizer
                .is_authorized(&request, &pack.policies, &pack.entities)
                .decision()
                == cedar_policy::Decision::Allow
        })
    });
    let refused = if permitted { None } else { controls.first() };
    if let Some(control) = refused {
        problems.push(Problem {
            file: case.file.clone(),
            line: case.line,
            what: format!(
                "`{}` says its principal is what puts it outside {permit}, and it is \
                 refused as `{}` too — a principal {permit} allows",
                case.name, control.principal
            ),
            fix: "`the_principal_is_the_boundary` claims one thing: ask the same request \
                  as a principal the permit allows and it is permitted. Here it is still \
                  denied, so the resource, the action or a condition is what refuses it — \
                  and the flag buys this case a pairing across principal types it has not \
                  earned. Drop the flag and pair it on its own principal type, or write the \
                  case that is really about who is asking"
                .to_owned(),
            check: "boundary-is-not-the-principal",
        });
    }
}

/// Every policy has at least one case that expects a denial and names it.
///
/// `§14.9`'s seventh invariant, and the reason it is a gate rather than a
/// policy: a policy cannot require that somebody wrote a test for it.
///
/// Returns the problems and how many policies were checked.
#[must_use]
pub fn coverage(pack: &Pack, cases: &[Case]) -> (Vec<Problem>, usize) {
    let authorizer = Authorizer::new();
    // **One set, not two.** There was a second, `exercised`, and a branch below
    // reporting a policy denied by a case that never exercised it — which every
    // insert made impossible, since the two were written together at both sites.
    // `denied_by ⊆ exercised` held by construction, so the branch was
    // unreachable: its body replaced with `panic!` left all eleven tests that
    // break the real pack on purpose green.
    let mut denied_by: BTreeSet<&str> = BTreeSet::new();
    for case in cases {
        if case.decision != Expected::Deny {
            // An `allow` case is not a denial test, whichever field it fills in.
            // `no_permit_matched` was credited regardless of the decision, so an
            // `allow` case naming a permit counted as that permit's negative
            // test. `run` catches that shape today by another route, and
            // `coverage` is called on its own by the gate.
            continue;
        }
        if let Some(named) = &case.because {
            denied_by.insert(named);
        }
        if let Some(permit) = &case.no_permit_matched {
            denied_by.insert(permit);
        }
    }

    let mut problems = Vec::new();

    // **A permit's denial case has to be about that permit.** Checking only that
    // the request stays denied with every forbid removed is true of *any*
    // request nothing permits — so a `no_permit_matched` case proved nothing
    // about the permit it named. Two things got through: a policy that can never
    // fire satisfied acceptance criterion 11, and all three shipped cases
    // retargeted at one unrelated request stayed green, which meant a
    // `Grant`/`Revoke` permit passed its denial test on a `Read`.
    //
    // The pairing is what makes it mean something: somewhere in the suite the
    // same permit has to *allow* the same action. Then the denial is a request
    // just outside a rule that demonstrably fires, which is what a boundary is.
    for case in cases {
        let Some(permit) = &case.no_permit_matched else {
            continue;
        };
        // **The same action, and the same two entity types.** Matching only on
        // the verb left the denial free to be about a different universe: the
        // one case testing `family.a_member_reads_shared_content`'s sensitivity
        // boundary, retargeted from `Content::"diary"` to `Feature::"gallery"` —
        // a type that permit can never apply to — kept `10 policy(ies) each have
        // a denial test` green, and the permit's whole `when` clause could then
        // be deleted, opening `private` and `restricted` content to the entire
        // household with criterion 11 reporting satisfied.
        //
        // **Unless the action is what is being tested**, which the case says of
        // itself. Requiring the same verb makes a permit's action list
        // untestable by construction — a case denied *because* the verb is
        // outside the list has a verb the permit never allows — and every
        // action list in the pack was pinned by nothing as a result. Widening
        // `action in [Action::"Grant", Action::"Revoke"]` to a bare `action`
        // gave every guardian Read, Write, Delete and Share over every resource
        // in the realm, with the whole gate green.
        //
        // The two entity types still have to match, so the permit is still one
        // that demonstrably fires on this kind of request — what changes is
        // that the verb is allowed to be the thing outside it.
        //
        // **Or the principal's type, which the case says of itself the same
        // way.** A permit written `principal is User` refuses a `Device` for
        // being one, so requiring a control of the case's own principal type
        // made every such type test unpinnable — and one shipped deleted, with
        // a device and an agent then allowed to undo a person's history. The
        // controls are kept rather than counted, because they are what that
        // claim is judged against.
        let controls: Vec<&Case> = cases
            .iter()
            .filter(|other| {
                other.decision == Expected::Allow
                    && other.because.as_deref() == Some(permit.as_str())
                    && (other.action == case.action || case.the_action_is_the_boundary)
                    && entity_type(&other.resource) == entity_type(&case.resource)
                    && (entity_type(&other.principal) == entity_type(&case.principal)
                        || case.the_principal_is_the_boundary)
            })
            .collect();
        the_flag_means_what_it_says(pack, case, cases, permit, &authorizer, &mut problems);
        the_principal_is_what_refuses_it(pack, case, &controls, permit, &authorizer, &mut problems);
        if controls.is_empty() {
            problems.push(Problem {
                file: case.file.clone(),
                line: case.line,
                what: format!(
                    "`{}` says it falls outside {permit}, and nothing shows {permit} \
                     allowing a {} to {} a {}",
                    case.name,
                    entity_type(&case.principal),
                    case.action,
                    entity_type(&case.resource)
                ),
                fix: "a request nothing permits is denied whatever the pack says, so a \
                      case like this passes against a permit that can never fire, against \
                      one that has nothing to do with the action, or against one that \
                      cannot reach this kind of resource at all. Add an `allow` case \
                      naming the same permit in `because`, on the same action and the \
                      same principal and resource types, and the denial becomes the \
                      boundary of a rule that demonstrably works"
                    .to_owned(),
                check: "boundary-without-a-control",
            });
        }
    }

    // **Counted inside the loop, because the returned number was `pack.len()`
    // read from the same object.** `assert_eq!(checked, pack.len())` in the
    // gate's own test was `x == x` and could not fail, while promising "coverage
    // looked at N of M policies" — so `pack.effects.iter().take(1)` left the gate
    // printing `ok 15 policy(ies) each have a denial test` with fourteen
    // unexamined and the assertion reading `15 == 15`. This file already demands
    // the opposite of its twin one screen down.
    let mut asked = 0_usize;
    for (id, effect) in &pack.effects {
        let file = pack.origins.get(id).cloned().unwrap_or_default();
        if !denied_by.contains(id.as_str()) {
            let how = match effect {
                Effect::Forbid => {
                    "a case that expects `deny` and names it in `because`. The runner then \
                     re-runs that case with this policy removed and requires the answer to \
                     become `allow`, so the case cannot pass against an empty pack"
                }
                Effect::Permit => {
                    "a case that expects `deny` and names it in `no_permit_matched` — a \
                     request that falls just outside it. The runner re-runs that case with \
                     every forbid removed and requires it to stay denied"
                }
            };
            problems.push(Problem {
                file,
                line: 0,
                what: format!("`{id}` has no denial test"),
                fix: format!(
                    "§14.9's seventh invariant: a policy without a negative test does not \
                     enter the monorepo. Write {how}"
                ),
                check: "no-denial-test",
            });
        }
        // **Counted where the verdict is, not where the loop is entered.** The
        // increment was the first line of the body, so a `continue` anywhere
        // below it left the number exact: measured, one `if id ==
        // "core.grants_expire" { continue; }` gave `ok 14 policy(ies) each have a
        // denial test` over thirteen examined, with the gate's own assertion
        // reading `14 == 14`. That is the defect the paragraph above says was
        // just repaired — the counter moved inside the loop and stayed above the
        // rule, which is half the distance.
        asked += 1;
    }
    (problems, asked)
}

/// The type of an entity uid: `Content` of `Content::"diary"`.
///
/// A uid nobody wrote as `Type::"id"` yields itself, so a malformed case is
/// compared as written rather than silently matching everything.
fn entity_type(uid: &str) -> &str {
    uid.split("::").next().unwrap_or(uid).trim()
}

/// A variant this code could not build is a clause it did not pin.
///
/// **And it used to be counted as one**: the increment sat above both
/// `continue`s, so a mutation that never ran raised the number. Measured, with
/// every proposed variant made unparseable, the count fell from 28 to 20 and
/// eight are reported here — before this, it stayed 28 with zero evaluated. That
/// number is the only thing standing behind `condition-untested` and
/// `branch-untested`, the two findings with no negative test.
///
/// Zero today; what this asserts is that it stays zero.
fn unbuildable_variants(unbuildable: usize) -> Vec<Problem> {
    if unbuildable == 0 {
        return Vec::new();
    }
    vec![Problem {
        file: String::new(),
        line: 0,
        what: format!(
            "{unbuildable} proposed mutation(s) could not be parsed or substituted, so \
             the clauses they were about were pinned by nothing"
        ),
        fix: "each is a clause this code offered to weaken and then could not build — \
              the rendering is malformed, or the pack refused the variant. Until it \
              builds, the number beside it is larger than what was checked"
            .to_owned(),
        check: "mutation-unbuildable",
    }]
}

/// Every widening of one permit's constraints that no case would notice.
///
/// Split out of [`check_every_permit_constraint_is_tested`] only because that
/// function grew past the line limit.
fn the_widenings_nothing_notices(
    pack: &Pack,
    name: &str,
    rendered: &str,
    cases: &[Case],
    authorizer: &Authorizer,
    inspected: &mut usize,
    unbuildable: &mut usize,
) -> Vec<Problem> {
    let mut problems = Vec::new();
    for (what, widened) in widenings(rendered) {
        let Ok(replacement) = Policy::parse(None, &widened) else {
            *unbuildable += 1;
            continue;
        };
        let Some(mutated) = pack_replacing(&pack.policies, name, &replacement) else {
            *unbuildable += 1;
            continue;
        };
        let noticed = cases.iter().any(|case| {
            let Ok(request) = build_request(pack, case) else {
                return false;
            };
            let before = authorizer
                .is_authorized(&request, &pack.policies, &pack.entities)
                .decision();
            let after = authorizer
                .is_authorized(&request, &mutated, &pack.entities)
                .decision();
            before != after
        });
        if !noticed {
            problems.push(Problem {
                file: pack.origins.get(name).cloned().unwrap_or_default(),
                line: 0,
                what: format!("nothing notices if `{name}` stops constraining {what}"),
                fix: format!(
                    "a constraint no case tests is one that can be widened away in \
                     silence — and a permit widened by one token is how a guardian \
                     came to read `restricted` content with every gate green. Add a \
                     case that is refused *because* of {what}, so removing it flips \
                     an answer"
                ),
                check: "permit-constraint-untested",
            });
        }
        // **The last statement a verdict reaches, not the first of the
        // body.** `guards_fail_closed` says this at the bottom of the file and
        // three of the four mutation loops did not do it — so one filter placed
        // under the increment paid `PINNED` and left the whole policy block
        // byte-identical: `40 case(s) … and 32 constraint(s) and condition(s) are
        // pinned by one of them`, `check passed`. What it hides is a permit that
        // stops constraining its action list, which is `permit-constraint-untested`'s
        // whole subject.
        *inspected += 1;
    }
    problems
}

/// Every permit's own constraints are held in place by some case.
///
/// **The check criterion 11 could not make.** `coverage` requires every policy
/// to have a denial test and pairs each with a control, and both were satisfied
/// by a permit that was simply too broad: widening
/// `family.a_guardian_administers_the_realm` from
/// `action in [Action::"Grant", Action::"Revoke"]` to a bare `action` — one
/// token — gave every guardian Read, Write, Delete and Share over everything in
/// the realm, including `restricted` content, with the gate reporting
/// `11 policy(ies) each have a denial test` and `check passed`.
///
/// No rule about how a case is *written* can see that, because nothing about
/// the pack changed except what it allows. What can see it is the mutation the
/// widening *is*: drop one of a permit's constraints and require some case to
/// notice. A constraint no case notices is a constraint nobody has tested, and a
/// constraint nobody has tested is one that can be widened away in silence.
///
/// This is `check_the_denial_is_not_free`'s twin. That one removes a `forbid`
/// and requires the answer to flip; this removes a `permit`'s narrowing and
/// requires the same.
///
/// Returns a problem per constraint that nothing holds.
fn check_every_permit_constraint_is_tested(
    pack: &Pack,
    cases: &[Case],
    authorizer: &Authorizer,
    problems: &mut Vec<Problem>,
) -> BTreeMap<String, usize> {
    // **Per policy, because a total is a count where the property is a set.**
    // `PINNED == 38` in `xtask/src/policy.rs` is one number over thirteen
    // policies, so one policy gaining a pinned conjunct pays for another losing
    // one and the written number never moves — the compensating move this
    // repository refuses everywhere else. Keyed by policy id, the two halves of
    // that trade are two rows and either one of them is a failure.
    let mut by_policy: BTreeMap<String, usize> = BTreeMap::new();
    // **How many constraints it looked at, because a step that inspected
    // nothing has not passed.** `policy.rs`'s own module doc says every check
    // there reports its count; this one returned nothing, so it could go to
    // zero invisibly — which is the state it was already in for its whole life
    // before the `widenings` fix, and the state deleting its one call line puts
    // it back into with the entire suite green.
    let mut inspected = 0_usize;
    // Variants this code proposed and could not build. A mutation that never
    // ran pinned nothing, and counting it as one is how `inspected` became a
    // fingerprint of assembled strings rather than of evaluated clauses.
    let mut unbuildable = 0_usize;
    for policy in pack.policies.policies() {
        let name = policy.id().to_string();
        let rendered = policy.to_string();
        let before_this_policy = inspected;
        // **Every conjunct of every condition, for both effects.** This ran on
        // `permit` scopes alone, so the conditions that carry every core
        // invariant were mutated by nothing at all: each of the seven is a
        // `forbid`, and `Effect::Permit` skipped the lot. One conjunct added to
        // one of them turned it off with the whole gate green —
        //
        // ```text
        // core.fail_closed:  when { … <= 0 }  ->  when { … <= 0 && context.via_agent == false }
        // -> ok 13 policy(ies) each have a denial test · check passed
        // ```
        //
        // — and `context.via_agent` is a caller-supplied flag, so every
        // agent-mediated request bypassed fail-closed while every case, which
        // sets it to `false`, stayed green.
        //
        // Removing one conjunct is the mutation, and it is the right one in both
        // directions: dropping a term from a `forbid`'s `when` makes it deny
        // *more*, so a case sitting on that term's boundary flips to `Deny`;
        // dropping one from a `permit`'s makes it allow more, and a case flips
        // the other way. A conjunct nothing notices the loss of is one that can
        // be added, or removed, in silence.
        for (what, without) in conjuncts(&rendered) {
            // **A `has` guard's boundary is a request no case can be.** Every
            // condition here is written `!(context has x) || …` or
            // `context has x && …` so that a request missing the attribute
            // forbids rather than erroring — and a case's context is built
            // against the schema, which refuses to omit a required key. So the
            // one request that would notice the guard's removal cannot be
            // written as a case at all, and reporting it here would demand a
            // test that cannot exist.
            //
            // It is not untested: `nc-policy`'s
            // `a_request_missing_a_context_attribute_is_refused_with_no_schema_to_catch_it`
            // builds exactly that request, with no schema, and requires `Deny`.
            // This is the one shape excluded, it is named rather than listed,
            // and the thing it excuses is proven somewhere a reader can find.
            if guards_a_dereference(&what, &rendered) {
                continue;
            }
            // **Counted after the variant has been built, parsed and
            // substituted.** The increment was above both `continue`s, so a
            // mutation this code cannot construct still raised the number:
            // measured, making every proposed variant unparseable left
            // `28 constraint(s) and condition(s) are pinned by one of them`
            // unchanged with **zero** of them actually evaluated. That number is
            // the only thing standing behind `condition-untested` and
            // `branch-untested`, the two findings with no negative test — and it
            // was a fingerprint of how many strings were assembled. This file's
            // own `widenings` paragraph records the same failure: *`Policy::parse`
            // refused all of them, and the `else { continue; }` swallowed the
            // lot. This check has never fired.*
            let Ok(replacement) = Policy::parse(None, &without) else {
                unbuildable += 1;
                continue;
            };
            let Some(mutated) = pack_replacing(&pack.policies, &name, &replacement) else {
                unbuildable += 1;
                continue;
            };
            let noticed = cases.iter().any(|case| {
                let Ok(request) = build_request(pack, case) else {
                    return false;
                };
                let before = authorizer
                    .is_authorized(&request, &pack.policies, &pack.entities)
                    .decision();
                let after = authorizer
                    .is_authorized(&request, &mutated, &pack.entities)
                    .decision();
                before != after
            });
            if !noticed {
                problems.push(Problem {
                    file: pack.origins.get(&name).cloned().unwrap_or_default(),
                    line: 0,
                    what: format!("nothing notices if `{name}` stops requiring {what}"),
                    fix: format!(
                        "a condition no case sits on the boundary of is one that can be \
                         added or removed in silence — and one conjunct added to a core \
                         `forbid` turned it off for every request that sets a caller-\
                         supplied flag, with the whole gate green. Add a case whose answer \
                         depends on {what}, so removing it flips one"
                    ),
                    check: "condition-untested",
                });
            }
            // **The last statement a verdict reaches, not the first of the
            // body.** `guards_fail_closed` says this at the bottom of the file and
            // three of the four mutation loops did not do it — so one filter placed
            // under the increment paid `PINNED` and left the whole policy block
            // byte-identical: `40 case(s) … and 32 constraint(s) and condition(s) are
            // pinned by one of them`, `check passed`. What it hides is a permit that
            // stops constraining its action list, which is `permit-constraint-untested`'s
            // whole subject.
            inspected += 1;
        }
        // **And inside a conjunct, not only whole ones.** The loop above drops
        // a conjunct; `guards_a_dereference` excuses the shape
        // `!(x has y) || <predicate>` because a request missing `y` cannot be
        // written as a case — and it excused the **predicate on the right of the
        // `||` along with the guard on its left**. Two core branches lived
        // there untested: turning `!(context has now) || expired` into
        // `context has now && expired` deletes `core.grant_not_expired`'s
        // fail-closed half, and turning `source != "policy"` into
        // `source == "manual"` lets an *inherited* grant be perpetual — both
        // measured green against the whole suite and `cargo xtask check`.
        //
        // So each is mutated where it lives: the guard's fail-closed half is
        // flipped to the fail-open one, and every comparison against a value is
        // flipped to its opposite. Neither mutation removes a term, so neither
        // is the loop above said differently.
        problems.extend(branches_nothing_notices(
            pack,
            &name,
            &rendered,
            cases,
            authorizer,
            &mut inspected,
            &mut unbuildable,
        ));
        problems.extend(guards_fail_closed(
            pack,
            &name,
            &rendered,
            cases,
            &mut inspected,
        ));
        if pack.effects.get(&name) == Some(&Effect::Permit) {
            problems.extend(the_widenings_nothing_notices(
                pack,
                &name,
                &rendered,
                cases,
                authorizer,
                &mut inspected,
                &mut unbuildable,
            ));
        }
        // **An `if`, where a `continue` was.** The row below has to be written
        // for a `forbid` as well, and a `continue` past it would leave every
        // core invariant out of the table that is supposed to hold them.
        by_policy.insert(name, inspected - before_this_policy);
    }
    // **A variant this code could not build is a clause it did not pin**, and it
    // used to be counted as one: the increment sat above both `continue`s, so a
    // mutation that never ran raised the number. Measured, with every proposed
    // variant made unparseable, `28 constraint(s) and condition(s) are pinned by
    // one of them` was unchanged while **zero** were evaluated — and that number
    // is the only thing standing behind `condition-untested` and
    // `branch-untested`, the two findings with no negative test.
    //
    // Reported rather than absorbed, so the count stays a count of clauses really
    // evaluated. Zero today; the assertion is that it stays zero.
    problems.extend(unbuildable_variants(unbuildable));
    by_policy
}

/// Whether `!(x has y) || …` carries nothing but the guard on its right.
///
/// `!(context has now) || context.now > 0` guards *and* asserts; the assertion
/// is a clause a case can decide, so the pair is not excused. A guard whose
/// right-hand side only reads the attribute it guarded — `!(x has y) || x.y` —
/// is the shape the excuse is for.
fn guard_is_the_whole_clause(text: &str) -> bool {
    let Some((left, right)) = text.split_once("||") else {
        return false;
    };
    // **Asked the other way, because a list consulted to excuse fails open.**
    // This was nine literal shapes the right-hand side must *not* hold, and a
    // shape missing from it took its conjunct out of the mutation loop in
    // silence. Four were measured missing by a reviewer in wave 14: a negated
    // assertion, an assertion about a *different* attribute, a method call —
    // which the sentence here claimed to catch, while a list of punctuation has
    // no way to see one — and everything past the *first* `||`, since the split
    // is `split_once` and `|| a || b` is two clauses read as one.
    //
    // What the excuse is for is one shape and only one: a request that does not
    // carry `y` cannot be written as a case, so `!(x has y) || x.y` has nothing
    // to mutate. Anything else on the right is a predicate a case can decide, so
    // the question is now an equality against the dereference the guard names.
    let Some((subject, attribute)) = left
        .trim()
        .trim_start_matches('!')
        .trim()
        .trim_start_matches('(')
        .trim_end_matches([')', ' '])
        .split_once(" has ")
    else {
        return false;
    };
    right.trim().trim_end_matches(['}', ' ', ';'])
        == format!("{}.{}", subject.trim(), attribute.trim())
}

/// A policy's text with its annotations and its string literals spent.
///
/// An annotation is prose the author attached to the policy; a string literal is
/// a value the policy compares against. Neither is an expression that reads an
/// attribute, and both are rendered by `to_string` — so both were being read as
/// dereferences. Spending rather than cutting keeps every offset where it was.
fn what_a_policy_evaluates(rendered: &str) -> String {
    let mut out = String::with_capacity(rendered.len());
    for line in rendered.lines() {
        if line.trim_start().starts_with('@') {
            out.extend(line.chars().map(|_| ' '));
            out.push('\n');
            continue;
        }
        let mut inside = false;
        let mut escaped = false;
        for letter in line.chars() {
            if inside {
                if escaped {
                    escaped = false;
                } else if letter == '\\' {
                    escaped = true;
                } else if letter == '"' {
                    inside = false;
                }
                out.push(if letter == '"' { '"' } else { ' ' });
                continue;
            }
            if letter == '"' {
                inside = true;
            }
            out.push(letter);
        }
        out.push('\n');
    }
    out
}

/// Whether this conjunct exists only so that the one beside it can dereference.
///
/// Three shapes, and they are the same shape: `context has grant`,
/// `!(context has via_agent) || context.via_agent`, and `resource is Content`
/// standing in front of `resource.sensitivity`. Each makes the policy apply to,
/// or step over, a request that would otherwise make the *condition* error — and
/// Cedar skips a policy whose condition errors, so what the guard prevents is
/// the rule silently ceasing to say anything.
///
/// **Their boundary is a request no case can be**, which is why they are
/// excluded here rather than left to be reported for ever. A case's context is
/// built against the schema, which refuses to omit a required key; and a
/// `no_permit_matched` case over a `Feature` is refused by the case runner,
/// correctly, because no permit in the pack ever allows a `Feature` and the
/// denial would prove nothing.
///
/// Excluded, named, and proven elsewhere: `nc-policy`'s
/// `a_request_missing_a_context_attribute_is_refused_with_no_schema_to_catch_it`
/// builds exactly the request a `has` guard is for, with no schema, and requires
/// `Deny`. The type test is proven by the narrowing gate, which refuses a core
/// invariant that carries one at all.
fn guards_a_dereference(conjunct: &str, rendered: &str) -> bool {
    let text = conjunct.trim_matches('`').trim();
    let single = !text.contains("&&");
    // `!(x has y) || <predicate>` — the guard, and **not** the predicate beside
    // it.
    //
    // **This excused the whole shape**, so a conjunct written that way was
    // removed from the mutation entirely: `condition-untested` could not be
    // reached by a guarded clause, and `guards_fail_closed` claims every
    // unguarded one, so between them the verdict had no reachable input on a
    // core policy. Measured across four attempts — a bare `has`, an unguarded
    // comparison, a guarded one that changed an answer, and a guarded one
    // implied by the clause above it: `guard-fails-open`, `guard-fails-open`,
    // `wrong-decision`, and then *nothing at all*, with `must_fail`'s own floor
    // reporting "the pack was broken on purpose and every check accepted it".
    //
    // The same hole was fixed one level down — the doc on the conjunct loop
    // records that two core branches lived on the right of a `||` untested, and
    // names them. What is excused here is that a request *missing* `y` cannot be
    // written as a case; what is on the right of the `||` is an ordinary
    // predicate, and removing it is an ordinary mutation.
    if text.starts_with("!(")
        && text.contains(" has ")
        && text.contains("||")
        && guard_is_the_whole_clause(text)
    {
        return true;
    }
    // `x has y`, alone — and **not** `!(x has y)`, alone.
    //
    // **A leading `!` reached this branch and was excused by it**, because
    // `"!(principal has node_key)".split(" has ").count()` is also 2. So the one
    // spelling that turns an invariant off for a whole entity type was removed
    // from the mutation loop by the branch written for the spelling that is a
    // type test the gate reports. The two are the same test and want opposite
    // treatment: `x has y` alone is a narrowing `narrows_itself` names, and
    // there is nothing to mutate; `!(x has y)` alone is the same narrowing
    // negated, and a case can be written for it.
    //
    // **And something beside it has to read the attribute, which this did not
    // ask.** The `is` branch four lines below enforces exactly that — it excuses
    // `x is T` only where `rendered` also holds `x.` — and this one excused
    // `x has y` whether or not a single byte of the policy dereferences `y`. One
    // rule written twice inside one function, with the second copy narrower,
    // which is the shape this file's own header warns about.
    //
    // What it cost, measured by a reviewer in round seventy-one wave 10: a line
    // `when { context has quorum_met }` added to `core.fail_closed` — invariant
    // 1, deny by default — narrows that policy to requests whose context
    // carries `quorum_met`, and **nothing went red**. `cargo test -p nc-policy`
    // answered `35 passed` and `42 passed`, and `cargo xtask check` printed the
    // policy section byte for byte as on a clean tree. The conjunct was removed
    // from the mutation loop by an excuse whose own name is
    // `guards_a_dereference`, about a clause that guarded no dereference.
    if single
        && !text.starts_with('!')
        && !text.contains("||")
        && text.split(" has ").count() == 2
        && let Some((subject, attribute)) = text.split_once(" has ")
    {
        let subject = subject.trim();
        let attribute = attribute.trim().trim_end_matches(['}', ' ', ';']);
        // **The whole attribute, not a prefix of one.** `contains` cannot tell
        // `resource.owner` from `resource.owner_id`, so `resource has owner` was
        // excused whenever the policy read *any* attribute starting with those
        // letters — the conjunct dropped from the mutation loop and a narrowing
        // `has` on a core `forbid` unreported, which is the defect this branch
        // was repaired for one wave earlier. Named by a reviewer in wave 11.
        // **What the policy *evaluates*, not everything it prints.**
        // `to_string` renders annotations verbatim, so `@doc("context.quorum_met")`
        // — prose, reading nothing — satisfied this excuse and took its conjunct
        // out of the mutation loop. A gate satisfied by a string literal, which
        // is the shape this file's own header names. Measured in round
        // seventy-one wave 12: with that annotation added to `core.fail_closed`,
        // the `condition-untested` finding for it disappeared.
        //
        // Spent here rather than at the call site, because `rendered` is also
        // what the scope rules read, and a `Group::"g"` with its name spent is
        // a widening nobody can compare.
        let evaluated = what_a_policy_evaluates(rendered);
        let path = format!("{subject}.{attribute}");
        return evaluated.match_indices(&path).any(|(at, _)| {
            evaluated[at + path.len()..]
                .chars()
                .next()
                .is_none_or(|next| !next.is_alphanumeric() && next != '_')
        });
    }
    // `x is T`, alone, with something beside it that reads `x.`.
    if single
        && !text.contains("||")
        && let Some((subject, _)) = text.split_once(" is ")
    {
        let subject = subject.trim();
        // **Through the same reader as the branch above.** That one was repaired
        // in wave 12 to spend annotations and literals before asking whether the
        // policy dereferences, and its comment says *"the `is` branch four lines
        // below enforces exactly that"*. It did not: this asked the raw render,
        // so `@doc("principal.status")` — prose — or a value the policy merely
        // compares against turned the excuse on and lifted the conjunct out of
        // the mutation loop. One rule written twice inside one function, and the
        // repair that named the sibling fixed one of the two.
        return matches!(subject, "principal" | "action" | "resource" | "context")
            && what_a_policy_evaluates(rendered).contains(&format!("{subject}."));
    }
    false
}

/// Every branch inside `name`'s conditions that no case sits on the boundary of.
///
/// Extracted so the loop it came from stays readable; the reasoning is at the
/// call site and on [`inside_a_conjunct`].
fn branches_nothing_notices(
    pack: &Pack,
    name: &str,
    rendered: &str,
    cases: &[Case],
    authorizer: &Authorizer,
    inspected: &mut usize,
    unbuildable: &mut usize,
) -> Vec<Problem> {
    let mut problems = Vec::new();
    for (what, mutated_text) in inside_a_conjunct(rendered) {
        let Ok(replacement) = Policy::parse(None, &mutated_text) else {
            *unbuildable += 1;
            continue;
        };
        let Some(mutated) = pack_replacing(&pack.policies, name, &replacement) else {
            *unbuildable += 1;
            continue;
        };
        let noticed = cases.iter().any(|case| {
            let Ok(request) = build_request(pack, case) else {
                return false;
            };
            let before = authorizer
                .is_authorized(&request, &pack.policies, &pack.entities)
                .decision();
            let after = authorizer
                .is_authorized(&request, &mutated, &pack.entities)
                .decision();
            before != after
        });
        if !noticed {
            problems.push(Problem {
                file: pack.origins.get(name).cloned().unwrap_or_default(),
                line: 0,
                what: format!("nothing notices if `{name}` reverses {what}"),
                fix: format!(
                    "a comparison no case sits on both sides of is one that can be \
                     reversed in silence. `grants_expire` turns on \
                     `context.grant.source != \"policy\"` and the schema declares three \
                     sources, so a case set that only ever sends one says nothing about \
                     the others. Add a case whose answer depends on {what}"
                ),
                check: "branch-untested",
            });
        }
        // **The last statement a verdict reaches, not the first of the
        // body.** `guards_fail_closed` says this at the bottom of the file and
        // three of the four mutation loops did not do it — so one filter placed
        // under the increment paid `PINNED` and left the whole policy block
        // byte-identical: `40 case(s) … and 32 constraint(s) and condition(s) are
        // pinned by one of them`, `check passed`. What it hides is a permit that
        // stops constraining its action list, which is `permit-constraint-untested`'s
        // whole subject.
        *inspected += 1;
    }
    problems
}

/// Every `!(x has y) ||` guard, proven to deny when `y` is absent.
///
/// **The half of every core invariant that no test reached.** Each of these
/// guards is written so that a request *missing* the attribute is refused rather
/// than sailing past a condition Cedar would skip for erroring. That is the
/// whole point of writing it `!(x has y) || …` instead of `x has y && …` — and
/// flipping one to the other left the entire suite and `cargo xtask check`
/// green, in eight places, one of them `core.grant_not_expired`: an expired
/// grant allowed, for any decision point that did not send `now`.
///
/// It could not be a case. Every attribute named here — `now`, `via_agent`,
/// `actor_chain_depth`, `policy_epoch`, `budget_remaining`, `sensitivity`,
/// `source` — is declared **required** by `policy/schema.cedarschema`, and a
/// case's request and the world's entities are both built against the schema,
/// which refuses to omit a required key. So the branch is unreachable *with the
/// schema on*, which is exactly why it kept passing, and exactly why the guard
/// is worth having: the decision point that forgets to pass a schema is the one
/// it holds the line for.
///
/// So the proof is direct, and it is the one `decide_without_a_schema`'s own
/// doc describes: build the request with the attribute gone and no schema to
/// refuse it, and require `Deny`.
///
/// Each guard is proven twice, because `Deny` on its own proves nothing — Cedar
/// denies by default, so a request nothing permits denies whatever the guard
/// says. The same request **with** the attribute must be permitted first.
fn guards_fail_closed(
    pack: &Pack,
    name: &str,
    rendered: &str,
    cases: &[Case],
    inspected: &mut usize,
) -> Vec<Problem> {
    let mut problems = Vec::new();
    // **A `forbid` only.** A `permit` whose condition errors is skipped too, and
    // being skipped is the *safe* direction for one: the request falls through
    // to Cedar's default deny. What fails open is a rule that was going to
    // refuse and stops being asked, which is what a `forbid` is.
    if pack.effects.get(name) != Some(&Effect::Forbid) {
        return problems;
    }
    let guards = attributes_read(rendered);
    if guards.is_empty() {
        return problems;
    }
    // **The policy's own denial cases, not any permitted request.** Asked
    // against a request the pack merely permits, four guards reported fail-open
    // and were not: a `forbid` that does not apply to a request is not a guard
    // that failed, it is a rule about something else. What has to hold is
    // narrower and is the thing that broke — a request **this policy denies**
    // must still be denied when the attribute is gone.
    let mine: Vec<&Case> = cases
        .iter()
        .filter(|case| case.because.as_deref() == Some(name))
        .collect();
    if mine.is_empty() {
        // **A finding, because this is a different question from coverage.**
        // This used to return silently, on the argument that the denial-coverage
        // gate already reports a policy with no case — but that gate asks
        // whether a case *exists*, and this needs one that says `because =
        // "<this policy>"`. Retype a `because` and coverage stays green while
        // every guard in the policy stops being proven, with `inspected` simply
        // getting smaller: a filter between the subject and the counter, which
        // `xtask/canaries/README.md` names as the shape a printed number cannot
        // show.
        problems.push(Problem {
            file: pack.origins.get(name).cloned().unwrap_or_default(),
            line: 0,
            what: format!(
                "`{name}` reads {} attribute(s) and no denial case names it, so nothing \
                 proved that it still denies when one of them is absent",
                guards.len()
            ),
            fix: "a case proves this policy's guards when its `because` names this \
                  policy. The denial-coverage gate asks only that a case exists, so a \
                  `because` naming a different policy leaves that gate green and this one \
                  with nothing to inspect. Set `because` to this policy's id"
                .to_owned(),
            check: "guard-has-no-case",
        });
        return problems;
    }
    // **The counter is the last statement a verdict reaches, not the first
    // statement of the body.** `*inspected += 1` stood at the top of this loop,
    // so one line below it turned the whole proof off for a named policy with
    // the printed number unchanged. Measured: `core.an_agent_never_impersonates`
    // flipped to its fail-open form is `guard-fails-open` and two red tests;
    // add `if name == "core.an_agent_never_impersonates" { continue; }` under
    // the counter and the policy step is byte-identical to a clean run —
    // `ok 40 case(s) … and 32 constraint(s) and condition(s) are pinned`. §14.6
    // then goes unasked for every decision point that omits `via_agent`.
    //
    // `AGENTS.md` states this rule; it was written for this crate's coverage
    // loop a round ago and not carried across to this one.
    for (subject, attribute) in guards {
        let mut proven = 0_usize;
        for case in &mine {
            let Ok(denied) = decide_case(pack, case, None) else {
                continue;
            };
            // The floor: this must be the case's own answer with no schema in
            // the way, or what follows is about a different request.
            if denied != "Deny" {
                continue;
            }
            // **And this policy must be what denies it.** Cedar denies by
            // default, so a request nothing permits is denied whatever the guard
            // says — and every denial case is such a request until proven
            // otherwise. Measured: with `core.grant_not_expired` flipped to its
            // fail-open form and `now` removed, the answer was still `Deny`,
            // because no permit reaches that request at all. The proof was
            // green against a rule that had been switched off.
            //
            // So the case is used only if taking this policy *out of the pack*
            // makes the request `Allow`. Then `Deny` with the policy in is this
            // policy's answer, and `Allow` with the attribute gone is this
            // policy's guard failing open.
            let Some(without_policy) = pack_without(&pack.policies, |id| id == name) else {
                continue;
            };
            if decide_case_against(pack, case, None, &without_policy).as_deref() != Ok("Allow") {
                continue;
            }
            proven += 1;
            let Ok(without) = decide_case(pack, case, Some((subject.as_str(), attribute.as_str())))
            else {
                continue;
            };
            if without == "Deny" {
                continue;
            }
            problems.push(Problem {
                file: pack.origins.get(name).cloned().unwrap_or_default(),
                line: case.line,
                what: format!(
                    "`{name}` denies `{}` and answers `{without}` to the same request with \
                     no `{subject}.{attribute}`, so it fails open when that attribute is \
                     absent",
                    case.name
                ),
                fix: format!(
                    "a condition that reads `{subject}.{attribute}` errors when it is absent, \
                     and Cedar skips a policy whose condition errors — so the rule stops \
                     saying anything and whatever permit is behind it answers. Write it \
                     `!({subject} has {attribute}) || …` so the absence is refused. The \
                     schema declares `{attribute}` required, so no case can omit it and \
                     flipping the guard left every test green — which is why this is \
                     proven here, against the pack, with no schema"
                ),
                check: "guard-fails-open",
            });
            proven = usize::MAX;
            break;
        }
        if proven == 0 {
            problems.push(Problem {
                file: pack.origins.get(name).cloned().unwrap_or_default(),
                line: 0,
                what: format!(
                    "no case can prove that `{name}` still denies when \
                     `{subject}.{attribute}` is absent: none of its denials is a request \
                     the pack would otherwise permit"
                ),
                fix: "a `forbid` is proven by a request that is allowed without it and \
                      denied with it. A denial case that nothing permits anyway is denied \
                      by Cedar's default, and every proof about it passes by saying \
                      nothing. Give the case a principal and resource some permit in the \
                      pack reaches"
                    .to_owned(),
                check: "guard-unprovable",
            });
        }
        // Here, because every path above reaches it: a guard proven, a guard
        // that failed open, and a guard nothing could prove are three verdicts
        // and this is the one statement all three pass through.
        *inspected += 1;
    }
    problems
}

/// The same decision as [`decide_case`], against a given set of policies.
fn decide_case_against(
    pack: &Pack,
    case: &Case,
    gone: Option<(&str, &str)>,
    policies: &PolicySet,
) -> Result<String, String> {
    let entities = match gone {
        Some((subject, attribute)) if subject != "context" => {
            let stripped = without_attribute(&pack.world, attribute)?;
            Entities::from_json_str(&stripped, None).map_err(|error| error.to_string())?
        }
        _ => pack.entities.clone(),
    };
    let mut context = case.context.clone();
    // **Along the path, not only at the top.** `context.grant.source` reaches an
    // attribute of whatever `grant` holds, and the branch above removes it only
    // when `grant` is an entity reference — which is what every shipped case
    // writes. Cedar also lets a context key hold a record inline, and for one of
    // those the strip above is a no-op: the proof then compares a request
    // against itself and passes for having changed nothing. Removed by path
    // rather than everywhere, so a same-named key elsewhere in the context is
    // not what makes a guard look open.
    if let Some((subject, key)) = gone
        && let Some(rest) = subject.strip_prefix("context")
    {
        let mut at = &mut context;
        for segment in rest.split('.').filter(|segment| !segment.is_empty()) {
            match at.get_mut(segment) {
                Some(deeper) => at = deeper,
                None => return decide_here(case, &context, policies, &entities),
            }
        }
        if let Some(object) = at.as_object_mut() {
            object.remove(key);
        }
    }
    decide_here(case, &context, policies, &entities)
}

/// One request, built and answered.
///
/// Split out so the path walk above can answer early when the context does not
/// go that deep, without duplicating the seven lines that build a request.
fn decide_here(
    case: &Case,
    context: &serde_json::Value,
    policies: &PolicySet,
    entities: &Entities,
) -> Result<String, String> {
    let principal = EntityUid::from_str(&case.principal).map_err(|error| error.to_string())?;
    let action = EntityUid::from_str(&case.action).map_err(|error| error.to_string())?;
    let resource = EntityUid::from_str(&case.resource).map_err(|error| error.to_string())?;
    let context =
        Context::from_json_value(context.clone(), None).map_err(|error| error.to_string())?;
    let request =
        Request::new(principal, action, resource, context, None).map_err(|e| e.to_string())?;
    Ok(format!(
        "{:?}",
        Authorizer::new()
            .is_authorized(&request, policies, entities)
            .decision()
    ))
}

/// What the pack answers one case's request, with no schema and one attribute
/// optionally gone.
///
/// No schema on purpose: with one, the request cannot be built at all, because
/// every attribute a guard names is declared required. The decision point that
/// forgets the schema is the one the guard is for.
fn decide_case(pack: &Pack, case: &Case, gone: Option<(&str, &str)>) -> Result<String, String> {
    decide_case_against(pack, case, gone, &pack.policies)
}

/// Every attribute a policy reads, whatever guards it.
///
/// **Not the guard's spelling.** This asked for `!(x has y) ||` and the defect
/// it exists to catch *deletes that text* — flipping the guard to
/// `x has y &&` left nothing for the rule to look at, so the proof passed on a
/// policy that had just been switched off. A gate keyed to the shape of the
/// thing it guards is a gate the fix for that thing removes.
///
/// So the subject is what the policy **dereferences**: every `context.x`,
/// `principal.x`, `resource.x` and `context.grant.x` in its text. A request
/// missing any of them is a request this policy has an opinion about, and the
/// opinion has to be `Deny` — however the guard in front of it is written, or
/// whether there is one at all.
fn attributes_read(rendered: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    // **Both spellings, because Cedar has two.** This matched `subject.name` and
    // not `subject["name"]`, which is the same dereference written the other
    // way — so rewriting `!(principal has budget_remaining) ||
    // principal.budget_remaining <= 0` as `principal["budget_remaining"] <= 0`
    // left this returning nothing, `guards_fail_closed` returned before proving
    // anything, and `check passed` with §14.6's budget rule failing open.
    // Measured against the pack with no schema: `Allow` for the index form,
    // `Deny` for the shipped one. Cedar's renderer keeps whichever the author
    // wrote, so reading one spelling is reading half the language.
    for subject in [
        "context.grant.",
        "context.grant[\"",
        "context.",
        "context[\"",
        "principal.",
        "principal[\"",
        "resource.",
        "resource[\"",
    ] {
        for (at, _) in rendered.match_indices(subject) {
            // `context.grant.source` is read by both `context.grant.` and
            // `context.`; the longer subject is tried first and wins, so the
            // shorter one must not also claim it.
            if subject.starts_with("context")
                && !subject.starts_with("context.grant")
                && rendered[at..].starts_with("context.grant")
            {
                continue;
            }
            let attribute: String = rendered[at + subject.len()..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            // **And the segment after it, because a path can be two deep.**
            // `resource.owner.status` yielded `("resource", "owner")` and never
            // `status`, so a guard written `!(resource has owner) ||
            // resource.owner.status != "deceased"` was proven by removing
            // `owner` — which it handles — and never by removing the attribute
            // it actually reads. Measured against the pack with no schema:
            // `Allow` with `status` gone, `Deny` for the shipped form, and
            // `check passed`. The entity stripper works by attribute name, so
            // naming the second segment is all this needs.
            // **`.name` and `["name"]`, because Cedar has both here too.** The
            // second segment was read only after a `.`, so `a["b"].c` fell
            // between the two spellings — `context["grant"].source` yielded
            // nothing and invariant 3 failed open with the gate silent.
            let after = &rendered[at + subject.len() + attribute.len()..];
            let after = after.strip_prefix("\"]").unwrap_or(after);
            if let Some(second) = next_segment(after) {
                let deeper = (
                    format!("{}.{attribute}", subject.trim_end_matches(['.', '[', '"'])),
                    second,
                );
                if !found.contains(&deeper) {
                    found.push(deeper);
                }
            }
            if attribute.is_empty() {
                continue;
            }
            let pair = (
                subject
                    .trim_end_matches("[\"")
                    .trim_end_matches('.')
                    .to_owned(),
                attribute,
            );
            if !found.contains(&pair) {
                found.push(pair);
            }
        }
    }
    found
}

/// The attribute name one dereference along, in either of Cedar's spellings.
///
/// **Reading the second segment only after a `.` left two of the four paths
/// through a two-deep read unwatched.** The spellings mix: `a.b["c"]` and
/// `a["b"]["c"]` are the same read as `a.b.c`, and both came back with no
/// second segment at all — so `guards_fail_closed` proved invariant 3 by
/// removing `grant`, which the guard handles, and never `source`, which is what
/// it reads. Measured against the pack with no schema, `context.grant["source"]
/// == "delegation"` rewritten from the dotted form: `Allow` with `source` gone,
/// `Deny` for the shipped form, and `check passed` throughout.
fn next_segment(after: &str) -> Option<String> {
    let rest = after
        .strip_prefix('.')
        .or_else(|| after.strip_prefix("[\""))?;
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// Removes `name` from this value and from every record inside it.
fn remove_everywhere(value: &mut serde_json::Value, name: &str) {
    match value {
        serde_json::Value::Object(fields) => {
            fields.remove(name);
            for nested in fields.values_mut() {
                remove_everywhere(nested, name);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                remove_everywhere(item, name);
            }
        }
        _ => {}
    }
}

/// `world.json` with one attribute removed from every entity that carries it.
///
/// Textual, and deliberately: `serde_json` would do it in a typed way and the
/// entities then have to be re-serialised, which is a second place for the JSON
/// to differ from what Cedar read. What is needed is one key gone.
fn without_attribute(world: &str, attribute: &str) -> Result<String, String> {
    let mut parsed: serde_json::Value =
        serde_json::from_str(world).map_err(|error| error.to_string())?;
    let Some(entities) = parsed.as_array_mut() else {
        return Err("the world is not an array of entities".to_owned());
    };
    for entity in entities {
        if let Some(attributes) = entity.get_mut("attrs") {
            // **Everywhere, not only at the top.** This removed top-level keys
            // only, so the whole second-segment mechanism was a no-op for a
            // record-valued attribute: the pair was "proven" by a removal that
            // never happened. Latent while the schema declares no record
            // attributes, and live the day one is added — which the comment
            // this replaces assumed was already covered.
            remove_everywhere(attributes, attribute);
        }
    }
    serde_json::to_string(&parsed).map_err(|error| error.to_string())
}

/// Each branch *inside* a condition, and the policy with it reversed.
///
/// One operator, chosen because a core invariant was defeated by exactly it with
/// the suite green. The other half of that finding — the `!(x has y) ||` guards
/// — is proven directly by [`guards_fail_closed`] instead, because the request
/// that notices it cannot be written as a case at all:
///
/// - `a != b` becomes `a == b`, and `a == b` becomes `a != b`. `grants_expire`
///   turns on `context.grant.source != "policy"`, and the schema declares three
///   sources — so a case set that only ever sends `manual` says nothing about
///   `inherited`.
///
/// Neither removes a term, so neither is [`conjuncts`] said differently: these
/// are policies of the same size that mean something else.
fn inside_a_conjunct(rendered: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for (operator, opposite) in [(" != ", " == "), (" == ", " != ")] {
        for (at, _) in rendered.match_indices(operator) {
            // Only inside a condition body: the scope's `principal == User::"…"`
            // is what the policy is *about*, and the widening loop is what tests
            // that. A `when`/`unless` body starts after the scope's `)`.
            // **Inside a condition body, asked by walking the bodies.** This
            // took the *last* `)` before the operator and looked for
            // `when`/`unless` between them — so whenever the body itself holds
            // parentheses, that paren is inside the body and the slice holds
            // neither word. Every guarded comparison in this pack is written
            // `!(x has y) || x.y <op> v`, so every one was dropped: measured, 4
            // flips proposed where there are 7.
            if !in_a_condition_body(rendered, at) {
                continue;
            }
            let mutated = format!(
                "{}{opposite}{}",
                &rendered[..at],
                &rendered[at + operator.len()..]
            );
            found.push((
                format!(
                    "`{}` at byte {at} into `{}`",
                    operator.trim(),
                    opposite.trim()
                ),
                mutated,
            ));
        }
    }
    found
}

/// Each top-level conjunct of each condition, and the policy without it.
///
/// Cedar renders a condition as `when { … }` or `unless { … }` after the scope.
/// A conjunct is a top-level `&&` operand of one of those bodies; dropping the
/// only one drops the whole clause, which is the same mutation said in one step.
///
/// Read from the rendered text for the same reason [`widenings`] is: the EST is
/// a tree to rebuild and this is a substring to remove, and what has to be
/// produced is a policy Cedar will parse.
/// Whether a byte sits inside a `when {`/`unless {` body.
///
/// The same walk `conjuncts` uses, asked of one position. Reading backwards for
/// the nearest `)` cannot answer it, because a body may hold parentheses of its
/// own — which is what made the scope filter drop every guarded comparison.
fn in_a_condition_body(rendered: &str, at: usize) -> bool {
    let mut from = 0_usize;
    while let Some((clause, open)) = ["\nwhen {", "\nunless {", "when {", "unless {"]
        .iter()
        .filter_map(|word| rendered[from..].find(word).map(|off| (*word, from + off)))
        .min_by_key(|(_, off)| *off)
    {
        let body_from = open + clause.len();
        let Some(close) = matching_brace(rendered, body_from) else {
            break;
        };
        if at >= body_from && at < close {
            return true;
        }
        from = close + 1;
    }
    false
}

fn conjuncts(rendered: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut from = 0_usize;
    while let Some((clause, open)) = ["\nwhen {", "\nunless {", "when {", "unless {"]
        .iter()
        .filter_map(|word| rendered[from..].find(word).map(|at| (*word, from + at)))
        .min_by_key(|(_, at)| *at)
    {
        let body_from = open + clause.len();
        let Some(close) = matching_brace(rendered, body_from) else {
            break;
        };
        let body = &rendered[body_from..close];
        let parts = top_level_parts_of(body, "&&");
        for (index, part) in parts.iter().enumerate() {
            let text = part.trim();
            if text.is_empty() {
                continue;
            }
            let without = if parts.len() == 1 {
                // The whole clause goes, and the `;` that ends the policy stays.
                let mut rest = rendered[..open].to_owned();
                rest.push_str(&rendered[close + 1..]);
                rest
            } else {
                let kept: Vec<&str> = parts
                    .iter()
                    .enumerate()
                    .filter(|(other, _)| *other != index)
                    .map(|(_, part)| part.trim())
                    .collect();
                format!(
                    "{}{}{}{}",
                    &rendered[..body_from],
                    format_args!(" {} ", kept.join(" && ")),
                    "}",
                    &rendered[close + 1..]
                )
            };
            found.push((format!("`{text}`"), without));
        }
        from = close + 1;
    }
    found
}

/// The index of the `}` closing the brace this body opened at `from`.
fn matching_brace(text: &str, from: usize) -> Option<usize> {
    let mut depth = 1_usize;
    for (at, character) in text.char_indices().skip_while(|(at, _)| *at < from) {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}

/// `text` split on top-level occurrences of `separator`, brackets ignored.
fn top_level_parts_of(text: &str, separator: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0_i32;
    let mut start = 0_usize;
    let bytes = text.as_bytes();
    let mut at = 0_usize;
    while at < bytes.len() {
        match bytes[at] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            _ => {}
        }
        if depth == 0 && text[at..].starts_with(separator) {
            parts.push(text[start..at].to_owned());
            at += separator.len();
            start = at;
            continue;
        }
        at += 1;
    }
    parts.push(text[start..].to_owned());
    parts
}

/// A comma-separated list split at the top level, ignoring brackets.
fn top_level_parts(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0_i32;
    for character in text.chars() {
        match character {
            '[' | '(' => {
                depth += 1;
                current.push(character);
            }
            ']' | ')' => {
                depth -= 1;
                current.push(character);
            }
            ',' if depth == 0 => parts.push(core::mem::take(&mut current)),
            other => current.push(other),
        }
    }
    parts.push(current);
    parts
}

/// The same pack with one policy replaced.
fn pack_replacing(policies: &PolicySet, name: &str, with: &Policy) -> Option<PolicySet> {
    let mut kept = PolicySet::new();
    for policy in policies.policies() {
        if policy.id().to_string() == name {
            kept.add(with.clone().new_id(PolicyId::new(name))).ok()?;
        } else {
            kept.add(policy.clone()).ok()?;
        }
    }
    Some(kept)
}

/// Each way one constraint can be dropped from a rendered policy.
///
/// Textual, on Cedar's own normalised rendering, because `cedar_policy` exposes
/// no way to rebuild a policy from parts. A variant that will not re-parse is
/// skipped rather than reported: this must never fail the build for a shape it
/// could not construct.
fn widenings(rendered: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    // **The scope's parenthesis, not the first one in the text.** Cedar renders
    // the annotations first, and every policy in this pack is *required* to
    // carry `@id("…")` — a policy without one is a hard failure. So the first
    // `(` was always `@id`'s, every variant this built was `@id(principal)…`,
    // `Policy::parse` refused all of them, and the `else { continue; }` at the
    // call site swallowed the lot. **This check has never fired.** Measured, on
    // the shipped pack: one widening per policy, each one unparseable.
    //
    // ```text
    // @id("family.a_guardian_administers_the_realm")
    // permit ( principal in Group::"role.guardian", action in […], resource );
    //
    // widenings -> [("`\"family.a_guardian_…\"`", "@id(principal)\npermit ( … ")]
    // ```
    //
    // What that cost: dropping `action in [Action::"Grant", Action::"Revoke"]`
    // to a bare `action` — which gives every guardian Read, Write, Delete and
    // Share over every resource in the realm, `restricted` content included —
    // passed the whole gate green. That is verbatim the defect this function's
    // own doc says it exists to catch.
    //
    // The effect keyword is what the scope belongs to, so the search starts
    // there. `permit` and `forbid` are the only two Cedar has.
    let effect = rendered
        .find("\npermit")
        .or_else(|| rendered.find("\nforbid"))
        .map_or_else(
            || {
                // No annotation rendered above it, which is the shape the unit
                // tests below used to be written in.
                if rendered.starts_with("permit") || rendered.starts_with("forbid") {
                    Some(0)
                } else {
                    None
                }
            },
            Some,
        );
    let Some(effect) = effect else {
        return found;
    };
    let Some(open) = rendered[effect..].find('(').map(|at| at + effect) else {
        return found;
    };
    let Some(close) = rendered[open..].find(')') else {
        return found;
    };
    // **Top-level commas only.** `action in [Action::"Grant", Action::"Revoke"]`
    // holds one, so splitting on every comma gave four parts for three
    // constraints — and every variant built from them failed to re-parse and was
    // skipped, which is how the widening this check exists for went unreported.
    let scope = &rendered[open + 1..open + close];
    let parts = top_level_parts(scope);
    for (index, part) in parts.iter().enumerate() {
        let bare = match index {
            0 => "principal",
            1 => "action",
            _ => "resource",
        };
        if part.trim() == bare {
            continue;
        }
        let mut widened: Vec<&str> = parts.iter().map(String::as_str).collect();
        widened[index] = bare;
        found.push((
            format!("`{}`", part.trim()),
            format!(
                "{}({}){}",
                &rendered[..open],
                widened.join(","),
                &rendered[open + close + 1..]
            ),
        ));
    }
    found
}

/// The pack minus every policy `drop` says to drop.
fn pack_without(policies: &PolicySet, drop: impl Fn(&str) -> bool) -> Option<PolicySet> {
    let mut kept = PolicySet::new();
    for policy in policies.policies() {
        if drop(policy.id().as_ref()) {
            continue;
        }
        kept.add(policy.clone()).ok()?;
    }
    Some(kept)
}

/// Whether a case is about a principal or a resource `world.json` does not hold.
///
/// **A ghost entity turns a `no_permit_matched` case into a test of Cedar's
/// default.** That is the only kind of denial test criterion 11 accepts for a
/// `permit`, and a nonexistent entity has no attributes — so the permit's
/// condition errors, Cedar skips it, and *"denied because nothing permitted
/// it"* is trivially true. Measured on 2026-09-08, one character changed in
/// `policy/packs/family/tests/permissions.toml`, `Content::"vault"` to
/// `Content::"vault-typo"`:
///
/// ```text
/// ok    40 case(s) in 8 file(s) decide as written, and 38 constraint(s) and
///       condition(s) are pinned by one of them
/// ```
///
/// — line for line what a clean run prints. And with a widening planted in the
/// permit that case exists to hold, the pair together is *still* green, while
/// `restricted` content is readable by every household member and by the other
/// realm's user. The widening alone is RED, which is what makes the typo the
/// finding.
///
/// The `forbid` half already had this protection from the other side —
/// `denial-proves-nothing` re-runs with the policy removed — and it cannot
/// stand in for this one: a request nothing permits is denied whether the
/// forbids are there or not, and a ghost entity is always such a request.
///
/// The action is not asked for: an action lives in the schema rather than in the
/// world, and `build_request` has already held it against the schema.
fn names_nobody(pack: &Pack, case: &Case, problems: &mut Vec<Problem>) -> bool {
    let mut missing = false;
    for (what, written) in [("principal", &case.principal), ("resource", &case.resource)] {
        let Ok(uid) = EntityUid::from_str(written) else {
            continue;
        };
        if pack.entities.get(&uid).is_some() {
            continue;
        }
        missing = true;
        problems.push(Problem {
            file: case.file.clone(),
            line: case.line,
            what: format!(
                "`{}` names the {what} `{written}`, which policy/world.json does not hold",
                case.name
            ),
            fix: "an entity that is not in the world has no attributes, so every condition \
                  reading one errors and the policy is skipped — a denial then proves that \
                  nothing permitted a request nothing could have permitted. Name an entity \
                  the world declares, or add it there"
                .to_owned(),
            check: "absent-entity",
        });
    }
    missing
}

fn request_for(pack: &Pack, case: &Case, problems: &mut Vec<Problem>) -> Option<Request> {
    match build_request(pack, case) {
        Ok(request) => Some(request),
        Err(what) => {
            problems.push(Problem {
                file: case.file.clone(),
                line: case.line,
                what: format!("`{}` is not a request the schema allows: {what}", case.name),
                fix: "the principal, action, resource and context all have to match \
                      policy/schema.cedarschema, or Cedar would never see this request"
                    .to_owned(),
                check: "malformed-request",
            });
            None
        }
    }
}

fn build_request(pack: &Pack, case: &Case) -> Result<Request, String> {
    let principal = EntityUid::from_str(&case.principal).map_err(|error| error.to_string())?;
    let action = EntityUid::from_str(&case.action).map_err(|error| error.to_string())?;
    let resource = EntityUid::from_str(&case.resource).map_err(|error| error.to_string())?;
    let context = Context::from_json_value(case.context.clone(), Some((&pack.schema, &action)))
        .map_err(|error| error.to_string())?;
    Request::new(principal, action, resource, context, Some(&pack.schema))
        .map_err(|error| error.to_string())
}

/// What the pack answers a request whose context is missing one attribute, with
/// no schema to refuse it first.
///
/// **The seam the gate could not reach.** `build_request` always passes
/// `Some(&pack.schema)`, and the schema refuses a malformed context before any
/// policy runs — so a policy that would have *errored* on a missing attribute,
/// and been skipped by Cedar while a permit answered, looks identical here to
/// one that forbids. That is a property of the decision point, not of the pack,
/// and `commands.dispatch` passes the schema too, through `nc-authz`. What the pack
/// can hold on its own is that its conditions do not error, and that is what this
/// exists to let a test assert.
///
/// The context is built with every key the reference declares except `absent`.
/// `agent_mediated` is what `via_agent` says when it is present: some conditions
/// only bite on an agent-mediated request, and dropping the attribute they read
/// proves nothing about a request they were never going to judge.
///
/// **A name this builder does not hold removed nothing, and said nothing.**
/// `absent` was a `&str` compared against a fixed seven-key list, so a typo, a
/// renamed attribute or an empty string handed the caller a *complete* request
/// and the ordinary decision, with nothing in the answer to tell that from a
/// removal. Measured on the shipped pack: `policy_epoch` answered `Deny` and
/// `policy_epochh` answered `Allow`, and for one row the two were the same
/// either way. Refusing an unknown name outright was measured and is too
/// strong — it refuses the legitimate "remove nothing" call — so the absence is
/// an `Option` and the complete decision comes back beside the reduced one,
/// because a removal that changes nothing is a row proving nothing and only the
/// caller can see that. Found in wave 31.
///
/// # Errors
///
/// Returns why the request could not be built, or that `absent` names no key
/// this builds.
pub fn decide_without_a_schema(
    pack: &Pack,
    principal: &str,
    action: &str,
    resource: &str,
    absent: Option<&str>,
    agent_mediated: bool,
) -> Result<WithoutAnAttribute, String> {
    let principal = EntityUid::from_str(principal).map_err(|error| error.to_string())?;
    let action = EntityUid::from_str(action).map_err(|error| error.to_string())?;
    let resource = EntityUid::from_str(resource).map_err(|error| error.to_string())?;
    let declared = [
        ("via_agent", serde_json::json!(agent_mediated)),
        ("confirmed_by_human", serde_json::json!(false)),
        ("actor_chain_depth", serde_json::json!(0)),
        ("policy_epoch", serde_json::json!(7)),
        ("quorum_met", serde_json::json!(true)),
        (
            "now",
            serde_json::json!({ "__extn": { "fn": "datetime", "arg": "2026-08-26T00:00:00Z" } }),
        ),
        (
            "grant",
            serde_json::json!({ "__entity": { "type": "Grant", "id": "g.current" } }),
        ),
    ];
    if let Some(name) = absent
        && !declared.iter().any(|(key, _)| *key == name)
    {
        return Err(format!(
            "`{name}` names no context key this builds, so nothing would be removed and \
             the answer would be the complete request's own"
        ));
    }
    let decide = |taken_out: Option<&str>| -> Result<String, String> {
        let mut context = serde_json::Map::new();
        for (key, value) in &declared {
            if taken_out != Some(*key) {
                context.insert((*key).to_owned(), value.clone());
            }
        }
        let context = Context::from_json_value(serde_json::Value::Object(context), None)
            .map_err(|error| error.to_string())?;
        let request = Request::new(
            principal.clone(),
            action.clone(),
            resource.clone(),
            context,
            None,
        )
        .map_err(|error| error.to_string())?;
        let decision = Authorizer::new()
            .is_authorized(&request, &pack.policies, &pack.entities)
            .decision();
        Ok(format!("{decision:?}"))
    };
    Ok(WithoutAnAttribute {
        without: decide(absent)?,
        complete: decide(None)?,
    })
}

/// What the pack answers with an attribute gone, and what it answers with it.
///
/// Both, because one of them alone cannot say whether the removal did anything:
/// a row where the complete request is refused too proves that the condition
/// does not error, and nothing about the attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WithoutAnAttribute {
    /// The decision over the context `absent` was taken out of.
    pub without: String,
    /// The decision over the same request with every declared key present.
    pub complete: String,
}

/// Every policy file under `directory`, in path order.
fn policy_files(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    walk(directory, &mut |path| {
        if path
            .extension()
            .is_some_and(|extension| extension == "cedar")
        {
            found.push(path.to_path_buf());
        }
    });
    found.sort();
    found
}

/// Every file under `directory`, depth first.
pub(crate) fn walk(directory: &Path, visit: &mut impl FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            walk(&path, visit);
        } else {
            visit(&path);
        }
    }
}

/// A path relative to the repository root, with forward slashes.
pub(crate) fn relative_to(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn path_of(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod narrowing {
    use super::{narrows_itself, quotes_the_constraint, references_marked, without_line_comments};
    use cedar_policy::Policy;

    /// A policy whose annotation opens no line carries no marker.
    ///
    /// **The anchor's fall-through reinstated the anchor it removed.** When no
    /// `@id("<name>")` began a line, the reader fell back to the first *text*
    /// occurrence — which is the anchor whose removal
    /// [`super::references_marked`]'s own doc records, and which any sentence
    /// naming the policy's id can move. Decided by nothing: every annotation
    /// this pack ships opens its line, so the clause was never run and deleting
    /// it left both suites green. A silent fall-through that fails open.
    ///
    /// What it must not do is refuse a marker on an ordinary policy, so both
    /// shapes are here: the annotation at the start of a line, and the same
    /// annotation written only inside prose.
    #[test]
    fn an_annotation_that_opens_no_line_anchors_nothing() {
        let ordinary = "// nc:narrows: `action in` — the action *is* the rule.\n\
                        @id(\"core.anchored\")\n\
                        forbid (principal, action in [Action::\"Grant\"], resource);\n";
        assert!(
            super::references_marked(ordinary, "core.anchored", "action in", ""),
            "a marker above an annotation that opens its line stopped being read"
        );

        let only_in_prose = "// nc:narrows: `action in` — the action *is* the rule.\n\
                             // This policy is @id(\"core.anchored\") and nothing else.\n\
                             forbid (principal, action in [Action::\"Grant\"], resource);\n";
        assert!(
            !super::references_marked(only_in_prose, "core.anchored", "action in", ""),
            "a policy whose annotation is only a sentence was read as marked, which is \
             the anchor this reader was written to stop using"
        );
    }

    /// A marker's reason names the constraint it excuses, entity and all.
    ///
    /// **Both directions, because this rule has been wrong in both.** It read
    /// the subject and the operator and stopped, so a reason arguing about
    /// `Device` excused a policy narrowed to `Agent` — a type appearing nowhere
    /// in that file. Then it read only the span the reason opens with, so an
    /// author who names the type first, which is how the sentence reads best,
    /// was refused by a message telling them to quote what they had just quoted.
    #[test]
    fn a_reason_names_the_entity_the_constraint_names() {
        for (accepted, reason) in [
            // The shortest form there is: the constraint and nothing after it.
            (true, "`principal is` — a type test and not a named entity"),
            // The same constraint quoted in full.
            (
                true,
                "`principal is Agent` — a type test and not a named entity",
            ),
            // The constraint quoted after the type it is about, which is the
            // sentence a person writes.
            (
                true,
                "an `Agent` is the only type that carries a budget, so `principal is` here \
                 is a type test",
            ),
            // A different type entirely, which is what a type test *is*.
            (
                false,
                "`principal is Device` — a type test and not a named entity",
            ),
            (
                false,
                "`principal is User` and `principal is Device` are both type tests",
            ),
            // A reason about something else, quoting nothing of this policy.
            (false, "the budget is an attribute of the principal"),
        ] {
            assert_eq!(
                quotes_the_constraint(reason, "principal is", "Agent"),
                accepted,
                "`{reason}` should {} excuse `principal is Agent`",
                if accepted { "" } else { "not" }
            );
        }
        // **And a type test may still carry the realm it renders beside.**
        // `principal is Agent in Realm::"home"` is one constraint with an extra
        // word, not two entities, and refusing it is how this rule was wrong
        // before.
        assert!(quotes_the_constraint(
            "`principal is Agent in Realm::\"home\"` — the household's own agents",
            "principal is in",
            "Agent",
        ));

        // **And the counterexample quotation stays refused.**
        // `core.no_self_escalation`'s paragraph quotes the thing it warns
        // against, and reading every span is what made that a risk again: the
        // entity is what tells the warning from the excuse.
        assert!(!quotes_the_constraint(
            "rescoping this to `action == Action::\"Read\"` is what went wrong before",
            "action ==",
            "Action::\"Delete\"",
        ));
        assert!(quotes_the_constraint(
            "deleting is the one destructive action, so `action == Action::\"Delete\"` is \
             the whole of it",
            "action ==",
            "Action::\"Delete\"",
        ));

        // **A list is a name too, and it is the whole list.** Cedar renders
        // `action in [Action::"Grant", Action::"Revoke"]` with an `entities`
        // array, which `the_entity_named` read as naming nothing — so the only
        // reason this rule could accept was one that stopped at the operator,
        // and an author quoting the constraint off the policy's own first line
        // was refused by a `fix` telling them to name what they had just named.
        // A reason for one verb of a two-verb list is a reason for a constraint
        // this policy does not have, and stays refused.
        let both = "Action::\"Grant\" Action::\"Revoke\"";
        for (accepted, reason) in [
            (true, "`action in` — the two verbs *are* the rule"),
            (
                true,
                "`action in [Action::\"Grant\", Action::\"Revoke\"]` — the two verbs *are* \
                 the rule",
            ),
            (
                true,
                "granting and revoking are the subject matter, so `action in \
                 [Action::\"Revoke\", Action::\"Grant\"]` is the invariant and not an \
                 exemption",
            ),
            (
                false,
                "`action in Action::\"Grant\"` — granting is the rule",
            ),
            (
                false,
                "`action in [Action::\"Grant\", Action::\"Delete\"]` — a pair this policy \
                 does not name",
            ),
            // **And a list longer than the policy's**, which the subset rule
            // accepted: a marker written for two verbs went on excusing the
            // policy after it was rescoped to one, which is the drift a marker
            // per constraint exists to catch.
            (
                false,
                "`action in [Action::\"Grant\", Action::\"Revoke\", Action::\"Delete\"]` — a \
                 trio this policy does not name",
            ),
            // **And a list an author spaces out.** The brackets and commas are
            // trimmed off each word, so `[ Action::"Grant" , Action::"Revoke" ]`
            // leaves empty words behind them — and the filter that drops those
            // was decided by no row, which is the frozen-not-covered shape this
            // file carries elsewhere.
            (
                true,
                "`action in [ Action::\"Grant\" , Action::\"Revoke\" ]` — the two verbs *are* \
                 the rule",
            ),
        ] {
            assert_eq!(
                quotes_the_constraint(reason, "action in", both),
                accepted,
                "`{reason}` should {} excuse a two-verb `action in`",
                if accepted { "" } else { "not" }
            );
        }
    }

    /// The excuse is on the marker's own line, and the paragraph under it is
    /// prose.
    ///
    /// **The reason a marker carries is not the same length as the reason a
    /// person reads.** `core.no_self_escalation`'s marker is followed, four
    /// lines down, by the sentence that explains why it is written at all: a
    /// blanket exemption once let `core.provenance` be rescoped to
    /// `action == Action::"Read"`. That sentence quotes the constraint it warns
    /// *against*, and the whole comment run was one reason, so it excused
    /// exactly that rescoping of the policy it sits on. Measured on 2026-09-11:
    /// invariant 4 narrowed to `action == Action::"Read"` produced no
    /// `core-narrows-by-entity-type` at all.
    ///
    /// The first fixture below is that file's shape, reduced to the two lines
    /// that matter. The third is what this must not cost: the constraint named
    /// after the type it is about, which is how the sentence reads best and is
    /// what the earlier repair here was reverted for — one line, so both spans
    /// are still on it.
    #[test]
    fn the_excuse_is_on_the_markers_own_line() {
        let warned = "// nc:narrows: `action in` — the action *is* the rule.\n\
                      // It is written here because a blanket exemption let\n\
                      // `core.provenance` be rescoped to `action == Action::\"Read\"`.\n\
                      @id(\"core.one\")\n\
                      forbid (principal, action in [Action::\"Grant\"], resource);\n";
        assert!(
            references_marked(warned, "core.one", "action in", ""),
            "the marker's own line excuses the constraint it names"
        );
        assert!(
            !references_marked(warned, "core.one", "action ==", "Action::\"Read\""),
            "a paragraph warning about a constraint is not an excuse for it"
        );

        let ordered = "// nc:narrows: an `Agent` is the only type with a budget, so \
                       `principal is` here is a type test.\n\
                       @id(\"core.two\")\n\
                       forbid (principal is Agent, action, resource);\n";
        assert!(
            references_marked(ordered, "core.two", "principal is", "Agent"),
            "the type named before the constraint is still one line"
        );

        // **And the cost, written down rather than discovered.** A marker whose
        // line carries no quote is refused however well the lines under it
        // argue. That is the rule, not an oversight: the six markers this pack
        // ships all quote on their own line, and the diagnostic says so.
        let below = "// nc:narrows:\n\
                     // `action in` — the action *is* the rule.\n\
                     @id(\"core.three\")\n\
                     forbid (principal, action in [Action::\"Grant\"], resource);\n";
        assert!(
            !references_marked(below, "core.three", "action in", ""),
            "a quote on the line under the marker does not excuse"
        );
    }

    /// A pack's floor, and what the gate reports about it.
    ///
    /// Goes through [`super::report_narrowings`] rather than
    /// [`super::narrows_itself`], because the rule under test is the *filter*
    /// in front of it and nothing reached that filter before: `report_narrowings`
    /// had no caller in this module, the shipped pack's one `forbid` carried no
    /// narrowing, and the whole pack branch could be deleted with every test in
    /// the workspace green.
    fn floor_problems(scope: &str, relative: &str) -> Vec<String> {
        let text = format!("@id(\"pack.floor\")\nforbid (\n  {scope}\n);\n");
        let policy: Policy = text.parse().expect("the fixture parses");
        let mut problems = Vec::new();
        super::report_narrowings(
            &policy,
            SCHEMA,
            &text,
            "pack.floor",
            relative,
            &mut problems,
        );
        problems.into_iter().map(|problem| problem.what).collect()
    }

    /// A pack's floor may say what kind of thing it is about, and may not say
    /// which ones.
    ///
    /// The operator is the line. A type test is a rule having a subject —
    /// refusing those would refuse `forbid (principal is Agent, …)`, which is
    /// an ordinary pack rule. Naming an entity takes that entity's class
    /// partly out from under the floor, and the class is what a floor is for.
    ///
    /// **The axis is not the line, and reading it as one was the defect.** The
    /// rule reported `principal` narrowings only, on the argument that a pack
    /// says which actions and resources it governs. `policy/core/README.md`
    /// records the same argument being tried for the core and being a hole.
    /// Measured on the shipped pack: rescoping its floor from `resource` to
    /// `resource in Folder::"household"` left all four `ok` lines byte-identical
    /// while restricted content in every other folder was shareable with no
    /// quorum.
    #[test]
    fn a_pack_floor_that_names_an_entity_is_reported() {
        /// A path under `policy/packs/`, which is what makes a `forbid` a
        /// pack's floor rather than a core invariant.
        const PACK: &str = "policy/packs/family/permissions.cedar";
        for (reported, scope) in [
            // A rule having a subject.
            (false, "principal, action, resource is Content"),
            (false, "principal is Agent, action, resource"),
            (false, "principal, action, resource"),
            // Each of these leaves the rest of a class uncovered.
            (true, "principal, action, resource in Folder::\"household\""),
            (true, "principal == Agent::\"ana\", action, resource"),
            (true, "principal, action == Action::\"Share\", resource"),
        ] {
            let found = floor_problems(scope, PACK);
            assert_eq!(
                !found.is_empty(),
                reported,
                "`{scope}` as a pack floor should {} be reported: {found:?}",
                if reported { "" } else { "not" }
            );
        }
    }

    /// `is in` is a tenant scoping wearing a type test's operator.
    ///
    /// Asserted against the filter rather than through a fixture policy,
    /// because the fixture schema declares no realm to scope into — the
    /// end-to-end path is [`super::narrows_itself`]'s own, which splits the two
    /// operators and has the measurement that an invariant held in one realm
    /// and was absent in every other one a device could be enrolled into.
    #[test]
    fn a_scoping_that_wears_a_type_tests_operator_is_not_a_type_test() {
        assert!(super::a_type_test("`principal is` in its scope"));
        assert!(super::a_type_test("`resource is Content`"));
        assert!(!super::a_type_test("`principal is in` in its scope"));
        assert!(!super::a_type_test("`resource in` in its scope"));
        assert!(!super::a_type_test("`principal ==` in its scope"));
        // A subject with no operator at all narrows nothing and is not a type
        // test either; the caller never hands one over, and a predicate that
        // answered `true` here would excuse whatever a future caller did.
        assert!(!super::a_type_test("`principal`"));
    }

    /// A pack's floor narrowed in a `when` is reported, and a type test there is
    /// excused only as a guard.
    ///
    /// The scope reader splits `is in` from `is` and has carried a paragraph
    /// about why since a marker written for `principal is Agent` excused
    /// `principal is Agent in Realm::"home"`. The condition reader rendered the
    /// same shape as a bare type test, and once a pack's floor was allowed to be
    /// a type test that made `resource is Content in Folder::"household"` in a
    /// `when` invisible — measured on the shipped floor, every `ok` line
    /// byte-identical, and no case able to see it because the shipped world has
    /// exactly one folder.
    ///
    /// A plain type test is held to more in a `when` than in the scope: it is
    /// excused only where it guards a read of the same subject, because in a
    /// condition it can stand beside one disjunct and leave the others.
    #[test]
    fn a_pack_floor_narrowed_in_a_condition_is_reported() {
        /// A pack floor whose scope is open and whose `when` carries the
        /// narrowing, which is where a pack rule naturally says what it is
        /// about.
        fn condition_problems(condition: &str) -> Vec<String> {
            let text = format!(
                "@id(\"pack.floor\")\nforbid (\n  principal,\n  action,\n  resource\n)\nwhen {{ {condition} }};\n"
            );
            let policy: Policy = text.parse().expect("the fixture parses");
            let mut problems = Vec::new();
            super::report_narrowings(
                &policy,
                SCHEMA,
                &text,
                "pack.floor",
                "policy/packs/family/permissions.cedar",
                &mut problems,
            );
            problems.into_iter().map(|problem| problem.what).collect()
        }

        // A type test in a condition that guards a read of the same subject is
        // a rule saying what it is about.
        assert!(
            condition_problems("resource is User && resource.realm == \"home\"").is_empty(),
            "a pack rule may say what kind of thing it governs, beside what it reads"
        );
        // **One that guards nothing is not.** The exemption covered every type
        // test in a condition, so a floor rewritten as `(principal.status ==
        // "suspended" && resource is Content)` held for one kind of resource and
        // printed the policy block byte-identical. The same test belongs in the
        // scope, and in a condition it is reported: alone, beside a read of
        // another subject, and negated.
        for condition in [
            "resource is User",
            "principal.realm == \"home\" && resource is User",
            "principal.realm == \"home\" && !(resource is User)",
        ] {
            let found = condition_problems(condition);
            assert!(
                found.iter().any(|what| what.contains("`resource is User`")),
                "`{condition}` narrows a floor to one kind of resource and guards no read \
                 of it, and it was not reported: {found:?}"
            );
        }
        // The same words with an entity beside them are not.
        let found = condition_problems("resource is User in Agent::\"ana\"");
        assert!(
            !found.is_empty(),
            "a floor narrowed to one named entity in a `when` leaves the rest of its class \
             uncovered, and it is reported wherever it is written: {found:?}"
        );
    }

    /// The pack branch is a branch, and the core branch still judges every type.
    ///
    /// A core invariant holds for every principal, every action and every
    /// resource the schema declares — including every *type* — so the type-test
    /// exemption above must not reach it. Written because the exemption was
    /// added for packs and the two branches share one loop.
    #[test]
    fn the_type_test_exemption_does_not_reach_a_core_invariant() {
        let found = floor_problems(
            "principal, action, resource is Content",
            "policy/core/quorum.cedar",
        );
        assert!(
            !found.is_empty(),
            "a core invariant that holds for one type only is still a narrowing: {found:?}"
        );
    }

    /// One policy, parsed — which is what the check reads now.
    fn parsed(scope: &str, condition: &str) -> Policy {
        format!("permit(\n  {scope}\n) when {{ {condition} }};")
            .parse()
            .expect("the fixture parses")
    }

    /// Whether the check reports this policy as narrowing itself, and how.
    ///
    /// **Every narrowing, not the first.** The function returned one, so a
    /// policy with two — a marked scope constraint and an unmarked one beside
    /// it — reported only the one the loop reached first.
    fn narrows(scope: &str, condition: &str) -> Option<String> {
        narrows_itself(&parsed(scope, condition), SCHEMA)
            .into_iter()
            .next()
            .map(|(_, what, _)| what)
    }

    /// Every narrowing the check reports for one policy.
    fn all_narrowings(scope: &str, condition: &str) -> Vec<String> {
        narrows_itself(&parsed(scope, condition), SCHEMA)
            .into_iter()
            .map(|(_, what, _)| what)
            .collect()
    }

    /// Two types, one attribute shared and one not — which is the whole
    /// distinction `has` turns on.
    const SCHEMA: &str = "entity User = { \"realm\": String };\n\
                          entity Agent = { \"realm\": String, \"autonomy\": String };\n";

    #[test]
    fn a_scope_splits_at_its_top_level_commas() {
        // `action in [Action::"Grant", Action::"Revoke"]` holds one, so
        // splitting on every comma gave four parts for three constraints — and
        // every variant built from them failed to re-parse and was skipped,
        // which is how the widening `permit-constraint-untested` exists for went
        // unreported by the check that was written to catch it.
        let parts = super::top_level_parts(
            "principal in Group::\"role.guardian\", action in [Action::\"Grant\", \
             Action::\"Revoke\"], resource",
        );
        assert_eq!(parts.len(), 3, "{parts:?}");
        assert!(parts[1].contains("Grant"), "{parts:?}");
        assert!(parts[1].contains("Revoke"), "{parts:?}");
    }

    #[test]
    fn every_constraint_of_a_permit_is_offered_for_widening() {
        // One variant per constraint, each dropping exactly that one.
        let rendered = "permit(principal in Group::\"g\", action in [Action::\"A\", \
                        Action::\"B\"], resource);";
        let offered = super::widenings(rendered);
        assert_eq!(offered.len(), 2, "{offered:?}");
        assert!(offered[0].1.contains("permit(principal,"), "{offered:?}");
        assert!(offered[1].1.contains("action,"), "{offered:?}");
        // An already-unconstrained policy offers nothing, which is why a permit
        // that has *already* been widened is caught by the case that pinned the
        // constraint rather than by this.
        assert_eq!(
            super::widenings("permit(principal, action, resource);"),
            [] as [(std::string::String, std::string::String); 0]
        );

        // **The shape the production path actually hands it**, which is the one
        // this test did not have. Cedar renders the annotations first, and every
        // policy in this pack is required to carry `@id("…")` — so the first `(`
        // in a real rendering is `@id`'s, and this function was reading the id
        // as the scope. It produced one unparseable variant per policy, the call
        // site's `else { continue; }` swallowed it, and the check had never
        // fired for any policy the loader accepts.
        //
        // The fixture above has no annotation, so it was green about a shape
        // nothing meets. Both are kept: the difference between them is the
        // defect.
        let annotated = "@id(\"pack.a_rule\")\npermit (\n  principal in Group::\"g\",\n  \
                         action in [Action::\"A\", Action::\"B\"],\n  resource\n);";
        let offered = super::widenings(annotated);
        assert_eq!(
            offered.len(),
            2,
            "the annotation was read as the scope: {offered:?}"
        );
        for (constraint, widened) in &offered {
            assert!(
                !constraint.contains("pack.a_rule"),
                "the id was offered as a constraint: {offered:?}"
            );
            assert!(
                widened.starts_with("@id(\"pack.a_rule\")"),
                "the widened policy lost its id, so it cannot re-parse: {widened}"
            );
            assert!(
                cedar_policy::Policy::parse(None, widened).is_ok(),
                "a variant that will not re-parse is silently skipped: {widened}"
            );
        }
    }

    #[test]
    fn a_constrained_scope_is_a_narrowing() {
        // A core invariant is written `forbid (principal, action, resource)` —
        // unconstrained, because it applies to everyone. These never touch a
        // condition, so a rule reading only `when`/`unless` never saw them.
        // **The action too.** It was excluded on the argument that an action
        // constraint is how a rule says what it governs — true of
        // `core.no_self_escalation`, and measured to be a hole everywhere else:
        // `core.provenance` rescoped to `action == Action::"Read"` gave every
        // write a pass with no grant at all and passed the whole gate. The two
        // policies that need one now carry `nc:narrows:` with the reason.
        for scope in [
            "principal, action == Action::\"Read\", resource",
            "principal, action in [Action::\"Grant\", Action::\"Revoke\"], resource",
            "principal == Agent::\"a\", action, resource",
            "principal in Agent::\"a\", action, resource",
            "principal is Agent, action, resource",
            "principal, action, resource is Content",
        ] {
            assert!(
                narrows(scope, "true").is_some(),
                "`{scope}` is an exemption and was not reported"
            );
        }
        // And the shape a core invariant is written in.
        assert!(narrows("principal, action, resource", "true").is_none());
    }

    #[test]
    fn a_condition_narrows_however_it_is_spelled() {
        // **Every one of these was a separate fix to a string matcher**, and the
        // last of them — one token, neither `is` nor `has` — is what said the
        // matcher was the wrong shape. Read from the EST, they are five node
        // kinds rather than five spellings.
        for condition in [
            "principal is Agent",
            "principal has autonomy",
            "principal == Agent::\"a\"",
            "principal in Agent::\"a\"",
            "Agent::\"a\" == principal",
            // And wrapped, because a narrowing inside an operator is still one.
            "true && principal is Agent",
            "!(resource is Content)",
            // **The negated `has`, which was excused wherever it stood.** An
            // attribute only one entity type declares makes `!(x has y)` an
            // exact test for *is not that type* — so this is `resource is not a
            // Feature` in different words, and the early return written for the
            // fail-closed disjunction `!(x has y) || x.y <op> v` covered it.
            // Measured, appending one of these to `core.fail_closed` left
            // invariant 1 off for every principal but a device with the whole
            // gate green.
            "!(principal has node_key)",
            "!(resource has tier)",
            "true && !(principal has node_key)",
            // And on either side of a `||` whose other half does not use the
            // attribute: two type tests wearing a guard's clothes.
            "!(principal has node_key) || resource is Content",
            "resource is Content || !(principal has node_key)",
            "principal is Agent || resource is Content",
            // **The spellings a syntax-tree walk was supposed to make
            // impossible**, and did not: the scan asked whether an operand *was*
            // a bare variable and gave up on the whole node when it was not.
            // Each of these is the same test as one above with a wrapper round
            // it, and each loaded clean against the shipped schema — one of them
            // turning `core.fail_closed`, invariant 1, off for every agent.
            "(if true then principal else principal) is Agent",
            "(if true then principal else principal) has autonomy",
            "principal in [Group::\"role.guardian\"]",
            "principal in [Agent::\"a\", Agent::\"b\"]",
            "resource in [Feature::\"gallery\"]",
            "principal == (if true then User::\"carla\" else User::\"ana\")",
            "principal.operator == User::\"ana\"",
            // **The membership family this walk could not see at all**, because
            // a `Set`'s body is a JSON array and the descent read only objects.
            // `[Agent::"a"].contains(principal)` is the same test as
            // `principal == Agent::"a"`, and it turned invariant 1 off for
            // `Delete`, `Grant` and `Revoke` with the policy section green.
            "[Agent::\"a\"].contains(principal)",
            "[Agent::\"a\", Agent::\"b\"].containsAny([principal])",
            "[Agent::\"a\"].containsAll([principal])",
            "[Action::\"Read\"].contains(action)",
            "[Content::\"photo\"].contains(resource)",
            "[principal].contains(Agent::\"a\")",
            "true && [Agent::\"a\"].contains(principal)",
            "if true then [Agent::\"a\"].contains(principal) else false",
        ] {
            assert!(
                narrows("principal, action, resource", condition).is_some(),
                "`{condition}` turns the invariant off and was not reported"
            );
        }

        // And what must stay legal, or a core invariant could not have a
        // condition at all.
        for condition in [
            "true",
            // **A context condition, deliberately.** Every one of these
            // invariants is written with one — `core.provenance` is
            // `unless { context has grant }` — and nothing structural separates
            // an escape that is the rule's own definition from one that is a way
            // around it. Reporting these would report `core.provenance` for
            // doing what it exists to do, which is the wrong-direction failure.
            "context.via_agent",
            "context has via_agent",
            "if context.via_agent then true else false",
            // An attribute *every* entity type declares is not a type test.
            "principal has realm",
            // **The fail-closed disjunction, which is why the negated `has` was
            // ever excused.** The half beside the guard *uses* the value the
            // guard proved is there, and that is what makes the pair a floor
            // rather than a type test: the `forbid` then applies to a principal
            // whose type does not declare the attribute, which is what a floor
            // has to do when the decision point forgets the schema. Both shipped
            // core policies that need one are this shape.
            "!(principal has budget_remaining) || principal.budget_remaining <= 0",
            "principal.budget_remaining <= 0 || !(principal has budget_remaining)",
            "!(resource has sensitivity) || resource.sensitivity != \"high\"",
            // Reading a context value and comparing it is not gating on one.
            "context.grant.reason != \"a device that presents its own key\"",
            // A string literal that reads like a rule is a `Value` node, and
            // cannot be mistaken for an operator by a tree walk.
            "context.grant.reason != \"the principal is a device\"",
            // Comparing two values, neither of them an entity literal.
            "principal.realm == resource.realm",
            // **A subject in one half and an entity in the other.** The operand
            // rule gathered both across a boolean combinator, so these — each
            // half individually accepted, and the second half being
            // `core.no_self_escalation`'s own condition — were reported as
            // turning the invariant off, naming `&&` as the operator.
            "principal == principal && User::\"ana\" == User::\"ana\"",
            "principal == resource || (context has grant \
             && context.grant.granted_by == User::\"ana\")",
            "context has grant && context.grant.granted_by == User::\"ana\"",
        ] {
            assert!(
                narrows("principal, action, resource", condition).is_none(),
                "`{condition}` is not a narrowing and was reported"
            );
        }
    }

    #[test]
    fn every_scope_subject_is_examined_and_not_only_the_first() {
        // **This returned at the first non-`All` subject**, so a marker excusing
        // that one left the other two unexamined — measured, eight of nine scope
        // constraints added across the marked core policies went unreported.
        // `forbid (principal, action == Action::"Delete", resource is Content)`
        // means deleting a `User` or a `Feature` needs no quorum, which is the
        // file's own worked example moved from the condition to the scope.
        let both = all_narrowings(
            "principal is Agent, action == Action::\"Delete\", resource",
            "true",
        );
        assert_eq!(both.len(), 2, "{both:?}");
        assert!(
            both.iter().any(|what| what.contains("principal")),
            "{both:?}"
        );
        assert!(both.iter().any(|what| what.contains("action")), "{both:?}");

        let all_three = all_narrowings(
            "principal is Agent, action == Action::\"Delete\", resource is Content",
            "true",
        );
        assert_eq!(all_three.len(), 3, "{all_three:?}");

        // And a condition narrowing is reported beside a scope one rather than
        // instead of it.
        let mixed = all_narrowings(
            "principal is Agent, action, resource",
            "resource is Content",
        );
        assert_eq!(mixed.len(), 2, "{mixed:?}");
    }

    #[test]
    fn the_shipped_schema_answers_both_ways() {
        // **The rule was measured returning `false` for every attribute**, which
        // makes the `has` branch fire on everything and the exemption it grants
        // unreachable — a check that always says yes decides nothing. Both
        // answers have to be reachable against the schema that ships, or this is
        // a rule with one outcome.
        let schema = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../policy/schema.cedarschema"),
        )
        .expect("the shipped schema is readable");

        // **No attribute of this schema is shared by every type**, so the
        // exemption is unreachable against it — and that is the schema's shape
        // rather than a broken rule: `Group` and `Folder` declare no attributes
        // at all, and the three principal types share none. Every `has` in a
        // core invariant is a type test today, which is the safe direction. It
        // is written down here because a branch with no instance is otherwise
        // indistinguishable from a branch that does not work, and this one was
        // reported as the second.
        for discriminating in ["realm", "budget_remaining", "autonomy", "node_key", "tier"] {
            assert!(
                !super::declared_by_every_entity(&schema, discriminating),
                "`{discriminating}` is declared by every entity type of the shipped \
                 schema, which nothing in it does today"
            );
        }

        // **And the exemption, exercised** — against a schema that does have a
        // universal attribute, because the branch has no instance in the shipped
        // one and a branch nothing runs is a branch nobody knows works.
        let shared = "entity User = { \"realm\": String };\n\
                      entity Agent = { \"realm\": String, \"autonomy\": String };\n";
        assert!(
            super::declared_by_every_entity(shared, "realm"),
            "an attribute every type declares was still read as a type test"
        );
        assert!(!super::declared_by_every_entity(shared, "autonomy"));
        // Including an entity with no attribute block at all, which is what
        // makes the shipped schema answer the way it does.
        let bare = format!("{shared}entity Folder in [Realm];\n");
        assert!(
            !super::declared_by_every_entity(&bare, "realm"),
            "a type that declares no attributes was counted as declaring one"
        );

        // The count itself, so a walk that read no entity at all cannot pass by
        // finding nothing to disagree with.
        assert_eq!(
            super::entity_types(&schema),
            schema
                .lines()
                .filter(|line| line.trim_start().starts_with("entity "))
                .count(),
            "the schema walk and the file disagree about how many entity types there are"
        );
    }

    #[test]
    fn a_comment_in_the_schema_is_not_an_entity_declaration() {
        // `declared_by_every_entity` counted three of the schema's own sentences
        // as types — two borrowed a neighbouring attribute block and one
        // incremented the count without reading a block at all, which is the
        // only reason it never returned `true`. A comment was load-bearing for
        // whether a core invariant could narrow itself.
        let commented = "// never as an entity with a schema of its own\nentity User = { \"realm\": String };\n";
        assert!(!without_line_comments(commented).contains("never as an entity"));
        assert!(without_line_comments(commented).contains("entity User"));
    }
}

#[cfg(test)]
mod the_mutation_that_would_not_build {
    /// A proposed variant that will not parse is reported, not counted.
    ///
    /// **`mutation-unbuildable` was the last of `nc-policy`'s thirty checks with
    /// no case.** Its own doc says "Zero today; what this asserts is that it
    /// stays zero" — and nothing asserted that it would report one: deleting the
    /// body left the suite green. The number it guards is what stands behind
    /// `condition-untested` and `branch-untested`, because a mutation that never
    /// compiled is a mutation no case could have noticed.
    ///
    /// It is a pure function of a count, so the case is a call.
    #[test]
    fn a_variant_that_would_not_build_is_reported() {
        let seen = super::unbuildable_variants(0);
        assert!(
            seen.is_empty(),
            "nothing failed to build and it reported something: {seen:?}"
        );
        let said = super::unbuildable_variants(3);
        assert_eq!(said.len(), 1, "three unbuildable variants are one finding");
        assert_eq!(said[0].check, "mutation-unbuildable");
        assert!(
            said[0].what.contains('3'),
            "the finding does not say how many: {}",
            said[0].what
        );
    }
}

#[cfg(test)]
mod the_dereference_excuse {
    use super::guards_a_dereference;

    /// Every conjunct shape the dereference excuse is asked about, and what
    /// it answers.
    ///
    /// Out of the test it drives because the test was 109 lines and clippy's
    /// limit is 100 — the table is the thing worth reading, so the table is
    /// what got the name.
    const THE_SHAPES_THE_EXCUSE_IS_ASKED_ABOUT: [(&str, &str, bool); 21] = [
        // **The shape the excuse is for, and the four it is not.** Measured
        // by a reviewer in wave 14: each of the last four was excused by the
        // nine-shape list, and the table had no row at all in which this
        // branch answered `true`, so the shape it exists for was pinned by
        // nothing either.
        (
            "!(context has via_agent) || context.via_agent",
            "when { !(context has via_agent) || context.via_agent }",
            true,
        ),
        (
            "!(context has via_agent) || !context.via_agent",
            "when { !(context has via_agent) || !context.via_agent }",
            false,
        ),
        (
            "!(context has via_agent) || context.quorum_met",
            "when { !(context has via_agent) || context.quorum_met }",
            false,
        ),
        (
            "!(context has tags) || context.tags.contains(\"x\")",
            "when { !(context has tags) || context.tags.contains(\"x\") }",
            false,
        ),
        (
            "!(context has via_agent) || context.via_agent || resource.public",
            "when { !(context has via_agent) || context.via_agent || resource.public }",
            false,
        ),
        // The three shapes it is written for.
        (
            "!(context has policy_epoch) || context.policy_epoch <= 0",
            "when { !(context has policy_epoch) || context.policy_epoch <= 0 }",
            // **Not excused any more.** The guard is on the left and an
            // ordinary predicate is on the right, and a predicate is a
            // clause a case can decide. Excusing the pair whole put this
            // clause outside the mutation, so `condition-untested` could
            // not be reached by a guarded conjunct while
            // `guards_fail_closed` claimed every unguarded one — between
            // them the verdict had no reachable input on a core policy.
            // Narrowing it raised the pinned count from 32 to 35 with no
            // new finding: all three clauses it freed are held by cases.
            false,
        ),
        // **A `has` that guards nothing is a condition, not a guard.** This
        // row asserted `true` and was the place the weaker behaviour was
        // written down: a `has` with no dereference anywhere in the policy,
        // excused out of the mutation loop. Its sibling row two below has
        // always held the same rule for `is`.
        ("resource has owner", "when { resource has owner }", false),
        // And the same clause where something does read the attribute,
        // which is what the excuse is for: the `has` is there so the line
        // below it is legal to write.
        (
            "resource has owner",
            "when { resource has owner && resource.owner == principal }",
            true,
        ),
        // **And an attribute that merely starts with this one.** `contains`
        // read `resource.owner_id` as a dereference of `owner`.
        (
            "resource has owner",
            "when { resource has owner && resource.owner_id == principal }",
            false,
        ),
        // **And an annotation, which reads nothing.** `to_string` renders
        // one verbatim, so prose naming the path satisfied this excuse and
        // took the conjunct out of the mutation loop.
        (
            "resource has owner",
            "@doc(\"resource.owner\")\npermit(principal, action, resource)\nwhen { resource has owner };",
            false,
        ),
        // And a string literal the policy compares against, for the same
        // reason: it is a value, not a dereference.
        (
            "resource has owner",
            "when { resource has owner && resource.kind == \"resource.owner\" }",
            false,
        ),
        // And the same two shapes on the `is` branch, which asked the raw
        // render until wave 13.
        (
            "principal is User",
            "@doc(\"principal.status\")\npermit(principal, action, resource)\nwhen { principal is User };",
            false,
        ),
        (
            "principal is User",
            "when { principal is User && resource.kind == \"principal.status\" }",
            false,
        ),
        // **And a conjunct that is not single, which is what `single`
        // decides.** Widening the `has` branch to require a dereference left
        // `let single = !text.contains("&&")` surviving mutation: with
        // `single = true` this row is excused — one ` has `, and the
        // attribute *is* read beside it — while the clause is two tests, the
        // second of which is an ordinary predicate a case can decide.
        (
            "resource has owner && resource.owner == principal",
            "when { resource has owner && resource.owner == principal }",
            false,
        ),
        // **And the branch `single` actually decides, which is the `is`
        // one.** Widening the `has` branch to require a dereference made
        // `single` redundant there — the attribute it reads out of a
        // two-test conjunct is never a path the policy holds — and
        // `let single = !text.contains("&&")` then survived mutation. Here
        // it is load-bearing: with `single = true` this conjunct is excused
        // as a type guard while it is two tests, the second of which is an
        // ordinary predicate a case can decide.
        (
            "principal is User && principal.status == \"active\"",
            "when { principal is User && principal.status == \"active\" }",
            false,
        ),
        (
            "principal is User",
            "when { principal is User && principal.realm == resource.realm }",
            true,
        ),
        // A real condition, which is what the loop exists to pin.
        (
            "context.policy_epoch <= 0",
            "when { context.policy_epoch <= 0 }",
            false,
        ),
        (
            "!(context has via_agent && context.via_agent)",
            "when { !(context has via_agent && context.via_agent) }",
            false,
        ),
        (
            "resource.sensitivity == \"private\"",
            "when { resource.sensitivity == \"private\" }",
            false,
        ),
        // `x is T` with nothing that dereferences `x`: the guard guards
        // nothing, so it is a condition like any other.
        (
            "principal is User",
            "when { principal is User && resource.realm == \"family\" }",
            false,
        ),
        // A `has` that is not alone is part of a compound condition.
        (
            "resource has owner && resource.owner == principal",
            "when { resource has owner && resource.owner == principal }",
            false,
        ),
    ];

    /// The one excuse that skips a conjunct, on both of its answers.
    ///
    /// **It had no test at all**, and `condition-untested` — the finding it
    /// suppresses — appears in exactly one place in this repository: where it is
    /// produced. So `if true { return true; }` at the top of this function
    /// excused every conjunct, took the pinned count from 28 to 21, and let
    /// `core.fail_closed` grow `&& !(context has via_agent && context.via_agent)`
    /// with `check passed` — every agent-mediated request out from under the
    /// fail-closed rule, reported by nothing.
    ///
    /// A guard is a clause that makes the *next* clause legal to write: `x has y`
    /// before `x.y`, `x is T` before `x.field`. Anything else is a condition, and
    /// a condition no case pins is a condition nobody would notice the loss of.
    ///

    #[test]
    fn a_guard_is_excused_and_a_condition_is_not() {
        for (conjunct, rendered, guard) in THE_SHAPES_THE_EXCUSE_IS_ASKED_ABOUT {
            assert_eq!(
                guards_a_dereference(conjunct, rendered),
                guard,
                "`{conjunct}` in `{rendered}`"
            );
        }
    }

    /// An attribute inside a set is removed too.
    ///
    /// **`remove_everywhere`'s `Array` arm was driven by nothing.**
    /// `policy/world.json` declares no set-valued attribute, so the recursion is
    /// never handed one — found by instrumenting all 146 loop and closure bodies
    /// in these four crates and running the suite once: 137 fired, and this was
    /// one of the two real cold sites. The arm can be deleted with nothing red,
    /// which makes `guards_fail_closed`'s proof a no-op the day a set attribute
    /// is added: the attribute would still be there, the guard would still read
    /// it, and "denies with the attribute gone" would be answered about a world
    /// where it is not gone.
    ///
    /// The sibling comment one level up records fixing exactly this shape —
    /// *"Everywhere, not only at the top … Latent while the schema declares no
    /// record attributes"* — and this arm arrived without a case.
    #[test]
    fn an_attribute_inside_a_set_is_removed_too() {
        let mut world: serde_json::Value = serde_json::from_str(
            r#"{"grants": [{"via_agent": true, "kept": 1}, {"nested": {"via_agent": 2}}]}"#,
        )
        .expect("the probe parses");
        super::remove_everywhere(&mut world, "via_agent");
        assert_eq!(
            world.to_string(),
            r#"{"grants":[{"kept":1},{"nested":{}}]}"#,
            "an attribute inside a set survived removal, so a guard reading it would be \
             proven against a world where it is still there"
        );
        // And the floor in the other direction: nothing else is touched.
        let mut untouched: serde_json::Value =
            serde_json::from_str(r#"{"a": [1, 2], "b": {"c": 3}}"#).expect("the probe parses");
        super::remove_everywhere(&mut untouched, "via_agent");
        assert_eq!(untouched.to_string(), r#"{"a":[1,2],"b":{"c":3}}"#);
    }
}
