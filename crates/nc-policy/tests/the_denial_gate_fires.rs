//! Every way the policy pack can be wrong, made to happen.
//!
//! The shape `nc-catalog-compiler`'s `the_gates_fire.rs` uses, and for the same
//! reason: a gate nobody has watched fire is a gate nobody knows is wired up
//! (`NOCTUA.md §19.3`).
//!
//! Each test copies the **real** pack, breaks one thing, and asserts the check
//! that fires is the one that should. A fixture pack would drift from the one
//! that ships, and then these tests would prove something about a pack nobody
//! deploys.

use std::path::{Path, PathBuf};

use nc_policy::{Problem, cases, coverage, load, run};

fn repository() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).to_path_buf()
}

/// A copy of the real pack, in a directory of this test's own.
struct Scratch {
    root: PathBuf,
}

/// **A test cleans up on the way out.** Removed only on the way in, every run
/// leaves its directory behind and nothing counts them: measured that way, one
/// passing `cargo test -p nc-policy --test the_denial_gate_fires` leaves 45
/// directories in `/tmp`, and 1,275 accumulate over an afternoon of `cargo xtask check`.
/// Its two sibling fixtures, `xtask/src/corpus.rs` and
/// `crates/nc-catalog-compiler/tests/the_gates_fire.rs`, carry the same `Drop`
/// for the same reason.
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Scratch {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("nc-policy-{}-{name}", std::process::id()));
        // Removed on the way in as well, because a previous run of this same
        // process id would otherwise be copied over rather than replaced.
        let _ = std::fs::remove_dir_all(&root);
        copy(&repository().join("policy"), &root.join("policy"));
        Self { root }
    }

    /// Rewrites a file, refusing an edit that changed nothing.
    ///
    /// A `replace` whose needle has drifted is a no-op, and a no-op leaves the
    /// pack valid — so the test passes for having broken nothing. That is
    /// `§19.5`'s vacuous assertion arriving through the fixture.
    fn edit(&self, relative: &str, edit: impl Fn(String) -> String) {
        let path = self.root.join(relative);
        let before = std::fs::read_to_string(&path).expect("the copy is readable");
        let after = edit(before.clone());
        assert_ne!(
            before, after,
            "the edit to {relative} changed nothing, so this test proves nothing"
        );
        std::fs::write(&path, after).expect("the copy is writable");
    }

    /// Everything wrong with the pack, its cases, and the two together.
    fn problems(&self) -> Vec<Problem> {
        let pack = match load(&self.root) {
            Ok(pack) => pack,
            Err(problems) => return problems,
        };
        let (cases, _) = match cases::load(&self.root) {
            Ok(both) => both,
            Err(problems) => return problems,
        };
        let (mut problems, _, _) = run(&pack, &cases);
        let (gaps, _) = coverage(&pack, &cases);
        problems.extend(gaps);
        problems
    }

    /// The pack is refused, `check` is what refused it, and the diagnostic names
    /// `naming` — in its sentence or in the file it is filed under.
    fn must_fail(&self, check: &str, naming: &str) {
        self.refused(check, naming, Names::InTheDiagnostic);
    }

    /// The same, for a `naming` the *message* computed rather than a path.
    ///
    /// See [`Names`] for the datum this separation was measured against.
    fn must_fail_saying(&self, check: &str, naming: &str) {
        self.refused(check, naming, Names::InTheSentence);
    }

    /// The pack still loads: an edit that changes no rule changes no verdict.
    ///
    /// The other direction of `must_fail`. A gate that refuses ordinary prose
    /// is refused here rather than found by somebody writing a comment.
    fn must_pass(&self) {
        let problems = self.problems();
        assert!(
            problems.is_empty(),
            "this edit changes no rule and the pack was refused: {:?} at {}",
            problems.iter().map(|p| p.check).collect::<Vec<_>>(),
            self.root.display()
        );
    }

    fn refused(&self, check: &str, naming: &str, names: Names) {
        assert!(
            !naming.is_empty(),
            "`{check}` was checked without saying what its message must name"
        );
        let problems = self.problems();
        assert!(
            !problems.is_empty(),
            "the pack was broken on purpose and every check accepted it: {} / {naming}",
            self.root.display()
        );
        let matching: Vec<&Problem> = problems
            .iter()
            .filter(|problem| problem.check == check)
            .collect();
        assert!(
            !matching.is_empty(),
            "nothing reported `{check}`; what did fire was {:?}",
            problems.iter().map(|p| p.check).collect::<Vec<_>>()
        );
        assert!(
            matching.iter().any(|problem| match names {
                Names::InTheDiagnostic =>
                    problem.what.contains(naming) || problem.file.contains(naming),
                Names::InTheSentence => problem.what.contains(naming),
            }),
            "`{check}` fired and named none of `{naming}` ({names:?}): {:?}",
            matching.iter().map(|p| &p.what).collect::<Vec<_>>()
        );
        for problem in &problems {
            assert!(
                problem.fix.len() > 40,
                "a diagnostic that only says what is wrong has done half the job: {}",
                problem.what
            );
        }
    }
}

/// Where a diagnostic has to name what broke.
///
/// **`naming` is not satisfied by the file path when it is a datum.**
/// `problem.what.contains(naming) || problem.file.contains(naming)`
/// is the right question when `naming` is a path — a problem about a whole file
/// names it in `file` and has nothing to repeat in `what` — and the wrong one
/// when `naming` is a *datum the sentence computed*. Measured, `case.name`
/// replaced by the constant `"some case in this pack"` in this crate's own
/// `lib.rs`: 34 suites, 0 failures, with
/// `denial-was-not-a-missing-permit` naming no case at all, because
/// `fail_closed.toml` was answering for the sentence.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Names {
    /// Anywhere a reader looks — the sentence or the file it is filed under.
    InTheDiagnostic,
    /// The sentence itself, because `naming` is a datum the message computed
    /// and a path cannot stand in for it.
    InTheSentence,
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("the scratch directory is writable");
    for entry in std::fs::read_dir(from)
        .expect("the pack is readable")
        .flatten()
    {
        let path = entry.path();
        let target = to.join(entry.file_name());
        if path.is_dir() {
            copy(&path, &target);
        } else {
            std::fs::copy(&path, &target).expect("the copy is writable");
        }
    }
}

/// The flag has to be true of the case that sets it.
///
/// **`the_action_is_the_boundary` is a per-case opt-out, and this is what
/// verifies it.** It relaxes the pairing rule so an action list can be pinned at all —
/// a case denied *because* the verb is outside the list has, by construction, a
/// verb the permit never allows. What it does not do is check that the verb
/// really is what refuses the request. Measured: the guardian boundary case
/// retargeted at a principal who is not a guardian, using an action the permit
/// *does* allow, keeps its flag and its name and, unverified, produces no
/// `boundary-without-a-control` at all.
///
/// The claim is one question: run the same request with a verb the permit
/// allows, and it must be permitted. If it is still denied, something other
/// than the action is refusing it.
#[test]
fn a_case_that_says_the_action_is_the_boundary_has_to_mean_it() {
    let scratch = Scratch::new("false-boundary");
    scratch.edit("policy/packs/family/tests/permissions.toml", |text| {
        // The guardian boundary case, retargeted at a principal who is not a
        // guardian. The action stays outside the list, so it is still denied —
        // but now by *who is asking*, not by the verb.
        let at = text
            .find("name = \"a guardian administers permissions and does not thereby read everything\"")
            .expect("the case is there");
        let ends = text[at..].find("\n\n").map_or(text.len(), |offset| at + offset);
        let case = &text[at..ends];
        let retargeted = case.replace("principal = 'User::\"ana\"'", "principal = 'User::\"bruno\"'");
        format!("{}{retargeted}{}", &text[..at], &text[ends..])
    });
    scratch.must_fail(
        "boundary-is-not-the-action",
        "does not thereby read everything",
    );
}

/// The principal flag has to be true of the case that sets it, like its twin.
///
/// `the_principal_is_the_boundary` lets a denial pair with an `allow` for
/// another kind of principal, which is the only way a `principal is User` can
/// be pinned at all. What it must not do is let a case refused for some *other*
/// reason buy that pairing. The device case moved to a realm it is not in keeps
/// its flag and its name, and is then refused by membership: asked again as the
/// principal of the permit's own `allow`, it is still denied.
///
/// **This fixture holds only while no control is written on `Realm::"office"`.**
/// The claim is that *some* principal the permit allows is permitted — see the
/// test below — so an `allow` for a member of that realm would permit the moved
/// request as that member, and the move would stop being a false claim.
#[test]
fn a_case_that_says_the_principal_is_the_boundary_has_to_mean_it() {
    let scratch = Scratch::new("false-principal-boundary");
    scratch.edit("policy/packs/family/tests/realm.toml", |text| {
        let at = text
            .find("name = \"a device in the realm steps through no history, however attested\"")
            .expect("the case is there");
        let ends = text[at..]
            .find("\n\n")
            .map_or(text.len(), |offset| at + offset);
        let case = &text[at..ends];
        let moved = case.replace(
            "resource = 'Realm::\"home\"'",
            "resource = 'Realm::\"office\"'",
        );
        format!("{}{moved}{}", &text[..at], &text[ends..])
    });
    scratch.must_fail_saying(
        "boundary-is-not-the-principal",
        "a device in the realm steps through no history",
    );
}

/// The principal flag is met by one principal the permit allows, not by all.
///
/// **One correct `allow` must not turn both correct cases red.** A flagged
/// case's controls match it on the resource's *type*, and the request asked
/// again keeps the case's own resource and context — so a control the permit
/// allows on another realm is refused on this one, by membership, and that
/// refusal says nothing about who is asking. Judged against every
/// control, `mallory Write Realm::"office"`, which Cedar allows, reports the
/// device case and the agent case as `boundary-is-not-the-principal` with not
/// one rule changed. The claim is that *some* principal the permit allows is
/// permitted here.
///
/// The control is written in a file that sorts before `realm.toml`, so it is
/// the first one judged — which is what a claim judged against the first
/// control alone gets wrong too.
#[test]
fn the_principal_flag_is_met_by_some_control_and_not_refuted_by_another_realms() {
    let scratch = Scratch::new("principal-boundary-other-realm");
    std::fs::write(
        scratch
            .root
            .join("policy/packs/family/tests/a_control_from_another_realm.toml"),
        "[default.context]\n\
         now = { datetime = \"2026-08-26T00:00:00Z\" }\n\
         via_agent = false\n\
         confirmed_by_human = false\n\
         actor_chain_depth = 0\n\
         policy_epoch = 7\n\
         quorum_met = true\n\
         grant = { entity = 'Grant::\"g.current\"' }\n\
         \n\
         [[case]]\n\
         name = \"a member of the other realm steps through her history there\"\n\
         principal = 'User::\"mallory\"'\n\
         action = \"Write\"\n\
         resource = 'Realm::\"office\"'\n\
         decision = \"allow\"\n\
         because = \"family.a_member_steps_through_their_own_history\"\n",
    )
    .expect("the copy is writable");

    // **The fixture is the first control, and the flagged cases are there.**
    // Either one missing and a clean pack below proves nothing.
    let (cases, _) = cases::load(&scratch.root).expect("the cases load");
    let at = |name: &str| {
        cases
            .iter()
            .position(|case| case.name == name)
            .unwrap_or_else(|| panic!("`{name}` is not among the cases"))
    };
    assert!(
        at("a member of the other realm steps through her history there")
            < at("a member steps through her history in her own realm"),
        "the other realm's control is not judged first, so this does not reach the \
         ordering a first-control-only claim gets wrong"
    );
    let flagged = cases
        .iter()
        .filter(|case| {
            case.the_principal_is_the_boundary
                && case.no_permit_matched.as_deref()
                    == Some("family.a_member_steps_through_their_own_history")
        })
        .count();
    assert!(
        flagged >= 2,
        "{flagged} case(s) say the principal is the history permit's boundary, and the \
         device case and the agent case both do"
    );

    let problems = scratch.problems();
    assert!(
        problems.is_empty(),
        "a correct `allow` on another realm refused a pack no rule of which changed: \
         {:#?}",
        problems
            .iter()
            .map(|problem| (problem.check, &problem.what))
            .collect::<Vec<_>>()
    );
}

/// A boundary flag on a case with no boundary is refused, not ignored.
///
/// **Both flags are judged where the pairing is, and that
/// reads `no_permit_matched` cases alone.** Without
/// this, `the_principal_is_the_boundary` added to the history permit's `allow`
/// case leaves `cargo xtask check` printing the policy block unchanged, and the
/// census prints `/boundary=principal` beside an `Allow` — a claim no code
/// evaluates, which `NOCTUA.md §19.5` calls a title rather than an
/// assertion. The same flag on a denial a `forbid` caused is green too.
#[test]
fn a_boundary_flag_on_a_case_with_no_boundary_is_refused() {
    let allow = Scratch::new("boundary-flag-on-an-allow");
    allow.edit("policy/packs/family/tests/realm.toml", |text| {
        text.replace(
            "name = \"a member steps through her history in her own realm\"\n",
            "name = \"a member steps through her history in her own realm\"\n\
             the_principal_is_the_boundary = true\n",
        )
    });
    allow.must_fail_saying(
        "boundary-flag-without-a-boundary",
        "a member steps through her history in her own realm",
    );

    let forbidden = Scratch::new("boundary-flag-on-a-forbid");
    forbidden.edit("policy/packs/family/tests/accounts.toml", |text| {
        text.replace(
            "name = \"a suspended account writes nothing, not even what it owns\"\n",
            "name = \"a suspended account writes nothing, not even what it owns\"\n\
             the_action_is_the_boundary = true\n",
        )
    });
    forbidden.must_fail_saying(
        "boundary-flag-without-a-boundary",
        "a suspended account writes nothing, not even what it owns",
    );
}

/// A pack's floor narrowed to one kind of resource in its condition is refused.
///
/// **On the shipped frozen-account floor.** `principal.status == "suspended"`
/// rewritten as `(principal.status == "suspended" && resource is Content)`
/// prints the policy block byte-identical and `check passed` when a type test
/// is excused anywhere in a pack floor's condition, and a suspended account has
/// its undo and redo back; the mutation loops never drop a disjunct. A type
/// test is excused there only where it guards a read of the same subject, which
/// this does not.
///
/// The cases in `accounts.toml` would also answer `wrong-decision` here, since
/// each status is decided on every kind of resource a permit reaches. This asks
/// for the gate's own verdict, because a case is written for the request its
/// author thought of and the gate reads the rule.
#[test]
fn a_pack_floor_narrowed_to_one_kind_of_resource_in_its_condition_is_refused() {
    let scratch = Scratch::new("pack-floor-type-test-in-a-condition");
    scratch.edit("policy/packs/family/accounts.cedar", |text| {
        text.replace(
            "  !(principal has status) ||\n  !(principal.status == \"active\" ||",
            "  !(principal has status) ||\n  resource is Content && !(principal.status == \"active\" ||",
        )
    });
    scratch.must_fail_saying(
        "core-narrows-by-entity-type",
        "`family.a_frozen_account_acts_on_nothing` is a pack's floor and leaves part of its \
         own class uncovered, by `resource is Content`",
    );
}

/// The principal flag frees the principal's type and nothing else.
///
/// The pairing still asks for an `allow` on the same action: a flag that let
/// the verb go as well would pair a device's denial with any `allow` the
/// permit ever gave on a realm, which is the free pass the pairing rule exists
/// to refuse. Moved to `Share`, a verb the history permit never allows, the
/// device case has no control left.
#[test]
fn the_principal_flag_does_not_free_the_action_too() {
    let scratch = Scratch::new("principal-boundary-verb");
    scratch.edit("policy/packs/family/tests/realm.toml", |text| {
        let at = text
            .find("name = \"a device in the realm steps through no history, however attested\"")
            .expect("the case is there");
        let ends = text[at..]
            .find("\n\n")
            .map_or(text.len(), |offset| at + offset);
        let case = &text[at..ends];
        let moved = case.replace("action = \"Write\"", "action = \"Share\"");
        format!("{}{moved}{}", &text[..at], &text[ends..])
    });
    scratch.must_fail_saying(
        "boundary-without-a-control",
        "a device in the realm steps through no history",
    );
}

/// A person's history is a person's: the type test saying so is pinned.
///
/// **The type test, held.** `family.a_member_steps_through_their_own_history`
/// without its `principal is User`, beside a note saying no request the schema
/// can build shows the type test working, is wrong twice over: two requests do
/// — an attested device and an agent acting openly as one, each writing the
/// realm, the undo and redo of a history that is not theirs — and both are
/// allowed. A denial for a device that can only pair with an `allow` for a
/// device cannot pin the type test; the two cases here say the principal is
/// their boundary, and deleting the type test turns both of them around.
#[test]
fn the_type_test_that_keeps_devices_out_of_a_persons_history_is_pinned() {
    let scratch = Scratch::new("history-type-test");
    scratch.edit("policy/packs/family/realm.cedar", |text| {
        text.replace(
            "permit (\n  principal is User,\n  action == Action::\"Write\",",
            "permit (\n  principal,\n  action == Action::\"Write\",",
        )
    });
    scratch.must_fail_saying(
        "wrong-decision",
        "a device in the realm steps through no history, however attested",
    );
    scratch.must_fail_saying(
        "wrong-decision",
        "nor does an agent, even one acting openly as an agent",
    );
}

/// A permit widened by one token is caught, and the check is reached to catch it.
///
/// **This holds the call site.** `check_every_permit_constraint_is_tested` is
/// the check that makes "every policy has a denial test" mean something — a
/// permit widened to a bare `action` keeps its denial test and stops being the
/// rule it was — and two unit tests on synthetic strings do not reach it:
/// deleting the one line that calls it leaves `check passed` with the same test
/// count and the same `N policy(ies) each have a denial test`, and
/// the `the_action_is_the_boundary` cases written to satisfy it become
/// decoration with nothing saying so.
///
/// Driven through `run`, which is the door `xtask` uses.
#[test]
fn widening_a_permit_by_one_token_is_reported() {
    // The one case that pins the household read permit's verb, taken away. With
    // it present a widening is caught as `wrong-decision` — the case flips —
    // which is the mechanism working; what this drives is the *other*
    // direction, the one that says nothing pins the constraint at all.
    //
    // **One case, so the constraint has to be one only that case pins.** Taking
    // away the guardian permit's action case leaves the list pinned and the
    // pack clean, because the pack's permits over the realm pin that same list
    // several times over — a guardian is refused `Share` on the realm, `Read`
    // on another realm and `Delete` on her own account, and each of those
    // becomes an `allow` if her `Grant`/`Revoke` list is widened. The permit
    // here is `Read` on shared content, and the one other case sending another
    // verb at content the household shares is refused by a `forbid`, which a
    // wider permit does not move.
    let scratch = Scratch::new("unpinned");
    scratch.edit("policy/packs/family/tests/permissions.toml", |text| {
        let at = text
            .find("name = \"a member reads what the house shares and does not share it onward\"")
            .expect("the case is there");
        let opens = text[..at].rfind("[[case]]").expect("its header");
        let ends = text[at..]
            .find("\n\n")
            .map_or(text.len(), |offset| at + offset + 1);
        format!("{}{}", &text[..opens], &text[ends..])
    });
    scratch.must_fail(
        "permit-constraint-untested",
        "a_member_reads_shared_content",
    );

    // **A `.toml` outside a `tests/` directory is not a case.** Deleted on its
    // own, the clause that says so reads every `.toml` under `policy/` as a
    // case file, the family pack's `pack.toml` among them, and the suite fails
    // on `case-file-malformed` — measured. That manifest is the file this
    // protects: read as a case it is not a case, and the count a gate compares
    // against a written number would move under it.
    // **And a case file one directory deeper is still a case.** A rule asking
    // whether the *immediate* parent is `tests`, while its own doc says any
    // such directory, runs a file one level deeper nowhere and leaves the count
    // where it was.
    let deeper = Scratch::new("deeper");
    let (before, _) = cases::load(&deeper.root).expect("the copied cases load");
    let grouped = deeper.root.join("policy/packs/family/tests/grouped");
    std::fs::create_dir_all(&grouped).expect("writable");
    std::fs::copy(
        deeper
            .root
            .join("policy/packs/family/tests/permissions.toml"),
        grouped.join("permissions.toml"),
    )
    .expect("readable and writable");
    let (after, _) = cases::load(&deeper.root).expect("the copied cases still load");
    assert!(
        after.len() > before.len(),
        "a case file one directory deeper was not read: {} then {}",
        before.len(),
        after.len()
    );

    let outside = Scratch::new("outside");
    let (before, _) = cases::load(&outside.root).expect("the copied cases load");
    std::fs::write(
        outside.root.join("policy/core/not-a-case.toml"),
        "[[case]]\nname = \"planted\"\n",
    )
    .expect("writable");
    let (after, _) = cases::load(&outside.root).expect("the copied cases still load");
    assert_eq!(
        before.len(),
        after.len(),
        "a `.toml` outside a `tests/` directory was read as a case file"
    );

    // And the count the step reports, which is what says the check looked at
    // anything at all. `PINNED_BY_POLICY` in `xtask/src/policy.rs` is where the
    // number per policy is written down.
    let pack = load(&Scratch::new("counted").root).expect("the shipped pack loads");
    let (cases, _) = cases::load(&Scratch::new("counted").root).expect("its cases load");
    let (_, _, pinned) = run(&pack, &cases);
    assert!(
        pinned.values().sum::<usize>() > 0,
        "the widening check offered no constraint for widening, so it inspected nothing"
    );
    // **And per policy, because that is what the map is for.** A total says
    // something was inspected; it does not say the policy this test is about
    // was. `xtask/src/policy.rs` holds the whole table; here it is enough that
    // more than one policy contributed, so a run that inspected one policy
    // thirty-eight times is not the same result as this.
    assert!(
        pinned.values().filter(|count| **count > 0).count() > 1,
        "one policy accounts for every pinned clause: {pinned:?}"
    );
}

/// Every case reports the line its name is declared on.
///
/// **Every diagnostic that carries a line is read back here.** `Case::line`
/// feeds every `line: case.line` in the judge, and nothing else in `nc-policy`
/// or in `xtask/src/policy.rs` checks `Problem::line` — so without this the
/// whole locator could be replaced by a constant, or by the line of the
/// first `[[case]]` header in the file, with every suite green, measured.
///
/// Reading the reported line *back off the file* is the check, and it is the
/// same shape `the_gates_fire.rs` uses for `blank-message`
/// and `name-collision`.
#[test]
fn every_case_reports_the_line_its_name_is_declared_on() {
    let (cases, files) = cases::load(&repository()).expect("the cases load");
    assert!(
        !cases.is_empty(),
        "no case was loaded, so nothing is located"
    );
    assert!(!files.is_empty(), "no case file was read");
    for case in &cases {
        let text = std::fs::read_to_string(repository().join(&case.file))
            .expect("a case file this loader read is readable");
        assert_ne!(
            case.line, 0,
            "`{}` in {} reports no line at all, so every diagnostic about it is filed \
             against the file as a whole",
            case.name, case.file
        );
        let reported = text
            .lines()
            .nth(case.line - 1)
            .expect("the reported line is a line of the file");
        assert!(
            reported.contains(&case.name),
            "`{}` in {} is reported at line {}, which reads `{}`",
            case.name,
            case.file,
            case.line,
            reported.trim()
        );
        assert!(
            !reported.trim_start().starts_with('#'),
            "`{}` in {} is reported at line {}, which is a comment: the pack's own files \
             explain each case in the prose above it, so that is where a bare text search \
             lands",
            case.name,
            case.file,
            case.line
        );
    }
}

/// A comment quoting a case name is not where that case is declared.
///
/// The pack's files open with a paragraph naming what each case is for, so this
/// is not a hypothetical shape — it is the shape those files are written in.
#[test]
fn a_comment_that_quotes_a_case_name_is_not_where_the_case_is_declared() {
    let scratch = Scratch::new("case-named-in-a-comment");
    let relative = "policy/core/tests/grants_expire.toml";
    let (before, _) = cases::load(&scratch.root).expect("the copied cases load");
    let first = before
        .iter()
        .find(|case| case.file == relative)
        .expect("that file holds a case")
        .clone();
    // **The declaration, quoted.** A comment naming the case in prose is
    // already refused by the needle — `line_of` is handed `name = "…"`, which
    // prose does not hold — so the comment clause beneath it is decided by
    // nothing else and can be deleted with every other suite green, measured.
    // What reaches the clause is a comment quoting the declaration itself,
    // which is how these files are written when somebody copies a case to
    // explain it.
    scratch.edit(relative, |text| {
        format!(
            "# name = \"{}\" is what this file opens with.\n{text}",
            first.name
        )
    });
    let (after, _) = cases::load(&scratch.root).expect("the edited cases still load");
    let moved = after
        .iter()
        .find(|case| case.name == first.name)
        .expect("the case is still there");
    assert_eq!(
        moved.line,
        first.line + 1,
        "a comment quoting the case name moved where the case is reported to be — it is \
         one line further down than it was, and nowhere else"
    );
}

/// A case whose name opens another case's reports its own line.
#[test]
fn a_case_whose_name_opens_another_reports_its_own_line() {
    let scratch = Scratch::new("case-name-opens-another");
    let relative = "policy/core/tests/grants_expire.toml";
    let (before, _) = cases::load(&scratch.root).expect("the copied cases load");
    let first = before
        .iter()
        .find(|case| case.file == relative)
        .expect("that file holds a case")
        .clone();
    // The second case's name becomes a prefix of the first's, so a search for
    // the shorter one finds the longer one's line first.
    let shortened: String = first
        .name
        .split_whitespace()
        .take(3)
        .collect::<Vec<&str>>()
        .join(" ");
    let second = before
        .iter()
        .filter(|case| case.file == relative && case.name != first.name)
        .map(|case| case.name.clone())
        .next()
        .expect("that file holds a second case");
    scratch.edit(relative, |text| {
        text.replace(
            &format!("name = \"{second}\""),
            &format!("name = \"{shortened}\""),
        )
    });
    let (after, _) = cases::load(&scratch.root).expect("the edited cases still load");
    let renamed = after
        .iter()
        .find(|case| case.name == shortened)
        .expect("the renamed case is there");
    let text = std::fs::read_to_string(scratch.root.join(relative)).expect("readable");
    let reported = text
        .lines()
        .nth(renamed.line - 1)
        .expect("the reported line is a line of the file");
    assert_eq!(
        reported.trim(),
        format!("name = \"{shortened}\""),
        "a case whose name opens another's was reported at the other one's line"
    );
    assert_ne!(
        renamed.line, first.line,
        "the shorter name was located at the longer case's line"
    );
}

#[test]
fn the_pack_this_repository_ships_is_clean() {
    // The baseline, and the coverage floor for every test below: they all break
    // the real pack, so the real pack has to start correct.
    let pack = load(&repository()).expect("the pack loads and validates");
    assert!(
        !pack.is_empty(),
        "no policy was loaded, so nothing is checked"
    );
    let (cases, files) = cases::load(&repository()).expect("the cases load");
    assert!(
        files.len() >= 5,
        "only {} case file(s) were found",
        files.len()
    );

    let (problems, ran, _) = run(&pack, &cases);
    assert!(problems.is_empty(), "{problems:#?}");
    // **Every case, not fifteen of them.** `ran >= 15` against 40 leaves
    // twenty-five cases payable by any filter between the loop's head and its
    // verdicts — which is what `ran += 1` sitting first in the body is: three
    // lines skipping one file leave the printed count byte-identical. The
    // counter is the last statement, so the number falls when a case is
    // skipped, and this is what says it may not.
    //
    // Equality, because every case in this pack is well-formed: the one
    // legitimate `continue` above the counter is `request_for` refusing a
    // malformed case, and such a case is a `problem`, which the line above
    // requires to be none.
    assert_eq!(
        ran,
        cases.len(),
        "{ran} of {} case(s) reached a verdict, and the rest were skipped between the \
         loop's head and its judgements with no problem reported",
        cases.len()
    );

    let (gaps, checked) = coverage(&pack, &cases);
    assert!(gaps.is_empty(), "{gaps:#?}");
    assert_eq!(
        checked,
        pack.len(),
        "coverage looked at {checked} of {} policies",
        pack.len()
    );
}

#[test]
fn an_allow_case_is_not_a_denial_test_however_it_is_written() {
    // Crediting a `no_permit_matched` case as a denial test whatever its
    // decision, `coverage` would count an `allow` case naming a permit as that
    // permit's negative test. `run` catches that shape by another route, and
    // `coverage` is called on its own by the gate — so without this, deleting
    // the guard leaves the whole suite green, which is the "wired, and can
    // regress out of existence without a red light" shape the two checks above
    // are held against.
    let scratch = Scratch::new("allow-is-not-a-denial");
    scratch.edit("policy/packs/family/tests/permissions.toml", |text| {
        text.replace(
            "decision = \"deny\"\nno_permit_matched = \"family.a_member_reads_shared_content\"",
            "decision = \"allow\"\nno_permit_matched = \"family.a_member_reads_shared_content\"",
        )
    });
    let (cases, _) = cases::load(&scratch.root).expect("the cases load");
    let pack = load(&scratch.root).expect("the pack loads");
    let (gaps, _) = coverage(&pack, &cases);
    assert!(
        gaps.iter().any(|gap| gap.check == "no-denial-test"
            && gap.what.contains("family.a_member_reads_shared_content")),
        "an `allow` case was credited as a denial test: {gaps:#?}"
    );
}

#[test]
fn a_statement_that_is_not_a_policy_is_refused_rather_than_ignored() {
    // **A Cedar template parses and is not a policy.** `PolicySet::from_str`
    // accepts it and `policies()` yields nothing, so a Cedar file in
    // `policy/core/` written this way would be read as in force and be in force
    // nowhere: no `@id` requirement, no narrowing check, no denial test, and the
    // `13 policy(ies) parse and validate` count unmoved — so `§19.3`'s floor
    // could not see it either.
    let templated = Scratch::new("policy-template");
    templated.edit("policy/core/fail_closed.cedar", |text| {
        text.replace("  principal,\n", "  principal == ?principal,\n")
    });
    templated.must_fail("policy-is-not-in-force", "fail_closed.cedar");

    // **And the template written the way a person writes an annotation.**
    // `@id ("…")`, with a space, is legal Cedar and `policy.annotation("id")`
    // reads it — and a rule counting the four-byte literal `@id("` misses it:
    // the space makes the whole statement invisible, a sixteenth rule on disk,
    // in force nowhere, with `check passed`.
    let spaced = Scratch::new("policy-template-spaced");
    spaced.edit("policy/core/fail_closed.cedar", |text| {
        text.replace("  principal,\n", "  principal == ?principal,\n")
            .replace("@id(\"", "@id (\"")
    });
    spaced.must_fail("policy-is-not-in-force", "fail_closed.cedar");

    // **And a comment that mentions an annotation is not one.** Refusing the
    // whole file for two ordinary lines of prose above the `@id` — the kind
    // this tree writes everywhere — makes `load` return `Err` and every later
    // policy check stop running: 32 of this harness's 36 tests failing, naming
    // the wrong one, measured. `fail_closed.cedar` writes the words one
    // character from tripping it.
    let commented = Scratch::new("policy-commented");
    commented.edit("policy/core/fail_closed.cedar", |text| {
        format!(
            "// A journal entry for a denial here records `@id(\"core.fail_closed\")` as\n             // the rule that decided it.\n{text}"
        )
    });
    commented.must_pass();

    // A second statement with no annotation of its own is a *policy* — Cedar
    // enforces it — so what it lacks is a stable name, and that is the rule
    // that says so, not "not in force", which is the one thing it is not.
    let extra = Scratch::new("policy-extra-statement");
    extra.edit("policy/core/fail_closed.cedar", |text| {
        format!("{text}\nforbid (principal, action, resource) when {{ false }};\n")
    });
    extra.must_fail("policy-without-an-id", "fail_closed.cedar");
}

#[test]
fn a_core_invariant_that_turns_itself_off_for_an_entity_type_is_refused() {
    // **Two exemptions, one per kind of entity type.** A case catches an
    // exemption only for a type it names, so the gate refuses the shape
    // itself, whatever the cases name:
    //
    //   provenance: `unless { … || principal is Agent }` — an agent acting with
    //               no grant at all, which is what invariant 2 exists to stop;
    //   quorum:     `unless { … || resource is Feature }` — one person removing
    //               a platform capability alone.
    //
    // Covering the space case by case cannot close this: the schema declares
    // three principal types and four resource types, and a permit for every
    // pair is a product decision nobody has made. The rule is structural.
    let scratch = Scratch::new("core-narrows");
    scratch.edit("policy/core/provenance.cedar", |text| {
        text.replace(
            "unless { context has grant };",
            "unless { context has grant || principal is Agent };",
        )
    });
    scratch.must_fail("core-narrows-by-entity-type", "core.provenance");

    // And the other one, in the other position.
    let quorum = Scratch::new("core-narrows-resource");
    quorum.edit("policy/core/quorum.cedar", |text| {
        text.replace(
            "unless { context has quorum_met && context.quorum_met };",
            "unless { context has quorum_met && context.quorum_met || resource is Feature };",
        )
    });
    quorum.must_fail(
        "core-narrows-by-entity-type",
        "core.destructive_needs_quorum",
    );

    // **`has` is the same test in different words**, and it can defeat this
    // check with one token. Every entity type in the schema declares a required
    // attribute no other type has — `Agent` has `autonomy` and
    // `budget_remaining`, `Device` has `node_key`, `Feature` has `tier` — so
    // `unless { principal has budget_remaining }` is an exact test for "is an
    // Agent". Unrefused, it turns invariant 1 off for every agent with
    // `check passed`.
    for attribute in ["budget_remaining", "node_key", "tier", "autonomy"] {
        let spelled = Scratch::new(&format!("core-narrows-has-{attribute}"));
        spelled.edit("policy/core/fail_closed.cedar", |text| {
            // The `unless` joins the policy, so the `;` it replaces is the
            // policy's own terminator rather than a second statement.
            format!(
                "{}\nunless {{ principal has {attribute} }};\n",
                text.trim_end().trim_end_matches(';')
            )
        });
        spelled.must_fail("core-narrows-by-entity-type", "core.fail_closed");
    }

    // **A declared exception in the scope is allowed**, so the rule is a rule and
    // not a ban: a later invariant that genuinely applies to some entity types
    // says so on its first line, where a reader meets it.
    let declared = Scratch::new("core-narrows-declared");
    declared.edit("policy/core/provenance.cedar", |text| {
        text.replace(
            "@id(\"core.provenance\")",
            "// nc:narrows: `principal is` — a device presents its own key rather than a grant\n             @id(\"core.provenance\")",
        )
    });
    declared.edit("policy/core/provenance.cedar", |text| {
        text.replace("  principal,\n", "  principal is Device,\n")
    });
    let problems = declared.problems();
    assert!(
        !problems
            .iter()
            .any(|problem| problem.check == "core-narrows-by-entity-type"),
        "a declared exception in the scope was refused: {problems:#?}"
    );

    // **And a marker excuses the constraint it names and no other.** Per
    // *policy*, the shipped marker on `core.destructive_needs_quorum`, written
    // about `action == Action::"Delete"`, would also cover a
    // `principal is User` added beside it — measured turning invariant 5 off
    // for every `Agent` and every `Device`, with `check passed`. `limits.rs`
    // holds its own hatch to the same rule.
    let beside = Scratch::new("core-narrows-beside-a-marker");
    beside.edit("policy/core/quorum.cedar", |text| {
        text.replace("  principal,\n", "  principal is User,\n")
    });
    beside.must_fail(
        "core-narrows-by-entity-type",
        "core.destructive_needs_quorum",
    );

    the_marker_names_a_constraint_and_its_operator();

    // And the shipped markers really are what let the two policies that need one
    // through, so the rule above is not passing because nothing uses it.
    let unmarked = Scratch::new("core-narrows-unmarked");
    unmarked.edit("policy/core/quorum.cedar", |text| {
        text.replace("// nc:narrows: `action ==`,", "// the action,")
    });
    unmarked.must_fail(
        "core-narrows-by-entity-type",
        "core.destructive_needs_quorum",
    );

    a_marker_excuses_only_what_it_names();
}

/// Which constraint a narrowing marker excuses, down to the operator.
///
/// Its own function because each of these is a whole plant with its own
/// scratch tree, and because the four together are one question: *what did
/// the author write down*, as against what sits near it.
fn the_marker_names_a_constraint_and_its_operator() {
    // **And the operator is part of the constraint the marker names.** The
    // marker on `core.budget_is_a_permission` is written about
    // `principal is Agent` — a type test, which is what `§14.6`'s *delegation,
    // never impersonation* is about — and it excused
    // `principal == Agent::"ana-assistant"` beside it: the same invariant
    // holding for exactly one entity in the world, with the gate green. Both
    // constraints are "the principal", and only one of them is a rule.
    let named = Scratch::new("core-narrows-a-named-entity");
    named.edit("policy/core/agent_authority.cedar", |text| {
        text.replacen(
            "  principal is Agent,\n",
            "  principal == Agent::\"ana-assistant\",\n",
            1,
        )
    });
    named.must_fail("core-narrows-by-entity-type", "core.budget_is_a_permission");

    // **And a realm scoping, which Cedar renders with the same `op`.**
    // `principal is Agent in Realm::"home"` is `op: "is"` with an `in` beside
    // it, so reading only the operator collapses it onto the plain type test
    // and the shipped marker — whose reason says *a type test and not a named
    // entity* — excuses it. The invariant would hold in one realm and be absent
    // in every other one a deployment creates; `policy/world.json` has two and
    // every agent lives in `home`, so no case could see it either.
    let scoped = Scratch::new("core-narrows-to-one-realm");
    scoped.edit("policy/core/agent_authority.cedar", |text| {
        text.replace(
            "  principal is Agent,\n",
            "  principal is Agent in Realm::\"home\",\n",
        )
    });
    scoped.must_fail(
        "core-narrows-by-entity-type",
        "core.an_agent_never_impersonates",
    );

    // **And the fuller quote is accepted, because too strong is the other way
    // to be wrong.** `principal is Agent` is the constraint as the author reads
    // it on the policy's first line; requiring the truncated rendering exactly
    // refuses it and tells them to quote what they have quoted.
    let fuller = Scratch::new("core-narrows-quoted-in-full");
    fuller.edit("policy/core/agent_authority.cedar", |text| {
        text.replace(
            "// nc:narrows: `principal is` —",
            "// nc:narrows: `principal is Agent` —",
        )
    });
    let problems = fuller.problems();
    assert!(
        !problems
            .iter()
            .any(|problem| problem.check == "core-narrows-by-entity-type"),
        "a reason quoting the constraint in full was refused: {problems:#?}"
    );

    // **And a reason may quote the thing it warns against.**
    // `core.no_self_escalation`'s paragraph quotes `action == Action::"Read"` to
    // say what can go wrong to `core.provenance` — and a prefix rule reading
    // *any* backtick span in the prose lets that quotation satisfy the
    // constraint `action ==` as well as the marker's own `action in`.
    // Rescoping invariant 4 to any `action == …` is then excused by the
    // sentence written to warn about exactly that, and only two
    // `wrong-decision` cases fire — a core policy whose scope no case happens
    // to cover would go through in silence.
    let counterexample = Scratch::new("core-narrows-reason-quotes-a-counterexample");
    counterexample.edit("policy/core/no_self_escalation.cedar", |text| {
        text.replace(
            "  action in [Action::\"Grant\", Action::\"Revoke\"],\n",
            "  action == Action::\"Delete\",\n",
        )
    });
    counterexample.must_fail("core-narrows-by-entity-type", "core.no_self_escalation");

    // **And with the entity the warning itself names**, which is the one the
    // entity rule above cannot tell from an excuse. `Delete` differs from the
    // `Read` the paragraph quotes, so the span is refused for saying the wrong
    // thing rather than for sitting in the wrong place; rescoping invariant 4
    // to `action == Action::"Read"` matches that sentence word for word, and
    // read to the end of the run it produces no finding at all — measured,
    // `load()` succeeds and only `wrong-decision` fires, from the two cases
    // that happen to cover this policy's scope. The reason is the marker's own
    // line, and the paragraph under it is prose.
    let quoted = Scratch::new("core-narrows-reason-quotes-the-very-rescoping");
    quoted.edit("policy/core/no_self_escalation.cedar", |text| {
        text.replace(
            "  action in [Action::\"Grant\", Action::\"Revoke\"],\n",
            "  action == Action::\"Read\",\n",
        )
    });
    quoted.must_fail("core-narrows-by-entity-type", "core.no_self_escalation");

    // **And the marker a careful author writes is accepted.** The rule above
    // can be wrong in the other direction too:
    // `action in [Action::"Share", Action::"Grant"]` renders with an `entities`
    // array and no `entity`, so read that way the constraint names nothing and
    // only a reason stopping at the operator could be accepted. The pack's own
    // marker rewritten to the list its policy actually names — strictly more
    // informative, and the spelling the printed `fix` asks for — is then
    // refused: measured, `check` reports
    // `family.giving_out_encrypted_content_needs_quorum` as a pack's floor
    // leaving part of its own class uncovered.
    let fuller = Scratch::new("core-narrows-reason-names-a-whole-list");
    fuller.edit("policy/packs/family/permissions.cedar", |text| {
        text.replace(
            "// nc:narrows: `action in` — the two verbs",
            "// nc:narrows: `action in [Action::\"Share\", Action::\"Grant\"]` — the two verbs",
        )
    });
    fuller.must_pass();

    // And one verb of the two is a reason for a rule this policy does not have.
    let half = Scratch::new("core-narrows-reason-names-half-a-list");
    half.edit("policy/packs/family/permissions.cedar", |text| {
        text.replace(
            "// nc:narrows: `action in` — the two verbs",
            "// nc:narrows: `action in Action::\"Share\"` — the two verbs",
        )
    });
    half.must_fail(
        "core-narrows-by-entity-type",
        "family.giving_out_encrypted_content_needs_quorum",
    );
}

/// The marker's reach: what it excuses, and what it does not.
fn a_marker_excuses_only_what_it_names() {
    // **Every marked policy, every subject the marker does not name.** A scan
    // returning at the first constrained subject leaves the other two
    // unexamined for a marker excusing that one: eight of nine constraints
    // added across the marked core policies go unreported, and every case still
    // decides as written — so case coverage cannot see any of them either.
    for (file, policy, added, subject) in [
        (
            "quorum.cedar",
            "core.destructive_needs_quorum",
            "  resource\n",
            "  resource is Content\n",
        ),
        (
            "quorum.cedar",
            "core.destructive_needs_quorum",
            "  principal,\n",
            "  principal is User,\n",
        ),
        (
            "agent_authority.cedar",
            "core.budget_is_a_permission",
            "  action,\n",
            "  action == Action::\"Read\",\n",
        ),
        (
            "no_self_escalation.cedar",
            "core.no_self_escalation",
            "  principal,\n",
            "  principal is User,\n",
        ),
    ] {
        let beside = Scratch::new(&format!("core-narrows-{policy}-{}", subject.len()));
        beside.edit(&format!("policy/core/{file}"), |text| {
            text.replacen(added, subject, 1)
        });
        beside.must_fail("core-narrows-by-entity-type", policy);
    }

    // **And the reason judged is the marker's own text.** Comment lines
    // collected with `.rev()` and joined put everything *above* a marker
    // *after* it, read as part of its reason — `nc:narrows: reasons` excused
    // for `principal` by a sentence written eight lines earlier about
    // something else.
    let borrowed = Scratch::new("core-narrows-borrowed-reason");
    borrowed.edit("policy/core/fail_closed.cedar", |text| {
        text.replace(
            "@id(\"core.fail_closed\")",
            "// a note about the principal of this rule\n             // nc:narrows: reasons\n             @id(\"core.fail_closed\")",
        )
    });
    borrowed.edit("policy/core/fail_closed.cedar", |text| {
        text.replace("  principal,\n", "  principal is User,\n")
    });
    borrowed.must_fail("core-narrows-by-entity-type", "core.fail_closed");

    // **And the same marker does not excuse a condition.** Excusing the policy,
    // one written for `core.destructive_needs_quorum`'s
    // `action == Action::"Delete"` — a real, documented proxy — would also
    // excuse a `|| resource is Feature` smuggled into its condition, which is
    // one person removing a platform capability alone. A scope constraint is on
    // the first line; a condition is where an exemption hides.
    let hidden = Scratch::new("core-narrows-hidden");
    hidden.edit("policy/core/provenance.cedar", |text| {
        text.replace(
            "@id(\"core.provenance\")",
            "// nc:narrows: `principal is` — a device presents its own key rather than a grant\n             @id(\"core.provenance\")",
        )
    });
    hidden.edit("policy/core/provenance.cedar", |text| {
        text.replace(
            "unless { context has grant };",
            "unless { context has grant || principal is Device };",
        )
    });
    hidden.must_fail("core-narrows-by-entity-type", "core.provenance");

    // **The action, which is not exempt wholesale.** `core.provenance`
    // rescoped to one action gives every write a pass with no grant at all,
    // and exempt, it passes — every provenance case uses `Read`, so case
    // coverage cannot see it.
    let scoped = Scratch::new("core-narrows-action");
    scoped.edit("policy/core/provenance.cedar", |text| {
        text.replace("  action,\n", "  action == Action::\"Read\",\n")
    });
    scoped.must_fail("core-narrows-by-entity-type", "core.provenance");

    // **And a bare marker with no reason excuses nothing.** The sibling hatch in
    // `xtask/src/limits.rs` has exactly this test; without this one,
    // `references_marked`'s requirement that the marker carry a reason is
    // asserted by nothing — `§19.5` in the escape hatch.
    let bare = Scratch::new("core-narrows-bare-marker");
    bare.edit("policy/core/provenance.cedar", |text| {
        text.replace(
            "@id(\"core.provenance\")",
            "// nc:narrows:\n@id(\"core.provenance\")",
        )
    });
    bare.edit("policy/core/provenance.cedar", |text| {
        text.replace(
            "unless { context has grant };",
            "unless { context has grant || principal is Device };",
        )
    });
    bare.must_fail("core-narrows-by-entity-type", "core.provenance");
}

/// A guard that fails open is reported, in every spelling of the path.
///
/// **`guard-fails-open` has its test here.** It is the check that
/// holds `§14.9`'s invariants against the decision point that forgets the
/// schema — a `forbid` whose condition errors is *skipped*, so a rule
/// written `context.grant.source != "policy"` stops being asked the moment the
/// attribute is absent and whatever permit is behind it answers. The shipped
/// pack passes it, so against the pack alone every way it could be unwired
/// looks exactly the same as working.
///
/// **And all four spellings are wired.** Cedar writes the same dereference
/// as `a.b.c`, `a.b["c"]`, `a["b"].c` and `a["b"]["c"]`, and its renderer keeps
/// whichever the author used. `attributes_read` looking for the second segment
/// only after a `.` brings the two ending in an index back with no second
/// segment, and the guard on `source` is never asked about — measured that way,
/// with the `has` guard deleted and the read written `context.grant["source"]`:
/// no `guard-fails-open`, and `check passed`.
#[test]
fn a_guard_that_fails_open_is_reported_however_the_path_is_written() {
    for (about, dereference) in [
        ("dotted", "context.grant.source"),
        ("indexed at the end", "context.grant[\"source\"]"),
        ("indexed in the middle", "context[\"grant\"].source"),
        ("indexed throughout", "context[\"grant\"][\"source\"]"),
    ] {
        let scratch = Scratch::new(&format!("guard-open-{}", about.replace(' ', "-")));
        // The guard taken away and nothing else: the policy still reads the same
        // attribute, and now errors instead of refusing when it is absent.
        scratch.edit("policy/core/grants_expire.cedar", |text| {
            text.replace(
                "(!(context.grant has source) || context.grant.source != \"policy\")",
                &format!("{dereference} != \"policy\""),
            )
        });
        scratch.must_fail("guard-fails-open", "core.grants_expire");
    }

    // And the shipped spelling of that guard is what keeps the pack clean, so
    // the four above are not passing because something else is broken.
    let clean = Scratch::new("guard-open-shipped");
    assert!(
        // **Empty, not merely free of one check.** `all(… != …)` over an empty
        // list is true however the gate behaves, and this closure, measured,
        // never sees an element. The shipped pack reports *nothing*, which is
        // both stronger and what the sentence below already claims.
        clean.problems().is_empty(),
        "the pack as shipped reports a problem: {:#?}",
        clean.problems()
    );
}

/// A denial nothing would otherwise permit proves nothing about its guard.
///
/// **`guard-unprovable` is named by this test**, and without it deleting the
/// check leaves `cargo xtask check` green — outside this test,
/// `grep -rn 'guard-unprovable' crates xtask` finds only the check's definition
/// in `crates/nc-policy/src/lib.rs` and a sentence in `xtask/canaries/README.md`.
/// It is the other half of `guard-fails-open`: that one says a guard must
/// refuse when the attribute is absent, and this one says the *case* that
/// proves it has to be a request the pack would otherwise permit. A denial
/// case pointed at a resource nothing permits is denied by Cedar's default, so
/// removing the whole policy changes nothing and every proof about it passes by
/// saying nothing.
///
/// `Content::"recipe"` belongs to `carla` and is `private`: the family pack's
/// `Read` permit stops at `realm`, and its owner permit needs the principal to be
/// the owner — so nothing in the pack reaches `ana` reading it, and the denial
/// stands with or without the policy the case names.
#[test]
fn a_denial_case_that_nothing_would_permit_is_refused() {
    let scratch = Scratch::new("guard-unprovable");
    scratch.edit("policy/core/tests/grants_expire.toml", |text| {
        text.replace(
            r#"resource = 'Content::"photo"'"#,
            r#"resource = 'Content::"recipe"'"#,
        )
    });
    scratch.must_fail("guard-unprovable", "grants_expire");
}

/// A case that exists but names a different policy proves nothing about this one.
///
/// **Two questions, not one.** The denial-coverage gate asks whether a policy
/// has a case; `guards_fail_closed` needs a case whose `because` names *this*
/// policy, and returning in silence when it finds none, retyping one `because`
/// leaves coverage green, every guard in the policy unproven, and the only
/// trace `inspected` getting smaller.
///
/// `core.grants_expire` reads `grant.valid_until`. Point its case at the sibling
/// policy in the same file and coverage still counts a case for the file, while
/// nothing is left to prove the guard.
#[test]
fn a_case_that_names_another_policy_leaves_the_guard_unproven() {
    let scratch = Scratch::new("guard-has-no-case");
    scratch.edit("policy/core/tests/grants_expire.toml", |text| {
        text.replacen(
            "because = \"core.grants_expire\"",
            "because = \"core.grant_not_expired\"",
            1,
        )
    });
    scratch.must_fail("guard-has-no-case", "grants_expire");
}

/// A confusable identifier is a rule that protects nothing.
///
/// **`pack-warns` has its case here, and it is the only reader of Cedar's own
/// warnings.** Measured without it, the loop replaced
/// with `let _ = result.validation_warnings().count();` — the policy step
/// byte-identical to a clean run, down to `check passed`.
///
/// And the guard is live rather than decorative: `Content::"di\u{430}ry"` with a
/// Cyrillic `а` is a *different entity* from `Content::"diary"`, so a `forbid`
/// written about it refuses nothing, its denial case still passes because Cedar
/// denies by default, and every printed count is unchanged. That is the class
/// the rest of this crate is structurally blind to: an impossible `forbid` is
/// caught indirectly, because `check_the_denial_is_not_free` needs an answer to
/// flip; a confusable one is not, because nothing about the pack changes.
///
/// `nc-catalog-compiler` refuses a bidi override in a *message*. This is the
/// file that says who may read it.
#[test]
fn a_confusable_identifier_in_a_policy_is_refused() {
    let scratch = Scratch::new("pack-warns");
    scratch.edit("policy/packs/family/permissions.cedar", |text| {
        format!(
            // **The probe carries its own narrowing reason.** It is a pack
            // `forbid` scoped to one named resource, which is exactly what the
            // pack-floor rule reports — so without the marker that rule fires
            // first and this test measures it instead of Cedar's warning. The
            // marker is what a deliberate narrowing looks like, and using one
            // here keeps the fixture aimed at the thing it is named for.
            "{text}\n// nc:narrows: `resource ==` names Content::\"di\u{430}ry\" on purpose \
             — this policy is a probe for Cedar's own confusable-identifier warning \
             and decides nothing.\n@id(\"family.confusable_probe\")\nforbid (\n  principal,\n  \
                 action,\n  resource == Content::\"di\u{430}ry\"\n);\n"
        )
    });
    scratch.must_fail("pack-warns", "di\u{430}ry");
}

#[test]
fn a_policy_with_no_denial_test_is_refused() {
    // `§14.9`'s seventh invariant, and the reason this crate exists: a policy
    // without a negative test does not enter the monorepo.
    let scratch = Scratch::new("no-denial");
    scratch.edit("policy/core/tests/quorum.toml", |text| {
        let cut = text
            .find("[[case]]\nname = \"deleting without quorum")
            .expect("the case is there");
        let keep = text
            .find("[[case]]\nname = \"and with the confirmation")
            .expect("the control is there");
        format!("{}{}", &text[..cut], &text[keep..])
    });
    // **And the pack's case for the same rule**, which shows the core's quorum
    // winning over a guardian's prune permit and is a denial test for it too:
    // with it left in, the rule is not without one and nothing is refused.
    scratch.edit("policy/packs/family/tests/realm.toml", |text| {
        let at = text
            .find("name = \"and not without one, which the core decides")
            .expect("the case is there");
        let opens = text[..at].rfind("[[case]]").expect("its header");
        let ends = text[at..]
            .find("\n\n")
            .map_or(text.len(), |offset| at + offset + 1);
        format!("{}{}", &text[..opens], &text[ends..])
    });
    scratch.must_fail("no-denial-test", "core.destructive_needs_quorum");
}

#[test]
fn a_denial_that_would_happen_anyway_is_refused() {
    // The vacuous case this whole format exists to make impossible. Cedar denies
    // by default, so a denial case passes against an empty pack unless something
    // proves the named policy is what caused it.
    //
    // Broken by pointing the guardian permit at a group nobody is in: the request
    // is then refused for want of a permit, and the case claiming
    // `core.no_self_escalation` refused it is no longer testing anything.
    let scratch = Scratch::new("free-denial");
    scratch.edit("policy/packs/family/permissions.cedar", |text| {
        text.replace(
            "principal in Group::\"role.guardian\",",
            "principal in Group::\"role.nobody\",",
        )
    });
    scratch.must_fail("denial-proves-nothing", "core.no_self_escalation");
}

#[test]
fn a_case_whose_expectation_does_not_match_cedar_is_refused() {
    // **The one assertion that gives every case in this pack its meaning** —
    // that Cedar's answer is the answer the case claims — and this holds it in
    // place: without it, deleting the whole `if got != expected` block leaves
    // the full suite green and `check passed`.
    //
    // Broken by flipping a shipped case's expectation, which is the smallest
    // edit that makes Cedar and the file disagree.
    let scratch = Scratch::new("wrong-decision");
    scratch.edit("policy/packs/family/tests/permissions.toml", |text| {
        text.replace(
            "resource = 'Content::\"photo\"'\ndecision = \"allow\"",
            "resource = 'Content::\"photo\"'\ndecision = \"deny\"",
        )
    });
    scratch.must_fail(
        "wrong-decision",
        "a child of the house reads what the house shares",
    );
}

#[test]
fn a_boundary_case_with_no_control_showing_the_permit_fires_is_refused() {
    // And the `boundary-without-a-control` loop, held the same way: without
    // this, deleting it entirely leaves `check passed`. A wired check can
    // regress out of existence without a red light.
    //
    // Broken by moving the boundary case onto a verb no `allow` case pairs it
    // with, which leaves it a denial that would happen against an empty pack.
    // Two case files show this permit allowing a `Read`, which is the pack being
    // well covered rather than an obstacle: the point is the *pair*, so the
    // smallest honest break is one the pair cannot follow.
    let scratch = Scratch::new("no-control");
    scratch.edit("policy/packs/family/tests/permissions.toml", |text| {
        text.replace(
            "action = \"Read\"\nresource = 'Content::\"diary\"'",
            "action = \"Write\"\nresource = 'Content::\"diary\"'",
        )
    });
    scratch.must_fail(
        "boundary-without-a-control",
        "family.a_member_reads_shared_content",
    );
}

#[test]
fn a_boundary_case_aimed_at_a_resource_the_permit_cannot_reach_is_refused() {
    // **A control matched on the action alone proves nothing.** Matched that
    // way, retargeting the boundary case at an entity type the permit can never
    // apply to keeps `10 policy(ies) each have a denial test` green — and the
    // permit's whole sensitivity condition can then be deleted, opening
    // `private` and `restricted` content to the entire household with
    // acceptance criterion 11 reporting satisfied.
    let scratch = Scratch::new("wrong-resource-type");
    scratch.edit("policy/packs/family/tests/permissions.toml", |text| {
        text.replace(
            "resource = 'Content::\"diary\"'\ndecision = \"deny\"",
            "resource = 'Feature::\"gallery\"'\ndecision = \"deny\"",
        )
    });
    scratch.must_fail(
        "boundary-without-a-control",
        "family.a_member_reads_shared_content",
    );
}

/// The flag's claim is judged against every matching control, not the first.
///
/// **The loop that judges every matching control is held here**:
/// appending `.take(1)` to the filter — which is exactly a `find` — leaves
/// every other `nc-policy` test passing.
///
/// The hole is reachable, not theoretical. `cases::load` sorts by path, so
/// `policy/core/tests/agent_authority.toml` precedes `quorum.toml`, which holds
/// the control that decides this case today. Put a second matching control in
/// the earlier file and a `find` selects *that* one — and the flagged
/// case, whose refusal is really `core.irreversible_needs_a_human` firing on
/// `via_agent` with no human, keeps its pairing exemption with no verdict at
/// all. That exemption is the one per-case opt-out of the rule that makes
/// acceptance criterion 11 mean anything.
#[test]
fn the_flag_is_judged_against_every_control_and_not_the_first() {
    let scratch = Scratch::new("flag-every-control");
    scratch.edit("policy/core/tests/agent_authority.toml", |text| {
        format!(
            "{text}\n\n[[case]]\nname = \"an earlier matching control\"\n             principal = 'User::\"ana\"'\naction = \"Write\"\n             resource = 'Content::\"diary\"'\ndecision = \"allow\"\n             because = \"family.an_owner_governs_their_own_content\"\n"
        )
    });
    scratch.edit("policy/packs/family/tests/permissions.toml", |text| {
        text.replace(
            "name = \"an owner governs her own content and does not grant over it\"",
            "name = \"an owner governs her own content and does not grant over it\"\n             context = { via_agent = true }",
        )
    });
    scratch.must_fail(
        "boundary-is-not-the-action",
        "an owner governs her own content",
    );
}

/// The flag path's control is of the case's own resource type too.
///
/// **One rule written twice, and each half has a case.** The filter requires
/// the control to match the case on *both* entity types, and without this
/// deleting the resource clause leaves the whole workspace green, while its
/// principal sibling on the next line is held by the test below.
///
/// The direction is a widening — more controls judged means the check fires
/// more — so the failure it allows is a false `boundary-is-not-the-action`
/// against a permit that cannot reach the case's resource type, not a hole. It
/// is still, without this, half a rule nothing reads.
#[test]
fn the_flag_paths_control_is_of_the_cases_own_resource_type() {
    let scratch = Scratch::new("flag-control-resource");
    scratch.edit("policy/core/tests/agent_authority.toml", |text| {
        format!(
            "{text}\n\n[[case]]\nname = \"a control on another resource type\"\n             principal = 'User::\"ana\"'\naction = \"Grant\"\n             resource = 'User::\"carla\"'\ndecision = \"allow\"\n             because = \"family.an_owner_governs_their_own_content\"\n"
        )
    });
    let problems = scratch.problems();
    assert!(
        problems
            .iter()
            .any(|problem| problem.what.contains("a control on another resource type")),
        "the planted control was not reported at all, so this proves nothing: {problems:#?}"
    );
    assert!(
        !problems
            .iter()
            .any(|problem| problem.check == "boundary-is-not-the-action"),
        "a control on `Feature` was paired with a `Content` boundary case, so the flag \
         path judged the claim against a resource type the permit cannot reach: \
         {problems:#?}"
    );
}

#[test]
/// The flag path's control has to be of the case's own principal type.
///
/// **The second copy of the pairing rule, held
/// here.** `the_action_is_the_boundary` has its own control lookup, and
/// deleting `entity_type(&other.principal) == entity_type(&case.principal)`
/// from it leaves every other `nc-policy` test green — the sibling test below
/// drives the *other* closure.
///
/// It cannot be given a case while the lookup uses `find`: `cases::load` sorts
/// by path, so a control written in the pack's own file is never the one
/// selected. With the lookup judging every matching control, an `Agent`
/// principal allowed by a permit whose other cases are all `User` is what
/// distinguishes the conjunct.
fn the_flag_paths_control_is_of_the_cases_own_principal_type() {
    let scratch = Scratch::new("flag-control-principal");
    scratch.edit("policy/core/tests/quorum.toml", |text| {
        format!(
            "{text}\n\n[[case]]\nname = \"agent control\"\n             principal = 'Agent::\"ana-assistant\"'\naction = \"Grant\"\n             resource = 'Content::\"notice\"'\ndecision = \"allow\"\n             because = \"family.an_owner_governs_their_own_content\"\n"
        )
    });
    // **The assertion is an absence, and it has to be.** The fixture is itself a
    // wrong decision, so `must_fail("wrong-decision", …)` passes whether or not
    // the conjunct is there, and asserting it proves nothing. What the
    // conjunct changes is the *second* problem: without
    // it, an `Agent` control is paired with a `User` case and the flag path
    // reports `boundary-is-not-the-action` against a permit that never reaches
    // an `Agent` at all.
    let problems = scratch.problems();
    assert!(
        problems
            .iter()
            .any(|problem| problem.check == "wrong-decision"),
        "the fixture stopped being a wrong decision, so this proves nothing: {problems:#?}"
    );
    assert!(
        !problems
            .iter()
            .any(|problem| problem.check == "boundary-is-not-the-action"),
        "an `Agent` allow-case was paired with a `User` boundary case, so the flag \
         path judged the claim against a control the permit cannot reach: {problems:#?}"
    );
}

/// A control is an `allow` case. A denial naming the permit is not one.
///
/// **The pairing rule's own decision clause, in both
/// copies.** `other.decision == Expected::Allow` appears twice — once
/// in `coverage`'s `paired` closure and once in `the_flag_means_what_it_says`'
/// control lookup — and deleting it from either leaves the rest of `nc-policy`
/// green. This test is the `coverage` copy, which is the dangerous direction:
/// without the clause, a boundary case pairs with another **denial** naming the
/// same permit, and the requirement that the permit somewhere *allows* the same
/// action stops requiring an allow at all. A permit that never fires would then
/// satisfy acceptance criterion 11 on a pair of denials, which is the exact
/// failure the pairing rule exists to close.
///
/// **The assertion is on this check and not on "some problem exists".** Turning
/// the pack's allow case into a denial and asserting `!problems.is_empty()` is
/// true either way — the edit raises a `wrong-decision` on the flipped case
/// whatever the closure does. What moves
/// with the clause is whether `boundary-without-a-control` fires **for the case
/// that lost its only control**, so that is what is named.
#[test]
fn a_denial_naming_the_permit_is_not_the_control_a_boundary_case_needs() {
    let scratch = Scratch::new("control-must-allow");
    // The only `allow` case naming `family.an_owner_governs_their_own_content`
    // on the verb `and another member does not` is denied for — the pack's two
    // other controls for that permit are a `Share` and a `Delete`, and the
    // pairing rule requires the same verb unless the case says the verb is what
    // it is about. Flipped to a denial it still names the permit, still matches
    // the action, and still matches both entity types: everything the closure
    // asks except the one clause under test.
    scratch.edit("policy/packs/family/tests/permissions.toml", |text| {
        text.replace(
            "name = \"an owner writes her own content\"\nprincipal = 'User::\"ana\"'\n\
             action = \"Write\"\nresource = 'Content::\"diary\"'\ndecision = \"allow\"",
            "name = \"an owner writes her own content\"\nprincipal = 'User::\"ana\"'\n\
             action = \"Write\"\nresource = 'Content::\"diary\"'\ndecision = \"deny\"",
        )
    });
    scratch.must_fail(
        "boundary-without-a-control",
        "and another member does not, however ordinary the request looks",
    );
}

/// And the flag path's control has to be an `allow` case too.
///
/// **The second copy of the clause above.** `the_flag_means_what_it_says` runs
/// its own control lookup, and deleting `other.decision == Expected::Allow`
/// from it leaves the rest green as well. The direction here is a widening —
/// more cases judged as controls means the check fires more often — so what it
/// allows is a false `boundary-is-not-the-action` against a case that is
/// correct, not a hole. It is still half a rule nothing else reads, and the
/// sibling tests above take the same shape for the same reason.
///
/// The planted case is a denial that names the permit, on the case's own two
/// entity types, carrying a verb the permit does not allow. Read as a control
/// it says "swap the verb for `Revoke` and it is still denied", which is true
/// of every request the permit never reached — and the flag's claim is about
/// the verbs the permit *does* allow.
#[test]
fn the_flag_paths_control_is_an_allow_case() {
    let scratch = Scratch::new("flag-control-allows");
    scratch.edit("policy/core/tests/quorum.toml", |text| {
        format!(
            "{text}\n\n[[case]]\nname = \"a denial that names the permit\"\n\
             principal = 'User::\"bruno\"'\naction = \"Revoke\"\n\
             resource = 'Content::\"photo\"'\ndecision = \"deny\"\n\
             because = \"family.an_owner_governs_their_own_content\"\n"
        )
    });
    let problems = scratch.problems();
    // **The planted case reached the judgement**, or the absence below is an
    // absence of everything. A denial cannot name a permit as what decided it,
    // so `wrong-policy` is what this case is: the guard is that it was read at
    // all, on the same path the control lookup walks.
    assert!(
        problems
            .iter()
            .any(|problem| problem.what.contains("a denial that names the permit")),
        "the planted case was not reported at all, so this proves nothing: {problems:#?}"
    );
    assert!(
        !problems
            .iter()
            .any(|problem| problem.check == "boundary-is-not-the-action"),
        "a denial naming the permit was taken for a control showing the permit fires, \
         so `an owner governs her own content and does not grant over it` was judged \
         against a verb the permit never allowed and reported as a lie: {problems:#?}"
    );
}

#[test]
fn a_boundary_case_aimed_at_a_principal_the_permit_cannot_reach_is_refused() {
    // **The other half of the same conjunct.** The pairing rule says "the same
    // action, and the same two entity types" — and without this only the
    // resource type is held: deleting
    // `entity_type(&other.principal) == entity_type(&case.principal)` leaves
    // `cargo test -p nc-policy` green and the full gate reporting
    // `N policy(ies) each have a denial test`.
    //
    // Two halves of one rule satisfied by different occurrences is a recurring
    // shape; here it is one half satisfied and the other unwritten.
    let scratch = Scratch::new("wrong-principal-type");
    scratch.edit("policy/packs/family/tests/permissions.toml", |text| {
        text.replace(
            "principal = 'User::\"carla\"'\naction = \"Write\"",
            "principal = 'Agent::\"ana-assistant\"'\naction = \"Write\"",
        )
    });
    scratch.must_fail(
        "boundary-without-a-control",
        "family.an_owner_governs_their_own_content",
    );
}

#[test]
fn a_rule_caused_denial_cannot_be_called_a_missing_permit() {
    // The other direction. `no_permit_matched` is for the boundary of a permit;
    // using it for a denial a `forbid` caused would hide which rule fired, and
    // the journal records exactly that (`§14.9`).
    let scratch = Scratch::new("mislabelled");
    scratch.edit("policy/core/tests/fail_closed.toml", |text| {
        text.replace(
            "because = \"core.fail_closed\"\ncontext = { policy_epoch = 0 }",
            "no_permit_matched = \"family.a_member_reads_shared_content\"\n\
             context = { policy_epoch = 0 }",
        )
    });
    // The case the message names, and not the file it lives in: the path
    // `fail_closed.toml` is in `problem.file` and would answer for a sentence
    // that had stopped naming any case at all.
    scratch.must_fail_saying(
        "denial-was-not-a-missing-permit",
        "a request carrying no epoch is refused, however well formed it is",
    );
}

/// `no_permit_matched` names a permit, and a `forbid` is not one.
///
/// **The arm that says so is reached by this test.** It is the only thing
/// stopping a `forbid` from being credited with a denial test
/// through `no_permit_matched` — and without this, replacing it with an early
/// return leaves `cargo xtask check` byte-identical to a clean run. Deleting it
/// outright is caught only by clippy's `match_same_arms`, which is a lint and
/// not a case.
///
/// `a_rule_caused_denial_cannot_be_called_a_missing_permit` is the other
/// direction: that one is about a case *mislabelled* as a boundary; this one is
/// about the name in the label being a rule that forbids.
/// A conjunct no case sits on the boundary of is reported.
///
/// **`condition-untested`'s negative case, which exists
/// because `guards_a_dereference` is narrow.** Excusing `!(x has y) || …` whole
/// puts a guarded conjunct outside the mutation, while `guards_fail_closed`
/// claims every unguarded one. Four attempts at a case under that excuse
/// produce `guard-fails-open`, `guard-fails-open`, `wrong-decision`, and then
/// nothing at all — `must_fail`'s own floor reporting "the pack was broken on
/// purpose and every check accepted it".
///
/// The conjunct here is guarded the way its neighbour is, and implied by it:
/// `<= 0` already gives `< 1` for an integer, so removing it flips no case.
#[test]
fn a_condition_no_case_decides_is_reported() {
    let scratch = Scratch::new("condition-untested");
    scratch.edit("policy/core/fail_closed.cedar", |text| {
        text.replace(
            "context.policy_epoch <= 0 };",
            "context.policy_epoch <= 0 }
when { !(context has policy_epoch) || context.policy_epoch < 1 };",
        )
    });
    scratch.must_fail("condition-untested", "core.fail_closed");
}

/// A comparison no case sits on both sides of is reported.
///
/// **`branch-untested`'s negative case.** Without it, `branch-untested` can be
/// switched off for every policy this repository ships with `check`
/// byte-identical:
/// `if !noticed && !name.starts_with("core.") && !name.starts_with("family.")`
/// leaves `40 case(s) … 38 constraint(s) and condition(s) are pinned` unmoved,
/// because the counter is below the verdict.
///
/// Two other shapes do not reach it, for reasons worth keeping: an unguarded
/// dereference is claimed by `guards_fail_closed` first, and a *redundant*
/// conjunct is claimed by `condition-untested`, whose removal is also unnoticed.
/// The shape that reaches this verdict is a comparison **inside** a conjunct
/// beside a term that masks it: the added disjunct here is unsatisfiable, so
/// flipping `== 5` to `!= 5` changes no answer, while removing the whole
/// conjunct *is* noticed — which is what keeps `condition-untested` silent and
/// makes this the only check that fires.
#[test]
fn a_branch_no_case_decides_is_reported() {
    let scratch = Scratch::new("branch-untested");
    scratch.edit("policy/core/fail_closed.cedar", |text| {
        text.replace(
            "context.policy_epoch <= 0 };",
            "context.policy_epoch <= 0 || \
             (context.policy_epoch <= 0 && context.policy_epoch == 5) };",
        )
    });
    scratch.must_fail("branch-untested", "core.fail_closed");
}

/// Every file the pack is loaded from is refused when it cannot be read.
///
/// **Every check is watched firing, not only produced.** `nc-catalog-compiler`
/// has a `must_fail` case for every one of its own, and these are the
/// `nc-policy` checks no other case reaches. A verdict nothing has watched fire
/// is a verdict nobody knows can.
#[test]
fn a_pack_that_cannot_be_read_is_refused_by_name() {
    for (named, body, check) in [
        (
            "policy/schema.cedarschema",
            "this is not a schema {{{",
            "schema-malformed",
        ),
        ("policy/world.json", "{ not json", "world-malformed"),
        (
            "policy/core/fail_closed.cedar",
            "forbid ( ( (",
            "policy-malformed",
        ),
    ] {
        let scratch = Scratch::new(check);
        scratch.edit(named, |_| body.to_owned());
        scratch.must_fail(check, named.rsplit('/').next().expect("a file name"));
    }
}

/// A file that exists and cannot be read is refused by name, not by silence.
///
/// The `*-unreadable` verdicts are the other half of the malformed ones: the
/// file is there, so no "missing" rule fires, and the contents never arrive. A
/// pack loaded without its schema, world or a policy answers *something* for
/// every case — which is how one unreadable file becomes seventy-three findings
/// about the wrong thing.
#[test]
fn a_file_that_cannot_be_read_is_refused_by_name() {
    for (named, check) in [
        ("policy/schema.cedarschema", "schema-unreadable"),
        ("policy/world.json", "world-unreadable"),
        ("policy/core/fail_closed.cedar", "policy-unreadable"),
        // **The fourth, whose identity has one other occurrence in the
        // repository: its own emission.** Coverage is attributed by a policy's
        // `@id` wherever the case is written, so an unreadable case file takes
        // its policies' denial tests with it — and §14.9's seventh invariant
        // then fails for rules that are in fact tested. Without this,
        // suppressing the identity leaves `check passed`.
        ("policy/core/tests/fail_closed.toml", "case-file-unreadable"),
        // And the pack's version, without which every realm of its profile is
        // held to nothing.
        ("policy/packs/family/pack.toml", "pack-manifest-unreadable"),
    ] {
        let scratch = Scratch::new(check);
        let path = scratch.root.join(named);
        nc_platform::testing::make_unreadable(&path)
            .expect("this test wrote the copy it is shutting");
        let said = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            scratch.must_fail(check, named.rsplit('/').next().expect("a file name"));
        }));
        // Readable again before the assertion is re-raised, so a failure here
        // does not leave a directory `Scratch`'s own `Drop` cannot remove.
        let _ = nc_platform::testing::make_private(&path);
        if let Err(panic) = said {
            std::panic::resume_unwind(panic);
        }
    }
}

/// A context value with no Cedar form is refused, by name.
///
/// **The other identity, asked for here.** `context-value-unconvertible`
/// otherwise appears exactly once in the repository — the line that emits it —
/// so dropping it before `load` returns leaves `cargo xtask check`
/// byte-identical. A context the runner cannot build is a case whose verdict
/// means nothing, and silently skipping it is how a boundary claim gets bought
/// without a check.
///
/// A table is how this format writes an extension value (`{ datetime = … }`) or
/// an entity reference (`{ entity = … }`); a table that is neither has no Cedar
/// value at all.
#[test]
fn a_context_value_with_no_cedar_form_is_refused_by_name() {
    let scratch = Scratch::new("context-unconvertible");
    scratch.edit("policy/packs/family/tests/permissions.toml", |text| {
        text.replace(
            "grant = { entity = 'Grant::\"g.current\"' }",
            "grant = { entity = 'not a uid at all' }",
        )
    });
    scratch.must_fail("context-value-unconvertible", "grant");
}

#[test]
fn a_forbid_cannot_be_named_as_the_permit_that_did_not_match() {
    let scratch = Scratch::new("forbid-as-permit");
    scratch.edit("policy/packs/family/tests/permissions.toml", |text| {
        text.replace(
            "no_permit_matched = \"family.a_member_reads_shared_content\"",
            "no_permit_matched = \"core.fail_closed\"",
        )
    });
    scratch.must_fail("not-a-permit", "core.fail_closed");
}

#[test]
fn a_case_naming_a_policy_that_does_not_exist_is_refused() {
    let scratch = Scratch::new("unknown-policy");
    scratch.edit("policy/packs/family/tests/permissions.toml", |text| {
        text.replace(
            "no_permit_matched = \"family.a_member_reads_shared_content\"",
            "no_permit_matched = \"family.a_rule_nobody_wrote\"",
        )
    });
    scratch.must_fail("unknown-policy", "family.a_rule_nobody_wrote");
}

#[test]
fn a_case_that_names_the_wrong_policy_is_refused() {
    let scratch = Scratch::new("wrong-policy");
    scratch.edit("policy/core/tests/provenance.toml", |text| {
        text.replace(
            "because = \"core.provenance\"",
            "because = \"core.fail_closed\"",
        )
    });
    scratch.must_fail("wrong-policy", "core.fail_closed");
}

#[test]
fn a_policy_with_no_id_is_refused() {
    // Without an `@id`, Cedar names a policy by position — so the id a journal
    // entry recorded today would name a different rule tomorrow.
    let scratch = Scratch::new("no-id");
    scratch.edit("policy/core/fail_closed.cedar", |text| {
        text.replace("@id(\"core.fail_closed\")\n", "")
    });
    scratch.must_fail("policy-without-an-id", "fail_closed.cedar");
}

#[test]
fn two_policies_sharing_an_id_are_refused() {
    let scratch = Scratch::new("duplicate-id");
    scratch.edit("policy/core/provenance.cedar", |text| {
        text.replace("@id(\"core.provenance\")", "@id(\"core.fail_closed\")")
    });
    scratch.must_fail("duplicate-policy-id", "core.fail_closed");
}

#[test]
fn a_policy_the_schema_does_not_support_is_refused() {
    // Validation, not just parsing. A policy that mentions an attribute the
    // schema has no name for can never match, and Cedar says nothing at run time
    // — it simply never fires, which reads exactly like a rule being obeyed.
    let scratch = Scratch::new("invalid");
    scratch.edit("policy/core/fail_closed.cedar", |text| {
        text.replace("context.policy_epoch", "context.policy_epcoh")
    });
    scratch.must_fail("pack-invalid", "policy/");
}

#[test]
fn a_case_whose_request_the_schema_refuses_is_reported() {
    let scratch = Scratch::new("bad-request");
    scratch.edit("policy/core/tests/provenance.toml", |text| {
        text.replace(
            "resource = 'Content::\"photo\"'",
            "resource = 'Folder::\"household\"'",
        )
    });
    // The case the message names, for the reason given beside
    // `denial-was-not-a-missing-permit`.
    scratch.must_fail_saying(
        "malformed-request",
        "a request that names no grant is refused even when a pack would permit it",
    );
}

/// The outsider the deferral case turns on is an outsider.
///
/// **The case is the pin, and this pins the fixture it rests on.**
/// `policy/packs/family/tests/permissions.toml` names
/// `User::"mallory"` reading what the household shares, and records the `allow`
/// as the gap that `reference/identity-and-authorization.md §3` leaves —
/// *"the day a core `forbid` arrives, this case flips and somebody decides on
/// purpose"*. Move `mallory` into `Realm::"home"` and the whole of
/// `cargo xtask check` is byte-identical to a clean run: the case then asks the
/// same question as `a child of the house reads what the house shares`, and a
/// future `forbid` would not flip it.
///
/// So the household this case is *outside* of is asserted here, from the world
/// the cases are evaluated against.
#[test]
fn the_outsider_the_deferral_rests_on_belongs_to_another_realm() {
    let world = std::fs::read_to_string(repository().join("policy/world.json"))
        .expect("the world is readable");
    let world: serde_json::Value = serde_json::from_str(&world).expect("the world is JSON");
    let entities = world.as_array().expect("the world is a list of entities");
    let realms = |named: &str| -> Vec<String> {
        entities
            .iter()
            .find(|entity| entity["uid"]["id"] == named)
            .unwrap_or_else(|| panic!("`{named}` is not in the world"))["parents"]
            .as_array()
            .map(|parents| {
                parents
                    .iter()
                    .filter(|parent| parent["type"] == "Realm")
                    .filter_map(|parent| parent["id"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    let outsider = realms("mallory");
    assert!(
        !outsider.is_empty(),
        "`mallory` belongs to no realm at all, so the case that calls them an outsider \
         is about nothing"
    );
    for household in realms("ana") {
        assert!(
            !outsider.contains(&household),
            "`mallory` shares `{household}` with a member of the household, so the case \
             written to pin the cross-realm gap asks the same question as the one above \
             it and a future `forbid` would not flip it"
        );
    }
}

/// A case about somebody the world does not hold is refused.
///
/// **A `no_permit_matched` case with a ghost entity is a test of Cedar's
/// default.** It is the only kind of denial test criterion 11 accepts for
/// a `permit`, and a nonexistent entity has no attributes — so the permit's
/// condition errors, Cedar skips it, and *"denied because nothing permitted
/// it"* is true of any request at all. Without the `absent-entity` check, the
/// gate answers one character changed in the family pack's `permissions.toml`,
/// `Content::"vault"` to `Content::"vault-typo"`, with
/// `ok 40 case(s) in 8 file(s) decide as written, and 38 constraint(s) and condition(s) are pinned by one of them`,
/// line for line what a clean run prints. With a clause-count-preserving
/// widening planted as well, in the permit that case exists to hold, the two
/// together stay green, while `restricted` content becomes readable by every
/// household member and by the other realm's user — and the widening *alone* is
/// red, which is what makes the typo the finding.
#[test]
fn a_case_about_somebody_the_world_does_not_hold_is_refused() {
    let scratch = Scratch::new("ghost");
    scratch.edit("policy/packs/family/tests/permissions.toml", |text| {
        text.replace(
            "resource = 'Content::\"vault\"'",
            "resource = 'Content::\"vault-typo\"'",
        )
    });
    scratch.must_fail("absent-entity", "policy/world.json");
    // And the principal side, which is the same rule read from the other end.
    let scratch = Scratch::new("ghost-principal");
    scratch.edit("policy/packs/family/tests/permissions.toml", |text| {
        text.replace(
            "principal = 'User::\"ana\"'",
            "principal = 'User::\"ana-typo\"'",
        )
    });
    scratch.must_fail("absent-entity", "policy/world.json");
}

#[test]
fn an_empty_pack_is_refused_rather_than_passing_everything() {
    // The floor. Cedar denies by default, so a pack that loaded nothing would
    // satisfy every denial case in the suite — the `§19.5` failure at the scale
    // of the whole deliverable.
    let scratch = Scratch::new("empty");
    for policy in [
        "policy/core/fail_closed.cedar",
        "policy/core/provenance.cedar",
        "policy/core/grants_expire.cedar",
        "policy/core/no_self_escalation.cedar",
        "policy/core/quorum.cedar",
        "policy/core/chain_depth.cedar",
        "policy/core/agent_authority.cedar",
        "policy/packs/family/accounts.cedar",
        "policy/packs/family/permissions.cedar",
        "policy/packs/family/realm.cedar",
    ] {
        std::fs::remove_file(scratch.root.join(policy)).expect("the copy is writable");
    }
    // **Every policy, and the list is checked.** A new Cedar file that this
    // list does not name leaves the pack non-empty, so this fails for a reason
    // that has nothing to do with what it tests — which is how it reads when
    // a policy file is added.
    //
    // **All of `policy/`, not `policy/core/`.** Reading the core's directory
    // alone, a pack's new file is the one kind this could not name: left off
    // the list above, it keeps the pack non-empty and the assertion below fails
    // saying that nothing reported `empty-pack` — the reason this check exists
    // to replace with the file's name.
    let mut left: Vec<String> = Vec::new();
    let mut unread = vec![scratch.root.join("policy")];
    while let Some(directory) = unread.pop() {
        for entry in std::fs::read_dir(&directory).expect("readable").flatten() {
            let path = entry.path();
            if path.is_dir() {
                unread.push(path);
            } else if path.extension().is_some_and(|kind| kind == "cedar") {
                left.push(
                    path.strip_prefix(&scratch.root)
                        .expect("under the copy")
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
    assert!(
        left.is_empty(),
        "the pack is not empty, so this tests something else: {left:?}"
    );
    scratch.must_fail("empty-pack", "policy/core/");
}

/// A core invariant still denies when the decision point forgets the schema.
///
/// **Cedar skips a policy whose condition errors, and answers from the
/// rest.** A core `forbid` that dereferences a context attribute directly does
/// not refuse a request missing one — the request is *allowed*, by whatever
/// permit remains. Measured with the shipped pack unguarded and a request with
/// no `policy_epoch`:
///
/// ```text
/// with the schema:    refused: while parsing context, expected the record to
///                     have an attribute `policy_epoch`, but it does not
/// without the schema: Allow
///   reasons: [PolicyID("policy12"), PolicyID("policy11")]
///   errors:  ["… record does not have the attribute `policy_epoch`"]
/// ```
///
/// The gate cannot see it, because `build_request` always passes the schema,
/// and the invariant cannot rest on the decision point in `nc-authz` doing the
/// same — so the conditions are written
/// `!(context has x) || …` and `unless { context has x && … }`, and a missing
/// attribute forbids.
///
/// `NOCTUA.md §2.4`: if the check cannot be performed, the answer is no.
#[test]
fn a_request_missing_a_context_attribute_is_refused_with_no_schema_to_catch_it() {
    let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let pack = nc_policy::load(root).expect("the pack loads");

    // Every attribute a core condition reads, one at a time.
    // `confirmed_by_human` is only read on an agent-mediated request, so the
    // request that tests it has to be one.
    for (absent, agent_mediated) in [
        ("policy_epoch", false),
        ("actor_chain_depth", false),
        ("quorum_met", false),
        ("via_agent", false),
        ("confirmed_by_human", true),
    ] {
        let answered = nc_policy::decide_without_a_schema(
            &pack,
            r#"User::"ana""#,
            r#"Action::"Delete""#,
            r#"Content::"diary""#,
            Some(absent),
            agent_mediated,
        )
        .expect("the request is built");
        assert_eq!(
            answered.without, "Deny",
            "a request with no `{absent}` was allowed: the condition errored, Cedar \
             skipped the policy, and a permit answered. The complete request answers \
             `{}`",
            answered.complete
        );
    }
}

/// An `absent` this builder does not hold is refused, and `None` removes nothing.
///
/// **A name that matches no key must not remove nothing and say
/// nothing.** Answered as the complete request's own decision, a caller asking
/// what the pack says without an attribute could not tell that from an
/// attribute that is still there — measured on the shipped pack,
/// where `policy_epoch` answers `Deny` and `policy_epochh`
/// answers `Allow`. Refusing outright is too strong, because "remove nothing"
/// is a legitimate question, so that is what `None` is for.
#[test]
fn an_absent_attribute_this_builder_does_not_hold_is_refused() {
    let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let pack = nc_policy::load(root).expect("the pack loads");
    let ask = |absent, agent_mediated| {
        nc_policy::decide_without_a_schema(
            &pack,
            r#"User::"ana""#,
            r#"Action::"Delete""#,
            r#"Content::"diary""#,
            absent,
            agent_mediated,
        )
    };
    for name in ["confirmed_by_humann", "", "nonsense", "POLICY_EPOCH"] {
        assert!(
            ask(Some(name), true).is_err(),
            "`{name}` names no key this builder holds, so nothing was removed and the \
             answer is the complete request's own: {:?}",
            ask(Some(name), true)
        );
    }
    let nothing_removed = ask(None, false).expect("the request is built");
    assert_eq!(
        nothing_removed.without, nothing_removed.complete,
        "`None` is the request with every declared key present, so the two decisions \
         are one decision"
    );
    // **The row the whole proof rests on**, and it is the one that moves: with
    // `policy_epoch` gone the pack refuses, and the same request with it there
    // is allowed. It is *not* agent-mediated, because an agent-mediated delete
    // is refused for its own reason and the pair would then read `Deny`/`Deny`
    // — which is exactly the shape the two decisions are returned to show.
    let removed = ask(Some("policy_epoch"), false).expect("the request is built");
    assert_eq!(
        (removed.without.as_str(), removed.complete.as_str()),
        ("Deny", "Allow"),
        "with `policy_epoch` gone the pack must refuse, and the same request with it \
         there must be allowed, or this row proves nothing about the attribute"
    );
}

/// The schema this repository ships, as text.
fn schema_text() -> String {
    std::fs::read_to_string(repository().join("policy/schema.cedarschema"))
        .expect("the schema is readable")
}

/// Every policy file this repository ships, as `compile` takes them.
///
/// Each is its path relative to the repository root, beside the text
/// that path holds.
fn every_policy_file() -> Vec<(String, String)> {
    /// Every `.cedar` file under `directory`, in path order, into `into`.
    fn gather(directory: &Path, root: &Path, into: &mut Vec<(String, String)>) {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(directory)
            .expect("the pack is readable")
            .flatten()
            .map(|entry| entry.path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                gather(&path, root, into);
            } else if path.extension().is_some_and(|kind| kind == "cedar") {
                let relative = path
                    .strip_prefix(root)
                    .expect("under the repository")
                    .to_string_lossy()
                    .into_owned();
                let text = std::fs::read_to_string(&path).expect("the policy is readable");
                into.push((relative, text));
            }
        }
    }
    let root = repository();
    let mut files = Vec::new();
    gather(&root.join("policy"), &root, &mut files);
    assert!(
        files.len() > 5,
        "only {} policy file(s) were gathered, so what compiles here is not the pack",
        files.len()
    );
    files
}

/// The checks a refusal named, in order.
fn checks(problems: &[Problem]) -> Vec<&str> {
    problems.iter().map(|problem| problem.check).collect()
}

/// Compiling every policy file answers the ids the gate's own load holds.
///
/// `load` compiles through `compile`, so this is the two doors agreeing on the
/// shipped pack — and the floor that says the gathering above read the pack.
#[test]
fn compiling_every_policy_file_answers_the_ids_the_gate_holds() {
    let compiled = nc_policy::compile(&schema_text(), &every_policy_file())
        .unwrap_or_else(|problems| panic!("the shipped policies do not compile: {problems:#?}"));
    let pack = load(&repository()).expect("the pack loads");
    assert!(!compiled.ids().is_empty(), "nothing was compiled");
    assert_eq!(
        compiled.ids(),
        pack.ids(),
        "`compile` and `load` hold different policies from the same files"
    );
}

/// The core and one pack, compiled alone, decide a request through Cedar.
///
/// That is how a decision point compiles them, and what it needs from `compile` is exactly its two accessors: a
/// schema to build the request against and the policies to answer it. The
/// request is one the family pack's own cases decide, so the answer is known
/// from both sides — `bruno` reads what the house shares and not ana's diary.
#[test]
fn the_core_and_one_pack_compiled_alone_decide_a_request() {
    use std::str::FromStr as _;

    use cedar_policy::{Authorizer, Context, Decision, Entities, EntityUid, Request};

    let files: Vec<(String, String)> = every_policy_file()
        .into_iter()
        .filter(|(path, _)| {
            path.starts_with("policy/core/") || path.starts_with("policy/packs/family/")
        })
        .collect();
    let compiled = nc_policy::compile(&schema_text(), &files)
        .unwrap_or_else(|problems| panic!("the core and family do not compile: {problems:#?}"));
    let world = std::fs::read_to_string(repository().join("policy/world.json"))
        .expect("the world is readable");
    let entities =
        Entities::from_json_str(&world, Some(compiled.schema())).expect("the world loads");
    let action = EntityUid::from_str(r#"Action::"Read""#).expect("an action");
    let context = serde_json::json!({
        "now": { "__extn": { "fn": "datetime", "arg": "2026-08-26T00:00:00Z" } },
        "via_agent": false,
        "confirmed_by_human": false,
        "actor_chain_depth": 0,
        "policy_epoch": 7,
        "quorum_met": true,
        "grant": { "__entity": { "type": "Grant", "id": "g.current" } },
    });
    let ask = |resource: &str| {
        let request = Request::new(
            EntityUid::from_str(r#"User::"bruno""#).expect("a principal"),
            action.clone(),
            EntityUid::from_str(resource).expect("a resource"),
            Context::from_json_value(context.clone(), Some((compiled.schema(), &action)))
                .expect("the context matches the schema"),
            Some(compiled.schema()),
        )
        .expect("the request matches the schema");
        Authorizer::new().is_authorized(&request, compiled.policies(), &entities)
    };
    let shared = ask(r#"Content::"photo""#);
    assert_eq!(shared.decision(), Decision::Allow);
    assert!(
        shared
            .diagnostics()
            .reason()
            .any(|id| id.to_string() == "family.a_member_reads_shared_content"),
        "the answer was not decided by the permit the family pack's own case names"
    );
    assert_eq!(ask(r#"Content::"diary""#).decision(), Decision::Deny);
}

/// `compile` refuses a schema that will not parse, and a set holding no policy.
#[test]
fn compile_refuses_an_unparsable_schema_and_an_empty_set() {
    let refused = nc_policy::compile("entity {", &every_policy_file())
        .expect_err("a schema that does not parse compiled");
    assert_eq!(checks(&refused), ["schema-malformed"]);
    let refused =
        nc_policy::compile(&schema_text(), &[]).expect_err("a set holding no policy compiled");
    assert_eq!(checks(&refused), ["empty-pack"]);
}

/// Validation waits for the policies to parse, and for nothing else.
///
/// A policy that does not parse is one defect, and reporting what the rest of
/// the set fails to validate as well would be a second diagnostic for it. A
/// broken world is a separate defect and hides nothing, which
/// `a_broken_world_does_not_hide_a_policy_the_schema_refuses` holds on its own
/// side.
#[test]
fn a_policy_that_does_not_parse_is_not_also_reported_invalid() {
    let invalid = (
        "policy/packs/probe/invalid.cedar".to_owned(),
        "@id(\"probe.invalid\")\npermit (principal, action, resource)\n\
         when { context.policy_epcoh > 0 };\n"
            .to_owned(),
    );
    let malformed = (
        "policy/packs/probe/malformed.cedar".to_owned(),
        "forbid ( ( (".to_owned(),
    );
    let alone = nc_policy::compile(&schema_text(), std::slice::from_ref(&invalid))
        .expect_err("a policy the schema refuses compiled");
    assert!(
        checks(&alone).contains(&"pack-invalid"),
        "the schema's refusal was not reported: {alone:#?}"
    );
    let both = nc_policy::compile(&schema_text(), &[invalid, malformed])
        .expect_err("a policy that does not parse compiled");
    assert!(checks(&both).contains(&"policy-malformed"), "{both:#?}");
    assert!(
        !checks(&both).contains(&"pack-invalid"),
        "a set with a policy that does not parse was validated as well: {both:#?}"
    );
}

/// A broken world does not hide a policy the schema refuses.
///
/// `load` compiles the policies on their own, and the world is read beside
/// them, so each defect is reported for itself.
#[test]
fn a_broken_world_does_not_hide_a_policy_the_schema_refuses() {
    let scratch = Scratch::new("world-and-policy");
    scratch.edit("policy/world.json", |_| "{ not json".to_owned());
    scratch.edit("policy/core/fail_closed.cedar", |text| {
        text.replace("context.policy_epoch", "context.policy_epcoh")
    });
    let found = scratch.problems();
    assert!(checks(&found).contains(&"world-malformed"), "{found:#?}");
    assert!(checks(&found).contains(&"pack-invalid"), "{found:#?}");
}

/// A policy whose `@id` begins with the decision point's prefix is refused.
///
/// The journal records a policy's `@id` and the decision point's own sentinels
/// in one column, so a pack rule named like a sentinel would make an `allow`
/// it decided read like a fail-closed refusal.
#[test]
fn a_policy_whose_id_the_decision_point_reserves_is_refused() {
    let scratch = Scratch::new("reserved-id");
    scratch.edit("policy/packs/family/permissions.cedar", |text| {
        text.replace(
            "@id(\"family.giving_out_encrypted_content_needs_quorum\")",
            "@id(\"kernel.no_permit_matched\")",
        )
    });
    scratch.must_fail_saying("policy-id-reserved", "kernel.no_permit_matched");
}

/// The reserved prefix is refused where an id begins, and nowhere else.
///
/// Two sides, because each alone is satisfied by the wrong rule: refusing only
/// the exact sentinels lets `kernel.anything` through, and refusing every id
/// that *holds* the prefix refuses a pack's own `probe.kernel.…`. The prefix
/// ends in its dot, so `kernels.…` is another word. `kernel.` alone is refused
/// as reserved, because that rule is asked first.
#[test]
fn the_reserved_prefix_is_refused_only_where_an_id_begins() {
    let schema = schema_text();
    for id in [
        "kernel.no_permit_matched",
        "kernel.a_rule_of_the_pack",
        "kernel.",
    ] {
        let refused = nc_policy::compile(&schema, &one_permit_named(id))
            .expect_err("an id under the decision point's prefix compiled");
        assert!(
            refused
                .iter()
                .any(|problem| problem.check == "policy-id-reserved" && problem.what.contains(id)),
            "`{id}` was not refused as reserved: {refused:#?}"
        );
    }
    // Each in the pack its first word names, which is the rule beside this one.
    for (pack, id) in [
        ("probe", "probe.kernel.no_permit_matched"),
        ("kernels", "kernels.no_permit_matched"),
    ] {
        let compiled = nc_policy::compile(&schema, &one_permit_in(pack, id))
            .unwrap_or_else(|problems| panic!("`{id}` was refused: {problems:#?}"));
        assert_eq!(compiled.ids(), [id]);
    }
}

/// An `@id` is lower-case words joined by dots, and no other shape compiles.
///
/// **The prefix rule compares bytes**, so `Kernel.no_permit_matched` and the
/// sentinel with a space in front are each another string to it — while any
/// view that folds case or trims whitespace shows either as the decision
/// point's own verdict. Held to one shape, neither can be written, nor a letter
/// or a digit from another script, first in its word or not.
/// Each refused id breaks one clause of the shape, and the accepted ones are
/// the shape at its edges: more than two words, and a digit after a letter.
#[test]
fn an_id_is_lower_case_words_joined_by_dots() {
    let schema = schema_text();
    for id in [
        "Kernel.no_permit_matched",
        " kernel.no_permit_matched",
        "k\u{435}rnel.no_permit_matched",
        "\u{43a}ernel.no_permit_matched",
        "family.rule\u{663}",
        "kernel",
        "family..a_rule",
        "family._a_rule",
        "family.2nd_rule",
        "family.a-rule",
        "family.a rule",
        "family.a_Rule",
    ] {
        let Err(refused) = nc_policy::compile(&schema, &one_permit_named(id)) else {
            panic!("`{id}` compiled, and it is not lower-case words joined by dots");
        };
        assert!(
            refused
                .iter()
                .any(|problem| problem.check == "policy-id-malformed" && problem.what.contains(id)),
            "`{id}` was not refused as malformed: {refused:#?}"
        );
    }
    for id in ["probe.a_rule", "probe.rule_2", "probe.grouped.a_rule"] {
        let compiled = nc_policy::compile(&schema, &one_permit_named(id))
            .unwrap_or_else(|problems| panic!("`{id}` was refused: {problems:#?}"));
        assert_eq!(compiled.ids(), [id]);
    }
}

/// An `@id`'s first word is the pack its file is in: `core` under
/// `policy/core/`, the profile under `policy/packs/<profile>/`.
///
/// **A real bug of this shape**: without the rule, a family file's `permit`
/// named `core.a_member_reads_the_realm` compiles beside one named `org.…`, so
/// the journal's record of which policy decided could name an invariant every
/// pack holds for a rule one pack wrote — the confusion the id rules exist to
/// prevent (`NOCTUA.md §14.9`).
#[test]
fn an_ids_first_word_is_the_pack_its_file_is_in() {
    let schema = schema_text();
    let permit = |file: &str, id: &str| {
        vec![(
            file.to_owned(),
            format!("@id(\"{id}\")\npermit (principal, action == Action::\"Read\", resource);\n"),
        )]
    };
    for (file, id) in [
        (
            "policy/packs/family/probe.cedar",
            "core.a_member_reads_the_realm",
        ),
        (
            "policy/packs/family/probe.cedar",
            "org.a_rule_of_another_pack",
        ),
        ("policy/packs/family/probe.cedar", "families.a_rule"),
        ("policy/core/probe.cedar", "family.a_rule"),
    ] {
        let Err(refused) = nc_policy::compile(&schema, &permit(file, id)) else {
            panic!("`{id}` compiled in `{file}`, and its first word is not that file's pack");
        };
        assert!(
            refused
                .iter()
                .any(|problem| problem.check == "policy-id-outside-its-pack"
                    && problem.what.contains(id)
                    && problem.file == file),
            "`{id}` in `{file}` was not refused as outside its pack: {refused:#?}"
        );
    }
    for (file, id) in [
        ("policy/packs/family/probe.cedar", "family.a_rule"),
        ("policy/packs/family/nested/probe.cedar", "family.a_rule"),
    ] {
        let compiled = nc_policy::compile(&schema, &permit(file, id))
            .unwrap_or_else(|problems| panic!("`{id}` in `{file}` was refused: {problems:#?}"));
        assert_eq!(compiled.ids(), [id]);
    }
}

/// One file holding one `permit` called `id`, for a test of what an id may be.
fn one_permit_named(id: &str) -> Vec<(String, String)> {
    one_permit_in("probe", id)
}

/// One file of the pack `pack` holding one `permit` called `id`.
fn one_permit_in(pack: &str, id: &str) -> Vec<(String, String)> {
    vec![(
        format!("policy/packs/{pack}/probe.cedar"),
        format!("@id(\"{id}\")\npermit (principal, action == Action::\"Read\", resource);\n"),
    )]
}

/// A pack that holds policies and no manifest is refused, naming the file.
///
/// Without a version there is nothing a realm could pin, and nothing a
/// decision point could compare a pin with.
#[test]
fn a_pack_holding_policies_with_no_manifest_is_refused() {
    let scratch = Scratch::new("manifest-missing");
    std::fs::remove_file(scratch.root.join("policy/packs/family/pack.toml"))
        .expect("the copy is writable");
    scratch.must_fail("pack-manifest-missing", "policy/packs/family/pack.toml");
}

/// A manifest that is anything but one unsigned version is refused.
///
/// It is refused by the pack it belongs to. Each shape it refuses is a case of
/// the unit test beside `Manifest::parse`, and this is the gate reaching it.
#[test]
fn a_manifest_that_is_not_one_version_is_refused() {
    for (body, slug) in [
        ("version = \"1\"\n", "quoted-version"),
        ("version = 1\nprofile = \"family\"\n", "second-key"),
    ] {
        let scratch = Scratch::new(slug);
        scratch.edit("policy/packs/family/pack.toml", |_| body.to_owned());
        scratch.must_fail("pack-manifest-malformed", "policy/packs/family/pack.toml");
    }
}

/// A manifest beside no policy is refused.
///
/// A version of nothing is a pin a realm could satisfy while no decision point
/// carries the pack.
#[test]
fn a_manifest_beside_no_policy_is_refused() {
    let scratch = Scratch::new("manifest-without-policies");
    std::fs::write(
        scratch.root.join("policy/packs/org/pack.toml"),
        "version = 1\n",
    )
    .expect("the copy is writable");
    scratch.must_fail(
        "pack-manifest-without-policies",
        "policy/packs/org/pack.toml",
    );
}

/// A pack whose policies all sit a directory down still needs its manifest.
///
/// A pack's own directory holds all of it however deep, which is what a
/// decision point compiles, so the search that decides whether a pack holds a
/// policy reads all of it too. Read only one level down, `org` holding one rule
/// in a subdirectory ships with no version, and the manifest that would give it
/// one is refused as standing beside nothing. The second half reads the
/// version: `office` is then held to `org@1`.
#[test]
fn a_pack_whose_policies_sit_deeper_still_needs_its_manifest() {
    let scratch = Scratch::new("manifest-for-a-deeper-pack");
    let at = scratch.root.join("policy/packs/org/grouped/x.cedar");
    std::fs::create_dir_all(at.parent().expect("a directory")).expect("writable");
    std::fs::write(
        &at,
        "@id(\"org.x\")\nforbid (principal, action, resource);\n",
    )
    .expect("writable");
    scratch.must_fail("pack-manifest-missing", "policy/packs/org/pack.toml");

    std::fs::write(
        scratch.root.join("policy/packs/org/pack.toml"),
        "version = 1\n",
    )
    .expect("writable");
    let refused = scratch.problems();
    assert!(
        refused
            .iter()
            .all(|problem| problem.check != "pack-manifest-without-policies"),
        "a manifest beside a pack whose policies sit a directory down was refused as \
         beside none: {refused:#?}"
    );
    scratch.must_fail_saying(
        "realm-pins-another-version",
        "pins `org@0.1.0`, and the `org` pack this tree carries is `org@1`",
    );
}

/// A realm pinning another version of its pack is refused, whichever side moved.
#[test]
fn a_realm_pinning_another_version_of_its_pack_is_refused() {
    // The realm moved: it pins a version this tree does not carry.
    let scratch = Scratch::new("pin-moved");
    scratch.edit("policy/world.json", |text| {
        text.replace(
            "\"policy_pack\": \"family@1\"",
            "\"policy_pack\": \"family@2\"",
        )
    });
    scratch.must_fail_saying(
        "realm-pins-another-version",
        "pins `family@2`, and the `family` pack this tree carries is `family@1`",
    );
    // The pack moved: its version changed and the realm was not migrated.
    let scratch = Scratch::new("version-moved");
    scratch.edit("policy/packs/family/pack.toml", |text| {
        text.replace("version = 1", "version = 2")
    });
    scratch.must_fail_saying(
        "realm-pins-another-version",
        "pins `family@1`, and the `family` pack this tree carries is `family@2`",
    );
}

/// A realm pinning another profile's pack is refused, carried or not.
///
/// **The pin rule works both ways.** Comparing the version only for a realm
/// whose profile selects a carried pack, and asking nothing of any other,
/// lets `office`, whose profile is `org`, pin `family@1` and the gate accept it
/// — while a decision point comparing only the pin with the family pack it
/// carries would let that realm through on rules nobody chose for it. Measured
/// by planting exactly that realm: the policy step all `ok`, and both suites
/// green. The second pin is the prefix read to its end: `organic` begins
/// with `org`, and is another profile.
#[test]
fn a_realm_pinning_another_profiles_pack_is_refused() {
    for (pin, slug) in [
        ("family@1", "pins-a-carried-pack"),
        ("organic@0.1.0", "pins-a-longer-profile"),
    ] {
        let scratch = Scratch::new(slug);
        scratch.edit("policy/world.json", |text| {
            text.replace(
                "\"policy_pack\": \"org@0.1.0\"",
                &format!("\"policy_pack\": \"{pin}\""),
            )
        });
        scratch.must_fail_saying(
            "realm-pins-another-profile",
            &format!(
                "`Realm::\"office\"` selects the `org` profile and pins `{pin}`, which is \
                 not a version of the `org` pack"
            ),
        );
    }
}

/// A realm whose profile selects no carried pack is held to nothing.
///
/// And the shipped world's family realm is held to the family pack. `office` pins a version of `org`, which holds no policy, so there is
/// nothing on disk to hold that version to: the gate must neither refuse it nor
/// count it as a pin it checked. A decision point denies that realm instead,
/// for carrying no such pack.
#[test]
fn a_realm_whose_profile_selects_no_carried_pack_is_held_to_nothing() {
    let pack = load(&repository()).expect("the pack loads");
    let versions: Vec<(&str, u32)> = pack
        .versions()
        .iter()
        .map(|(profile, version)| (profile.as_str(), *version))
        .collect();
    assert_eq!(versions, [("family", 1)]);
    let pins: Vec<(&str, &str)> = pack
        .pins()
        .iter()
        .map(|(realm, pin)| (realm.as_str(), pin.as_str()))
        .collect();
    assert_eq!(pins, [("home", "family@1")]);

    let scratch = Scratch::new("pin-of-nothing");
    scratch.edit("policy/world.json", |text| {
        text.replace(
            "\"policy_pack\": \"org@0.1.0\"",
            "\"policy_pack\": \"org@whatever it likes\"",
        )
    });
    scratch.must_pass();
}

/// A tree with no packs directory carries no pack and holds no pin.
///
/// Every realm's pin is then held to nothing on disk, which `Pack::pins`
/// answers as empty — the answer the policy step refuses as a world that
/// holds no realm to a pack — rather than as agreement.
#[test]
fn a_tree_without_packs_carries_none_and_holds_no_pin() {
    let scratch = Scratch::new("no-packs");
    std::fs::remove_dir_all(scratch.root.join("policy/packs")).expect("the copy is writable");
    let pack = load(&scratch.root)
        .unwrap_or_else(|problems| panic!("the core alone was refused: {problems:#?}"));
    assert!(
        pack.versions().is_empty(),
        "a pack was carried: {:?}",
        pack.versions()
    );
    assert!(pack.pins().is_empty(), "a pin was held: {:?}", pack.pins());
}

/// A `.toml` under `policy/` that nothing reads is refused by name.
///
/// Every one is a case file or a pack's manifest, and one of either shape
/// anywhere else reads as in force and is not.
#[test]
fn a_toml_nothing_reads_is_refused_by_name() {
    for (at, slug) in [
        ("policy/core/pack.toml", "manifest-in-the-core"),
        ("policy/packs/family/grouped/pack.toml", "manifest-too-deep"),
        ("policy/packs/family/version.toml", "manifest-renamed"),
        ("policy/cases.toml", "case-outside-tests"),
    ] {
        let scratch = Scratch::new(slug);
        let path = scratch.root.join(at);
        std::fs::create_dir_all(path.parent().expect("a directory")).expect("writable");
        std::fs::write(&path, "version = 1\n").expect("writable");
        scratch.must_fail("toml-read-by-nothing", at);
    }
}

/// A policy in no pack is refused by name, however near a pack it sits.
///
/// **A rule the gate holds and no decision point loads.** The gate compiles and
/// cases every `.cedar` under `policy/`, and a decision point compiles the core
/// and its realm's pack, so a file in neither would pass every check while it
/// protects nothing. Measured by moving the family pack's only `forbid` into a
/// file directly under `policy/packs/`: the policy step all `ok`, both suites
/// green, and the core and the family pack compiled alone allowing the request
/// that `forbid`'s own case denies.
#[test]
fn a_policy_in_no_pack_is_refused_by_name() {
    for (at, slug) in [
        ("policy/packs/stray.cedar", "policy-beside-the-packs"),
        ("policy/stray.cedar", "policy-beside-the-core"),
    ] {
        let scratch = Scratch::new(slug);
        std::fs::write(
            scratch.root.join(at),
            "@id(\"probe.in_no_pack\")\nforbid (principal, action, resource);\n",
        )
        .expect("writable");
        scratch.must_fail("policy-in-no-pack", at);
    }
    // And a pack's own directory holds all of that pack, however deep: a
    // `forbid` of everything one directory further down is read — every allow
    // case turns — and is not refused as in no pack.
    let scratch = Scratch::new("policy-in-a-pack");
    let at = scratch
        .root
        .join("policy/packs/family/grouped/nested.cedar");
    std::fs::create_dir_all(at.parent().expect("a directory")).expect("writable");
    std::fs::write(
        &at,
        "@id(\"family.nested_probe\")\nforbid (principal, action, resource);\n",
    )
    .expect("writable");
    let refused = scratch.problems();
    assert!(
        !refused.is_empty(),
        "a `forbid` of everything inside the family pack turned no verdict, so \
         nothing read it"
    );
    assert!(
        refused
            .iter()
            .all(|problem| problem.check != "policy-in-no-pack"),
        "a policy inside a pack's own directory was refused as in none: {refused:#?}"
    );
    // Where a file sits is the gate's question alone. `compile` takes the
    // paths it is handed, which is why `Compiled`'s own doc says a decision
    // point is to load only files the gate has passed.
    let stray = [(
        "policy/stray.cedar".to_owned(),
        "@id(\"probe.in_no_pack\")\nforbid (principal, action, resource);\n".to_owned(),
    )];
    let compiled = nc_policy::compile(&schema_text(), &stray)
        .unwrap_or_else(|problems| panic!("`compile` judged where a file sits: {problems:#?}"));
    assert_eq!(compiled.ids(), ["probe.in_no_pack"]);
}

/// A checkout under a directory called `tests` reads no manifest as a case.
///
/// **Inside `policy`, not along the absolute path.** A case loader asking
/// whether *any* ancestor of a file is called `tests`, all the way up the
/// absolute path, reads the manifest under such a checkout as a case file, and
/// it fails to parse as one.
#[test]
fn a_repository_under_a_directory_named_tests_reads_no_manifest_as_a_case() {
    let scratch = Scratch::new("under-tests");
    let nested = scratch.root.join("tests").join("repository");
    copy(&scratch.root.join("policy"), &nested.join("policy"));
    let (_, files) = cases::load(&nested)
        .unwrap_or_else(|problems| panic!("the cases do not load: {problems:#?}"));
    assert!(!files.is_empty(), "no case file was read at all");
    assert!(
        files.iter().all(|file| !file.ends_with("pack.toml")),
        "a manifest was read as a case file: {files:#?}"
    );
    load(&nested).unwrap_or_else(|problems| panic!("the pack is refused: {problems:#?}"));
}

/// The guard removes its directory when it is dropped.
///
/// **What this type does is decided by its name.** `BUILDS_A_SCRATCH`
/// in `xtask/src/check.rs` counts a site as guarded when the body around it
/// names a type from `REMOVES_ITS_SCRATCH_ON_THE_WAY_OUT`, and `Scratch` is on
/// that list — so a gutted `Drop` here would leave the census reporting every
/// site in this file as removed however the test leaves, while one run of this
/// suite left 85 directories in the temporary directory, with the whole gate
/// green.
///
/// A name is not a guarantee. This is.
#[test]
fn the_scratch_is_gone_when_the_guard_is() {
    let root = {
        let scratch = Scratch::new("drop-is-a-guarantee");
        let root = scratch.root.clone();
        assert!(root.is_dir(), "the scratch was not built");
        root
    };
    assert!(
        !root.exists(),
        "`Scratch`'s `Drop` left {} behind, and the census that counts this file's \
         sites as guarded reads the type's name rather than what it does",
        root.display()
    );
}
