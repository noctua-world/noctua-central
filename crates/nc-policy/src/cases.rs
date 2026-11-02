//! The case files: what they say, and how they are read.
//!
//! Every `*.toml` under any `tests/` directory, read into one list.
//!
//! **Not paired by file name**, though two comments used to say so — renaming
//! `policy/core/tests/fail_closed.toml` to anything at all keeps the gate green. The
//! real mechanism is stronger: coverage is attributed by `@id`, through
//! `Pack::origins`, so a policy moved to another file keeps its denial test and
//! a file holding two policies is understood as holding two.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::problem::{Problem, line_of};
use crate::value::to_cedar;

/// What a case expects to happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    /// The authorizer permits it.
    Allow,
    /// The authorizer refuses it.
    Deny,
}

impl Decision {
    /// The word, for a message.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }
}

/// One case as it is written in the file.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Written {
    name: String,
    principal: String,
    action: String,
    resource: String,
    decision: Decision,
    /// The policy that decided. For an `allow`, the permit that granted it; for
    /// a `deny`, the forbid that refused it.
    #[serde(default)]
    because: Option<String>,
    /// For a `deny` that no policy caused: the permit this request falls just
    /// outside of.
    #[serde(default)]
    no_permit_matched: Option<String>,
    /// That what puts it outside is the **action**, not the resource.
    ///
    /// A `no_permit_matched` case has to be paired with an `allow` case naming
    /// the same permit on the same action and the same two entity types, so
    /// that the denial is the boundary of a rule that demonstrably fires. That
    /// pairing makes a permit's *action list* untestable by construction: a
    /// case denied because the verb is outside the list has, by definition, a
    /// verb the permit never allows.
    ///
    /// Which left every action list in the pack pinned by nothing. Widening
    /// `action in [Action::"Grant", Action::"Revoke"]` to a bare `action` gives
    /// every guardian Read, Write, Delete and Share over every resource in the
    /// realm — and passed the whole gate green.
    ///
    /// So a case may say that the action is what it is testing. The pairing
    /// then asks for the same permit allowing the same two entity types on
    /// *some* verb, which is still a rule that demonstrably fires — and this
    /// case is a verb outside its list.
    #[serde(default)]
    the_action_is_the_boundary: bool,
    /// That what puts it outside is the **principal's type**, not the resource.
    ///
    /// The same construction one field along. A permit that says
    /// `principal is User` is refused to a `Device` *because* it is a device, so
    /// the case that shows the type test working has a kind of principal the
    /// permit never allows — and the pairing, which asks for an `allow` on the
    /// same two entity types, could not be satisfied by any request at all.
    ///
    /// Which left the type test pinned by nothing, and a type test nothing pins
    /// is one that can be deleted in silence. Measured on 2026-09-29 by a
    /// reviewer: `family.a_member_steps_through_their_own_history` shipped
    /// without its `principal is User`, a note beside it saying no request could
    /// show the type test working, and an attested device and an agent both
    /// allowed to write the realm — the undo and redo of a person's history.
    ///
    /// So a case may say that the principal is what it is testing. The pairing
    /// then asks for the same permit allowing the same action on the same kind
    /// of resource to *some* principal, and this case is a principal outside it.
    #[serde(default)]
    the_principal_is_the_boundary: bool,
    /// Keys that override the file's defaults.
    #[serde(default)]
    context: BTreeMap<String, toml::Value>,
    /// Keys removed from the file's defaults, for the requests that are about an
    /// **absent** value.
    #[serde(default)]
    context_without: Vec<String>,
}

/// A whole case file.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    default: Defaults,
    #[serde(rename = "case")]
    cases: Vec<Written>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Defaults {
    #[serde(default)]
    context: BTreeMap<String, toml::Value>,
}

/// One case, ready to run.
#[derive(Debug, Clone)]
pub struct Case {
    /// Repository-relative path of the file it came from.
    pub file: String,
    /// The line its `name` is written on.
    pub line: usize,
    /// What it is called.
    pub name: String,
    /// The principal's entity uid, as written.
    pub principal: String,
    /// The action's entity uid, expanded from the bare name in the file.
    pub action: String,
    /// The resource's entity uid, as written.
    pub resource: String,
    /// What must happen.
    pub decision: Decision,
    /// The policy that must be among the reasons.
    pub because: Option<String>,
    /// The permit this request must fall outside of.
    pub no_permit_matched: Option<String>,
    /// Whether the action, rather than the resource, is what puts it outside.
    pub the_action_is_the_boundary: bool,
    /// Whether the principal, rather than the resource, is what puts it outside.
    pub the_principal_is_the_boundary: bool,
    /// The context, as Cedar JSON.
    pub context: serde_json::Value,
}

/// Reads every case file under `policy/`.
///
/// Returns the cases and the files they came from, so a `tests/` directory that
/// lost its contents is visible rather than green.
///
/// # Errors
///
/// Returns a problem per malformed file. A file that cannot be parsed does not
/// stop the others being read: one pass reports everything.
pub fn load(root: &Path) -> Result<(Vec<Case>, Vec<PathBuf>), Vec<Problem>> {
    let mut files = Vec::new();
    let policy = root.join("policy");
    crate::walk(&policy, &mut |path| {
        if is_a_case_file(&policy, path) {
            files.push(path.to_path_buf());
        }
    });
    files.sort();

    let mut cases = Vec::new();
    let mut problems = Vec::new();
    for path in &files {
        let relative = crate::relative_to(root, path);
        let Ok(text) = std::fs::read_to_string(path) else {
            problems.push(Problem {
                file: relative,
                line: 0,
                what: "the case file could not be read".to_owned(),
                fix: "coverage is attributed by the policy's `@id`, wherever the case \
                      is written — so an unreadable file takes its policies' denial \
                      tests with it, and §14.9's seventh invariant then fails for \
                      rules that are in fact tested. Fix the file rather than the \
                      policies it covers"
                    .to_owned(),
                check: "case-file-unreadable",
            });
            continue;
        };
        match parse(&relative, &text) {
            Ok(parsed) => cases.extend(parsed),
            Err(mut found) => problems.append(&mut found),
        }
    }
    if problems.is_empty() {
        Ok((cases, files))
    } else {
        Err(problems)
    }
}

/// Whether `path` is a case file: a `.toml` under a `tests/` directory.
///
/// **Under *any* `tests/` directory**, not only an immediate parent of that
/// name, so a case file grouped one directory deeper inside a pack's own tests
/// is still run and still counted.
///
/// **And inside `policy`, never along the absolute path**: a repository checked
/// out under any directory called `tests` would otherwise make every `.toml`
/// under `policy/` a case file — a pack's manifest included, which then fails
/// to parse as one. The manifest gate reads this same predicate, so what the
/// case loader runs and what that gate accepts as a case are one rule.
pub(crate) fn is_a_case_file(policy: &Path, path: &Path) -> bool {
    let inside = crate::relative_to(policy, path);
    path.extension()
        .is_some_and(|extension| extension == "toml")
        && inside.split('/').rev().skip(1).any(|part| part == "tests")
}

/// One case's context: the file's defaults, minus its removals, plus its
/// overrides, as Cedar JSON.
fn context_of(
    relative: &str,
    defaults: &BTreeMap<String, toml::Value>,
    written: &Written,
    line: usize,
    problems: &mut Vec<Problem>,
) -> serde_json::Value {
    let mut context = defaults.clone();
    for key in &written.context_without {
        if context.remove(key).is_none() {
            problems.push(Problem {
                file: relative.to_owned(),
                line,
                what: format!(
                    "`{}` removes `{key}` from the context, which the defaults do not set",
                    written.name
                ),
                fix: "a case that removes a key nothing sets reads as though it were \
                      testing an absence, and tests the same request as every other case \
                      in the file. Either set it in `[default.context]` or drop the removal"
                    .to_owned(),
                check: "context-removes-nothing",
            });
        }
    }
    for (key, value) in &written.context {
        context.insert(key.clone(), value.clone());
    }

    let mut object = serde_json::Map::new();
    for (key, value) in &context {
        match to_cedar(&format!("context.{key}"), value) {
            Ok(json) => {
                object.insert(key.clone(), json);
            }
            Err(error) => problems.push(Problem {
                file: relative.to_owned(),
                line,
                what: format!("`{}` has no Cedar value: {}", error.at, error.what),
                fix: "write an extension value as a table with one `datetime` key, and an \
                      entity reference as a table with one `entity` key holding the whole \
                      uid"
                .to_owned(),
                check: "context-value-unconvertible",
            }),
        }
    }
    serde_json::Value::Object(object)
}

/// Whether a case says enough about what it expects, and nothing that goes
/// unjudged.
///
/// **Cedar denies by default**, so a bare deny is true of an empty pack — the
/// vacuous assertion this whole format exists to make impossible. A flag that
/// nothing reads is the same failure from the other side: a claim written into
/// the case that no code evaluates.
fn judge(relative: &str, written: &Written, line: usize, problems: &mut Vec<Problem>) {
    if written.decision == Decision::Deny
        && written.because.is_none()
        && written.no_permit_matched.is_none()
    {
        problems.push(Problem {
            file: relative.to_owned(),
            line,
            what: format!(
                "`{}` expects a denial and says nothing about why",
                written.name
            ),
            fix: "Cedar denies by default, so a bare denial passes whatever the pack says \
                  and proves nothing. Name the forbid that refuses it in `because`, or the \
                  permit it falls outside of in `no_permit_matched`"
                .to_owned(),
            check: "denial-without-a-reason",
        });
    }
    if written.because.is_some() && written.no_permit_matched.is_some() {
        problems.push(Problem {
            file: relative.to_owned(),
            line,
            what: format!("`{}` names both a cause and a missing permit", written.name),
            fix: "a denial has one kind: either a forbid refused it, or nothing permitted \
                  it. Naming both means one of the two is not being checked"
                .to_owned(),
            check: "denial-with-two-reasons",
        });
    }
    // **A boundary flag is a claim about a boundary, and only a
    // `no_permit_matched` case has one.** Both flags are judged where the
    // pairing is, and that loop reads `no_permit_matched` cases alone. On an
    // `allow`, or on a denial a `forbid` caused, a flag was a claim no code
    // evaluated, and the census in `xtask/src/policy.rs` shipped
    // `/boundary=principal` beside an `Allow`. Measured by a reviewer: the flag
    // on the history permit's `allow` case left the policy block unchanged and
    // `check passed`, and on a `because` case `cargo test -p nc-policy` stayed
    // green.
    let flags: Vec<&str> = [
        (
            written.the_action_is_the_boundary,
            "`the_action_is_the_boundary`",
        ),
        (
            written.the_principal_is_the_boundary,
            "`the_principal_is_the_boundary`",
        ),
    ]
    .into_iter()
    .filter_map(|(set, flag)| set.then_some(flag))
    .collect();
    if written.no_permit_matched.is_none() && !flags.is_empty() {
        problems.push(Problem {
            file: relative.to_owned(),
            line,
            what: format!(
                "`{}` sets {} and names no permit it falls outside of",
                written.name,
                flags.join(" and ")
            ),
            fix: "the two boundary flags say which part of a request puts it outside the \
                  permit named in `no_permit_matched`, and they are judged against that \
                  permit's `allow` cases. On an `allow`, or on a denial a `forbid` caused, \
                  there is no permit to be outside of, so nothing judges the flag and it is \
                  a claim no code evaluates. Drop the flag, or make the case the \
                  `no_permit_matched` denial it describes"
                .to_owned(),
            check: "boundary-flag-without-a-boundary",
        });
    }
}

/// Parses one file's text. Split out so the tests can drive it without a tree.
fn parse(relative: &str, text: &str) -> Result<Vec<Case>, Vec<Problem>> {
    let file: File = match toml::from_str(text) {
        Ok(file) => file,
        Err(error) => {
            return Err(vec![Problem {
                file: relative.to_owned(),
                line: error
                    .span()
                    .map_or(0, |span| text[..span.start].matches('\n').count() + 1),
                what: format!("the case file does not parse: {}", error.message()),
                fix: "a case is a `[[case]]` with `name`, `principal`, `action`, \
                      `resource` and `decision`, and the file may carry a \
                      `[default.context]` the cases override"
                    .to_owned(),
                check: "case-file-malformed",
            }]);
        }
    };

    let mut problems = Vec::new();
    let mut cases = Vec::new();
    for written in file.cases {
        // **The declaration, not the name.** A case name is a sentence, and a
        // sentence can open another: `line_of` answered with the *first* line
        // holding the text, so a case whose name begins an earlier one was filed
        // against that earlier case. Asking for the line that declares it is one
        // needle rather than a rule about which occurrence to take, and a case
        // file that writes the key some other way reports line zero — which
        // `every_case_reports_the_line_its_name_is_declared_on` refuses over the
        // whole shipped pack, rather than leaving it to be discovered.
        let line = line_of(text, &format!("name = \"{}\"", written.name));
        let context = context_of(
            relative,
            &file.default.context,
            &written,
            line,
            &mut problems,
        );
        judge(relative, &written, line, &mut problems);
        cases.push(Case {
            file: relative.to_owned(),
            line,
            name: written.name,
            principal: written.principal,
            action: format!("Action::{:?}", written.action),
            resource: written.resource,
            decision: written.decision,
            because: written.because,
            no_permit_matched: written.no_permit_matched,
            the_action_is_the_boundary: written.the_action_is_the_boundary,
            the_principal_is_the_boundary: written.the_principal_is_the_boundary,
            context,
        });
    }

    if problems.is_empty() {
        Ok(cases)
    } else {
        Err(problems)
    }
}

#[cfg(test)]
mod tests {
    use super::{Decision, parse};

    const HEAD: &str = r#"
[default.context]
now = { datetime = "2026-08-26T00:00:00Z" }
policy_epoch = 7
"#;

    fn one(body: &str) -> String {
        format!("{HEAD}{body}")
    }

    #[test]
    fn a_case_inherits_the_defaults_and_overrides_one_key() {
        let cases = parse(
            "x.toml",
            &one(r#"
[[case]]
name = "an epoch of zero"
principal = 'User::"ana"'
action = "Read"
resource = 'Content::"photo"'
decision = "deny"
because = "core.fail_closed"
context = { policy_epoch = 0 }
"#),
        )
        .expect("parses");
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].context["policy_epoch"], 0);
        assert_eq!(cases[0].context["now"]["__extn"]["fn"], "datetime");
        assert_eq!(cases[0].action, r#"Action::"Read""#);
        assert_eq!(cases[0].decision, Decision::Deny);
    }

    #[test]
    fn a_removal_takes_the_key_out() {
        let cases = parse(
            "x.toml",
            &one(r#"
[[case]]
name = "no epoch at all"
principal = 'User::"ana"'
action = "Read"
resource = 'Content::"photo"'
decision = "deny"
because = "core.fail_closed"
context_without = ["policy_epoch"]
"#),
        )
        .expect("parses");
        assert!(cases[0].context.get("policy_epoch").is_none());
    }

    #[test]
    fn removing_a_key_nothing_sets_is_a_problem() {
        // Otherwise the case reads as though it tested an absence while running
        // exactly the request every other case in the file runs.
        let problems = parse(
            "x.toml",
            &one(r#"
[[case]]
name = "removes what was never there"
principal = 'User::"ana"'
action = "Read"
resource = 'Content::"photo"'
decision = "deny"
because = "core.fail_closed"
context_without = ["quorum_met"]
"#),
        )
        .expect_err("refused");
        assert_eq!(problems[0].check, "context-removes-nothing");
        assert!(
            problems[0].what.contains("quorum_met"),
            "{}",
            problems[0].what
        );
    }

    #[test]
    fn a_denial_that_says_nothing_about_why_is_refused() {
        // The vacuous case, and the one this whole format exists to make
        // impossible: Cedar denies by default, so `decision = "deny"` alone
        // passes against an empty pack.
        let problems = parse(
            "x.toml",
            &one(r#"
[[case]]
name = "denied, somehow"
principal = 'User::"ana"'
action = "Read"
resource = 'Content::"photo"'
decision = "deny"
"#),
        )
        .expect_err("refused");
        assert_eq!(problems[0].check, "denial-without-a-reason");
    }

    #[test]
    fn a_denial_naming_both_kinds_is_refused() {
        let problems = parse(
            "x.toml",
            &one(r#"
[[case]]
name = "both at once"
principal = 'User::"ana"'
action = "Read"
resource = 'Content::"photo"'
decision = "deny"
because = "core.fail_closed"
no_permit_matched = "family.a_member_reads_shared_content"
"#),
        )
        .expect_err("refused");
        assert_eq!(problems[0].check, "denial-with-two-reasons");
    }

    #[test]
    fn an_allow_needs_no_reason_and_is_not_a_problem() {
        // The other half: a gate that refused every case without `because` would
        // pass the two tests above and be wrong.
        let cases = parse(
            "x.toml",
            &one(r#"
[[case]]
name = "an ordinary read"
principal = 'User::"ana"'
action = "Read"
resource = 'Content::"photo"'
decision = "allow"
"#),
        )
        .expect("parses");
        assert_eq!(cases[0].decision, Decision::Allow);
    }

    #[test]
    fn a_boundary_flag_on_a_case_with_no_boundary_is_refused() {
        // Both flags, on an `allow` and on a denial a `forbid` caused. Neither
        // has a permit to fall outside of, so nothing would judge the flag.
        for (flag, decision, reason) in [
            (
                "the_principal_is_the_boundary",
                "allow",
                "because = \"family.a_member_steps_through_their_own_history\"",
            ),
            (
                "the_action_is_the_boundary",
                "deny",
                "because = \"family.a_frozen_account_acts_on_nothing\"",
            ),
        ] {
            let problems = parse(
                "x.toml",
                &one(&format!(
                    "
[[case]]
name = \"a flag nothing reads\"
{flag} = true
principal = 'User::\"bruno\"'
action = \"Write\"
resource = 'Realm::\"home\"'
decision = \"{decision}\"
{reason}
"
                )),
            )
            .expect_err("refused");
            assert_eq!(problems[0].check, "boundary-flag-without-a-boundary");
            assert!(
                problems[0].what.contains(flag)
                    && problems[0].what.contains("a flag nothing reads"),
                "the refusal names neither the flag nor the case: {}",
                problems[0].what
            );
        }
        // And the case the flag is for is accepted, or a rule refusing every
        // flag would pass the loop above.
        parse(
            "x.toml",
            &one(r#"
[[case]]
name = "a device steps through no history"
the_principal_is_the_boundary = true
principal = 'Device::"ana-laptop"'
action = "Write"
resource = 'Realm::"home"'
decision = "deny"
no_permit_matched = "family.a_member_steps_through_their_own_history"
"#),
        )
        .expect("a flag on the denial it describes parses");
    }

    #[test]
    fn an_unknown_field_is_refused_rather_than_ignored() {
        // A misspelled `because` that was silently dropped would turn a checked
        // denial into an unchecked one, and nothing would say so.
        let problems = parse(
            "x.toml",
            &one(r#"
[[case]]
name = "misspelled"
principal = 'User::"ana"'
action = "Read"
resource = 'Content::"photo"'
decision = "deny"
becuase = "core.fail_closed"
"#),
        )
        .expect_err("refused");
        assert_eq!(problems[0].check, "case-file-malformed");
    }
}
