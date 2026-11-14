//! The policy pack, as a step of `cargo xtask check`.
//!
//! `NOCTUA.md §14.9`'s seventh invariant is the one that cannot be a policy:
//! **every policy has a denial test, and a policy without a negative test does
//! not enter the monorepo.** A Cedar rule cannot require that somebody wrote a
//! test for it, so it is a gate, and this is the gate.
//!
//! Three things are checked and each reports its count, because a step that
//! inspected nothing has not passed (`§19.3`):
//!
//! * the pack parses and **validates against the schema** — a policy naming an
//!   attribute the schema has no word for can never match, and Cedar says
//!   nothing about it at run time;
//! * every case does what it says, run through Cedar's own authorizer;
//! * every policy has a denial test that could not pass against an empty pack.

/// The core policies, as the files that hold them.
///
/// **Not "`§14.9`'s seven".** `§14.9` names seven
/// invariants; two of them — re-evaluation on every request, and every policy
/// having a denial test — are deliberately *not* policies, and
/// `policy/core/README.md` says so in the rows that carry them. What is on disk
/// is `§14.9`'s invariants 1 to 5 plus four of `§14.6`'s rules, and that is
/// **eleven rules in seven files**: `grants_expire.cedar` carries three, and
/// `agent_authority.cedar` and `quorum.cedar` carry two each.
///
/// **The numbers in this paragraph are held.** `CORE_RULES` two screens down is
/// `[&str; 11]` and `grants_expire.cedar` holds `core.grants_expire`,
/// `core.grant_has_begun` and `core.grant_not_expired`, and
/// `the_written_numbers_are_the_numbers` reads this comment and holds both
/// against the tree, which is the treatment `PINNED` already has for the same
/// reason: a number in prose beside a constant is a number that rots, and this
/// is the paragraph a reader consults to decide whether the core is complete.
///
/// The two sevens are a coincidence, and a constant that reads as though they
/// were the same set sends a reader to `§14.9` for two files that are not in it.
///
/// This list counts **files**, which is what `ls policy/core/*.cedar` answers.
/// The policies inside them are counted separately and by a different step, and
/// the two numbers are not the same noun.
///
/// **Written, because every other number in this step is a count of the tree.**
/// `pack.len()`, the case count and the coverage count all shrink together when
/// a policy is deleted, and each still reads as `ok`. This is the one thing here
/// that a deletion cannot make agree with itself.
///
/// It cannot stop somebody editing both places. No step compares the tree with
/// what it held before, so nothing it reads remembers; what it changes is that
/// the removal is a line of the pull request's diff instead of a smaller
/// number.
const CORE_POLICIES: [&str; 7] = [
    "agent_authority.cedar",
    "chain_depth.cedar",
    "fail_closed.cedar",
    "grants_expire.cedar",
    "no_self_escalation.cedar",
    "provenance.cedar",
    "quorum.cedar",
];

/// Every rule the family pack ships, by name.
///
/// **A list and a floor for the pack, as for the core.** Held only to "not
/// empty", a policy and its only denial case can be deleted together and every
/// count moves down in lockstep — the policies that parse and validate, the
/// cases and the constraints they pin, the policies with a denial test — under
/// `check passed`, measured; and what goes is the rule that sharing
/// `restricted` content needs a quorum — irreversible key distribution, by
/// that policy's own twenty lines of justification.
///
/// Compared with the pack on disk in both directions, like `CORE_POLICIES`: a
/// name here that is not in the pack is a rule somebody removed, and one in the
/// pack that is not here is a rule nobody wrote down.
const PACK_RULES: [&str; 11] = [
    "family.a_frozen_account_acts_on_nothing",
    "family.a_guardian_administers_the_realm",
    "family.a_guardian_prunes_the_realm",
    "family.a_guardian_reads_the_realm",
    "family.a_member_names_files",
    "family.a_member_reads_shared_content",
    "family.a_member_reads_their_own_account",
    "family.a_member_steps_through_their_own_history",
    "family.an_owner_governs_their_own_content",
    "family.an_unattested_device_acts_on_nothing",
    "family.giving_out_encrypted_content_needs_quorum",
];

/// Every rule the core carries, by the id the journal records.
///
/// **A file is not a rule, and three of the seven hold more than one.** The list
/// above is per *file*, so with it alone, deleting
/// `@id("core.an_agent_never_impersonates")` from `agent_authority.cedar` —
/// which keeps `core.budget_is_a_permission` and stays a file that decides
/// something — moves three numbers by one and leaves `ok 7 core policy file(s)`
/// unchanged, with everything green, measured — and `§14.6`'s invariant that
/// an agent never impersonates its operator is gone.
///
/// So the ids are written down too, compared with the pack in both directions.
/// A file cannot appear or vanish quietly, and neither can a rule inside
/// one.
const CORE_RULES: [&str; 11] = [
    "core.an_agent_never_impersonates",
    "core.budget_is_a_permission",
    "core.chain_depth_is_bounded",
    "core.destructive_needs_quorum",
    "core.fail_closed",
    "core.grant_has_begun",
    "core.grant_not_expired",
    "core.grants_expire",
    "core.irreversible_needs_a_human",
    "core.no_self_escalation",
    "core.provenance",
];

use std::path::Path;

use crate::ui;

/// Every policy the core holds is a `forbid`, which is what its own file says.
///
/// **`policy/core/README.md` states it, and this holds it.** That file says
/// *"Every policy here is a `forbid`"*, and the paragraph beside it is the
/// design: Cedar denies by default, so an invariant that *restricts* is a
/// `forbid` and one that *permits* would widen what is allowed. This reads the
/// policies' effects, not the file's words, so it holds what that sentence
/// claims rather than the sentence: an edit turning the sentence into `permit`
/// changes nothing this step reads.
/// Two gates read that file and neither asks whether what it says is true:
/// `counts.rs` judges a numeral only where the noun is one of its seven, and
/// `references.rs`'s own header says a sentence's *truth* is not a question a
/// gate can put.
///
/// So the structural half is held here, and it is the half that matters: a
/// `permit` under the core directory is the thing that paragraph says cannot
/// exist. The packs are untouched by it — their permits live there on purpose,
/// which is why the question is asked of the directory a policy came from.
///
/// **What stays unheld is written down rather than implied.** The invariant to
/// file table is prose about meaning: that one file is what makes invariant 2
/// true is a judgement, and this gate does not make it. The marker distribution
/// is held one gate over, by the per-file counts in `EXCUSED_IN`.
fn the_core_is_all_forbids(pack: &nc_policy::Pack, failures: &mut Vec<String>) {
    // **One call, because two calls are two prefixes.** With the count from
    // `ids_under` and the verdict from `permits_under`, each given
    // `policy/core/` separately, either can be aimed elsewhere on its own and
    // the other goes on agreeing. Measured with two calls: `permits_under`
    // pointed at `policy/core/tests/` leaves this line byte-identical, over
    // nothing examined at all; `ids_under` ignoring its prefix leaves it
    // printing the whole pack's count as the core's.
    // Reading the effects of the very list that was counted is what makes the
    // two halves one question.
    let read = pack.effects_under("policy/core/");
    let permits: Vec<String> = read
        .iter()
        .filter(|(_, permits)| *permits)
        .map(|(id, _)| id.clone())
        .collect();
    // The floor: a prefix that matched nothing would agree with any file.
    let core = read.len();
    // **And a second floor, under the verdict rather than under the count.** A
    // reader that answers `forbid` to everything satisfies this line for every
    // core policy there is — measured, with the effect hardcoded, `ok 11 core
    // policy(ies) are `forbid`` printed unchanged. The packs are where a
    // `permit` lives on purpose, so a reader that can see none anywhere is a
    // reader whose verdict here means nothing.
    let permits_anywhere = pack
        .effects_under("policy/")
        .into_iter()
        .filter(|(_, permits)| *permits)
        .count();
    if permits_anywhere == 0 {
        ui::failure(
            "no policy anywhere in the pack reads as a `permit`, and the family pack \
             ships three — so whatever this step says about the core it would say about \
             any tree",
        );
        failures.push("no policy reads as a permit".to_owned());
        return;
    }
    if core == 0 {
        ui::failure(
            "no policy was read from the core directory, so what its own file says about \
             every policy there was compared against nothing",
        );
        failures.push("the core was read as empty".to_owned());
    } else if permits.is_empty() {
        ui::ok(&format!(
            "{core} core policy(ies) are `forbid`, which is what policy/core/README.md says"
        ));
    } else {
        for id in &permits {
            ui::failure(&format!(
                "`{id}` is a `permit` in the core, and that directory's own file says every \
                 policy there is a `forbid`"
            ));
        }
        ui::detail(
            "Cedar denies by default, so a core rule that permits widens what is allowed \
             rather than restricting it — move it to a pack, or make it a `forbid`",
        );
        failures.push("a core policy permits".to_owned());
    }
}

/// At least one realm was held to the version of the pack its profile selects.
///
/// `nc_policy::load` refuses a realm pinning another version, so a pack that
/// loaded has every judged pin right — which is also what a judgement over no
/// realm at all looks like. A world whose every realm selects a profile this
/// tree carries no pack for, or a reader that stopped recognising a realm,
/// holds no pin to anything and refuses nothing; the count is what tells that
/// apart from agreement.
///
/// **Both numbers the line prints are held.** The second is the packs the
/// realms were matched to, each pin against the one its pack's version builds,
/// and a unit test holds both for the shipped world — a number printed and read
/// by nothing can say `0 pack(s)` beside `1 realm(s)` with the step green.
fn the_realms_pin_the_packs_they_select(
    pins: &std::collections::BTreeMap<String, String>,
    versions: &std::collections::BTreeMap<String, u32>,
    failures: &mut Vec<String>,
) {
    let (realms, packs, unheld) = the_packs_the_pins_are_held_to(pins, versions);
    if realms == 0 {
        ui::failure(
            "no realm in policy/world.json selects a pack this tree carries, so no realm's \
             pin was held to a version at all",
        );
        ui::detail(
            "a realm pins `<profile>@<version>`, and the version is the one key of \
             policy/packs/<profile>/pack.toml. The world the cases decide against needs a \
             realm of the profile this stage implements, pinned to it",
        );
        failures.push("no realm's pin was held to a pack".to_owned());
    } else if !unheld.is_empty() {
        for (realm, pin) in &unheld {
            ui::failure(&format!(
                "`Realm::\"{realm}\"` is held to a pack and pins `{pin}`, which no pack's \
                 manifest declares"
            ));
        }
        ui::detail(
            "`nc_policy::load` returns a realm as held only when its pin is the one its \
             pack's version builds, so a pin no declared version builds is the loader and \
             this step disagreeing about what a pin is. Read `nc_policy::manifest` first",
        );
        failures.push("a realm held to a pack pins no version a pack declares".to_owned());
    } else {
        ui::ok(&format!(
            "{realms} realm(s) pin the version of the pack their profile selects, held to \
             {packs} pack manifest(s)"
        ));
    }
}

/// The realms held to a pack, the packs holding them, and each realm neither is.
///
/// Every pin is matched against the pin each declared version builds, through
/// [`nc_policy::manifest::Manifest::pin`] — the one place that spelling is made —
/// so the second number counts the packs some realm was actually held to, and
/// a pack declaring a version no realm pins is not one of them. A pin no
/// declared version builds comes back with its realm.
fn the_packs_the_pins_are_held_to<'pins>(
    pins: &'pins std::collections::BTreeMap<String, String>,
    versions: &std::collections::BTreeMap<String, u32>,
) -> (usize, usize, Vec<(&'pins str, &'pins str)>) {
    let declared: std::collections::BTreeMap<String, &str> = versions
        .iter()
        .map(|(profile, version)| {
            (
                nc_policy::manifest::Manifest::new(*version).pin(profile),
                profile.as_str(),
            )
        })
        .collect();
    let mut held = std::collections::BTreeSet::new();
    let mut unheld = Vec::new();
    for (realm, pin) in pins {
        match declared.get(pin) {
            Some(profile) => {
                held.insert(*profile);
            }
            None => unheld.push((realm.as_str(), pin.as_str())),
        }
    }
    (pins.len(), held.len(), unheld)
}

/// Every rule `§14.9` names is in the pack, and every core rule is named.
///
/// **`CORE_POLICIES` counts files, and three of the seven hold more than one rule.**
/// Without the rule list, deleting `@id("core.an_agent_never_impersonates")`
/// from a file that keeps its other rule moves three numbers by one and leaves
/// `ok 7 core policy file(s)` unchanged, with everything green — `§14.6`'s
/// invariant that an agent never impersonates its operator simply gone.
fn the_core_rules_are_the_pack(pack: &nc_policy::Pack, failures: &mut Vec<String>) {
    let held = pack.ids();
    // **Both lists, because a pack rule is a rule.** With the core rules named
    // and floored and the family pack held only to "not empty", a policy and
    // its only denial case go together with every count moving down in
    // lockstep and `check passed`.
    let written: Vec<&str> = CORE_RULES
        .iter()
        .chain(PACK_RULES.iter())
        .copied()
        .collect();
    let mut absent: Vec<&str> = written
        .iter()
        .filter(|rule| !held.iter().any(|id| id == *rule))
        .copied()
        .collect();
    // **Every id the pack holds, not two prefixes.** A filter on `core.` and
    // `family.` leaves a rule arriving under any other prefix neither `absent`
    // nor `unnamed` — and `policy/packs/org` and `policy/packs/collective` are
    // loaded. Measured: a probe rule dropped under `policy/packs/org` with an
    // `org.`-prefixed id gives an `ok … policy(ies) parse and validate` line
    // one higher from this step under that filter, and only the denial-test
    // gate objects — which is satisfied by writing one case. A pin that reads
    // two of the prefixes a directory can hold is a pin over a set that does
    // not enumerate the set.
    let mut unnamed: Vec<&str> = held
        .iter()
        .filter(|id| !written.contains(&id.as_str()))
        .map(String::as_str)
        .collect();
    absent.sort_unstable();
    unnamed.sort_unstable();
    if absent.is_empty() && unnamed.is_empty() {
        ui::ok(&format!(
            "{} policy(ies) parse and validate, {} of them the core and pack rules \
             by name",
            pack.len(),
            written.len()
        ));
    } else {
        for rule in &absent {
            ui::failure(&format!(
                "`{rule}` is written down in `xtask/src/policy.rs` and the pack does \
                 not hold it"
            ));
        }
        for rule in &unnamed {
            ui::failure(&format!(
                "`{rule}` is a rule the pack holds and nothing in `xtask/src/policy.rs` \
                 names"
            ));
        }
        // **The remedy names the list the rule belongs in.** `written` is
        // `CORE_RULES` chained with `PACK_RULES`, so these arms fire for a pack
        // rule too — and one fixed sentence would tell a reader to put an `org.`
        // id into `CORE_RULES`, which then fails that list's own length
        // assertion. `§15.1` asks a diagnostic to end with an actionable fix,
        // and a fix that cannot be followed is worse than none.
        let core = absent
            .iter()
            .chain(unnamed.iter())
            .all(|rule| rule.starts_with("core."));
        if core {
            ui::detail(
                "a file is not a rule, and three of the seven core files hold more than one: \
                 deleting one rule from a file that keeps another moved three counts by one \
                 and left the file count unchanged. Add the id to `CORE_RULES` in \
                 xtask/src/policy.rs, or take the rule out of policy/core/",
            );
        } else {
            ui::detail(
                "a rule under policy/packs/ belongs to `PACK_RULES`, not `CORE_RULES` — the \
                 two lists are compared against the pack together and each is held to its \
                 own length. Add the id to `PACK_RULES` in xtask/src/policy.rs, or take the \
                 rule out of policy/packs/",
            );
        }
        failures.push(format!(
            "{} core rule(s) the pack and the list disagree about",
            absent.len() + unnamed.len()
        ));
    }
}

/// Every command whose action the schema does not declare, said in full.
///
/// **Named, because a test can reach it only so.** In a clean tree the answer
/// is empty either way, so the refusal branch could be deleted with nothing a
/// run prints moving — `if !declared.iter().any(…)` replaced by `if false` is
/// caught by the test that hands this a schema without the action. What that
/// branch catches is the one this gate's own doc names: `action = "Reed"`,
/// which without it *"compiles, generates, documents itself in
/// `docs/commands.md` and passes the whole gate"*.
///
/// The floor beside it is caught, because the verdict census pins the condition's
/// text next to the sentence it guards; this branch pushes into a list rather
/// than calling `ui::`, so no census sees it. `policy.rs` already carries
/// `a_policy_whose_denial_test_is_gone_is_a_gap` for exactly this shape one
/// function along.
/// The sentence stays at the call site, because that is where the verdict
/// census pins it: what is named here is the *comparison*, which is the half no
/// test could reach.
fn the_commands_the_schema_does_not_declare(
    declared: &[String],
) -> Vec<&'static nc_catalog::commands::Command> {
    nc_catalog::commands::COMMANDS
        .iter()
        .filter(|command| !declared.iter().any(|action| action == command.action))
        .collect()
}

/// The entity type a command's `resource_selector` names, read through its
/// argument schema.
///
/// **The selector is read by the rule that judged it**, `nc_schema`'s, which
/// the catalog compiler holds every command to and the kernel holds every
/// plugin's to: `realm` is the `Realm` entity, `feature:<id>` the `Feature` it
/// names, and `argument:<name>` the type the property's `x-entity` names —
/// declared, required and bounded, or the compiler refuses the catalog. So every selector a compiled catalog carries
/// names a type this can compare, the form that extracts from the arguments
/// included.
///
/// **`x-entity` is what lets it answer for that form.** Without it the type an
/// argument names is known only at run time, and a step that refuses such a
/// command — measured with `journal.verify` selecting `argument:from` — tells
/// the author to make the selector an entity type, the one thing the field is
/// documented not to be. The schema says the type, and a command acting as
/// `Read` on a `Group` is named here at build time rather than denied by the
/// decision point at run time.
///
/// `None` is left for a selector its own schema does not back, which a
/// compiled catalog cannot hold; the step still counts one rather than read it.
fn the_type_a_selector_names(selector: &str, args_schema: &str) -> Option<String> {
    let schema = nc_schema::Schema::compile(args_schema).ok()?;
    nc_schema::Selector::parse(selector, &schema)
        .ok()
        .map(|selected| selected.entity_type().to_owned())
}

/// Every command whose resource is one its own action cannot be applied to.
///
/// **The resource as well as the verb.** Every shipped command selects the
/// realm or a feature of the platform, and with no action applying to a
/// `Realm` in the schema the request each of the first builds is one Cedar's
/// own schema refuses, while a comparison of verbs alone prints
/// `ok … command(s) name an action the schema declares`.
/// Measured by appending a case with `resource = 'Realm::"home"'`:
///
/// ```text
/// what: "`…` is not a request the schema allows: resource type `Realm` is not
///        valid for `Action::\"Read\"`",
/// check: "malformed-request",
/// ```
///
/// The schema and `reference/identity-and-authorization.md §4` both carry
/// `Realm` — §4 is the file that outranks, so it moves first — and
/// this is the comparison that keeps the pair from drifting.
fn the_commands_the_schema_cannot_apply(pack: &nc_policy::Pack) -> (Vec<String>, usize, usize) {
    let commands = the_catalog_as_commands();
    let (wrong, unread) =
        the_commands_a_schema_cannot_apply(&commands, &|action| pack.resources_for(action));
    (wrong, unread, commands.len())
}

/// The catalog's commands, as the comparison beside this one needs them.
///
/// **Its own function because a length is not the mapping.** As three lines
/// inside the caller, entered by no test, with a guard counting how many
/// commands arrive — which `.map` preserves — the selector replaced by a
/// constant *different* from the shipped one, `"user"` for `"realm"`, leaves
/// `cargo xtask check` byte-identical to a clean run, measured, because the
/// count is right and nothing reads the values. The test beside it compares
/// this against the catalog field by field.
fn the_catalog_as_commands() -> Vec<Command<'static>> {
    nc_catalog::commands::COMMANDS
        .iter()
        .map(|command| {
            (
                command.name,
                command.action,
                command.resource_selector,
                command.args_schema,
            )
        })
        .collect()
}

/// A command as this comparison needs it: its name, its verb, its selector
/// and the schema the selector is read through.
///
/// **`nc_catalog::Command` is sealed and a test cannot build one**, which is
/// right — its fields are what a dispatcher decides on — and a comparison over
/// it is drivable only by the catalog that ships. So the comparison takes
/// the four fields it reads, the shipped caller maps them in, and a case can
/// hand it a command the catalog does not contain.
type Command<'a> = (&'a str, &'a str, &'a str, &'a str);

/// The same comparison, asked of a function rather than of a parsed schema.
///
/// **Split out so a test can drive it.** The three functions this comparison
/// is built from appear nowhere but at one call site each, so
/// `if !allowed.contains(&named)` narrowed to
/// `if !allowed.contains(&named) && named.is_empty()` — so the branch can never
/// fire — leaves the whole policy section byte-identical to a clean run, so
/// a test of its own is what fails.
///
/// And within the step it is the only guard against the drift it exists for:
/// with `Realm` taken out of the resource list in *both*
/// `policy/schema.cedarschema` and `reference/identity-and-authorization.md §4`
/// — the coherent edit, since §4 outranks and moves first — every other line of
/// the step stays `ok`, the §4 comparison agrees because both files moved
/// together, and only this branch objects. With the narrowing in place the
/// step objects to nothing, while every shipped command selecting the realm
/// builds a request Cedar's own schema refuses; what fails then is
/// `a_command_whose_resource_its_action_cannot_reach_is_named`.
fn the_commands_a_schema_cannot_apply(
    commands: &[Command],
    allowed_for: &dyn Fn(&str) -> Vec<String>,
) -> (Vec<String>, usize) {
    let mut wrong = Vec::new();
    // How many resources this comparison could not read statically, which the
    // step prints. A number that climbs is the comparison quietly covering less
    // of the catalog, and that is the only way this passing-over can rot.
    let mut unread = 0_usize;
    for (name, action, selector, args_schema) in commands {
        let allowed = allowed_for(action);
        if allowed.is_empty() {
            // The action itself is not declared, which the comparison beside
            // this one reports; saying it twice would name one defect as two.
            continue;
        }
        let Some(named) = the_type_a_selector_names(selector, args_schema) else {
            // A selector its own schema does not back, which the compiler
            // refuses; not read as a type, and not silence either: it is
            // counted and said.
            unread += 1;
            continue;
        };
        if !allowed.contains(&named) {
            wrong.push(format!(
                "`{name}` acts as `{action}` on a `{named}`, and the schema lets that \
                 action apply to {allowed:?}"
            ));
        }
    }
    (wrong, unread)
}

/// Every command's `action` is one the Cedar schema declares.
///
/// **The two sides, compared.** `catalog/commands.toml` carries an
/// `action` per command whose own comment calls it "the Cedar action name the
/// PDP evaluates", and `policy/schema.cedarschema` declares six —
/// `Read, Write, Delete, Share, Grant, Revoke`. With nothing reading one against
/// the other, `action = "Reed"` compiles, generates, documents itself in
/// `docs/commands.md` and passes the whole gate; what it costs is a request the
/// PDP cannot build, discovered when a person tries the command rather than
/// when the catalog is written. The registry's other referents — a message key,
/// an error code, a table's ordering column — are each checked against the
/// thing they name, and this is the one that names something outside the
/// catalog, so the catalog's own checks do not reach it.
///
/// The names come from the parsed schema through [`nc_policy::Pack::actions`]
/// rather than from the schema's text, so what a command is compared against is
/// the same set the validator holds.
///
/// The comparison itself is [`the_commands_the_schema_does_not_declare`], which
/// is where a test can reach it: this function needs a parsed `Pack`, and the
/// catalog's commands are a static.
fn every_command_names_an_action_the_schema_has(
    pack: &nc_policy::Pack,
    failures: &mut Vec<String>,
) {
    let declared = pack.actions();
    let mut wrong: Vec<String> = Vec::new();
    for command in the_commands_the_schema_does_not_declare(&declared) {
        wrong.push(format!(
            "`{}` acts as `{}`, and the schema declares {declared:?}",
            command.name, command.action
        ));
    }
    let undeclared = wrong.len();
    let (unreachable_resources, unread, compared) = the_commands_the_schema_cannot_apply(pack);
    wrong.extend(unreachable_resources);
    let unreachable = wrong.len() - undeclared;
    // **The number the walk produced, not the number the catalog holds.**
    // Printing `COMMANDS.len()` here, while the walk beside it is handed a
    // mapping built at one call site, would let `.iter().take(1)` in that
    // mapping leave the `ok … command(s) name an action the schema declares`
    // line unchanged over **one** command compared, byte-identical to a clean
    // run. A count that is not the walk's is a count that cannot say the walk
    // was short.
    if compared != nc_catalog::commands::COMMANDS.len() {
        ui::failure(&format!(
            "{compared} of the catalog's {} command(s) reached this comparison: the \
             mapping that hands them over is narrowing, and a command it drops is one \
             whose resource nothing here compares",
            nc_catalog::commands::COMMANDS.len()
        ));
        failures.push(
            "the command comparison was handed fewer commands than the catalog holds".to_owned(),
        );
        return;
    }
    // The floor is on both sides: a schema with no actions would agree with an
    // empty catalog, and an empty catalog would agree with any schema.
    if declared.is_empty() || nc_catalog::commands::COMMANDS.is_empty() {
        ui::failure(&format!(
            "{} action(s) in the schema and {} command(s) in the catalog: one side is \
             empty, so this comparison decided nothing",
            declared.len(),
            nc_catalog::commands::COMMANDS.len()
        ));
        failures.push("the command-action comparison is empty".to_owned());
        return;
    }
    if wrong.is_empty() {
        ui::ok(&format!(
            "{compared} command(s) name an action the schema declares, of {} declared",
            declared.len()
        ));
        // **And how many resources this could not compare.** Every selector a
        // compiled catalog holds names its type — `realm`, a feature, or an
        // argument whose `x-entity` says it — so one this cannot read is a selector its own
        // schema does not back, which the compiler refuses. Passing over one
        // silently would make this step quietly cover less of the catalog;
        // saying the number is what makes that visible.
        if unread > 0 {
            ui::detail(&format!(
                "{unread} of them select their resource through a selector their own \
                 schema does not back, so the type each names could not be read here — \
                 `cargo xtask build` refuses such a catalog"
            ));
        }
        return;
    }
    for problem in &wrong {
        ui::failure(problem);
    }
    // **A reason per defect, because two live in this `wrong`.** One summary,
    // `a command names an action the schema does not declare`, for every member
    // of it is false for a *resource* mismatch: the action
    // is declared, and a reader following it goes to the schema's action list,
    // which is correct. Measured with `Realm` taken out of the schema and §4
    // together and one summary: two `error` lines about a `Realm` resource, then
    // that summary verbatim. One function along has the same shape, where
    // calling both kinds `policy(ies) without a denial test` would send a reader
    // looking for a policy that is in fact covered.
    // Written as arms rather than as a `match` handed to `push`, because
    // `every_verdict_is_pinned_to_where_it_is_said` reads the line above the
    // call: a condition written there is pinned, and an expression passed in is
    // a site that rule records as unreadable.
    if undeclared == 0 {
        failures.push("a command's resource is one its action cannot be applied to".to_owned());
    } else if unreachable == 0 {
        failures.push("a command names an action the schema does not declare".to_owned());
    } else {
        failures.push(
            "a command names an action the schema does not declare, and another's resource \
             is one its action cannot be applied to"
                .to_owned(),
        );
    }
}

/// One problem, located, and then what to do about it.
///
/// **`Problem` carries a `fix`, and this prints it.** The type is the two-field
/// shape every gate here uses — `what` is the defect, `fix` is the remedy, kept
/// apart so that "there is a remedy" is structural rather than a word count — and
/// `Display` renders only `file:line — what`, because `:0` needs a special case
/// and the fix does not fit on one line. Printed through `Display` alone, every
/// diagnostic this step produces ends at the defect: a newcomer who writes
/// `action in [...]` in a core policy is told the policy "turns itself off for
/// `action in` in its scope" and nothing else, while the sentence naming
/// `nc:narrows:`, the marker that excuses it, lives in `nc-policy` and is the
/// only mention of that marker anywhere they can reach.
///
/// `NOCTUA.md §15.1`: a diagnostic is located and ends with the fix.
fn report(problem: &nc_policy::Problem) {
    ui::failure(&format!("{problem}"));
    ui::detail(&problem.fix);
}

/// Which of `CORE_POLICIES` is absent, present and deciding nothing, or present
/// and unnamed.
///
/// **Both directions, and both of them here.** With the reverse — a Cedar file
/// in `policy/core/` that the list does not name — checked only by
/// `the_core_list_is_the_directory`, a unit test far below the step, a
/// newcomer who adds an eighth core policy is told
/// `ok 7 core policy file(s) are on disk` by the step, with the tree holding
/// eight, and the disagreement surfaces later in the run as a bare `assert_eq!`
/// panic naming no file to edit. The step that reads the directory is the one
/// that should say the directory and the list disagree.
fn core_policies_that_decide_nothing(root: &Path) -> Vec<String> {
    let mut missing = Vec::new();
    for name in CORE_POLICIES {
        let path = root.join("policy/core").join(name);
        match std::fs::read_to_string(&path) {
            Err(_) => missing.push(format!("{name} is not there")),
            Ok(text) => {
                if !decides_something(&text) {
                    missing.push(format!(
                        "{name} holds no `permit` or `forbid`, so it decides nothing"
                    ));
                }
            }
        }
    }

    let mut unnamed: Vec<String> = std::fs::read_dir(root.join("policy/core"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| {
            std::path::Path::new(name)
                .extension()
                .is_some_and(|kind| kind.eq_ignore_ascii_case("cedar"))
                && !CORE_POLICIES.contains(&name.as_str())
        })
        .collect();
    unnamed.sort();
    for name in unnamed {
        missing.push(format!(
            "{name} is there and `CORE_POLICIES` in xtask/src/policy.rs does not name it, \
             so this step walked past it"
        ));
    }
    missing
}

/// Whether the pack this stage implements holds a policy that decides something.
///
/// **`CORE_POLICIES` is compared with `policy/core/` in both directions**, so a
/// core invariant cannot be deleted quietly. `policy/packs/` has no list of
/// files, and its counts move together: deleting `family`'s policy file *and*
/// its case file shrinks the policy count, the case count and the denial-test
/// count in step, so no count line can object — the same shape one directory
/// along. [`PACK_RULES`] names the family pack's rules, so a rule that goes is
/// named there.
///
/// What a pack can be asked beside that list is that an implemented pack is
/// not empty: `family` is the one Stage 10 implements, `org` and `collective`
/// are deliberately a README each, and a pack that is implemented has at least
/// one policy that decides something.
fn pack_decides_nothing(root: &Path) -> bool {
    let family = root.join("policy/packs/family");
    let mut found = 0_usize;
    let walked = crate::gates::walk(&family, &mut |path| {
        if path.extension().is_some_and(|kind| kind == "cedar")
            && std::fs::read_to_string(path).is_ok_and(|text| decides_something(&text))
        {
            found += 1;
        }
    });
    walked.is_err() || found == 0
}

/// Whether a Cedar file holds a policy that decides something.
///
/// **Not layout, because Cedar's grammar is not line-oriented.** Asking whether
/// a *line*, once its comment is stripped, begins with `permit` or `forbid`
/// reports a legitimate policy whose effect keyword follows its `@id(…)` on one
/// line as deciding nothing. Measured, `policy/core/provenance.cedar` written
/// on a single line:
/// `error … holds no `permit` or `forbid`, so it decides nothing`, with a
/// remedy telling the author to add the file to `CORE_POLICIES` — where it
/// already is — and the same run printing `ok 15 policy(ies) parse and
/// validate` two lines below, having counted that very file. A refusal whose
/// fix cannot be followed is worse than no refusal.
///
/// So the question is asked of the code rather than of the layout: comments go
/// first, and the effect keyword is then looked for as a *word* anywhere in
/// what is left. Cedar's own `Policy::to_string` keeps `forbid (` at the start
/// of a line, so nothing in this tree emits the tripping form today — which is
/// what made this a false refusal waiting for a legitimate spelling rather than
/// an admission of a bad one.
///
/// Written once, because two copies of one rule is how the narrower copy
/// survives: `pack_decides_nothing` asks the same question of the pack.
fn decides_something(text: &str) -> bool {
    // **Every comment, not the one spelling, and not inside a string.**
    // `split("//")` reads a `//` inside a quoted annotation as the start of a
    // comment and throws the rest of the line away, and it cannot see a
    // `/* … */` at all. `without_comments` is the whole-file state machine the
    // rest of the gates use.
    let code = crate::gates::without_comments(text, "//");
    ["permit", "forbid"].into_iter().any(|effect| {
        code.match_indices(effect).any(|(at, _)| {
            let word = |character: char| character.is_alphanumeric() || character == '_';
            !code[..at].chars().next_back().is_some_and(word)
                && !code[at + effect.len()..].chars().next().is_some_and(word)
        })
    })
}

/// Every case this repository's suite runs, held against the number written
/// down.
///
/// **The one artefact of this step no other written number holds.**
/// `CORE_POLICIES`, `CORE_RULES`, `PACK_RULES`, `PINNED` and `PINNED_BY_POLICY`
/// are each written down and compared; a case count printed and compared with
/// nothing lets the last `[[case]]` of the family pack's `permissions.toml` be
/// deleted — the one whose own comment calls it *"a deferral, pinned rather than
/// left implicit … the day a core `forbid` arrives, this case flips and somebody
/// decides on purpose"* — with the step printing a case count one lower and
/// passing, measured.
///
/// That is the shape this file argues against for policies and rules: an
/// expectation that shrinks together with the thing it measures. What
/// a deletion can actually take is narrow — a case that pins a clause is caught
/// by `PINNED_BY_POLICY`, and a policy's only denial case by the coverage check
/// — which leaves exactly the cases that record a deliberate decision, and
/// `policy/core/README.md` names one of those as what stands in for the realm
/// floor this stage does not have.
///
/// Its own function beside its sibling floor, because `check` sits at clippy's
/// hundred-line bound.
fn the_cases_are_the_written_number(
    ran: usize,
    cases: &[nc_policy::Case],
    failures: &mut Vec<String>,
) {
    // **And per file, because one total conserves under the deletion it exists
    // to catch.** `CASES` is summed over every case file, so held to that
    // total alone, the case it exists for — the deferral recording the
    // cross-realm gap `reference/identity-and-authorization.md §3` leaves open
    // — can be deleted and any other case written in its place with every
    // number in the transcript byte-identical. Measured against the total
    // alone: the deferral removed and a duplicate of its neighbour appended
    // under another name gives the same case and constraint counts and
    // `check passed`. It pins no conjunct and is nobody's denial test, so
    // `PINNED_BY_POLICY` and the coverage check cannot see it either.
    //
    // [`PINNED_BY_POLICY`] does the same one constant along, for the reason
    // its own doc gives: a count is not a set.
    let mut here: Vec<(&str, &str, &str, &str, &str, String, String)> = cases
        .iter()
        .map(|case| {
            (
                case.file.as_str(),
                case.name.as_str(),
                case.principal.as_str(),
                case.action.as_str(),
                case.resource.as_str(),
                case.context.to_string(),
                what_the_case_asserts(case),
            )
        })
        .collect();
    here.sort_unstable();
    let written: Vec<(&str, &str, &str, &str, &str, String, String)> = THE_CASES
        .iter()
        .map(
            |(file, name, principal, action, resource, context, asserts)| {
                (
                    *file,
                    *name,
                    *principal,
                    *action,
                    *resource,
                    (*context).to_owned(),
                    (*asserts).to_owned(),
                )
            },
        )
        .collect();
    if here != written {
        // Two lists rather than one diff of eighty-two lines: what went and what
        // arrived is the whole of what a reader needs, and a case swapped for
        // another shows up as one of each.
        let gone: Vec<&(&str, &str, &str, &str, &str, String, String)> =
            written.iter().filter(|row| !here.contains(row)).collect();
        let arrived: Vec<&(&str, &str, &str, &str, &str, String, String)> =
            here.iter().filter(|row| !written.contains(row)).collect();
        ui::failure(&format!(
            "the cases this pack runs are not the ones written down. Gone: {gone:?}. \
             Arrived: {arrived:?}. A case deleted is a decision nobody has to make again — \
             read what went before writing the new list, because a case added elsewhere \
             pays for one taken out here and every number this step prints stays the same"
        ));
        failures.push("the policy cases are not the ones written down".to_owned());
    }

    // **The condition above the call, not an early return.**
    // `every_verdict_is_pinned_to_where_it_is_said` resolves the arm a sentence
    // sits in by reading the line above it, so a guard that returns leaves both
    // of these at top level and the census records `—`. Measured with such a
    // guard, `ran == CASES` widened to `ran == CASES || true`: the whole xtask
    // suite green — the number this function's own doc calls *the one artefact
    // of this step no other written number holds* switched off in one token.
    // Written this way the census pins the condition, which is what kills it.
    if ran != CASES {
        ui::failure(&format!(
            "{ran} case(s) ran and this repository's suite has {CASES}: a case deleted is a \
         decision nobody has to make again. Open the file and read what went, because the \
         number beside it is satisfied by any case at all — `THE_CASES` in \
         xtask/src/policy.rs is the list, and the failure above names which row went"
        ));
        failures.push(format!(
            "{ran} policy case(s) ran, and {CASES} are written down"
        ));
    }
}

/// What one case asserts, as one comparable sentence.
///
/// **What a case *answers*, as well as what it *asks*.** The fields that say
/// what a case asks are there so a case cannot be moved off the boundary it
/// pins — and without `decision` the case can be turned around on the spot
/// instead. Measured without it: `policy/core/grants_expire.cedar`'s
/// `context.grant.valid_until <= context.now` narrowed to `<`, and the case
/// written for that exact instant — *"a grant expiring at this very instant has
/// expired"* — flipped from `deny` to `allow` with its `because` repointed at
/// the family rule that then permits it: every other field of this census
/// byte-identical, the step printing the same case and constraint counts, and
/// the tree honouring a permission that has just run out. `PINNED_BY_POLICY`
/// cannot see it either: the clause is not removed, its boundary is moved, and
/// a count is a count.
///
/// All five of the fields that make up an answer, because each is a different
/// way to say the same thing less: `decision` is the verdict, `because` names
/// the policy that must be among the reasons, `no_permit_matched` names the
/// permit the request must fall outside, and the two boundary flags say which
/// of the request's parts put it there — the verb, or who is asking.
fn what_the_case_asserts(case: &nc_policy::Case) -> String {
    // Written as fields rather than as prose, because every sentence this tool
    // ships has to be in one of two frozen lists — and a row of a census is not
    // a sentence a reader is shown, it is a value a diff prints.
    use core::fmt::Write as _;
    let mut said = format!("{:?}", case.decision);
    if let Some(policy) = &case.because {
        let _ = write!(said, "/because={policy}");
    }
    if let Some(permit) = &case.no_permit_matched {
        let _ = write!(said, "/outside={permit}");
    }
    if case.the_action_is_the_boundary {
        said.push_str("/boundary=action");
    }
    if case.the_principal_is_the_boundary {
        said.push_str("/boundary=principal");
    }
    said
}

/// How many cases the pack's own suite runs — see
/// [`the_cases_are_the_written_number`], which is where the argument for it is.
const CASES: usize = 81;

/// Every entity `policy/world.json` declares, and every attribute it carries.
///
/// **The cases pin the entity they name and never what that entity is.** The
/// case census holds seven fields so a case cannot be moved off the boundary it
/// pins — and a row records `Grant::"g.expiring_now"`, not that grant's
/// `valid_until`. Measured without this: one day taken off that attribute, with
/// `core.grant_not_expired`'s `<=` narrowed to `<` in the same edit, leaves
/// the step's case and constraint counts, every row of [`THE_CASES`]
/// byte-identical, and `check passed` — while a permission that has just run
/// out is honoured. The control is what makes it a finding: the same narrowing
/// with `policy/world.json` pristine turns the policy step and four `nc-policy`
/// tests red, so without this list one line of that file silences the case,
/// the census, the denial gate and the fail-open gate together.
///
/// Rows are `(entity, attribute, value)`, with `<parent>` for a membership
/// edge, because a realm a principal belongs to is as much a fact of the world
/// as an attribute is. An entity carrying nothing gets one row with two empty
/// fields, so that deleting it is a row that went rather than nothing at all.
#[cfg(test)]
const THE_WORLD: [(&str, &str, &str); 132] = [
    (
        "Agent::\"ana-assistant\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    ("Agent::\"ana-assistant\"", "autonomy", "\"supervised\""),
    ("Agent::\"ana-assistant\"", "budget_remaining", "1000"),
    (
        "Agent::\"ana-assistant\"",
        "operator",
        "{\"__entity\":{\"type\":\"User\",\"id\":\"ana\"}}",
    ),
    (
        "Agent::\"ana-assistant-spent\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    (
        "Agent::\"ana-assistant-spent\"",
        "autonomy",
        "\"supervised\"",
    ),
    ("Agent::\"ana-assistant-spent\"", "budget_remaining", "0"),
    (
        "Agent::\"ana-assistant-spent\"",
        "operator",
        "{\"__entity\":{\"type\":\"User\",\"id\":\"ana\"}}",
    ),
    (
        "Content::\"diary\"",
        "<parent>",
        "{\"type\":\"Folder\",\"id\":\"household\"}",
    ),
    (
        "Content::\"diary\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    ("Content::\"diary\"", "encrypted", "true"),
    (
        "Content::\"diary\"",
        "owner",
        "{\"__entity\":{\"type\":\"User\",\"id\":\"ana\"}}",
    ),
    ("Content::\"diary\"", "sensitivity", "\"private\""),
    (
        "Content::\"notice\"",
        "<parent>",
        "{\"type\":\"Folder\",\"id\":\"household\"}",
    ),
    (
        "Content::\"notice\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    ("Content::\"notice\"", "encrypted", "false"),
    (
        "Content::\"notice\"",
        "owner",
        "{\"__entity\":{\"type\":\"User\",\"id\":\"ana\"}}",
    ),
    ("Content::\"notice\"", "sensitivity", "\"public\""),
    (
        "Content::\"photo\"",
        "<parent>",
        "{\"type\":\"Feature\",\"id\":\"gallery\"}",
    ),
    (
        "Content::\"photo\"",
        "<parent>",
        "{\"type\":\"Folder\",\"id\":\"household\"}",
    ),
    (
        "Content::\"photo\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    ("Content::\"photo\"", "encrypted", "false"),
    (
        "Content::\"photo\"",
        "owner",
        "{\"__entity\":{\"type\":\"User\",\"id\":\"ana\"}}",
    ),
    ("Content::\"photo\"", "sensitivity", "\"realm\""),
    (
        "Content::\"postcard\"",
        "<parent>",
        "{\"type\":\"Folder\",\"id\":\"household\"}",
    ),
    (
        "Content::\"postcard\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    ("Content::\"postcard\"", "encrypted", "false"),
    (
        "Content::\"postcard\"",
        "owner",
        "{\"__entity\":{\"type\":\"User\",\"id\":\"dora\"}}",
    ),
    ("Content::\"postcard\"", "sensitivity", "\"realm\""),
    (
        "Content::\"recipe\"",
        "<parent>",
        "{\"type\":\"Folder\",\"id\":\"household\"}",
    ),
    (
        "Content::\"recipe\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    ("Content::\"recipe\"", "encrypted", "true"),
    (
        "Content::\"recipe\"",
        "owner",
        "{\"__entity\":{\"type\":\"User\",\"id\":\"carla\"}}",
    ),
    ("Content::\"recipe\"", "sensitivity", "\"private\""),
    (
        "Content::\"vault\"",
        "<parent>",
        "{\"type\":\"Folder\",\"id\":\"household\"}",
    ),
    (
        "Content::\"vault\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    ("Content::\"vault\"", "encrypted", "true"),
    (
        "Content::\"vault\"",
        "owner",
        "{\"__entity\":{\"type\":\"User\",\"id\":\"ana\"}}",
    ),
    ("Content::\"vault\"", "sensitivity", "\"restricted\""),
    (
        "Device::\"ana-laptop\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    ("Device::\"ana-laptop\"", "attested", "true"),
    ("Device::\"ana-laptop\"", "node_key", "\"b62a0e7f4c1d9a35\""),
    (
        "Device::\"ana-laptop\"",
        "owner",
        "{\"__entity\":{\"type\":\"User\",\"id\":\"ana\"}}",
    ),
    (
        "Device::\"old-tablet\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    ("Device::\"old-tablet\"", "attested", "false"),
    ("Device::\"old-tablet\"", "node_key", "\"4e91c07d2b6a8f13\""),
    (
        "Device::\"old-tablet\"",
        "owner",
        "{\"__entity\":{\"type\":\"User\",\"id\":\"bruno\"}}",
    ),
    (
        "Feature::\"gallery\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    ("Feature::\"gallery\"", "tier", "\"included\""),
    (
        "Feature::\"naming\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    ("Feature::\"naming\"", "tier", "\"included\""),
    ("Folder::\"household\"", "", ""),
    (
        "Folder::\"household\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    (
        "Grant::\"g.beginning_now\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    (
        "Grant::\"g.beginning_now\"",
        "granted_by",
        "{\"__entity\":{\"type\":\"User\",\"id\":\"ana\"}}",
    ),
    (
        "Grant::\"g.beginning_now\"",
        "reason",
        "\"opens at exactly the instant the cases call now, which is the boundary `<` decides and `<=` does not\"",
    ),
    ("Grant::\"g.beginning_now\"", "source", "\"manual\""),
    (
        "Grant::\"g.beginning_now\"",
        "valid_from",
        "{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26\"}}",
    ),
    (
        "Grant::\"g.beginning_now\"",
        "valid_until",
        "{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2027-06-01\"}}",
    ),
    (
        "Grant::\"g.by_policy\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    (
        "Grant::\"g.by_policy\"",
        "granted_by",
        "{\"__entity\":{\"type\":\"User\",\"id\":\"ana\"}}",
    ),
    (
        "Grant::\"g.by_policy\"",
        "reason",
        "\"membership of the realm, which the pack confers\"",
    ),
    ("Grant::\"g.by_policy\"", "source", "\"policy\""),
    (
        "Grant::\"g.by_policy\"",
        "valid_from",
        "{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-01-01\"}}",
    ),
    (
        "Grant::\"g.current\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    (
        "Grant::\"g.current\"",
        "granted_by",
        "{\"__entity\":{\"type\":\"User\",\"id\":\"ana\"}}",
    ),
    (
        "Grant::\"g.current\"",
        "reason",
        "\"a household member, added when the realm was created\"",
    ),
    ("Grant::\"g.current\"", "source", "\"manual\""),
    (
        "Grant::\"g.current\"",
        "valid_from",
        "{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-01-01\"}}",
    ),
    (
        "Grant::\"g.current\"",
        "valid_until",
        "{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2027-01-01\"}}",
    ),
    (
        "Grant::\"g.expired\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    (
        "Grant::\"g.expired\"",
        "granted_by",
        "{\"__entity\":{\"type\":\"User\",\"id\":\"ana\"}}",
    ),
    (
        "Grant::\"g.expired\"",
        "reason",
        "\"a visiting relative, for the length of the visit\"",
    ),
    ("Grant::\"g.expired\"", "source", "\"manual\""),
    (
        "Grant::\"g.expired\"",
        "valid_from",
        "{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2025-12-01\"}}",
    ),
    (
        "Grant::\"g.expired\"",
        "valid_until",
        "{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-01-15\"}}",
    ),
    (
        "Grant::\"g.expiring_now\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    (
        "Grant::\"g.expiring_now\"",
        "granted_by",
        "{\"__entity\":{\"type\":\"User\",\"id\":\"ana\"}}",
    ),
    (
        "Grant::\"g.expiring_now\"",
        "reason",
        "\"expires at exactly the instant the cases call now, which is the boundary `<=` decides and `<` does not\"",
    ),
    ("Grant::\"g.expiring_now\"", "source", "\"manual\""),
    (
        "Grant::\"g.expiring_now\"",
        "valid_from",
        "{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-01-01\"}}",
    ),
    (
        "Grant::\"g.expiring_now\"",
        "valid_until",
        "{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26\"}}",
    ),
    (
        "Grant::\"g.not_yet\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    (
        "Grant::\"g.not_yet\"",
        "granted_by",
        "{\"__entity\":{\"type\":\"User\",\"id\":\"ana\"}}",
    ),
    (
        "Grant::\"g.not_yet\"",
        "reason",
        "\"a guest pass for a visit next June, written today\"",
    ),
    ("Grant::\"g.not_yet\"", "source", "\"manual\""),
    (
        "Grant::\"g.not_yet\"",
        "valid_from",
        "{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2027-06-01\"}}",
    ),
    (
        "Grant::\"g.not_yet\"",
        "valid_until",
        "{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2027-06-30\"}}",
    ),
    (
        "Grant::\"g.not_yet_by_policy\"",
        "granted_by",
        "{\"__entity\":{\"type\":\"User\",\"id\":\"ana\"}}",
    ),
    (
        "Grant::\"g.not_yet_by_policy\"",
        "reason",
        "\"membership the pack will confer from next June, written today\"",
    ),
    ("Grant::\"g.not_yet_by_policy\"", "source", "\"policy\""),
    (
        "Grant::\"g.not_yet_by_policy\"",
        "valid_from",
        "{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2027-06-01\"}}",
    ),
    (
        "Grant::\"g.perpetual\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    (
        "Grant::\"g.perpetual\"",
        "granted_by",
        "{\"__entity\":{\"type\":\"User\",\"id\":\"ana\"}}",
    ),
    (
        "Grant::\"g.perpetual\"",
        "reason",
        "\"written by hand with no end, which is the shape §9.3 refuses\"",
    ),
    ("Grant::\"g.perpetual\"", "source", "\"manual\""),
    (
        "Grant::\"g.perpetual\"",
        "valid_from",
        "{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-01-01\"}}",
    ),
    ("Group::\"role.guardian\"", "", ""),
    (
        "Group::\"role.guardian\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    ("Realm::\"home\"", "policy_pack", "\"family@1\""),
    ("Realm::\"home\"", "profile", "\"family\""),
    ("Realm::\"office\"", "policy_pack", "\"org@0.1.0\""),
    ("Realm::\"office\"", "profile", "\"org\""),
    (
        "User::\"ana\"",
        "<parent>",
        "{\"type\":\"Group\",\"id\":\"role.guardian\"}",
    ),
    (
        "User::\"ana\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    (
        "User::\"ana\"",
        "birth_date",
        "{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"1988-04-12\"}}",
    ),
    ("User::\"ana\"", "status", "\"active\""),
    (
        "User::\"bruno\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    (
        "User::\"bruno\"",
        "birth_date",
        "{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2016-09-30\"}}",
    ),
    ("User::\"bruno\"", "status", "\"active\""),
    (
        "User::\"carla\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    (
        "User::\"carla\"",
        "birth_date",
        "{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"1991-01-05\"}}",
    ),
    ("User::\"carla\"", "status", "\"active\""),
    (
        "User::\"dora\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    (
        "User::\"dora\"",
        "birth_date",
        "{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"1952-03-14\"}}",
    ),
    ("User::\"dora\"", "status", "\"suspended\""),
    (
        "User::\"elias\"",
        "<parent>",
        "{\"type\":\"Group\",\"id\":\"role.guardian\"}",
    ),
    (
        "User::\"elias\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    (
        "User::\"elias\"",
        "birth_date",
        "{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"1941-11-02\"}}",
    ),
    ("User::\"elias\"", "status", "\"deceased\""),
    (
        "User::\"fabio\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    (
        "User::\"fabio\"",
        "birth_date",
        "{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"1964-07-21\"}}",
    ),
    ("User::\"fabio\"", "status", "\"dormant\""),
    (
        "User::\"gloria\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    (
        "User::\"gloria\"",
        "birth_date",
        "{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"1947-05-09\"}}",
    ),
    ("User::\"gloria\"", "status", "\"presumed_inactive\""),
    (
        "User::\"hugo\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"home\"}",
    ),
    (
        "User::\"hugo\"",
        "birth_date",
        "{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"1983-12-01\"}}",
    ),
    ("User::\"hugo\"", "status", "\"archived\""),
    (
        "User::\"mallory\"",
        "<parent>",
        "{\"type\":\"Realm\",\"id\":\"office\"}",
    ),
    (
        "User::\"mallory\"",
        "birth_date",
        "{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"1979-02-02\"}}",
    ),
    ("User::\"mallory\"", "status", "\"active\""),
];

/// The same 81, by what each one asks.
///
/// **A total is satisfied by any cases that add up to it, and so is a count per
/// file.** The one thing `CASES` exists to protect is a case that records
/// a *decision* — and a decision deleted is paid for by an ordinary case
/// written in its place, in the same file, with every number this step prints
/// unchanged. Measured against the counts alone: the deferral at the foot of
/// the family pack — the written-down `allow` recording the cross-realm gap
/// `reference/identity-and-authorization.md §3` leaves open, whose own comment
/// says *"the day a core `forbid` arrives, this case flips and somebody decides
/// on purpose"* — deleted, and a duplicate of its neighbour appended under
/// another name leaves every count unchanged and `check passed`. It pins no
/// conjunct and is nobody's denial test, so `PINNED_BY_POLICY` and the coverage
/// check cannot see it either.
///
/// A count is not a set, which is what [`PINNED_BY_POLICY`] says one constant
/// along. Nor is a name, nor three of the four fields a request has: two family
/// cases differ only in their *action* — one about sharing restricted content
/// without a quorum, one about granting it — so without the verb, changing the
/// second's leaves the step's case line byte-identical, and no case anywhere
/// exercises `Action::"Grant"` against encrypted content. That is the verb the
/// policy's own comment calls the one that distributes the key, and a quorum
/// rule covering `Share` alone permits it, measured, with no case to say so.
///
/// Nor is a name: the deferral's principal changed from `User::"mallory"` —
/// another realm, which *is* the decision — to the child of the house named in
/// the first case of the same file makes it an exact duplicate of its
/// neighbour with the name untouched and the policy step byte-identical. So
/// the row is what the case *asks*: who, about what, in which file, under
/// which name.
///
/// **And the context, which is what most of the case files actually decide
/// by.** Five fields are not enough either. `agent_authority.toml`,
/// `chain_depth.toml`, `fail_closed.toml`, `grants_expire.toml` and the family
/// pack hold the request constant and vary `policy_epoch`, `quorum_met`,
/// `via_agent`, `actor_chain_depth` or `grant.entity` — so without them in the
/// row a case can be moved off the boundary it pins in one token and the bound
/// then widened to match. Two are measured:
/// `chain_depth.toml`'s denial moved from depth 3 to 3000 with
/// `chain_depth.cedar` widened from `> 2` to `> 2999`, which permits an agent
/// chain four deep against `NOCTUA.md §14.6`; and `fail_closed.toml`'s
/// `policy_epoch = -1` changed to `0`, which deletes the negative-epoch case by
/// making it a duplicate of the one above it. Both leave the step's case and
/// constraint counts unchanged and `check passed`, and `PINNED_BY_POLICY`
/// cannot see either, because both cases cite the same policy.
///
/// The context is the whole Cedar JSON, sorted, because a field left out of
/// this row is a field a case can be moved along. Changing any of the six is an
/// edit somebody reads.
const THE_CASES: [(&str, &str, &str, &str, &str, &str, &str); 81] = [
    (
        "policy/core/tests/agent_authority.toml",
        "a person acting unmarked is not impersonating",
        "User::\"ana\"",
        "Action::\"Read\"",
        "Content::\"notice\"",
        "{\"actor_chain_depth\":2,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Allow/because=family.a_member_reads_shared_content",
    ),
    (
        "policy/core/tests/agent_authority.toml",
        "an agent acting unmarked is impersonating and is refused",
        "Agent::\"ana-assistant\"",
        "Action::\"Read\"",
        "Content::\"notice\"",
        "{\"actor_chain_depth\":2,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/because=core.an_agent_never_impersonates",
    ),
    (
        "policy/core/tests/agent_authority.toml",
        "an agent that has spent its budget is refused",
        "Agent::\"ana-assistant-spent\"",
        "Action::\"Read\"",
        "Content::\"notice\"",
        "{\"actor_chain_depth\":2,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":true}",
        "Deny/because=core.budget_is_a_permission",
    ),
    (
        "policy/core/tests/agent_authority.toml",
        "an agent with budget, acting as an agent, is not refused by these",
        "Agent::\"ana-assistant\"",
        "Action::\"Read\"",
        "Content::\"notice\"",
        "{\"actor_chain_depth\":2,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":true}",
        "Allow/because=family.a_member_reads_shared_content",
    ),
    (
        "policy/core/tests/chain_depth.toml",
        "an agent acting for a person is within the limit",
        "Agent::\"ana-assistant\"",
        "Action::\"Read\"",
        "Content::\"notice\"",
        "{\"actor_chain_depth\":2,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":true}",
        "Allow/because=family.a_member_reads_shared_content",
    ),
    (
        "policy/core/tests/chain_depth.toml",
        "and a third link in the chain is refused",
        "Agent::\"ana-assistant\"",
        "Action::\"Read\"",
        "Content::\"notice\"",
        "{\"actor_chain_depth\":3,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":true}",
        "Deny/because=core.chain_depth_is_bounded",
    ),
    (
        "policy/core/tests/fail_closed.toml",
        "a request carrying no epoch is refused, however well formed it is",
        "User::\"ana\"",
        "Action::\"Read\"",
        "Content::\"photo\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":0,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/because=core.fail_closed",
    ),
    (
        "policy/core/tests/fail_closed.toml",
        "and an epoch below zero, which is the same absence with a different bug behind it",
        "User::\"ana\"",
        "Action::\"Read\"",
        "Content::\"photo\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":-1,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/because=core.fail_closed",
    ),
    (
        "policy/core/tests/fail_closed.toml",
        "the first epoch a realm has is a decision point, not an absent one",
        "User::\"ana\"",
        "Action::\"Read\"",
        "Content::\"photo\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":1,\"quorum_met\":true,\"via_agent\":false}",
        "Allow/because=family.a_member_reads_shared_content",
    ),
    (
        "policy/core/tests/grants_expire.toml",
        "a grant expiring at this very instant has expired",
        "User::\"ana\"",
        "Action::\"Read\"",
        "Content::\"photo\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.expiring_now\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/because=core.grant_not_expired",
    ),
    (
        "policy/core/tests/grants_expire.toml",
        "a grant the pack confers from a date that has not come is not a grant yet",
        "User::\"ana\"",
        "Action::\"Read\"",
        "Content::\"photo\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.not_yet_by_policy\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/because=core.grant_has_begun",
    ),
    (
        "policy/core/tests/grants_expire.toml",
        "a grant the pack confers needs no end, which is the exemption §4 writes",
        "User::\"ana\"",
        "Action::\"Read\"",
        "Content::\"photo\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.by_policy\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Allow/because=family.a_member_reads_shared_content",
    ),
    (
        "policy/core/tests/grants_expire.toml",
        "a grant whose end has passed is refused",
        "User::\"ana\"",
        "Action::\"Read\"",
        "Content::\"photo\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.expired\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/because=core.grant_not_expired",
    ),
    (
        "policy/core/tests/grants_expire.toml",
        "a grant whose window has not opened yet is not a grant",
        "User::\"ana\"",
        "Action::\"Read\"",
        "Content::\"photo\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.not_yet\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/because=core.grant_has_begun",
    ),
    (
        "policy/core/tests/grants_expire.toml",
        "a grant whose window opens at this very instant has begun",
        "User::\"ana\"",
        "Action::\"Read\"",
        "Content::\"photo\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.beginning_now\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Allow/because=family.a_member_reads_shared_content",
    ),
    (
        "policy/core/tests/grants_expire.toml",
        "a manual grant with no end is refused, however recently it was written",
        "User::\"ana\"",
        "Action::\"Read\"",
        "Content::\"photo\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.perpetual\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/because=core.grants_expire",
    ),
    (
        "policy/core/tests/grants_expire.toml",
        "and one that has begun is",
        "User::\"ana\"",
        "Action::\"Read\"",
        "Content::\"photo\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Allow/because=family.a_member_reads_shared_content",
    ),
    (
        "policy/core/tests/no_self_escalation.toml",
        "a guardian cannot grant to herself",
        "User::\"ana\"",
        "Action::\"Grant\"",
        "User::\"ana\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/because=core.no_self_escalation",
    ),
    (
        "policy/core/tests/no_self_escalation.toml",
        "but she may grant to another member, so the refusal above is about her and not about Grant",
        "User::\"ana\"",
        "Action::\"Grant\"",
        "User::\"carla\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Allow/because=family.a_guardian_administers_the_realm",
    ),
    (
        "policy/core/tests/no_self_escalation.toml",
        "nor revoke from herself, which is how a constraint is removed rather than added",
        "User::\"ana\"",
        "Action::\"Revoke\"",
        "User::\"ana\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/because=core.no_self_escalation",
    ),
    (
        "policy/core/tests/provenance.toml",
        "a request that names no grant is refused even when a pack would permit it",
        "User::\"ana\"",
        "Action::\"Read\"",
        "Content::\"photo\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/because=core.provenance",
    ),
    (
        "policy/core/tests/provenance.toml",
        "an agent with no grant is refused like anybody else",
        "Agent::\"ana-assistant\"",
        "Action::\"Read\"",
        "Content::\"notice\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":true}",
        "Deny/because=core.provenance",
    ),
    (
        "policy/core/tests/provenance.toml",
        "and so is a device",
        "Device::\"ana-laptop\"",
        "Action::\"Read\"",
        "Content::\"notice\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/because=core.provenance",
    ),
    (
        "policy/core/tests/quorum.toml",
        "an agent cannot delete on a human's behalf without that human confirming",
        "User::\"ana\"",
        "Action::\"Delete\"",
        "Content::\"diary\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":true}",
        "Deny/because=core.irreversible_needs_a_human",
    ),
    (
        "policy/core/tests/quorum.toml",
        "and with the confirmation it goes through, so the refusal is the confirmation's absence",
        "User::\"ana\"",
        "Action::\"Delete\"",
        "Content::\"diary\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":true,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":true}",
        "Allow/because=family.an_owner_governs_their_own_content",
    ),
    (
        "policy/core/tests/quorum.toml",
        "deleting without quorum is refused, even by the owner of the thing",
        "User::\"ana\"",
        "Action::\"Delete\"",
        "Content::\"diary\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":false,\"via_agent\":false}",
        "Deny/because=core.destructive_needs_quorum",
    ),
    (
        "policy/packs/family/tests/accounts.toml",
        "a device nobody attested reads nothing, not even what the house has made public",
        "Device::\"old-tablet\"",
        "Action::\"Read\"",
        "Content::\"notice\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/because=family.an_unattested_device_acts_on_nothing",
    ),
    (
        "policy/packs/family/tests/accounts.toml",
        "a guardian who has died does not read even his own account",
        "User::\"elias\"",
        "Action::\"Read\"",
        "User::\"elias\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/because=family.a_frozen_account_acts_on_nothing",
    ),
    (
        "policy/packs/family/tests/accounts.toml",
        "a guardian who has died grants nothing, before his roles have migrated",
        "User::\"elias\"",
        "Action::\"Grant\"",
        "Feature::\"gallery\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/because=family.a_frozen_account_acts_on_nothing",
    ),
    (
        "policy/packs/family/tests/accounts.toml",
        "a guardian who has died prunes nothing, before his roles have migrated",
        "User::\"elias\"",
        "Action::\"Delete\"",
        "Realm::\"home\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/because=family.a_frozen_account_acts_on_nothing",
    ),
    (
        "policy/packs/family/tests/accounts.toml",
        "a guardian who has died reads nothing the house shares",
        "User::\"elias\"",
        "Action::\"Read\"",
        "Content::\"photo\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/because=family.a_frozen_account_acts_on_nothing",
    ),
    (
        "policy/packs/family/tests/accounts.toml",
        "a suspended account does not read even its own account",
        "User::\"dora\"",
        "Action::\"Read\"",
        "User::\"dora\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/because=family.a_frozen_account_acts_on_nothing",
    ),
    (
        "policy/packs/family/tests/accounts.toml",
        "a suspended account has no file named",
        "User::\"dora\"",
        "Action::\"Read\"",
        "Feature::\"naming\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/because=family.a_frozen_account_acts_on_nothing",
    ),
    (
        "policy/packs/family/tests/accounts.toml",
        "a suspended account steps through no history, not even its own",
        "User::\"dora\"",
        "Action::\"Write\"",
        "Realm::\"home\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/because=family.a_frozen_account_acts_on_nothing",
    ),
    (
        "policy/packs/family/tests/accounts.toml",
        "a suspended account writes nothing, not even what it owns",
        "User::\"dora\"",
        "Action::\"Write\"",
        "Content::\"postcard\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/because=family.a_frozen_account_acts_on_nothing",
    ),
    (
        "policy/packs/family/tests/accounts.toml",
        "an account gone dormant still reads its own",
        "User::\"fabio\"",
        "Action::\"Read\"",
        "User::\"fabio\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Allow/because=family.a_member_reads_their_own_account",
    ),
    (
        "policy/packs/family/tests/accounts.toml",
        "an account in a state this pack does not know reads nothing, not even itself",
        "User::\"hugo\"",
        "Action::\"Read\"",
        "User::\"hugo\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/because=family.a_frozen_account_acts_on_nothing",
    ),
    (
        "policy/packs/family/tests/accounts.toml",
        "an account presumed inactive still reads what the house shares",
        "User::\"gloria\"",
        "Action::\"Read\"",
        "Content::\"photo\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Allow/because=family.a_member_reads_shared_content",
    ),
    (
        "policy/packs/family/tests/accounts.toml",
        "an attested device reads what the house has made public",
        "Device::\"ana-laptop\"",
        "Action::\"Read\"",
        "Content::\"notice\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Allow/because=family.a_member_reads_shared_content",
    ),
    (
        "policy/packs/family/tests/features.toml",
        "a device of the house has no file named, however attested",
        "Device::\"ana-laptop\"",
        "Action::\"Read\"",
        "Feature::\"naming\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/outside=family.a_member_names_files/boundary=principal",
    ),
    (
        "policy/packs/family/tests/features.toml",
        "a member has a file named and writes nothing to the feature",
        "User::\"carla\"",
        "Action::\"Write\"",
        "Feature::\"naming\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/outside=family.a_member_names_files/boundary=action",
    ),
    (
        "policy/packs/family/tests/features.toml",
        "a member of the house has a file named",
        "User::\"carla\"",
        "Action::\"Read\"",
        "Feature::\"naming\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Allow/because=family.a_member_names_files",
    ),
    (
        "policy/packs/family/tests/features.toml",
        "an outsider has a file named, which only the decision point refuses",
        "User::\"mallory\"",
        "Action::\"Read\"",
        "Feature::\"naming\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Allow/because=family.a_member_names_files",
    ),
    (
        "policy/packs/family/tests/features.toml",
        "and so does a child of the house",
        "User::\"bruno\"",
        "Action::\"Read\"",
        "Feature::\"naming\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Allow/because=family.a_member_names_files",
    ),
    (
        "policy/packs/family/tests/features.toml",
        "nor does an agent acting openly as one, until it has a permit of its own",
        "Agent::\"ana-assistant\"",
        "Action::\"Read\"",
        "Feature::\"naming\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":true}",
        "Deny/outside=family.a_member_names_files/boundary=principal",
    ),
    (
        "policy/packs/family/tests/features.toml",
        "the naming permit reaches no other feature",
        "User::\"carla\"",
        "Action::\"Read\"",
        "Feature::\"gallery\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/outside=family.a_member_names_files",
    ),
    (
        "policy/packs/family/tests/permissions.toml",
        "a child of the house reads what the house shares",
        "User::\"bruno\"",
        "Action::\"Read\"",
        "Content::\"photo\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Allow/because=family.a_member_reads_shared_content",
    ),
    (
        "policy/packs/family/tests/permissions.toml",
        "a guardian administers permissions and does not thereby read everything",
        "User::\"ana\"",
        "Action::\"Read\"",
        "User::\"carla\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/outside=family.a_guardian_administers_the_realm/boundary=action",
    ),
    (
        "policy/packs/family/tests/permissions.toml",
        "a guardian does not grant restricted content without a quorum",
        "User::\"ana\"",
        "Action::\"Grant\"",
        "Content::\"vault\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":false,\"via_agent\":false}",
        "Deny/because=family.giving_out_encrypted_content_needs_quorum",
    ),
    (
        "policy/packs/family/tests/permissions.toml",
        "a guardian does not share restricted content without a quorum",
        "User::\"ana\"",
        "Action::\"Share\"",
        "Content::\"vault\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":false,\"via_agent\":false}",
        "Deny/because=family.giving_out_encrypted_content_needs_quorum",
    ),
    (
        "policy/packs/family/tests/permissions.toml",
        "a guardian grants over a person without a quorum, which is not key distribution",
        "User::\"ana\"",
        "Action::\"Grant\"",
        "User::\"carla\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":false,\"via_agent\":false}",
        "Allow/because=family.a_guardian_administers_the_realm",
    ),
    (
        "policy/packs/family/tests/permissions.toml",
        "a guardian revokes over a realm she is not in, which the guardian permit does not refuse",
        "User::\"ana\"",
        "Action::\"Revoke\"",
        "Realm::\"office\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Allow/because=family.a_guardian_administers_the_realm",
    ),
    (
        "policy/packs/family/tests/permissions.toml",
        "a member reads what the house has made public",
        "User::\"bruno\"",
        "Action::\"Read\"",
        "Content::\"notice\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Allow/because=family.a_member_reads_shared_content",
    ),
    (
        "policy/packs/family/tests/permissions.toml",
        "a member reads what the house shares and does not share it onward",
        "User::\"bruno\"",
        "Action::\"Share\"",
        "Content::\"notice\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/outside=family.a_member_reads_shared_content/boundary=action",
    ),
    (
        "policy/packs/family/tests/permissions.toml",
        "a member who is not a guardian grants nothing",
        "User::\"bruno\"",
        "Action::\"Grant\"",
        "User::\"carla\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/outside=family.a_guardian_administers_the_realm",
    ),
    (
        "policy/packs/family/tests/permissions.toml",
        "an outsider reads what this household shares, which is the gap invariant 4 leaves",
        "User::\"mallory\"",
        "Action::\"Read\"",
        "Content::\"photo\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Allow/because=family.a_member_reads_shared_content",
    ),
    (
        "policy/packs/family/tests/permissions.toml",
        "an owner governs her own content and does not grant over it",
        "User::\"carla\"",
        "Action::\"Grant\"",
        "Content::\"recipe\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/outside=family.an_owner_governs_their_own_content/boundary=action",
    ),
    (
        "policy/packs/family/tests/permissions.toml",
        "an owner writes her own content",
        "User::\"ana\"",
        "Action::\"Write\"",
        "Content::\"diary\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Allow/because=family.an_owner_governs_their_own_content",
    ),
    (
        "policy/packs/family/tests/permissions.toml",
        "and another member does not, however ordinary the request looks",
        "User::\"carla\"",
        "Action::\"Write\"",
        "Content::\"diary\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/outside=family.an_owner_governs_their_own_content",
    ),
    (
        "policy/packs/family/tests/permissions.toml",
        "and does not share private content without one either",
        "User::\"ana\"",
        "Action::\"Share\"",
        "Content::\"diary\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":false,\"via_agent\":false}",
        "Deny/because=family.giving_out_encrypted_content_needs_quorum",
    ),
    (
        "policy/packs/family/tests/permissions.toml",
        "and not what is private, which §13.3 has already encrypted from us as well",
        "User::\"bruno\"",
        "Action::\"Read\"",
        "Content::\"diary\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/outside=family.a_member_reads_shared_content",
    ),
    (
        "policy/packs/family/tests/permissions.toml",
        "and not what is restricted, which is the label §13.3 encrypts hardest",
        "User::\"bruno\"",
        "Action::\"Read\"",
        "Content::\"vault\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/outside=family.a_member_reads_shared_content",
    ),
    (
        "policy/packs/family/tests/permissions.toml",
        "and shares it with one",
        "User::\"ana\"",
        "Action::\"Share\"",
        "Content::\"vault\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Allow/because=family.an_owner_governs_their_own_content",
    ),
    (
        "policy/packs/family/tests/permissions.toml",
        "sharing what the house already shares needs no quorum",
        "User::\"ana\"",
        "Action::\"Share\"",
        "Content::\"notice\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":false,\"via_agent\":false}",
        "Allow/because=family.an_owner_governs_their_own_content",
    ),
    (
        "policy/packs/family/tests/realm.toml",
        "a device in the realm steps through no history, however attested",
        "Device::\"ana-laptop\"",
        "Action::\"Write\"",
        "Realm::\"home\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/outside=family.a_member_steps_through_their_own_history/boundary=principal",
    ),
    (
        "policy/packs/family/tests/realm.toml",
        "a guardian prunes her own realm and not another",
        "User::\"ana\"",
        "Action::\"Delete\"",
        "Realm::\"office\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/outside=family.a_guardian_prunes_the_realm",
    ),
    (
        "policy/packs/family/tests/realm.toml",
        "a guardian prunes the realm and not her own account, which she may only read",
        "User::\"ana\"",
        "Action::\"Delete\"",
        "User::\"ana\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/outside=family.a_member_reads_their_own_account/boundary=action",
    ),
    (
        "policy/packs/family/tests/realm.toml",
        "a guardian prunes the realm she keeps, with a quorum",
        "User::\"ana\"",
        "Action::\"Delete\"",
        "Realm::\"home\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Allow/because=family.a_guardian_prunes_the_realm",
    ),
    (
        "policy/packs/family/tests/realm.toml",
        "a guardian reads her own realm and not another",
        "User::\"ana\"",
        "Action::\"Read\"",
        "Realm::\"office\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/outside=family.a_guardian_reads_the_realm",
    ),
    (
        "policy/packs/family/tests/realm.toml",
        "a guardian reads the realm and does not share it",
        "User::\"ana\"",
        "Action::\"Share\"",
        "Realm::\"home\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/outside=family.a_guardian_reads_the_realm/boundary=action",
    ),
    (
        "policy/packs/family/tests/realm.toml",
        "a guardian reads the realm she keeps",
        "User::\"ana\"",
        "Action::\"Read\"",
        "Realm::\"home\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Allow/because=family.a_guardian_reads_the_realm",
    ),
    (
        "policy/packs/family/tests/realm.toml",
        "a member reads her own account",
        "User::\"bruno\"",
        "Action::\"Read\"",
        "User::\"bruno\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Allow/because=family.a_member_reads_their_own_account",
    ),
    (
        "policy/packs/family/tests/realm.toml",
        "a member reads her own account and does not rewrite it",
        "User::\"bruno\"",
        "Action::\"Write\"",
        "User::\"bruno\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/outside=family.a_member_reads_their_own_account/boundary=action",
    ),
    (
        "policy/packs/family/tests/realm.toml",
        "a member steps through her history in her own realm",
        "User::\"bruno\"",
        "Action::\"Write\"",
        "Realm::\"home\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Allow/because=family.a_member_steps_through_their_own_history",
    ),
    (
        "policy/packs/family/tests/realm.toml",
        "a member who is not a guardian prunes nothing, quorum or not",
        "User::\"bruno\"",
        "Action::\"Delete\"",
        "Realm::\"home\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/outside=family.a_guardian_prunes_the_realm",
    ),
    (
        "policy/packs/family/tests/realm.toml",
        "a step taken on an expired grant is refused, whatever the pack permits",
        "User::\"ana\"",
        "Action::\"Write\"",
        "Realm::\"home\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.expired\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/because=core.grant_not_expired",
    ),
    (
        "policy/packs/family/tests/realm.toml",
        "and a member who is not a guardian does not",
        "User::\"carla\"",
        "Action::\"Read\"",
        "Realm::\"home\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/outside=family.a_guardian_reads_the_realm",
    ),
    (
        "policy/packs/family/tests/realm.toml",
        "and not another member's, however much she administers",
        "User::\"ana\"",
        "Action::\"Read\"",
        "User::\"bruno\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/outside=family.a_member_reads_their_own_account",
    ),
    (
        "policy/packs/family/tests/realm.toml",
        "and not without one, which the core decides and the pack cannot override",
        "User::\"ana\"",
        "Action::\"Delete\"",
        "Realm::\"home\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":false,\"via_agent\":false}",
        "Deny/because=core.destructive_needs_quorum",
    ),
    (
        "policy/packs/family/tests/realm.toml",
        "and somebody from another realm does not",
        "User::\"mallory\"",
        "Action::\"Write\"",
        "Realm::\"home\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":false}",
        "Deny/outside=family.a_member_steps_through_their_own_history",
    ),
    (
        "policy/packs/family/tests/realm.toml",
        "nor does an agent, even one acting openly as an agent",
        "Agent::\"ana-assistant\"",
        "Action::\"Write\"",
        "Realm::\"home\"",
        "{\"actor_chain_depth\":0,\"confirmed_by_human\":false,\"grant\":{\"__entity\":{\"type\":\"Grant\",\"id\":\"g.current\"}},\"now\":{\"__extn\":{\"fn\":\"datetime\",\"arg\":\"2026-08-26T00:00:00Z\"}},\"policy_epoch\":7,\"quorum_met\":true,\"via_agent\":true}",
        "Deny/outside=family.a_member_steps_through_their_own_history/boundary=principal",
    ),
];

/// Every clause a case pins, held against the number written down.
///
/// **65, and what the clauses behind it are.** The frozen-account floor names
/// the states that may act — `active`, `dormant` and `presumed_inactive`, three
/// comparisons, and a case for each state notices one deleted. Listing the
/// states that may not, `suspended` and `deceased`, allows any other value of a
/// `status` nothing checks: a typo, a different case, a state added later.
///
/// **The history permit's type test.** Without its `principal is User`,
/// `family.a_member_steps_through_their_own_history` lets two requests the
/// schema can build write the realm — measured, an attested device and an
/// agent acting openly as one. The pairing rule cannot pin it — a denial for a
/// device pairs only with an `allow` for a device, which that permit never
/// gives — so a case may say `the_principal_is_the_boundary`, and two of them
/// pin the type test. The naming permit's `principal is User` is pinned the
/// same way, beside its verb and the one feature it names.
///
/// **The family pack's rules for the decision point.** Their clauses are one
/// row each in [`PINNED_BY_POLICY`]: the realm's permits pin their verb, their
/// membership condition and, where a request can show it, their type; the two
/// floors pin each comparison they make and each attribute they read. Three
/// type tests are not among them, because no request the schema can build
/// shows one working — `policy/packs/family/realm.cedar` says which, beside
/// each permit that leaves one out.
///
/// **The one excuse that skips a conjunct.** Asking whether the right-hand side
/// of `!(x has y) || …` holds none of nine literal shapes lets four ordinary
/// predicates walk through it, measured — a negated assertion, an assertion
/// about a different attribute, a method call, and everything past the first
/// `||`. Asked the other way, as an equality against the dereference the guard
/// names, it does not excuse the second conjunct of
/// `core.an_agent_never_impersonates`, which is one clause inside the mutation
/// loop.
///
/// **Two cases at boundaries no mutation operator reaches:** a grant whose
/// window opens at the instant the cases call `now`, and a realm's first valid
/// epoch. Both are the *too strong* direction — an ordering comparison widened
/// by one turns an Allow into a Deny, and `inside_a_conjunct` proposes
/// `==`/`!=` only, so no mutation operator reaches one. A case or nothing.
///
/// **The family pack's quorum rule.** Covering `action == Action::"Share"` and
/// `sensitivity == "restricted"` alone, it allows `ana Grant Content::"vault"`
/// with `quorum_met = false`, and `ana Grant Content::"recipe"` — carla's
/// *private* content, which ana is refused a `Read` on two lines above — as
/// well, measured. `Grant` is the verb that distributes the key. The rule names
/// both verbs and both encrypted labels, and says `resource is Content` so that
/// granting over a person stays ordinary administration; that clause is a pin.
///
/// **The prose and the constant are one number.** A number that moves in the
/// line the compiler reads and not here sends a reader reconciling a count
/// against a document to the wrong one, and this doc is the only account of
/// *why* the number is what it is.
/// `the_written_number_is_the_number` reads this comment and requires the two to
/// agree, in the shape `xtask/canaries/README.md` calls one rule written twice.
const PINNED: usize = 65;

/// The same 65, by the policy each clause belongs to.
///
/// **A count is not a set, and every policy here shares one number.** `PINNED`
/// is a total: a policy gaining a pinned conjunct pays for another losing one,
/// and the line a reader checks never moves. That is the compensating move
/// this repository refuses everywhere else — `CORE_POLICIES` is a name list
/// compared both directions for the same reason one directory along, and a
/// total is the count beside it.
///
/// Both directions here too: a policy that appears in one and not the other is
/// a failure, so a *new* policy pinned by nothing cannot arrive under a total
/// that happens to hold. `core.provenance` is written with **0** for exactly
/// that reason — its conjuncts are all guards, which
/// `guards_a_dereference` excuses by name — and a zero row says so where an
/// absent row would say nothing.
const PINNED_BY_POLICY: [(&str, usize); 22] = [
    ("core.an_agent_never_impersonates", 2),
    ("core.budget_is_a_permission", 2),
    ("core.chain_depth_is_bounded", 2),
    ("core.destructive_needs_quorum", 2),
    ("core.fail_closed", 2),
    ("core.grant_has_begun", 3),
    ("core.grant_not_expired", 3),
    ("core.grants_expire", 4),
    ("core.irreversible_needs_a_human", 3),
    ("core.no_self_escalation", 2),
    ("core.provenance", 1),
    ("family.a_frozen_account_acts_on_nothing", 5),
    ("family.a_guardian_administers_the_realm", 2),
    ("family.a_guardian_prunes_the_realm", 4),
    ("family.a_guardian_reads_the_realm", 3),
    ("family.a_member_names_files", 3),
    ("family.a_member_reads_shared_content", 4),
    ("family.a_member_reads_their_own_account", 3),
    ("family.a_member_steps_through_their_own_history", 4),
    ("family.an_owner_governs_their_own_content", 3),
    ("family.an_unattested_device_acts_on_nothing", 2),
    ("family.giving_out_encrypted_content_needs_quorum", 6),
];

/// Split out of [`check`] only because that function is at the line limit.
fn the_pinned_clauses_are_the_written_number(
    pinned: &std::collections::BTreeMap<String, usize>,
    failures: &mut Vec<String>,
) {
    // **The number, not "more than nothing".** `pinned > 0` is a floor with
    // everything in it: widening `guards_a_dereference` — the one function that
    // excuses a conjunct from the mutation loop — to `if true { return true; }`
    // takes every clause it then excuses out of this count, with
    // `check passed` under such a floor. What goes with them is the loop's
    // whole subject: with the excuse widened, `core.fail_closed` can be given
    // `&& !(context has via_agent && context.via_agent)` and every agent-mediated
    // request escapes it, with nothing to report it.
    //
    // Written out, the way `CANDIDATES == 64` and `WAIT_ATTEMPTS == 1000` are, so
    // a conjunct that stops being pinned has to be explained rather than
    // absorbed. It rises when a policy gains a clause, which is a deliberate,
    // reviewed change to this line.
    let total: usize = pinned.values().sum();
    // **The table first, because it says which policy moved.** The total below
    // is what the console reports and what the documents quote; this is what
    // stops one policy's gain paying for another's loss.
    //
    // **The skip lives inside the function.** As `if total != 0` here, at a
    // call site no test enters, switching the guard off stops the rows being
    // compared at all and leaves only the total, which that comparison's own
    // doc calls the compensating move. The reason for the skip — a pack that did
    // not load is named by the step above, and repeating it here as one
    // missing row per policy buries it — sits where the tests over that
    // function and the operator over it can both reach it.
    let written: std::collections::BTreeMap<String, usize> = PINNED_BY_POLICY
        .iter()
        .map(|(name, count)| ((*name).to_owned(), *count))
        .collect();
    assert_the_same_policies_are_pinned(pinned, &written, failures);
    if total != PINNED && total != 0 {
        ui::failure(&format!(
            "{total} constraint(s) and condition(s) are pinned by a case, and this \
             repository's policies have {PINNED}: a clause that stopped being pinned is \
             one no case would notice the removal of"
        ));
        ui::detail(
            "if a policy gained or lost a clause on purpose, say so here — the number is \
             written down because `> 0` is a floor with everything in it, and widening \
             the one excuse that skips a conjunct took it from 28 to 21 with the run \
             still green",
        );
        failures.push("the pinned-clause count moved".to_owned());
    }
}

/// Every policy pinned by the same number of clauses as before, by name.
///
/// Reported as three lists rather than as one diff, because what a reader needs
/// is which policy moved and in which direction — a policy that stopped being
/// pinned is a rule a case would not notice the removal of, and a policy that
/// is here and not in the table is one nobody has looked at.
fn assert_the_same_policies_are_pinned(
    measured: &std::collections::BTreeMap<String, usize>,
    written: &std::collections::BTreeMap<String, usize>,
    failures: &mut Vec<String>,
) {
    // A pack that pinned nothing at all did not load, which the step above
    // reports; saying it again as one row per written policy buries the one
    // sentence a reader needs.
    let nothing_was_pinned = measured.values().sum::<usize>() == 0;
    let mut moved: Vec<String> = Vec::new();
    for (name, count) in measured {
        match written.get(name) {
            Some(expected) if expected == count => {}
            Some(expected) => moved.push(format!(
                "`{name}` pins {count} clause(s) and `PINNED_BY_POLICY` says {expected}"
            )),
            None => moved.push(format!(
                "`{name}` pins {count} clause(s) and `PINNED_BY_POLICY` does not name it, so \
                 this policy arrived with nobody looking at what it pins"
            )),
        }
    }
    for name in written.keys() {
        if !measured.contains_key(name) {
            moved.push(format!(
                "`{name}` is in `PINNED_BY_POLICY` and the pack no longer holds it"
            ));
        }
    }
    if nothing_was_pinned {
        return;
    }
    // The condition above the call, for the reason
    // `the_cases_are_the_written_number` carries: an early return puts the
    // verdict at top level, where the census that pins it records `—`.
    if !moved.is_empty() {
        ui::failure(
            "the clauses each policy pins are not the ones written down. A total cannot \
             see this: one policy gaining a pinned conjunct pays for another losing one \
             and the written number never moves",
        );
        for line in &moved {
            ui::detail(line);
        }
        failures.push("the pinned clauses moved policy".to_owned());
    }
}

/// Runs the whole policy gate, accumulating failures rather than returning early.
pub(crate) fn check(root: &Path, failures: &mut Vec<String>) {
    ui::heading("policy");

    // **The core policies, by name.** Counted only by what is on disk, a policy
    // file that is deleted makes every count in this step smaller and nothing
    // says so: measured, `policy/core/grants_expire.cedar` and its case file
    // both removed, and the whole step green — the policy count, the case count
    // and the denial-test count each a little smaller — with two of the
    // platform's core invariants simply gone. Only a
    // citation in `policy/core/README.md` would notice that one, and a policy
    // nothing happens to cite would go in silence.
    //
    // This is the list, in the same shape as
    // `wall.rs`'s `SHAPES` and `check.rs`'s `SUITES`: a written list makes
    // removing one a two-place edit, one of them here, where whoever reads the
    // diff meets the claim that a core invariant was dropped on purpose.
    // **Each one still holds a policy, not just a file.** Asking `is_file()`
    // and nothing about the contents, a core policy file reduced to a comment
    // leaves the list untouched in the diff — the one thing the written list is
    // supposed to buy — while `pack.len()`, the case count and the coverage
    // count all just get smaller and each still reads `ok`. Measured with
    // `grants_expire.cedar` emptied and its two cases removed:
    //
    // ```text
    // ok  11 policy(ies) parse and validate       (13 before)
    // ok  7 core invariant(s) of §14.9 are on disk
    // ok  25 case(s) in 8 file(s) decide as written  (27 before)
    // ok  N policy(ies) each have a denial test  (fewer than before)
    // check passed
    // ```
    //
    // Two of `§14.9`'s invariants gone, three numbers quietly smaller. The
    // question is not whether the file exists but whether it still says
    // something, and the pack is where the answer is: a policy file that
    // contributes no `permit` or `forbid` contributes nothing to any decision.
    let mut missing: Vec<String> = core_policies_that_decide_nothing(root);
    if pack_decides_nothing(root) {
        missing.push(
            "packs/family holds no policy that decides anything, and it is the pack this \
             stage implements"
                .to_owned(),
        );
    }

    missing.sort_unstable();
    if missing.is_empty() {
        ui::ok(&format!(
            "{} core policy file(s) are on disk and decide something, by name, and \
             the family pack is not empty",
            CORE_POLICIES.len()
        ));
    } else {
        for problem in &missing {
            ui::failure(&format!("policy/core/{problem}"));
        }
        ui::detail(
            "§14.9's core invariants are a written list because one must not be able to \
             appear or vanish quietly: add the file to `CORE_POLICIES` in \
             xtask/src/policy.rs, or take it out of policy/core/",
        );
        failures.push(format!("{} core policy(ies) missing", missing.len()));
    }

    let pack = match nc_policy::load(root) {
        Ok(pack) => pack,
        Err(problems) => {
            for problem in &problems {
                report(problem);
            }
            failures.push(format!(
                "{} problem(s) with the policy pack",
                problems.len()
            ));
            return;
        }
    };
    the_core_rules_are_the_pack(&pack, failures);
    the_core_is_all_forbids(&pack, failures);
    the_realms_pin_the_packs_they_select(pack.pins(), pack.versions(), failures);
    every_command_names_an_action_the_schema_has(&pack, failures);

    let (cases, files) = match nc_policy::cases::load(root) {
        Ok(both) => both,
        Err(problems) => {
            for problem in &problems {
                report(problem);
            }
            failures.push(format!(
                "{} problem(s) with the policy cases",
                problems.len()
            ));
            return;
        }
    };
    if files.is_empty() {
        ui::failure(
            "no case file was found, so every policy is unproven and the coverage \
             check below has nothing to count",
        );
        failures.push("no policy cases".to_owned());
        return;
    }

    let (problems, ran, pinned_by_policy) = nc_policy::run(&pack, &cases);
    // The total the console reports, and the table below is what holds it.
    let pinned: usize = pinned_by_policy.values().sum();
    // **And how many constraints the widening check looks at.** That check is
    // what makes "every policy has a denial test" mean something — a permit
    // widened by one token keeps its denial test and stops being the rule it
    // was — and with no count returned it could inspect nothing and say
    // nothing: deleting its one call line would leave the entire suite green,
    // measured.
    the_pinned_clauses_are_the_written_number(&pinned_by_policy, failures);
    the_cases_are_the_written_number(ran, &cases, failures);
    if pinned == 0 {
        ui::failure(
            "no constraint or condition was offered for mutation, so nothing checks that \
             a rule can be widened by one token with its denial test still passing",
        );
        failures.push("the widening check inspected nothing".to_owned());
    }
    if ran == 0 {
        ui::failure("no case ran, so the pack was not exercised at all");
        failures.push("no policy case ran".to_owned());
    } else if problems.is_empty() {
        ui::ok(&format!(
            "{ran} case(s) in {} file(s) decide as written, and {pinned} constraint(s) \
             and condition(s) are pinned by one of them",
            files.len()
        ));
    } else {
        for problem in &problems {
            report(problem);
        }
        failures.push(format!("{} policy case(s) failed", problems.len()));
    }

    denial_tests(&pack, &cases, &problems, pinned, failures);
    the_schema_is_the_reference(root, failures);
}

/// The declarations `reference/identity-and-authorization.md §4` fences, and the
/// ones `policy/schema.cedarschema` writes, as comparable statements.
///
/// **Text, not Cedar's own parse, and the reason is the difference between the
/// two files.** They are the same language and they are written in different
/// house styles: the reference leaves attribute names bare, the schema quotes
/// them, and the schema carries four hundred lines of prose the reference does
/// not. What survives normalisation is the declarations themselves — comments
/// stripped, quotes dropped, whitespace collapsed, split at the `;` that ends
/// each statement — which is exactly the content the transcription claim is
/// about.
///
/// Returned as a set rather than a list, because the two files are free to
/// declare in different orders and a reader comparing them by hand would not
/// call that a disagreement.
fn declarations_in(source: &str) -> std::collections::BTreeSet<String> {
    let without_comments: String = source
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
        .collect::<Vec<_>>()
        .join(" ");
    without_comments
        .split(';')
        .map(|statement| {
            let collapsed = statement
                .replace('"', "")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            // A trailing comma before a closing brace is a spelling, not a
            // declaration: the reference writes one and the schema does not.
            optionality_in_one_place(&collapsed).replace(", }", " }")
        })
        .filter(|statement| !statement.is_empty())
        .collect()
}

/// The two spellings Cedar allows for an optional attribute, written one way.
///
/// `valid_until: datetime?` and `valid_until?: datetime` are the same
/// declaration, and the two files choose differently: the reference puts the
/// question mark on the type and the schema puts it on the name. Measured, a
/// comparison that calls that a disagreement reports four entities and the
/// action block as drifted on a tree where nothing drifted — which is the
/// wrong-direction failure a gate has to be free of before its findings mean
/// anything.
///
/// A `?` anywhere else is left where it is: only one that ends the **type** of
/// a `name: Type` declaration moves.
///
/// **A type, not a bare identifier.** A walk back over `[A-Za-z0-9_]` normalises
/// exactly the types §4 happens to use today and no others: `tags: Set<String>?`
/// stops at the `>`, keeps its question mark where it is, and then disagrees
/// with the schema's `"tags"?: Set<String>` — the same declaration, spelled the
/// two ways Cedar allows. Measured by declaring one in each file's own house
/// style: **both** halves of the comparison fire at once, *"the schema is a
/// transcription and has fallen behind the document that outranks it"* and
/// *"this belongs there first"* — and
/// following the second pushes the transcription's spelling up into the file
/// that outranks, which is the worst thing a gate in this position can ask for.
///
/// The `:` of the current declaration is remembered instead, at depth zero in
/// `<…>`, and forgotten at a comma or a brace. Whatever the type is made of, a
/// `?` that ends it moves to the name.
fn optionality_in_one_place(statement: &str) -> String {
    let mut out: Vec<char> = Vec::with_capacity(statement.len());
    // Where this declaration's `:` sits, and how deep inside `<…>` we are.
    let mut colon: Option<usize> = None;
    let mut depth = 0_i32;
    for character in statement.chars() {
        match character {
            '<' => depth += 1,
            '>' => depth -= 1,
            ':' if depth == 0 => colon = Some(out.len()),
            ',' | '{' | '}' if depth == 0 => colon = None,
            _ => {}
        }
        if character != '?' {
            out.push(character);
            continue;
        }
        // `name: Type?` becomes `name?: Type`, which is the spelling the schema
        // uses. A `?` with no `: ` behind it in this declaration is already on
        // the name, or is not an optionality mark at all, and stays put.
        match colon {
            Some(at) if out.get(at + 1) == Some(&' ') && out.len() > at + 2 => {
                out.insert(at, '?');
                colon = Some(at + 1);
            }
            _ => out.push('?'),
        }
    }
    out.into_iter().collect()
}

/// Whether a fence's info string says the block is Cedar.
///
/// **The two spellings, written out.** A bare fence is Cedar because that is
/// how §4 writes its schema today. `cedar` is the language. `cedarschema` is
/// Cedar's schema dialect — the identifier its own tooling uses, and the
/// extension of `policy/schema.cedarschema`, the file this section is
/// transcribed into — so it is the likelier tag on this fence, not a foreign
/// language, and refusing it makes the gate report that §4 fences nothing.
///
/// Anything else names another language and is an illustration: this section
/// already carries prose about PostgreSQL, and a reader taking every fence
/// parses an ordinary fence tagged `sql` under this heading as declarations.
fn fences_cedar(info: &str) -> bool {
    info.is_empty()
        || info.eq_ignore_ascii_case("cedar")
        || info.eq_ignore_ascii_case("cedarschema")
}

/// Every Cedar block `reference/identity-and-authorization.md §4` fences.
///
/// **Every block, not the first one.** Reading one fence and stopping makes the
/// gate's subject *block 1 of §4* while its verdict says *§4*: a declaration the
/// reference makes in a second fence — an entirely ordinary Markdown edit, and
/// this document already fences twice in other sections — is compared against
/// nothing, and the schema can fall behind the document that outranks
/// `NOCTUA.md` with the step printing `ok`. Measured: a whole `entity Session`
/// added to §4 in a fence of its own leaves `10 schema declaration(s) are the
/// ones … §4 makes` unmoved and `check passed`, with §4 making eleven.
///
/// Reading one fence is wrong in the other direction too: splitting the
/// existing block in two — entities in one fence, actions in another, with a
/// sentence between — reports everything after the split as *invented by the
/// schema*, which tells the author to move a correct declaration into the
/// document it is already in.
///
/// The section ends where the next `## ` heading begins, so a fence in §5 is
/// not §4's.
fn the_fenced_schema_of(reference: &str) -> Option<String> {
    let section = reference.split_once("\n## 4. Cedar schema")?.1;
    let section = section
        .split_once("\n## ")
        .map_or(section, |(above, _)| above);
    let mut blocks = String::new();
    let mut rest = section;
    while let Some((_, after)) = rest.split_once("```") {
        let Some((block, beyond)) = after.split_once("```") else {
            break;
        };
        rest = beyond;
        // **The fence's info string decides whether it is Cedar**, and reading
        // every fence reads every *language*. Measured: an ordinary illustrative
        // ```sql block under this heading — and the section already carries four
        // prose paragraphs discussing PostgreSQL — is parsed as declarations,
        // and the gate then refuses with `the schema is a transcription and has
        // fallen behind`, which states the inverse of what happened and sends
        // the author to paste SQL into a file Cedar has to parse.
        //
        // A bare fence is Cedar because that is how this section writes its
        // schema today; anything that names another language is an example.
        //
        // **And `cedarschema` is Cedar.** Taking `cedar` and nothing else is the
        // mirror failure — too strong rather than too weak, and green only
        // while nobody writes the likelier tag. `cedarschema` is Cedar's own
        // schema dialect, the identifier its tooling uses and the extension of
        // the file this section is transcribed into; it is the tag an author
        // reaches for
        // when labelling this very fence. Measured with `cedar` alone: tagging
        // §4's fence with it makes the gate say
        // `has no fenced block under `## 4. Cedar schema``, which states the
        // inverse of the truth, and tagging only some of the fences reports the
        // schema's correct declarations as invented.
        let (info, body) = block.split_once('\n').unwrap_or((block, ""));
        if !fences_cedar(info.trim()) {
            continue;
        }
        blocks.push_str(body);
        blocks.push('\n');
    }
    (!blocks.trim().is_empty()).then_some(blocks)
}

/// What each file declares and the other does not, in that order.
///
/// **Both directions are mutation operators, and a test kills each.**
/// `xtask/mutants.py` carries one for each — the schema falling behind the
/// reference, and the schema inventing a declaration the reference does not
/// make — and with only the live tree to drive the comparison, both survive.
///
/// So the comparison is a function over two sets, and the test beside it drives
/// each direction on its own. `§4` outranks the schema, which is why the first
/// half is the one that names a transcription falling behind.
fn the_two_ways_they_disagree<'a>(
    declared: &'a std::collections::BTreeSet<String>,
    transcribed: &'a std::collections::BTreeSet<String>,
) -> (Vec<&'a String>, Vec<&'a String>) {
    (
        declared.difference(transcribed).collect(),
        transcribed.difference(declared).collect(),
    )
}

/// `policy/schema.cedarschema` declares what the reference declares, both ways.
///
/// **The one file in this repository declared to be a transcription of the
/// document that outranks `NOCTUA.md` is compared with that document
/// here.** Every other cross-document claim here has a gate — commands against
/// the schema's actions, `CORE_RULES` against the pack, `PINNED_BY_POLICY`
/// against the mutation loop, every `§` against a real section — and without
/// this one it rests on a person having last checked. Measured without it:
/// swapping `guest_tier` for an invented `nickname`, an omission and an
/// invention in one edit, leaves the whole gate green, and `guest_tier` appears
/// nowhere else in the tree for anything else to notice.
///
/// **Both directions, because the two failures are different.** A declaration in
/// the reference and not here is a transcription that fell behind; one here and
/// not in the reference is an invention, and the schema's own header says what
/// to do about it — *where this file and the reference disagree, the reference is
/// what changes, because it is the one that outranks.*
///
/// It reads the workspace above this repository, so a bare checkout announces a
/// skip rather than reporting a failure it cannot have an opinion about. That is
/// the fifth step to do so, and `check.rs`'s `workspace_above` carries the
/// header saying why the shape is a skip and not an error — a gate that *cannot*
/// run is not a gate that failed, and it is not one that passed either.
fn the_schema_is_the_reference(root: &Path, failures: &mut Vec<String>) {
    let above = crate::sections::what_is_above(root);
    let Some(workspace) = crate::sections::workspace_of(root) else {
        crate::check::no_workspace("the Cedar schema's transcription", &above);
        return;
    };
    let reference = workspace.join("reference/identity-and-authorization.md");
    let Ok(text) = std::fs::read_to_string(&reference) else {
        ui::failure(
            "the workspace is above this repository and \
             reference/identity-and-authorization.md is not in it, so the schema was \
             compared against nothing",
        );
        failures.push("the reference is unreadable".to_owned());
        return;
    };
    let Some(fenced) = the_fenced_schema_of(&text) else {
        ui::failure(
            "reference/identity-and-authorization.md has no fenced block under \
             `## 4. Cedar schema`, so this step read nothing to compare against",
        );
        failures.push("the reference's schema is unreadable".to_owned());
        return;
    };
    let Ok(schema) = std::fs::read_to_string(root.join("policy").join("schema.cedarschema")) else {
        ui::failure("policy/schema.cedarschema is not readable");
        failures.push("the schema is unreadable".to_owned());
        return;
    };
    let declared = declarations_in(&fenced);
    let transcribed = declarations_in(&schema);
    if declared.is_empty() {
        ui::failure(
            "§4 of the reference fenced no declaration this step could read, so it \
             compared the schema against an empty set",
        );
        failures.push("the reference declares nothing".to_owned());
        return;
    }
    let (behind, invented) = the_two_ways_they_disagree(&declared, &transcribed);
    if behind.is_empty() && invented.is_empty() {
        ui::ok(&format!(
            "{} schema declaration(s) are the ones reference/identity-and-authorization.md §4 makes",
            declared.len()
        ));
        return;
    }
    for statement in behind {
        ui::failure(&format!(
            "reference/identity-and-authorization.md §4 declares `{statement}` and policy/schema.cedarschema does \
             not: the schema is a transcription and has fallen behind the document that \
             outranks it"
        ));
    }
    for statement in invented {
        ui::failure(&format!(
            "policy/schema.cedarschema declares `{statement}` and reference/identity-and-authorization.md §4 does \
             not: where the two disagree the reference is what changes, so this belongs \
             there first"
        ));
    }
    failures.push("the schema is not the reference".to_owned());
}

/// §14.9, invariant 7: a policy without a negative test does not enter the
/// monorepo.
///
/// Takes what the case runner found, because what this count is worth depends on
/// it.
fn denial_tests(
    pack: &nc_policy::Pack,
    cases: &[nc_policy::Case],
    problems: &[nc_policy::Problem],
    pinned: usize,
    failures: &mut Vec<String>,
) {
    let (gaps, checked) = nc_policy::coverage(pack, cases);
    // **Not `ok` while the case-runner disowned a case.** `coverage` credits a
    // policy on the strength of a case *naming* it, and what makes that naming
    // honest is the step above: a case claiming `because = "core.chain_depth_
    // is_bounded"` while something else decided the request is a
    // `wrong-decision` finding there. Measured with the count printed regardless
    // — one case retargeted so an unrelated policy refuses it:
    //
    // ```text
    // error … says core.chain_depth_is_bounded decided it, and what decided it
    //       was {"core.irreversible_needs_a_human"}
    // ok    N policy(ies) each have a denial test         <- criterion 11, green
    // ```
    //
    // `check` fails, so the tree is safe — but the line that *is* criterion
    // 11 would stay green over a policy whose denial test proves nothing about
    // it, and "proven by a gate that counts them" invites reading that line
    // alone.
    // A count is only worth what the cases behind it are worth.
    //
    // **The gaps are reported first, and unconditionally.** The reassuring
    // count is withheld; withholding it by an `if` in front of the whole block
    // takes the gap list and its `failures.push` with it, so a core policy
    // with no denial test at all becomes invisible the moment any case fails.
    // Measured — one denial case deleted, and one unrelated case retargeted so
    // it decides for the wrong reason:
    //
    // ```text
    // error policy/core/chain_depth.cedar — `core.chain_depth_is_bounded` has
    //       no denial test                                <- alone: printed
    //                                                     <- with a failing case: gone
    // ```
    //
    // `pinned == 0` withholds the count too, for the reason the `pinned` check
    // above states in its own words: it is what makes this count mean anything.
    // Both of the count's dependencies withhold it.
    // **And this arm cannot be reached from `check`, which is written down
    // rather than left to look like coverage.** `checked` is what `coverage`
    // counted over `pack.effects`, and `load` returns `Ok(Pack)` only when it
    // found no problems — a pack holding no policy at all is an `empty-pack`
    // problem, so `check` returns above without ever calling this. Measured:
    // `if checked == 0` written `if false` leaves the whole suite green, and it
    // is not a gap in the tests — no input can tell the two apart. What it is a
    // floor under is `coverage` itself: the counter it returns sits at the
    // bottom of a loop whose own paragraph records a `continue` inserted above
    // it, and a `continue` that fired for *every* policy would report `ok 0
    // policy(ies) each have a denial test` with criterion 11 asked of nothing.
    // That is the reading this arm exists for, and it is the reason it stays.
    if checked == 0 {
        ui::failure("coverage looked at no policy, so §14.9, invariant 7 was not enforced");
        failures.push("the denial-test gate checked nothing".to_owned());
    } else if !gaps.is_empty() {
        for gap in &gaps {
            report(gap);
        }
        // Not "policy(ies) without a denial test": `coverage` also reports
        // `boundary-without-a-control`, which is a case whose control is
        // missing rather than a policy whose test is. Calling both the same
        // thing sends a reader looking for a policy that is in fact covered.
        failures.push(format!("{} gap(s) in the denial tests", gaps.len()));
    } else if !problems.is_empty() || pinned == 0 {
        ui::detail(
            "the denial-test count is not reported: it is worth what the cases behind it \
             are worth, and a line above says they are not worth it yet",
        );
    } else {
        ui::ok(&format!("{checked} policy(ies) each have a denial test"));
    }
}

#[cfg(test)]
mod tests {
    /// A policy file decides something, and prose about one does not.
    ///
    /// **Both halves of this rule, driven here.** Reduced to
    /// `code.contains(effect)` — dropping the word boundary whole — the tree's
    /// own files answer as before, because no file on disk holds a word that
    /// merely contains `permit` or `forbid`, and the two `@doc` rows below
    /// fail. The comment strip is the same: it exists against a false refusal
    /// no file on disk provokes, and the commented-out rows decide it. Both are
    /// the narrowing half of a rule whose other half is live, so a wrong answer
    /// from either is silent.
    #[test]
    fn what_counts_as_deciding_something() {
        for (text, decides, why) in [
            (
                "permit(principal, action, resource);",
                true,
                "a permit decides",
            ),
            (
                "forbid(principal, action, resource);",
                true,
                "a forbid decides",
            ),
            (
                "// permit(principal, action, resource);",
                false,
                "a commented-out policy decides nothing",
            ),
            (
                "/* permit(principal, action, resource); */",
                false,
                "a block comment decides nothing either",
            ),
            (
                "@doc(\"permitted only for the owner\")\n@id(\"a\")",
                false,
                "a word that contains `permit` is not one",
            ),
            (
                "@doc(\"see forbidden.md\")\n@id(\"a\")",
                false,
                "a word that contains `forbid` is not one",
            ),
            (
                "@doc(\"the rule at https://example.invalid/x\")\npermit(principal, action, resource);",
                true,
                "a `//` inside a string does not open a comment",
            ),
        ] {
            assert_eq!(super::decides_something(text), decides, "{why}: {text}");
        }
    }

    use super::{CORE_POLICIES, CORE_RULES, PINNED, check, the_fenced_schema_of};

    /// The schema and the reference are compared in both directions, and the
    /// two spellings of an optional attribute are one declaration.
    ///
    /// **Three operators over this step, and this is the test that kills
    /// them.** With nothing but the live tree to drive the step, all three
    /// survive.
    ///
    /// What each holds: `§4` outranks `policy/schema.cedarschema`, so a
    /// declaration in the reference and not the schema is a transcription that
    /// has fallen behind, and one in the schema and not the reference belongs in
    /// the reference first. And `valid_until: datetime?` and
    /// `valid_until?: datetime` are the same declaration written the two ways
    /// Cedar allows — the reference puts the mark on the type and the schema on
    /// the name, so a comparison that calls that a disagreement reports four
    /// entities and the action block as drifted on a tree where nothing drifted.
    #[test]
    fn the_schema_and_the_reference_disagree_in_two_directions_and_not_over_a_spelling() {
        let set = |declarations: &[&str]| -> std::collections::BTreeSet<String> {
            declarations.iter().map(|it| (*it).to_owned()).collect()
        };

        // Agreement is silence, which is the floor: a comparison that always
        // reported would satisfy both halves below for the wrong reason.
        let same = set(&["entity Realm", "entity User"]);
        let (behind, invented) = super::the_two_ways_they_disagree(&same, &same);
        assert!(
            behind.is_empty() && invented.is_empty(),
            "two identical sets were reported as disagreeing: {behind:?} {invented:?}"
        );

        let reference = set(&["entity Realm", "entity User", "entity Grant"]);
        let schema = set(&["entity Realm", "entity User", "entity Folder"]);
        let (behind, invented) = super::the_two_ways_they_disagree(&reference, &schema);
        assert_eq!(
            behind.iter().map(|it| it.as_str()).collect::<Vec<_>>(),
            ["entity Grant"],
            "a declaration the reference makes and the schema does not was not reported, \
             so the transcription may fall behind the document that outranks it"
        );
        assert_eq!(
            invented.iter().map(|it| it.as_str()).collect::<Vec<_>>(),
            ["entity Folder"],
            "a declaration the schema makes and the reference does not was not reported, \
             so the schema may invent one"
        );

        // And the spelling that is not a disagreement, through the reader that
        // normalises it — the two files genuinely write it differently.
        //
        // **A type made of more than one word, too.** A walk back over
        // `[A-Za-z0-9_]` reads an identifier and not a type, so it
        // normalises exactly the types §4 uses today: `Set<String>?` stops at
        // the `>` and the same declaration in the two files' own house styles
        // comes back as *both* halves of a drift at once — with the remedy
        // telling the author to push the transcription's spelling up into the
        // document that outranks.
        for (on_the_type, on_the_name) in [
            (
                "entity Grant { valid_until: datetime? };",
                "entity Grant { valid_until?: datetime };",
            ),
            (
                "entity Grant { tags: Set<String>? };",
                "entity Grant { tags?: Set<String> };",
            ),
            (
                "entity Grant { seen: Set<Set<String>>? };",
                "entity Grant { seen?: Set<Set<String>> };",
            ),
        ] {
            let one = super::declarations_in(on_the_type);
            let other = super::declarations_in(on_the_name);
            let (behind, invented) = super::the_two_ways_they_disagree(&one, &other);
            assert!(
                behind.is_empty() && invented.is_empty(),
                "`{on_the_type}` and `{on_the_name}` are one declaration in the two \
                 spellings Cedar allows, and this reports drift on a tree where nothing \
                 drifted: {behind:?} {invented:?}"
            );
        }
    }

    /// A rule the pack holds under no written prefix is named.
    ///
    /// **The other direction of that comparison, entered here.** On a clean
    /// tree `unnamed` is empty whatever the filter does, so emptying it leaves
    /// the step byte-identical — and what it catches is a rule arriving under a
    /// prefix nobody remembered. Measured with a filter on two prefixes: a
    /// probe under `policy/packs/org` with an `org.`-prefixed id gives an
    /// `ok … policy(ies) parse and validate` line one higher, and only the
    /// denial-test gate objects, which is satisfied by writing one case.
    #[test]
    fn a_rule_the_pack_holds_and_no_list_names_is_reported() {
        /// The whole of `from` into `to`, files and directories alike.
        fn copy_into(from: &Path, to: &Path) {
            std::fs::create_dir_all(to).expect("writable");
            for entry in std::fs::read_dir(from).expect("readable").flatten() {
                let (source, target) = (entry.path(), to.join(entry.file_name()));
                if source.is_dir() {
                    copy_into(&source, &target);
                } else {
                    std::fs::copy(&source, &target).expect("copyable");
                }
            }
        }

        let guard = crate::check::LeavesNothingBehind::new("scratch");
        let scratch = guard.root.clone();
        let _ = std::fs::remove_dir_all(&scratch);
        // Removed however this test leaves, which the path at the end of the
        // function is not: a failing assertion takes neither.
        copy_into(&root().join("policy"), &scratch.join("policy"));

        // The shipped pack, copied, is the control: it has to agree before the
        // probe means anything.
        let shipped = nc_policy::load(&scratch).expect("the copied pack loads");
        let mut failures = Vec::new();
        super::the_core_rules_are_the_pack(&shipped, &mut failures);
        assert!(
            failures.is_empty(),
            "the shipped pack, copied, was reported: {failures:?}"
        );

        // A rule under a directory that is loaded and a prefix no list names —
        // with the manifest a pack holding a policy needs, and the realm of
        // that profile pinned to it, so the pack loads and this line is what
        // is asked about it.
        std::fs::write(
            scratch.join("policy/packs/org/probe.cedar"),
            "@id(\"org.probe\")\nforbid (principal, action, resource);\n",
        )
        .expect("writable");
        std::fs::write(scratch.join("policy/packs/org/pack.toml"), "version = 1\n")
            .expect("writable");
        let world = std::fs::read_to_string(scratch.join("policy/world.json")).expect("readable");
        assert_eq!(
            world.matches("\"org@0.1.0\"").count(),
            1,
            "the org realm's pin moved"
        );
        std::fs::write(
            scratch.join("policy/world.json"),
            world.replace("\"org@0.1.0\"", "\"org@1\""),
        )
        .expect("writable");
        let widened = nc_policy::load(&scratch).expect("the pack with a probe loads");
        let mut failures = Vec::new();
        super::the_core_rules_are_the_pack(&widened, &mut failures);
        assert!(
            !failures.is_empty(),
            "a rule the pack holds under a prefix nothing names was walked past"
        );
    }

    /// A world where no realm selects a carried pack is a finding, not a pass.
    ///
    /// `nc_policy::load` refuses a realm pinning another version, so a pack
    /// that loaded has every judged pin right — and so does one where nothing
    /// was judged. The shipped world, copied, is the control: its family realm
    /// is held to the family pack. Then that realm is handed a profile whose
    /// pack holds no policy, and a pin of that profile, which leaves every pin
    /// in the world unheld — the pin moves with it, because a realm pinning
    /// another profile's pack is refused before any floor is asked.
    #[test]
    fn a_world_where_no_realm_selects_a_carried_pack_is_a_finding() {
        /// The whole of `from` into `to`, files and directories alike.
        fn copy_into(from: &Path, to: &Path) {
            std::fs::create_dir_all(to).expect("writable");
            for entry in std::fs::read_dir(from).expect("readable").flatten() {
                let (source, target) = (entry.path(), to.join(entry.file_name()));
                if source.is_dir() {
                    copy_into(&source, &target);
                } else {
                    std::fs::copy(&source, &target).expect("copyable");
                }
            }
        }

        let guard = crate::check::LeavesNothingBehind::new("unheld-pins");
        let scratch = guard.root.clone();
        let _ = std::fs::remove_dir_all(&scratch);
        copy_into(&root().join("policy"), &scratch.join("policy"));

        let shipped = nc_policy::load(&scratch).expect("the copied pack loads");
        let mut failures = Vec::new();
        super::the_realms_pin_the_packs_they_select(
            shipped.pins(),
            shipped.versions(),
            &mut failures,
        );
        assert!(
            failures.is_empty(),
            "the shipped world, copied, was reported: {failures:?}"
        );

        let world = std::fs::read_to_string(scratch.join("policy/world.json")).expect("readable");
        for (written, what) in [
            ("\"profile\": \"family\"", "the world's family realm"),
            ("\"policy_pack\": \"family@1\"", "its pin"),
        ] {
            assert_eq!(
                world.matches(written).count(),
                1,
                "{what} moved, so this edit would prove nothing"
            );
        }
        std::fs::write(
            scratch.join("policy/world.json"),
            world
                .replace("\"profile\": \"family\"", "\"profile\": \"collective\"")
                .replace(
                    "\"policy_pack\": \"family@1\"",
                    "\"policy_pack\": \"collective@1\"",
                ),
        )
        .expect("writable");
        let unheld = nc_policy::load(&scratch).expect("a world pinning no carried pack loads");
        let mut failures = Vec::new();
        super::the_realms_pin_the_packs_they_select(
            unheld.pins(),
            unheld.versions(),
            &mut failures,
        );
        assert_eq!(failures, ["no realm's pin was held to a pack"]);
    }

    /// The realms are counted against the packs that hold them, both numbers.
    ///
    /// The shipped world holds one realm to the one pack that declares a
    /// version, which is the line the step prints: `1 realm(s) … held to 1 pack
    /// manifest(s)`. Beside it, two realms of one pack are one pack, and a pack
    /// declaring a version no realm pins is not counted as holding one.
    #[test]
    fn the_realms_are_counted_against_the_packs_that_hold_them() {
        let shipped = nc_policy::load(root()).expect("the shipped pack loads");
        assert_eq!(
            super::the_packs_the_pins_are_held_to(shipped.pins(), shipped.versions()),
            (1, 1, Vec::new())
        );

        let owned = |pairs: &[(&str, &str)]| -> std::collections::BTreeMap<String, String> {
            pairs
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect()
        };
        let versions: std::collections::BTreeMap<String, u32> =
            [("family".to_owned(), 1), ("org".to_owned(), 3)].into();
        assert_eq!(
            super::the_packs_the_pins_are_held_to(
                &owned(&[("home", "family@1"), ("cottage", "family@1")]),
                &versions
            ),
            (2, 1, Vec::new())
        );
    }

    /// A pin no declared version builds is a finding that names its realm.
    ///
    /// `nc_policy::load` never returns such a realm as held, so no pack it
    /// loads reaches this branch and the maps are written by hand: a pin of
    /// another version, and one whose pack declares nothing.
    #[test]
    fn a_pin_no_declared_version_builds_is_a_finding() {
        let pins: std::collections::BTreeMap<String, String> = [
            ("home".to_owned(), "family@2".to_owned()),
            ("office".to_owned(), "org@1".to_owned()),
        ]
        .into();
        let versions: std::collections::BTreeMap<String, u32> = [("family".to_owned(), 1)].into();
        let (realms, packs, unheld) = super::the_packs_the_pins_are_held_to(&pins, &versions);
        assert_eq!((realms, packs), (2, 0));
        assert_eq!(unheld, [("home", "family@2"), ("office", "org@1")]);

        let mut failures = Vec::new();
        super::the_realms_pin_the_packs_they_select(&pins, &versions, &mut failures);
        assert_eq!(
            failures,
            ["a realm held to a pack pins no version a pack declares"]
        );
    }

    /// A pack that pinned nothing is one sentence, and a moved count is named.
    ///
    /// **The skip lives inside the function, and this is what enters it.**
    /// Written `if total != 0` above the call, at a call site no test enters,
    /// switching it off would stop `PINNED_BY_POLICY`'s rows being compared at
    /// all — leaving only the total, which is the compensating move that
    /// comparison exists to stop.
    #[test]
    fn a_pack_that_pinned_nothing_is_not_fifteen_missing_rows() {
        let written: std::collections::BTreeMap<String, usize> = [
            ("core.fail_closed".to_owned(), 2),
            ("family.read".to_owned(), 3),
        ]
        .into_iter()
        .collect();

        // Agreement says nothing.
        let mut failures = Vec::new();
        super::assert_the_same_policies_are_pinned(&written, &written, &mut failures);
        assert!(failures.is_empty(), "agreement was reported: {failures:?}");

        // A count that moved is named, and named by the policy it moved on.
        let moved: std::collections::BTreeMap<String, usize> = [
            ("core.fail_closed".to_owned(), 1),
            ("family.read".to_owned(), 4),
        ]
        .into_iter()
        .collect();
        let mut failures = Vec::new();
        super::assert_the_same_policies_are_pinned(&moved, &written, &mut failures);
        assert_eq!(
            failures.len(),
            1,
            "two policies moved in opposite directions and this said {failures:?}"
        );

        // **And a pack that pinned nothing at all is silent here.** The step
        // above reports a pack that did not load; repeating it as one row per
        // written policy buries the sentence a reader needs.
        let nothing: std::collections::BTreeMap<String, usize> = [
            ("core.fail_closed".to_owned(), 0),
            ("family.read".to_owned(), 0),
        ]
        .into_iter()
        .collect();
        let mut failures = Vec::new();
        super::assert_the_same_policies_are_pinned(&nothing, &written, &mut failures);
        assert!(
            failures.is_empty(),
            "a pack that pinned nothing was reported as every policy moving: {failures:?}"
        );
    }

    /// A command whose resource its own action cannot be applied to.
    ///
    /// **The comparison, held by a test.** Its three functions appear nowhere
    /// but at one call site each, and the two sentences they build are pushed
    /// into a `Vec<String>` rather than said through `ui::`, so the console
    /// census cannot see them either. The branch narrowed so it can never fire
    /// leaves the whole policy section byte-identical to a clean run, and fails
    /// this test.
    ///
    /// Within the step it is also the only guard against the drift it exists
    /// for. Take `Realm` out of the resource list in both
    /// `policy/schema.cedarschema` and the §4 table that outranks it — the
    /// coherent edit — and every other line of the step still says `ok`: the §4
    /// comparison agrees because both files moved together, the action
    /// comparison agrees, the pack validates, every case decides as written.
    /// Only this branch objects.
    ///
    /// The control is first, for the reason its neighbour states: a comparison
    /// that always reports would satisfy the second half for the wrong reason.
    #[test]
    fn a_command_whose_resource_its_action_cannot_reach_is_named() {
        // What the schema ships, as the parsed schema answers it.
        let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        let pack = nc_policy::load(root).expect("the shipped pack loads");
        let shipped: Vec<super::Command> = nc_catalog::commands::COMMANDS
            .iter()
            .map(|command| {
                (
                    command.name,
                    command.action,
                    command.resource_selector,
                    command.args_schema,
                )
            })
            .collect();
        let (real, unread) = super::the_commands_a_schema_cannot_apply(&shipped, &|action| {
            pack.resources_for(action)
        });
        assert_eq!(
            unread, 0,
            "a shipped command's resource was not compared, and every one names its \
             type outright"
        );
        assert!(
            real.is_empty(),
            "the shipped commands were reported against the shipped schema: {real:?}"
        );

        // The drift: each type a shipped command selects — the realm, a
        // feature — gone from every action's resource list in turn, which is
        // what removing it from §4 and the schema together produces. Exactly
        // the commands selecting it are named, each with the type, and the
        // others are not.
        for (gone, selecting) in [("Realm", "realm"), ("Feature", "feature:")] {
            let (without, _) = super::the_commands_a_schema_cannot_apply(&shipped, &|action| {
                pack.resources_for(action)
                    .into_iter()
                    .filter(|named| named != gone)
                    .collect()
            });
            let expected: Vec<&nc_catalog::commands::Command> = nc_catalog::commands::COMMANDS
                .iter()
                .filter(|command| command.resource_selector.starts_with(selecting))
                .collect();
            assert!(
                !expected.is_empty(),
                "no shipped command selects a `{gone}`, so this drift is about nothing"
            );
            assert_eq!(
                without.len(),
                expected.len(),
                "with `{gone}` out of the schema each command selecting it builds a request \
                 Cedar's own schema refuses — {} of {} were named: {without:?}",
                without.len(),
                expected.len()
            );
            for (command, said) in expected.iter().zip(&without) {
                assert!(
                    said.contains(command.name) && said.contains(gone),
                    "the sentence does not name the command and the type it selects: {said}"
                );
            }
        }

        // **An action the schema does not declare is somebody else's finding.**
        // Reported here as well, it would name one defect as two — which is the
        // shape the neighbour below is written about.
        let (none, _) = super::the_commands_a_schema_cannot_apply(&shipped, &|_| Vec::new());
        assert!(
            none.is_empty(),
            "a command whose action the schema does not declare was reported twice: {none:?}"
        );

        // **The mapping, field by field.** A guard on its length is satisfied
        // by any `.map`, so under it a selector replaced by a constant
        // different from the shipped one leaves the whole run byte-identical —
        // and with the catalog holding two selectors, `realm` and
        // `feature:naming`, no one constant can stand in for both.
        let mapped = super::the_catalog_as_commands();
        assert_eq!(
            mapped.len(),
            nc_catalog::commands::COMMANDS.len(),
            "the mapping handed over {} of the catalog's {} command(s)",
            mapped.len(),
            nc_catalog::commands::COMMANDS.len()
        );
        for (mapped, command) in mapped.iter().zip(nc_catalog::commands::COMMANDS) {
            assert_eq!(
                *mapped,
                (
                    command.name,
                    command.action,
                    command.resource_selector,
                    command.args_schema
                ),
                "`{}` reached the comparison as {mapped:?}, which is not what the catalog \
                 says it is",
                command.name
            );
        }

        // **And the shipped caller itself, entered here.** The mapping that
        // hands the catalog's commands over lives at one call site, and every
        // case above builds its own copy — so `.iter().take(1)` there would
        // leave a step printing only the catalog's own length saying
        // `ok … command(s) name an action the schema declares` over one command
        // compared. It fails the assertion below.
        let (_, _, compared) = super::the_commands_the_schema_cannot_apply(&pack);
        assert_eq!(
            compared,
            nc_catalog::commands::COMMANDS.len(),
            "the shipped mapping handed {compared} of the catalog's {} command(s) to the \
             comparison",
            nc_catalog::commands::COMMANDS.len()
        );
    }

    /// An argument selector is compared through the type its schema names.
    ///
    /// Split from [`a_command_whose_resource_its_action_cannot_reach_is_named`]
    /// to keep that test under clippy's line limit: that test holds
    /// the comparison against the shipped catalog, and this one holds the
    /// reader of the one form the shipped catalog does not use.
    #[test]
    fn an_argument_selector_is_compared_through_the_type_its_schema_names() {
        let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        let pack = nc_policy::load(root).expect("the shipped pack loads");
        let shipped = super::the_catalog_as_commands();
        // **A selector that extracts from the arguments is read through its
        // schema, and compared like `realm` is.** `x-entity` names the type an
        // argument selects, so an argument naming a type its action
        // applies to is fine and one naming a type it does not is named — at
        // build time, rather than denied by the decision point on the first
        // dispatch.
        let (content, could_not_read) = super::the_commands_a_schema_cannot_apply(
            &[("note.get", "Read", "argument:note", CONTENT)],
            &|action| pack.resources_for(action),
        );
        assert!(
            content.is_empty(),
            "an argument naming a `Content` was reported for a `Read`: {content:?}"
        );
        assert_eq!(
            could_not_read, 0,
            "a selector its schema backs was counted as unread rather than compared"
        );
        let (group, _) = super::the_commands_a_schema_cannot_apply(
            &[("note.get", "Read", "argument:note", GROUP)],
            &|action| pack.resources_for(action),
        );
        assert!(
            group.len() == 1 && group[0].contains("note.get") && group[0].contains("Group"),
            "an argument naming a type no action applies to was not named: {group:?}"
        );

        // **And a selector its own schema does not back is counted, not
        // read.** The compiler refuses one, so a compiled catalog holds none —
        // and if a stale one ever reached this step, the count is what says so.
        let unbacked: Vec<super::Command> = shipped
            .iter()
            .map(|(name, action, _, schema)| (*name, *action, "argument:from", *schema))
            .collect();
        let (passed_over, could_not_read) =
            super::the_commands_a_schema_cannot_apply(&unbacked, &|action| {
                pack.resources_for(action)
            });
        assert!(
            passed_over.is_empty(),
            "a selector this could not read was reported as a type: {passed_over:?}"
        );
        assert_eq!(
            could_not_read,
            unbacked.len(),
            "a resource this comparison could not read was passed over in silence, so \
             the step covers less of the catalog than its own line claims"
        );

        // And a selector this cannot read is not read as a type.
        let unreadable = super::the_type_a_selector_names("not a selector", CONTENT);
        assert!(
            unreadable.is_none(),
            "a selector holding a space was read as an entity type: {unreadable:?}"
        );
        assert_eq!(
            super::the_type_a_selector_names("realm", CONTENT).as_deref(),
            Some("Realm"),
            "the realm selector is the `Realm` entity"
        );
        assert_eq!(
            super::the_type_a_selector_names("argument:note", GROUP).as_deref(),
            Some("Group"),
            "an argument selector is the type its property's `x-entity` names"
        );
    }

    /// An argument schema whose `note` identifies a `Content`.
    const CONTENT: &str = r#"{"type":"object","additionalProperties":false,"properties":{"note":{"type":"string","x-entity":"Content","maxLength":128}},"required":["note"]}"#;

    /// The same schema, with `note` identifying a `Group`, which no action applies to.
    const GROUP: &str = r#"{"type":"object","additionalProperties":false,"properties":{"note":{"type":"string","x-entity":"Group","maxLength":128}},"required":["note"]}"#;

    /// A command naming an action the schema does not declare is reported.
    ///
    /// **The refusal branch, and this is the test that reaches it.** In a clean
    /// tree the answer is empty whether the branch is there or not, so
    /// replacing it with `if false` changes nothing a run prints and fails this
    /// test — and what it catches is what this gate exists for:
    /// `action = "Reed"`, which without it *"compiles, generates, documents
    /// itself in `docs/commands.md` and passes the whole gate"*.
    ///
    /// Driven against the *real* catalog with a schema that is not the real one,
    /// because the commands are a static and the schema is what varies in the
    /// failure this is about.
    #[test]
    fn a_command_whose_action_the_schema_lacks_is_named() {
        // What the tree ships: every command's action is declared, so nothing is
        // wrong. This is the floor — a comparison that always reports would pass
        // the assertion below for the wrong reason.
        let real: Vec<String> = nc_catalog::commands::COMMANDS
            .iter()
            .map(|command| (*command.action).to_owned())
            .collect();
        let left_over = super::the_commands_the_schema_does_not_declare(&real);
        assert!(
            left_over.is_empty(),
            "the catalog's own actions were reported as undeclared: {left_over:?}"
        );

        // A schema that declares the actions with one letter moved — the shape
        // this gate exists for.
        let misspelt: Vec<String> = real
            .iter()
            .map(|action| action.replace('e', "ee"))
            .collect();
        let wrong = super::the_commands_the_schema_does_not_declare(&misspelt);
        assert_eq!(
            wrong.len(),
            nc_catalog::commands::COMMANDS.len(),
            "a schema declaring none of the catalog's actions reported {} of {}",
            wrong.len(),
            nc_catalog::commands::COMMANDS.len()
        );
        for (command, named) in nc_catalog::commands::COMMANDS.iter().zip(&wrong) {
            assert_eq!(
                named.name, command.name,
                "the comparison answered about a command other than the one it read"
            );
        }

        // And one action dropped from an otherwise complete schema: exactly the
        // commands declaring it are named, which is what says the comparison
        // is per command rather than all-or-nothing.
        let dropped = real[1].clone();
        let missing: Vec<String> = real
            .iter()
            .filter(|action| **action != dropped)
            .cloned()
            .collect();
        let named: Vec<&str> = super::the_commands_the_schema_does_not_declare(&missing)
            .iter()
            .map(|command| command.name)
            .collect();
        let declaring: Vec<&str> = nc_catalog::commands::COMMANDS
            .iter()
            .filter(|command| command.action == dropped)
            .map(|command| command.name)
            .collect();
        assert!(
            declaring.len() < nc_catalog::commands::COMMANDS.len(),
            "every command declares `{dropped}`, so this cannot tell one from all"
        );
        assert_eq!(named, declaring, "`{dropped}` dropped");
    }

    /// Which fences of §4 are the schema, and which are examples beside it.
    ///
    /// **This function's own test.** The live document fences exactly once, so
    /// every rule about *which* fences it reads would be decided by a file that
    /// cannot express the question. Two readings get it wrong, and this drives
    /// both: reading only the first fence, so a declaration in a second one is
    /// compared against nothing; and reading every fence whatever language it
    /// names, so an illustrative
    /// ```sql block is parsed as Cedar and the gate refuses with a sentence
    /// stating the inverse of what happened.
    #[test]
    fn the_schema_is_every_cedar_fence_of_its_own_section() {
        let section = |body: &str| {
            format!(
                "# A document\n\n## 4. Cedar schema\n\n{body}\n## 5. Next\n\n```\nentity Elsewhere;\n```\n"
            )
        };

        // Every Cedar fence, bare or named, and the prose between them is not one.
        let two = section(
            "```\nentity User;\n```\n\nA session is an entity too:\n\n```cedar\nentity Session;\n```\n",
        );
        let read = the_fenced_schema_of(&two).expect("§4 fences a schema");
        assert!(read.contains("entity User;"), "{read:?}");
        assert!(read.contains("entity Session;"), "{read:?}");
        assert!(
            !read.contains("A session is an entity too"),
            "prose between two fences was read as a declaration: {read:?}"
        );

        // **The dialect tag is Cedar, not another language.** Taking `cedar`
        // and nothing else is the mirror of the defect above — too strong, and
        // green only while nobody writes the likelier tag. Measured with `cedar`
        // alone: tagging §4's one fence `cedarschema` makes the gate answer that
        // the section has no fenced block at all.
        let dialect = section("```cedarschema\nentity User;\n```\n");
        let read = the_fenced_schema_of(&dialect)
            .expect("a fence tagged with Cedar's own schema dialect is Cedar");
        assert!(read.contains("entity User;"), "{read:?}");
        let shouted = section("```CedarSchema\nentity User;\n```\n");
        assert!(
            the_fenced_schema_of(&shouted).is_some_and(|read| read.contains("entity User;")),
            "the tag is a language, not a spelling"
        );

        // A fence naming another language is an example, not a declaration.
        let illustrated = section(
            "```\nentity User;\n```\n\nRelations live in PostgreSQL:\n\n```sql\nCREATE TABLE grants (id text);\n```\n",
        );
        let read = the_fenced_schema_of(&illustrated).expect("§4 fences a schema");
        assert!(read.contains("entity User;"), "{read:?}");
        assert!(
            !read.contains("CREATE TABLE"),
            "an illustrative `sql` fence was read as Cedar, and the gate then tells the \
             author their schema has fallen behind: {read:?}"
        );

        // The section ends at the next heading.
        let read =
            the_fenced_schema_of(&section("```\nentity User;\n```\n")).expect("§4 fences a schema");
        assert!(
            !read.contains("entity Elsewhere"),
            "a fence from the section below was read as §4's: {read:?}"
        );

        // A section with no fence at all is nothing, not an empty schema.
        assert!(
            the_fenced_schema_of(&section("Nothing is declared here.\n")).is_none(),
            "a section with no fence answered a schema"
        );
    }
    use std::path::Path;

    fn root() -> &'static Path {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
    }

    /// The pack this stage implements is not empty.
    ///
    /// **`CORE_POLICIES` is compared with its directory both ways, and this is
    /// the floor for `policy/packs/`.** Deleting `family`'s policy file together
    /// with its case file shrinks the policy count, the case count and the
    /// denial-test count *in step*, so a count alone reads `ok` over it — the
    /// same shape the core list exists to refuse, one directory along.
    #[test]
    fn the_pack_this_stage_implements_is_not_empty() {
        let scratch = std::env::temp_dir().join(format!("nc-emptypack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        let family = scratch.join("policy/packs/family");
        std::fs::create_dir_all(&family).expect("writable");

        // A pack with a README and no policy is what `org` and `collective`
        // deliberately are — and what `family` must not be.
        std::fs::write(family.join("README.md"), "# family\n").expect("writable");
        assert!(super::pack_decides_nothing(&scratch));

        // A policy file that decides nothing is the same absence, one level in.
        //
        // **The word in a comment, because `// nothing at all` alone decides
        // nothing.** `decides_something` spends comments before it looks for
        // `permit` or `forbid`, and with nothing but those words in the file the
        // spending is never the thing that decides — measured, with such
        // fixtures alone the whole comment-stripping half deleted leaves both
        // tests passing. A file
        // whose only `forbid` is in a sentence about forbidding decides nothing,
        // and that is exactly the file a person writes while drafting one.
        for empty in [
            "// nothing at all\n",
            "// this is where a forbid will go\n",
            "// permit nothing yet; see the README\n",
            "// forbid\n// permit\n",
        ] {
            std::fs::write(family.join("permissions.cedar"), empty).expect("writable");
            assert!(
                super::pack_decides_nothing(&scratch),
                "`{empty}` decides nothing and was read as a policy"
            );
        }

        std::fs::write(
            family.join("permissions.cedar"),
            "permit (principal, action, resource);\n",
        )
        .expect("writable");
        assert!(!super::pack_decides_nothing(&scratch));

        // And the tree as it ships, or the assertions above are about a shape
        // this repository is not in.
        assert!(!super::pack_decides_nothing(root()));
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn a_core_policy_that_decides_nothing_is_a_finding() {
        // **The rule's own test.** The rule replaced by `is_file()` fails it;
        // so replaced, `grants_expire.cedar` reduced to a comment with its cases
        // removed gives `ok 7 core policy(ies) … by name` while three
        // neighbouring numbers — the policies, the cases and the denial tests —
        // quietly shrink, measured.
        //
        // Driven through `check` rather than through a helper, for the reason
        // `every_gate_this_step_owns_is_wired_to_its_call_site` gives: a rule
        // and its call site are two things, and a helper's test exercises only
        // the first.
        let scratch = std::env::temp_dir().join(format!("nc-corepol-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        let core = scratch.join("policy/core");
        std::fs::create_dir_all(&core).expect("writable");
        for name in CORE_POLICIES {
            std::fs::write(core.join(name), "// nothing at all\n").expect("writable");
        }
        // The family pack is a separate rule with its own message; this scratch
        // carries one so the complaint under test is the only one about `core`.
        let family = scratch.join("policy/packs/family");
        std::fs::create_dir_all(&family).expect("writable");
        std::fs::write(
            family.join("permissions.cedar"),
            "permit (principal, action, resource);\n",
        )
        .expect("writable");
        let mut failures = Vec::new();
        check(&scratch, &mut failures);
        assert!(
            failures
                .iter()
                .any(|failure| failure.contains("core policy(ies) missing")),
            "policy files that decide nothing passed: {failures:?}"
        );

        // **And the other direction, entered here.** The list is compared with
        // the directory *both* ways, and on a clean tree `unnamed` is empty
        // whatever the loop does, so emptying it leaves the step
        // byte-identical. A policy file that arrives
        // in `policy/core/` and is not in `CORE_POLICIES` is one this step walks
        // past — and walking past a core invariant is the thing the list exists
        // to make impossible.
        std::fs::write(
            core.join("probe_that_nobody_named.cedar"),
            "@id(\"core.probe\")\nforbid (principal, action, resource);\n",
        )
        .expect("writable");
        let walked_past = super::core_policies_that_decide_nothing(&scratch);
        assert!(
            walked_past
                .iter()
                .any(|said| said.contains("probe_that_nobody_named.cedar")
                    && said.contains("does not name it")),
            "a policy in the core directory that no list names was walked past: \
             {walked_past:?}"
        );
        std::fs::remove_file(core.join("probe_that_nobody_named.cedar")).expect("removable");

        // And with one real statement in each, that particular complaint stops —
        // the others remain, because a scratch tree has no schema or cases, and
        // this test is about the one rule.
        for name in CORE_POLICIES {
            std::fs::write(
                core.join(name),
                "@id(\"core.probe\")\nforbid (principal, action, resource);\n",
            )
            .expect("writable");
        }
        let mut failures = Vec::new();
        check(&scratch, &mut failures);
        assert!(
            !failures
                .iter()
                .any(|failure| failure.contains("core policy(ies) missing")),
            "policy files that decide something were reported as missing: {failures:?}"
        );
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// A policy whose denial test is gone is a gap, and the gap fails the run.
    ///
    /// **Criterion 11's only failure branch, and this is the test that reaches
    /// it.** Every other test in this module drives the *other* steps — a core
    /// policy that decides nothing, an empty pack, the written numbers — so
    /// `} else if !gaps.is_empty()` written `} else if false` is this test's to
    /// catch. With a real gap behind it the difference is the whole of
    /// criterion 11. Unpatched, the run names the policy that
    /// has no denial test and fails with `1 gap(s) in the denial tests`;
    /// patched, both lines are gone and `check passed` stands over a core
    /// invariant with nothing proving it can refuse anything.
    ///
    /// Driven through `denial_tests` against a real pack rather than through a
    /// synthetic one, because what the arm reports is `coverage`'s answer about
    /// policies that exist — and a gap is made by *removing* a denial case,
    /// which needs a pack that has one.
    #[test]
    fn a_policy_whose_denial_test_is_gone_is_a_gap() {
        let scratch = std::env::temp_dir().join(format!("nc-nodenial-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        let mut stack = vec![root().join("policy")];
        while let Some(directory) = stack.pop() {
            std::fs::create_dir_all(
                scratch.join(directory.strip_prefix(root()).expect("under the root")),
            )
            .expect("writable");
            for entry in std::fs::read_dir(&directory).expect("readable").flatten() {
                let from = entry.path();
                if from.is_dir() {
                    stack.push(from);
                    continue;
                }
                let to = scratch.join(from.strip_prefix(root()).expect("under the root"));
                std::fs::copy(&from, &to).expect("writable");
            }
        }

        let judged = |at: &Path| -> Vec<String> {
            let pack = nc_policy::load(at).expect("the copied pack loads");
            let (cases, _) = nc_policy::cases::load(at).expect("the copied cases load");
            let (problems, _, pinned_by_policy) = nc_policy::run(&pack, &cases);
            let pinned: usize = pinned_by_policy.values().sum();
            let mut failures = Vec::new();
            super::denial_tests(&pack, &cases, &problems, pinned, &mut failures);
            failures
        };

        // The control, and it is what makes the assertion below mean anything:
        // the arm reports a *count*, and a copy that was already failing would
        // produce one without a case having been removed.
        let clean = judged(&scratch);
        assert!(
            clean.is_empty(),
            "the copied pack does not pass the denial-test step as it stands, so \
             nothing below is about the removed case: {clean:?}"
        );

        // The depth limit's denial case, deleted — which is `§14.6`'s third rule
        // left with nothing showing it refuses anything.
        let file = scratch.join("policy/core/tests/chain_depth.toml");
        let text = std::fs::read_to_string(&file).expect("the copied case file is readable");
        let at = text.rfind("[[case]]").expect("the file declares cases");
        assert!(
            text[at..].contains("core.chain_depth_is_bounded"),
            "the last case in chain_depth.toml is no longer the depth limit's denial \
             test, so deleting it is not the gap this test makes"
        );
        std::fs::write(&file, &text[..at]).expect("writable");

        let found = judged(&scratch);
        let names_the_gap = |failures: &[String]| {
            failures.iter().any(|said| {
                said.contains("gap in the denial")
                    || said.contains("gaps in the denial")
                    || said.contains("gap(s) in the denial tests")
            })
        };
        assert!(
            names_the_gap(&found),
            "a core policy was left with no denial test and the step reported nothing. \
             §14.9's seventh invariant is the one line of this gate that says a rule \
             which cannot be shown to refuse anything does not enter the monorepo: \
             {found:?}"
        );

        // **And through the step, because the cases above cannot hold the wire
        // between them.** Everything above hands `denial_tests` a vector of its
        // own, so it cannot see what [`check`] does with the one it owns:
        // `denial_tests(&pack, &cases, &problems, pinned, failures)` written
        // `denial_tests(…, &mut Vec::new())` leaves the gap printed and the run
        // that printed it succeeding — measured, with every case above
        // passing — and fails this.
        // Criterion 11's only failure reaches the verdict through that
        // argument, and `check.rs`'s
        // `every_gate_this_step_owns_is_wired_to_its_call_site` does exactly
        // this for three other gates.
        let mut through_the_step = Vec::new();
        check(&scratch, &mut through_the_step);
        assert!(
            names_the_gap(&through_the_step),
            "the gap was reported and the step did not fail for it, so criterion 11's \
             one failure branch reaches the verdict through nothing: {through_the_step:?}"
        );

        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn the_written_list_is_the_files_on_disk() {
        // **Both directions**, or the list is a claim about a tree that no
        // longer exists. A file on disk the list does not name would never be
        // missed if it were deleted; a name here for a file that is gone is a
        // permanent failure nobody can fix by writing a policy.
        //
        // `§14.9` says seven, and this is the one place in the step where that
        // number does not come from counting the directory it is checking.
        let mut on_disk: Vec<String> = std::fs::read_dir(root().join("policy/core"))
            .expect("the core policies are there")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| {
                std::path::Path::new(name)
                    .extension()
                    .is_some_and(|k| k == "cedar")
            })
            .collect();
        on_disk.sort();
        let mut declared: Vec<String> = CORE_POLICIES
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
        declared.sort();
        assert_eq!(
            declared, on_disk,
            "CORE_POLICIES and policy/core disagree about which policies exist"
        );
        // **No assertion that the number is `§14.9`'s.** "§14.9 names seven core
        // invariants" is true of `§14.9` and not of this directory: two of
        // those seven are deliberately not policies, and three of `§14.6`'s
        // rules are. The two sevens agree by coincidence, and an assertion that
        // agrees by coincidence is one that will one day disagree for the wrong
        // reason.
        // What is load-bearing is the both-directions comparison above.
        assert!(
            !CORE_POLICIES.is_empty(),
            "the core pack is empty, so the floor below it can never fire"
        );

        // **And there are seven of them, which is a fourth place a deletion
        // cannot reach.** The both-directions comparison above keeps the list
        // and the directory honest with each other, and a core policy, its case
        // file *and* its line here deleted in one edit leave three places that
        // agree; this count is a fourth that has to move with them.
        //
        // **What this number is *not* is a correspondence with the reference.**
        // A message saying `§9 names seven core invariants, and this list holds
        // a file for each` would assert a correspondence that does not hold in
        // either direction: two of §9's seven (re-evaluation, and the
        // denial-test gate) are deliberately not policies and have no file, and
        // two files carry `§14.6` rules that appear nowhere in §9. Citing `§9`
        // instead of `§14.9` changes the document named and leaves the
        // coincidence where it is. A message that asserts a mapping a reader
        // cannot verify is worse than a message that asserts a floor, because
        // it sends them to check the wrong thing: a legitimate file split then
        // reads as a reference violation.
        assert_eq!(
            CORE_POLICIES.len(),
            7,
            "this list holds seven files today, and a deletion agreed in three places must \
             not be a one-place edit. It is a floor on the list, not a correspondence with \
             any section: `policy/core/README.md` is where the mapping between files and \
             invariants is written, and it is not one-to-one"
        );
        // **And the rules, which need a number of their own.** The paragraph
        // above says a file is not a rule and
        // three of the seven hold more than one — so the file floor cannot see
        // a *rule* deleted from a file that still exists.
        // With `core.grant_not_expired` deleted from its policy, its line from
        // `CORE_RULES`, and its two cases from the case file, every count moves
        // down together:
        //
        // ```text
        // ok  7 core policy file(s) are on disk and decide something…
        // ok  13 policy(ies) parse and validate, 9 of them the core rules by name
        // ok  32 case(s) in 8 file(s) decide as written…
        // check passed
        // ```
        //
        // Putting only the case back shows what has gone: *a grant whose end has
        // passed is refused — expected deny and Cedar answered Allow*. Every
        // expired grant honoured, `§14.9`'s third invariant gone, four places
        // agreeing. This is the fifth, and a deletion cannot reach it either.
        // And the pack's, for the same reason: a rule with one denial case and no
        // written name goes with that case and nothing notices.
        assert_eq!(
            super::PACK_RULES.len(),
            11,
            "the family pack ships eleven rules; one deleted with its only denial case \
             moved every count down together and left the run green"
        );
        assert_eq!(
            super::CORE_RULES.len(),
            11,
            "the seven core invariants are written as eleven rules — `grants_expire` \
             holds three of them, one per end of a grant's window plus the rule that \
             there be a window — and a rule deleted from a file that still exists \
             moves every other count down with it"
        );
    }

    /// `CORE_POLICIES`' doc names the numbers the tree holds.
    ///
    /// **That paragraph states counts the tree can contradict**: `CORE_RULES`
    /// beside it is `[&str; 11]` and `grants_expire.cedar` holds three. It is
    /// the paragraph a reader consults to decide whether the core is complete,
    /// and the sibling constant in this same file is held by
    /// `the_written_number_is_the_number` for exactly this reason, in the same
    /// shape.
    #[test]
    fn the_written_numbers_are_the_numbers() {
        const SPELLED: [&str; 13] = [
            "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
            "eleven", "twelve",
        ];
        let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/policy.rs"))
            .expect("this file is readable");
        let doc = source
            .split_once("const CORE_POLICIES")
            .expect("`CORE_POLICIES` is declared here")
            .0;
        let rules = SPELLED
            .get(CORE_RULES.len())
            .expect("this many rules has a spelling here");
        let files = SPELLED
            .get(CORE_POLICIES.len())
            .expect("this many files has a spelling here");
        let said = format!("**{rules} rules in {files} files**");
        assert!(
            doc.contains(&said),
            "`CORE_POLICIES`' doc does not say `{said}`. It is the paragraph a reader \
             consults to decide whether the core is complete, and it has been wrong twice"
        );

        // And the file that holds three says three, counted from disk. `sharing`
        // is counted in the same pass for the whole-file half below.
        let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        let mut most = ("", 0_usize);
        let mut sharing = 0_usize;
        for named in CORE_POLICIES {
            let text = std::fs::read_to_string(root.join("policy/core").join(named))
                .unwrap_or_else(|error| panic!("{named} is readable: {error}"));
            // **An annotation, not the words anywhere in the file.** Counting
            // every `@id(` in the text makes a comment naming a policy's own id
            // a second rule: measured, two comment lines added under
            // `no_self_escalation.cedar`'s header —
            // *"every journal entry this rule writes records
            // `@id(\"core.no_self_escalation\")`"* — and the test reports
            // `4 of the core files hold more than one rule` against a prose
            // number of three, on a file whose rules have not changed. `§14.9`
            // is precisely the rule that the journal records which policy
            // decided, so that sentence is one these files will keep writing.
            // An annotation begins its line; prose does not.
            let held = text
                .match_indices("@id(")
                .filter(|(at, _)| {
                    text[..*at]
                        .rsplit_once('\n')
                        .is_none_or(|(_, last)| last.trim().is_empty())
                })
                .count();
            if held > 1 {
                sharing += 1;
            }
            if held > most.1 {
                most = (named, held);
            }
        }
        let spelled = SPELLED
            .get(most.1)
            .expect("this many rules in one file has a spelling here");
        assert!(
            doc.contains(&format!("`{}` carries {spelled}", most.0)),
            "`{}` carries {} rule(s) and the doc does not say so",
            most.0,
            most.1
        );

        // **And the rest of the file, not only what sits above the constant.**
        // A read of `source.split_once` up to the constant — the module header
        // and the paragraph above `CORE_POLICIES` — leaves every copy of the
        // distribution sentence below that split unjudged, and one of them is
        // the `ui::detail` a developer reads on the terminal when this very
        // rule fires. A copy there saying that three files hold two rules each
        // is 3 × 2 + 4 × 1 = 10 rules, against `CORE_RULES: [&str; 11]`, handed
        // to somebody repairing the pack. Measured with such a read: a copy
        // below the split edited to say five of the files hold four —
        // twenty-two rules — and `cargo test -p xtask --bins policy::` passes.
        //
        // **Widening the two `contains` above instead would make them
        // weaker.** A substring found anywhere satisfies a `contains`, so a
        // rotted paragraph beside the constant would be excused by a right
        // sentence four hundred lines away. The whole file gets the opposite
        // shape: every occurrence is judged, and a distribution written as one
        // number per file is refused outright — because the files do not hold
        // the same number, and that is the shape such a sentence goes wrong in.
        //
        // Every occurrence is judged rather than the ones that look like claims.
        // The copies that exist are the same sentence, and one that means
        // something else can name `policy/core`'s files without counting them.
        let shared = SPELLED
            .get(sharing)
            .expect("this many sharing files has a spelling here");
        let phrase = format!("of the {files}");
        let mut judged = 0_usize;
        for (at, _) in source.match_indices(&phrase) {
            let counted = source[..at]
                .trim_end()
                .rsplit(|character: char| !character.is_alphanumeric())
                .next()
                .unwrap_or_default();
            assert_eq!(
                counted, *shared,
                "`{counted} {phrase}` — {sharing} of the core files hold more \
                 than one rule, counted from `policy/core`"
            );
            let rest = &source[at + phrase.len()..];
            let rest = rest.strip_prefix(" core files").unwrap_or(rest);
            assert!(
                rest.starts_with(" hold more than one") || rest.starts_with(" holds more than one"),
                "`{phrase}` is followed by `{}`. The core files do not hold the same \
                 number of rules, so a sentence giving one number for each of them is \
                 arithmetic that disagrees with `CORE_RULES`: say how many hold more \
                 than one and leave the split to the paragraph above the constant",
                rest.chars().take(40).collect::<String>()
            );
            judged += 1;
        }
        assert!(
            judged > 0,
            "nothing in this file says how many core files hold more than one rule, \
             so this half of the gate is wired to nothing"
        );
    }

    /// `PINNED`'s doc names the same number the constant does.
    ///
    /// **Unheld, the two drift apart**: the number moves with the family pack's
    /// quorum rule, and only the line the compiler reads has to move with it.
    /// The doc is the only account of why the count is what it is, so a reader
    /// reconciling against a stale one reconciles against a number that is no
    /// longer true.
    #[test]
    fn the_written_number_is_the_number() {
        let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/policy.rs"))
            .expect("this file is readable");
        let doc = source
            .split_once("const PINNED: usize")
            .expect("`PINNED` is declared here")
            .0;
        let doc = doc
            .rsplit_once("/// Every clause a case pins")
            .expect("`PINNED`'s doc opens with its summary")
            .1;
        let written: Vec<usize> = doc
            .split(|character: char| !character.is_ascii_digit())
            .filter(|run| !run.is_empty())
            .filter_map(|run| run.parse().ok())
            .collect();
        // **The *first* number, not any of them.** The doc block can name other
        // numbers, and `contains` would then accept a `PINNED` equal to any of
        // them — so the two-place edit this enforces would collapse to one
        // place, silently, the day the constant reaches one of those values.
        let first = written.first().copied();
        assert!(
            first == Some(PINNED),
            "`PINNED` is {PINNED} and its doc comment names {written:?} — the prose is the \
             only account of why the count is what it is, so a number in it that the constant \
             does not have is a reader sent to the wrong one"
        );
    }

    /// The two lists beside `CASES` and `PINNED` open with the numbers those
    /// constants hold.
    ///
    /// **A sentence that counts cases rots unless something holds it.** A count
    /// moved in the one line beside the list and not in the sentences that
    /// restate it leaves them saying the old number in the present tense. The
    /// summaries a reader reconciles a count against carry one, so they are
    /// held here, the way
    /// `the_written_number_is_the_number` holds `PINNED`'s own paragraph.
    #[test]
    fn the_lists_beside_the_counts_open_with_the_same_number() {
        let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/policy.rs"))
            .expect("this file is readable");
        for (list, count) in [
            ("const THE_CASES", super::CASES),
            ("const PINNED_BY_POLICY", PINNED),
        ] {
            let above = source
                .split_once(list)
                .unwrap_or_else(|| panic!("`{list}` is declared here"))
                .0;
            // The first line of the doc block that sits directly on the list:
            // walked up from the declaration while the lines are doc or
            // attribute lines, so a summary further up the file cannot answer.
            let summary = above
                .lines()
                .rev()
                .take_while(|line| line.starts_with("///") || line.starts_with("#["))
                .filter(|line| line.starts_with("///"))
                .last()
                .unwrap_or_default();
            let said = format!("/// The same {count}, by ");
            assert!(
                summary.starts_with(&said),
                "`{list}` accounts for {count} and its doc opens `{summary}`. It is the line \
                 a reader reconciles the count against, and a number beside a list that the \
                 list does not have sends that reader to the wrong one"
            );
        }
    }

    /// The manual's row for a policy names every list a case's name is held in.
    ///
    /// **A new case is a row in two lists, and the manual has to name both.** A
    /// case's name ships as a sentence through [`super::THE_CASES`], so it is a
    /// row of `WHAT_ELSE_THIS_TOOL_SAYS` in `check.rs` as well — and a row for a
    /// policy in `AGENTS.md`'s *Making a change* table naming `THE_CASES` and
    /// not the other leaves its reader, doing exactly what it says, with a red
    /// suite naming a sentence nobody told them about.
    ///
    /// Derived rather than listed: every constant in this crate's sources that
    /// holds the first case's name as a row is a list a new case moves, and the
    /// row has to name each one.
    #[test]
    fn the_manual_names_every_list_a_new_case_is_written_into() {
        let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        let manual =
            std::fs::read_to_string(root.join("AGENTS.md")).expect("AGENTS.md is readable");
        let row = manual
            .lines()
            .find(|line| line.starts_with("| a policy |"))
            .expect("`AGENTS.md`'s table has a row for a policy");
        let needle = format!("\"{}\",", super::THE_CASES[0].1);
        let mut holders = std::collections::BTreeSet::new();
        for entry in std::fs::read_dir(root.join("xtask/src"))
            .expect("the sources are readable")
            .flatten()
        {
            let text = std::fs::read_to_string(entry.path()).expect("a source is readable");
            for (at, _) in text.match_indices(&needle) {
                // The nearest declaration above the row is the list it is in.
                let holder = text[..at].lines().rev().find_map(|line| {
                    line.trim_start()
                        .strip_prefix("const ")
                        .and_then(|rest| rest.split_once(':'))
                        .map(|(name, _)| name.to_owned())
                });
                holders.insert(holder.unwrap_or_default());
            }
        }
        // **Two, not "some".** The census found in one list would leave the
        // other, which is this test's whole subject, compared against nothing.
        assert!(
            holders.contains("THE_CASES") && holders.len() >= 2,
            "a case's name was found in {holders:?}, and it ships through `THE_CASES` and \
             the sentence census both — the walk has stopped finding what it is for"
        );
        for holder in &holders {
            assert!(
                row.contains(&format!("`{holder}`")),
                "a case's name is a row of `{holder}`, and `AGENTS.md`'s row for a policy \
                 does not name it: whoever adds a case by that row meets `{holder}` as a red \
                 test instead of an instruction"
            );
        }
    }

    /// The world the cases decide against is the one written down.
    ///
    /// See [`super::THE_WORLD`] for the measurement behind it.
    #[test]
    fn the_world_the_cases_decide_against_is_the_one_written_down() {
        let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        let text = std::fs::read_to_string(root.join("policy/world.json"))
            .expect("policy/world.json is a file in this repository");
        let world: serde_json::Value =
            serde_json::from_str(&text).expect("policy/world.json is JSON");
        let mut found: Vec<(String, String, String)> = Vec::new();
        for entity in world.as_array().expect("the world is a list of entities") {
            let uid = entity.get("uid").expect("every entity has a uid");
            let named = format!(
                "{}::\"{}\"",
                uid.get("type")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default(),
                uid.get("id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
            );
            let attributes = entity.get("attrs").and_then(serde_json::Value::as_object);
            match attributes {
                Some(attributes) if !attributes.is_empty() => {
                    for (key, value) in attributes {
                        found.push((named.clone(), key.clone(), value.to_string()));
                    }
                }
                _ => found.push((named.clone(), String::new(), String::new())),
            }
            for parent in entity
                .get("parents")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
            {
                found.push((named.clone(), "<parent>".to_owned(), parent.to_string()));
            }
        }
        found.sort();
        let written: Vec<(String, String, String)> = super::THE_WORLD
            .iter()
            .map(|(entity, attribute, value)| {
                (
                    (*entity).to_owned(),
                    (*attribute).to_owned(),
                    (*value).to_owned(),
                )
            })
            .collect();
        assert_eq!(
            found, written,
            "the world these cases decide against is not the one written down. An \
             attribute that moved is a case moved off the boundary it pins, with every \
             row of `THE_CASES` and every number this step prints unchanged — read what \
             changed before writing the new list"
        );
    }
}
