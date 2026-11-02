//! Every guarantee the catalog claims, proven by **making it fail**.
//!
//! `NOCTUA.md §19.3` opens with a gate that was complete, tested and wired to
//! nothing for three milestones. The defence against that is not another unit
//! test of the checking function — it is running the real compiler over a real
//! catalog that has been broken on purpose, and reading what it says.
//!
//! Each test copies the repository's own `catalog/` into a scratch directory,
//! breaks exactly one thing, and asserts three things: that the build fails, that
//! **the check that fired is the one the test names**, and that the message
//! names the offending key, code or table.
//!
//! **The second of those was a finding.** An earlier version matched a substring
//! of the message against either field, so a test could be satisfied by a
//! completely different check that happened to mention the same word — one of
//! them was, and stayed green with the check it named switched off entirely.
//! `Problem::check` is a stable identity per check, and asserting on it is what
//! makes "this failed for the reason claimed" an assertion rather than a
//! coincidence.

use std::path::{Path, PathBuf};

use nc_catalog_compiler::ledger::Bootstrap;
use nc_catalog_compiler::{Problem, compile};

fn repository() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).to_path_buf()
}

/// A copy of the real catalog, in a directory of this test's own.
///
/// The real one, not a fixture: a fixture drifts from the registries it
/// imitates, and then these tests prove something about a catalog nobody ships.
struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("nc-catalog-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        copy(&repository().join("catalog"), &root.join("catalog"));
        Self { root }
    }

    /// Rewrites a file, refusing an edit that changed nothing.
    ///
    /// A `replace` whose needle has drifted is a no-op, and a no-op leaves the
    /// catalog valid — so the test passes for having broken nothing. That is
    /// `§19.5`'s vacuous assertion arriving through the fixture rather than
    /// through the assertion.
    ///
    /// **And the message says where to look, which it did not.** Every fixture
    /// here edits a copy of the repository's own `catalog/`, so a needle that
    /// stops matching means the *shipped registry* changed — a value weakened,
    /// a key renamed, a budget rewritten. Read as "this test proves nothing" it
    /// sends the reader to this file, where nothing is wrong, and the catalog
    /// edit that caused it goes unexamined. The failure a weakened catalog
    /// should produce is a statement about the catalog.
    fn edit(&self, relative: &str, edit: impl Fn(String) -> String) {
        let path = self.root.join(relative);
        let before = std::fs::read_to_string(&path).expect("the copy is readable");
        let after = edit(before.clone());
        assert_ne!(
            before, after,
            "the edit to {relative} changed nothing, so this test proves nothing — and \
             what it edits is a copy of the repository's own {relative}, so what changed \
             is that file. Read the shipped registry first: a value weakened or a key \
             renamed there is what makes a fixture here stop matching, and this test has \
             been proving nothing since it did"
        );
        std::fs::write(&path, after).expect("the copy is writable");
    }

    /// Replaces `from` with `to`, naming `from` when it is no longer there.
    ///
    /// The same floor as [`Scratch::edit`] with the needle in the message, so a
    /// fixture that has drifted says which line of the shipped registry it was
    /// written against instead of leaving a reader to diff the file.
    fn replacing(&self, relative: &str, from: &str, to: &str) {
        let path = self.root.join(relative);
        let before = std::fs::read_to_string(&path).expect("the copy is readable");
        assert!(
            before.contains(from),
            "{relative} in the repository no longer holds `{from}`, which this test \
             breaks on purpose. The shipped registry is what changed; this fixture is \
             written against the line that is gone"
        );
        std::fs::write(&path, before.replace(from, to)).expect("the copy is writable");
    }

    /// Adds a file the catalog did not have, refusing one that would overwrite.
    ///
    /// `edit` cannot reach a check that needs a *second* file — two locales
    /// whose tags camel-case to one variant is the case, and it needs a locale
    /// the copy does not hold. Refusing an existing path keeps this from being
    /// a silent `edit` with no floor under it.
    fn add(&self, relative: &str, text: &str) {
        let path = self.root.join(relative);
        assert!(
            !path.exists(),
            "{relative} already exists, so this would be an edit with no check that it \
             changed anything"
        );
        std::fs::write(&path, text).expect("the copy is writable");
    }

    fn problems(&self) -> Vec<Problem> {
        match compile(&self.root, Bootstrap::Refuse) {
            Ok(_) => Vec::new(),
            Err(problems) => problems,
        }
    }

    /// The problem this check produced, and there has to be exactly one.
    fn only(&self, check: &str) -> Problem {
        let mut matching: Vec<Problem> = self
            .problems()
            .into_iter()
            .filter(|problem| problem.check == check)
            .collect();
        assert_eq!(
            matching.len(),
            1,
            "`{check}` produced {} problem(s), so there is no one line to read back",
            matching.len()
        );
        matching.remove(0)
    }

    /// The file and line a diagnostic reports, with the line's text.
    ///
    /// **Eleven line numbers and nothing read one.** Every located diagnostic in
    /// this compiler carries a line, and no test in any suite ever opened the
    /// file and looked: measured in round sixty-two, each of six diagnostics'
    /// line expression replaced by the constant `1` survived
    /// `cargo test -p nc-catalog-compiler`. A line number nothing reads back is
    /// a number, not a location.
    ///
    /// The two callers below assert a line *computed from the file*, never a
    /// number written down here — a pinned number is the same defect one layer
    /// out, and it would have to be rewritten every time a locale file grows.
    fn reported(&self, check: &str) -> (String, usize, String) {
        let problem = self.only(check);
        let text = std::fs::read_to_string(self.root.join(&problem.file))
            .expect("the file a diagnostic names is readable");
        let Some(at) = problem.line else {
            panic!(
                "`{check}` filed its problem against {} as a whole, and it is about one key",
                problem.file
            );
        };
        let line = text
            .lines()
            .nth(at - 1)
            .unwrap_or_else(|| {
                panic!(
                    "`{check}` points at {}:{at} and that file has {} line(s)",
                    problem.file,
                    text.lines().count()
                )
            })
            .to_owned();
        (problem.file, at, line)
    }

    /// A key nobody wrote is filed where it would be written: after its group.
    ///
    /// The group is the keys sharing this one's prefix, and the *last* of them
    /// is the insertion point — the first of them, or the end of the file, both
    /// file a newcomer's key in the wrong block, which is what `where_to_write`
    /// exists to stop and what nothing was checking.
    fn points_after_the_group_of(&self, check: &str, key: &str) {
        let (file, at, line) = self.reported(check);
        let text = std::fs::read_to_string(self.root.join(&file)).expect("readable");
        let (prefix, _) = key.rsplit_once('.').expect("a message key has segments");
        let needle = format!("\"{prefix}.");
        let group: Vec<usize> = text
            .lines()
            .enumerate()
            .filter(|(_, held)| held.trim_start().starts_with(&needle))
            .map(|(index, _)| index + 1)
            .collect();
        assert!(
            !group.is_empty(),
            "no key in {file} shares `{prefix}.`, so this test is about nothing"
        );
        assert!(
            group.len() > 1,
            "only one key in {file} shares `{prefix}.`, so the first of the group and the \
             last of it are the same line and this proves neither"
        );
        assert_eq!(
            at,
            *group.last().expect("the group is not empty"),
            "`{check}` points at {file}:{at} — `{}` — and the group sharing `{prefix}.` \
             ends at line {:?}, which is where a newcomer told to add `{key}` would put it",
            line.trim(),
            group.last()
        );
    }

    /// A key that exists is filed on the line that declares it.
    fn points_at_the_line_declaring(&self, check: &str, key: &str) {
        let (file, at, line) = self.reported(check);
        assert!(
            line.trim_start().starts_with(&format!("\"{key}\"")),
            "`{check}` points at {file}:{at}, which reads `{}` and is not where `{key}` \
             is declared",
            line.trim()
        );
    }

    /// The build fails, `check` is what failed, and the diagnostic names
    /// `naming` — in its sentence or in the file it is filed under.
    fn must_fail(&self, check: &str, naming: &str) {
        self.refused(check, naming, Names::InTheDiagnostic);
    }

    /// The same, for a `naming` the *message* computed rather than a path.
    ///
    /// See [`Names`] for the three data this separation was measured against.
    fn must_fail_saying(&self, check: &str, naming: &str) {
        self.refused(check, naming, Names::InTheSentence);
    }

    fn refused(&self, check: &str, naming: &str, names: Names) {
        // **An empty name is refused, not accepted.** `what.contains("")` is
        // true, so passing `""` here silently switched off the second assertion
        // below — and four cases did exactly that, under a module header
        // promising every message names what broke.
        assert!(
            !naming.is_empty(),
            "`{check}` was checked without saying what its message must name"
        );
        let problems = self.problems();
        assert!(
            !problems.is_empty(),
            "the catalog was broken on purpose and the build accepted it"
        );
        let matching: Vec<&Problem> = problems
            .iter()
            .filter(|problem| problem.check == check)
            .collect();
        assert!(
            !matching.is_empty(),
            "nothing reported `{check}`; what did fire was {:?}\n{}",
            problems.iter().map(|p| p.check).collect::<Vec<_>>(),
            render(&problems)
        );
        // Against the diagnostic **as a reader sees it**, which is the file and
        // the sentence together. A problem about a whole file names it in `file`
        // and has nothing to repeat in `what`.
        assert!(
            matching.iter().any(|problem| match names {
                Names::InTheDiagnostic =>
                    problem.what.contains(naming) || problem.file.contains(naming),
                Names::InTheSentence => problem.what.contains(naming),
            }),
            "`{check}` fired and named none of `{naming}` ({names:?}), so a reader would \
             have to search for what broke:\n{}",
            render(&problems)
        );
        for problem in &problems {
            assert!(
                problem.fix.len() > 40,
                "a diagnostic that only says what is wrong has done half the job: {problem}"
            );
            assert!(
                !problem.check.is_empty(),
                "a problem with no check identity cannot be asserted on: {problem}"
            );
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Where a diagnostic has to name what broke.
///
/// **`naming` was satisfied by the file path, and six rows paid for it.**
/// `problem.what.contains(naming) || problem.file.contains(naming)` is the
/// right question when `naming` is a path — a problem about a whole file names
/// it in `file` and has nothing to repeat in `what` — and the wrong one when
/// `naming` is a *datum the sentence computed*. Measured, three data corrupted
/// and no path touched:
///
/// ```text
/// load.rs   floor(…, "emails")   -> "areas"   34 suites, 0 failures
/// load.rs   floor(…, "commands") -> "areas"   34 suites, 0 failures
/// lib.rs    case.name            -> "some case in this pack"
///                                             34 suites, 0 failures
/// ```
///
/// `catalog/emails.toml` then reported *"declares no areas"* and
/// `denial-was-not-a-missing-permit` named no case, with every suite green:
/// `"emails"` is a substring of `catalog/emails.toml`, so the file path was
/// answering for the sentence.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Names {
    /// Anywhere a reader looks — the sentence or the file it is filed under.
    InTheDiagnostic,
    /// The sentence itself, because `naming` is a datum the message computed
    /// and a path cannot stand in for it.
    InTheSentence,
}

fn render(problems: &[Problem]) -> String {
    problems
        .iter()
        .map(|problem| format!("  [{}] {problem}\n    {}", problem.check, problem.fix))
        .collect::<Vec<_>>()
        .join("\n")
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("the scratch directory is writable");
    for entry in std::fs::read_dir(from)
        .expect("the catalog is readable")
        .flatten()
    {
        let path = entry.path();
        let target = to.join(entry.file_name());
        if path.is_dir() {
            copy(&path, &target);
        } else {
            std::fs::copy(&path, &target).expect("the file is copyable");
        }
    }
}

#[test]
fn the_real_catalog_compiles() {
    // The control. Without it, every test below would pass on a catalog that
    // was already broken for an unrelated reason.
    let scratch = Scratch::new("control");
    let problems = scratch.problems();
    assert!(problems.is_empty(), "{}", render(&problems));
}

#[test]
fn a_diagnostic_about_something_present_in_a_file_says_which_line() {
    // §15.1 asks for the exact location. Three classes of i18n diagnostic used
    // to carry none, although the key was physically present in the file being
    // reported on.
    //
    // The name used to claim *every* diagnostic and this checked one class. The
    // rule cannot be "every diagnostic carries a line" — `missing-key` is about
    // a key that is **not** in the file, and there is no line to point at — so
    // what is asserted is the rule that can be true: a diagnostic naming
    // something the file contains says where it is.
    let scratch = Scratch::new("located");
    scratch.edit("catalog/i18n/pt-BR.toml", |mut text| {
        text.push_str("\n\"error.db.invented\" = \"Uma frase que ninguém procura.\"\n");
        text
    });
    let problems = scratch.problems();
    let orphan = problems
        .iter()
        .find(|problem| problem.check == "orphan-key")
        .expect("an orphan key is a problem");
    assert!(
        orphan.line.is_some(),
        "the key is in the file being reported on, and the diagnostic did not say where"
    );

    // And the counterpart, so the exception is stated rather than left as the
    // reason the rule above is narrow: a key absent from a locale is reported at
    // the end of the file it has to be written into, never at nothing.
    let missing = Scratch::new("located-missing");
    missing.edit("catalog/i18n/pt-BR.toml", |text| {
        text.lines()
            .filter(|line| !line.starts_with("\"error.db.prune-failed\""))
            .collect::<Vec<_>>()
            .join("\n")
    });
    let problems = missing.problems();
    let absent = problems
        .iter()
        .find(|problem| problem.check == "missing-key")
        .expect("a missing key is a problem");
    assert!(
        absent.line.is_some(),
        "a diagnostic about an absent key still has a file to point into"
    );
}

/// A key that is present and says nothing is refused.
///
/// **`""` and `"   "` passed every rule in `messages.rs`.** The completeness
/// guarantee is about a key's presence, never about it having a sentence: the
/// empty string was caught downstream by a generated-crate assertion, and the
/// whitespace one only by a Markdown cell that happens to trim — so an ordinary
/// tidy-up to that comparison would have shipped a blank translation with
/// `check passed`. Asked of the source locale too, because an empty English
/// sentence is a page with a gap in it.
#[test]
fn a_key_that_says_nothing_fails_the_build() {
    // **Both locale files, because the check was asked of only one.** This
    // test's own doc claimed the source locale was covered while its body
    // edited `pt-BR.toml` alone — and the call site sat past the `continue`
    // that skips the source. Retargeting this loop is what measured it: a blank
    // English sentence was accepted with no problem reported at all.
    for file in ["catalog/i18n/pt-BR.toml", "catalog/i18n/en.toml"] {
        for blank in ["\"\"", "\"   \""] {
            let scratch = Scratch::new("blank");
            scratch.edit(file, |text| {
                text.lines()
                    .map(|line| {
                        if line.starts_with("\"error.db.prune-failed\"") {
                            format!("\"error.db.prune-failed\" = {blank}")
                        } else {
                            line.to_owned()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
            scratch.must_fail("blank-message", "error.db.prune-failed");
            // **And it points at the line the key is on.** `§15.1` asks for the
            // exact location, and this diagnostic located itself with
            // `where_to_write` — whose own doc says it answers the line a key
            // that is *not* in the file should be written on. The key here is in
            // the file, so the number pointed seven lines past it, onto a
            // neighbouring key with a full sentence in it: a reader following
            // the location finds healthy prose and no gap.
            //
            // Read off the scratch copy rather than written down, because the
            // line moves whenever a key is added above it — the number is not
            // the subject, the correspondence is.
            let reported = scratch
                .problems()
                .into_iter()
                .find(|problem| problem.check == "blank-message")
                .and_then(|problem| problem.line)
                .expect("the blank-message diagnostic carries a line");
            let text = std::fs::read_to_string(scratch.root.join(file))
                .expect("the locale this test just edited");
            let holds_the_key = text
                .lines()
                .nth(reported - 1)
                .expect("the reported line is inside the file");
            assert!(
                holds_the_key.contains("\"error.db.prune-failed\""),
                "`blank-message` points at {file}:{reported}, which holds \
                 {holds_the_key:?} and not the key it is about"
            );
        }
    }
}

#[test]
fn every_diagnostic_is_english() {
    // A developer's build log used to read "Português do Brasil is missing …",
    // in a repository whose every artifact is English (§2.1).
    let scratch = Scratch::new("english");
    scratch.edit("catalog/i18n/pt-BR.toml", |text| {
        text.lines()
            .filter(|line| !line.starts_with("\"error.db.prune-failed\""))
            .collect::<Vec<_>>()
            .join("\n")
    });
    // **The loop is the whole test, so an empty one asserts nothing.** This
    // bypasses `must_fail`, which is what every sibling here uses to require the
    // check to have fired at all: measured, with the `missing-key` push deleted
    // from `messages.rs`, its two named canaries went red and this printed `ok`
    // over zero diagnostics. A `for` over an empty collection is `§19.5`'s
    // vacuous assertion in its plainest form.
    let problems = scratch.problems();
    assert!(
        !problems.is_empty(),
        "removing a key from one locale produced no diagnostic at all, so the endonym \
         rule below read nothing"
    );
    for problem in problems {
        assert!(
            !problem.what.contains("Português"),
            "a diagnostic carried the locale's endonym: {problem}"
        );
    }
}

// --- i18n ---

#[test]
fn a_key_missing_from_one_locale_fails_the_build_and_the_error_names_it() {
    let scratch = Scratch::new("missing-key");
    scratch.edit("catalog/i18n/pt-BR.toml", |text| {
        text.lines()
            .filter(|line| !line.starts_with("\"error.db.prune-failed\""))
            .collect::<Vec<_>>()
            .join("\n")
    });
    scratch.must_fail("missing-key", "error.db.prune-failed");
}

/// A locale file is one whatever case its extension is written in.
///
/// **The rule reads the extension case-insensitively and nothing drove that.**
/// Written `== "toml"`, a file named `fr.TOML` stops being a locale — it is not
/// read, so it is not in any completeness check, and a locale missing every key
/// in the catalog passes the build in silence. The whole suite stayed green
/// under that narrowing, which is what makes it worth a fixture: the repair
/// exists because a translator's editor, or a case-insensitive filesystem, can
/// produce that name without anyone choosing it.
///
/// The fixture removes a key from the renamed file, so the failure is the
/// completeness rule firing — proof the file was *read*, not merely present.
#[test]
fn a_locale_file_is_one_whatever_case_its_extension_is_written_in() {
    let scratch = Scratch::new("shouting-extension");
    let from = scratch.root.join("catalog/i18n/pt-BR.toml");
    let to = scratch.root.join("catalog/i18n/pt-BR.TOML");
    std::fs::rename(&from, &to).expect("the copy is ours to rename");
    let text = std::fs::read_to_string(&to).expect("the copy is readable");
    std::fs::write(
        &to,
        text.lines()
            .filter(|line| !line.starts_with("\"error.db.prune-failed\""))
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .expect("the copy is writable");
    scratch.must_fail("missing-key", "error.db.prune-failed");
}

#[test]
fn a_key_only_one_locale_has_fails_the_build_too() {
    // The direction that gets forgotten. Nothing looks this key up, so without a
    // check it is a translation of a sentence that does not exist, maintained
    // forever by people who cannot tell.
    let scratch = Scratch::new("extra-key");
    scratch.edit("catalog/i18n/pt-BR.toml", |mut text| {
        text.push_str("\n\"error.db.invented\" = \"Uma frase que ninguém procura.\"\n");
        text
    });
    scratch.must_fail("orphan-key", "error.db.invented");
}

#[test]
fn a_placeholder_that_differs_from_the_source_fails_the_build() {
    let scratch = Scratch::new("placeholder");
    scratch.edit("catalog/i18n/pt-BR.toml", |text| {
        text.replace(
            "{field} precisa estar entre {min} e {max}.",
            "{campo} precisa estar entre {min} e {max}.",
        )
    });
    scratch.must_fail("placeholders-differ", "error.validation.out-of-range");
}

#[test]
fn a_locale_file_whose_tag_disagrees_with_its_name_fails_the_build() {
    let scratch = Scratch::new("tag");
    scratch.edit("catalog/i18n/pt-BR.toml", |text| {
        text.replace("bcp47 = \"pt-BR\"", "bcp47 = \"pt\"")
    });
    // The tag the file now declares, which is the datum the sentence computed;
    // `pt-BR` alone is in the file path and would be answered for by it.
    scratch.must_fail_saying("locale-tag-mismatch", "bcp47 `pt`");
}

#[test]
fn a_registry_entry_pointing_at_a_key_nobody_wrote_fails_the_build() {
    // Through a **notification**, not an error. Pointing an error at a missing
    // key also repoints its ledger reservation, and the earlier version of this
    // test was satisfied entirely by the ledger's diagnostic — it stayed green
    // with the check it names disabled.
    let scratch = Scratch::new("dangling-key");
    scratch.edit("catalog/notifications.toml", |text| {
        text.replace(
            "body = \"notification.catalog.mechanism.body\"",
            "body = \"notification.catalog.no-such-sentence\"",
        )
    });
    scratch.must_fail(
        "dangling-message-key",
        "notification.catalog.no-such-sentence",
    );
}

#[test]
fn a_sentence_that_uses_what_nothing_declares_fails_the_build() {
    let scratch = Scratch::new("params-undeclared");
    scratch.edit("catalog/notifications.toml", |text| {
        text.replace("params = [\"name\"]", "params = []")
    });
    scratch.must_fail("param-not-declared", "catalog.mechanism");
}

#[test]
fn a_title_is_checked_and_not_only_a_body() {
    // The hole this check had: an entry governs **two** messages and only the
    // body was compared, under a comment promising both were. A subject or a
    // title could interpolate anything at all and the build stayed green, and
    // the reader would see the braces.
    //
    // Edited in both locales, so `placeholders-differ` cannot be what fires and
    // this cannot pass for the wrong reason.
    let scratch = Scratch::new("params-title");
    for locale in ["en", "pt-BR"] {
        scratch.edit(&format!("catalog/i18n/{locale}.toml"), |text| {
            text.replace(
                "\"notification.catalog.mechanism.title\" = \"",
                "\"notification.catalog.mechanism.title\" = \"{who} ",
            )
        });
    }
    scratch.must_fail("param-not-declared", "notification.catalog.mechanism.title");
}

#[test]
fn an_email_subject_is_checked_too() {
    // The same hole on the other registry, because a check written for one of
    // two shapes is a check that will be written for one of three.
    let scratch = Scratch::new("params-subject");
    for locale in ["en", "pt-BR"] {
        scratch.edit(&format!("catalog/i18n/{locale}.toml"), |text| {
            text.replace(
                "\"email.catalog.mechanism.subject\" = \"",
                "\"email.catalog.mechanism.subject\" = \"{who} ",
            )
        });
    }
    scratch.must_fail("param-not-declared", "email.catalog.mechanism.subject");
}

#[test]
fn a_command_message_may_not_interpolate_anything() {
    // A command has no `params` field at all, so a placeholder in its title is
    // one nobody can ever fill in — there is no caller, only a palette and the
    // generated developer surfaces. Declaring the empty set is what makes that a
    // build failure rather than braces on a screen.
    let scratch = Scratch::new("params-command");
    for locale in ["en", "pt-BR"] {
        scratch.edit(&format!("catalog/i18n/{locale}.toml"), |text| {
            text.replace(
                "\"command.journal.verify.title\" = \"",
                "\"command.journal.verify.title\" = \"{who} ",
            )
        });
    }
    scratch.must_fail("param-not-declared", "command.journal.verify.title");
}

#[test]
fn a_declaration_no_sentence_uses_fails_the_build() {
    // The other direction, and the one a rename leaves behind: every caller is
    // made to supply a value that no reader ever sees.
    let scratch = Scratch::new("params-unused");
    scratch.edit("catalog/notifications.toml", |text| {
        text.replace("params = [\"name\"]", "params = [\"name\", \"leftover\"]")
    });
    scratch.must_fail("param-never-used", "leftover");
}

/// Every located diagnostic points at a line the file actually has, computed.
///
/// Six checks, each broken the way its own test breaks it, and the line each one
/// reports opened and compared with a line worked out from the file. This is the
/// half `must_fail` never covered: it reads the sentence and the file, and never
/// the number between them.
#[test]
fn a_key_nobody_wrote_is_filed_after_the_group_it_belongs_to() {
    let missing = Scratch::new("located-missing-key");
    missing.edit("catalog/i18n/pt-BR.toml", |text| {
        text.lines()
            .filter(|line| !line.starts_with("\"error.db.prune-failed\""))
            .collect::<Vec<_>>()
            .join("\n")
    });
    missing.points_after_the_group_of("missing-key", "error.db.prune-failed");

    let dangling = Scratch::new("located-dangling-key");
    dangling.edit("catalog/notifications.toml", |text| {
        text.replace(
            "body = \"notification.catalog.mechanism.body\"",
            "body = \"notification.catalog.no-such-sentence\"",
        )
    });
    dangling.points_after_the_group_of(
        "dangling-message-key",
        "notification.catalog.no-such-sentence",
    );
}

/// A diagnostic about a key that exists is filed on that key's own line.
#[test]
fn a_diagnostic_about_a_key_that_exists_is_filed_on_that_keys_line() {
    let ghost = Scratch::new("located-ghost-key");
    for locale in ["en", "pt-BR"] {
        ghost.edit(&format!("catalog/i18n/{locale}.toml"), |text| {
            format!("{text}\n\"error.db.ghost\" = \"A ghost sentence nothing points at.\"\n")
        });
    }
    ghost.points_at_the_line_declaring("unreferenced-key", "error.db.ghost");

    let undeclared = Scratch::new("located-params-undeclared");
    undeclared.edit("catalog/notifications.toml", |text| {
        text.replace("params = [\"name\"]", "params = []")
    });
    undeclared
        .points_at_the_line_declaring("param-not-declared", "notification.catalog.mechanism.body");

    let unused = Scratch::new("located-params-unused");
    unused.edit("catalog/notifications.toml", |text| {
        text.replace("params = [\"name\"]", "params = [\"name\", \"leftover\"]")
    });
    unused.points_at_the_line_declaring("param-never-used", "notification.catalog.mechanism.title");
}

// --- error codes ---

#[test]
fn a_renumbered_error_code_fails_the_build() {
    // The append-only rule. Every log entry ever written with a code still
    // refers to it, so repointing one makes history lie.
    let scratch = Scratch::new("renumbered");
    scratch.edit("catalog/errors.toml", |text| {
        text.replace("code = \"NC-DB-0003\"", "code = \"NC-DB-0009\"")
    });
    scratch.must_fail("code-removed", "NC-DB-0003");
}

#[test]
fn repointing_a_code_at_a_different_error_fails_the_build() {
    // Repointed at a key of its own, written into both locales in the same edit.
    // Repointing at a key another code already uses would fire `shared-message`
    // as well, and a test that accepts either is a test that proves neither.
    let scratch = Scratch::new("repointed");
    scratch.edit("catalog/i18n/en.toml", |mut text| {
        text.push_str("\n\"error.db.something-else\" = \"Something else went wrong.\"\n");
        text
    });
    scratch.edit("catalog/i18n/pt-BR.toml", |mut text| {
        text.push_str("\n\"error.db.something-else\" = \"Aconteceu outra coisa.\"\n");
        text
    });
    scratch.edit("catalog/errors.toml", |text| {
        text.replace(
            "message = \"error.db.prune-failed\"",
            "message = \"error.db.something-else\"",
        )
    });
    scratch.must_fail("code-repointed", "NC-DB-0003");
}

#[test]
fn deleting_the_ledger_does_not_free_a_single_code() {
    // `rm catalog/errors.reserved.toml` used to free every code in the
    // repository, and the next build blessed whatever renumbering followed.
    let scratch = Scratch::new("no-ledger");
    std::fs::remove_file(scratch.root.join("catalog/errors.reserved.toml")).expect("removable");
    // **The file, not a word of the sentence.** These asserted `"missing"` and
    // `"checksum"` against templates that always print `is missing` and
    // `checksum`, so the second half of `must_fail` — the half the module header
    // calls "the message names the offending key, code or table" — could not
    // fail while the check fired. The `!naming.is_empty()` guard closes only the
    // empty string. A file-wide diagnostic names its file.
    scratch.must_fail("ledger-missing", "errors.reserved.toml");
}

#[test]
fn clearing_the_ledgers_checksum_does_not_free_a_code() {
    // The door the first fix left open: keep the reservations, drop the
    // checksum, repoint a code — and `cargo xtask check` reported `check passed`
    // on a catalog whose error code had silently changed meaning.
    let scratch = Scratch::new("cleared-checksum");
    scratch.edit("catalog/errors.reserved.toml", |text| {
        text.lines()
            .map(|line| {
                if line.starts_with("checksum") {
                    "checksum = \"\"".to_owned()
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    });
    scratch.must_fail("ledger-unvouched", "errors.reserved.toml");
}

#[test]
fn hand_editing_the_ledger_does_not_ratify_itself() {
    // It used to: the ledger was generated output compared against output
    // derived from the ledger, so `check` reported "in sync" after a hand edit.
    let scratch = Scratch::new("edited-ledger");
    scratch.edit("catalog/errors.reserved.toml", |text| {
        text.lines()
            .filter(|line| !line.starts_with("\"NC-POL-0002\""))
            .collect::<Vec<_>>()
            .join("\n")
    });
    scratch.must_fail("ledger-checksum", "errors.reserved.toml");
}

#[test]
fn an_event_may_not_declare_an_identifier_field_while_5_1_is_open() {
    // **The twelfth way into the type wall**, and the registry itself offered
    // it. `type = "id"` generates a field of type `nc_ids::OpaqueId`, and
    // `OpaqueId::new` accepts any 1..=128 bytes of `[A-Za-z0-9._:-]` — a
    // username, a document number, a phone number, an email local-part.
    // Measured: one registry line, an ordinary call site with no `format!`, no
    // `to_string()`, no leak and no macro, and
    // `{"actor":"ana.paula.silva.123.456.789-00"}` reached the store with all
    // eleven compile-fail fixtures green and `check passed`.
    //
    // None of the fixtures refuses it because none of them can: the field is
    // generated, the value is a legitimate `OpaqueId`, and every type involved
    // is exactly what the registry asked for. The wall's guarantee rested on the
    // *contents of the registry* rather than on the types, which is not what
    // `§15.4` claims.
    let scratch = Scratch::new("field-kind-id");
    scratch.edit("catalog/log-events.toml", |text| {
        text.replace(
            "{ name = \"table\", type = \"literal\" },",
            "{ name = \"table\", type = \"literal\" },\n               { name = \"actor\", type = \"id\" },",
        )
    });
    scratch.must_fail("field-kind-is-open", "actor");

    // And the kinds that stay: a minted identifier and a name from the source.
    for kind in ["snowflake", "literal"] {
        let allowed = Scratch::new(&format!("field-kind-{kind}"));
        allowed.edit("catalog/log-events.toml", |text| {
            text.replace(
                "{ name = \"table\", type = \"literal\" },",
                &format!(
                    "{{ name = \"table\", type = \"literal\" }},\n                       {{ name = \"actor\", type = \"{kind}\" }},"
                ),
            )
        });
        let problems = allowed.problems();
        // **Empty, not merely free of one check.** `!any(…)` over an empty
        // list is true whatever the compiler did, and an iterator probe over the
        // whole suite measured this closure never seeing an element. A kind the
        // registry declares has to compile *clean*, which is the assertion.
        assert!(problems.is_empty(), "`{kind}` was refused: {problems:#?}");
    }
}

#[test]
fn an_event_may_not_declare_a_field_the_log_writes_itself() {
    // They collided in one map and the event's won, so a line could carry a
    // level its event does not have and a service it did not come from. The
    // ordering in `nc-log` is one half; refusing the name here is the other, and
    // it is the half a reader meets when they write the field.
    for owned in ["_msg", "level", "service", "version"] {
        let scratch = Scratch::new(&format!("owned-{}", owned.trim_start_matches('_')));
        scratch.edit("catalog/log-events.toml", |text| {
            text.replace(
                "{ name = \"table\", type = \"literal\" },",
                &format!(
                    "{{ name = \"table\", type = \"literal\" }},\n  \
                     {{ name = \"{owned}\", type = \"literal\" }},"
                ),
            )
        });
        scratch.must_fail("field-the-log-owns", owned);
    }
}

#[test]
fn the_pages_carry_values_the_shipped_registry_happens_not_to_use() {
    // **The same argument as the test below, for the six published pages.**
    // Three branches of `docs.rs` cannot be falsified against the shipped
    // catalog at all:
    //
    // * `cell()`, "the only thing between a translator's sentence and a table
    //   that renders as garbage" — no message in either locale contains a `|`,
    //   so replacing it with the identity function left everything green;
    // * `parameters()`'s populated branch — the one email declares none;
    // * `"It carries no fields."` — all the events have fields.
    //
    // And two columns of `commands.md` that the generated-source twin below
    // covers and the page side did not: **no command declares a keybinding**, so
    // the page's `—` was a constant, and **both declare the same resource
    // selector**, so `realm` was one too. Those are the two facts a dispatcher
    // and a keyboard user each read first.
    //
    // Driven over a copy that does carry them, rather than by inventing product
    // decisions in the registry that ships.
    let scratch = Scratch::new("page-diversity");
    for locale in ["en", "pt-BR"] {
        scratch.edit(&format!("catalog/i18n/{locale}.toml"), |text| {
            text.replace(
                "\"email.catalog.mechanism.body\" = \"",
                "\"email.catalog.mechanism.body\" = \"{who} a | b ",
            )
        });
    }
    scratch.edit("catalog/emails.toml", |text| {
        text.replace("params = []", "params = [\"who\"]")
    });
    // **The notification's severity**, which the shipped registry has only one
    // value of: publishing a literal `info` produced a byte-identical page and
    // everything green. The generated-source twin covers this; the page side
    // did not.
    scratch.edit("catalog/notifications.toml", |text| {
        text.replace("severity = \"info\"", "severity = \"warning\"")
    });
    // One command differs and the other does not, so a constant fails whichever
    // value it is: `—` misses the declared key, `C-c C-v` misses the absent one.
    // The selector reads an argument the schema has to back — declared,
    // required, a bounded string naming its entity type — so the schema grows
    // the argument it selects.
    scratch.edit("catalog/commands.toml", |text| select_an_argument(&text));
    scratch.edit("catalog/log-events.toml", |text| {
        text.replace(
            "[[event]]\nname = \"journal.appended\"",
            "[[event]]\nname = \"catalog.nothing_to_say\"\n             description = \"An event with no fields at all\"\n             level = \"debug\"\nfields = []\n\n             [[event]]\nname = \"journal.appended\"",
        )
    });

    let compiled = compile(&scratch.root, Bootstrap::Refuse)
        .expect("a catalog that differs only in these values still compiles");
    let page = |name: &str| {
        compiled
            .files
            .iter()
            .find(|file| file.path.ends_with(name))
            .unwrap_or_else(|| panic!("{name} is emitted"))
            .contents
            .clone()
    };

    // A `|` inside a table cell ends the cell, so the row after it is one column
    // short and every value beyond it moves left by one.
    let emails = page("emails.md");
    assert!(
        emails.contains("a \\| b"),
        "a pipe in a message was published raw, which breaks the row it is in"
    );
    assert!(
        !emails.lines().any(|line| line.contains("a | b")),
        "a pipe reached a table cell unescaped"
    );
    assert!(
        emails.contains("Parameters: `{who}`"),
        "an email's parameters did not reach its page"
    );

    let notifications = page("notifications.md");
    assert!(
        notifications.contains("Severity: warning."),
        "a notification's severity did not reach its page"
    );

    // **Both columns, on the page, and in both directions.** The section of the
    // command that declares them publishes them; the section of the one that
    // does not publishes no keybinding row at all.
    //
    // **`Keybinding | — |` was on every row of the shipped page**, because no
    // command declares one — a column of em-dashes a reader has to look at
    // twice to learn it says nothing. The row appears when there is a binding
    // and not otherwise, and this test drives both halves.
    let commands = page("commands.md");
    let section = |name: &str| -> String {
        let opening = format!("## `{name}`\n");
        let at = commands
            .find(&opening)
            .unwrap_or_else(|| panic!("no section for `{name}`"));
        let rest = &commands[at + opening.len()..];
        rest.find("\n## ")
            .map_or(rest, |end| &rest[..end])
            .to_owned()
    };
    let declared = section("journal.verify");
    assert!(
        declared.contains("| Keybinding | `C-c C-v` |"),
        "a declared keybinding did not reach the page: {declared}"
    );
    assert!(
        declared.contains("| Resource selector | `argument:target` |"),
        "a resource selector did not reach the page: {declared}"
    );
    let bare = section("limits.prune");
    assert!(
        !bare.contains("| Keybinding |"),
        "a command with no keybinding published an empty row for one: {bare}"
    );
    assert!(
        bare.contains("| Resource selector | `realm` |"),
        "the other command's resource selector was published: {bare}"
    );

    let events = page("log-events.md");
    assert!(
        events.contains("It carries no fields."),
        "an event with no fields published a table with no rows"
    );
    // And the branch is not the only one taken: an event *with* fields still
    // publishes its table, so a constant would not pass.
    assert!(
        events.contains("| Field | Kind | Optional |"),
        "no event published its fields"
    );
}

/// `journal.verify` selecting `argument:target` with a keybinding, and the
/// schema declaring the argument that selector reads.
///
/// The two diversity fixtures share it because the selector is judged against
/// the schema: `argument:target` alone, over a schema with no `target`, is a
/// catalog `nc_schema::judge` refuses, and the fixtures are about the emitter.
fn select_an_argument(text: &str) -> String {
    text.replacen(
        "resource_selector = \"realm\"",
        "resource_selector = \"argument:target\"\nkeybinding = \"C-c C-v\"",
        1,
    )
    .replace(
        r#"{"type":"object","properties":{"from":{"type":"integer","minimum":0}},"additionalProperties":false}"#,
        r#"{"type":"object","properties":{"from":{"type":"integer","minimum":0},"target":{"type":"string","x-entity":"Content","maxLength":128}},"required":["target"],"additionalProperties":false}"#,
    )
}

#[test]
fn the_emitter_carries_values_the_shipped_registry_happens_not_to_use() {
    // **A comparison is only as falsifiable as the registry is diverse.** Four
    // fields in `xtask/tests/the_registry_reaches_the_code.rs` are compared
    // against a registry that has exactly one value for each, so an emitter
    // writing that value as a constant passes: no command declares a keybinding,
    // both declare the same resource selector, the one email declares no
    // parameters, and the one notification is `info`. The `Some(…)` branch of
    // `keybinding` was dead code with no test at all.
    //
    // Rather than invent product decisions in the shipped registry, this drives
    // the emitter over a copy that does differ, and asserts each value reaches
    // the generated source.
    let scratch = Scratch::new("emitter-diversity");
    scratch.edit("catalog/commands.toml", |text| select_an_argument(&text));
    scratch.edit("catalog/notifications.toml", |text| {
        text.replace("severity = \"info\"", "severity = \"warning\"")
    });
    // The one this test named and did not touch: it edited commands and
    // notifications and asserted on their modules, never on an email. Replacing
    // the email's params with a constant `&[]` was green.
    scratch.edit("catalog/emails.toml", |text| {
        text.replace("params = []", "params = [\"who\"]")
    });
    for locale in ["en", "pt-BR"] {
        scratch.edit(&format!("catalog/i18n/{locale}.toml"), |text| {
            text.replace(
                "\"email.catalog.mechanism.body\" = \"",
                "\"email.catalog.mechanism.body\" = \"{who} ",
            )
        });
    }
    scratch.edit("catalog/i18n/pt-BR.toml", |text| {
        text.replace("dir   = \"ltr\"", "dir   = \"rtl\"")
    });

    let compiled = compile(&scratch.root, Bootstrap::Refuse)
        .expect("a catalog that differs only in these values still compiles");
    let commands = compiled
        .files
        .iter()
        .find(|file| file.path.ends_with("commands.rs"))
        .expect("the commands module is emitted");
    assert!(
        commands.contents.contains("Some(\"C-c C-v\")"),
        "a declared keybinding did not reach the generated source"
    );
    assert!(
        commands.contents.contains("\"argument:target\""),
        "a resource selector did not reach the generated source"
    );
    let notifications = compiled
        .files
        .iter()
        .find(|file| file.path.ends_with("notifications.rs"))
        .expect("the notifications module is emitted");
    assert!(
        notifications.contents.contains("Severity::Warning"),
        "a notification's severity did not reach the generated source"
    );
    // And the writing direction, which both shipped locales share — so emitting
    // a constant `"ltr"` produced a byte-identical file and nothing went red. On
    // the day an RTL locale arrives, `<html dir>` would be wrong platform-wide
    // (`§10.9`) and this is the only thing that would say so.
    let locales = compiled
        .files
        .iter()
        .find(|file| file.path.ends_with("i18n.rs"))
        .expect("the i18n module is emitted");
    assert!(
        locales.contents.contains("\"rtl\""),
        "a locale's writing direction did not reach the generated source"
    );

    let emails = compiled
        .files
        .iter()
        .find(|file| file.path.ends_with("emails.rs"))
        .expect("the emails module is emitted");
    assert!(
        emails.contents.contains("\"who\""),
        "an email's declared parameter did not reach the generated source"
    );
}

#[test]
fn every_file_the_emitter_writes_is_somewhere_the_stale_check_walks() {
    // `GENERATED_DIRECTORIES` is what `cargo xtask check` walks looking for a
    // file that carries the generated header and that nothing emits any more.
    // Nothing tied it to what the emitter actually writes, so an emitter that
    // began writing outside those directories would leave the leftovers there
    // invisible — and `build` never deletes.
    //
    // A later stage that emits into a hand-written crate cannot simply add its
    // directory here: `GENERATED_DIRECTORIES` means *every* file under it is
    // generated, and a crate with one generated module and five hand-written ones
    // would then be reported as five stale files. It has to be listed as a file,
    // and this test is where that decision has to be made rather than discovered.
    let compiled = compile(&repository(), Bootstrap::Refuse).expect("the catalog compiles");
    assert!(
        !compiled.files.is_empty(),
        "nothing was emitted, so this proves nothing"
    );
    for file in &compiled.files {
        let in_a_generated_directory = nc_catalog_compiler::GENERATED_DIRECTORIES
            .iter()
            .any(|directory| file.path.starts_with(&format!("{directory}/")));
        let named_as_a_generated_file =
            nc_catalog_compiler::GENERATED_FILES.contains(&file.path.as_str());
        assert!(
            in_a_generated_directory || named_as_a_generated_file,
            "`{}` is emitted somewhere the stale-file check does not look, so a leftover \
             beside it would never be reported. Either it belongs under a directory that \
             is wholly generated, or it has to be named in GENERATED_FILES",
            file.path
        );
        assert!(
            !(in_a_generated_directory && named_as_a_generated_file),
            "`{}` is claimed twice, and the two rules mean different things",
            file.path
        );
    }
}

#[test]
fn a_locale_that_cannot_be_read_is_reported_as_such() {
    // The one check identity of fifty-odd that nothing exercised, found by
    // suppressing each in turn and seeing which left the suite green.
    //
    // Reached with a **directory** named like a locale rather than with
    // permissions: `chmod 0` does nothing when the tests run as root, which they
    // do in a container, and a test that quietly passes for that reason is worse
    // than none. The listing filters on the extension and not on being a file,
    // so this is the same path a real unreadable file takes.
    let scratch = Scratch::new("locale-unreadable");
    std::fs::create_dir_all(scratch.root.join("catalog/i18n/xx.toml"))
        .expect("the copy is writable");
    scratch.must_fail("locale-unreadable", "xx.toml");
}

#[test]
fn the_remedy_code_repointed_prescribes_actually_works() {
    // `code-repointed`'s fix used to end "rename it in both places at once — the
    // ledger follows a rename when the code and the sentence move together". The
    // ledger follows nothing: renaming the key alone repeats the same error for
    // ever, and hand-editing the ledger as well fails its checksum. A remedy that
    // sends the reader in a circle is worse than none, because they trust it.
    //
    // This walks the remedy that replaced it and asserts the build comes back.
    let scratch = Scratch::new("repoint-remedy");
    for file in [
        "catalog/errors.toml",
        "catalog/i18n/en.toml",
        "catalog/i18n/pt-BR.toml",
    ] {
        scratch.edit(file, |text| {
            text.replace("error.db.prune-failed", "error.db.prune-did-not-finish")
        });
    }
    // Renaming the key alone is refused, which is the half that must not change.
    scratch.must_fail("code-repointed", "NC-DB-0003");

    // **And the remedy this walks is the remedy the diagnostic prints.** Without
    // this the test hard-codes a procedure and asserts that procedure works,
    // which stays green while the text goes back to prescribing something that
    // does not — the exact defect it was written for. Restoring the old wording
    // has to turn this red.
    let problems = scratch.problems();
    let repointed = problems
        .iter()
        .find(|problem| problem.check == "code-repointed")
        .expect("it fired a moment ago");
    for artefact in [
        "catalog/errors.reserved.toml",
        "checksum",
        "--reserve",
        "both locales",
    ] {
        assert!(
            repointed.fix.contains(artefact),
            "the remedy does not mention `{artefact}`, and the steps below are what it \
             has to describe:\n{}",
            repointed.fix
        );
    }

    // Then the remedy: take the old reservation out, blank the checksum, and
    // reserve. Blanking the checksum alone is not enough and must not be —
    // `Bootstrap::Allow` forgives a ledger nothing vouches for, and it does not
    // forgive a code that changed meaning.
    scratch.edit("catalog/errors.reserved.toml", |text| {
        text.lines()
            .filter(|line| !line.starts_with("\"NC-DB-0003\""))
            .map(|line| {
                if line.starts_with("checksum = ") {
                    "checksum = \"\"".to_owned()
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    });
    let compiled = compile(&scratch.root, Bootstrap::Allow)
        .expect("the remedy the diagnostic prescribes has to leave a catalog that compiles");
    assert!(
        compiled
            .ledger
            .contents
            .contains("error.db.prune-did-not-finish"),
        "the rename did not reach the ledger, so the remedy did not remedy anything"
    );
    assert!(
        !compiled.ledger.contents.contains("error.db.prune-failed"),
        "the old meaning is still reserved, so the ledger now claims both"
    );
}

#[test]
fn a_duplicate_error_code_fails_the_build() {
    let scratch = Scratch::new("duplicate");
    scratch.edit("catalog/errors.toml", |mut text| {
        text.push_str(
            "\n[[error]]\ncode = \"NC-DB-0001\"\ndescription = \"A second claim on one code\"\n\
             message = \"error.db.second\"\nseverity = \"error\"\nretryable = false\n",
        );
        text
    });
    scratch.must_fail("duplicate-code", "NC-DB-0001");
}

#[test]
fn two_codes_sharing_one_message_fail_the_build() {
    // They would generate one Rust variant twice, and `cargo xtask build` used
    // to report `ok` before rustc failed inside a file whose header forbids
    // editing it.
    let scratch = Scratch::new("shared-message");
    scratch.edit("catalog/errors.toml", |mut text| {
        text.push_str(
            "\n[[error]]\ncode = \"NC-DB-0004\"\ndescription = \"Another way to say it\"\n\
             message = \"error.db.unreachable\"\nseverity = \"error\"\nretryable = false\n",
        );
        text
    });
    scratch.must_fail("shared-message", "NC-DB-0004");
}

#[test]
fn a_code_in_an_undeclared_area_fails_the_build() {
    let scratch = Scratch::new("area");
    scratch.edit("catalog/errors.toml", |text| {
        text.replace("code = \"NC-VAL-0001\"", "code = \"NC-XYZ-0001\"")
    });
    scratch.must_fail("unknown-area", "XYZ");
}

#[test]
fn a_code_that_does_not_match_the_scheme_fails_the_build() {
    let scratch = Scratch::new("scheme");
    scratch.edit("catalog/errors.toml", |text| {
        text.replace("code = \"NC-VAL-0001\"", "code = \"NC-VAL-1\"")
    });
    scratch.must_fail("malformed-code", "NC-VAL-1");
}

#[test]
fn an_empty_description_fails_the_build() {
    let scratch = Scratch::new("no-description");
    scratch.edit("catalog/errors.toml", |text| {
        text.replace(
            "description = \"Pruning a table did not finish, and a later run may\"",
            "description = \"\"",
        )
    });
    scratch.must_fail("empty-description", "NC-DB-0003");
}

// --- names that become Rust items ---

#[test]
fn a_name_that_is_not_a_rust_identifier_fails_here_rather_than_inside_rustc() {
    // `2fa.enabled` used to reach rustfmt, whose error names a byte offset into
    // a stream the developer cannot open.
    let scratch = Scratch::new("identifier");
    scratch.edit("catalog/notifications.toml", |text| {
        text.replace("id = \"catalog.mechanism\"", "id = \"2fa.enabled\"")
    });
    scratch.must_fail("name-not-an-identifier", "2fa.enabled");
}

#[test]
fn two_names_that_differ_only_in_a_hyphen_collide() {
    // `camel` maps `-` and `_` identically, so `prune-failed` and
    // `prune_failed` become one variant. Nothing said so until rustc did.
    let scratch = Scratch::new("collision");
    scratch.edit("catalog/log-events.toml", |mut text| {
        text.push_str(
            "\n[[event]]\nname = \"prune.within-limit\"\ndescription = \"A collision\"\n\
             level = \"debug\"\nfields = []\n",
        );
        text
    });
    scratch.must_fail("name-collision", "prune.within-limit");
}

// --- an unknown field is a control that silently stops applying ---

/// Every typo this build must refuse, and the struct each one reaches.
///
/// Written beside the test rather than inside it: a row per registry file and
/// one per nested table crosses the hundred-line budget `clippy.toml` sets, and
/// a budget met by *reaching fewer structs* is the defect this table exists to
/// close.
const A_TYPO_REFUSED: [(&str, &str, &str, &str, &str); 20] = [
    (
        "lockout-typo",
        "catalog/limits.toml",
        "on_lockout = ",
        "on_lockut = ",
        "on_lockut",
    ),
    (
        "attempts-typo",
        "catalog/limits.toml",
        "lockout_after = 8",
        "lockout_aftr = 8",
        "lockout_aftr",
    ),
    (
        "memory-typo",
        "catalog/limits.toml",
        "memory_kib = 19_456",
        "memory_kb = 19_456",
        "memory_kb",
    ),
    (
        "optional-typo",
        "catalog/log-events.toml",
        "optional = true",
        "optionall = true",
        "optionall",
    ),
    // **And one per registry, because twenty attributes were decided by
    // three.** `model.rs` carries `deny_unknown_fields` on twenty structs
    // and the rows above reach `limits.toml` and `log-events.toml` only:
    // measured by a reviewer on 2026-09-13, the attribute deleted from
    // `ErrorEntry` left the whole tree green, and a `note = "…"` added to
    // `catalog/errors.toml` was then silently dropped instead of failing
    // the build.
    (
        "error-typo",
        "catalog/errors.toml",
        "description = \"The journal's hash chain is broken at an entry\"",
        "descriptionn = \"The journal's hash chain is broken at an entry\"",
        "descriptionn",
    ),
    (
        "command-typo",
        "catalog/commands.toml",
        "resource_selector = \"realm\"",
        "resource_selecter = \"realm\"",
        "resource_selecter",
    ),
    (
        "email-typo",
        "catalog/emails.toml",
        "subject = \"email.catalog.mechanism.subject\"",
        "subjekt = \"email.catalog.mechanism.subject\"",
        "subjekt",
    ),
    // **A registry file is not a struct.** The rows above reach one struct
    // per file and `limits.toml` alone has seven; deleting the attribute
    // from `TableLimit` left the whole compiler suite green — measured by a
    // reviewer on 2026-09-13, with `retention_days = 30` on a PostgreSQL
    // table silently dropped instead of refused by name. These reach the
    // nested tables inside each file.
    // **And the nested tables inside each file.** Round 58 added a row per
    // registry and reached eight of the twenty structs that carry the
    // attribute; a reviewer swept all twenty on 2026-09-14 and twelve
    // survived — including the one `check.rs` delegates a rule to by name.
    //
    // **Three of the seven below reach a struct an earlier row already had**,
    // and the sentence here used to claim each reached a different one: the
    // event-field row lands where `optional-typo` does, the hashing row where
    // `memory-typo` does, and the rate-limit row where the two budget rows do.
    // They are kept because a second field of one struct is a second thing that
    // can be typed wrongly, and the three structs they were *meant* for have
    // rows of their own at the end of this list. Reported on 2026-09-14.
    //
    // **And one of them was answered by the wrong diagnostic.** `typ` is
    // `type` with a character taken off, so with the attribute deleted serde
    // says `missing field \`type\`` — which holds the string `typ` and
    // satisfied the assertion that the refusal names it. The typo is longer
    // than the field now, which no missing-field message can contain.
    (
        "locale-meta-typo",
        "catalog/i18n/en.toml",
        "bcp47 = \"en\"",
        "bcp47z = \"en\"",
        "bcp47z",
    ),
    (
        "event-field-typo",
        "catalog/log-events.toml",
        "{ name = \"waited\", type = \"duration\" }",
        "{ name = \"waited\", typz = \"duration\" }",
        "typz",
    ),
    (
        "hashing-typo",
        "catalog/limits.toml",
        "iterations = 2",
        "iterationss = 2",
        "iterationss",
    ),
    (
        "rate-limit-typo",
        "catalog/limits.toml",
        "window_seconds = ",
        "window_secondss = ",
        "window_secondss",
    ),
    (
        "store-limit-typo",
        "catalog/limits.toml",
        "max_disk_bytes = 8_589_934_592",
        "max_disk_bitez = 8_589_934_592",
        "max_disk_bitez",
    ),
    (
        "table-limit-typo",
        "catalog/limits.toml",
        "max_rows = 500_000",
        "max_rowss = 500_000",
        "max_rowss",
    ),
    (
        "notification-typo",
        "catalog/notifications.toml",
        "title = \"notification.catalog.mechanism.title\"",
        "titel = \"notification.catalog.mechanism.title\"",
        "titel",
    ),
    // **And the three the sweep found still standing.** Round 60 deleted each of
    // the twenty attributes in turn and ran this suite: twelve survived. Seven
    // of those are file-level structs, which the test below reaches; these are
    // the three inner ones no row named — the two rate-limit budgets' container,
    // an event's own fields as against a field's, and the pair of Argon2id
    // configurations as against one configuration's parameters.
    (
        "rate-limit-container-typo",
        "catalog/limits.toml",
        "surface = \"activation_code\"",
        "surface = \"activation_code\"\nsurfase = \"extra\"",
        "surfase",
    ),
    (
        "event-entry-typo",
        "catalog/log-events.toml",
        "name = \"journal.appended\"",
        "name = \"journal.appended\"\ndescriptionz = \"extra\"",
        "descriptionz",
    ),
    (
        "hashing-pair-typo",
        "catalog/limits.toml",
        "[argon2id.device_pin]",
        "[argon2id.device_pinz]",
        "device_pinz",
    ),
    // The kernel's limits and the two shapes of `catalog/capabilities.toml`,
    // each a struct of its own with the attribute on it.
    (
        "kernel-typo",
        "catalog/limits.toml",
        "undo_history = 100",
        "undo_historyy = 100",
        "undo_historyy",
    ),
    (
        "capability-typo",
        "catalog/capabilities.toml",
        "description = \"capability.store.description\"",
        "descriptionn = \"capability.store.description\"",
        "descriptionn",
    ),
    (
        "ungated-typo",
        "catalog/capabilities.toml",
        "interface = \"types\"",
        "interface = \"types\"\nreasonz = \"extra\"",
        "reasonz",
    ),
];

/// The registry files, each of whose file-level struct no typo row reached.
///
/// **A rename is two refusals and the weaker one answers first.** Every row
/// above renames a key, so without `deny_unknown_fields` serde reports the
/// *missing* field instead — and where the typo is the real name with a
/// character taken off, the diagnostic naming the *missing* `max_disk_bytes`
/// holds the string `max_disk_byte` and satisfied the assertion that the
/// refusal names it. Two
/// rows were passing that way, measured on 2026-09-14 by deleting all twenty
/// attributes one at a time.
///
/// A key *added* cannot be answered by a missing-field message, because nothing
/// is missing. That is what this list is: one unknown key at the top of each
/// registry, where the top is the file-level struct — the seven that survived
/// the same sweep, among them the one `check.rs` delegates a rule to by name.
const AN_UNKNOWN_KEY_AT_THE_TOP: [(&str, &str); 8] = [
    ("errors", "catalog/errors.toml"),
    ("notifications", "catalog/notifications.toml"),
    ("emails", "catalog/emails.toml"),
    ("commands", "catalog/commands.toml"),
    ("log-events", "catalog/log-events.toml"),
    ("limits", "catalog/limits.toml"),
    ("locale", "catalog/i18n/en.toml"),
    ("capabilities", "catalog/capabilities.toml"),
];

#[test]
fn a_typo_in_a_field_name_fails_the_build_rather_than_disabling_a_control() {
    // Every one of these used to compile clean. `on_lockut` left the device PIN
    // never wiping its credential, which §10.6 requires.
    for (name, file, from, to, naming) in A_TYPO_REFUSED {
        let scratch = Scratch::new(name);
        scratch.edit(file, |text| text.replace(from, to));
        // The offending key by name, so the test cannot be satisfied by any
        // other malformed-registry diagnostic.
        scratch.must_fail("registry-malformed", naming);
    }
}

/// A key no struct declares at the top of a registry fails the build.
///
/// The file-level struct of each registry — `ErrorFile`, `Limits`, `LocaleFile`
/// and the rest — carries `deny_unknown_fields` and was decided by nothing: a
/// sweep that deleted all twenty of those attributes one at a time left the
/// whole suite green for seven of them. A key at the top of the file is what
/// reaches exactly that struct, and adding one rather than renaming one is what
/// keeps a missing-field message from answering in its place.
#[test]
fn an_unknown_key_at_the_top_of_a_registry_fails_the_build() {
    for (label, file) in AN_UNKNOWN_KEY_AT_THE_TOP {
        let scratch = Scratch::new(&format!("top-{label}"));
        scratch.edit(file, |text| format!("nc_not_a_field = \"x\"\n{text}"));
        scratch.must_fail("registry-malformed", "nc_not_a_field");
    }
}

#[test]
fn two_defects_of_different_kinds_are_both_reported() {
    // **`compile` returned on `load`'s problems**, so a catalog with an i18n
    // defect *and* a renumbered code reported only the first kind — while the
    // function's own doc promised "every problem found … rather than the first".
    // A developer fixed one, ran the build again, and met the other; the whole
    // reason `check` accumulates is that being shown one problem at a time is a
    // miserable loop.
    //
    // The two here are found by different passes: `unreferenced-key` comes out
    // of `load`, and `code-repointed` out of the ledger reconcile that used to
    // sit behind an early return.
    let scratch = Scratch::new("two-kinds");
    scratch.edit("catalog/i18n/en.toml", |text| {
        format!("{text}\n\"error.db.ghost\" = \"A ghost sentence nothing points at.\"\n")
    });
    scratch.edit("catalog/i18n/pt-BR.toml", |text| {
        format!("{text}\n\"error.db.ghost\" = \"Uma frase fantasma.\"\n")
    });
    scratch.edit("catalog/errors.toml", |text| {
        text.replace(
            "description = \"The database did not answer\"",
            "description = \"The printer tray is empty\"",
        )
    });

    let problems = scratch.problems();
    let kinds: Vec<&str> = problems.iter().map(|problem| problem.check).collect();
    assert!(
        kinds.contains(&"unreferenced-key"),
        "the load-time defect is missing: {kinds:?}"
    );
    assert!(
        kinds.contains(&"code-repointed"),
        "the ledger defect was never computed, so only the first pass reported: {kinds:?}"
    );
}

#[test]
fn a_sentence_no_registry_points_at_is_refused() {
    // **The direction nothing asked.** `messages.rs` checked source→locale,
    // locale→source, placeholders, and registry→locale — every direction but
    // this one. A key present in both locales that no entry references
    // compiled, landed in `KEYS` and `MESSAGES`, and appeared on no page:
    //
    // ```text
    // echo '"error.db.ghost" = "A ghost sentence nothing points at."' >> en.toml
    // echo '"error.db.ghost" = "Uma frase fantasma."'                 >> pt-BR.toml
    // cargo xtask build  ->  ok  15 generated file(s), 1 rewritten
    // ```
    //
    // Which is a sentence translated into every language and shown to nobody,
    // maintained forever by people who cannot tell it is dead — the argument
    // `orphan-key` is built on, one file over.
    let scratch = Scratch::new("ghost-key");
    scratch.edit("catalog/i18n/en.toml", |text| {
        format!("{text}\n\"error.db.ghost\" = \"A ghost sentence nothing points at.\"\n")
    });
    scratch.edit("catalog/i18n/pt-BR.toml", |text| {
        format!("{text}\n\"error.db.ghost\" = \"Uma frase fantasma.\"\n")
    });
    scratch.must_fail("unreferenced-key", "error.db.ghost");
}

#[test]
fn a_message_key_may_not_be_owned_by_two_registries() {
    // **The check that stopped at the registry boundary.** `check_errors`
    // refuses two error codes sharing one message key, because the key is a
    // code's identity in the ledger — and nothing said the same across
    // registries. Pointing a notification's title at an error's key compiled:
    //
    // ```text
    // title = "error.db.unreachable"
    // cargo xtask build  ->  ok  15 generated file(s), 2 rewritten
    // ```
    //
    // The notification then owns the sentence `NC-DB-0001` resolves to. Editing
    // it "for the notification" changes what the error says, and the ledger sees
    // nothing, because the code's number, key and description never moved.
    let scratch = Scratch::new("shared-key");
    scratch.edit("catalog/notifications.toml", |text| {
        let at = text
            .find("title = \"")
            .expect("the notification has a title");
        let ends = text[at + 9..].find('"').expect("it is a string") + at + 9;
        format!("{}error.db.unreachable{}", &text[..at + 9], &text[ends..])
    });
    scratch.must_fail("key-owned-twice", "error.db.unreachable");
}

#[test]
fn one_entry_may_not_point_two_slots_at_one_key() {
    // **The dedup was on the owner's name.** A notification whose `title` and
    // `body` are the same key had one owner and never tripped the gate — while
    // its own reasoning applies unchanged: rewording that sentence for the
    // title silently rewords the body, and neither slot can be edited for its
    // own purpose. It surfaced only by accident, as the abandoned key becoming
    // an orphan, and a *new* entry written that way trips nothing at all.
    let scratch = Scratch::new("one-entry-two-slots");
    scratch.edit("catalog/notifications.toml", |text| {
        let at = text.find("body = \"").expect("the notification has a body");
        let ends = text[at + 8..].find('"').expect("it is a string") + at + 8;
        let title = {
            let opens = text.find("title = \"").expect("and a title") + 9;
            let closes = text[opens..].find('"').expect("a string") + opens;
            text[opens..closes].to_owned()
        };
        format!("{}{title}{}", &text[..at + 8], &text[ends..])
    });
    scratch.must_fail("key-owned-twice", "twice");
}

// --- limits ---

#[test]
fn a_table_bounded_by_the_pack_may_not_also_name_a_row_limit() {
    // The two together are how a global constant one day prunes below what a
    // policy pack promised (`NOCTUA.md §15.8`).
    //
    // **One at a time, because both at once cannot tell the halves apart.**
    // The fixture set `max_rows` and `prune_batch` together, so either disjunct
    // deleted left the other firing and the whole suite green — and
    // `prune_batch` has no second net anywhere in the tree, so that half was
    // held by nothing at all.
    for (which, added) in [
        (
            "max_rows",
            "strategy = \"policy-pack\"\nmax_rows = 100\norder_by = \"sequence\"",
        ),
        (
            "prune_batch",
            "strategy = \"policy-pack\"\nprune_batch = 10\norder_by = \"sequence\"",
        ),
    ] {
        let scratch = Scratch::new(&format!("limits-{which}"));
        scratch.edit("catalog/limits.toml", |text| {
            text.replace("strategy = \"policy-pack\"\norder_by = \"sequence\"", added)
        });
        scratch.must_fail("pack-and-rows", "journal_entries");
    }
}

#[test]
fn a_table_exempt_from_the_row_limit_has_to_say_why() {
    // **The escape hatch that had no reason, no census entry and no test.**
    // `strategy = "policy-pack"` satisfies the unbounded-table gate without
    // declaring a bound and the pruner then never touches the table — so an
    // unbounded, never-pruned table entered the tree with three lines of TOML
    // and nothing written anywhere a parser reads:
    //
    // ```toml
    // [tables.login_attempts]
    // strategy = "policy-pack"
    // order_by = "id"
    // ```
    //
    // `2 table(s) created in the migrations were checked`, `check passed`. Its
    // sibling `nc:bounded:` has required a non-empty reason from the first day,
    // is tested for the bare-marker case, and is counted in the census line.
    let scratch = Scratch::new("pack-no-reason");
    scratch.edit("catalog/limits.toml", |text| {
        let at = text
            .find("pack_reason = \"\"\"")
            .expect("the journal gives one");
        let ends = text[at..].find("\"\"\"\n").expect("it is a block string");
        let closes = text[at + ends + 3..].find("\"\"\"").expect("and it closes") + at + ends + 6;
        format!("{}{}", &text[..at], &text[closes..])
    });
    scratch.must_fail("pack-without-a-reason", "journal_entries");
}

#[test]
fn a_blank_reason_is_not_a_reason() {
    // **The `trim().is_empty()` half was tested by nothing.** The test above
    // deletes the whole `pack_reason` block, so only the `None` arm ran, and
    // weakening the predicate to a bare `is_none()` left 100 tests and
    // `check passed` green — with `pack_reason = ""` and `pack_reason = "   "`
    // both buying the exemption from `§15.8`'s row-limit gate.
    //
    // Its sibling `nc:bounded:` has had the bare-marker test from the first
    // day. That is the one of its three properties this fix did not copy.
    for blank in ["\"\"", "\"   \"", "\"\\n\""] {
        let scratch = Scratch::new("pack-blank-reason");
        scratch.edit("catalog/limits.toml", |text| {
            let at = text
                .find("pack_reason = \"\"\"")
                .expect("the journal gives one");
            let ends = text[at..].find("\"\"\"\n").expect("it is a block string");
            let closes =
                text[at + ends + 3..].find("\"\"\"").expect("and it closes") + at + ends + 6;
            format!("{}pack_reason = {blank}{}", &text[..at], &text[closes..])
        });
        scratch.must_fail("pack-without-a-reason", "journal_entries");
    }
}

#[test]
fn a_reason_on_a_table_the_pruner_does_bound_is_refused() {
    // The other direction. A reason explains an exemption, and a table naming
    // `max_rows` and `prune_batch` is not exempt — leaving one there reads as a
    // bound nobody applies, which is the confusion the field exists to end.
    let scratch = Scratch::new("reason-no-pack");
    scratch.edit("catalog/limits.toml", |text| {
        text.replace(
            "prune_batch = 50_000\n",
            "prune_batch = 50_000\npack_reason = \"a pack will bound it one day\"\n",
        )
    });
    scratch.must_fail("reason-without-a-pack", "access_records");
}

#[test]
fn a_row_bounded_table_needs_both_numbers() {
    let scratch = Scratch::new("half-limit");
    scratch.edit("catalog/limits.toml", |text| {
        text.replace("prune_batch = 50_000\n", "")
    });
    scratch.must_fail("incomplete-row-limit", "access_records");
}

#[test]
fn a_batch_larger_than_the_limit_fails_the_build() {
    let scratch = Scratch::new("big-batch");
    scratch.edit("catalog/limits.toml", |text| {
        text.replace("prune_batch = 50_000", "prune_batch = 900_000")
    });
    scratch.must_fail("batch-past-limit", "access_records");
}

/// `§15.8` says prune in *large* batches, and the batch has a floor.
///
/// **The sentence was `[CLOSED]` and enforced by nothing.** The two rules beside
/// this one bound the batch at zero and from above; `prune_batch = 1` against
/// `max_rows = 500_000` was a legal catalog that passed the whole gate, and on a
/// table receiving writes that never converges while every tick reports `Ok`.
/// The four things that objected when it was measured were all `assert_ne!`
/// drift guards whose needle is the literal `prune_batch = 50_000`, and whose
/// own message tells a reader to update the fixture — which is the shape this
/// tree already caught once for the journal, one field along.
#[test]
fn a_batch_that_is_row_by_row_at_this_scale_fails_the_build() {
    for batch in ["1", "4_999"] {
        let scratch = Scratch::new(&format!("small-batch-{batch}"));
        scratch.edit("catalog/limits.toml", |text| {
            text.replace("prune_batch = 50_000", &format!("prune_batch = {batch}"))
        });
        scratch.must_fail("batch-below-a-hundredth", "access_records");
    }
    // And the boundary from the other side, because a rule that refuses a
    // legitimate batch is the half that is harder to notice: a hundredth of the
    // limit exactly is accepted, and what this repository ships is a tenth.
    let scratch = Scratch::new("batch-at-a-hundredth");
    scratch.edit("catalog/limits.toml", |text| {
        text.replace("prune_batch = 50_000", "prune_batch = 5_000")
    });
    assert!(
        scratch
            .problems()
            .iter()
            .all(|problem| problem.check != "batch-below-a-hundredth"),
        "a batch of exactly a hundredth of the limit was refused, so the floor is one \
         step higher than the paragraph that argues for it"
    );
}

#[test]
fn a_batch_of_zero_fails_the_build() {
    let scratch = Scratch::new("zero-batch");
    scratch.edit("catalog/limits.toml", |text| {
        text.replace("prune_batch = 50_000", "prune_batch = 0")
    });
    scratch.must_fail("zero-batch", "access_records");
}

/// A table that refuses at its limit carries a limit, and nothing that says
/// pruning.
///
/// **`refuse-at-limit` is the third strategy, and the second the pruner leaves
/// alone.** User state — what a plugin keeps — is bounded by its writer
/// refusing a new row, so an entry with no number is a table nothing bounds, a
/// number of zero is a table that can hold nothing, and a batch or a pack
/// reason beside it is a sentence about a pruning that never happens.
#[test]
fn a_table_that_refuses_at_its_limit_names_a_limit_and_nothing_else() {
    // The shipped entry's last two lines, which each case below breaks.
    let shipped = "order_by = \"rev_unit\"\nmax_rows = 100_000\n";
    for (check, broken) in [
        (
            "refuse-without-a-limit",
            "order_by = \"rev_unit\"\n".to_owned(),
        ),
        (
            "refuse-at-zero",
            "order_by = \"rev_unit\"\nmax_rows = 0\n".to_owned(),
        ),
        (
            "refuse-and-batch",
            format!("{shipped}prune_batch = 10_000\n"),
        ),
        (
            "reason-without-a-pack",
            format!("{shipped}pack_reason = \"store_entries is user state and is refused\"\n"),
        ),
    ] {
        let scratch = Scratch::new(&format!("refusing-{check}"));
        scratch.edit("catalog/limits.toml", |text| text.replace(shipped, &broken));
        scratch.must_fail(check, "store_entries");
    }
}

/// A limit counted per owner belongs to a table whose writer refuses at it,
/// and the owner is one this catalog knows.
///
/// **The pruner counts a whole table**, so a `per` beside `oldest-first` is a
/// bound applied to more rows than the entry says, and a `policy-pack` table
/// has no number to count at all.
#[test]
fn a_limit_counted_per_owner_belongs_to_a_refusing_writer() {
    for (table, after) in [
        ("access_records", "order_by = \"recorded\"\n"),
        ("journal_entries", "order_by = \"sequence\"\n"),
    ] {
        let scratch = Scratch::new(&format!("per-owner-{table}"));
        scratch.edit("catalog/limits.toml", |text| {
            assert_eq!(text.matches(after).count(), 1, "{after:?} drifted");
            text.replace(after, &format!("{after}per = \"plugin\"\n"))
        });
        scratch.must_fail("per-owner-without-a-refusal", table);
    }
    let scratch = Scratch::new("per-owner-unknown");
    scratch.edit("catalog/limits.toml", |text| {
        text.replacen("\nper = \"plugin\"\n", "\nper = \"realm\"\n", 1)
    });
    scratch.must_fail_saying("registry-malformed", "`realm`, expected `plugin` or `unit`");
}

/// A table's database is one of the two there are, and says which by name.
#[test]
fn a_table_in_a_database_that_does_not_exist_fails_the_build() {
    let scratch = Scratch::new("no-such-database");
    scratch.edit("catalog/limits.toml", |text| {
        // At the start of a line: the file's header mentions the key in a comment.
        text.replacen(
            "\ndatabase = \"local\"\n",
            "\ndatabase = \"elsewhere\"\n",
            1,
        )
    });
    scratch.must_fail_saying(
        "registry-malformed",
        "`elsewhere`, expected `server` or `local`",
    );
}

#[test]
fn a_table_with_no_ordering_column_fails_the_build() {
    let scratch = Scratch::new("no-order");
    scratch.edit("catalog/limits.toml", |text| {
        text.replace("order_by = \"sequence\"", "order_by = \"\"")
    });
    scratch.must_fail("no-order-column", "journal_entries");
}

#[test]
fn a_catalog_with_nothing_the_pruner_can_prune_fails_the_build() {
    // Every table on `policy-pack` is one the pruner must not touch, so a
    // registry of nothing but those leaves it with no data and its tests proving
    // nothing — green, and empty.
    let scratch = Scratch::new("nothing-prunable");
    scratch.edit("catalog/limits.toml", |text| {
        text.replace("strategy = \"oldest-first\"", "strategy = \"policy-pack\"")
            .replace("max_rows = 500_000\nprune_batch = 50_000\n", "")
    });
    scratch.must_fail("nothing-prunable", "catalog/limits.toml");
}

/// A kernel limit of zero fails the build, whichever width its field has.
///
/// Two rows, a `u64` and a `u32`, because `KernelLimits::each` widens the
/// narrow ones on the way to the check, and a row that dropped them would leave
/// every count bound unchecked while the byte bounds stayed refused.
#[test]
fn a_kernel_limit_of_zero_fails_the_build() {
    for (from, to, naming) in [
        (
            "memory_bytes = 67_108_864",
            "memory_bytes = 0",
            "`[kernel] memory_bytes` is zero",
        ),
        (
            "undo_history = 100",
            "undo_history = 0",
            "`[kernel] undo_history` is zero",
        ),
    ] {
        let scratch = Scratch::new(&format!("kernel-zero-{}", to.replace(' ', "")));
        scratch.replacing("catalog/limits.toml", from, to);
        scratch.must_fail_saying("kernel-limit-zero", naming);
    }
}

/// Every relation between two kernel limits is refused at the number where it
/// stops holding, and not one past it.
///
/// **At the boundary, because the boundary is the rule.** A call tree's
/// deadline equal to a host call's is a host-call deadline that bounds nothing,
/// and an epoch deadline equal to the tree's is one the tree reaches at the
/// same instant; a payload equal to a store value is the one that still fits.
/// So each row sets one number to exactly the value where the relation first
/// fails, which is what tells `>` from `>=` and `<` from `<=` — the shipped
/// catalog, which compiles, is the other side of each.
#[test]
fn a_kernel_limit_that_outgrows_the_one_bounding_it_fails_the_build() {
    for (check, from, to, naming) in [
        (
            "dispatch-deadline-within-a-host-call",
            "dispatch_deadline_ms = 30_000",
            "dispatch_deadline_ms = 20_000",
            "`[kernel] dispatch_deadline_ms` is 20000 and `host_call_deadline_ms` is 20000",
        ),
        (
            "host-call-deadline-within-a-request",
            "net_timeout_ms = 15_000",
            "net_timeout_ms = 20_000",
            "`[kernel] host_call_deadline_ms` is 20000 and `net_timeout_ms` is 20000",
        ),
        (
            "epoch-deadline-past-the-dispatch-deadline",
            "epoch_deadline_ticks = 500",
            "epoch_deadline_ticks = 3_000",
            "10 × 3000, which does not come before `dispatch_deadline_ms`, 30000",
        ),
        (
            "payload-larger-than-a-store-value",
            "payload_bytes = 1_048_576",
            "payload_bytes = 1_048_577",
            "`[kernel] payload_bytes` is 1048577 and `store_value_bytes` is 1048576",
        ),
        (
            "unit-smaller-than-a-store-write",
            "unit_bytes = 16_777_216",
            "unit_bytes = 2_097_407",
            "`[kernel] unit_bytes` is 2097407 and twice `store_value_bytes` + \
             `store_key_bytes` is 2 × 1048576 + 256",
        ),
        (
            "event-queue-smaller-than-an-event",
            "event_queue_bytes = 16_777_216",
            "event_queue_bytes = 1_048_671",
            "`[kernel] event_queue_bytes` is 1048671 and `payload_bytes` + the longest topic \
             is 1048576 + 96",
        ),
    ] {
        let scratch = Scratch::new(check);
        scratch.replacing("catalog/limits.toml", from, to);
        scratch.must_fail_saying(check, naming);
        // **And nothing else.** Each row moves one number, so one relation is
        // the only thing it can break; a second check firing would be a rule
        // reading the wrong field.
        let fired: Vec<&str> = scratch
            .problems()
            .iter()
            .map(|problem| problem.check)
            .collect();
        assert_eq!(
            fired,
            [check],
            "{to} broke more than the one relation it is about"
        );
    }
}

/// An epoch deadline so long its product does not fit is refused, not wrapped.
///
/// `epoch_tick_ms × epoch_deadline_ticks` is multiplied checked: a product that
/// overflowed and wrapped would be a small number, and a small number passes
/// the relation it was meant to fail.
#[test]
fn an_epoch_deadline_too_long_to_multiply_fails_the_build() {
    let scratch = Scratch::new("epoch-overflow");
    scratch.replacing(
        "catalog/limits.toml",
        "epoch_deadline_ticks = 500",
        "epoch_deadline_ticks = 1_844_674_407_370_955_162",
    );
    scratch.must_fail(
        "epoch-deadline-past-the-dispatch-deadline",
        "epoch_deadline_ticks",
    );
}

/// A capability whose interface, name or functions the world could not hold
/// fails the build, each naming the capability.
#[test]
fn a_capability_the_world_could_not_hold_fails_the_build() {
    for (check, from, to, naming) in [
        (
            "interface-malformed",
            "interface = \"store\"\nfunctions",
            "interface = \"Store\"\nfunctions",
            "`store` names the interface `Store`",
        ),
        (
            "capability-outside-its-interface",
            "name = \"store\"",
            "name = \"storage\"",
            "`storage` does not name its interface `store`",
        ),
        (
            "capability-outside-its-interface",
            "name = \"blobs.read\"",
            "name = \"blobs.read.again\"",
            "`blobs.read.again` does not name its interface `blobs`",
        ),
        (
            "function-malformed",
            "\"[method]reader.size\"",
            "\"[method]reader\"",
            "`blobs.read` grants `[method]reader`",
        ),
        (
            "function-malformed",
            "functions = [\"get\", \"put\"",
            "functions = [\"Get\", \"put\"",
            "`store` grants `Get`",
        ),
    ] {
        let scratch = Scratch::new(&format!("capability-{check}-{}", to.len()));
        scratch.replacing("catalog/capabilities.toml", from, to);
        scratch.must_fail_saying(check, naming);
    }
}

/// A function granted by two capabilities fails the build, naming both.
///
/// One function, one grant: if `open` were granted by `blobs.write` as well
/// as by `blobs.read`, a plugin denied reading could open a blob by asking to
/// write.
#[test]
fn a_function_granted_twice_fails_the_build() {
    let scratch = Scratch::new("function-granted-twice");
    scratch.replacing(
        "catalog/capabilities.toml",
        "functions = [\"create\", ",
        "functions = [\"open\", \"create\", ",
    );
    scratch.must_fail_saying(
        "function-granted-twice",
        "`blobs#open` is granted by `blobs.read` and by `blobs.write`",
    );
}

/// An interface given to every plugin is one, given once, not a capability's,
/// and says why — each refusal naming the interface.
#[test]
fn an_interface_given_to_every_plugin_has_to_be_one_and_say_why() {
    for (check, addition, naming) in [
        (
            "duplicate-ungated",
            "\n[[ungated]]\ninterface = \"log\"\nfunctions = []\nreason = \"A second reason for the same interface.\"\n",
            "`log` is given to every plugin twice",
        ),
        (
            "interface-malformed",
            "\n[[ungated]]\ninterface = \"Clock\"\nfunctions = []\nreason = \"A clock every plugin may read.\"\n",
            "`Clock` is given to every plugin, and the world cannot spell it",
        ),
        (
            "interface-gated-and-ungated",
            "\n[[ungated]]\ninterface = \"store\"\nfunctions = []\nreason = \"Every plugin keeps a little.\"\n",
            "`store` is given to every plugin and is also the interface of `store`",
        ),
        (
            "ungated-without-a-reason",
            "\n[[ungated]]\ninterface = \"clock\"\nfunctions = []\nreason = \"\u{2063}\"\n",
            "`clock` is given to every plugin and says no why",
        ),
    ] {
        let scratch = Scratch::new(&format!("ungated-{check}"));
        scratch.edit("catalog/capabilities.toml", |mut text| {
            text.push_str(addition);
            text
        });
        scratch.must_fail_saying(check, naming);
    }
}

/// The functions an interface gives every plugin are held as a capability's
/// are: each one the world could name, and each written in one place only.
///
/// **The list is what the reason covers**, so the kernel's coverage test
/// reads the gift from it rather than from the world: one written twice, or
/// granted by a capability as well, is one whose place a reader cannot tell.
#[test]
fn a_function_given_to_every_plugin_is_one_the_world_names_once() {
    for (check, from, to, naming) in [
        (
            "function-malformed",
            "functions = [\"write\"]",
            "functions = [\"Write\"]",
            "`log` gives every plugin `Write`, which the world cannot name",
        ),
        (
            "function-malformed",
            "functions = [\"write\"]",
            "functions = [\"write\", \"[method]ring\"]",
            "`log` gives every plugin `[method]ring`, which the world cannot name",
        ),
        (
            "function-granted-twice",
            "functions = [\"write\"]",
            "functions = [\"write\", \"write\"]",
            "`log#write` is given to every plugin twice",
        ),
        (
            "function-granted-twice",
            "interface = \"types\"\nfunctions = []",
            "interface = \"store\"\nfunctions = [\"get\"]",
            "`store#get` is granted by `store` and given to every plugin",
        ),
    ] {
        let scratch = Scratch::new(&format!("ungated-{check}-{}", to.len()));
        scratch.replacing("catalog/capabilities.toml", from, to);
        scratch.must_fail_saying(check, naming);
    }
}

/// A capability's sentence is checked like every other.
///
/// It has to exist, and it may not interpolate anything, because a manifest's
/// grants are shown where no caller supplies a value.
#[test]
fn a_capability_sentence_is_held_to_the_locale_rules() {
    let dangling = Scratch::new("capability-dangling");
    dangling.replacing(
        "catalog/capabilities.toml",
        "description = \"capability.net.description\"",
        "description = \"capability.net.nothing-wrote-this\"",
    );
    dangling.must_fail("dangling-message-key", "capability.net.nothing-wrote-this");

    let interpolating = Scratch::new("capability-params");
    for locale in ["en", "pt-BR"] {
        interpolating.edit(&format!("catalog/i18n/{locale}.toml"), |text| {
            text.replace(
                "\"capability.net.description\" = \"",
                "\"capability.net.description\" = \"{who} ",
            )
        });
    }
    interpolating.must_fail("param-not-declared", "capability.net.description");
}

#[test]
fn hashing_parameters_below_the_floor_fail_the_build() {
    // **Ten cells, and this exercised one.** `check_hashing` is a two-row table
    // — `password` and `device_pin` — crossed with five parameters, and the one
    // case written here set the *password*'s `memory_kib` to 64. Two holes came
    // with that. The device PIN's row was driven by nothing at all, and the
    // value 64 is so far under every floor that it also proves nothing about
    // where the floor is: dropping the PIN's floor from `65_536` to the
    // password's `19_456`, with `catalog/limits.toml`'s own `memory_kib` moved
    // to `20_480` beside it, left `cargo xtask build` green, the generations
    // byte-identical and the ledger's md5 unchanged — while the derivation
    // guarding an enumerable four-to-eight-digit secret fell from 64 MiB to 20
    // MiB per guess, against a registry comment that says per-guess cost is the
    // only defence that scales.
    //
    // So each cell is broken at exactly `floor - 1`. A value far below the
    // floor proves only that *some* floor exists; one step below it is what
    // pins *this* floor, because lowering the floor by any amount lets the
    // fixture through and this test goes red.
    //
    // The floors are written out rather than read from the compiler, for the
    // reason `every_field_of_a_record_has_an_edit` gives about `FIELDS`: two
    // hand lists disagree loudly, and a list derived from the thing it checks
    // agrees with any weakening by construction.
    const FLOORS: [(&str, &str, u32); 10] = [
        ("password", "memory_kib", 19_456),
        ("password", "iterations", 1),
        ("password", "parallelism", 1),
        ("password", "output_length", 16),
        ("password", "salt_length", 16),
        // Deliberately harder than the password's, and the row nothing reached.
        ("device_pin", "memory_kib", 65_536),
        ("device_pin", "iterations", 1),
        ("device_pin", "parallelism", 1),
        ("device_pin", "output_length", 16),
        ("device_pin", "salt_length", 16),
    ];

    for (which, key, floor) in FLOORS {
        let scratch = Scratch::new(&format!("argon-{which}-{key}"));
        scratch.edit("catalog/limits.toml", |text| {
            just_under(&text, which, key, floor - 1)
        });
        // Both halves of the cell are named: the row and the parameter. With
        // one parameter broken per scratch, this is the only `hashing-below-floor`
        // that can fire, so the message identifies the cell rather than the
        // table.
        scratch.must_fail("hashing-below-floor", &format!("argon2id.{which} {key}"));
    }
}

/// Sets one key of one `[argon2id.*]` row, leaving the other row alone.
///
/// A plain `replace` cannot address a cell here: both rows declare all five
/// keys with the same names, so `memory_kib` matches twice and `salt_length =
/// 16` is the same text in each. Section-aware, and it refuses an edit that
/// found no such key — the floor `Scratch::edit` puts under a needle that has
/// drifted only fires when the text changed, and a key renamed in the registry
/// would otherwise make this a rewrite of nothing.
fn just_under(text: &str, which: &str, key: &str, value: u32) -> String {
    let header = format!("[argon2id.{which}]");
    let mut inside = false;
    let mut written = 0_usize;
    let mut out = String::new();
    for line in text.lines() {
        if line.trim_start().starts_with('[') {
            inside = line.trim() == header;
        }
        if inside && line.trim_start().starts_with(&format!("{key} = ")) {
            out.push_str(key);
            out.push_str(" = ");
            out.push_str(&value.to_string());
            out.push('\n');
            written += 1;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    assert_eq!(
        written, 1,
        "catalog/limits.toml holds {written} `{key}` line(s) under `{header}` and this \
         fixture needs exactly one: the shipped registry is what changed"
    );
    out
}

#[test]
fn a_delay_curve_that_falls_fails_the_build() {
    // §15.5 requires an *escalating* delay. The gate used to check only that the
    // curve was non-empty, while its own message said "escalating".
    //
    // **This was called `a_flat_delay_curve_fails_the_build` and plants a
    // *descending* one.** A flat curve is the case the rule beside it was
    // accepting, and the fixture named for it drove the other rule — so the one
    // shape §15.5's sentence is about was exercised by nothing at all. The name
    // is what it plants now, and the flat case is the fixture below.
    let scratch = Scratch::new("falling-curve");
    scratch.edit("catalog/limits.toml", |text| {
        text.replace(
            "delay_ms = [0, 1_000, 5_000, 30_000, 120_000]",
            "delay_ms = [500, 400, 300, 200]",
        )
    });
    scratch.must_fail("curve-descends", "device_pin");
}

#[test]
fn a_delay_curve_that_never_rises_fails_the_build() {
    // The case `curve-descends` accepts and §15.5 asks about. Measured on
    // 2026-09-08 before this rule existed: `email_confirmation`'s per-identifier
    // curve as `[1_000, 1_000, 1_000, 1_000, 1_000]` gave
    // `ok 15 generated file(s), 2 rewritten` and `check passed` with no warn
    // line, and `docs/limits.md` republished it four lines under its own
    // sentence saying the curve escalates.
    let scratch = Scratch::new("flat-curve");
    scratch.edit("catalog/limits.toml", |text| {
        text.replace(
            "delay_ms = [0, 1_000, 5_000, 30_000, 120_000]",
            "delay_ms = [1_000, 1_000, 1_000, 1_000]",
        )
    });
    scratch.must_fail("curve-is-flat", "device_pin");

    // And a curve of one value, whose one value repeats for ever.
    let scratch = Scratch::new("one-value-curve");
    scratch.edit("catalog/limits.toml", |text| {
        text.replace(
            "delay_ms = [0, 1_000, 5_000, 30_000, 120_000]",
            "delay_ms = [1_000]",
        )
    });
    scratch.must_fail("curve-is-flat", "device_pin");
}

#[test]
fn a_lockout_that_can_never_be_reached_fails_the_build() {
    let scratch = Scratch::new("unreachable-lockout");
    scratch.edit("catalog/limits.toml", |text| {
        text.replace(
            "lockout_after = 8, cumulative = true, ",
            "cumulative = true, ",
        )
    });
    scratch.must_fail("lockout-unreachable", "device_pin");
}

/// A name the generated file already imports is refused before it is written.
///
/// **`cargo xtask build` said `ok` and wrote source that does not compile.** The
/// name check held each event against the *other events*, and
/// `crates/nc-log/src/events.rs` opens with imports of its own — so an event
/// called `field` generated `pub struct Field` beside `use crate::{Event, Field,
/// LogValue as _};`, the build reported `15 generated file(s), 3 rewritten`, and
/// the next `cargo build` failed inside a file whose header forbids editing it:
///
/// ```text
/// error[E0255]: the name `Field` is defined multiple times
///   --> crates/nc-log/src/events.rs:44:1
/// ```
///
/// All six are checked, because the list is written down and a list written down
/// is a list that can lose an entry. `Vec` and `Option` are the ones that do not
/// collide but *shadow*, which surfaces as an `E0107` on the `fields` method
/// written for every event — further still from the three lines that caused it.
#[test]
fn an_event_named_after_an_import_fails_the_build() {
    // `option` and `vec` are refused one rule earlier, by `identifier`, which
    // knows the prelude — `a_field_named_after_a_keyword_fails_the_build` covers
    // those. These three are names the *file* holds and Rust does not.
    //
    // **`log_value` was a fourth and it is not a collision.** The preamble
    // imports `LogValue as _`, which brings the trait in for method resolution
    // and binds no name, so a generated `pub struct LogValue` compiles —
    // measured on 2026-09-08 by appending exactly that to
    // `crates/nc-log/src/events.rs`: `cargo build -p nc-log` finished and
    // clippy said nothing, while `pub struct Field` on the same line gives
    // `error[E0255]: the name `Field` is defined multiple times`. This loop
    // drove the *rule* — that the compiler refuses these four — which is a
    // restatement of the constant rather than a claim about what rustc needs.
    // The hole is held by `every_import_here_is_a_name_the_preamble_binds`,
    // which reads the emitter's preamble.
    for (name, generated) in [("event", "Event"), ("field", "Field"), ("level", "Level")] {
        let scratch = Scratch::new(&format!("import-collision-{name}"));
        scratch.edit("catalog/log-events.toml", |text| {
            format!(
                "{text}\n[[event]]\nname = \"{name}\"\ndescription = \"A probe event\"\n\
                 level = \"info\"\nfields = [ {{ name = \"rows\", type = \"u64\" }} ]\n"
            )
        });
        scratch.must_fail("name-collision", generated);

        // **And the line it reports is the line the name is on.** `must_fail`
        // reads the check, the sentence and the file, and never `problem.line`
        // — measured, the whole locator replaced by `None` left the entire
        // workspace green. The name was located by a bare substring, so
        // `level`, `event` and `field` each reported a line inside an entry the
        // author never touched, a hundred lines above the one they added.
        //
        // The line moves whenever an entry is added above it, so the number is
        // not the subject: the correspondence is.
        let problems = scratch.problems();
        let reported = problems
            .iter()
            .find(|problem| problem.check == "name-collision")
            .and_then(|problem| problem.line)
            .expect("a name collision is located");
        let text = std::fs::read_to_string(scratch.root.join("catalog/log-events.toml"))
            .expect("the scratch registry is readable");
        let said = text
            .lines()
            .nth(reported - 1)
            .expect("the reported line is in the file");
        assert!(
            said.contains(&format!("name = \"{name}\"")),
            "`{name}` collides on line {reported}, which holds `{said}` — the author is \
             sent to an entry they did not write"
        );
    }
}

/// A field named after a Rust keyword is refused, with the registry line.
///
/// **The keyword list held six names and five of them are not keywords.** It was
/// written for `camel`-cased type names, compared case-sensitively, while
/// `identifier()` has a second caller that hands it a *snake*-cased field name —
/// so `type`, `ref`, `match` and `in` all reached the emitter and the build died
/// inside rustfmt, at an offset into a stream:
///
/// ```text
/// error rustfmt refused the emitted source: expected identifier, found keyword `type`
///          --> <stdin>:170:9
/// ```
///
/// Nothing shipped broken — but the author was sent to a stream offset instead
/// of to the three lines of registry that caused it, which is the whole reason
/// this check exists.
#[test]
fn a_field_named_after_a_keyword_fails_the_build() {
    // A field is `snake`-cased, so what it can collide with is the lowercase
    // half of the list.
    for name in [
        "type", "ref", "match", "in", "move", "box", "loop", "static",
    ] {
        let scratch = Scratch::new(&format!("keyword-field-{name}"));
        scratch.replacing(
            "catalog/log-events.toml",
            "name = \"entries\"",
            &format!("name = \"{name}\""),
        );
        scratch.must_fail("name-not-an-identifier", name);
    }
    // An event name is `camel`-cased, so what it can collide with is the
    // prelude — the other half of the same list, and the half the old six-entry
    // list was reaching for with `Box`.
    for name in ["option", "vec", "result", "string", "box"] {
        let scratch = Scratch::new(&format!("prelude-event-{name}"));
        scratch.edit("catalog/log-events.toml", |text| {
            format!(
                "{text}\n[[event]]\nname = \"{name}\"\ndescription = \"A probe event\"\n\
                 level = \"info\"\nfields = [ {{ name = \"rows\", type = \"u64\" }} ]\n"
            )
        });
        scratch.must_fail("name-not-an-identifier", name);
    }
}

/// An area `NOCTUA.md §15.1` names can carry a code, and a key nobody could use
/// is refused where it is declared.
///
/// **`P2P` was declared, published as available, and unusable.** `area_of`
/// required every byte of the area to be `is_ascii_uppercase`, and `b'2'` is
/// not — so the first peer-to-peer code was refused as *malformed*:
///
/// ```text
/// error catalog/errors.toml:414 — NC-P2P-0001 is not of the form NC-<AREA>-<NNNN>
///       the scheme is fixed by NOCTUA.md §15.1: `NC-`, a declared area, then
///       exactly four digits — such as NC-AUTH-0031
/// ```
///
/// while `P2P` sat thirty lines above in `[areas]` and `docs/errors.md`
/// published it. Nothing validated the keys of that table, so the two halves of
/// one rule could disagree in silence — which is why the second half is here
/// too.
///
/// In a scratch copy, deliberately: a code declared in the real tree is
/// **reserved permanently** by the next build, and undoing that takes the
/// three-step path `ledger.rs` documents.
#[test]
fn an_area_with_a_digit_carries_a_code_and_a_bad_key_is_refused() {
    let scratch = Scratch::new("area-with-a-digit");
    scratch.edit("catalog/errors.toml", |text| {
        format!(
            "{text}\n[[error]]\ncode = \"NC-P2P-0001\"\ndescription = \"A probe\"\n\
             message = \"error.p2p.probe\"\nseverity = \"error\"\nretryable = false\n"
        )
    });
    // Two sentences, because `untranslated` is a rule of its own and a probe
    // that trips it would be reported as this test's failure.
    for (locale, sentence) in [
        ("catalog/i18n/en.toml", "A probe."),
        ("catalog/i18n/pt-BR.toml", "Uma sondagem."),
    ] {
        scratch.edit(locale, |text| {
            format!("{text}\n\"error.p2p.probe\" = \"{sentence}\"\n")
        });
    }
    let problems = scratch.problems();
    assert!(
        problems.is_empty(),
        "`§15.1` names `P2P` as an area and a code in it was refused: {}",
        render(&problems)
    );

    let bad = Scratch::new("area-that-is-not-one");
    bad.replacing(
        "catalog/errors.toml",
        "P2P  = \"Peer-to-peer",
        "p2p  = \"Peer-to-peer",
    );
    bad.must_fail("area-not-an-area", "p2p");
}

/// A field name the generated struct cannot carry is refused at the registry.
///
/// **Three ways in, all measured with `cargo xtask build` reporting `ok`.**
///
/// ```text
/// { name = "max-rows" } beside { name = "max_rows" }
///   cargo build  ->  error[E0124]: field `max_rows` is already declared
/// { name = "Applied" }
///   cargo clippy ->  structure field `Applied` should have a snake case name
/// description = "… from <version> to <version>"
///   cargo doc    ->  error: unclosed HTML tag `version`
/// ```
///
/// Each fails inside `crates/nc-log/src/events.rs`, whose header forbids editing
/// it, with no registry line named — the outcome `load.rs`'s module header says
/// these checks exist to prevent. The third is `doc()`'s to escape; the first two
/// are refusals, because a field is a name somebody chose and can choose again.
#[test]
fn a_field_name_the_generated_struct_cannot_carry_fails_the_build() {
    let colliding = Scratch::new("field-collision");
    colliding.replacing(
        "catalog/log-events.toml",
        "{ name = \"max_rows\", type = \"u64\" }",
        "{ name = \"max_rows\", type = \"u64\" },\n  { name = \"max-rows\", type = \"u64\" }",
    );
    colliding.must_fail("duplicate-field", "max-rows");

    for capitalised in ["Applied", "maxRows", "TOTAL"] {
        let shouting = Scratch::new(&format!("field-case-{capitalised}"));
        shouting.replacing(
            "catalog/log-events.toml",
            "{ name = \"applied\"",
            &format!("{{ name = \"{capitalised}\""),
        );
        shouting.must_fail("field-not-snake-case", capitalised);
    }
}

/// Five values that were accepted and should not have been.
///
/// Each was measured reaching generated output with no diagnostic: a
/// description of three spaces ("the only English an operator reading the log
/// gets"), a command with no name (which is how a command is invoked, how its
/// page titles the section, and the key `nc_catalog::command()` takes), an area
/// that says nothing about what it covers on a page whose column exists to say
/// it, a locale whose `dir` is neither `ltr` nor `rtl`, and a `bcp47` that is
/// not an identifier — the one generated name `load.rs`'s own header promises to
/// check and did not.
///
/// **Nine now, and the four that arrived are a character that shows nothing.**
/// The sites this drives asked `trim().is_empty()`, which sees `""` and `"   "`
/// and not `U+2063`; the rows for those are in
/// [`a_value_showing_nothing_is_refused_too`], which is a function of its own
/// because this one is at clippy's line limit.
#[test]
fn a_value_that_says_nothing_is_refused_where_it_is_written() {
    let blank = Scratch::new("blank-description");
    blank.replacing(
        "catalog/log-events.toml",
        "description = \"Migrations were applied",
        "description = \"   \" # was: \"Migrations were applied",
    );
    blank.must_fail("empty-description", "db.migrated");

    let unnamed = Scratch::new("unnamed-command");
    unnamed.replacing(
        "catalog/commands.toml",
        "name = \"journal.verify\"",
        "name = \"\"",
    );
    unnamed.must_fail("command-unnamed", "commands.toml");

    a_value_showing_nothing_is_refused_too();

    // **And the surface a rate limit is looked up by**, which had no rule at
    // all. Measured on 2026-09-08 with `surface = ""`: `cargo xtask build`
    // reported `ok 15 generated file(s), 2 rewritten`, `check --offline` exited
    // 0, and `docs/limits.md` carried two budget rows for a surface with no
    // name while `nc_catalog::rate_limit("")` answered `Some`. It is the same
    // role `name` plays for a command, one registry over.
    let nameless = Scratch::new("rate-limit-unnamed");
    nameless.replacing(
        "catalog/limits.toml",
        "surface = \"password_reset\"",
        "surface = \"\"",
    );
    nameless.must_fail("rate-limit-unnamed", "limits.toml");
    // **Once, and located.** The rule was written twice byte for byte, so one
    // nameless surface produced the same sentence twice and the second copy
    // decided nothing; and its anchor read `[[rate_limits]]` while the table is
    // `[[rate_limit]]`, so `line_of` never matched and the message reached a
    // developer as a bare filename in a 196-line file holding five of these
    // blocks. Every sibling rule in that file is located, and
    // `a_diagnostic_about_something_present_in_a_file_says_which_line` is the
    // same principle one registry over.
    let said: Vec<Problem> = nameless
        .problems()
        .into_iter()
        .filter(|problem| problem.check == "rate-limit-unnamed")
        .collect();
    assert_eq!(
        said.len(),
        1,
        "one nameless surface produced {} diagnostic(s): {said:#?}",
        said.len()
    );
    let text = std::fs::read_to_string(nameless.root.join("catalog/limits.toml"))
        .expect("the copy is readable");
    let first = text
        .lines()
        .position(|line| line.trim() == "[[rate_limit]]")
        .map(|before| before + 1)
        .expect("the registry declares rate limits");
    assert!(
        said[0].line.is_some_and(|at| at >= first),
        "the diagnostic is filed at {:?} and the first `[[rate_limit]]` is at line \
         {first}, so a developer is told the file and not which of its rate limits: \
         {:#?}",
        said[0].line,
        said[0]
    );

    // And one space, which is the half the two rules below this were missing:
    // both trimmed and neither was ever handed whitespace.
    let spaced = Scratch::new("rate-limit-spaced");
    spaced.replacing(
        "catalog/limits.toml",
        "surface = \"password_reset\"",
        "surface = \"   \"",
    );
    spaced.must_fail("rate-limit-unnamed", "limits.toml");

    let quiet = Scratch::new("area-undescribed");
    quiet.edit("catalog/errors.toml", |text| {
        let at = text.find("CANON = \"").expect("the area is declared");
        let end = text[at..].find('\n').expect("a line") + at;
        format!("{}CANON = \"\"{}", &text[..at], &text[end..])
    });
    quiet.must_fail("area-undescribed", "CANON");

    // **One space, not the empty string.** Both this rule and
    // `store-enforced-by-nobody` trim before they test, and both were driven
    // with `""` alone — so the trim was decided by nothing and deleting it from
    // either left the whole workspace suite green. `check_errors`' own comment
    // records that regression being repaired once already; the repair went in
    // with no test under it.
    let spaced_area = Scratch::new("area-undescribed-spaced");
    spaced_area.edit("catalog/errors.toml", |text| {
        let at = text.find("CANON = \"").expect("the area is declared");
        let end = text[at..].find('\n').expect("a line") + at;
        format!("{}CANON = \"   \"{}", &text[..at], &text[end..])
    });
    spaced_area.must_fail("area-undescribed", "CANON");

    // **The endonym, the third field of `[meta]`.** It reaches the generated
    // `///` line and `Locale::name()`, and nothing checked it: blank made the
    // build say `ok` and rustc fail inside a file nobody may edit, and a dash
    // shipped green with the switcher entry naming nothing.
    for said in ["\"\"", "\"   \"", "\"-\"", "\"--\"", "\"0\""] {
        let unnamed = Scratch::new(&format!("locale-unnamed-{}", said.len()));
        unnamed.replacing(
            "catalog/i18n/en.toml",
            "name  = \"English\"",
            &format!("name  = {said}"),
        );
        unnamed.must_fail("locale-unnamed", "en.toml");
    }

    let sideways = Scratch::new("locale-direction");
    sideways.replacing(
        "catalog/i18n/en.toml",
        "dir   = \"ltr\"",
        "dir   = \"sideways\"",
    );
    sideways.must_fail("locale-direction", "sideways");

    let numbered = Scratch::new("locale-identifier");
    numbered.replacing("catalog/i18n/en.toml", "bcp47 = \"en\"", "bcp47 = \"2x\"");
    numbered.must_fail("locale-not-an-identifier", "2x");
}

/// The same absence written as a character that shows nothing.
///
/// Its own function because the caller is at clippy's line limit — the same
/// reason three of its neighbours are functions.
fn a_value_showing_nothing_is_refused_too() {
    // **The three spellings of the same absence, at the sites that only had
    // one.** These rules asked `trim().is_empty()`, which sees `""` and `"   "`
    // and not a character that shows nothing — and this test drove only the
    // first two, at four of the seven sites. Measured on 2026-09-11, one value
    // at a time replaced by a single `U+2063 INVISIBLE SEPARATOR` against the
    // shipped catalog: the error area's description, a command's name, a
    // limit's `surface`, a store's `enforced_by` and a log event's description
    // were **all accepted with no problem at all**, and `surface` carried
    // through to two published budget rows for a surface with no name.
    for (what, file, from, to, check, naming) in [
        (
            "an error area described by nothing a reader sees",
            "catalog/errors.toml",
            "CANON = ",
            "CANON = \"\u{2063}\" # was: ",
            "area-undescribed",
            "CANON",
        ),
        (
            "a command named by nothing a reader sees",
            "catalog/commands.toml",
            "name = \"journal.verify\"",
            "name = \"\u{2063}\"",
            "command-unnamed",
            "commands.toml",
        ),
        (
            "a rate limit whose surface shows nothing",
            "catalog/limits.toml",
            "surface = \"password_reset\"",
            "surface = \"\u{2063}\"",
            "rate-limit-unnamed",
            "limits.toml",
        ),
        (
            "a store nothing legible enforces",
            "catalog/limits.toml",
            "enforced_by = \"the log store's own retention flags",
            "enforced_by = \"\u{2063}\" # was: \"the log store's own retention flags",
            "store-enforced-by-nobody",
            "limits.toml",
        ),
        // **The five this corpus's own prose named and did not drive.** Each
        // site asks `says_nothing`, and each could be put back to
        // `trim().is_empty()` with all 101 tests in this crate green: measured
        // one at a time, the error description, the ordering column, a
        // command's action, its resource selector and a log event's
        // description. A value made of invisible separators is accepted by
        // `trim`, published as an empty cell, and — for the ordering column —
        // handed to the pruner as the column to sort by.
        (
            "an error described by nothing a reader sees",
            "catalog/errors.toml",
            "description = \"The journal's hash chain is broken at an entry\"",
            "description = \"\u{2063}\"",
            "empty-description",
            "errors.toml",
        ),
        (
            "a table ordered by nothing a reader sees",
            "catalog/limits.toml",
            "order_by = \"sequence\"",
            "order_by = \"\u{2063}\"",
            "no-order-column",
            "journal_entries",
        ),
        (
            "a command whose action shows nothing",
            "catalog/commands.toml",
            "action = \"Read\"",
            "action = \"\u{2063}\"",
            "command-unauthorizable",
            "commands.toml",
        ),
        (
            "a command whose resource selector shows nothing",
            "catalog/commands.toml",
            "resource_selector = \"realm\"",
            "resource_selector = \"\u{2063}\"",
            "command-unauthorizable",
            "commands.toml",
        ),
        (
            "a log event described by nothing a reader sees",
            "catalog/log-events.toml",
            "description = \"An entry was appended to the chain\"",
            "description = \"\u{2063}\"",
            "empty-description",
            "log-events.toml",
        ),
    ] {
        let invisible = Scratch::new(&format!("invisible-{check}"));
        invisible.replacing(file, from, to);
        assert!(
            !invisible.problems().is_empty(),
            "{what}: the catalog was broken on purpose and the build accepted it"
        );
        invisible.must_fail(check, naming);
    }
}

#[test]
fn a_field_the_store_reserves_fails_the_build() {
    // The store owns every name starting with `_`. `_time` is the record's own
    // timestamp: a line carrying one is accepted with `200 OK` and then dropped,
    // so the shipper counts it delivered and the record is nowhere. Refused here
    // rather than left to `clippy::pub_underscore_fields`, which is a lint on the
    // emitted struct and a lint anybody may allow.
    let scratch = Scratch::new("reserved-field");
    scratch.replacing(
        "catalog/log-events.toml",
        "name = \"applied\"",
        "name = \"_time\"",
    );
    scratch.must_fail("field-the-log-owns", "_time");
}

#[test]
fn a_curve_that_ends_at_a_millisecond_fails_the_build() {
    // `curve-ends-free` refuses a final `0` because the last value repeats for
    // ever. A final `1` repeats just as freely and satisfied it: measured,
    // `login` — whose own comment says the escalating delay is what carries that
    // surface — given `[0, 0, 0, 0, 0, 0, 1]`, with `cargo xtask check` green
    // and the curve republished on `docs/limits.md`. Ten attempts free and then
    // a millisecond each is an unlimited surface with a table about it.
    let scratch = Scratch::new("cheap-curve");
    scratch.replacing(
        "catalog/limits.toml",
        "delay_ms = [0, 0, 500, 1_000, 2_000, 5_000, 10_000]",
        "delay_ms = [0, 0, 0, 0, 0, 0, 1]",
    );
    scratch.must_fail("curve-ends-cheap", "login");
}

/// A lockout exempts a cheap curve only if it arrives before the tail repeats.
///
/// **The exemption was `lockout_after.is_none()`** — so any lockout at all
/// bought it, however far away. `login` given `delay_ms = [0, 0, 0, 0, 0, 0, 1]`
/// and `lockout_after = 1_000_000` passed the whole gate and republished the
/// curve on `docs/limits.md`: a million tries at a millisecond each is about
/// seventeen minutes of unthrottled guessing before the surface locks, on the
/// one surface whose own comment in `catalog/limits.toml` says the escalating
/// delay is what carries it.
///
/// The hole and the rule are both asserted here, because an exemption is a gate
/// switched off for a named case and only the case can say whether it was the
/// right one.
#[test]
fn a_lockout_too_far_away_does_not_excuse_a_cheap_curve() {
    let scratch = Scratch::new("cheap-curve-far-lockout");
    scratch.replacing(
        "catalog/limits.toml",
        "per_identifier = { window_seconds = 900, max_attempts = 10, delay_ms = [0, 0, 500, 1_000, 2_000, 5_000, 10_000] }",
        "per_identifier = { window_seconds = 900, max_attempts = 10, delay_ms = [0, 0, 0, 0, 0, 0, 1], lockout_after = 1_000_000 }",
    );
    scratch.must_fail("curve-ends-cheap", "login");
}

/// And a lockout inside the curve's own length does excuse it.
///
/// The other half of the exemption: with the tail unreachable, the cheap value
/// never repeats and there is nothing left to complain about. Without this the
/// rule above could be tightened to "no exemption at all" and no test would
/// notice — which is how a gate stops being a rule and becomes a refusal.
#[test]
fn a_lockout_inside_the_curve_excuses_a_cheap_one() {
    let scratch = Scratch::new("cheap-curve-near-lockout");
    scratch.replacing(
        "catalog/limits.toml",
        "per_identifier = { window_seconds = 900, max_attempts = 10, delay_ms = [0, 0, 500, 1_000, 2_000, 5_000, 10_000] }",
        "per_identifier = { window_seconds = 900, max_attempts = 6, delay_ms = [0, 0, 0, 0, 0, 0, 1], lockout_after = 7 }",
    );
    let problems = scratch.problems();
    // **`is_empty`, not "no `curve-ends-cheap` among them".** `problems` was
    // empty, so `.all(…)` was true of nothing — the assertion this test exists
    // for passed by having no elements. It also could not tell "accepted" from
    // "refused for some other reason": with a probe refusal added to
    // `check_budget` that fires on this very fixture, seven tests in this file
    // went red and this one stayed green, which is the opposite of what a
    // positive control is for. The three sibling positive controls in this file
    // all spell it this way.
    assert!(
        problems.is_empty(),
        "a lockout that fires at the last delay leaves nothing repeating: {}",
        render(&problems)
    );
}

/// An English sentence with the punctuation nudged is still an English sentence.
///
/// **`untranslated` compared bytes**, so the rule was one keystroke wide: the
/// same text with a trailing space, a different capital, or a doubled space
/// passed as a translation, and every gate downstream — presence, placeholders,
/// and both test floors phrased as fractions — then reads it as translated. The
/// published page shows one sentence in two language columns, which is the exact
/// failure the byte comparison exists to catch.
#[test]
fn a_translation_that_only_moves_punctuation_fails_the_build() {
    const KEY: &str = "\"error.db.migration-failed\"";
    let scratch = Scratch::new("near-copy-translation");
    let line_for = |file: &str| -> String {
        let text =
            std::fs::read_to_string(scratch.root.join(file)).expect("the locale is readable");
        let at = text.find(KEY).expect("the key is declared in this locale");
        text[at..].lines().next().expect("a line").to_owned()
    };
    let english = line_for("catalog/i18n/en.toml");
    let value = english
        .split_once(" = ")
        .expect("a value")
        .1
        .trim()
        .trim_matches('"')
        .to_owned();
    // The copy-paste, and the four things somebody changes without thinking:
    // a doubled space, a full stop dropped, a space left on the end, and the
    // capital at the front. Each one alone made this pass as a translation.
    //
    // **Case was absent, and `the_same_sentence` folds on it.** The comment
    // listed three and the fold does three-plus-one, so `text.to_lowercase()`
    // could be reverted to `text.to_owned()` with the crate suite and the whole
    // gate green: the other two thirds of the same rule are what caught every
    // plant. A rule normalised on three things needs three cases.
    // **One mark, and the set names seven.** The fixture trimmed a full stop and
    // nothing else, so `!`, `?`, `:`, `;`, `,` and `…` were decided by no input
    // — measured, the whole set narrowed to `matches!(character, '.')` and the
    // crate suite and the gate both green. An English sentence pasted into a
    // translation with its `!` adjusted then passes as a translation. Every mark
    // in the set is trimmed in turn.
    for mark in ['.', '!', '?', ':', ';', ',', '…'] {
        let one = format!("{}{mark}", value.trim_end_matches(mark));
        assert!(
            nc_catalog_compiler::the_same_sentence_for_tests(&one, value.trim_end_matches(mark)),
            "`{mark}` is in the punctuation the untranslated rule ignores and trimming \
             it made two sentences different, so that mark is decided by no input"
        );
    }
    let cased = value
        .trim_end_matches('.')
        .replacen(' ', "  ", 1)
        .to_uppercase();
    let nearly = format!("{cased} ");
    let ours = line_for("catalog/i18n/pt-BR.toml");
    assert_ne!(ours, english, "the two locales already hold the same line");
    scratch.edit("catalog/i18n/pt-BR.toml", |text| {
        text.replace(&ours, &format!("{KEY} = \"{nearly}\""))
    });
    scratch.must_fail("untranslated", "error.db.migration-failed");
}

/// An error whose message key is outside the `error.` namespace.
///
/// **A live branch nothing reached.** `emit::error_variant` stripped `error.`
/// from the message key and camelled what was left, with an `unwrap_or` for a
/// key outside that namespace — and no rule required one. Measured, a code whose
/// message key is `probe.p2p.thing`: `ACCEPTED, 0 problem(s)`, and the generated
/// crate grew `ErrorCode::ProbeP2pThing` — a public name for something that does
/// not read as an error, decided by a fallback no test entered.
///
/// Refused at the registry rather than tested at the emitter, which is
/// `load.rs`'s own rule: every generated name is decided where it is written.
/// The `unwrap_or` is gone, so the emitter says what is true.
#[test]
fn an_error_message_key_outside_its_namespace_fails_the_build() {
    let scratch = Scratch::new("error-message-namespace");
    let text = std::fs::read_to_string(scratch.root.join("catalog/errors.toml"))
        .expect("the registry is readable");
    let line = text
        .lines()
        .find(|line| line.trim_start().starts_with("message = \"error."))
        .expect("an error declares a message key in the namespace")
        .to_owned();
    let outside = line.replace("\"error.", "\"probe.");
    assert_ne!(outside, line, "the key was already outside the namespace");
    scratch.edit("catalog/errors.toml", |text| text.replace(&line, &outside));
    scratch.must_fail("error-message-namespace", "probe.");
}

/// A command's argument schema carrying a character a reader sees nothing of.
///
/// **The one registry value written to a published page with no escaper.**
/// Every description reaches `docs/` through `cell`, which spends the invisible
/// class; the schema goes into a fenced JSON block verbatim, because a caller is
/// validated against exactly these bytes and the pages test requires the page to
/// hold them as written. Measured, a schema carrying `U+202E` and `U+202C`:
/// accepted, and both reached `docs/commands.md` raw while
/// `crates/nc-catalog/src/commands.rs` held neither.
///
/// `catalog/commands.toml`'s own header says the schema is what exposes a
/// command to an AI agent, and `INVISIBLE_RANGES`' comment names the tag block
/// for that reason — text that reads one way to a person and another to whatever
/// consumes it. The repair that closed this class for descriptions closed it at
/// `cell`, and the one value that does not go through `cell` is the one the
/// comment is about.
///
/// The first half is the too-strong control: JSON's own whitespace is exempt, so
/// a pretty-printed multi-line schema still compiles. `\n` is inside
/// `INVISIBLE_RANGES` (`0x0000..=0x001F`), so a blanket refusal would reject the
/// ordinary way a JSON Schema is written.
#[test]
fn a_schema_a_reader_cannot_see_all_of_fails_the_build() {
    const SHIPPED: &str = "{\"type\":\"object\",\"properties\":{\"from\":{\"type\":\"integer\",\"minimum\":0}},\"additionalProperties\":false}";
    let scratch = Scratch::new("schema-invisible");
    let holds = std::fs::read_to_string(scratch.root.join("catalog/commands.toml"))
        .expect("the registry is readable");
    assert!(
        holds.contains(SHIPPED),
        "the schema this test edits is not the one the registry ships"
    );

    // The control: the same JSON over several lines, which is how most schemas
    // are written and which a blanket refusal of `INVISIBLE_RANGES` would reject.
    let pretty = SHIPPED.replacen('{', "{\n  ", 1);
    scratch.edit("catalog/commands.toml", |text| {
        text.replace(SHIPPED, &pretty)
    });
    assert!(
        scratch.problems().is_empty(),
        "a pretty-printed multi-line schema was refused, so this rule refuses the \
         ordinary way a JSON Schema is written: {:#?}",
        scratch.problems()
    );

    let hidden = pretty.replacen('{', "{\u{202e}\u{202c}", 1);
    scratch.edit("catalog/commands.toml", |text| {
        text.replace(&pretty, &hidden)
    });
    scratch.must_fail_saying("schema-invisible-character", "202E");
}

/// A translation that is the English sentence with a space written invisibly.
///
/// **The normaliser and the writer disagreed about what a reader sees.**
/// `the_same_sentence`'s `plainly` *deleted* every character `INVISIBLE_RANGES`
/// holds; `printable` — which writes every `///` line and every published cell —
/// replaces the same characters with a space. So the English sentence with one
/// of its spaces written `U+200D` normalised to `oldrecords` for this rule and
/// published as `Old records` on the page: measured, filed as the pt-BR value,
/// `ACCEPTED, 0 problem(s)`, and `docs/errors.md` then carried the identical
/// English sentence in *both* language columns — the exact failure this rule
/// exists to catch, arriving through the one transformation nobody had compared.
///
/// Clippy cannot see it either: `quoted` writes the character as the ASCII
/// escape `\u{200d}`.
#[test]
fn a_translation_that_is_the_source_sentence_with_a_space_written_invisibly_fails_the_build() {
    const KEY: &str = "\"error.db.unreachable\"";
    let scratch = Scratch::new("invisible-space-translation");
    let line_for = |file: &str| -> String {
        let text =
            std::fs::read_to_string(scratch.root.join(file)).expect("the locale is readable");
        let at = text.find(KEY).expect("the key is declared in this locale");
        text[at..].lines().next().expect("a line").to_owned()
    };
    let english = line_for("catalog/i18n/en.toml");
    let value = english
        .split_once(" = ")
        .expect("a value")
        .1
        .trim()
        .trim_matches('"')
        .to_owned();
    // One space written as a zero-width joiner. Nothing else changes.
    let forged = value.replacen(' ', "\u{200d}", 1);
    assert_ne!(
        forged, value,
        "the sentence holds no space to write invisibly"
    );
    let ours = line_for("catalog/i18n/pt-BR.toml");
    assert_ne!(ours, english, "the two locales already hold the same line");
    scratch.edit("catalog/i18n/pt-BR.toml", |text| {
        text.replace(&ours, &format!("{KEY} = \"{forged}\""))
    });
    scratch.must_fail("untranslated", "error.db.unreachable");
}

/// A sentence made only of characters a reader sees nothing of.
///
/// **`no_key_is_blank` asked `char::is_whitespace` where the rest of the file
/// asks `shows_nothing`.** Its own doc says it exists because `""` and `"   "`
/// both passed every rule here; a value of three `U+2063 INVISIBLE SEPARATOR`
/// characters did too — and unlike the first two, nothing downstream catches it.
/// The published cell is empty and `nc_catalog::message` hands a caller a string
/// a reader sees nothing of. The control one predicate away, `"   "`, was
/// refused as `blank-message` throughout.
#[test]
fn a_sentence_a_reader_sees_nothing_of_fails_the_build() {
    const KEY: &str = "\"error.db.unreachable\"";
    let scratch = Scratch::new("invisible-blank-message");
    let text =
        std::fs::read_to_string(scratch.root.join("catalog/i18n/pt-BR.toml")).expect("readable");
    let at = text.find(KEY).expect("the key is declared in this locale");
    let ours = text[at..].lines().next().expect("a line").to_owned();
    scratch.edit("catalog/i18n/pt-BR.toml", |text| {
        text.replace(&ours, &format!("{KEY} = \"\u{2063}\u{2063}\u{2063}\""))
    });
    scratch.must_fail("blank-message", "error.db.unreachable");
}

#[test]
fn a_code_that_changes_retryability_fails_the_build() {
    // `retryable` was outside the recorded meaning, so `NC-DB-0002` — "a
    // migration failed to apply", `critical` — could go from `false` to `true`
    // with the whole gate green, its ledger line unchanged, `docs/errors.md`
    // republishing it as retryable and `ErrorCode::retryable()` returning
    // `true`. A scheduler keyed on that field then retries a failed migration
    // for ever, and logs from before and after are indistinguishable — which is
    // the property this ledger exists to protect.
    let scratch = Scratch::new("retryable-repointed");
    scratch.edit("catalog/errors.toml", |text| {
        let at = text.find("NC-DB-0002").expect("the code is declared");
        let from = text[at..]
            .find("retryable = false")
            .expect("it is not retryable")
            + at;
        let mut moved = text.clone();
        moved.replace_range(from..from + "retryable = false".len(), "retryable = true");
        moved
    });
    scratch.must_fail("code-repointed", "NC-DB-0002");
}

#[test]
fn a_locale_value_that_is_the_source_sentence_fails_the_build() {
    // Every i18n rule was about a key's presence and its placeholders, and none
    // about what the value says: seven pt-BR sentences replaced verbatim with
    // their English text compiled, generated, and published both language
    // columns of `docs/errors.md` identical, with `check passed`. The only
    // bounds were two test floors phrased as fractions, which a handful of
    // untranslated strings satisfies comfortably.
    let scratch = Scratch::new("untranslated");
    scratch.edit("catalog/i18n/pt-BR.toml", |text| {
        let english = std::fs::read_to_string(repository().join("catalog/i18n/en.toml"))
            .expect("the source locale is there");
        let key = "\"error.journal.chain-broken\"";
        let line = english
            .lines()
            .find(|line| line.trim_start().starts_with(key))
            .expect("the key is in the source locale");
        text.lines()
            .map(|held| {
                if held.trim_start().starts_with(key) {
                    line
                } else {
                    held
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    });
    scratch.must_fail("untranslated", "error.journal.chain-broken");
}

#[test]
fn a_code_that_changes_severity_fails_the_build() {
    // A code's recorded meaning is its key, its severity and its description.
    // `check_errors` never read `severity` at all, so `NC-JRNL-0002` — "an
    // entry's hash does not match the entry's own contents" — could go from
    // `critical` to `info` with the whole gate green and `docs/errors.md`
    // republishing it as informational. Every consumer pages on
    // `>= Severity::Error`, so that one word decides whether journal tampering
    // wakes anybody.
    let scratch = Scratch::new("severity-repointed");
    scratch.edit("catalog/errors.toml", |text| {
        let at = text.find("NC-JRNL-0002").expect("the code is declared");
        let from = text[at..]
            .find("severity = \"critical\"")
            .expect("it is critical")
            + at;
        let mut moved = text.clone();
        moved.replace_range(
            from..from + "severity = \"critical\"".len(),
            "severity = \"info\"",
        );
        moved
    });
    scratch.must_fail("code-repointed", "NC-JRNL-0002");
}

#[test]
fn a_store_that_keeps_nothing_fails_the_build() {
    // `[stores.*]` had no validation function at all: it was counted and never
    // read, while `[tables.*]` is validated seven ways. The two numbers here are
    // what `cargo xtask check` compares the running log store's flags against,
    // so a zero is a bound the gate then certifies the store as honouring.
    let scratch = Scratch::new("store-keeps-nothing");
    scratch.replacing(
        "catalog/limits.toml",
        "retention_days = 30",
        "retention_days = 0",
    );
    scratch.must_fail("store-keeps-nothing", "central_log");
}

#[test]
fn a_store_with_no_disk_fails_the_build() {
    let scratch = Scratch::new("store-has-no-disk");
    scratch.replacing(
        "catalog/limits.toml",
        "max_disk_bytes = 8_589_934_592",
        "max_disk_bytes = 0",
    );
    scratch.must_fail("store-has-no-disk", "central_log");
}

#[test]
fn a_store_nothing_enforces_fails_the_build() {
    // `enforced_by` is the sentence
    // `xtask/tests/the_stack_enforces_the_declared_bounds.rs` reads to decide
    // what to check, so an empty one is a limit with no enforcer and a test that
    // looks for one.
    let scratch = Scratch::new("store-enforced-by-nobody");
    scratch.replacing(
        "catalog/limits.toml",
        "enforced_by = \"the log store's own retention flags, set in docker-compose.yml\"",
        "enforced_by = \"\"",
    );
    scratch.must_fail("store-enforced-by-nobody", "central_log");

    // **And one space**, which is what the rule's `trim` is for and what nothing
    // handed it: deleting the trim left the whole workspace suite green, and the
    // suite that reads this sentence would then be given three spaces to look
    // for an enforcer in. `check_errors`' own comment records the same
    // regression being repaired once, with no test put under the repair.
    let spaced = Scratch::new("store-enforced-by-spaces");
    spaced.replacing(
        "catalog/limits.toml",
        "enforced_by = \"the log store's own retention flags, set in docker-compose.yml\"",
        "enforced_by = \"   \"",
    );
    spaced.must_fail("store-enforced-by-nobody", "central_log");
}

#[test]
fn a_window_of_no_length_fails_the_build() {
    // **Every other number in a budget is a count inside the window, and the
    // window itself was checked by nothing.** `max_attempts`, the delay curve
    // and `lockout_after` are all "in `window_seconds`", so zero empties all
    // three at once — and nine budgets shipped with nothing able to say so.
    // What the page then publishes is `Window 0s` beside an attempt count,
    // which reads as a limit and is not one.
    let scratch = Scratch::new("zero-window");
    scratch.replacing(
        "catalog/limits.toml",
        "window_seconds = 900",
        "window_seconds = 0",
    );
    scratch.must_fail("window-of-no-length", "login");
}

#[test]
fn a_cumulative_count_with_no_lockout_fails_the_build() {
    // `cumulative`'s own documentation is "whether the lockout count survives
    // the window", so with no lockout there is no count to survive — the same
    // shape as `on_lockout` without `lockout_after`, which has been refused
    // since it was written. Set on a budget that never locks out it reads, in
    // the registry and on the generated page, as a defence that is not there.
    let scratch = Scratch::new("cumulative-without-lockout");
    scratch.replacing(
        "catalog/limits.toml",
        "per_identifier = { window_seconds = 900, max_attempts = 10,",
        "per_identifier = { cumulative = true, window_seconds = 900, max_attempts = 10,",
    );
    scratch.must_fail("cumulative-without-lockout", "login");
}

#[test]
fn a_duplicate_rate_limit_surface_fails_the_build() {
    let scratch = Scratch::new("duplicate-surface");
    scratch.edit("catalog/limits.toml", |mut text| {
        text.push_str(
            "\n[[rate_limit]]\nsurface = \"login\"\n\
             per_identifier = { window_seconds = 60, max_attempts = 1, delay_ms = [1_000] }\n\
             per_address = { window_seconds = 60, max_attempts = 1, delay_ms = [1_000] }\n",
        );
        text
    });
    scratch.must_fail("duplicate-surface", "login");
}

// --- commands ---

#[test]
fn a_command_that_reads_and_mutates_fails_the_build() {
    let scratch = Scratch::new("command");
    scratch.edit("catalog/commands.toml", |text| {
        text.replace(
            "reversibility = \"read\"\nmutating = false",
            "reversibility = \"read\"\nmutating = true",
        )
    });
    scratch.must_fail("read-that-mutates", "journal.verify");
}

#[test]
fn a_command_whose_argument_schema_is_not_json_fails_the_build() {
    // The schema is the command's AI tool interface. A malformed one removes the
    // tool, and nothing else in the system would notice.
    let scratch = Scratch::new("schema");
    scratch.edit("catalog/commands.toml", |text| {
        text.replace(
            r#"{"type":"object","properties":{"from":{"type":"integer","minimum":0}},"additionalProperties":false}"#,
            "{not json",
        )
    });
    scratch.must_fail("schema-unparsable", "journal.verify");
}

#[test]
fn a_command_with_no_action_fails_the_build() {
    let scratch = Scratch::new("no-action");
    scratch.edit("catalog/commands.toml", |text| {
        text.replace("action = \"Read\"", "action = \"\"")
    });
    scratch.must_fail("command-unauthorizable", "journal.verify");
}

/// The other half of the same rule, which no fixture drove.
///
/// **`says_nothing(action) || says_nothing(resource_selector)`** — and only the
/// action half had a fixture, so the selector disjunct could be deleted with
/// every test in the tree green. A command with no selector is one the policy
/// decision point has nothing to evaluate against, which `NOCTUA.md §11.7`
/// says is rejected at activation: the build is where that has to be caught,
/// because a command that reaches activation has already shipped.
#[test]
fn a_command_with_no_resource_selector_fails_the_build() {
    let scratch = Scratch::new("no-selector");
    scratch.edit("catalog/commands.toml", |text| {
        text.replace("resource_selector = \"realm\"", "resource_selector = \"\"")
    });
    scratch.must_fail("command-unauthorizable", "journal.verify");
}

/// The schema `journal.verify` ships, which the cases below break one way each.
const VERIFY_SCHEMA: &str = r#"{"type":"object","properties":{"from":{"type":"integer","minimum":0}},"additionalProperties":false}"#;

/// A keyword outside the subset the platform validates is refused at build.
///
/// **A keyword a validator does not know is a keyword it ignores**, and the
/// schema is the command's AI tool interface: `pattern` published on
/// `docs/commands.md` reads as a constraint to every agent and constrains
/// nothing. `NOCTUA.md §11.7` has the kernel refuse it at `activate`, through
/// the same judge.
#[test]
fn a_schema_keyword_outside_the_subset_fails_the_build() {
    for keyword in ["pattern", "$ref", "oneOf", "format", "default"] {
        let scratch = Scratch::new(&format!("schema-keyword-{}", keyword.len()));
        scratch.replacing(
            "catalog/commands.toml",
            VERIFY_SCHEMA,
            &VERIFY_SCHEMA.replace(r#""minimum":0"#, &format!(r#""minimum":0,"{keyword}":"x""#)),
        );
        scratch.must_fail_saying("schema-unknown-keyword", keyword);
    }
}

/// A schema inside the keyword list and outside the subset is refused too.
///
/// One case per shape the subset refuses that a catalog author could plausibly
/// write — an open object, a type with no fraction-free spelling, bounds that
/// cross, a keyword on the wrong type — each naming what it refuses.
#[test]
fn a_schema_outside_the_subset_fails_the_build() {
    for (name, schema, naming) in [
        (
            "open",
            r#"{"type":"object","properties":{"from":{"type":"integer","minimum":0}}}"#,
            "additionalProperties",
        ),
        (
            "number",
            r#"{"type":"object","properties":{"from":{"type":"number"}},"additionalProperties":false}"#,
            "`number`",
        ),
        (
            "crossed",
            r#"{"type":"object","properties":{"from":{"type":"integer","minimum":2,"maximum":1}},"additionalProperties":false}"#,
            "`maximum`",
        ),
        (
            "misplaced",
            r#"{"type":"object","properties":{"from":{"type":"integer","maxLength":3}},"additionalProperties":false}"#,
            "`maxLength`",
        ),
        (
            "required",
            r#"{"type":"object","properties":{"from":{"type":"integer"}},"required":["to"],"additionalProperties":false}"#,
            "`to`",
        ),
    ] {
        let scratch = Scratch::new(&format!("schema-subset-{name}"));
        scratch.replacing("catalog/commands.toml", VERIFY_SCHEMA, schema);
        scratch.must_fail_saying("schema-outside-the-subset", naming);
    }
}

/// A command with no argument schema at all is refused, naming the command.
#[test]
fn a_command_with_no_argument_schema_fails_the_build() {
    let scratch = Scratch::new("schema-missing");
    scratch.replacing("catalog/commands.toml", VERIFY_SCHEMA, "");
    scratch.must_fail("schema-missing", "journal.verify");
}

/// A selector outside the grammar is refused, and so is one naming an argument
/// the schema does not back.
///
/// **The two halves are two checks**, because the remedies differ: a typo in
/// the selector is fixed in the selector, and a selector reading `from` — an
/// optional integer — is fixed in the schema or by selecting `realm`.
#[test]
fn a_selector_the_grammar_or_the_schema_does_not_back_fails_the_build() {
    let spelled = Scratch::new("selector-malformed");
    spelled.edit("catalog/commands.toml", |text| {
        text.replacen(
            "resource_selector = \"realm\"",
            "resource_selector = \"Realm\"",
            1,
        )
    });
    spelled.must_fail_saying("selector-malformed", "Realm");

    for (name, selector, naming) in [
        ("undeclared", "argument:target", "target"),
        ("optional", "argument:from", "from"),
    ] {
        let unbacked = Scratch::new(&format!("selector-{name}"));
        unbacked.edit("catalog/commands.toml", |text| {
            text.replacen(
                "resource_selector = \"realm\"",
                &format!("resource_selector = \"{selector}\""),
                1,
            )
        });
        unbacked.must_fail_saying("selector-names-no-entity", naming);
    }
}

/// A selector reaching the realm through an argument is refused.
///
/// Found by review: the judge accepted `argument:<name>` over a property whose
/// `x-entity` is `Realm`, so a command could hand the decision point a realm id
/// its caller wrote — and the rule that keeps a mutating realm-wide selector
/// off a plugin is written against `realm`. The remedy differs from the other
/// selector refusals: nothing in the schema fixes it, and `realm` does.
#[test]
fn a_selector_reaching_the_realm_through_an_argument_fails_the_build() {
    let scratch = Scratch::new("selector-realm-argument");
    scratch.replacing(
        "catalog/commands.toml",
        VERIFY_SCHEMA,
        r#"{"type":"object","properties":{"from":{"type":"integer","minimum":0},"realm_id":{"type":"string","x-entity":"Realm","maxLength":128}},"required":["realm_id"],"additionalProperties":false}"#,
    );
    scratch.edit("catalog/commands.toml", |text| {
        text.replacen(
            "resource_selector = \"realm\"",
            "resource_selector = \"argument:realm_id\"",
            1,
        )
    });
    scratch.must_fail_saying("selector-realm-through-an-argument", "realm_id");
}

/// A command name outside the grammar the kernel will activate is refused.
#[test]
fn a_command_name_outside_the_grammar_fails_the_build() {
    for name in ["journalverify", "Journal.verify", "journal..verify"] {
        let scratch = Scratch::new(&format!("command-name-{}", name.len()));
        scratch.replacing(
            "catalog/commands.toml",
            "name = \"journal.verify\"",
            &format!("name = \"{name}\""),
        );
        scratch.must_fail_saying("command-name-malformed", name);
    }
}

/// Every problem the judge finds in one command is reported, not the first.
///
/// Found by review: every other command case here breaks one field, so the
/// compiler could keep the judge's first problem and drop the rest with every
/// suite green — against its own promise that an author learns every defect in
/// one build. This breaks two fields of one command and asks for both.
#[test]
fn every_problem_in_one_command_is_reported_rather_than_the_first() {
    let scratch = Scratch::new("two-problems");
    scratch.replacing(
        "catalog/commands.toml",
        "name = \"journal.verify\"",
        "name = \"Journal.verify\"",
    );
    scratch.edit("catalog/commands.toml", |text| {
        text.replacen(
            "resource_selector = \"realm\"",
            "resource_selector = \"Realm\"",
            1,
        )
    });
    scratch.must_fail_saying("command-name-malformed", "Journal.verify");
    scratch.must_fail_saying("selector-malformed", "Realm");
}

/// A title that is empty, or longer than a key could reasonably be, is refused.
#[test]
fn a_command_title_that_is_empty_or_too_long_fails_the_build() {
    let untitled = Scratch::new("command-untitled");
    untitled.replacing(
        "catalog/commands.toml",
        "title = \"command.journal.verify.title\"",
        "title = \"  \"",
    );
    untitled.must_fail_saying("command-untitled", "journal.verify");

    let long = Scratch::new("command-title-too-long");
    long.replacing(
        "catalog/commands.toml",
        "title = \"command.journal.verify.title\"",
        &format!("title = \"command.journal.verify.{}\"", "t".repeat(128)),
    );
    long.must_fail_saying("command-title-too-long", "journal.verify");
}

/// A title holding a character a reader sees nothing of is refused.
///
/// The title is what a person reads before running a command; the judge holds
/// it to the table the argument schema is held to, and this is that rule
/// reaching the catalog under a check of its own.
#[test]
fn a_command_title_a_reader_cannot_see_all_of_fails_the_build() {
    let scratch = Scratch::new("command-title-invisible");
    scratch.replacing(
        "catalog/commands.toml",
        "title = \"command.journal.verify.title\"",
        "title = \"command.journal.verify\u{202e}.title\"",
    );
    scratch.must_fail_saying("command-title-invisible-character", "journal.verify");
}

#[test]
fn two_commands_bound_to_one_chord_fail_the_build() {
    let scratch = Scratch::new("keybinding");
    scratch.edit("catalog/commands.toml", |text| {
        text.replace(
            "mutating = false",
            "mutating = false\nkeybinding = \"C-c j\"",
        )
        .replace("mutating = true", "mutating = true\nkeybinding = \"C-c j\"")
    });
    scratch.must_fail("duplicate-keybinding", "C-c j");
}

#[test]
fn a_keybinding_the_shell_cannot_parse_fails_the_build() {
    let scratch = Scratch::new("bad-chord");
    scratch.edit("catalog/commands.toml", |text| {
        text.replace("mutating = false", "mutating = false\nkeybinding = \"X-q\"")
    });
    scratch.must_fail("malformed-keybinding", "X-q");
}

// --- log events ---

#[test]
fn a_duplicate_event_field_fails_the_build() {
    let scratch = Scratch::new("duplicate-field");
    scratch.edit("catalog/log-events.toml", |text| {
        text.replace(
            "fields = [{ name = \"attempt\", type = \"u32\" }]",
            "fields = [{ name = \"attempt\", type = \"u32\" }, { name = \"attempt\", type = \"u32\" }]",
        )
    });
    scratch.must_fail("duplicate-field", "attempt");
}

#[test]
fn a_field_type_the_wall_does_not_have_fails_the_build() {
    // The type wall's registry cannot name a dynamic string, and the refusal is
    // at catalog-parse time rather than at compile time — which is earlier and
    // better, and is what the registry's comment now says.
    let scratch = Scratch::new("field-type");
    scratch.edit("catalog/log-events.toml", |text| {
        text.replace(
            "{ name = \"attempt\", type = \"u32\" }",
            "{ name = \"attempt\", type = \"text\" }",
        )
    });
    scratch.must_fail("registry-malformed", "text");
}

// --- whole registries ---

#[test]
fn an_empty_registry_fails_the_build() {
    // Emptying four registries used to leave the catalog step green at "0
    // notification(s), 0 email(s), 0 command(s), 0 log event(s)". §19.3's first
    // rule is that a count of zero is not a pass.
    //
    // **Four of ten, and the other six arrived with no case.** `floor` is called
    // from ten places; this table named the four from the original incident.
    // Measured with the other six replaced by `let _ = …len();`: the whole
    // workspace green. Three of them are the only thing between
    // `catalog/limits.toml` and an empty registry — with the floors gone and
    // every `[[rate_limit]]` deleted, `cargo xtask build` then `cargo xtask
    // check` printed `ok 15 generated file(s) match the catalog` and
    // `pub const RATE_LIMITS: &[RateLimit] = &[];`, so every §15.5 escalation
    // rule quantified over nothing and `docs/limits.md` published an empty table
    // under prose describing curves.
    for (name, file, what) in [
        (
            "empty-notifications",
            "catalog/notifications.toml",
            "notifications",
        ),
        ("empty-emails", "catalog/emails.toml", "emails"),
        ("empty-commands", "catalog/commands.toml", "commands"),
        ("empty-events", "catalog/log-events.toml", "log events"),
        (
            "empty-capabilities",
            "catalog/capabilities.toml",
            "capabilities",
        ),
    ] {
        let scratch = Scratch::new(name);
        scratch.edit(file, |text| {
            text.lines()
                .filter(|line| line.trim_start().starts_with('#') || line.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        });
        scratch.must_fail_saying("empty-registry", what);
    }

    // **The other six, one section at a time.** Emptying a whole file reaches
    // only the first floor the loader gets to, and several of these sections are
    // *required* fields of their file's shape — deleting `[areas]` or `[meta]`
    // outright is `registry-malformed` and never arrives. So each section is
    // emptied in place: its header stays, its entries go, which is the file an
    // author who removed the last entry would leave behind.
    // `container` says whether the header itself is the required field —
    // `[areas]` and `[message]` are, and their headers stay — or whether it is
    // an entry, as `[tables.x]`, `[stores.x]`, `[[rate_limit]]` and `[[error]]`
    // are, in which case it goes with the rest.
    let emptied = |section: &'static str, container: bool| {
        move |text: String| {
            let mut kept: Vec<&str> = Vec::new();
            let mut inside = false;
            for line in text.lines() {
                let trimmed = line.trim_start();
                if trimmed.starts_with('[') {
                    inside = trimmed.starts_with(section);
                    if !inside || container {
                        kept.push(line);
                    }
                    continue;
                }
                if !inside {
                    kept.push(line);
                }
            }
            kept.join("\n")
        }
    };
    for (name, file, section, container, what) in [
        (
            "empty-tables",
            "catalog/limits.toml",
            "[tables.",
            false,
            "tables",
        ),
        (
            "empty-stores",
            "catalog/limits.toml",
            "[stores.",
            false,
            "stores",
        ),
        (
            "empty-rate-limits",
            "catalog/limits.toml",
            "[[rate_limit]]",
            false,
            "rate limits",
        ),
        (
            "empty-errors",
            "catalog/errors.toml",
            "[[error]]",
            false,
            "error codes",
        ),
        (
            "empty-areas",
            "catalog/errors.toml",
            "[areas]",
            true,
            "areas",
        ),
        (
            "empty-locale",
            "catalog/i18n/en.toml",
            "[message]",
            true,
            "messages",
        ),
    ] {
        let scratch = Scratch::new(name);
        scratch.edit(file, emptied(section, container));
        scratch.must_fail_saying("empty-registry", what);
    }
}

#[test]
fn a_missing_registry_fails_the_build() {
    // **Every registry, each naming its own path.** This removed one file and
    // asserted the message said `"read"` — which is a word of the template
    // (`"could not be read: {error}"`) and therefore true of the diagnostic
    // whatever path it was handed. `load::read` is given `relative` and builds
    // `Problem::in_file(relative, …)`; replacing that argument with any
    // hard-coded path — so that a developer who deleted `catalog/limits.toml`
    // was sent to look at `catalog/errors.toml` — left this test green, under a
    // module header promising "the message names the offending key, code or
    // table" and against criterion 6, which is *the error names it*.
    //
    // One fixture cannot catch that: a constant satisfies the one path it
    // happens to equal. Six do, because a constant can be at most one of them.
    for relative in [
        "catalog/errors.toml",
        "catalog/notifications.toml",
        "catalog/emails.toml",
        "catalog/limits.toml",
        "catalog/commands.toml",
        "catalog/log-events.toml",
        "catalog/capabilities.toml",
    ] {
        let leaf = relative
            .rsplit('/')
            .next()
            .expect("a path with a file name");
        let scratch = Scratch::new(&format!("missing-{leaf}"));
        std::fs::remove_file(scratch.root.join(relative)).expect("removable");
        // The path, not a word of the sentence around it.
        scratch.must_fail("registry-unreadable", relative);
    }
}

#[test]
fn a_malformed_registry_produces_a_located_error() {
    // `NOCTUA-PLAN.md` Stage 10 asks for exactly this and it had no test.
    let scratch = Scratch::new("malformed");
    scratch.edit("catalog/errors.toml", |mut text| {
        text.push_str("\n[[error]\ncode = \"NC-DB-0005\"\n");
        text
    });
    let problems = scratch.problems();
    let malformed = problems
        .iter()
        .find(|problem| problem.check == "registry-malformed")
        .expect("a malformed registry is a problem");
    assert!(
        malformed.line.is_some(),
        "a syntax error has a line and the diagnostic did not give one: {malformed}"
    );
}

#[test]
fn a_duplicate_name_in_every_registry_that_has_one_fails_the_build() {
    // Four sibling `duplicate-*` checks were tested and four were not, which is
    // how a family of checks loses half its coverage without anyone noticing.
    for (name, file, addition, check, naming) in [
        (
            "dup-notification",
            "catalog/notifications.toml",
            "\n[[notification]]\nid = \"catalog.mechanism\"\ntitle = \"notification.catalog.mechanism.title\"\n\
             body = \"notification.catalog.mechanism.body\"\nparams = [\"name\"]\nseverity = \"info\"\n",
            "duplicate-notification",
            "catalog.mechanism",
        ),
        (
            "dup-email",
            "catalog/emails.toml",
            "\n[[email]]\nid = \"catalog.mechanism\"\nsubject = \"email.catalog.mechanism.subject\"\n\
             body = \"email.catalog.mechanism.body\"\nparams = []\n",
            "duplicate-email",
            "catalog.mechanism",
        ),
        (
            "dup-command",
            "catalog/commands.toml",
            "\n[[command]]\nname = \"journal.verify\"\ntitle = \"command.journal.verify.title\"\n\
             description = \"command.journal.verify.description\"\naction = \"Read\"\n\
             resource_selector = \"realm\"\nreversibility = \"read\"\nmutating = false\n\
             args_schema = \"\"\"\n{\"type\":\"object\"}\"\"\"\n",
            "duplicate-command",
            "journal.verify",
        ),
        (
            "dup-event",
            "catalog/log-events.toml",
            "\n[[event]]\nname = \"journal.appended\"\ndescription = \"A second claim\"\n\
             level = \"debug\"\nfields = []\n",
            "duplicate-event",
            "journal.appended",
        ),
        (
            "dup-capability",
            "catalog/capabilities.toml",
            "\n[[capability]]\nname = \"net\"\ninterface = \"net\"\nfunctions = []\n\
             description = \"capability.ui.description\"\n",
            "duplicate-capability",
            "`net` is declared twice",
        ),
    ] {
        let scratch = Scratch::new(name);
        scratch.edit(file, |mut text| {
            text.push_str(addition);
            text
        });
        scratch.must_fail(check, naming);
    }
}

#[test]
fn a_command_that_mutates_and_says_it_does_not_fails_the_build() {
    let scratch = Scratch::new("mutation-undeclared");
    scratch.edit("catalog/commands.toml", |text| {
        text.replace(
            "reversibility = \"write\"\nmutating = true",
            "reversibility = \"write\"\nmutating = false",
        )
    });
    scratch.must_fail("mutation-undeclared", "limits.prune");
}

#[test]
fn a_command_whose_schema_is_not_an_object_fails_the_build() {
    // **Both arms, because `schema-not-an-object` is pushed from two.**
    // `{"type":"array"}` *is* a JSON object and reaches only the first — the one
    // that reads the `type` key — so the second, for a schema that is not an
    // object at all, was the fourth instance of the shape
    // `the_second_call_site_of_each_shared_check_fires_too` exists for. Removing
    // it left the whole workspace green while `[]`, `"hello"` and `42` all
    // compiled through as argument schemas, and `commands.toml`'s own fix text
    // says what that costs: this is what exposes the command to an AI agent, so
    // a malformed one silently removes the tool.
    for shape in [r#"{"type":"array"}"#, "[]", r#""hello""#, "42", "null"] {
        let scratch = Scratch::new(&format!("schema-shape-{}", shape.len()));
        scratch.edit("catalog/commands.toml", |text| {
            text.replace(
                r#"{"type":"object","properties":{"from":{"type":"integer","minimum":0}},"additionalProperties":false}"#,
                shape,
            )
        });
        scratch.must_fail("schema-not-an-object", "journal.verify");
    }
}

#[test]
fn a_budget_with_no_curve_or_no_attempts_fails_the_build() {
    // Each case names the surface it breaks. It used to pass `""` as the name,
    // and `what.contains("")` is true — so half of `must_fail` was switched off
    // for these four, and a diagnostic saying only "something is wrong" would
    // have satisfied all of them while the module header promised every message
    // names the offending key.
    for (name, from, to, check, naming) in [
        (
            "no-curve",
            "delay_ms = [0, 1_000, 5_000, 30_000, 120_000]",
            "delay_ms = []",
            "no-delay-curve",
            "device_pin",
        ),
        (
            "no-attempts",
            "max_attempts = 5, delay_ms = [0, 1_000, 5_000, 30_000, 120_000]",
            "max_attempts = 0, delay_ms = [0, 1_000, 5_000, 30_000, 120_000]",
            "zero-attempts",
            "device_pin",
        ),
        (
            "early-lockout",
            "max_attempts = 5, delay_ms = [0, 1_000, 5_000, 30_000, 120_000], lockout_after = 8",
            "max_attempts = 5, delay_ms = [0, 1_000, 5_000, 30_000, 120_000], lockout_after = 2",
            "lockout-before-limit",
            "device_pin",
        ),
        (
            "free-curve",
            "delay_ms = [0, 0, 250, 500, 1_000, 2_000, 5_000]",
            "delay_ms = [0, 0, 0]",
            "curve-ends-free",
            "login",
        ),
    ] {
        let scratch = Scratch::new(name);
        scratch.edit("catalog/limits.toml", |text| text.replace(from, to));
        scratch.must_fail(check, naming);
    }
}

#[test]
fn a_ledger_that_is_not_a_ledger_fails_the_build() {
    let scratch = Scratch::new("ledger-malformed");
    scratch.edit("catalog/errors.reserved.toml", |mut text| {
        text.push_str("\nthis is not toml [[\n");
        text
    });
    scratch.must_fail("ledger-malformed", "catalog/errors.reserved.toml");
}

#[test]
fn a_missing_locale_directory_fails_the_build() {
    let scratch = Scratch::new("no-locales");
    std::fs::remove_dir_all(scratch.root.join("catalog/i18n")).expect("removable");
    scratch.must_fail("locales-unreadable", "catalog/i18n/");
}

#[test]
fn a_single_locale_fails_the_build() {
    let scratch = Scratch::new("one-locale");
    std::fs::remove_file(scratch.root.join("catalog/i18n/pt-BR.toml")).expect("removable");
    scratch.must_fail("too-few-locales", "catalog/i18n/");
}

#[test]
fn removing_the_source_locale_fails_the_build() {
    let scratch = Scratch::new("no-source");
    std::fs::remove_file(scratch.root.join("catalog/i18n/en.toml")).expect("removable");
    scratch.must_fail("no-source-locale", "catalog/i18n/");
}

/// Every control character a registry can hold is escaped on its way into Rust.
///
/// **The arm that does it was reached by no test.** Five control characters were
/// handled by name and the rest went through as themselves: a NUL written as a
/// `\u0000` escape in `catalog/i18n/` reached the generated table as a raw NUL
/// byte, and a `\u0007` as a raw BEL. rustc accepts both, so it was never a
/// build break — it was a NUL in a sentence shown to a user and in the Markdown
/// page beside it. The fix has a paragraph recording all of that and no test, so
/// deleting the arm left `cargo xtask check` green over 829 tests, with the raw
/// bytes present in the generated source.
///
/// Asserted on the output bytes rather than on the escape's spelling: what must
/// not happen is a control character *reaching* the generated file, and there is
/// more than one correct way to write one out.
#[test]
fn no_control_character_reaches_the_generated_source() {
    // Every control character C0 and C1 hold, swept rather than listed —
    // `is_control()` is the rule, and a list here would be the exclusion list
    // this repository keeps removing.
    let mut checked = 0_usize;
    for point in 0..=0x9F_u32 {
        let Some(character) = char::from_u32(point) else {
            continue;
        };
        if !character.is_control() {
            continue;
        }
        checked += 1;
        let written = nc_catalog_compiler::quoted(&format!("before{character}after"));
        assert!(
            !written.chars().any(char::is_control),
            "`U+{point:04X}` reached the generated source as itself: {written:?}"
        );
        // And it is still the same sentence: an escaper that dropped the
        // character would satisfy the assertion above by removing it.
        assert!(
            written.starts_with("\"before") && written.ends_with("after\""),
            "`U+{point:04X}` was not written through at all: {written:?}"
        );
    }
    // The floor: a sweep that found nothing to check would pass by not looping.
    assert!(
        checked > 60,
        "only {checked} control character(s) were checked, so this proves little"
    );

    // The floor in the other direction: ordinary text is unchanged, and the two
    // characters that would break out of a Rust string are still escaped — so
    // the sweep is about control characters and not about an escaper that
    // rewrites everything or one that has stopped escaping anything.
    assert_eq!(nc_catalog_compiler::quoted("plain text"), "\"plain text\"");
    assert_eq!(
        nc_catalog_compiler::quoted("a \"quote\" and a \\ slash"),
        "\"a \\\"quote\\\" and a \\\\ slash\""
    );
}

/// The same three checks, at the call sites their existing cases do not reach.
///
/// **A check name with two call sites is one call site tested.** Each of
/// `empty-description`, `name-not-an-identifier` and `name-collision` is pushed
/// from two places in `load.rs`, and the cases beside this one break the first
/// of each: the errors registry, `notifications.toml`, and a duplicated event
/// name. The second of each fired in no test at all, so all three could be
/// deleted with `cargo xtask check` green over 829 tests.
///
/// The shared name is what made the gap invisible to review — the string is in
/// the test file, so the guard looks tested. What each of these lets through:
/// a log event with an empty description, a log-event field whose snake-case
/// form is not an identifier, and two locales whose BCP-47 tags camel-case to
/// one enum variant.
#[test]
fn the_second_call_site_of_each_shared_check_fires_too() {
    // A log event with no description — the registry's own case breaks an error
    // code's.
    let scratch = Scratch::new("event-no-description");
    scratch.edit("catalog/log-events.toml", |text| {
        text.replace(
            "description = \"An entry was appended to the chain\"",
            "description = \"\"",
        )
    });
    scratch.must_fail("empty-description", "journal.appended");

    // A log-event *field* whose name is not an identifier — the existing case
    // renames a notification id.
    let scratch = Scratch::new("event-field-identifier");
    scratch.edit("catalog/log-events.toml", |text| {
        text.replace("name = \"sequence\"", "name = \"2fa\"")
    });
    scratch.must_fail("name-not-an-identifier", "2fa");

    // Two locales that camel-case to one variant — the existing case collides
    // two event names. A *second* file rather than an edited tag, because
    // editing one file's tag is caught first by `locale-tag-mismatch`: the
    // filename selects the locale at run time and the tag reaches the
    // document's `lang`, so the two disagreeing is its own defect. Here the
    // name and the tag agree and the collision is with the locale beside it.
    let scratch = Scratch::new("locale-collision");
    let source = std::fs::read_to_string(
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
            .join("catalog/i18n/pt-BR.toml"),
    )
    .expect("the source locale is readable");
    scratch.add(
        "catalog/i18n/pt_BR.toml",
        &source.replace("bcp47 = \"pt-BR\"", "bcp47 = \"pt_BR\""),
    );
    scratch.must_fail("name-collision", "PtBR");

    // **`registry-unreadable`'s second site, which said something never true.**
    // `compile` re-reads `catalog/errors.toml` after `load` has already opened
    // it, and on failure it built a *fresh* vector holding one problem —
    // discarding everything `load` found, three lines under a comment promising
    // "every problem found … rather than the first" — with the sentence "it was
    // read a moment ago, so something changed the tree mid-build". `load` opens
    // that file first, so the sentence is false for every input that reaches it:
    // a developer with a missing file was sent to look for a concurrent writer.
    //
    // Both halves are asserted: the wording is `load`'s, and the other problems
    // survive.
    let scratch = Scratch::new("errors-unreadable");
    scratch.edit("catalog/i18n/pt-BR.toml", |text| {
        text.replace(
            "\"error.db.prune-failed\"",
            "\"error.db.prune-failed.moved\"",
        )
    });
    std::fs::remove_file(scratch.root.join("catalog/errors.toml")).expect("removable");
    let problems = scratch.problems();
    assert!(
        problems
            .iter()
            .any(|problem| problem.check == "registry-unreadable"
                && problem.what.contains("could not be read")),
        "the re-read's refusal is not `load`'s: {problems:#?}"
    );
    assert!(
        !problems
            .iter()
            .any(|problem| problem.fix.contains("mid-build")),
        "the refusal still tells a reader something changed the tree while the build ran, \
         which is never true of a file `load` opens first: {problems:#?}"
    );
    // **Exactly one, and it is `load`'s.** The assertion was `len() > 1` under a
    // message about the locale defect surviving — and it passed on a *duplicate*
    // while that defect really was gone: `load` returns the moment a registry
    // cannot be read, so its i18n checks never run. A test that names one thing
    // and is satisfied by another is a test that agrees with the next defect too.
    let said = problems
        .iter()
        .filter(|problem| problem.check == "registry-unreadable")
        .count();
    assert_eq!(
        said, 1,
        "an unreadable registry was reported {said} time(s). `load::read` builds this \
         diagnostic; a second copy from the re-read is the same sentence printed twice: \
         {problems:#?}"
    );
}

/// The characters that are not invisible and still break out of a line.
///
/// Split out because the sweep it belongs to crossed a hundred lines. Takes a
/// counter so a caller can prove it ran rather than trusting that it was called.
fn the_visible_escapes_are_made(checked: &mut usize) {
    // Both escapers grew these arms after measured incidents — `<version>` in a
    // registry description made `RUSTDOCFLAGS=-D warnings` refuse the generated
    // crate with `unclosed HTML tag`, and `Ask <your administrator>` in a locale
    // put raw HTML in a published table — and neither arm was reached by any
    // test: the sweep above passes only control characters and direction marks,
    // and nothing in the shipped catalog holds one of these, so the drift check
    // cannot see them either. Reducing `doc` to `out.push(character)` left the
    // whole workspace green.
    for (character, escaped) in [('<', "&lt;"), ('>', "&gt;")] {
        let line = nc_catalog_compiler::doc(&format!("before{character}after"));
        assert_eq!(
            line,
            format!("/// before{escaped}after\n"),
            "`{character}` reached a doc line as itself, and rustdoc reads it as markup"
        );
    }
    for character in ['[', ']', '\\'] {
        let line = nc_catalog_compiler::doc(&format!("before{character}after"));
        assert_eq!(
            line,
            format!("/// before\\{character}after\n"),
            "`{character}` reached a doc line unescaped, so the next character means \
             something it was not written to mean"
        );
    }
    // `cell` escapes the two that break a table and the one that opens markup;
    // `>` needs no escape once `<` is escaped, and its own comment says so.
    for (character, escaped) in [
        ('<', "&lt;".to_owned()),
        ('|', "\\|".to_owned()),
        ('\\', "\\\\".to_owned()),
    ] {
        let written = nc_catalog_compiler::cell_for_tests(&format!("before{character}after"));
        assert_eq!(
            written,
            format!("before{escaped}after"),
            "`{character}` reached a published table cell as itself"
        );
    }
    *checked += 1;
}

/// A doc line and a Markdown cell carry no character that changes how they read.
///
/// **`quoted`'s repair covered string literals and not this.** The same
/// `\u0000` that `quoted`'s paragraph records reaching the generated table went
/// on reaching every `///` line and every Markdown cell, because `doc` replaced
/// `\n` and `\r` and nothing else. And `is_control()` is category `Cc`, which
/// does not include the bidirectional overrides — those rustc *denies* inside a
/// literal, so a translator pasting one into `catalog/i18n/` produced a
/// workspace that would not compile, pointing at a generated file nobody edited.
///
/// Swept rather than listed, and asserted on the output bytes: what must not
/// happen is such a character *reaching* a generated file.
#[test]
fn no_doc_line_or_cell_carries_an_invisible_character() {
    let mut checked = 0_usize;
    let mut above = 0_usize;
    // **One list, iterated — there used to be three, and they disagreed.** The
    // set was transcribed here a second time and swept over a third set of
    // ranges, and none of the three reached `U+110BD`/`U+110CD`, `U+00AD`,
    // `U+180E`, `U+FFF9`–`U+FFFB` or `U+1D17B`–`U+1D182`: five clauses of
    // `shows_nothing`, fifteen code points, decided by nothing. Measured, all
    // five deleted from `crates/nc-catalog-compiler/src/lib.rs` and this crate
    // stayed green at 48, 90 and 3 passed — and with the Kaithi pair gone, the
    // English sentence plus `U+110BD` filed as the pt-BR translation of
    // `NC-JRNL-0001` gave `check passed` with both language columns identical.
    //
    // **The filter used to be the predicate under test**, so every code point
    // the predicate did not name was enumerated here and never asserted — the
    // range was decoration. Measured: `U+2063 INVISIBLE SEPARATOR` sat inside
    // `0x200B..=0x206F`, was skipped by that line, and let the English sentence
    // be published in the pt-BR column of `NC-JRNL-0001`. There is no filter
    // now, because there is nothing to filter: the ranges *are* the set, so no
    // point can be enumerated and then excused, and none of them holds a
    // character a reader can see. Sweeping a wide range instead is the other
    // error — `U+2010` is a hyphen and has to survive.
    //
    // **And above the basic plane**, which nothing swept for a round: `Cc` tops
    // out at `U+009F` and the direction marks used to top out at `U+FEFF`, so
    // `as_toml`'s `\UXXXXXXXX` arm — the one written for a character that needs
    // eight digits — was unreachable. `U+E0020`–`U+E007F` is the tag block:
    // invisible everywhere and spelling out ASCII, which matters because
    // `commands.toml`'s descriptions are what expose a command to an AI agent.
    for point in nc_catalog_compiler::INVISIBLE_RANGES
        .into_iter()
        .flat_map(|(first, last)| first..=last)
    {
        let Some(character) = char::from_u32(point) else {
            continue;
        };
        checked += 1;
        // **The character itself, not the predicate under test.** Every
        // assertion in this sweep asked whether the output still held anything
        // `is_control() || is_a_direction_mark()` answered for — the same
        // predicate the escapers consult — so shrinking the predicate shrank the
        // expectation with it and the assertion could not fail for any input.
        // Measured, `'\u{202a}'..='\u{202e}'` narrowed to `'\u{202a}'..='\u{202b}'`,
        // which drops POP DIRECTIONAL FORMATTING, LEFT-TO-RIGHT OVERRIDE and
        // RIGHT-TO-LEFT OVERRIDE from all four escapers at once:
        // `check passed`, 1082 tests. Only the astral arm was independent, which
        // is why a plant above the BMP died and every one inside it lived.
        //
        // What is asserted is that the character is not in the output, which is
        // a fact about the character.
        let line = nc_catalog_compiler::doc(&format!("before{character}after"));
        assert!(
            // The doc line's own terminator is not the character under test.
            !line.trim_end_matches('\n').contains(character),
            "`U+{point:04X}` reached a doc line as itself: {line:?}"
        );
        assert!(
            line.starts_with("/// before") && line.trim_end().ends_with("after"),
            "`U+{point:04X}` was not written through at all: {line:?}"
        );
        // And the literal escaper covers the same set, which it did not: the
        // direction marks passed `is_control()` and went through verbatim.
        let written = nc_catalog_compiler::quoted(&format!("before{character}after"));
        assert!(
            !written.chars().any(|held| held == character),
            "`U+{point:04X}` reached a string literal as itself: {written:?}"
        );
        // And a published table cell, which `printable` claims to write and did
        // not: `cell` was not one of its callers, so a control character or a
        // bidi override in a registry value reached every generated page raw
        // while the generated *code* beside it was correctly escaped.
        let published = nc_catalog_compiler::cell_for_tests(&format!("before{character}after"));
        assert!(
            !published.chars().any(|held| held == character),
            "`U+{point:04X}` reached a published table cell as itself: {published:?}"
        );
        // And the ledger's own escaper, whose eight-digit arm this reaches.
        let reserved =
            nc_catalog_compiler::ledger::as_toml_for_tests(&format!("before{character}after"));
        assert!(
            !reserved.chars().any(|held| held == character),
            "`U+{point:04X}` reached the reservation ledger as itself: {reserved:?}"
        );
        if point > 0xFFFF {
            assert!(
                reserved.contains(&format!("\\U{point:08X}")),
                "`U+{point:04X}` is above the basic plane and the ledger wrote \
                 {reserved:?}, which is not the eight-digit form the grammar needs"
            );
            above += 1;
        }
    }
    assert!(
        above > 100,
        "only {above} character(s) above the basic plane were checked, and the \
         eight-digit arm existed for a round with nothing able to reach it"
    );
    assert!(
        checked > 70,
        "only {checked} character(s) were checked, so this proves little"
    );
    // The visible ones, whose escapes are the same rule about the same cells.
    let mut visible = 0_usize;
    the_visible_escapes_are_made(&mut visible);
    assert_eq!(visible, 1, "the visible-escape cases did not run");

    // The floor in the other direction: ordinary prose is unchanged.
    assert_eq!(nc_catalog_compiler::doc("plain text"), "/// plain text\n");
    // Including text in a script that reads right to left, which is not a
    // direction *mark* and must survive — a gate that stripped it would make
    // this repository unable to carry the languages it exists to carry.
    assert_eq!(nc_catalog_compiler::doc("قائمة"), "/// قائمة\n");
}

/// The one list both readers take holds only characters a reader cannot see,
/// and holds all of them.
///
/// **The sweep above cannot ask this.** It iterates `INVISIBLE_RANGES` and
/// `shows_nothing` answers from the same array — which is the repair: the
/// predicate and the expectation were two hand lists that disagreed, and neither
/// reached `U+110BD`/`U+110CD`, `U+00AD`, `U+180E`, `U+FFF9`–`U+FFFB` or
/// `U+1D17B`–`U+1D182`. Measured before the array existed: those five clauses
/// deleted together, fifteen code points, and
/// `cargo test -p nc-catalog-compiler` green at 48, 90 and 3 passed. One array
/// means they can no longer disagree, and it also means a range deleted from it
/// is a range the sweep stops asking about.
///
/// So the array is held from outside itself, by three things none of which is
/// another transcription of it: its own frozen size, `char::is_control` and
/// `is_a_direction_mark` having to be subsets of it, and `char::escape_debug` —
/// std's Unicode tables rather than a list written here — having to decline to
/// print every character in it. That last one is the guard against the opposite
/// error, widening a range until the sweep looks thorough: `U+2010` is a hyphen,
/// std prints it, and this refuses it.
#[test]
fn the_invisible_ranges_hold_only_characters_a_reader_cannot_see() {
    let mut previous: Option<u32> = None;
    for (first, last) in nc_catalog_compiler::INVISIBLE_RANGES {
        assert!(
            first <= last,
            "`U+{first:04X}..=U+{last:04X}` runs backwards"
        );
        if let Some(earlier) = previous {
            assert!(
                earlier < first,
                "`U+{first:04X}` does not come after `U+{earlier:04X}`; this array is \
                 read by eye as well as by code, so it stays ordered and disjoint"
            );
        }
        previous = Some(last);
    }

    // Frozen, because what this guards against is a deletion, and a deletion is
    // exactly what a count sees. Measured: 4244 code points over 25 ranges, every
    // one of them swept above and proven to reach no generated file.
    let counted: u32 = nc_catalog_compiler::INVISIBLE_RANGES
        .into_iter()
        .map(|(first, last)| last - first + 1)
        .sum();
    assert_eq!(
        counted, 4244,
        "the invisible set covers {counted} code point(s) and covered 4244 when each \
         was measured against all four escapers; a character added or removed here \
         gains or loses every escaper at once, and the untranslated-message rule with \
         them"
    );

    let mut held = 0_u32;
    for point in 0..=0x10_FFFF_u32 {
        let Some(character) = char::from_u32(point) else {
            continue;
        };
        // The two narrower predicates are floors under the array: `is_control`
        // is std's own `Cc` table, and the direction marks are what four
        // escapers were built around.
        if character.is_control() || nc_catalog_compiler::is_a_direction_mark(character) {
            assert!(
                nc_catalog_compiler::shows_nothing(character),
                "`U+{point:04X}` is a control character or a direction mark and the \
                 wider predicate no longer holds it, so the array has drifted below \
                 the two narrower readers of it"
            );
        }
        if nc_catalog_compiler::shows_nothing(character) {
            held += 1;
            // `escape_debug` yields the character itself for one std will print,
            // and a backslash for the format, separator, unassigned and
            // combining ones. Nothing this repository calls invisible may be a
            // character std prints.
            assert_ne!(
                character.escape_debug().next(),
                Some(character),
                "`U+{point:04X}` is a character std prints as itself, so a reader sees \
                 it, and the sweep would be asserting that a visible character is \
                 stripped out of every generated file"
            );
        }
    }
    assert_eq!(
        held, counted,
        "the predicate answered for {held} character(s) and the array covers \
         {counted}; they are meant to be the same list read twice"
    );
}

/// The two escapers a page goes through write this character as itself.
///
/// Split out because the sweep that calls it crossed a hundred lines. The code
/// point travels beside the character so a failure names it: the glyph is drawn
/// as nothing, which is the whole reason this set exists, and a message holding
/// only the character would be a message with a hole in it.
fn the_escapers_leave_it_alone(character: char, point: u32) {
    let line = nc_catalog_compiler::doc(&format!("before{character}after"));
    assert!(
        line.contains(character),
        "`U+{point:04X}` was stripped out of a doc line: {line:?}. What the escapers \
         hold is replaced by a space, and these characters stand inside words rather \
         than between them — a set that has reached this one has taken back a question \
         that is not its own, and cuts a word in half doing it"
    );
    let published = nc_catalog_compiler::cell_for_tests(&format!("before{character}after"));
    assert!(
        published.contains(character),
        "`U+{point:04X}` was stripped out of a published cell: {published:?}, and the two \
         sets are two because this one is left in the sentence"
    );
}

/// The blank set is a second set, and nothing that writes a file reads it.
///
/// **One array was answering two questions.** `INVISIBLE_RANGES` decides what
/// four escapers replace with a space, and it also decided whether two sentences
/// are the same sentence — and those are not the same question. Measured before
/// the split, the English sentence plus one `U+3164 HANGUL FILLER` filed as the
/// pt-BR `error.journal.chain-broken`: `Ok, 0 problem(s)`, and `docs/errors.md`
/// publishing that English sentence in the pt-BR column of `NC-JRNL-0001`.
///
/// **And no gate objects to merging them back**, which is why this one exists.
/// Measured, all 27 points inserted into `INVISIBLE_RANGES` in order with its
/// length at 33 and the frozen count below moved to 4271: the whole crate suite
/// passes, `char::escape_debug` declines to print every one of the 27, and
/// `⚠\u{FE0F} the disk is full` comes out of `doc` as `⚠  the disk is full`
/// while `한\u{3164}국` comes out of a cell as `한 국`. So each point is asserted
/// on both sides — that the escapers leave it where it stands, and that the
/// untranslated rule refuses to be told apart by it.
#[test]
fn the_blank_set_is_a_second_set_and_no_escaper_reads_it() {
    let mut previous: Option<u32> = None;
    for (first, last) in nc_catalog_compiler::BLANK_RANGES {
        assert!(
            first <= last,
            "`U+{first:04X}..=U+{last:04X}` runs backwards"
        );
        if let Some(earlier) = previous {
            assert!(
                earlier < first,
                "`U+{first:04X}` does not come after `U+{earlier:04X}`; this array is \
                 read by eye as well as by code, so it stays ordered and disjoint"
            );
        }
        previous = Some(last);
    }
    let counted: u32 = nc_catalog_compiler::BLANK_RANGES
        .into_iter()
        .map(|(first, last)| last - first + 1)
        .sum();
    assert_eq!(
        counted, 27,
        "the blank set covers {counted} code point(s) and covered 27 when each was \
         measured against the untranslated rule and against both escapers a page \
         goes through; a point added or removed here changes which sentences count \
         as the same sentence"
    );

    let sentence = "the activity history does not add up";
    let mut swept = 0_u32;
    for point in nc_catalog_compiler::BLANK_RANGES
        .into_iter()
        .flat_map(|(first, last)| first..=last)
    {
        let Some(character) = char::from_u32(point) else {
            continue;
        };
        swept += 1;
        assert!(
            !nc_catalog_compiler::shows_nothing(character),
            "`U+{point:04X}` is in both sets, so the escapers have taken back the \
             question this array exists to answer on its own"
        );
        assert!(
            !nc_catalog_compiler::tells_two_sentences_apart(character),
            "`U+{point:04X}` is in this array and the predicate says it can make two \
             sentences different ones"
        );
        assert!(
            nc_catalog_compiler::the_same_sentence_for_tests(
                sentence,
                &format!("{sentence}{character}")
            ),
            "a sentence with `U+{point:04X}` appended is the same sentence to a \
             reader and a different one to the untranslated rule, which is how an \
             English sentence reaches a translated column"
        );
        the_escapers_leave_it_alone(character, point);
    }
    assert_eq!(
        swept, counted,
        "{swept} of the {counted} code point(s) in the array were swept"
    );

    // The floor in the other direction, which is the error this array makes
    // easy: widening it until nothing tells two sentences apart. A hyphen, a
    // digit, a letter and an accented letter are all things a reader sees.
    for character in ['x', '\u{2010}', 'ç', '4'] {
        assert!(
            nc_catalog_compiler::tells_two_sentences_apart(character),
            "`{character}` is a character a reader sees and the predicate ignores it"
        );
        assert!(
            !nc_catalog_compiler::the_same_sentence_for_tests(
                sentence,
                &format!("{sentence}{character}")
            ),
            "adding `{character}` left two sentences the same one, so the rule can be \
             satisfied by a change a reader can see"
        );
    }
}

/// An English sentence with a character drawn as nothing added is still English.
///
/// The plant that produced the split, run against the real compiler over a copy
/// of the repository's own catalog: `U+3164` appended to the source sentence and
/// filed as the pt-BR translation of `NC-JRNL-0001` — `critical`, non-retryable,
/// the journal-tampering message. It compiled clean before the repair.
#[test]
fn a_source_sentence_with_a_blank_character_added_fails_the_build() {
    let scratch = Scratch::new("untranslated-blank");
    // The floor under the plant: the shipped catalog is accepted as it stands,
    // so what the assertion below reports is the one character added here and
    // not a rule that has started refusing the registry this repository ships.
    let standing = scratch.problems();
    assert!(
        standing.is_empty(),
        "the catalog was copied and nothing was broken yet, and the build refused \
         it:\n{}",
        render(&standing)
    );
    let key = "\"error.journal.chain-broken\"";
    let english = std::fs::read_to_string(repository().join("catalog/i18n/en.toml"))
        .expect("the source locale is there");
    let line = english
        .lines()
        .find(|line| line.trim_start().starts_with(key))
        .expect("the key is in the source locale")
        .trim_end();
    let planted = format!("{}\u{3164}\"", line.trim_end_matches('"'));
    scratch.edit("catalog/i18n/pt-BR.toml", |text| {
        text.lines()
            .map(|held| {
                if held.trim_start().starts_with(key) {
                    planted.as_str()
                } else {
                    held
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    });
    scratch.must_fail("untranslated", "error.journal.chain-broken");
}

/// The guard removes its directory when it is dropped.
///
/// **What this type does was decided by its name.** `BUILDS_A_SCRATCH` in
/// `xtask/src/check.rs` counts a site as guarded when the body around it names a
/// type from `REMOVES_ITS_SCRATCH_ON_THE_WAY_OUT`, and `Scratch` is on that
/// list — so gutting this `Drop` left the census reporting every site in this
/// file as removed however the test leaves, while one run of this suite left 85
/// directories in the temporary directory. Reported on 2026-09-14, with the
/// whole gate green.
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
