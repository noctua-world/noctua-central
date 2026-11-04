//! The fifth mechanism: a corpus that decides whether the *verdict* is the rule.
//!
//! The four mechanisms `xtask/canaries/README.md` names — a canary, a line
//! floor, a two-counter identity, a fixture at an unpredictable path — all
//! answer "did the walk reach the line?". None of them asks whether the
//! predicate that ran on that line is the one somebody wrote. This is that
//! question, asked the only way it can be: a table of `(input, expected
//! verdict)` pairs driven straight through the shipped entry point, holding for
//! every clause of the predicate one input that clause **alone** decides.
//!
//! Each corpus drives the real function over a scratch tree rather than a
//! re-expressed copy of the judgement, so a weakening anywhere in the
//! composition — the walk's scope, the attribute accumulator, the name list —
//! is inside what the corpus tests.
//!
//! The corpus is the load-bearing half. `xtask/mutants.py` is the meta-gate: it
//! applies one single-token weakening at a time to the predicate and re-runs
//! only these tests; a weakening that leaves them green is a clause the corpus
//! does not decide, and that is a finding about this file.

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::gates::Finding;

    // **The image cases name a tag no machine has pulled.** `every_image_is_pinned`
    // asks docker what a pulled tag resolves to and reports a digest that is not
    // that tag's — a rule about the *referent* of a pin, because an image of
    // PostgreSQL 16.14 can run under the tag `18-alpine` with every printed line
    // of the shape rule identical, measured. These cases are about the *shape*
    // rule beside it, so they use a name that cannot resolve: the referent check
    // then says it could not ask, which is its honest answer, and the shape rule
    // is what the corpus decides.

    /// A scratch tree, removed when the case is over.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// The guard removes its directory when it is dropped.
    ///
    /// **A census decides what this type does by its name.**
    /// [`crate::check::BUILDS_A_SCRATCH`] counts a site as guarded when the body
    /// around it names a type from `REMOVES_ITS_SCRATCH_ON_THE_WAY_OUT`, and
    /// `Scratch` is on that list — so gutting this `Drop` would leave every site
    /// in this file counted as removed however the test leaves while the
    /// directories accumulate. Measured in the policy pack's suite, with such
    /// a `Drop`: 85 left by one run, with the whole gate green.
    ///
    /// A name is not a guarantee. This is.
    #[test]
    fn the_scratch_is_gone_when_the_guard_is() {
        let root = {
            let scratch = Scratch::new();
            let root = scratch.0.clone();
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

    impl Scratch {
        /// A root of this process's own, unique per call.
        fn new() -> Self {
            static NEXT: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
            let nth = NEXT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!("nc-corpus-{}-{nth}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).expect("a writable scratch");
            Self(root)
        }

        fn write(&self, relative: &str, contents: &str) {
            let path = self.0.join(relative);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("a writable scratch");
            std::fs::write(&path, contents).expect("a writable scratch");
        }

        fn root(&self) -> &Path {
            &self.0
        }
    }

    /// How many findings of this kind the run produced.
    fn of_kind(findings: &[Finding], naming: &str) -> usize {
        findings
            .iter()
            .filter(|finding| finding.what.contains(naming))
            .count()
    }

    /// How many findings this file produced.
    fn of_file(findings: &[Finding], named: &str) -> usize {
        findings
            .iter()
            .filter(|finding| finding.file == named)
            .count()
    }

    /// Every case, reported together rather than at the first failure.
    ///
    /// **This is the assertion behind every driver in this file, and it needs a
    /// case of its own.** Without one, `wrong.is_empty()` widened to
    /// `wrong.len() < 100` leaves `cargo xtask check` printing `check passed` — one
    /// token, and the mechanism `xtask/canaries/README.md` calls the answer to
    /// "is the verdict the rule" reports nothing at all. `mutants.py` counts all
    /// its weakenings as `killed` through this function, so that script's tally is
    /// worth exactly what this line is worth.
    ///
    /// `the_corpus_reports_a_case_that_decided_wrongly` is the negative case, in
    /// the shape the gates themselves are held to: hand it a wrong answer and
    /// require it to say so.
    fn verdict(wrong: &[String], predicate: &str) {
        assert!(
            wrong.is_empty(),
            "{} of the corpus for `{predicate}` decided differently from what it is \
             written to decide:\n{}",
            wrong.len(),
            wrong.join("\n")
        );
    }

    /// `claim`'s own copy of the verdict, callable so the case below can drive
    /// it. Without that case it survives the identical weakening.
    fn the_claim_gate_reads(wrong: &[String]) {
        assert!(
            wrong.is_empty(),
            "{} of the corpus for `claim` decided differently from what it is written \
             to decide:\n{}",
            wrong.len(),
            wrong.join("\n")
        );
    }

    /// `caret_uses`' own copy, likewise.
    fn the_caret_rule_reads(wrong: &[String]) {
        assert!(
            wrong.is_empty(),
            "{} of the corpus for `caret_uses` decided differently from what it is \
             written to decide:\n{}",
            wrong.len(),
            wrong.join("\n")
        );
    }

    /// Every digest pin is accounted for, and the arm that says so decides.
    ///
    /// **The sentence is in the census, and the comparison has to be decided
    /// too.** `WHAT_THE_GATE_CAN_SAY` holds the message, so deleting the guard is
    /// caught — and without this, `accounted != pinned` widened to
    /// `accounted > pinned + 100` leaves `cargo test -p xtask` byte-identical to
    /// baseline, measured. A guard that can be weakened without being deleted is one the
    /// verdict census cannot see weakened.
    ///
    /// Both directions: fewer answered than pinned is a pin walked past, and
    /// more is a pin counted twice — a gate that noticed only one notices half.
    #[test]
    fn every_digest_pin_is_answered_for() {
        let skipped = |how_many: usize| -> Vec<crate::gates::Finding> {
            (0..how_many)
                .map(|at| crate::gates::Finding {
                    file: "docker-compose.yml".to_owned(),
                    line: at + 1,
                    // The sentence the gate itself uses, so this fixture does
                    // not put a verdict nobody wrote into the census that reads
                    // every `Finding` constructed in `xtask/src`.
                    what: {
                        // Written with the gate's own placeholder name, so the
                        // literal here *is* the literal there and the verdict
                        // census — which reads every `Finding` constructed in
                        // `xtask/src`, tests included — sees no new sentence.
                        let tag = format!("probe:{at}");
                        format!("`{tag}` is not pulled here, so its digest was not compared")
                    },
                    fix: String::new(),
                })
                .collect()
        };
        for (pinned, compared, wrong, unanswerable, accepted) in [
            (2, 2, 0, 0, true),
            (2, 0, 0, 2, true),
            (3, 1, 1, 1, true),
            (0, 0, 0, 0, true),
            (2, 1, 0, 0, false),
            (2, 0, 0, 0, false),
            (2, 2, 1, 0, false),
            (1, 1, 1, 1, false),
        ] {
            let said = crate::check::every_pin_was_answered_for(
                pinned,
                compared,
                wrong,
                &skipped(unanswerable),
            );
            assert_eq!(
                said.is_ok(),
                accepted,
                "{pinned} pinned, {compared} compared, {wrong} wrong and {unanswerable} \
                 unanswerable was {}: a pin in none of the three was walked past, and a \
                 pin in two of them was counted twice",
                if said.is_ok() { "accepted" } else { "refused" }
            );
            if let Err(problem) = said {
                assert!(
                    problem.contains(&pinned.to_string())
                        && problem.contains(&compared.to_string()),
                    "the refusal says `{problem}` and names neither number"
                );
            }
        }
    }

    /// Every walk that has two counters still asks whether they agree.
    ///
    /// **`the_identity_is_an_identity` decides the function, and this decides
    /// the call sites.** Without it, deleting one — replaced by
    /// `let _ = (entered, judged);` so no count moves and `dead_code` stays
    /// quiet — leaves `cargo test -p xtask` identical, measured, and
    /// `WHAT_THE_GATE_CAN_SAY` cannot see it either, because the sentence lives
    /// in the shared function rather than at the site. A walk that has lost its
    /// identity is a walk that can skip a line in silence, which is the whole
    /// defect one shared function is there to prevent.
    ///
    /// The table is the independent second reader: every walk that carries two
    /// counters is written into it, and one more that grows them writes itself
    /// down here.
    #[test]
    fn every_two_counter_walk_still_asks() {
        // The one this test itself drives is in `corpus.rs`, which is why the
        // expected set names it: a probe is a call site too.
        // **A count per file, not a set of files plus a grand total.** Asserted
        // as a set of files and a total of calls, with `gates.rs` carrying more
        // than one of the calls, deleting one of its calls and duplicating
        // another anywhere keeps both exact. Measured, as one edit in one file:
        // the blanket-allow walk's identity deleted and the §15.1 walk's
        // duplicated gave `corpus:: … ok` and `check passed`. Those two walks
        // are the ones this repository leans on hardest, and the sentence a lost
        // guard would have said lives in the shared function, so the verdict
        // census cannot see the loss either.
        // **The walk, not the file.** A count per file cannot see a call moved
        // *within* one: `gates.rs` carries two, so deleting the blanket-allow
        // walk's identity and duplicating the §15.1 walk's leaves `gates.rs: 2`
        // exact and the first walk with no identity at all. The enclosing
        // function is the walk, and that is what has to be named.
        // `corpus.rs`'s own is inside a test module, so no column-zero `fn`
        // precedes it — the empty name is this probe, and naming it that way is
        // more honest than pretending the scan resolved it.
        const ASKED: [(&str, &str, usize); 10] = [
            ("corpus.rs", "", 1),
            ("determinism.rs", "nothing_the_emitter_reads_changes", 1),
            ("gates.rs", "every_hatch_carries_its_reason", 1),
            ("gates.rs", "no_code_is_written_at_a_call_site", 1),
            ("gates.rs", "no_source_puts_the_lints_down", 1),
            ("local.rs", "no_local_sql_outside_the_migrations", 1),
            ("references.rs", "run", 1),
            ("sections.rs", "every_section_cited_exists", 1),
            ("wall.rs", "compile_fail", 1),
            ("workflow.rs", "the_identity", 1),
        ];
        let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src"));
        // **The needle is assembled**, or this scanner counts itself one call
        // too many: the line holding the literal is a line of code in a file the
        // walk reads.
        let needle = ["everything_reached", "a_verdict("].join("_");
        let mut sites: Vec<(String, String, usize)> = Vec::new();
        for entry in std::fs::read_dir(root).expect("the source directory is readable") {
            let path = entry.expect("a directory entry").path();
            if path.extension().is_none_or(|kind| kind != "rs") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let named = path
                .file_name()
                .expect("a file name")
                .to_string_lossy()
                .into_owned();
            let mut walk = String::new();
            let mut counts: Vec<(String, String, usize)> = Vec::new();
            for line in text.lines() {
                // A function at column zero is a walk; the calls below it are
                // its own until the next one.
                if let Some(rest) = line
                    .strip_prefix("pub(crate) fn ")
                    .or_else(|| line.strip_prefix("fn "))
                    .and_then(|rest| rest.split('(').next())
                {
                    rest.clone_into(&mut walk);
                }
                let code = crate::gates::line_without_a_comment(line);
                // The definition is not a call, and neither is a doc mention.
                if !code.contains(needle.as_str()) || code.contains("pub(crate) fn") {
                    continue;
                }
                match counts
                    .iter_mut()
                    .find(|(file, at, _)| *file == named && *at == walk)
                {
                    Some((_, _, count)) => *count += 1,
                    None => counts.push((named.clone(), walk.clone(), 1)),
                }
            }
            sites.extend(counts);
        }
        sites.sort();
        let expected: Vec<(String, String, usize)> = ASKED
            .iter()
            .map(|(file, walk, count)| ((*file).to_owned(), (*walk).to_owned(), *count))
            .collect();
        assert_eq!(
            sites, expected,
            "the two-counter identity is asked in {sites:?} and the walks that have two \
             counters are {ASKED:?}. A call deleted here is a walk that can skip a line \
             with every printed number whole — and the sentence it would have said lives \
             in the shared function, so the verdict census cannot see the loss either"
        );
    }

    /// The two-counter identity is an identity.
    ///
    /// **Every two-counter walk rests on it, and this hands it two numbers that
    /// disagree.** Written out once per gate instead — inline in `gates.rs` and
    /// `determinism.rs`, and in `sections.rs` and `references.rs` as a
    /// byte-identical `every_citation_reached_a_verdict` — the comparison has to
    /// be widened in every copy to be measured, and widened in every copy it
    /// leaves every gate in this repository able to skip a line silently:
    ///
    /// ```text
    /// if reached != judged   ->   if reached > judged + 100
    ///   check passed
    /// ```
    ///
    /// It is one function, and this is the case that decides it. `equal`
    /// says nothing; either direction says something, because a filter can make
    /// the second counter smaller and a double-count can make it larger, and a
    /// gate that only notices one of those notices half.
    #[test]
    fn the_identity_is_an_identity() {
        for (judged, reached, expected) in [(7, 7, 0), (7, 6, 1), (6, 7, 1), (0, 0, 0)] {
            let mut findings = Vec::new();
            crate::gates::everything_reached_a_verdict(
                "line",
                "probe.rs",
                judged,
                reached,
                &mut findings,
            );
            assert_eq!(
                findings.len(),
                expected,
                "{judged} judged and {reached} that reached a verdict produced \
                 {} finding(s), and the counters are here so a skipped line cannot be \
                 silent",
                findings.len()
            );
            if let Some(finding) = findings.first() {
                assert!(
                    finding.what.contains(&judged.to_string())
                        && finding.what.contains(&reached.to_string()),
                    "the finding says `{}` and names neither number, so a reader cannot \
                     tell which way the walk went wrong",
                    finding.what
                );
            }
        }
    }

    /// The verdict is a verdict: a case that decided wrongly is reported.
    ///
    /// **Measured without it**, as the only change to the tree:
    ///
    /// ```text
    /// assert!(wrong.is_empty(), …)   ->   assert!(wrong.len() < 100, …)
    ///   check passed
    /// ```
    ///
    /// Most drivers rest on `verdict` and two more carry their own copy of the
    /// same assertion — all three are checked here, because without this all
    /// three survive that weakening.
    #[test]
    fn the_corpus_reports_a_case_that_decided_wrongly() {
        let wrong = ["a case that decided wrongly".to_owned()];
        for (what, run) in [
            (
                "verdict",
                Box::new(|| verdict(&wrong, "probe")) as Box<dyn Fn() + std::panic::UnwindSafe>,
            ),
            ("claim", Box::new(|| the_claim_gate_reads(&wrong)) as Box<_>),
            (
                "caret_uses",
                Box::new(|| the_caret_rule_reads(&wrong)) as Box<_>,
            ),
        ] {
            // The hook is silenced so a passing run does not print a backtrace
            // for a panic it asked for.
            let previous = std::panic::take_hook();
            std::panic::set_hook(Box::new(|_| {}));
            let said = std::panic::catch_unwind(run);
            std::panic::set_hook(previous);
            assert!(
                said.is_err(),
                "`{what}` accepted a case that decided wrongly, so every corpus row it \
                 carries is decided by nothing"
            );
        }
    }

    // ---------------------------------------------------------------------------
    // 1. `gates::holds_an_environment`, through `gates::committed_env_files`.
    //
    // What it decides: whether a committed file carries real credentials. A
    // weakening ships a secret in a clone with `no .env is committed` printed over
    // it. Clauses: the `env` part, the two-part module exemption and its
    // known-format tail, and the four template words.
    // ---------------------------------------------------------------------------

    /// Sentences that state a count, and sentences that name something with a number.
    ///
    /// **Every digit form of every countable noun falls through a rule about the
    /// word in front.** Neither that rule — "a numeral with a word in front of
    /// it identifies rather than counts" — nor a list of label words — `stage`,
    /// `page`, `round` — will do. The list lets half of fresh ordinary English
    /// through, and the rule is right about `Stage 10` and wrong about
    /// `there are 9 crates`, while the document stating the rule this gate
    /// enforces gives a count of tests, written in digits after *there are*, as
    /// its own example of the forbidden sentence.
    ///
    /// So the counting side is named instead, and this is the corpus that
    /// decides it: a word missing from `COUNTS_THEM` is a miss, and a word
    /// wrongly in it is a false finding, which `counts.rs`'s own header calls
    /// worse than no gate.
    const COUNTS: [(&str, &str, bool); 25] = [
        // Written out, which is how every count in these documents is written.
        ("There are nine crates in this repository.", "crates", true),
        ("The tree holds twelve migrations.", "migrations", true),
        // And in digits, which is how a person writes one without thinking.
        ("There are 9 crates in this repository.", "crates", true),
        ("The tree holds 12 migrations.", "migrations", true),
        ("This repository ships 6 registries.", "registries", true),
        (
            "The log has 11 compile-fail fixtures today.",
            "compile-fail fixtures",
            true,
        ),
        (
            "The journal has 13 integration tests.",
            "integration tests",
            true,
        ),
        ("There are 41 integration tests.", "integration tests", true),
        ("The core is 7 invariants.", "invariants", true),
        ("The core was 7 invariants.", "invariants", true),
        ("The pack contains 4 registries.", "registries", true),
        ("It carries 3 migrations.", "migrations", true),
        ("The catalog declares 5 registries.", "registries", true),
        ("This lists 8 migrations.", "migrations", true),
        (
            "The suite names 2 integration tests.",
            "integration tests",
            true,
        ),
        ("It adds 3 migrations.", "migrations", true),
        ("All 9 crates build.", "crates", true),
        ("Only 2 migrations remain.", "migrations", true),
        ("Just 4 registries are declared.", "registries", true),
        ("Both 2 crates compile.", "crates", true),
        // A number that identifies rather than counts keeps its label.
        (
            "Stage 10 migrations were written for it.",
            "migrations",
            false,
        ),
        ("See page 12 invariants for the rest.", "invariants", false),
        ("Issue 5 migrations are tracked there.", "migrations", false),
        ("Round 3 crates were reviewed.", "crates", false),
        // A year is a year, not a count of anything.
        (
            "In 2026 migrations became the schema of record.",
            "migrations",
            false,
        ),
    ];

    #[test]
    fn the_stated_count_predicate_decides_every_clause() {
        let mut wrong = Vec::new();
        for (line, noun, expected) in COUNTS {
            let found = !crate::counts::claim_for_tests(line, noun).is_empty();
            if found != expected {
                wrong.push(format!(
                    "  {line:?} — the gate reads it as {}, and it is {}",
                    if found { "a stated count" } else { "prose" },
                    if expected {
                        "a count that can rot"
                    } else {
                        "a number that names something"
                    }
                ));
            }
        }
        the_claim_gate_reads(&wrong);
    }

    /// Where rustc put the caret, against the door a fixture is named for.
    ///
    /// **The compile-fail gate asks where the refusal happened.** A fixture
    /// that names the door somewhere in its code and fails for an unrelated
    /// reason satisfies every other rule — measured without this question,
    /// `Origin`'s two fields went back to `pub &'static str` with
    /// `check passed` and an email address shipped into a log line. The caret's
    /// position is the one thing a fixture does not choose.
    ///
    /// `(the fixture's lines, the line rustc pointed at, is it the door's)`.
    const CARETS: [(&[&str], usize, bool); 94] = [
        // The ordinary case: the caret's own line names the door.
        (&["fn accepts<T: nc_log::LogValue>(_: T) {}"], 1, true),
        // **A fixture's own item does not pay for the door.** A fixture may
        // declare a `trait LogValue` of its own, and then a block opening
        // `impl LogValue for X {` names nothing of `nc-log`'s. `caret_uses`
        // accepts only the qualified `nc_log::` path, so a bare name pays for
        // nothing, and these rows hold that: a rule accepting the bare
        // `LogValue` certifies this fixture.
        //
        // **Nor does a list of openers.** Consulted to *excuse* a bare name, it
        // fails open at every spelling it misses — `let` is one, and there are
        // at least ten more — which is why the rule asks for the path instead.
        (
            &["trait LogValue {}", "impl LogValue for Smuggled {"],
            2,
            false,
        ),
        // **An import does not make the bare name the door's either.** Deciding
        // whether a bare name on a caret line is `nc-log`'s goes wrong in more
        // than one direction, so the rule reads the qualified path, and a
        // fixture written this way is reported as **not proving its door** —
        // loud and actionable, where a wrong guess certifies a forgery in
        // silence.
        (
            &["use nc_log::LogValue;", "impl LogValue for Smuggled {"],
            2,
            false,
        ),
        (&["impl LogValue for Smuggled {"], 1, false),
        // Each of these binds the short name and none of them opens with a
        // keyword a list of openers would hold. Under such a list every one
        // pays for the door; the import question refuses them all without
        // knowing what a binding looks like.
        (
            &["let LogValue = 5;", "impl LogValue for Smuggled {"],
            2,
            false,
        ),
        (
            &[
                "let (LogValue, _) = (5, 6);",
                "impl LogValue for Smuggled {",
            ],
            2,
            false,
        ),
        (
            &["for LogValue in 0..3 {", "impl LogValue for Smuggled {"],
            2,
            false,
        ),
        (
            &["mod LogValue {}", "impl LogValue for Smuggled {"],
            2,
            false,
        ),
        // A glob import and the `pub use` re-export spelling are imports, and
        // neither puts a qualified path on the caret line.
        (
            &["use nc_log::*;", "impl LogValue for Smuggled {"],
            2,
            false,
        ),
        (
            &["pub use nc_log::LogValue;", "impl LogValue for Smuggled {"],
            2,
            false,
        ),
        // An import of something else from the same crate does not make this
        // bare name the door's.
        (
            &["use nc_log::Field;", "impl LogValue for Smuggled {"],
            2,
            false,
        ),
        // A group import binds every name in it, and the bare name it binds is
        // still not what this rule reads.
        (
            &[
                "use nc_log::{LogValue, Field};",
                "impl LogValue for Smuggled {",
            ],
            2,
            false,
        ),
        // **An alias binds another name.** The bare `LogValue` is then free for
        // the fixture's own item, which is the forgery this row is written
        // against — measured against a rule that asks only for an import, which
        // certifies it.
        (
            &[
                "use nc_log::LogValue as Other;",
                "impl LogValue for Smuggled {",
            ],
            2,
            false,
        ),
        // **A `use` inside a string literal is not an import.** Read through
        // `without_comment`, which copies a literal's contents through, a loose
        // ` use ` split takes `pub const NOTE: &str = "we` as a visibility — so
        // one const would set the bare-name door for the whole file.
        (
            &[
                "pub const NOTE: &str = \"we use nc_log::LogValue here\";",
                "impl LogValue for Smuggled {",
            ],
            2,
            false,
        ),
        // **Imported and shadowed.** Rust resolves the bare name to the
        // fixture's own item, so the caret is about that and not about the
        // door. This is the one arrangement the import question cannot decide
        // alone, and the reason the local-declaration question is asked beside
        // it rather than instead of it.
        (
            &[
                "use nc_log::LogValue;",
                "trait LogValue {}",
                "impl LogValue for Smuggled {",
            ],
            3,
            false,
        ),
        // A commented-out import is not an import — the same rule `names` holds
        // one line lower.
        (
            &["// use nc_log::LogValue;", "impl LogValue for Smuggled {"],
            2,
            false,
        ),
        // **A comment is not code.** A rule that strips `//!` and nothing else
        // lets one ordinary `//` line naming the door pay for it — measured, a
        // forgery about nothing the door guards passed the whole gate, and
        // deleting that single line turned it red.
        (
            &[
                "fn main() {",
                "// nc_log::LogValue",
                "    takes_one(\"x\");",
                "}",
            ],
            3,
            false,
        ),
        // Imported and written short. This is the one arrangement the bare
        // spelling exists for, and it is refused: the fixture is asked to write
        // the path rather than certified on a name no rule over the bare
        // spelling decides.
        (
            &["use nc_log::LogValue;", "fn accepts<T: LogValue>(_: T) {}"],
            2,
            false,
        ),
        // Inside an `impl` of the door: the trait's own `const` is what refuses.
        (
            &[
                "impl nc_log::LogValue for Mine {",
                "    const TEXT: &'static str = String::new().leak();",
                "}",
            ],
            2,
            true,
        ),
        // Nested one level deeper, still inside it.
        (
            &[
                "impl nc_log::LogValue for Mine {",
                "    fn to_field(&self) -> Field {",
                "        String::new().leak()",
                "    }",
                "}",
            ],
            3,
            true,
        ),
        // **A dead type annotation on an initialiser block.** `let held: …= {`
        // is a measured attack; `const HELD: … = {` and `static HELD: … = {` are
        // the same attack under a keyword `AN_ITEM_OPENS_WITH` excuses. A rule
        // that lets the keyword excuse the line certifies one carried all the
        // way to a shipped fixture, measured: the door named only in the dead
        // annotation, the refusal `Literal`'s, and the gate certifying it with
        // `Variant`'s field made public.
        (
            &[
                "const HELD: Option<nc_log::LogValue> = {",
                "    takes_one(\"plain\")",
                "};",
            ],
            2,
            false,
        ),
        (
            &[
                "static HELD: Option<nc_log::LogValue> = {",
                "    takes_one(\"plain\")",
                "};",
            ],
            2,
            false,
        ),
        // **The same forgery, five more ways.** Each of these walks past a rule
        // that tests for `= {`, measured: the brace is not the question, and each
        // answer to it is one spelling.
        (
            &[
                "const HELD: Option<nc_log::LogValue> = vec![",
                "    takes_one(\"plain\")",
                "];",
            ],
            2,
            false,
        ),
        (
            &[
                "let held: Option<nc_log::LogValue> = [",
                "    takes_one(\"plain\")",
                "];",
            ],
            2,
            false,
        ),
        (
            &[
                "const HELD: Option<nc_log::LogValue> = match text {",
                "    _ => takes_one(\"plain\"),",
                "};",
            ],
            2,
            false,
        ),
        (
            &[
                "static HELD: Option<nc_log::LogValue> = if flag {",
                "    takes_one(\"plain\")",
                "} else {",
                "    None",
                "};",
            ],
            2,
            false,
        ),
        (
            &[
                "const HELD: Option<nc_log::LogValue> = Some(Wrapper {",
                "    inner: takes_one(\"plain\"),",
                "});",
            ],
            2,
            false,
        ),
        // **A redundant annotation on a struct literal declares the door.**
        // Asked as *"is any occurrence inside the annotation"*, this is
        // silenced — and a shipped fixture rewritten this way is reported as
        // not proving its door, measured. The too-strong half.
        (
            &[
                "let origin: nc_log::LogValue = nc_log::LogValue {",
                "    field: takes_one(\"plain\"),",
                "};",
            ],
            2,
            true,
        ),
        // **And a dead type argument, which is a dead mention with no colon of
        // its own.** `::<` is two colons, which the annotation rule steps over
        // by construction: the sixth spelling of one forgery.
        (
            &[
                "let held = foo::<nc_log::LogValue>({",
                "    takes_one(\"plain\")",
                "});",
            ],
            2,
            false,
        ),
        (
            &[
                "for v in Vec::<nc_log::LogValue>::new() {",
                "    takes_one(\"plain\");",
                "}",
            ],
            2,
            false,
        ),
        // **And the control that makes `=` alone the wrong rule.** A default
        // type parameter puts an `=` before the brace of a genuine item header,
        // and the door in it is what the line declares.
        (
            &["struct S<T = nc_log::LogValue> {", "    field: T,", "}"],
            2,
            true,
        ),
        // And the `let` spelling itself, which decides that the step-over runs
        // at all: with the whole predicate off, the annotation above pays.
        (
            &[
                "let held: Option<nc_log::LogValue> = {",
                "    takes_one(\"plain\")",
                "};",
            ],
            2,
            false,
        ),
        // A dead mention as a *sibling* of the real error, which is the measured
        // attack: the enclosing block is `fn main`, and it names no door.
        (
            &[
                "fn main() {",
                "    let _: Option<nc_log::LogValue> = None;",
                "    takes_one(\"plain\");",
                "}",
            ],
            3,
            false,
        ),
        // An unrelated item at column zero, naming no door.
        (&["const ELSEWHERE: &str = String::new().leak();"], 1, false),
        // The door named in the `//!` header, which is prose the fixture wrote.
        (&["//! EXPECT: nc_log::LogValue", "fn main() {}"], 1, false),
        // A caret past the end of the file decides nothing.
        (&["fn main() {}"], 9, false),
        // **A bound's colon is not an annotation's.** Opening the annotation
        // region at the first single `:` with no regard for `<…>` depth turns
        // the corpus's own canonical "declares the door" shape into a mention
        // with one generic parameter. Measured on all four shapes a colon takes
        // in an item header.
        (
            &[
                "impl<T: Clone> nc_log::LogValue for Mine<T> {",
                "    const TEXT: &'static str = String::new().leak();",
                "}",
            ],
            2,
            true,
        ),
        (
            &[
                "pub trait Mine: nc_log::LogValue {",
                "    const TEXT: &'static str = String::new().leak();",
                "}",
            ],
            2,
            true,
        ),
        (
            &[
                "impl<'a: 'static> nc_log::LogValue for Ref<'a> {",
                "    const TEXT: &'static str = String::new().leak();",
                "}",
            ],
            2,
            true,
        ),
        (
            &[
                "impl Mine where Mine: nc_log::LogValue {",
                "    const TEXT: &'static str = String::new().leak();",
                "}",
            ],
            2,
            true,
        ),
        // **One space, and the turbofish is gone.** Rust lexes `::` and `<` as
        // separate tokens. The sibling corpus one function away carries rows for
        // this hazard, and this copy carries them too.
        (
            &[
                "let held = foo:: <nc_log::LogValue>({",
                "    takes_one(\"plain\")",
                "});",
            ],
            2,
            false,
        ),
        // **A return type is a mention, whether or not a parameter carries a
        // colon.** A rule that reads it as one only after a parameter's colon
        // gives these the opposite answer to the same shape one typed parameter
        // apart.
        (
            &[
                "let held = |_x| -> nc_log::LogValue {",
                "    takes_one(\"plain\")",
                "};",
            ],
            2,
            false,
        ),
        (
            &[
                "fn make() -> nc_log::LogValue {",
                "    takes_one(\"plain\")",
                "}",
            ],
            2,
            false,
        ),
        // **Two arrows can drive a depth counter below zero**, so that an `=`
        // inside a pair of angle brackets reads as the end of the annotation and
        // everything after it, the door included, reads alive.
        (
            &[
                "let held: fn() -> fn() -> Box<dyn Iterator<Item = nc_log::LogValue>> = {",
                "    takes_one(\"plain\")",
                "};",
            ],
            2,
            false,
        ),
        // **A door in a string literal, and a door handed to a macro.** Neither
        // is what the line declares, and a walk that spends comments and
        // nothing else reads both as declarations.
        (
            &[
                "let held = vec![\"nc_log::LogValue\", {",
                "    takes_one(\"plain\")",
                "}];",
            ],
            2,
            false,
        ),
        (
            &[
                "let held = only_the_first!(nc_log::LogValue, {",
                "    takes_one(\"plain\")",
                "});",
            ],
            2,
            false,
        ),
        // **A number with two sources.** A predicate comparing a byte offset
        // from `match_indices` against character offsets from `enumerate` is
        // wrong here: nineteen non-ASCII characters before the door are enough
        // for the byte offset to overrun the annotation's end and read alive.
        // The ASCII form of the same annotation is pinned above.
        (
            &[
                "let held: \u{3a9}\u{3a9}\u{3a9}\u{3a9}\u{3a9}\u{3a9}\u{3a9}\u{3a9}\u{3a9}\u{3a9}\u{3a9}\u{3a9}\u{3a9}\u{3a9}\u{3a9}\u{3a9}\u{3a9}\u{3a9}\u{3a9}<nc_log::LogValue> = {",
                "    takes_one(\"plain\")",
                "};",
            ],
            2,
            false,
        ),
        // **A bound is a mention, and the exception list reads the header's
        // word, not any word on the line.** Five of these six are measured
        // undecided without these rows:
        // `impl<T: …>` for the depth test, the wrapped `where` line for the
        // word that excuses it, the arrow inside a turbofish for `closing`'s
        // own guard, and the parameter list for the bracket arm that ends a
        // type at its own `)` — which is also the only row that asks whether
        // the walk counts parentheses at all.
        (
            &[
                "impl<T: nc_log::LogValue> Wrapper<T> {",
                "    takes_one(\"plain\")",
                "}",
            ],
            2,
            false,
        ),
        (
            &[
                "where T: nc_log::LogValue {",
                "    takes_one(\"plain\")",
                "}",
            ],
            2,
            true,
        ),
        (
            &[
                "let held = foo::<fn() -> nc_log::LogValue>({",
                "    takes_one(\"plain\")",
                "});",
            ],
            2,
            false,
        ),
        (
            &[
                "fn helper(_: u8) { nc_log::LogValue {",
                "    takes_one(\"plain\")",
                "} }",
            ],
            2,
            true,
        ),
        // **A word before the colon is not the header's word.** `r#trait`
        // splits into `r` and `trait`, and an enclosing item written on the
        // same line puts a real `impl` in front of an ordinary binding — both
        // measured turning a rule that reads any word before the colon off for
        // the line, with the dead annotation forgeries above walking through
        // after one token.
        (
            &[
                "let r#trait: Option<nc_log::LogValue> = {",
                "    takes_one(\"plain\")",
                "};",
            ],
            2,
            false,
        ),
        (
            &[
                "impl Smuggled { const HELD: nc_log::LogValue = {",
                "    takes_one(\"plain\")",
                "}; }",
            ],
            2,
            false,
        ),
        // **Three more, each written so that one clause decides it.** A
        // bound on an item header the exception list *does* excuse, so the
        // depth test is what makes it a mention; a binding whose name merely
        // begins with one of the excused words; and an item whose brace is
        // already open before the colon, which is a statement inside it.
        (
            &[
                "trait Wrap<T: nc_log::LogValue> {",
                "    takes_one(\"plain\")",
                "}",
            ],
            2,
            false,
        ),
        (
            &[
                "let wherever: Option<nc_log::LogValue> = {",
                "    takes_one(\"plain\")",
                "};",
            ],
            2,
            false,
        ),
        (
            &[
                "trait Smuggled { const HELD: nc_log::LogValue = {",
                "    takes_one(\"plain\")",
                "}; }",
            ],
            2,
            false,
        ),
        // **Three clauses a deletion sweep finds undecided without these.** The
        // first shows a line can reach `opens_a_type`'s parenthesis half; the
        // second is the only shape that asks whether the walk lowers its angle
        // counter after a generic parameter list; the third is the two-level
        // sibling forgery `caret_uses`' own header names as the attack the
        // indentation chain exists to stop, held here by that chain rather than
        // by the annotation rule.
        (
            &[
                "impl Mine where fn(held: nc_log::LogValue): Copy {",
                "    const TEXT: &'static str = String::new().leak();",
                "}",
            ],
            2,
            false,
        ),
        (
            &[
                "pub trait Mine<T>: nc_log::LogValue {",
                "    const TEXT: &'static str = String::new().leak();",
                "}",
            ],
            2,
            true,
        ),
        (
            &[
                "fn main() {",
                "    let _ = nc_log::LogValue::default();",
                "    if flag {",
                "        takes_one(\"plain\");",
                "    }",
                "}",
            ],
            4,
            false,
        ),
        // **Two clauses that look unreachable and are not.** A bounded
        // parameter before a default one reaches `the_type_ends`' comma arm; a
        // macro token tree holding a bare `where` reaches the exception list,
        // which a match against the raw line gets wrong.
        (
            &[
                "struct S<A: Clone, B = nc_log::LogValue> {",
                "    field: B,",
                "}",
            ],
            2,
            true,
        ),
        (
            &[
                "m!( where ); const HELD: Option<nc_log::LogValue> = {",
                "    takes_one(\"plain\")",
                "};",
            ],
            2,
            false,
        ),
        // **And a third clause that looks unreachable.** A struct
        // literal's field colon opens a type by this rule's own reckoning, so
        // an `==` in the value ends the region four characters in and the door
        // after it is never spent.
        (
            &[
                "let held = Wrapper { seen: FLAG == nc_log::LogValue::Named, inner: {",
                "    takes_one(\"plain\")",
                "} };",
            ],
            2,
            false,
        ),
        // **A guard of four characters lets the fifth walk through.** The `=`
        // of a match arm and the `=` of `|=` both end the annotation region
        // under such a guard, and the door written after either is never spent
        // — the dead-mention forgery above, one operator on. The
        // third row is the one no character rule can reach: an ordinary `=`
        // inside a nested block, where nothing about the `=` is wrong at all.
        (
            &[
                "let held = Wrapper { seen: match flag { _ => nc_log::LogValue::Named }, inner: {",
                "    takes_one(\"plain\")",
                "} };",
            ],
            2,
            false,
        ),
        (
            &[
                "let held = Wrapper { inner: { flags |= nc_log::LogValue::MASK; 0 }, seen: {",
                "    takes_one(\"plain\")",
                "} };",
            ],
            2,
            false,
        ),
        (
            &[
                "let held = Wrapper { inner: { flags = nc_log::LogValue::MASK; 0 }, seen: {",
                "    takes_one(\"plain\")",
                "} };",
            ],
            2,
            false,
        ),
        // **A const generic argument is a brace inside a type.** An exception
        // list asking whether a `{` sits anywhere between the excusing word and
        // the colon lets one `Wrapper<{ SIZE }>` disqualify a genuine `where`,
        // and the predicate declaring the door is then reported as proving
        // nothing — the row above it, one const argument apart, is `true`. The second
        // row is the too-weak half a balanced-brace rule would leave: a trait
        // body that opens and closes before an ordinary binding's colon
        // balances to zero and turns the whole rule off.
        (
            &[
                "impl Mine where Wrapper<{ SIZE }>: nc_log::LogValue {",
                "    const TEXT: &'static str = String::new().leak();",
                "}",
            ],
            2,
            true,
        ),
        (
            &[
                "trait A { } const HELD: Option<nc_log::LogValue> = {",
                "    takes_one(\"plain\")",
                "};",
            ],
            2,
            false,
        ),
        // **And the brace that closes a block the region never opened.** The
        // annotation on `seen` stops where the struct literal does; without
        // that the region swallows the rest of the line and the declaration
        // written after it is refused, which is the too-strong direction.
        (
            &[
                "let held = Wrapper { seen: match flag { _ => 0 } }; nc_log::LogValue {",
                "    takes_one(\"plain\")",
                "} };",
            ],
            2,
            true,
        ),
        // **Five rows, each written for one clause**, because a clause nothing
        // can decide is a clause that is not there. The first three are
        // `a_block_opens_in`'s bracket counters: a brace inside `[…]` is an
        // array length, a brace after an `->` inside `<…>` is still inside
        // them, and a stray `>` that drives the angle count negative puts a
        // genuine const argument at depth zero. The last two are
        // `the_type_ends`' block depth, which the three rows above reach only
        // through a *nested* brace: a compound assignment and a comma written
        // in a struct literal's own field value reach it at depth zero.
        (
            &[
                "impl Mine where [(); { SIZE }]: nc_log::LogValue {",
                "    const TEXT: &'static str = String::new().leak();",
                "}",
            ],
            2,
            true,
        ),
        (
            &[
                "impl Mine where Wrapper<fn() -> [u8; 1], { SIZE }>: nc_log::LogValue {",
                "    const TEXT: &'static str = String::new().leak();",
                "}",
            ],
            2,
            true,
        ),
        (
            &[
                "impl Mine where [(); (N > 0) as usize]: Copy, Wrapper<{ SIZE }>: nc_log::LogValue {",
                "    const TEXT: &'static str = String::new().leak();",
                "}",
            ],
            2,
            true,
        ),
        (
            &[
                "let held = Wrapper { seen: flags |= nc_log::LogValue::MASK, inner: {",
                "    takes_one(\"plain\")",
                "} };",
            ],
            2,
            false,
        ),
        (
            &[
                "let held = Wrapper { inner: { first, nc_log::LogValue::MASK }, seen: {",
                "    takes_one(\"plain\")",
                "} };",
            ],
            2,
            false,
        ),
        // **An arrow inside a generic argument list is not the line's return
        // type.** A fill that runs to the next brace with no bracket depth lets
        // one arrow turn a genuine `where` predicate into a mention: the first
        // two rows are one arrow apart, and such a fill decides them opposite
        // ways. The last two are the too-strong direction, which a fill that
        // simply stops at the first `>` breaks — a return type whose own type
        // holds the door, and a bound whose argument does.
        (
            &[
                "impl Mine where Wrapper<u8>: nc_log::LogValue {",
                "    const TEXT: &'static str = String::new().leak();",
                "}",
            ],
            2,
            true,
        ),
        (
            &[
                "impl Mine where Wrapper<fn() -> u8>: nc_log::LogValue {",
                "    const TEXT: &'static str = String::new().leak();",
                "}",
            ],
            2,
            true,
        ),
        (
            &[
                "fn make() -> Result<u8, nc_log::LogValue> {",
                "    takes_one(\"plain\")",
                "}",
            ],
            2,
            false,
        ),
        (
            &[
                "impl Mine where Wrapper<fn() -> nc_log::LogValue>: Copy {",
                "    takes_one(\"plain\")",
                "}",
            ],
            2,
            false,
        ),
        // **Six rows, each a spelling of "this walk counts brackets by reading
        // characters".** A macro's token tree hides a `<` from the walk that
        // steps over macros and shows it to the one that does not, so a
        // comparison inside a macro can turn the rule off; the mirror is a macro
        // whose own delimiter is a brace, which can be read as the line's block.
        // A shift raises the angle count by two with nothing to lower it, so the
        // region can swallow the line. And a `>` that shuts a
        // generic argument list leaves the `=` beside it a token of its own,
        // which the lexer alone cannot say — the control beneath it is the
        // `>=` whose `=` must still not end a region.
        (
            &[
                "impl Mine where m!(N < 1): Copy { const HELD: Option<nc_log::LogValue> = {",
                "    takes_one(\"plain\")",
                "}; }",
            ],
            2,
            false,
        ),
        (
            &[
                "impl Mine where m!{ N }: nc_log::LogValue {",
                "    const TEXT: &'static str = String::new().leak();",
                "}",
            ],
            2,
            true,
        ),
        (
            &[
                "let held: [u8; 1 << 3] = [0; 8]; nc_log::LogValue {",
                "    takes_one(\"plain\")",
                "}",
            ],
            2,
            true,
        ),
        (
            &[
                "impl<T: Into<<u8 as Trait>::Out> + nc_log::LogValue> Wrapper<T> {",
                "    takes_one(\"plain\")",
                "}",
            ],
            2,
            false,
        ),
        (
            &[
                "let held: Option<u8>= x; nc_log::LogValue {",
                "    takes_one(\"plain\")",
                "}",
            ],
            2,
            true,
        ),
        (
            &[
                "let held = Wrapper { seen: flag >= nc_log::LogValue::MASK, inner: {",
                "    takes_one(\"plain\")",
                "} };",
            ],
            2,
            false,
        ),
        // **Five rows, and the first three turn the rows above against their own
        // rules.** A comparison before a `>=` can inflate the angle count so the
        // flag that reads it certifies the forgery its own doc refuses; the
        // shift guard has to be in every walk that counts angles, not one of
        // the three; and a sentence naming the const argument needs an
        // expression that sees a brace. The last two are controls: a statement
        // boundary is what ends an annotation, and an array length's own `;` is
        // not one.
        (
            &[
                "let held = Wrapper { seen: first < 1, next: flag >= nc_log::LogValue::MASK, inner: {",
                "    takes_one(\"plain\")",
                "} };",
            ],
            2,
            false,
        ),
        (
            &[
                "let held = Wrapper { seen: first<1, next: flag >= nc_log::LogValue::MASK, inner: {",
                "    takes_one(\"plain\")",
                "} };",
            ],
            2,
            false,
        ),
        (
            &[
                "impl Mine where Wrapper<fn() -> [u8; 1 << 3]>: nc_log::LogValue {",
                "    const TEXT: &'static str = String::new().leak();",
                "}",
            ],
            2,
            true,
        ),
        (
            &[
                "let held: Wrapper<{ 1 << 3 }> = x; nc_log::LogValue {",
                "    takes_one(\"plain\")",
                "}",
            ],
            2,
            true,
        ),
        (
            &[
                "let held: [u8; nc_log::LogValue::SIZE] = [0; 8]; takes_one(\"x\") {",
                "    takes_one(\"plain\")",
                "}",
            ],
            2,
            false,
        ),
        // **The fourth walk, and the brace a return type is written inside.**
        // Without the shift guard in `a_block_opens_in`, the fourth of the walks
        // in `wall.rs` that count angle brackets, a shift in a `where` predicate
        // hides the brace that shuts the header, and the dead annotation after
        // it reads as declaring the door. The third row is the same sentence one
        // walk over — a `the_return_type_ends` returning at the first `{` with
        // no depth lets a const argument inside the return type end the fill at
        // its own brace. The last two are the controls: a lone comparison and a `>>`
        // are decided correctly with or without the guard.
        (
            &[
                "impl Mine where [(); 1 << 3]: Copy { const HELD: Option<nc_log::LogValue> = {",
                "    takes_one(\"plain\")",
                "}; }",
            ],
            2,
            false,
        ),
        (
            &[
                "impl Mine where Wrapper<{ 1 << 3 }>: Copy { const HELD: Option<nc_log::LogValue> = {",
                "    takes_one(\"plain\")",
                "}; }",
            ],
            2,
            false,
        ),
        (
            &[
                "fn make() -> Wrapper<{ SIZE }, nc_log::LogValue> {",
                "    takes_one(\"plain\")",
                "}",
            ],
            2,
            false,
        ),
        (
            &[
                "impl Mine where [(); (N < 1) as usize]: Copy { const HELD: Option<nc_log::LogValue> = {",
                "    takes_one(\"plain\")",
                "}; }",
            ],
            2,
            false,
        ),
        (
            &[
                "impl Mine where [(); 1 >> 3]: Copy { const HELD: Option<nc_log::LogValue> = {",
                "    takes_one(\"plain\")",
                "}; }",
            ],
            2,
            false,
        ),
        // The door on a line the caret is neither on nor inside.
        (
            &[
                "fn helper(_: nc_log::LogValue) {}",
                "const ELSEWHERE: &str = String::new().leak();",
            ],
            2,
            false,
        ),
    ];

    /// `(the file's code, the span of the expectation, is it the subject side,
    /// does the file declare it)`.
    ///
    /// **The clause that decides whether a fixture wrote its own sentence.**
    /// `conjures_its_own_reason` needs a whole fixture directory and a compiler,
    /// so the meta-gate cannot weaken it there — `mutants.py` runs `corpus::`
    /// and `limits::` for `wall.rs`, and everything else in that function is
    /// reachable only from `wall::tests`. The door beside `caret_uses_for_tests`
    /// exists for that reason, and this is its corpus.
    const DECLARATIONS: [(&str, &str, bool, bool); 27] = [
        // The bound side: what the wall refuses is never the fixture's to
        // declare, under any of the seven keywords.
        ("trait LogValue {}", "String: LogValue", false, true),
        (
            "pub(crate) trait LogValue {}",
            "String: LogValue",
            false,
            true,
        ),
        (
            "mod inner { pub trait LogValue {} }",
            "String: LogValue",
            false,
            true,
        ),
        (
            "mod nc_log { pub mod sealed {} }",
            "Mine: nc_log::sealed::Sealed",
            false,
            true,
        ),
        // ..and a file that declares none of it.
        ("fn main() {}", "String: LogValue", false, false),
        // The subject side: only the names the language and its prelude own.
        ("struct String;", "String: LogValue", true, true),
        ("struct str;", "&str: LogValue", true, true),
        ("type Vec = u8;", "Vec<u8>: LogValue", true, true),
        // A fixture's own subject is its own, which is what a fixture is.
        ("struct Mine;", "Mine: nc_log::LogValue", true, false),
        // ..and the subject rule does not reach across the colon.
        ("trait LogValue {}", "String: LogValue", true, false),
        // **The eighth binder, which has no keyword in front of it.**
        // `fn takes<String>(name: String)` shadows the prelude `String` for that
        // function's whole body, and the seven keywords look for `struct
        // String`, `type String`, `fn String` — none of which is written.
        // Measured against the seven keywords alone: all four `LogValue`
        // fixtures rewritten this way kept their
        // `EXPECT` lines byte for byte, went on refusing to compile after the
        // three `impl LogValue` the wall exists to forbid were added, and the
        // whole step printed `12 fixture(s) refuse to compile` with `check
        // passed`.
        (
            "fn takes<String>(name: String) { accepts(name); }",
            "String: LogValue",
            true,
            true,
        ),
        ("struct Holder<str>(str);", "&str: LogValue", true, true),
        // ..and the whitespace rustc allows between the name and its group.
        // Nothing formats `crates/nc-log/fixtures/`, so one space there turns
        // off a rule that expects none, with the shipped expectation unchanged.
        (
            "fn takes <String>(name: String) { accepts(name); }",
            "String: LogValue",
            true,
            true,
        ),
        (
            "fn takes\n<String>(name: String) { accepts(name); }",
            "String: LogValue",
            true,
            true,
        ),
        (
            "impl<String> Mine for String {}",
            "String: LogValue",
            true,
            true,
        ),
        // **And the four keywords the rows above do not reach.** The binder's
        // list is `trait struct enum union type fn impl`, and the rows above
        // exercise three of them: with the list narrowed to
        // `["struct ", "fn ", "impl"]` and only those rows, the whole suite and
        // the whole gate are green, measured. Each of the four is a real Rust
        // binder that shadows a prelude name inside its own item, which is the
        // forgery the wall exists to refuse — and the sibling list one function
        // up carries a row per keyword for exactly this reason, under a comment
        // saying a frozen list is not a covered one.
        (
            "trait Shadow<String> { fn f(x: String); }",
            "String: LogValue",
            true,
            true,
        ),
        ("enum Shadow<str> { A(str) }", "&str: LogValue", true, true),
        (
            "union Shadow<String> { a: std::mem::ManuallyDrop<String> }",
            "String: LogValue",
            true,
            true,
        ),
        (
            "type Shadow<String> = String;",
            "String: LogValue",
            true,
            true,
        ),
        // ..and an ordinary *use* of the prelude's type is not a binding, which
        // is what keeps this from refusing every fixture there is.
        (
            "fn takes(name: Vec<String>) {}",
            "String: LogValue",
            true,
            false,
        ),
        // ..nor is a parameter of another name, however the bound is written.
        (
            "fn accepts<T: nc_log::LogValue>(_: T) {}",
            "String: LogValue",
            true,
            false,
        ),
        (
            "fn accepts<T: Into<String>>(_: T) {}",
            "String: LogValue",
            true,
            false,
        ),
        // The three keywords no row above reaches. Without these, `enum`,
        // `union` and `fn` are in the list and decided by nothing: each can be
        // dropped with the whole corpus green, which is a list frozen rather
        // than covered.
        ("enum LogValue {}", "String: LogValue", false, true),
        ("union LogValue { a: u8 }", "String: LogValue", false, true),
        ("fn LogValue() {}", "String: LogValue", false, true),
        // **And a comment about a binder is not a binder.** Every shipped
        // fixture opens with a `//!` header saying what it refuses and why, and
        // a rule that reads that header as code takes a sentence naming
        // `fn takes<String>` for the fixture declaring its own `String`, and
        // refuses it. Nothing formats `crates/nc-log/fixtures/`, so nothing
        // rewrites the sentence either. The too-strong direction, which these
        // rows hold.
        (
            "//! `fn takes<String>` would shadow it\nfn main() {}",
            "String: LogValue",
            true,
            false,
        ),
        (
            "// the type Vec<String> is what this refuses\nfn main() {}",
            "String: LogValue",
            true,
            false,
        ),
    ];

    #[test]
    fn the_declaration_predicate_decides_every_clause() {
        let mut wrong = Vec::new();
        for (code, span, subject, expected) in DECLARATIONS {
            let found = crate::wall::declared_in_for_tests(code, span, subject).is_some();
            if found != expected {
                wrong.push(format!(
                    "  `{code}` against `{span}` — the gate says the fixture {} it, and it \
                     {}",
                    if found {
                        "declares"
                    } else {
                        "does not declare"
                    },
                    if expected { "does" } else { "does not" }
                ));
            }
        }
        assert!(
            wrong.is_empty(),
            "{} of the corpus for the declaration rule decided differently from what it \
             is written to decide:\n{}",
            wrong.len(),
            wrong.join("\n")
        );
    }

    #[test]
    fn the_caret_predicate_decides_every_clause() {
        let mut wrong = Vec::new();
        for (lines, at, expected) in CARETS {
            let found = crate::wall::caret_uses_for_tests(lines, at, "nc_log::LogValue");
            if found != expected {
                wrong.push(format!(
                    "  {lines:?} at line {at} — the gate says the refusal is {}, and it \
                     is {}",
                    if found { "the door's" } else { "unrelated" },
                    if expected { "the door's" } else { "unrelated" }
                ));
            }
        }
        the_caret_rule_reads(&wrong);
    }

    /// `(file name, is it an environment file)`.
    ///
    /// One input per clause, and one per entry of `KNOWN_FORMATS` — a list is a
    /// clause per element, and removing one element is exactly a single-token
    /// weakening.
    const ENVIRONMENTS: [(&str, bool); 50] = [
        // The plain shapes.
        (".env", true),
        (".env.local", true),
        (".env.production", true),
        ("env", true),
        ("env.production", true),
        ("secrets.env.local", true),
        ("production.env", true),
        // The format is the last part, not the first.
        ("rs.env", true),
        // No `env` part at all.
        ("settings.toml", false),
        ("environment.toml", false),
        ("envelope.rs", false),
        // A file with a type is that type — one input per `KNOWN_FORMATS` entry.
        ("env.rs", false),
        // **The leading dot decides whether the type exemption applies at
        // all**: a Rust module is not a dotfile, and the dot is an empty first
        // part, so a hidden file whose name holds `env` is an environment
        // whatever follows it. A rule answering `false` here lets a hidden
        // environment written as shell or as JSON — two of the four formats an
        // environment actually comes in — reach a clone with `check passed`,
        // measured.
        (".env.rs.", true),
        // **The non-dotfile with a trailing dot**, which reading the parts
        // unfiltered decides: it is three parts and gets no exemption, where a
        // rule dropping empty parts makes it `["env", "rs"]`, a module. The
        // dotfile above is the same shape behind a leading empty part, and
        // dropping empty parts makes it `["env", "rs"]` too.
        // **A doubled or trailing dot is not a file type.** Dropping empty
        // parts collapses both to `["env", "rs"]`, so a module exemption would
        // fire while `.gitignore` ignores them — 48 such names, measured against
        // real `git check-ignore` over 115,454 generated ones.
        ("env.rs.", true),
        ("env..rs", true),
        ("env..sh..", true),
        // **`env` first, a format last, and something in between**: an
        // environment named for its deployment, written in the format an
        // environment is written in. The module exemption asks for exactly two
        // parts, and these have three.
        ("env.production.sh", true),
        ("env.local.json", true),
        // **direnv's file**, whose parts are an empty one and `envrc`, so a rule
        // reading parts alone does not call it an environment file — while the
        // docstring says "anything beginning `.env` is one". The `.gitignore`
        // agreement test lists it too, because a gap here and a gap in
        // `.gitignore` leave both halves off together.
        (".envrc", true),
        // **Case is folded by the reporter, not by the predicate.** These rows
        // drive `committed_env_files`, which folds; `holds_an_environment` stays
        // case-sensitive because the `.gitignore` agreement test binds it to
        // git, which is. Measured, an upper-cased env file under `policy/core/`
        // holding a password: `check passed`, with the whole run differing by
        // one number that went *up*.
        (".ENV", true),
        (".Env.local", true),
        ("PRODUCTION.ENV", true),
        // **The cased variant this block is written for**, not a copy of the
        // row fifty lines above: the same case is the same clause, and a copy
        // decides nothing.
        (".ENV.LOCAL", true),
        (".env.sh", true), // allow-reference: a name the rule judges, not a file here
        (".env.json", true), // allow-reference: a name the rule judges, not a file here
        (".env.toml", true), // allow-reference: a name the rule judges, not a file here
        // Not a dotfile, but `env` is not the whole name either — something
        // precedes it, so the module exemption does not reach it.
        ("secrets.env.sh", true),
        // **And with a kind the exemption would accept**, which is what tells
        // `["env", last]` from `[.., "env", last]`: `sh` is an environment
        // format, so the pattern's *shape* decided nothing for the row above —
        // both readings answer the same. Something precedes `env` here, so the
        // module exemption must not reach it whatever the kind.
        ("secrets.env.md", true),
        // **The five formats an environment is actually written in.** These
        // read `false` — a module exemption asking only whether the suffix is a
        // file kind this repository holds makes a file named `env` wearing one
        // of them "a module named `env`". Measured,
        // one such file under `policy/` holding `DATABASE_URL` and
        // `AWS_SECRET_ACCESS_KEY`: `check passed`, exit 0, online and
        // `--offline` both, with the whole run differing from a clean one by a
        // single number — the path count going up by one. The gate enumerated
        // the credential file and reported that it had counted one more.
        ("env.toml", true),
        ("env.yml", true),
        ("env.yaml", true),
        ("env.json", true),
        ("env.sh", true),
        // And the kinds an environment is not written in, which is what the
        // exemption is for: a module named `env`, of that type.
        ("env.md", false),
        ("env.sql", false),
        ("env.lock", false),
        ("env.cedar", false),
        ("env.cedarschema", false),
        ("env.tsv", false),
        // The templates — one per word, and the arrangement that separates `any`
        // from `all`.
        (".env.example", false),
        (".env.sample", false),
        (".env.template", false),
        (".env.dist", false),
        (".env.local.example", false),
        ("example.env", false),
        // A qualifier that names no format is not a type.
        (".env.private", true),
        (".env.staging", true),
        // Not an environment file by any clause.
        ("Cargo.toml", false),
        ("README.md", false),
        ("justfile", false),
    ];

    #[test]
    fn the_environment_predicate_decides_every_clause() {
        let mut wrong = Vec::new();
        for (name, expected) in ENVIRONMENTS {
            let scratch = Scratch::new();
            scratch.write(name, "TOKEN=live\n");
            let (findings, walked) =
                crate::gates::committed_env_files(scratch.root()).expect("a readable scratch");
            assert!(walked >= 1, "the walk did not reach `{name}`");
            let found = of_kind(&findings, "is committed here") == 1;
            if found != expected {
                wrong.push(format!(
                    "  `{name}` — the gate says {}, and it is {}",
                    if found {
                        "committed secret"
                    } else {
                        "harmless"
                    },
                    if expected {
                        "a file holding an environment"
                    } else {
                        "not one"
                    }
                ));
            }
        }
        verdict(&wrong, "holds_an_environment");
    }

    // ---------------------------------------------------------------------------
    // 2. `gates::codes_in` and `gates::glued`, through
    //    `gates::no_code_is_written_at_a_call_site`.
    //
    // What it decides: whether a line spells out an error code. A weakening ships a
    // hand-written code the registry can never renumber, translate or publish, with
    // `source line(s) name no error code as text (§15.1)` printed over it.
    // ---------------------------------------------------------------------------

    /// `(the line, how many codes it spells out)`.
    const HAND_WRITTEN: [(&str, usize); 25] = [
        // The plain code, and the four ways it is assembled.
        (r#"let code = "NC-KERN-4242";"#, 1),
        (r#"let code = concat!("NC-", "KERN-4242");"#, 1),
        (r#"let code = concat!("NC-" , "KERN-4242");"#, 1),
        // Whitespace touching the glue is glue: the space is inside the literal, so
        // only the pop half of the trim can remove it.
        (r#"let code = concat!("NC-KERN-42 ", "42");"#, 1),
        (r#"let code = "NC-KERN-424\x34";"#, 1),
        (r#"let code = "NC-KERN-424\u{34}";"#, 1),
        (r#"let code = "nc-kern-4245".to_ascii_uppercase();"#, 1),
        (r#"let code = "nc-kern-4245".to_uppercase();"#, 1),
        // A declared code is one too: the identity is the variant, not the
        // spelling.
        (r#"let code = "NC-DB-0001";"#, 1),
        // Two on one line are two.
        (r#"let a = "NC-KERN-4242"; let b = "NC-DB-0009";"#, 2),
        // The area must be upper case and non-empty, and a hyphen must follow it.
        (r#"let code = "NC-kern-4242";"#, 0),
        (r#"let code = "NC--4242";"#, 0),
        (r#"let code = "NC-KERN_4242";"#, 0),
        (r#"let code = "NC-KERN 4242";"#, 0),
        // Exactly four digits.
        (r#"let code = "NC-KERN-424";"#, 0),
        (r#"let code = "NC-KERN-42425";"#, 0),
        (r#"let code = "NC-KERN-4A42";"#, 0),
        (r#"let code = "NC-KERN-";"#, 0),
        // The prefix is `NC-` and nothing else.
        (r#"let code = "XC-KERN-4242";"#, 0),
        (r#"let code = "NCKERN-4242";"#, 0),
        (r#"let code = "NC_KERN_4242";"#, 0),
        (r#"let code = "ANC-KERN-4242";"#, 1),
        // Prose with spaced hyphens is prose, and the glue must not read it as a
        // code — the regression `glued`'s own comment records.
        ("// NC - CANON - 0001 is the shape of a code", 0),
        (r#"let words = "NC - CANON - 0001";"#, 0),
        // A comment is not a call site.
        (
            r"// the journal raises NC-KERN-4242 when the chain breaks",
            0,
        ),
    ];

    #[test]
    fn the_hand_written_code_predicate_decides_every_clause() {
        let declared = vec!["NC-DB-0001".to_owned(), "NC-DB-0009".to_owned()];
        let mut wrong = Vec::new();
        for (line, expected) in HAND_WRITTEN {
            let scratch = Scratch::new();
            scratch.write("crates/nc-probe/src/lib.rs", &format!("{line}\n"));
            let (findings, read, _) =
                crate::gates::no_code_is_written_at_a_call_site(scratch.root(), &declared)
                    .expect("a readable scratch");
            assert!(read >= 1, "the walk did not reach the line `{line}`");
            let found = of_kind(&findings, "written out as text");
            if found != expected {
                wrong.push(format!(
                    "  `{line}` — the gate found {found} code(s), and it spells out {expected}"
                ));
            }
        }
        verdict(&wrong, "codes_in");
    }

    /// The scope is a clause too: which directories the rule is about.
    ///
    /// A single-token widening or narrowing here is a whole crate the rule stops
    /// being asked about, and no count can see it — `raw` moves with the scope.
    const SCOPE: [(&str, usize); 8] = [
        ("crates/nc-probe/src/lib.rs", 1),
        ("crates/nc-probe/tests/tampering.rs", 1),
        ("migrations/0009_probe.sql", 1),
        // The local database's migrations, which ship to every user's machine.
        ("local-migrations/0009_probe.sql", 1),
        // The registry, its compiler and the pages generated from them are where
        // codes belong.
        ("crates/nc-catalog/src/lib.rs", 0),
        ("crates/nc-catalog-compiler/src/ledger.rs", 0),
        // Not the platform: build-time diagnostics are not codes this registry
        // declares.
        ("xtask/src/gates.rs", 0),
        ("docs/errors.md", 0),
    ];

    #[test]
    fn the_hand_written_code_scope_decides_every_clause() {
        let declared: Vec<String> = Vec::new();
        let mut wrong = Vec::new();
        for (path, expected) in SCOPE {
            let scratch = Scratch::new();
            let comment = if std::path::Path::new(path)
                .extension()
                .is_some_and(|kind| kind.eq_ignore_ascii_case("sql"))
            {
                "--"
            } else {
                "//"
            };
            scratch.write(
                path,
                &format!("{comment} x\nlet code = \"NC-KERN-4242\";\n"),
            );
            let (findings, _, _) =
                crate::gates::no_code_is_written_at_a_call_site(scratch.root(), &declared)
                    .expect("a readable scratch");
            let found = of_kind(&findings, "written out as text");
            if found != expected {
                wrong.push(format!(
                    "  `{path}` — the gate found {found}, and the rule reaches it {expected} time(s)"
                ));
            }
        }
        verdict(&wrong, "no_code_is_written_at_a_call_site's scope");
    }

    // ---------------------------------------------------------------------------
    // 3. `gates::lints_put_down` and `LINT_GROUPS`, through
    //    `gates::no_source_puts_the_lints_down`.
    //
    // What it decides: whether a file turns a whole lint group off for everything
    // below it. A weakening ships acceptance criterion 3's own suite with the lints
    // off and `ok lints` printed over it.
    // ---------------------------------------------------------------------------

    /// `(the file's text, how many blanket allows it holds)`.
    const BLANKET: [(&str, usize); 53] = [
        // The plain shape, and the two openers.
        ("#![allow(clippy::all)]\n", 1),
        ("#![expect(clippy::all)]\n", 1),
        // Wrapped the way rustfmt wraps it once the list is long.
        ("#![allow(\n    clippy::all,\n    clippy::pedantic\n)]\n", 2),
        // The predicate that is always true in front of it.
        ("#![cfg_attr(all(), allow(clippy::all))]\n", 1),
        // Room around the name is the same name.
        ("#![allow(clippy :: all)]\n", 1),
        // Two groups in one attribute are two.
        ("#![allow(clippy::all, clippy::pedantic)]\n", 2),
        // One input per entry of `LINT_GROUPS` — removing one is a single-token
        // weakening, and each is a group.
        ("#![allow(warnings)]\n", 1),
        ("#![allow(unused)]\n", 1),
        ("#![allow(clippy::nursery)]\n", 1),
        ("#![allow(clippy::cargo)]\n", 1),
        ("#![allow(clippy::correctness)]\n", 1),
        ("#![allow(clippy::style)]\n", 1),
        ("#![allow(clippy::complexity)]\n", 1),
        ("#![allow(clippy::perf)]\n", 1),
        ("#![allow(clippy::suspicious)]\n", 1),
        ("#![allow(clippy::restriction)]\n", 1),
        // The eleven rustc groups a list written from memory misses. Each is a
        // group by `rustc -W help`'s own table and an entry of `LINT_GROUPS`,
        // and dropping that entry fails its row here.
        ("#![allow(deprecated_safe)]\n", 1),
        ("#![allow(future_incompatible)]\n", 1),
        ("#![allow(keyword_idents)]\n", 1),
        ("#![allow(let_underscore)]\n", 1),
        ("#![allow(nonstandard_style)]\n", 1),
        ("#![allow(refining_impl_trait)]\n", 1),
        ("#![allow(rust_2018_compatibility)]\n", 1),
        ("#![allow(rust_2018_idioms)]\n", 1),
        ("#![allow(rust_2021_compatibility)]\n", 1),
        ("#![allow(rust_2024_compatibility)]\n", 1),
        ("#![allow(unknown_or_malformed_diagnostic_attributes)]\n", 1),
        // An inner attribute, not an outer one: an outer allow is scoped to the
        // item under it and is the mechanism working.
        ("#[allow(clippy::all)]\nfn f() {}\n", 0),
        // **Unless the item is a module, where "scoped to the item under it" is
        // a whole file.** This case is the one an argument from a function
        // misses: measured on the real tree, a
        // planted `clippy::uninlined_format_args` failed a cold clippy, and then
        // one `#[allow(clippy::all, clippy::pedantic)]` above `pub mod entry;`
        // left clippy silent with `check passed`.
        ("#[allow(clippy::all)]\nmod m;\n", 1),
        ("#[allow(clippy::pedantic)]\npub mod m;\n", 1),
        ("#[allow(clippy::all)]\npub(crate) mod m;\n", 1),
        // Stacked, because attributes stack: the allow is still on the module.
        ("#[cfg(test)]\n#[allow(clippy::all)]\nmod m;\n", 1),
        // And wrapped over lines, the shape rustfmt produces on its own.
        ("#[allow(\n    clippy::all\n)]\nmod m;\n", 1),
        // A blank line between them does not make it a different item.
        ("#[allow(clippy::all)]\n\nmod m;\n", 1),
        // An outer allow on anything else is still the mechanism working.
        ("#[allow(clippy::all)]\nstruct S;\n", 0),
        ("#[allow(clippy::all)]\nimpl S {}\n", 0),
        // A name that merely begins with `mod` is not a module — and the case
        // has to *start* with those three letters for the rule to be doing any
        // work. `fn model()` is refused by the `fn` in front of it whatever
        // `mod ` is spelled as, so it leaves `starts_with("mod")` deciding
        // nothing; `mutants.py` reports the clause as a survivor without this.
        ("#[allow(clippy::all)]\nfn model() {}\n", 0),
        ("#[allow(clippy::all)]\nmodel!();\n", 0),
        ("#[allow(clippy::all)]\npub modest_thing();\n", 0),
        // And an attribute on no item at all puts nothing down.
        ("#[allow(clippy::all)]\n", 0),
        // **`#![`, not `#!`.** Widening the opener to `#!` is not an equivalent
        // weakening, though it can be argued as one: "the only other
        // `#!`-initial line in Rust is a shebang, which balances and holds no
        // lint name". This gate reads lines, and the interior of a multi-line
        // raw string is a line — so the widened rule reports a *false* finding
        // against a shell script carried in a `r#"…"#`, in the direction that
        // costs a reader time rather than hiding a defect.
        (
            "const S: &str = r#\"\n#!allow(clippy::all)\necho hello\n\"#;\n",
            0,
        ),
        // And a real shebang, which is the case the argument was about.
        ("#!/usr/bin/env rust-script\nfn main() {}\n", 0),
        // **And a shebang above a real one**, which is what says the shebang
        // does not *hide* the allow beneath it. Neither row can tell `#![` from
        // `#!`: measured, both spellings give the same count for both, because
        // a shebang with no bracket in it accumulates and closes on the line
        // below. What distinguishes them is a shebang that carries a bracket,
        // and the assertion that catches it is about a different kind of
        // finding, so it lives in `gates::tests` beside the floor it belongs to.
        (
            "#!/usr/bin/env rust-script\n#![allow(clippy::all)]\nfn main() {}\n",
            1,
        ),
        // A single named lint is not a group.
        ("#![allow(clippy::match_same_arms)]\n", 0),
        ("#![allow(dead_code)]\n", 0),
        ("#![expect(clippy::match_same_arms)]\n", 0),
        // A word that ends in `allow(` is not one.
        ("#![disallow(clippy::all)]\n", 0),
        ("#![my_allow(clippy::all)]\n", 0),
        // A comment is not an attribute.
        ("// #![allow(clippy::all)]\n", 0),
        ("//! `#![allow(clippy::all)]` is what this refuses\n", 0),
        // Ordinary code decides nothing and must produce nothing.
        ("fn main() {}\n", 0),
        ("#![doc = \"clippy::all\"]\n", 0),
        // An unbalanced line that never was an attribute must not swallow the file.
        ("let x = \"(\";\n#![allow(clippy::all)]\n", 1),
    ];

    #[test]
    fn the_blanket_allow_predicate_decides_every_clause() {
        let mut wrong = Vec::new();
        for (text, expected) in BLANKET {
            let scratch = Scratch::new();
            scratch.write("crates/nc-probe/src/lib.rs", text);
            let (findings, read) = crate::gates::no_source_puts_the_lints_down(scratch.root())
                .expect("a readable scratch");
            assert!(read >= 1, "the walk did not reach `{text:?}`");
            let found = of_kind(&findings, "turns off");
            if found != expected {
                wrong.push(format!(
                    "  {text:?} — the gate found {found} blanket allow(s), and it holds {expected}"
                ));
            }
        }
        verdict(&wrong, "lints_put_down + LINT_GROUPS");
    }

    // ---------------------------------------------------------------------------
    // 3a. `gates::every_hatch_carries_its_reason`'s lint half.
    //
    // What it decides: whether a lint put down carries a written reason. It
    // holds the form of invariant 15 — a judgement whose substance, `AGENTS.md`
    // says, no gate can hold — and without a corpus both of the clauses below
    // can be wrong with every review passing.
    // ---------------------------------------------------------------------------

    /// `(the file's text, how many hatches in it carry no written reason)`.
    const REASONS: [(&str, usize); 21] = [
        // The plain shape, both spellings, at an item and at a crate root.
        ("#[allow(clippy::all)]\nfn f() {}\n", 1),
        ("#[expect(clippy::all)]\nfn f() {}\n", 1),
        ("#![allow(clippy::all)]\n", 1),
        ("#![expect(clippy::all)]\n", 1),
        // The reason, written where `#[expect]` takes it.
        ("#[allow(clippy::all, reason = \"why\")]\nfn f() {}\n", 0),
        ("#[expect(clippy::all, reason = \"why\")]\nfn f() {}\n", 0),
        // **And on one line.** Asked as `!code.contains('"')`, the rule drops
        // every single-line attribute that answers it — a reason is written in
        // quotes — before judging it: measured over a tree holding thirty-one,
        // three.
        ("#[allow(clippy::all, reason = \"why\")]\nmod m;\n", 0),
        // Wrapped over lines, the shape rustfmt produces once the list is long.
        (
            "#[allow(\n    clippy::all,\n    reason = \"why\"\n)]\nfn f() {}\n",
            0,
        ),
        (
            "#[allow(\n    clippy::all,\n    clippy::pedantic\n)]\nfn f() {}\n",
            1,
        ),
        // A comment above is the other way to earn one: a crate-level attribute
        // has no item to sit on, and the generated crates carry theirs that way.
        ("// why\n#[allow(clippy::all)]\nfn f() {}\n", 0),
        ("// why\n#![allow(clippy::all)]\n", 0),
        // **A doc comment is not a reason.** `///` begins with `//`, so a rule
        // asking for a `//` line above is satisfied by an allow written under
        // the documentation of the item it sits on — which is where an allow is
        // usually written, and which says nothing about the lint. Such a rule is
        // off for every documented item in the repository.
        ("/// The item.\n#[allow(clippy::all)]\nfn f() {}\n", 1),
        ("//! The module.\n#![allow(clippy::all)]\n", 1),
        ("/// The item.\n#[expect(clippy::all)]\nfn f() {}\n", 1),
        // And a doc comment above an ordinary one still earns it: what is asked
        // is the line immediately above.
        (
            "/// The item.\n// why\n#[allow(clippy::all)]\nfn f() {}\n",
            0,
        ),
        // **A quote in a trailing comment does not take the line out of the
        // gate**, which is the same clause seen from the other side: under
        // `!code.contains('"')` it is not counted, not judged, and no finding.
        ("#[allow(clippy::all)] // see \"the note\"\nfn f() {}\n", 1),
        // A string is not an attribute. The emitter's template carries a real
        // `#![allow(...)]` inside a `RENDER` literal, and a rule that reads
        // literals takes it for one of this tree's own hatches.
        ("const S: &str = \"\\n#[allow(clippy::all)]\\n\";\n", 0),
        ("const S: &str = r#\"\n#![allow(clippy::all)]\n\"#;\n", 0),
        // A comment is not an attribute either.
        ("// #[allow(clippy::all)]\nfn f() {}\n", 0),
        (
            "/// `#[allow(clippy::all)]` is what this refuses\nfn f() {}\n",
            0,
        ),
        // Ordinary code decides nothing and must produce nothing.
        ("fn main() {}\n", 0),
    ];

    #[test]
    fn the_written_reason_predicate_decides_every_clause() {
        let mut wrong = Vec::new();
        for (text, expected) in REASONS {
            let scratch = Scratch::new();
            scratch.write("crates/nc-probe/src/lib.rs", text);
            // **A second file, because the gate refuses a tree with nothing put
            // down at all** — that is a walk that stopped matching rather than a
            // clean repository, and it is the floor the gate carries. This one
            // is a correctly written hatch, so it holds the floor up without
            // adding a finding to any case.
            scratch.write(
                "crates/nc-anchor/src/lib.rs",
                "#[allow(clippy::all, reason = \"the floor this walk carries\")]\nfn f() {}\n",
            );
            let (findings, read, _) = crate::gates::every_hatch_carries_its_reason(scratch.root())
                .expect("a readable scratch");
            assert!(read >= 1, "the walk did not reach `{text:?}`");
            let found = of_kind(&findings, "no written reason");
            if found != expected {
                wrong.push(format!(
                    "  {text:?} — the gate found {found} unreasoned hatch(es), and it holds \
                     {expected}"
                ));
            }
        }
        verdict(&wrong, "every_hatch_carries_its_reason's lint half");
    }

    // ---------------------------------------------------------------------------
    // 4. `gates::is_a_test_attribute`, `gates::runs` and the four `what` arms,
    //    through `gates::no_ignored_test`.
    //
    // What it decides: whether an attribute hides a test from `cargo test`. It is
    // the most dangerous verdict in `xtask/src/` because it is the only one whose
    // failure disables every other guarantee in the repository: everything else
    // here is asserted by a test, and a weakening ships acceptance criterion 3's
    // own suite not running at all — with `ok N test(s)` printed over it, because
    // the census shares this predicate and both sides of the floor move together.
    // The gate's own doc records it happening.
    // ---------------------------------------------------------------------------

    /// `(the file's text, how many tests it hides)`.
    ///
    /// One input per clause of `is_a_test_attribute` — the path resolution, the
    /// `cfg_attr` argument list, the depth-zero comma — per clause of `runs` — the
    /// wrapped attribute, the blank line, the doc comment inside a run — and per
    /// arm of the `what` that names the finding.
    const HIDDEN: [(&str, usize); 39] = [
        // The plain shape, and the reason that gets past clippy.
        ("#[test]\n#[ignore]\nfn t() {}\n", 1),
        ("#[test]\n#[ignore = \"a reason\"]\nfn t() {}\n", 1),
        // Either order: the run is a set, not a sequence.
        ("#[ignore = \"a reason\"]\n#[test]\nfn t() {}\n", 1),
        // A condition is the other way to delete a test, and it is not `#[ignore]`.
        ("#[test]\n#[cfg(unix)]\nfn t() {}\n", 1),
        ("#[cfg(all(unix, not(unix)))]\n#[test]\nfn t() {}\n", 1),
        // A `cfg_attr` can add `#[ignore]` under a condition.
        (
            "#[test]\n#[cfg_attr(unix, ignore = \"a reason\")]\nfn t() {}\n",
            1,
        ),
        // `is_a_test_attribute` resolves a path, and one input per spelling of
        // one — the last segment is the whole rule.
        ("#[tokio::test]\n#[ignore = \"a reason\"]\nfn t() {}\n", 1),
        ("#[::tokio::test]\n#[ignore = \"a reason\"]\nfn t() {}\n", 1),
        (
            "#[std::prelude::v1::test]\n#[ignore = \"a reason\"]\nfn t() {}\n",
            1,
        ),
        // Arguments after the path are still that path.
        (
            "#[tokio::test(flavor = \"multi_thread\")]\n#[ignore = \"a reason\"]\nfn t() {}\n",
            1,
        ),
        // A `cfg_attr` whose *argument* is the test attribute: two findings,
        // because the `cfg_attr` and the `ignore` each hide it by themselves.
        (
            "#[cfg_attr(all(), ::tokio::test)]\n#[ignore = \"a reason\"]\nfn t() {}\n",
            2,
        ),
        // The condition holds its own commas, so the attributes are what follows
        // the last comma at depth zero.
        (
            "#[cfg_attr(all(unix, feature = \"x\"), test)]\n#[ignore = \"a reason\"]\nfn t() {}\n",
            2,
        ),
        // `runs`: an attribute `cargo fmt` wrapped across four lines is one
        // attribute.
        (
            "#[cfg_attr(\n    all(unix, feature = \"x\"),\n    ignore = \"a reason\"\n)]\n#[test]\nfn t() {}\n",
            1,
        ),
        // `runs`: a blank line inside a run stays inside it.
        ("#[test]\n\n#[ignore = \"a reason\"]\nfn t() {}\n", 1),
        // `runs`: so does a doc comment between the attribute and the item.
        (
            "#[test]\n#[ignore = \"a reason\"]\n/// what it proves\nfn t() {}\n",
            1,
        ),
        // A `mod x;` declares a whole file, and a condition on it deletes every
        // test in that file without touching it.
        ("#[cfg(feature = \"slow\")]\nmod slow_cases;\n", 1),
        // **And behind a visibility**, which the brace form has two rows for.
        // `pub mod entry;` is how half the module declarations in this tree are
        // written.
        ("#[cfg(feature = \"slow\")]\npub mod slow_cases;\n", 1),
        ("#[cfg(unix)]\npub(crate) mod slow_cases;\n", 1),
        // A word beginning `mod` is not the keyword.
        ("#[cfg(unix)]\npub modest_helper();\n", 0),
        // **A literal that spans lines does not hide the tests below it.**
        // A `holds_a_test` asking a whole-file stripper for one line at a time
        // blanks the first line of such a literal and gets every line after it
        // back as code — a `}` in one closes the module early and the gate goes
        // silent on a condition that is still there. Measured on
        // `crates/nc-policy/src/problem.rs`: with the fixture, `check passed`;
        // without it, `this test is hidden from the run by a condition`.
        (
            "#[cfg(unix)]\nmod probe {\n    let shape = \"a fixture whose tail is\n}\";\n    \
             #[test]\n    fn t() {}\n}\n",
            1,
        ),
        // And the same shape with no test in it is still nobody's finding.
        (
            "#[cfg(unix)]\nmod probe {\n    let shape = \"a fixture whose tail is\n}\";\n    \
             pub fn helper() {}\n}\n",
            0,
        ),
        // An inner attribute governs the file, so it hides every test below it —
        // and there is no item to look ahead to.
        ("#![cfg(unix)]\n#[test]\nfn t() {}\n", 1),
        (
            "#![cfg_attr(unix, deny(warnings))]\n#[test]\nfn t() {}\n",
            1,
        ),
        // Two hidden tests are two.
        (
            "#[test]\n#[ignore = \"a\"]\nfn one() {}\n#[test]\n#[ignore = \"b\"]\nfn two() {}\n",
            2,
        ),
        // A test that runs is not a finding.
        ("#[test]\nfn t() {}\n", 0),
        ("#[tokio::test]\nfn t() {}\n", 0),
        // `#[cfg(test)]` on a test module is how one is written, and it is the
        // condition `cargo test` sets.
        ("#[cfg(test)]\nmod tests {\n#[test]\nfn t() {}\n}\n", 0),
        ("#[cfg(test)]\npub mod tests {\n#[test]\nfn t() {}\n}\n", 0),
        // A word that ends in `test` is not the attribute `test`.
        ("#[test_case]\n#[ignore = \"a reason\"]\nfn t() {}\n", 0),
        ("#[testing]\n#[ignore = \"a reason\"]\nfn t() {}\n", 0),
        // An `#[ignore]` on something that is not a test hides no test.
        ("#[ignore = \"a reason\"]\nfn helper() {}\n", 0),
        ("#[cfg(unix)]\nfn helper() {}\n", 0),
        // An inner attribute over a file holding no test hides nothing.
        ("#![cfg(unix)]\npub fn f() {}\n", 0),
        // A comment is not an attribute.
        ("// #[test]\n// #[ignore]\nfn t() {}\n", 0),
        // A doc *attribute* quoting one is not one either: a predicate that
        // reads an attribute's text without asking whose it is takes the
        // quotation for the real thing.
        ("#[doc = \"#[ignore]\"]\n#[test]\nfn t() {}\n", 0),
        // **A test module is one by what it holds, not by its name.** Many of
        // this tree's test modules are called something other than `tests` —
        // `query_tests`, `the_frozen_path_rule`, `hostile_store_tests` — and a
        // second condition on one of them is invisible to a gate reading names,
        // leaving the census as the only reader: it carries the run's doc-tests
        // as slack, so hiding one or two moves no number anywhere. Measured on
        // `crates/nc-ids/src/base32.rs`: the test count fell by two under
        // `check passed`.
        (
            "#[cfg(test)]\n#[cfg(not(test))]\nmod width_tests {\n#[test]\nfn t() {}\n}\n",
            1,
        ),
        // And the ordinary spelling under such a name is still not a finding, or
        // thirty modules in this tree become one each.
        (
            "#[cfg(test)]\nmod width_tests {\n#[test]\nfn t() {}\n}\n",
            0,
        ),
        (
            "#[cfg(test)]\npub mod query_tests {\n#[test]\nfn t() {}\n}\n",
            0,
        ),
        // A module holding no test is a module: a condition on one is somebody's
        // build decision and not a test taken out of the run.
        (
            "#[cfg(test)]\n#[cfg(not(test))]\nmod helpers {\npub fn h() {}\n}\n",
            0,
        ),
    ];

    #[test]
    fn the_hidden_test_predicate_decides_every_clause() {
        let mut wrong = Vec::new();
        for (text, expected) in HIDDEN {
            let scratch = Scratch::new();
            scratch.write("crates/nc-probe/src/lib.rs", text);
            // Read back off disk and handed to the shipped entry point the way
            // `check.rs` hands it: a repository-relative name and the file's text.
            let read = std::fs::read_to_string(scratch.root().join("crates/nc-probe/src/lib.rs"))
                .expect("a readable scratch");
            let findings = crate::gates::no_ignored_test("crates/nc-probe/src/lib.rs", &read);
            if findings.len() != expected {
                wrong.push(format!(
                    "  {text:?} — the gate found {}, and it hides {expected} test(s)",
                    findings.len()
                ));
            }
        }
        verdict(&wrong, "is_a_test_attribute + runs + no_ignored_test");
    }

    // ---------------------------------------------------------------------------
    // 4b. `dev::is_backdated`, the forward-only migration rule.
    //
    // What it decides: whether a migration on disk is new and numbered below one
    // this database has already applied. Written inline in `migrate`, which no
    // test runs, in a file outside `mutants.py`'s sources, the clause is decided
    // from neither direction: measured that way, the condition replaced by
    // `false` gives `cargo test --workspace --locked` exit 0, every target `ok`,
    // and `cargo xtask dev` applying version 0 after version 12 while printing
    // `ok 1 migration(s) applied`. So it is the function `dev::is_backdated`,
    // driven here and mutated through `SOURCES`. `_sqlx_migrations` records the
    // order it is told, so afterwards the database reads as though the backdated
    // migration had always been there — which is acceptance criterion 10's whole
    // subject.
    // ---------------------------------------------------------------------------

    /// `(what the database has applied, the highest of it)`.
    ///
    /// **The guard's argument, which `BACKDATED` does not reach.** Every row of
    /// `BACKDATED` hands `is_backdated` a `highest` written out as `12`, so
    /// without these rows the expression that computes it is decided by
    /// nothing: measured, `applied.last()` replaced by `applied.first()` left
    /// `check passed`, and a migration numbered 13 was applied and recorded
    /// after one numbered 20.
    ///
    /// The rows are chosen so that *the last*, *the first* and *the smallest*
    /// each answer differently from the largest on at least one of them — an
    /// unordered read is the same defect one line up, and a list that is sorted
    /// throughout cannot tell the four apart.
    const THE_HIGHEST_APPLIED: [(&[i64], Option<i64>); 6] = [
        // Nothing applied: the ordinary first `dev`, and refusing it would be
        // the other half of getting this wrong.
        (&[], None),
        (&[7], Some(7)),
        // Sorted, which is what the `SELECT` gives today.
        (&[1, 2, 12], Some(12)),
        // Unsorted, where *the last* is wrong.
        (&[1, 20, 3], Some(20)),
        // Unsorted, where *the first* is wrong.
        (&[3, 20, 1], Some(20)),
        // Where the smallest is wrong and the last happens to be right, so a
        // rule reading the minimum is not saved by the row above.
        (&[9, 4, 31], Some(31)),
    ];

    #[test]
    fn the_highest_applied_is_the_highest_and_not_the_last_row_read() {
        let mut wrong = Vec::new();
        for (applied, expected) in THE_HIGHEST_APPLIED {
            let answered = crate::dev::highest_applied(applied);
            if answered != expected {
                wrong.push(format!(
                    "  {applied:?} — the rule answered {answered:?}, and the highest \
                     applied is {expected:?}"
                ));
            }
        }
        verdict(&wrong, "dev::highest_applied");
    }

    /// What the record-limits line says about the tables an escape hatch covers.
    ///
    /// **A line whose branch is unreachable is a line nobody says**, and without
    /// these rows the two functions that build these suffixes are decided by
    /// nothing. Measured, `if exempt.is_empty()` replaced by `if true`: the
    /// whole xtask suite green and `check passed`, with the run printing
    /// `ok 2 table(s) created in the migrations were checked` and dropping the
    /// clause that names `journal_entries` — the only place a run says that
    /// table declares no bound. The same for `excused_by_a_marker`, which
    /// *looks* covered: deleting its literal is caught by a source-text
    /// sentence census, and a sentence census sees that a sentence exists, not
    /// whether anything says it.
    ///
    /// Both the empty case and the plural are rows, because the empty case is
    /// what a weakening turns every input into.
    const WHAT_THE_HATCH_LINE_SAYS: [(&[&str], &str, &str); 6] = [
        (&[], "", ""),
        (
            &["journal_entries"],
            ", and 1 excused by a marker: journal_entries",
            ", and 1 of them declares no bound here: journal_entries",
        ),
        (
            &["journal_entries", "access_records"],
            ", and 2 excused by a marker: journal_entries, access_records",
            ", and 2 of them declare no bound here: journal_entries, access_records",
        ),
        // A third, so "1 or many" is not the whole vocabulary the number has.
        (
            &["a", "b", "c"],
            ", and 3 excused by a marker: a, b, c",
            ", and 3 of them declare no bound here: a, b, c",
        ),
        // A name with a schema on it, which the marker half does see.
        (
            &["archive.login_attempts"],
            ", and 1 excused by a marker: archive.login_attempts",
            ", and 1 of them declares no bound here: archive.login_attempts",
        ),
        // And a quoted name, which a fold applied to both sides gets wrong.
        (
            &["SOMETHING_UNBOUNDED"],
            ", and 1 excused by a marker: SOMETHING_UNBOUNDED",
            ", and 1 of them declares no bound here: SOMETHING_UNBOUNDED",
        ),
    ];

    #[test]
    fn a_hatch_that_covers_something_is_said_in_the_line_that_counts_it() {
        let mut wrong = Vec::new();
        for (tables, by_a_marker, by_a_pack) in WHAT_THE_HATCH_LINE_SAYS {
            let owned: Vec<String> = tables.iter().map(|it| (*it).to_owned()).collect();
            let said = crate::check::excused_by_a_marker_for_tests(&owned);
            if said != by_a_marker {
                wrong.push(format!(
                    "  {tables:?} — the marker half said `{said}`, and it says `{by_a_marker}`"
                ));
            }
            let said = crate::check::the_pack_exemption_in_for_tests(tables);
            if said != by_a_pack {
                wrong.push(format!(
                    "  {tables:?} — the pack half said `{said}`, and it says `{by_a_pack}`"
                ));
            }
        }
        verdict(&wrong, "check::the record-limits hatch line");
    }

    /// And the line the shipped catalog actually produces names every table the
    /// catalog exempts — the floor the two above cannot be, because a written
    /// list is not the catalog.
    #[test]
    fn every_table_a_pack_exempts_is_named_in_the_line() {
        let exempt: Vec<&str> = nc_catalog::TABLES
            .iter()
            .filter(|table| table.strategy == nc_catalog::Strategy::PolicyPack)
            .map(|table| table.name)
            .collect();
        assert!(
            !exempt.is_empty(),
            "no table in the shipped catalog is exempted by a pack, so the line below is a \
             statement about nothing — and `journal_entries` is one"
        );
        let said = crate::check::exempt_by_a_pack_for_tests();
        for table in &exempt {
            assert!(
                said.contains(table),
                "`{table}` is exempted by a pack and the record-limits line does not name \
                 it: `{said}`. That line is the only place a run says the table declares \
                 no bound"
            );
        }
    }

    /// One line of `docker compose ps -a`, and what it says about that service.
    ///
    /// **Acceptance criterion 10's health loop, entered by no other test.** The
    /// `dev::` tests reach the log store's status line, the project name,
    /// `--down`, the search path and the forward-only rule, and not the function
    /// that decides whether the stack is up. Measured, `health != "healthy"`
    /// replaced by `health == "unhealthy"`: `check passed`, and a cold
    /// `cargo xtask dev` reported `ok 2 service(s) healthy` against a
    /// PostgreSQL still initialising, then failed on the first query.
    ///
    /// Here, beside the other tables a weakening is judged against:
    /// `xtask/mutants.py` runs `corpus::` for `dev.rs`, as well as `limits::`
    /// and `dev::`.
    ///
    /// **`starting` is "not yet" and not a failure.** A cold stack reports it on
    /// the first poll, so a rule that refused it would fail every first `dev`,
    /// which is the other way to get this wrong.
    const HOW_A_SERVICE_STANDS: [(&str, &str); 9] = [
        ("postgres running healthy", "healthy"),
        // A second healthy service, so "the first line decides" is not enough.
        ("logs running healthy", "healthy"),
        ("postgres running starting", "not yet: postgres is starting"),
        (
            "postgres running unhealthy",
            "not yet: postgres is unhealthy",
        ),
        // Running and declaring no healthcheck: permanent, and the 120 s it
        // costs when read as `starting` is why it is its own answer.
        ("logs running", "no healthcheck: logs"),
        // Not running, whatever the health column says. `-a` is what makes an
        // exited container appear here rather than vanish from the list.
        ("postgres exited", "not yet: postgres is exited"),
        (
            "postgres restarting healthy",
            "not yet: postgres is restarting",
        ),
        // A name and nothing else — a state column compose left empty.
        ("postgres", "not yet: postgres is not running"),
        // No name either, which is what a blank-ish line reaches this with.
        ("", "not yet: ? is not running"),
    ];

    #[test]
    fn every_line_the_health_wait_reads_is_judged_the_same_way() {
        let mut wrong = Vec::new();
        for (line, expected) in HOW_A_SERVICE_STANDS {
            let mut said = Vec::new();
            let answered = match crate::dev::standing_of(line, &mut said) {
                crate::dev::Standing::Healthy => "healthy".to_owned(),
                crate::dev::Standing::NotYet => format!("not yet: {}", said.join(" and ")),
                crate::dev::Standing::NoHealthcheck(service) => {
                    format!("no healthcheck: {service}")
                }
            };
            if answered != expected {
                wrong.push(format!(
                    "  `{line}` — the wait answered `{answered}`, and it is `{expected}`"
                ));
            }
        }
        verdict(&wrong, "dev::standing_of");
    }

    /// `(the version on disk, whether it is backdated)`, against a database that
    /// has applied 1, 2 and 12.
    ///
    /// One input per clause and per boundary: below and unapplied, below and
    /// already applied, the highest itself, and above everything.
    const BACKDATED: [(i64, bool); 8] = [
        // New and below the highest — the defect the rule exists for.
        (0, true),
        (3, true),
        (11, true),
        // Already applied, so running it again is not out of order.
        (1, false),
        (2, false),
        // The highest applied is not below itself.
        (12, false),
        // New and above everything applied, which is the ordinary case.
        (13, false),
        (99, false),
    ];

    #[test]
    fn the_forward_only_predicate_decides_every_clause() {
        let applied = [1_i64, 2, 12];
        let mut wrong = Vec::new();
        for (version, expected) in BACKDATED {
            let answered = crate::dev::is_backdated(version, 12, &applied);
            if answered != expected {
                wrong.push(format!(
                    "  migration {version} against {applied:?} — the rule answered \
                     {answered}, and it is {expected}"
                ));
            }
        }
        verdict(&wrong, "dev::is_backdated");
    }

    // ---------------------------------------------------------------------------
    // 5. `limits::excused`, `limits::is_the_marker` and the statement scanner,
    //    through `limits::every_table_declares_a_bound`.
    //
    // What it decides: whether a table a migration creates has somewhere that
    // bounds its growth. A weakening ships acceptance criterion 5 — `NOCTUA.md
    // §15.8` — with a table of personal records growing without limit and
    // `N table(s) created in the migrations were checked` printed over it. The
    // hatch half is the dangerous half: `is_the_marker` decides whether one
    // comment line turns the rule *off*, and it is 16 lines of the tree today.
    // ---------------------------------------------------------------------------

    /// `(the migration's text, how many tables it leaves unbounded)`.
    ///
    /// `login_attempts` and its neighbours are names `catalog/limits.toml` does
    /// not declare, so each is a table the rule has something to say about.
    const BOUNDS: [(&str, usize); 80] = [
        // **The marker excuses a table a rename creates, as the remedy says it
        // does.** `unbounded` prints one `fix` on both paths — *If it genuinely
        // cannot grow, say so … on a line beginning `-- nc:bounded: <reason>`* —
        // so a hatch reached only from `CREATE TABLE` hands a developer who did
        // exactly what the gate told them the same finding back.
        (
            concat!(
                "CREATE TABLE install_marker (id INT);\n",
                "-- nc:bounded: login_attempts holds one row per install\n",
                "ALTER TABLE install_marker RENAME TO login_attempts;\n"
            ),
            1,
        ),
        // And the other direction: a marker that names a different table does
        // not excuse this one, on the rename path as on the create path.
        (
            concat!(
                "CREATE TABLE install_marker (id INT);\n",
                "-- nc:bounded: journal_entries is written once per boot\n",
                "ALTER TABLE install_marker RENAME TO login_attempts;\n"
            ),
            2,
        ),
        // **`IF` is a non-reserved keyword, so it is a legal name**, and a skip
        // that consumes two tokens after it without asking what they are is
        // wrong. Measured on PostgreSQL 18.6: `CREATE TABLE if (id int)`,
        // `DROP TABLE if`, `ALTER TABLE if RENAME TO landed` and
        // `CREATE SCHEMA if; CREATE TABLE if.t (…)` all succeed. Such a scanner
        // reads the first as creating a table called `int` — a table nobody
        // wrote, which is criterion 5's second half failing in the one way this
        // file's header forbids — and drops the rename whole.
        ("CREATE TABLE if (id INT);\n", 1),
        (
            concat!(
                "CREATE TABLE install_marker (id INT);\n",
                "ALTER TABLE install_marker RENAME TO if;\n",
                "ALTER TABLE if RENAME TO landed;\n"
            ),
            // Three, and every one of them named: `install_marker` created and
            // retired, `if` created by the first rename and retired by the
            // second, `landed` holding the rows at the end. Read as an
            // existence test, the `if` is swallowed and only two are said.
            3,
        ),
        // **And the verb is the token after the name, not the first one like it
        // below.** `set` is a legal column name and `schema` a legal type name,
        // so searching forward reads an `ADD COLUMN` as a table move: measured on
        // PostgreSQL 18.6 the statement adds a column and moves nothing, while a
        // gate searching forward retired `login_attempts` and invented
        // `not.login_attempts`.
        (
            concat!(
                "CREATE TABLE login_attempts (id INT);\n",
                "ALTER TABLE login_attempts ADD COLUMN set schema NOT NULL;\n"
            ),
            1,
        ),
        // **A comment that *describes* the marker is not the marker.** The rule
        // asks that the marker *begin* the comment, and these rows decide it:
        // relaxed to a substring without them, both of these excuse the table
        // below them with the whole of `cargo xtask check` green, measured. The
        // second is this gate's own
        // `fix` text, which is the likeliest comment a developer writes next.
        (
            concat!(
                "-- TODO(#42): decide whether to write `nc:bounded",
                ": login_attempts cannot ever grow`\n",
                "-- by the installer, because it holds one row per install.\n",
                "CREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n"
            ),
            1,
        ),
        (
            concat!(
                "-- If it genuinely cannot grow, say so in the comment block directly\n",
                "-- above it, on a line beginning `-- nc:bounded",
                ": <reason>` — login_attempts here.\n",
                "CREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n"
            ),
            1,
        ),
        // The plain shape.
        ("CREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n", 1),
        // The hatch, and each of the three rules that keep it from being free.
        (
            "-- nc:bounded: login_attempts holds exactly one row\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            0,
        ),
        // The reason must name *this* table, or an excuse drifts onto the next.
        (
            "-- nc:bounded: install_marker holds exactly one row\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            1,
        ),
        // **And the name handed back in the case SQL folds it from.** A
        // "says something besides the name" half using a case-sensitive
        // `replace` excuses this — the string the rule's own paragraph names as
        // the defect, one shift key away.
        (
            "-- nc:bounded: login_attempts LOGIN_ATTEMPTS TODO\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            1,
        ),
        // **And ten spaces, which is not ten characters of prose.** A length
        // half measuring the span the words sit in, rather than the words,
        // accepts them.
        (
            "-- nc:bounded: login_attempts x          y\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            1,
        ),
        // **And ten digits, which is not a sentence either.** A length half
        // counting alphanumerics lets a row of zeroes pay for the prose a reader
        // is owed — the same defect as the row above, one character class over.
        (
            "-- nc:bounded: login_attempts 0000000000 0\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            1,
        ),
        // **And a schema qualifier however SQL writes it.** A clause that makes
        // `archive.login_attempts` a *different* table by asking whether the
        // character before the name is a `.` lets the three spellings
        // PostgreSQL prints and accepts past it, and a reason about another
        // schema's table then excuses an unbounded one in `public`.
        (
            "-- nc:bounded: \"archive\".\"login_attempts\" holds one row per rotation\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            1,
        ),
        (
            "-- nc:bounded: archive.\"login_attempts\" holds one row per rotation\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            1,
        ),
        // **And the three spaced spellings, which are a decision rather than a
        // bug.** A `.` with nothing between it and the name is a qualifier; a
        // `.` followed by a space is the end of a sentence, and the two cannot
        // both be read from the same string. Reading the spaced forms as
        // qualifiers closes a hole nobody writes through and opens one anybody
        // could: `the installer writes it once and never again. login_attempts
        // holds at most one row` is true, about this table, and that reading
        // refuses it. So the spaced forms are not qualifiers, and the three rows
        // below are what the decision gives up, stated where a reader meets
        // them. The tight spellings, which is how a qualified name is actually
        // written, are still refused above.
        // **And a character a reader sees nothing of does not stand between
        // them.** The decision above is about what *stands* between the dot and
        // the name, and asking only whether that character is a dot lets a
        // zero-width space excuse the table while rendering identically to the
        // tight spelling the rows above refuse, measured. A tab is the same
        // shape; a no-break space is not,
        // because it renders as a space, which is what the decision says the end
        // of a sentence looks like.
        (
            "-- nc:bounded: archive.\u{200b}login_attempts holds one row per rotation\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            1,
        ),
        // A tab renders as a gap, so by this rule's own criterion it ends a
        // sentence exactly as a space does. Written down as `1`, this would
        // refuse the sentence the decision above rescues, with one space
        // replaced by a tab.
        (
            "-- nc:bounded: archive.\tlogin_attempts holds one row per rotation\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            0,
        ),
        (
            "-- nc:bounded: the installer writes it once and never again.\tlogin_attempts holds at most one row\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            0,
        ),
        // **The quotes prose actually uses.** A backtick is this repository's
        // house style for an identifier, and a rule that knows no quotes lets
        // it walk past.
        (
            "-- nc:bounded: archive.`login_attempts` holds one row per rotation\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            1,
        ),
        (
            "-- nc:bounded: archive.'login_attempts' holds one row per rotation\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            1,
        ),
        (
            "-- nc:bounded: [archive].[login_attempts] holds one row per rotation\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            1,
        ),
        // **A character that renders as nothing is not a word boundary either.**
        // The qualifier clause knows about them, and boundary clauses reading the
        // raw character would let a reason about a *different* table excuse this
        // one while rendering exactly like one.
        (
            "-- nc:bounded: old_\u{200b}login_attempts holds one row per rotation\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            1,
        ),
        (
            "-- nc:bounded: login_attempts\u{200b}_history holds one row per rotation\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            1,
        ),
        // **And the classes the escapers' table does not hold.** A variation
        // selector and a combining mark render as nothing here and are neither
        // `Cc` nor `Cf`; VS-16 and VS-17 are the same kind of character, and a
        // rule borrowing that array gives them opposite verdicts, because the
        // tag plane happens to sit in an array written for a different question.
        (
            "-- nc:bounded: archive.\u{fe0f}login_attempts holds one row per rotation\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            1,
        ),
        (
            "-- nc:bounded: archive.\u{0301}login_attempts holds one row per rotation\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            1,
        ),
        (
            "-- nc:bounded: archive.\u{e0100}login_attempts holds one row per rotation\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            1,
        ),
        // **The classes a hand-written table missed.** A Mongolian free variation
        // selector, whose neighbour `U+180E` is in such an array; a Devanagari
        // stress mark; and two marks a plane up. Each walks past ranges written
        // by hand, which is why the question is std's.
        (
            "-- nc:bounded: archive.\u{180b}login_attempts holds one row per rotation\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            1,
        ),
        (
            "-- nc:bounded: archive.\u{951}login_attempts holds one row per rotation\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            1,
        ),
        (
            "-- nc:bounded: old_\u{180b}login_attempts holds one row per rotation\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            1,
        ),
        // **And `public.`, which is the one qualifier that cannot denote another
        // relation.** The third row is what says the decision excused one schema
        // name and not the qualifier rule: a table the migration declared inside
        // a schema of its own is still refused.
        (
            "-- nc:bounded: public.login_attempts holds one row per install\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            0,
        ),
        (
            "-- nc:bounded: \"public\".\"login_attempts\" holds one row per install\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            0,
        ),
        // A schema whose quoted name merely *ends* in `public` is another
        // schema: a quoted identifier may hold hyphens, dots and spaces.
        (
            "-- nc:bounded: \"archive-public\".login_attempts holds one row per rotation\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            1,
        ),
        (
            "-- nc:bounded: \"my.public\".login_attempts holds one row per rotation\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            1,
        ),
        // A quoted identifier's name is exactly what stands between its quotes.
        (
            "-- nc:bounded: \"public \".login_attempts holds one row per rotation\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            1,
        ),
        (
            "-- nc:bounded: \"public\u{200b}\".login_attempts holds one row per rotation\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            1,
        ),
        (
            "-- nc:bounded: public.login_attempts holds one row\nCREATE SCHEMA h\n  CREATE TABLE login_attempts (id INT);\n",
            1,
        ),
        (
            "-- nc:bounded: archive.\u{a0}login_attempts holds one row per rotation\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            0,
        ),
        (
            "-- nc:bounded: archive . login_attempts holds one row per rotation\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            0,
        ),
        (
            "-- nc:bounded: archive . \"login_attempts\" holds one row per rotation\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            0,
        ),
        (
            "-- nc:bounded: \"archive\" . \"login_attempts\" holds one row per rotation\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            0,
        ),
        // **And the sentence the decision rescues, which is why it goes that
        // way.** This is the marker an author writes when the reason is a
        // sentence and the table's name follows it. Nothing about it is a
        // qualifier.
        (
            "-- nc:bounded: the installer writes it once and never again. login_attempts holds at most one row\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            0,
        ),
        // **And ten hyphens, which is not a sentence either.** Counting every
        // character that is not whitespace lets punctuation pay for the prose.
        (
            "-- nc:bounded: login_attempts ---------- x\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            1,
        ),
        // **And the table's own name in the case the migration writes it.**
        // SQL folds an unquoted identifier and `Token::resolved` folds with it,
        // so this marker is about this table — and a naming half that is
        // case-sensitive beside a half that folds refuses a true reason with a
        // diagnostic quoting a name the file never wrote.
        (
            "-- nc:bounded: LOGIN_ATTEMPTS holds at most one row per device\nCREATE TABLE LOGIN_ATTEMPTS (id TEXT PRIMARY KEY);\n",
            0,
        ),
        // **And a quoted name, which SQL does *not* fold.** The row above is
        // right because an unquoted identifier is folded by the server and by
        // `Token::resolved` with it. A quoted one keeps its case and is a
        // different relation — `something_unbounded` and `"SOMETHING_UNBOUNDED"`
        // both exist, side by side, on a real server. Folding both sides
        // unconditionally excuses the second with a reason about the first:
        // measured through the shipped gate that way, one migration creating
        // both and one marker naming the lower-case one reported
        // `` `something_unbounded` has no entry in catalog/limits.toml `` in the
        // same run that excused `SOMETHING_UNBOUNDED`. The excuse drifts onto a
        // table it says nothing about, which is what this predicate exists to
        // stop.
        (
            "-- nc:bounded: something_unbounded holds one row, written at install\nCREATE TABLE \"SOMETHING_UNBOUNDED\" (id INT);\n",
            1,
        ),
        // And the same table named as it is written, which is excused.
        (
            "-- nc:bounded: \"SOMETHING_UNBOUNDED\" holds one row, written at install\nCREATE TABLE \"SOMETHING_UNBOUNDED\" (id INT);\n",
            0,
        ),
        // Both at once, which is the shape measured above: one marker, two
        // relations, and only the one it names may be excused.
        (
            "-- nc:bounded: something_unbounded holds one row, written at install\nCREATE TABLE something_unbounded (id INT);\nCREATE TABLE \"SOMETHING_UNBOUNDED\" (id INT);\n",
            1,
        ),
        // An empty reason is not a reason.
        ("-- nc:bounded:\nCREATE TABLE login_attempts (id INT);\n", 1),
        // **The name is not the reason**, and this row decides that clause.
        // Were every row of this table a long reason, switching the clause off
        // in `a_reason_names_what_it_exempts` would leave
        // `cargo test --workspace --locked` at exit 0, measured, and the bare
        // table name would then excuse an unbounded table.
        (
            "-- nc:bounded: login_attempts\nCREATE TABLE login_attempts (id INT);\n",
            1,
        ),
        // **And a name repeated is still a name.** A rule that takes out only
        // the *first* occurrence before counting what is left lets the table
        // written twice pay for the whole thing — measured through the gate that
        // way, both of these were excused, and the second is the string the
        // rule's own paragraph names as the defect.
        (
            "-- nc:bounded: login_attempts login_attempts x\nCREATE TABLE login_attempts (id INT);\n",
            1,
        ),
        (
            "-- nc:bounded: login_attempts TODO login_attempts\nCREATE TABLE login_attempts (id INT);\n",
            1,
        ),
        // **And the mechanism restated is not prose, however often.** With both
        // counts over every word the reason holds, the marker itself written
        // four more times is four words and thirty-six letters and it excuses
        // an unbounded table — the third case the predicate's own doc says it
        // refuses, reached by repetition.
        (
            "-- nc:bounded: login_attempts nc:bounded: nc:bounded: nc:bounded: nc:bounded:\nCREATE TABLE login_attempts (id INT);\n",
            1,
        ),
        // **And dressing a token is not writing another one.** Splitting the
        // reason on whitespace leaves the punctuation attached, so one comma and
        // one stop make three distinct words out of the marker: measured through
        // criterion 5's own driver, this excused an unbounded table while the
        // identical reason without the two marks — the row above — was refused.
        (
            "-- nc:bounded: login_attempts nc:bounded: nc:bounded:, nc:bounded:.\nCREATE TABLE login_attempts (id INT);\n",
            1,
        ),
        // **And a bound stated with a number is a bound.** A length counting
        // letters alone refuses `at most 1 row` — nine letters, four words, true,
        // and the way anybody writes a bound — and its diagnostic blames the
        // word count, which is fine. The other way of being wrong,
        // in the one place this predicate's doc has a paragraph about it.
        (
            "-- nc:bounded: login_attempts at most 1 row\nCREATE TABLE login_attempts (id INT);\n",
            0,
        ),
        // **And a row of digits is still not a sentence**, which is the
        // direction the paragraph above must not undo.
        (
            "-- nc:bounded: login_attempts 0000000000 0\nCREATE TABLE login_attempts (id INT);\n",
            1,
        ),
        // And the direction that would refuse correct code: a true sentence
        // that happens to repeat a word is a true sentence.
        (
            "-- nc:bounded: login_attempts one row per install, one row only\nCREATE TABLE login_attempts (id INT);\n",
            0,
        ),
        // **Each half of the rule alone**, so the `||` between them is decided.
        // Two words and three characters exercises the length half only; one
        // word and seventeen characters exercises the word half only. Without
        // both, `||` could become `&&` and every reason that fails exactly one
        // half would be excused.
        (
            "-- nc:bounded: login_attempts a b\nCREATE TABLE login_attempts (id INT);\n",
            1,
        ),
        (
            "-- nc:bounded: login_attempts averyverylongword\nCREATE TABLE login_attempts (id INT);\n",
            1,
        ),
        // And the other direction, because a rule that refuses a short true
        // sentence is wrong where it is harder to notice: this is the shortest
        // reason the repository actually writes.
        (
            "-- nc:bounded: login_attempts holds one row\nCREATE TABLE login_attempts (id INT);\n",
            0,
        ),
        // A comment that *describes* the hatch is not the hatch.
        (
            "-- see nc:bounded: for login_attempts\nCREATE TABLE login_attempts (id INT);\n",
            1,
        ),
        // The block must be contiguous with the statement.
        (
            "-- nc:bounded: login_attempts holds one row\n\nCREATE TABLE login_attempts (id INT);\n",
            1,
        ),
        // ...and a comment line between the marker and the statement is still the
        // block, so the walk upward is the rule and not the line above.
        (
            "-- nc:bounded: login_attempts holds one row\n-- and it is written by the installer\nCREATE TABLE login_attempts (id INT);\n",
            0,
        ),
        // Indentation is not a second spelling.
        (
            "    -- nc:bounded: login_attempts holds one row\nCREATE TABLE login_attempts (id INT);\n",
            0,
        ),
        // **The statement must be the only one on its line**, or one reason
        // excuses two tables. It excuses the one it sits above and the second
        // statement on that line does not inherit it — which is the whole of
        // `shares_its_line`, and it is the second statement that carries it.
        (
            "-- nc:bounded: install_marker holds exactly one row\nCREATE TABLE install_marker (id INT); CREATE TABLE login_attempts (id TEXT);\n",
            1,
        ),
        // ...and with the marker naming the *second* one, neither is excused:
        // the first is not what the reason is about, and the second shares a line.
        (
            "-- nc:bounded: login_attempts holds exactly one row\nCREATE TABLE install_marker (id INT); CREATE TABLE login_attempts (id TEXT);\n",
            2,
        ),
        // Two statements, no marker, are two.
        (
            "CREATE TABLE install_marker (id INT);\nCREATE TABLE login_attempts (id TEXT);\n",
            2,
        ),
        // The statement is the unit, not the line: four shapes of valid
        // PostgreSQL that a line-by-line reading walks straight through.
        (
            "CREATE TABLE\n    login_attempts (id TEXT PRIMARY KEY);\n",
            1,
        ),
        ("CREATE TABLE IF NOT EXISTS login_attempts (id INT);\n", 1),
        ("/* CREATE TABLE login_attempts (id INT); */\n", 0),
        ("-- CREATE TABLE login_attempts (id INT);\n", 0),
        // A `CREATE TABLE` written inside a string literal is data.
        (
            "CREATE TABLE install_marker (id INT);\nINSERT INTO install_marker VALUES ('CREATE TABLE login_attempts (id INT)');\n",
            1,
        ),
        // A quoted identifier is one name, and flattening on whitespace makes it
        // two — a finding naming a table nobody wrote.
        ("CREATE TABLE \"login attempts\" (id INT);\n", 1),
        // A schema is part of the name.
        ("CREATE TABLE archive.login_attempts (id INT);\n", 1),
        // A rename creates a table and retires one, and both directions count.
        (
            "CREATE TABLE install_marker (id INT);\nALTER TABLE install_marker RENAME TO login_attempts;\n",
            2,
        ),
        // A table the catalog already bounds is not a finding.
        (
            "CREATE TABLE access_records (id TEXT NOT NULL, recorded_at TIMESTAMPTZ NOT NULL);\n",
            0,
        ),
        // Neither is something that is not a table.
        // **A schema qualifier is a different table.** `references::names`
        // accepts a preceding `.` because in a path it is a separator; in SQL it
        // names another schema's table, so a rule borrowing it lets this excuse
        // a bare `login_attempts` in `public` — the excuse drifting onto a new
        // table, which is the
        // failure the naming requirement exists to stop.
        (
            "-- nc:bounded: archive.login_attempts holds exactly one row\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            1,
        ),
        // **A marker excuses one table, not a statement full of them.** This is
        // the case where one statement declares two tables, and it decides
        // `excused()`'s `alone_in_its_statement` clause. Without it the clause
        // can be dropped with the suite green, measured: a test whose fixture
        // writes the marker about a bare name, while the parser produces a
        // schema-qualified one, passes for a different reason — the naming rule
        // refuses the excuse before this clause is asked.
        (
            "-- nc:bounded: h.install_marker holds exactly one row\nCREATE SCHEMA h\n  CREATE TABLE install_marker (id INT)\n  CREATE TABLE login_attempts (id INT);\n",
            2,
        ),
        // **Two occurrences, one per half.** With `names` taking the bare
        // mention inside `login_attempts_history` and the `.`-rule taking the
        // qualified one, a reason about another schema's table excuses a table
        // with no entry at all: measured, `unbounded=0 excused=["login_attempts"]`.
        (
            "-- nc:bounded: archive.login_attempts holds one row, unlike login_attempts_history\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            1,
        ),
        // **The boundary *before* the name.** Without it a reason about
        // `old_login_attempts` excuses `login_attempts` — the substring defect
        // `names` exists for, which the trailing-boundary check alone does
        // not cover because the name ends where the longer word ends.
        (
            "-- nc:bounded: old_login_attempts holds exactly one row\nCREATE TABLE login_attempts (id TEXT PRIMARY KEY);\n",
            1,
        ),
        ("CREATE VIEW login_attempts AS SELECT 1;\n", 0),
        ("CREATE INDEX login_attempts ON access_records (id);\n", 0),
    ];

    /// Literals that put a file on this machine into generated output, and
    /// literals that only look like one.
    ///
    /// **The clauses of `determinism.rs`'s rule, decided by the suite the
    /// mutation runner executes.** A doubled slash, an interpolated placeholder,
    /// a climb that starts a segment in rather than at the front — each is one
    /// character wide, and without these rows none of them is decided by
    /// anything a weakening reaches.
    ///
    /// Driven through `nothing_the_emitter_reads_changes`, the real entry point,
    /// so a filter between the walk and the judgement fails these too.
    const MACHINE_PATHS: [(&str, usize); 12] = [
        // **Not `/etc/`, which a needle rule beside this one claims first.**
        // One finding is reported per line, so a fixture naming that directory
        // would pin the needle and never reach the rule this drives.
        ("let here = \"/var/lib/dbus/machine-id\";\n", 1),
        // POSIX resolves `//x` and `///x` to `/x`, so a slash too many is the
        // same file. A rule that keeps the emitter's own doc comments out by
        // reading the slashes, rather than what follows them, misses it.
        ("let here = \"//var/lib/dbus/machine-id\";\n", 1),
        ("let here = \"///var/lib/dbus/machine-id\";\n", 1),
        // What follows the slashes in a format string is a placeholder.
        ("let here = format!(\"/{dir}/machine-id\");\n", 1),
        // A climb reaches the same files, and a `..` is a segment wherever it is.
        ("let here = \"../../../var/lib/dbus/machine-id\";\n", 1),
        ("let here = \"./../../../var/lib/dbus/machine-id\";\n", 1),
        ("let here = \"src/../../../var/lib/dbus/machine-id\";\n", 1),
        // And the shapes it must not refuse. A gate green because nothing
        // legitimate has tried yet is the other way to be wrong.
        ("// a doubled slash in a comment is a comment\n", 0),
        ("let here = \"..and then some prose about paths\";\n", 0),
        ("let here = \"relative/inside/the/repository.rs\";\n", 0),
        ("let here = \"///\\n\";\n", 0),
        ("let here = \"//!\";\n", 0),
    ];

    /// Every clause of the absolute-path rule is decided by a literal here.
    #[test]
    fn the_absolute_path_predicate_decides_every_clause() {
        let mut wrong = Vec::new();
        for (code, expected) in MACHINE_PATHS {
            let scratch = Scratch::new();
            // The places the emitter writes from. All but one are directories
            // and one is a file, and the gate refuses a root where any is
            // missing — which is itself the floor that says it inspected
            // something.
            scratch.write("crates/nc-catalog-compiler/src/lib.rs", code);
            scratch.write("crates/nc-canon/src/lib.rs", "");
            scratch.write("crates/nc-schema/src/lib.rs", "");
            scratch.write("xtask/src/build.rs", "");
            let (findings, read) =
                crate::determinism::nothing_the_emitter_reads_changes(scratch.root())
                    .expect("a readable scratch");
            let found = of_kind(&findings, "an absolute path reaches generated output");
            if found != expected {
                wrong.push(format!(
                    "  {code:?} — the gate found {found} absolute path(s), and it leaves \
                     {expected}"
                ));
            }
            // **And it read every line it was given.** `read` is a count of
            // lines, and the fixture's own line count is a number this file
            // arrives at without asking the gate — which is what makes it worth
            // comparing. A walk that stops early, or a filter between the
            // counter and the judgement, moves one and not the other.
            assert_eq!(
                read,
                code.lines().count(),
                "the walk read {read} line(s) of the {} it was given: {code:?}",
                code.lines().count()
            );
        }
        verdict(&wrong, "the absolute-path rule");
    }

    /// Statements the redirect rule must refuse, and shapes it must not.
    ///
    /// **The gate this drives is decided here, and wired.** Without a corpus it
    /// can be wired to nothing unnoticed: measured, handed the first 400 bytes of
    /// every migration it refused nothing, and `cargo xtask check`'s output was
    /// byte-identical to a full run. A clause with no row here is one the suite
    /// cannot settle.
    ///
    /// Driven through `every_table_declares_a_bound`, the real caller, so what is
    /// covered is the wire as well as the rule: a filter between the reader and
    /// the judgement fails these too.
    ///
    /// Every refused shape below is measured against PostgreSQL 18.6, and every
    /// allowed one parses.
    const REDIRECTS: [(&str, usize); 52] = [
        ("SET search_path = archive;\nCREATE TABLE t (id INT);\n", 1),
        ("SET search_path TO archive;\nCREATE TABLE t (id INT);\n", 1),
        ("SET search_path=archive;\nCREATE TABLE t (id INT);\n", 1),
        (
            "SET LOCAL search_path TO archive;\nCREATE TABLE t (id INT);\n",
            1,
        ),
        ("RESET search_path;\nCREATE TABLE t (id INT);\n", 1),
        ("RESET ALL;\nCREATE TABLE t (id INT);\n", 1),
        ("ALTER DATABASE noctua SET search_path TO archive;\n", 1),
        ("ALTER ROLE noctua SET search_path TO archive;\n", 1),
        ("SELECT set_config('search_path', 'archive', false);\n", 1),
        (
            "SELECT set_config('search' || '_path', 'archive', false);\n",
            1,
        ),
        // The statements that re-aim resolution without naming the path. The
        // default path opens with `"$user"`, which PostgreSQL re-expands per
        // statement, so these move where an unqualified `CREATE TABLE` lands
        // while `SHOW search_path` is unchanged.
        ("SET ROLE archive;\nCREATE TABLE t (id INT);\n", 1),
        ("SET LOCAL ROLE archive;\nCREATE TABLE t (id INT);\n", 1),
        (
            "SET SESSION AUTHORIZATION archive;\nCREATE TABLE t (id INT);\n",
            1,
        ),
        (
            "SET session_authorization = archive;\nCREATE TABLE t (id INT);\n",
            1,
        ),
        ("DISCARD ALL;\nCREATE TABLE t (id INT);\n", 1),
        // **Not at token zero.** A routine body's statements open with `BEGIN`,
        // so a `DO` block moves the write off position zero; `ALTER DATABASE`
        // puts it in `pg_db_role_setting`, where every future session reads it.
        (
            "DO $$ BEGIN SET ROLE archive; END $$;\nCREATE TABLE t (id INT);\n",
            1,
        ),
        ("ALTER DATABASE noctua SET role = archive;\n", 1),
        // **A rider on the line the hand-clearance hatch clears.** The hatch
        // matches that statement by equality; relaxed to a substring, this line
        // carries a redirect through it.
        (
            "SELECT set_config('search_path', regexp_replace(current_setting('search_path'), \
             ', pg_temp$', ''), false); SELECT set_config('search_path', 'archive', false);\n",
            // Two, because the hatch clears neither: it matches the statement it
            // names by equality, and this line is not that statement. Relaxed to
            // a substring it clears *both*, which is the defect.
            2,
        ),
        // **And the hatch clearing what it exists to clear.** Every case above
        // is the hatch refusing; without this, the direction it exists for is
        // decided only by `migrations/0011_a_trigger_names_its_own_schema.sql`,
        // and nothing in this corpus holds it open. A
        // hatch tested in one direction is an exclusion list nobody tried.
        (
            concat!(
                "SELECT set_config('search_path', ",
                "regexp_replace(current_setting('search_path'), ', pg_temp$', ''), false);\n"
            ),
            0,
        ),
        // **With a comment riding on its line**, which is what the strip in
        // `read_by_hand` is for and what nothing else exercises: dropping
        // `without_comments` there leaves the rest of the suite green, because the one
        // cleared statement in this repository carries no `--` on either of its
        // lines. Without the strip the comment's words join the statement's, the
        // equality fails, and a migration this repository actually writes is
        // reported — the hatch narrowing until it refuses something legitimate.
        (
            concat!(
                "SELECT set_config('search_path', ",
                "regexp_replace(current_setting('search_path'), ', pg_temp$', ''), false); ",
                "-- and pg_temp is off the path again\n"
            ),
            0,
        ),
        // And the shapes a migration in this repository actually writes, which
        // this rule must not refuse. A gate green because nothing legitimate has
        // tried yet is the other way to be wrong.
        (
            "SELECT set_config('search_path', quote_ident(current_schema()), false);\n",
            0,
        ),
        (
            "SELECT set_config('search_path', current_setting('search_path') || ', pg_temp', \
             false);\n",
            0,
        ),
        ("CREATE TABLE t (id INT);\n", 0),
        // **`SET` is a non-reserved word.** A schema may be called it, and a
        // statement may end where this rule reads a value: both of these can
        // make a gate abort with a Rust slice message rather than produce a
        // finding. Measured on PostgreSQL 18.6 — `CREATE SCHEMA set;` and a
        // table in it both succeed.
        ("CREATE VIEW v AS SELECT c FROM set.t;\n", 0),
        ("SET nc.p;\n", 0),
        // **An upsert and a merge write a column, not a setting.** Both
        // measured on PostgreSQL 18.6: the row is written and `SHOW search_path`
        // is unchanged. The third is the second with the column dropped from the
        // `INSERT` list — the shape that tells a fixture passing for the right
        // reason from one passing for a reason of its own.
        (
            "INSERT INTO paths VALUES (1) ON CONFLICT (id) DO UPDATE SET search_path = 'a';\n",
            0,
        ),
        (
            "MERGE INTO paths p USING s ON p.id = s.id WHEN MATCHED THEN UPDATE \
             SET search_path = 'a';\n",
            0,
        ),
        (
            "INSERT INTO paths (id) VALUES (1) ON CONFLICT (id) DO UPDATE SET search_path = 'b';\n",
            0,
        ),
        // **Deeper than any migration's first statement.** The count the two
        // readers compare certifies how much was *read*, never that a verdict
        // survived — so a one-line filter on the output silences every refusal
        // below line 24 with the two numbers equal to the byte, measured. Nine of the
        // twelve migrations begin below that line and the deepest begins at 68,
        // so a case has to be deeper than the tree it stands for. This one puts
        // its redirect at line 71.
        (
            "-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\n-- a line of the prose every migration in this tree opens with.\nSET search_path = archive;\\nCREATE TABLE t (id INT);\\n",
            1,
        ),
        // **The other three things this reader refuses.** These can be written
        // down here only because the filter above reads every verdict rather
        // than the redirect alone; without them, the whole of the
        // `COPY … FROM stdin`, `IMPORT FOREIGN SCHEMA` and dynamic-`EXECUTE`
        // rules has no mutation coverage.
        ("COPY t FROM stdin;\n", 1),
        ("IMPORT FOREIGN SCHEMA remote INTO public;\n", 1),
        (
            "DO $$ BEGIN EXECUTE format('CREATE TABLE %I (id INT)', 't'); END $$;\n",
            1,
        ),
        // A statement whose last token is the word, which the pair reader cannot
        // see because a one-token statement has no pairs.
        ("EXECUTE;\n", 1),
        // And the shapes beside them that must not be refused: a privilege is
        // followed by `ON` or by a comma, a trigger by `FUNCTION`.
        ("GRANT EXECUTE ON FUNCTION nc.probe(text) TO PUBLIC;\n", 0),
        (
            "CREATE TRIGGER t AFTER INSERT ON paths EXECUTE FUNCTION nc.probe();\n",
            0,
        ),
        (
            "CREATE TRIGGER u AFTER INSERT ON paths EXECUTE PROCEDURE nc.probe();\n",
            0,
        ),
        (
            "ALTER DEFAULT PRIVILEGES IN SCHEMA nc REVOKE EXECUTE ON FUNCTIONS FROM PUBLIC;\n",
            0,
        ),
        // **And the word as an ordinary name.** `EXECUTE` is non-reserved on
        // PostgreSQL 18.6, so a table and a column may both be called it — and
        // a rule reading the word alone refuses each of these three as dynamic
        // SQL, which tells a developer a column declaration builds a statement
        // out of text and
        // offers a hatch on a statement that needs none.
        ("CREATE TABLE execute (id INT);\n", 0),
        ("CREATE TABLE t (execute INT);\n", 0),
        ("ALTER TABLE t RENAME COLUMN a TO execute;\n", 0),
        // A `set_config` whose name this gate cannot read is its own refusal, and
        // a signature is not a call.
        ("SELECT set_config(knob, 'off', false);\n", 1),
        (
            "REVOKE ALL ON FUNCTION set_config(text, text, boolean) FROM PUBLIC;\n",
            0,
        ),
        // **A column may be called `search_path`.** Neither statement writes a
        // setting, and the clauses that tell them apart from a redirect — a
        // `SET` or a `RESET` is present, and an operator follows the name — are
        // each deletable with the whole suite green without these rows,
        // measured.
        (
            "CREATE VIEW v AS SELECT * FROM paths WHERE search_path = 'archive';\n",
            0,
        ),
        (
            "ALTER TABLE paths ALTER COLUMN search_path SET NOT NULL;\n",
            0,
        ),
        // And writing to that column. No statement writes a session setting
        // and rows at once, which is what tells this `SET` from the others.
        ("UPDATE paths SET search_path = 'archive';\n", 0),
        // **Past the longest migration this repository has.** The scope case
        // above stops at ten statements, and `0012` holds twenty-two — so
        // without this a rule switched off for the tail of a real file is
        // invisible here while the suite stays green. A ceiling below the thing
        // it is written to cover
        // is not a ceiling.
        (
            "SELECT 1;\nSELECT 1;\nSELECT 1;\nSELECT 1;\nSELECT 1;\nSELECT 1;\nSELECT 1;\nSELECT 1;\nSELECT 1;\nSELECT 1;\nSELECT 1;\nSELECT 1;\nSELECT 1;\nSELECT 1;\nSELECT 1;\nSELECT 1;\nSELECT 1;\nSELECT 1;\nSELECT 1;\nSELECT 1;\nSELECT 1;\nSELECT 1;\nSELECT 1;\nSET search_path = archive;\\nCREATE TABLE t (id INT);\\n",
            1,
        ),
        // **And the two shapes that hatch must not let through.** `UPDATE` is
        // an *unreserved* keyword, so a schema may be called it; and PL/pgSQL
        // puts an `UPDATE` and a later `SET` in one `;`-delimited chunk. Both are
        // measured on PostgreSQL 18.6: the path moves and an unqualified
        // `CREATE TABLE` lands in the new schema.
        ("SET search_path = update;\nCREATE TABLE t (id INT);\n", 1),
        (
            "DO $$ DECLARE r record; BEGIN FOR r IN UPDATE paths SET n = n + 1 \
             RETURNING * LOOP SET search_path = archive; END LOOP; END $$;\n",
            1,
        ),
        // **And a CTE in the loop's header.** PL/pgSQL's `FOR r IN <query> LOOP
        // <stmt>;` header holds no `;`, so a `(UPDATE …)` there and a
        // `SET search_path` in the body are one chunk — and the arm that clears
        // a column write must not answer for the whole of it. Measured on PostgreSQL
        // 18.6: the path moved and an unqualified `CREATE TABLE` landed in the
        // new schema.
        (
            "DO $$ DECLARE r record; BEGIN FOR r IN WITH bumped AS (UPDATE paths \
             SET n = n + 1 RETURNING n) SELECT n FROM bumped LOOP \
             SET search_path = archive; END LOOP; END $$;\n",
            1,
        ),
        // **A conflict action wears an opener's clothes.** `ON CONFLICT … DO
        // UPDATE SET` and `MERGE … WHEN MATCHED THEN UPDATE SET` are clauses of
        // the statement above them, not new statements — and a rule that takes
        // either for an opener inside a `FOR … IN` loop turns itself off for a
        // chunk holding a redirect.
        (
            "DO $$ DECLARE r record; BEGIN FOR r IN INSERT INTO paths VALUES (1) \
             ON CONFLICT (id) DO UPDATE SET n = 1 RETURNING * LOOP \
             SET search_path = archive; END LOOP; END $$;\n",
            1,
        ),
        // **Past the eighth statement.** Every other case here is one or two
        // statements, and a rule switched off for the tail of a file agrees with
        // all of them. Real migrations hold nine to twenty-two: `0001` has nine
        // and `0012` has twenty-two, so the tail is most of each.
        (
            "SELECT 1;\nSELECT 1;\nSELECT 1;\nSELECT 1;\nSELECT 1;\nSELECT 1;\nSELECT 1;\nSELECT 1;\nSET search_path = archive;\nCREATE TABLE t (id INT);\n",
            1,
        ),
        // **Past the first few hundred bytes.** Every other case here is one
        // short line, and a reader handed only the head of a file agrees with
        // all of them. Migrations in this tree open with a block of prose, so
        // the first statement of a real one is a long way in — which is exactly
        // where a reader that stops early stops seeing. This header is over four
        // hundred bytes on its own.
        (
            "-- This migration is written the way every migration in this tree is written: a\n\
             -- block of prose above the statements, saying what is being changed and why, so\n\
             -- that a reader who comes to it later does not have to reconstruct the intent\n\
             -- from the SQL. That block is long, and it is meant to be. What it means for a\n\
             -- gate is that the first statement of a migration is rarely near the top of the\n\
             -- file, and a reader that stops early stops before anything it exists to read.\n\
             SET search_path = archive;\n\
             CREATE TABLE t (id INT);\n",
            1,
        ),
    ];

    /// No case in this file is a copy of another.
    ///
    /// **A declaration says how many cases a table holds and not that they are
    /// different ones.** Measured, one row of `SCOPE` copied from the row above
    /// it: the count held, the declaration held, the census that reads that
    /// declaration held — and the clause the row decided was decided by
    /// nothing, with every mutation over it surviving.
    ///
    /// Asked of the values rather than of the source. Reading rows out of the
    /// text is the wrong tool: a comment inside a table
    /// holds unbalanced brackets, a case spans lines, and the strippers that
    /// would remove the comments are not byte-for-byte with their input.
    ///
    /// A case's *first field* may legitimately repeat — the same shape asked two
    /// different questions is two cases — so it is the whole row that must be
    /// distinct.
    ///
    /// **And the tables are this file's, not a list somebody kept up.** A table
    /// left off a hand list is the one place in this file where a copy costs
    /// nothing. Measured with `CARETS` left off — a real weakening of `wall.rs`'s
    /// local-declaration guard (`== Some(short)` to `== Some(door)`), paid for by
    /// replacing one `CARETS` row with a copy of the row above it — the suite
    /// went from one RED test to green, with the declared length and every
    /// census over it intact. Any table forgotten the same way is the same hole,
    /// so the names are read from the declarations and the check is that the
    /// two agree.
    #[test]
    fn no_case_is_a_copy_of_another() {
        /// Every case of one table, and none of them twice.
        ///
        /// Records the table's name as it checks it, so what is compared against
        /// the declarations below is what was actually handed here and not a
        /// second list written beside the calls. Handed back rather than
        /// returned, because the log's type wall refuses a function that
        /// produces a `&'static str` — and it is right to: this one is given
        /// one, but the reader that has to tell the two apart is the wall
        /// itself.
        fn distinct<T: core::fmt::Debug + Ord>(
            checked: &mut std::collections::BTreeSet<&'static str>,
            table: &'static str,
            rows: impl IntoIterator<Item = T>,
        ) {
            let mut seen = std::collections::BTreeSet::new();
            let copied: Vec<T> = rows
                .into_iter()
                .filter(|row| !seen.insert(row_key(row)))
                .collect();
            assert!(
                copied.is_empty(),
                "`{table}` holds {} case(s) copied from another, so it decides that many \
                 clauses fewer than its length claims: {copied:#?}",
                copied.len()
            );
            checked.insert(table);
        }
        /// A case as a comparable key, which is how it is written.
        fn row_key<T: core::fmt::Debug>(row: &T) -> String {
            format!("{row:?}")
        }

        let mut checked: std::collections::BTreeSet<&'static str> =
            std::collections::BTreeSet::new();
        distinct(&mut checked, "ENVIRONMENTS", ENVIRONMENTS);
        distinct(&mut checked, "HAND_WRITTEN", HAND_WRITTEN);
        distinct(&mut checked, "BLANKET", BLANKET);
        distinct(&mut checked, "REASONS", REASONS);
        distinct(&mut checked, "HIDDEN", HIDDEN);
        distinct(&mut checked, "BACKDATED", BACKDATED);
        distinct(&mut checked, "HOW_A_SERVICE_STANDS", HOW_A_SERVICE_STANDS);
        distinct(
            &mut checked,
            "WHAT_THE_HATCH_LINE_SAYS",
            WHAT_THE_HATCH_LINE_SAYS,
        );
        distinct(&mut checked, "THE_HIGHEST_APPLIED", THE_HIGHEST_APPLIED);
        distinct(&mut checked, "BOUNDS", BOUNDS);
        distinct(&mut checked, "COUNTS", COUNTS);
        distinct(&mut checked, "CARETS", CARETS);
        distinct(&mut checked, "DECLARATIONS", DECLARATIONS);
        distinct(&mut checked, "SCOPE", SCOPE);
        distinct(&mut checked, "COMPOSE_FILES", COMPOSE_FILES);
        distinct(&mut checked, "REDIRECTS", REDIRECTS);
        distinct(&mut checked, "MACHINE_PATHS", MACHINE_PATHS);

        // Every table this module declares, read from its own text. A `const`
        // at module level whose type opens an array is a corpus table; the one
        // inside a function — a census expectation compared for equality, which
        // fails on a duplicate by itself — is indented past this and is not one.
        let declared: std::collections::BTreeSet<&'static str> = include_str!("corpus.rs")
            .lines()
            .filter_map(|line| line.strip_prefix("    const "))
            .filter(|rest| rest.contains(": ["))
            .filter_map(|rest| rest.split(':').next())
            .collect();
        assert_eq!(
            checked, declared,
            "a corpus table is declared and its rows are not compared, or a name here \
             names no table. The rows of an unchecked table can be copied from one \
             another with the declared length and every census over it intact, which is \
             the defect this whole test exists for"
        );
    }

    /// Every clause of the redirect rule is decided by a statement here.
    #[test]
    fn the_redirect_predicate_decides_every_clause() {
        let mut wrong = Vec::new();
        for (sql, expected) in REDIRECTS {
            let scratch = Scratch::new();
            scratch.write("migrations/0001_probe.sql", sql);
            let (findings, _, _, _) = crate::limits::every_table_declares_a_bound(scratch.root())
                .expect("a readable scratch");
            // The refusal reader's own verdicts. The other two kinds this step
            // produces are about the *table* census — a table with no entry, an
            // entry with no table — and in a scratch tree holding one probe file
            // they say only that `catalog/limits.toml` describes the real
            // repository, which is not what these cases are written to decide.
            let found = findings
                .iter()
                .filter(|finding| {
                    !finding.what.contains("has no entry in")
                        && !finding.what.contains("is bounded and nothing creates it")
                })
                .count();
            if found != expected {
                wrong.push(format!(
                    "  {sql:?} — the gate found {found} redirect(s), and it leaves {expected}"
                ));
            }
            // **And the reader read the whole file**, which the `expect` above
            // carries: the count the refusal reader publishes is held against
            // the reader beside it, and a disagreement is an `Err` rather than a
            // finding. As a finding, scanned for by its text in this loop, it
            // would be silenced together with this assertion by one filter on
            // the verdicts.
            // Without either, a defect that stops the scan early leaves
            // `found == 0` on every refused row, reading exactly like a rule
            // that was never reached.
        }
        verdict(&wrong, "the redirect rule");
    }

    #[test]
    fn the_unbounded_table_predicate_decides_every_clause() {
        let mut wrong = Vec::new();
        for (sql, expected) in BOUNDS {
            let scratch = Scratch::new();
            scratch.write("migrations/0001_probe.sql", sql);
            let (findings, _, _, _) = crate::limits::every_table_declares_a_bound(scratch.root())
                .expect("a readable scratch");
            let found = of_kind(&findings, "has no entry in catalog/limits.toml");
            if found != expected {
                wrong.push(format!(
                    "  {sql:?} — the gate found {found} unbounded table(s), and it leaves \
                     {expected}"
                ));
            }
            // **And the error names it**, which is the other half of acceptance
            // criterion 5: counting findings by kind never reads which table one
            // names. Measured, with `limits.rs`'s message changed from
            // `` format!("`{name}` has no entry …") `` to a fixed
            // `"a table has no entry …"`, this driver read that way — the
            // command `xtask/canaries/README.md` names as the answer to
            // criterion 5 — reported `1 passed`.
            for finding in &findings {
                if !finding.what.contains("has no entry in catalog/limits.toml") {
                    continue;
                }
                // The name the refusal quotes has to be a name the migration
                // wrote. Compared that way round rather than by re-parsing the
                // SQL: this file's job is to decide the gate's verdict, not to
                // keep a second parser that would have to learn `CREATE TABLE`
                // across a newline, a quoted identifier and `RENAME TO` — all of
                // which `BOUNDS` holds — and a second parser is how two readers
                // come to disagree.
                let quoted = finding
                    .what
                    .split_once('`')
                    .and_then(|(_, rest)| rest.split_once('`'))
                    .map(|(named, _)| named)
                    .unwrap_or_default();
                // **Outside a string literal.** `sql.contains(quoted)` alone is
                // satisfied by a name that appears in an `INSERT … VALUES
                // ('CREATE TABLE login_attempts …')` — one of the cases in
                // `BOUNDS` — so a refusal naming the wrong table would pass on
                // exactly the case written to prove the parser ignores prose.
                let outside_a_literal = {
                    let mut kept = String::with_capacity(sql.len());
                    let mut inside = false;
                    for character in sql.chars() {
                        if character == '\'' {
                            inside = !inside;
                            continue;
                        }
                        kept.push(if inside { ' ' } else { character });
                    }
                    kept
                };
                // The bare name, because a schema qualifier is one the parser
                // synthesises: `CREATE SCHEMA h` followed by
                // `CREATE TABLE install_marker` is `h.install_marker` in the
                // refusal and two separate words in the migration.
                let bare = quoted.rsplit('.').next().unwrap_or(quoted);
                if bare.is_empty() || !outside_a_literal.contains(bare) {
                    wrong.push(format!(
                        "  {sql:?} — the refusal is `{}`, which quotes no table this \
                         migration declares. `a table has no entry in \
                         catalog/limits.toml` sends a reader through every migration in \
                         the tree, and acceptance criterion 5 is `a table with no bound \
                         fails, **and the error names it**`",
                        finding.what
                    ));
                }
            }
        }
        verdict(
            &wrong,
            "excused + is_the_marker + every_table_declares_a_bound",
        );
    }

    // ---------------------------------------------------------------------------
    // 6. `gates::pinned_by_digest` and `gates::is_a_build_key`, through
    //    `gates::every_image_is_pinned`.
    //
    // What it decides: whether the stack `cargo xtask dev` brings up — and CI
    // brings up — runs bytes anybody outside this repository can change. A tag is
    // a name its publisher can repoint; a `build:` key is a Dockerfile nothing
    // here reads. A weakening ships that with `N image(s) in the resolved stack
    // and N written in its files, each pinned by digest` printed over it, and the
    // gate's own doc records the ways this reaches the running stack.
    //
    // It drives `docker compose config` in the scratch tree, which is what the
    // shipped gate does — the whole reason the gate exists is that reading one
    // file is not reading the stack.
    // ---------------------------------------------------------------------------

    /// Sixty-four lowercase hex characters, which is what a digest is.
    const DIGEST: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90";

    /// How many, so the array type carries the number and the meta-gate can pin
    /// it.
    ///
    /// **These two case lists are built by functions rather than declared as
    /// tables**, so `the_corpus_still_decides_every_predicate_it_is_written_for`
    /// does not see them as tables — and a function body can be emptied.
    /// `verdict(&wrong, …)` is `assert!(wrong.is_empty())`, which is true of a
    /// loop that never ran, so with `referent_cases()` returning `Vec::new()` and
    /// `one_of_them_is` weakened to a substring test that lets PostgreSQL 16.14
    /// run under the tag `18-alpine`, the whole gate printed `check passed`,
    /// measured with the list returned as a `Vec` rather than an array.
    /// Deleting statements deletes no `#[test]` attribute, so the suite floor
    /// cannot see it either.
    ///
    /// A fixed-size return type makes emptying the body a compile error, and the
    /// pin on this line makes lowering the number a failing test.
    const IMAGE_CASES: usize = 20;

    /// `(the compose file's body, how many findings it produces)`.
    ///
    /// Two passes read the same file — what `docker compose config` resolves and
    /// what the file says — so an image that is unpinned in both is two findings,
    /// and the cases where they differ are the cases the two passes exist for.
    fn image_cases() -> [(String, usize); IMAGE_CASES] {
        [
            // Pinned, with the tag kept in front so a reader can see the version.
            //
            // **No finding, and an announced skip instead.** Nothing has pulled
            // `example.invalid/pinned:1` — nothing can — so its digest cannot be
            // compared with what the tag resolves to. A silent `warn` lets 18's
            // digest ship under a tag reading 17 through a green run, measured;
            // an error makes the gate red on every CI runner, where neither
            // image is pulled. A runner that *does* pull them is green:
            // `docker manifest inspect` gives each tag the same index as the
            // digest beside it, measured.
            //
            // The digest is what pins the bytes and it is judged here whatever
            // the daemon says. What the comparison adds is that the label beside
            // them is still honest, and where that cannot be asked the honest
            // report is that it was not — announced, so the verdict counts it
            // among the steps that did not run. A wrong digest against a tag
            // that *is* pulled is still an error, which is the case below.
            (
                format!("services:\n  a:\n    image: example.invalid/pinned:1@sha256:{DIGEST}\n"),
                0,
            ),
            // **The key's own spelling.** YAML allows whitespace before the
            // colon and docker resolves it, while `strip_prefix("image:")`
            // reads one spelling — measured with that reader, one space added
            // in `docker-compose.yml`:
            // `ok 2 image(s) in the resolved stack and 1 written in its files`,
            // the written count halved and the step still saying `ok`. A
            // quoted key is the other spelling.
            (
                "services:\n  a:\n    image : postgres:latest\n".to_owned(),
                2,
            ),
            (
                "services:\n  a:\n    \"image\": postgres:latest\n".to_owned(),
                2,
            ),
            // A tag is not a pin.
            (
                "services:\n  a:\n    image: example.invalid/pinned:1\n".to_owned(),
                2,
            ),
            (
                "services:\n  a:\n    image: postgres:latest\n".to_owned(),
                2,
            ),
            // `contains(\"@sha256:\")` is not a digest test: one input per clause
            // of the length, the alphabet and the name.
            (
                "services:\n  a:\n    image: example.invalid/pinned:1@sha256:deadbeef\n".to_owned(),
                2,
            ),
            (
                format!(
                    "services:\n  a:\n    image: example.invalid/pinned:1@sha256:{}\n",
                    DIGEST.to_uppercase()
                ),
                2,
            ),
            (
                format!(
                    "services:\n  a:\n    image: example.invalid/pinned:1@sha256:{}g\n",
                    &DIGEST[..63]
                ),
                2,
            ),
            (
                format!(
                    "services:\n  a:\n    image: example.invalid/pinned:1@sha256:{}\n",
                    &DIGEST[..63]
                ),
                2,
            ),
            (
                format!("services:\n  a:\n    image: example.invalid/pinned:1@sha256:{DIGEST}0\n"),
                2,
            ),
            // **A digest with no name in front of it never reaches either pass.**
            // `docker compose config` refuses the file, `every_image_is_pinned`
            // returns on that, and the file pass — which is inside the same
            // function, after the early return — never runs. So the
            // `!name.is_empty()` clause of `pinned_by_digest` is unreachable
            // through the shipped entry point, and this case says so rather than
            // pretending the predicate decided it.
            (format!("services:\n  a:\n    image: @sha256:{DIGEST}\n"), 1),
            // A built service has no digest at all, and nothing here reads a
            // `FROM` line.
            (
                "services:\n  a:\n    build:\n      context: .\n".to_owned(),
                2,
            ),
            // Flow style is YAML too: one line that is neither `build:` nor
            // `build: `.
            ("services:\n  a: {build: {context: \".\"}}\n".to_owned(), 2),
            // ...and with no space after the colon, which is the spelling
            // `starts_with(\"build: \")` alone does not see. YAML permits it when
            // the key is quoted, and compose reads it.
            (
                "services:\n  a: {\"build\":{\"context\": \".\"}}\n".to_owned(),
                2,
            ),
            // **A quoted `build:` key, in block style.** The flow-style case
            // above reaches `is_a_build_key` through the `{` split; this one is
            // the plain spelling, and it is what keeps the predicate's own
            // unquoting reachable. A file pass handing it an already-unquoted
            // line leaves the clause decided by no input, and the meta-gate then
            // reports a weakening of it as surviving.
            (
                "services:\n  a:\n    \"build\":\n      context: .\n".to_owned(),
                2,
            ),
            // A quoted key is a key; compose normalises the quotes away and the
            // file pass has to as well.
            (
                "services:\n  a:\n    \"image\": example.invalid/pinned:1\n".to_owned(),
                2,
            ),
            (
                "services:\n  a:\n    \"build\":\n      context: .\n".to_owned(),
                2,
            ),
            // **A variable is not a pin, and this is the case the file pass
            // exists for.** `docker compose config` resolves it against whichever
            // environment ran the command, so the resolved pass sees a pinned
            // image and the file does not — one finding, from the file pass
            // alone. The referent is not a second: an image nothing has
            // pulled is an announced skip rather than a defect, for the reason
            // written on the first case above.
            (
                format!(
                    "services:\n  a:\n    image: ${{IMG:-example.invalid/pinned:1@sha256:{DIGEST}}}\n"
                ),
                1,
            ),
            // Two unpinned images are two of each.
            (
                "services:\n  a:\n    image: example.invalid/pinned:1\n  b:\n    image: alpine:3\n"
                    .to_owned(),
                4,
            ),
            // A profile hides a service from the default projection entirely, and
            // `--profile \"*\"` is what brings it back.
            (
                "services:\n  a:\n    image: example.invalid/pinned:1\n    profiles: [\"debug\"]\n"
                    .to_owned(),
                2,
            ),
        ]
    }

    /// How many, for the reason written on [`IMAGE_CASES`].
    const REFERENT_CASES: usize = 10;

    /// One entry of `RepoDigests`, and every way a substring test says yes to it.
    ///
    /// **A referent check written as `listed.contains(digest)` answers the wrong
    /// question.** `RepoDigests` is a list of `repository@sha256:…`, one per
    /// repository the image is tagged into, so a substring match answers "this
    /// digest is somewhere in this list" — not "this tag resolves to this
    /// digest". This is the clause that decides it, and without this corpus it
    /// has no case: measured, the implementation replaced by `Ok(true)` left
    /// every test of this crate green.
    // allow-logging: `referent_cases` hands back a list of corpus labels written in this file
    fn referent_cases() -> [(String, &'static str, String, bool); REFERENT_CASES] {
        let other = "b".repeat(64);
        [
            // The ordinary yes.
            (
                format!("[\"example.invalid/pinned@sha256:{DIGEST}\"]"),
                "example.invalid/pinned:1",
                format!("sha256:{DIGEST}"),
                true,
            ),
            // The tag resolves to something else. This is the whole point.
            (
                format!("[\"example.invalid/pinned@sha256:{other}\"]"),
                "example.invalid/pinned:1",
                format!("sha256:{DIGEST}"),
                false,
            ),
            // **The digest is in the list, against another repository.** An image
            // tagged into two registries carries two entries, and `contains` says
            // yes to this.
            (
                format!(
                    "[\"other.invalid/thing@sha256:{DIGEST}\",\"example.invalid/pinned@sha256:{other}\"]"
                ),
                "example.invalid/pinned:1",
                format!("sha256:{DIGEST}"),
                false,
            ),
            // The right repository among several.
            (
                format!(
                    "[\"other.invalid/thing@sha256:{other}\",\"example.invalid/pinned@sha256:{DIGEST}\"]"
                ),
                "example.invalid/pinned:1",
                format!("sha256:{DIGEST}"),
                true,
            ),
            // **A prefix.** `contains` says yes to a digest that is the start of
            // a longer one, which is how a truncated pin would pass.
            (
                format!("[\"example.invalid/pinned@sha256:{DIGEST}\"]"),
                "example.invalid/pinned:1",
                format!("sha256:{}", &DIGEST[..32]),
                false,
            ),
            // An image with no repository digest at all — built locally, never
            // pushed. `[]` must not be a yes.
            (
                "[]".to_owned(),
                "example.invalid/pinned:1",
                format!("sha256:{DIGEST}"),
                false,
            ),
            // A tag with no version, which is a legal reference.
            (
                format!("[\"example.invalid/pinned@sha256:{DIGEST}\"]"),
                "example.invalid/pinned",
                format!("sha256:{DIGEST}"),
                true,
            ),
            // **A registry with a port**, whose colon is not a tag separator.
            // A registry named with a port has two colons and only the second
            // introduces a tag; cutting at the first leaves the host alone as
            // the repository, which matches nothing. The clause that guards
            // this — `.filter(|(_, after)| !after.contains('/'))` — is decided
            // by this case; without it a mutation removing the clause survives
            // while the corpus says it is covered. A private registry on a port
            // is the ordinary way to run one.
            (
                format!("[\"registry.example:5000/pinned@sha256:{DIGEST}\"]"),
                "registry.example:5000/pinned",
                format!("sha256:{DIGEST}"),
                true,
            ),
            // The same, with a tag after the port: both colons present, and only
            // the last one is the tag.
            (
                format!("[\"registry.example:5000/pinned@sha256:{DIGEST}\"]"),
                "registry.example:5000/pinned:v1",
                format!("sha256:{DIGEST}"),
                true,
            ),
            // And a port whose digest is another image's, which must not match.
            (
                format!("[\"registry.example:5000/other@sha256:{DIGEST}\"]"),
                "registry.example:5000/pinned",
                format!("sha256:{DIGEST}"),
                false,
            ),
        ]
    }

    #[test]
    fn the_referent_predicate_decides_every_clause() {
        let mut wrong = Vec::new();
        for (listed, tag, digest, expected) in referent_cases() {
            let answer = crate::gates::one_of_them_is(&listed, tag, &digest);
            if answer != expected {
                wrong.push(format!(
                    "  {listed:?} against `{tag}` and `{digest}` — the gate said {answer}, \
                     and it holds {expected}"
                ));
            }
        }
        verdict(&wrong, "one_of_them_is");
    }

    #[test]
    fn the_image_pin_predicate_decides_every_clause() {
        let mut wrong = Vec::new();
        for (body, expected) in image_cases() {
            let scratch = Scratch::new();
            scratch.write("docker-compose.yml", &body);
            let findings = crate::gates::every_image_is_pinned(scratch.root(), false).findings;
            if findings.len() != expected {
                wrong.push(format!(
                    "  {body:?} — the gate found {}, and it holds {expected}: {}",
                    findings.len(),
                    findings
                        .iter()
                        .map(|finding| finding.what.as_str())
                        .collect::<Vec<_>>()
                        .join(" | ")
                ));
            }
        }
        verdict(&wrong, "pinned_by_digest + is_a_build_key");
    }

    /// How many, for the reason written on [`IMAGE_CASES`].
    const FLOW_CASES: usize = 5;

    // **A `String` label, like its sibling `image_cases`.** A function handing
    // back a `&'static str` is a door into the log's type wall whatever it
    // calls, and the wall judges this file — so the label is owned rather than
    // borrowed, which costs a test nothing.
    /// A spelling of an image the file pass cannot read, with the defect it hides.
    ///
    /// **`gates::the_image_on` splits a line at the first colon and stops.** So
    /// `a: {image: postgres:latest}` is the key `a`, the file pass reads no image
    /// there and none of its rules is applied to one — while `gates::is_a_build_key`,
    /// in the same file, splits on `{` and `,` because flow style is YAML too. One
    /// rule written twice, and the image half is the narrower copy.
    ///
    /// Measured on the tree as it is, that is not a hole. The resolved
    /// projection prints one image line per service whatever the file spelled, so a
    /// spelling the file pass cannot read lowers its own count and not docker's, and
    /// `xtask/src/check.rs` refuses a run whose file count is under the resolved one.
    /// For the third case below — a variable whose default is pinned, where the
    /// resolved pass sees a pin and the file pass sees nothing, so neither has a
    /// finding to make — docker resolved 2 and the file pass read 1. Alone in the
    /// file it is 1 and 0, which the emptiness guard takes first.
    ///
    /// **These cases decide that.** Every other case in this section counts
    /// findings, and this spelling produces none, so without these an `else if`
    /// on the line loop that pays the file count back without judging what it
    /// counted — the widening somebody reaching for this writes first — leaves
    /// the rest of this crate's tests green with `postgres:latest` in the
    /// resolved stack, measured.
    ///
    /// So every case here carries a real defect, and what is asserted is the verdict
    /// rather than the parser: read and judged, or the counts refuse the run. A
    /// later widening of `gates::the_image_on` keeps this green by moving a case
    /// from the second arm to the first.
    ///
    fn flow_cases() -> [(String, String); FLOW_CASES] {
        let pinned = format!("example.invalid/pinned:1@sha256:{DIGEST}");
        [
            // Alone in the file, so the file pass reads nothing at all and the
            // emptiness guard is what refuses the run rather than the floor.
            (
                "a bare tag, alone in the file".to_owned(),
                "services:\n  a: {image: postgres:latest, restart: unless-stopped}\n".to_owned(),
            ),
            // Beside an image it can read, so only the floor is left.
            (
                "a bare tag beside an image the file pass can read".to_owned(),
                format!(
                    "services:\n  first:\n    image: {pinned}\n  \
                     a: {{image: postgres:latest, restart: unless-stopped}}\n"
                ),
            ),
            // **The shape neither pass has a finding for.** A variable resolves
            // against whichever environment ran the command, so the resolved pass
            // sees the pinned default; the file pass is the one that catches a
            // variable, and this is the spelling it cannot read. The counts are
            // the whole defence.
            (
                "a variable with a pinned default".to_owned(),
                format!(
                    "services:\n  first:\n    image: {pinned}\n  \
                     a: {{image: \"${{IMG:-{pinned}}}\", restart: unless-stopped}}\n"
                ),
            ),
            // A quoted key inside the flow mapping, which is the spelling the
            // block-style pass already normalises away.
            (
                "a quoted key inside the mapping".to_owned(),
                "services:\n  a: {\"image\": postgres:latest}\n".to_owned(),
            ),
            // Two services on one line: a reader that took the first entry and
            // stopped would call this one image and pass the other.
            (
                "two services on one line".to_owned(),
                format!("services: {{a: {{image: postgres:latest}}, b: {{image: {pinned}}}}}\n"),
            ),
        ]
    }

    /// An image the resolved stack runs, written where the file pass cannot read
    /// it, never reaches a green verdict.
    ///
    /// The three guards in front of the step's `ok` line are asserted as a
    /// disjunction because which one fires is not the property: the file pass read
    /// nothing, the file pass read fewer images than docker resolved, or a finding
    /// was made. See [`FLOW_CASES`] for the measurement this is written from.
    ///
    /// **And a floor under the case itself.** Every assertion here is satisfied by
    /// a run where docker answered nothing — the error `every_image_is_pinned`
    /// files when it cannot resolve the stack is itself a finding — so a machine
    /// with no docker would pass this while asking it nothing.
    #[test]
    fn a_flow_style_image_never_reaches_a_green_verdict() {
        let mut wrong = Vec::new();
        for (about, body) in flow_cases() {
            let scratch = Scratch::new();
            scratch.write("docker-compose.yml", &body);
            let pins = crate::gates::every_image_is_pinned(scratch.root(), false);
            if pins.read == 0 {
                wrong.push(format!(
                    "  {about} — the resolved stack held no image, so this case asked \
                     nothing: {}",
                    pins.findings
                        .iter()
                        .map(|finding| finding.what.as_str())
                        .collect::<Vec<_>>()
                        .join(" | ")
                ));
                continue;
            }
            if pins.written >= pins.read && pins.findings.is_empty() {
                wrong.push(format!(
                    "  {about} — docker resolved {} image(s), the pass over the files \
                     counted {} and judged none of them, so the step prints its `ok` line \
                     over a stack this spelling hid an unpinned image in",
                    pins.read, pins.written
                ));
            }
        }
        verdict(&wrong, "the floor under in_the_compose_files");
    }

    /// The file list is a clause per element: a name dropped from it is a whole
    /// file compose reads and this gate stops reading.
    ///
    /// The base file is pinned and the file under test is not, and only the file
    /// pass is counted — it labels its findings with the file they came from,
    /// which is exactly the question here.
    const COMPOSE_FILES: [&str; 8] = [
        "docker-compose.yml",
        "docker-compose.yaml",
        "docker-compose.override.yml",
        "docker-compose.override.yaml",
        "compose.yml",
        "compose.yaml",
        "compose.override.yml",
        "compose.override.yaml",
    ];

    /// The one clause `docker compose config` refuses to let the resolved pass
    /// see: an image reference with a digest and no name in front of it.
    ///
    /// Compose reads the **first** name it finds, and this gate's file pass
    /// reads all eight:
    ///
    /// ```text
    /// compose.yaml            <- compose opens this one and stops
    /// docker-compose.yaml     <- the file pass still reads it
    /// ```
    ///
    /// So a file compose never opens is where a reference compose would refuse
    /// can be put in front of `pinned_by_digest` — which is the only way this
    /// clause is reachable through the shipped entry point at all.
    #[test]
    fn the_digest_needs_a_name_in_front_of_it() {
        let scratch = Scratch::new();
        scratch.write(
            "compose.yaml",
            &format!("services:\n  base:\n    image: alpine:3@sha256:{DIGEST}\n"),
        );
        scratch.write(
            "docker-compose.yaml",
            &format!("services:\n  a:\n    image: @sha256:{DIGEST}\n"),
        );
        let findings = crate::gates::every_image_is_pinned(scratch.root(), false).findings;
        let found = of_kind(&findings, "is written here and is not pinned by digest");
        let wrong = if found == 1 {
            Vec::new()
        } else {
            vec![format!(
                "  `@sha256:<64 hex>` — the file pass reported {found} finding(s), and a \
                 digest with no name in front of it is not an image reference: {}",
                findings
                    .iter()
                    .map(|finding| finding.what.as_str())
                    .collect::<Vec<_>>()
                    .join(" | ")
            )]
        };
        verdict(&wrong, "pinned_by_digest's name clause");
    }

    #[test]
    fn the_compose_file_list_decides_every_clause() {
        let mut wrong = Vec::new();
        for named in COMPOSE_FILES {
            let scratch = Scratch::new();
            if named != "docker-compose.yml" {
                scratch.write(
                    "docker-compose.yml",
                    &format!("services:\n  base:\n    image: alpine:3@sha256:{DIGEST}\n"),
                );
            }
            scratch.write(
                named,
                "services:\n  a:\n    image: example.invalid/pinned:1\n",
            );
            let findings = crate::gates::every_image_is_pinned(scratch.root(), false).findings;
            // **The file pass by its own sentence, not by the label.** The
            // resolved pass hardcodes `docker-compose.yml` as the file of every
            // finding it makes, so counting by name would count both passes for
            // one of the eight and one pass for the other seven.
            let found = of_kind(&findings, "is written here and is not pinned by digest");
            // **And that the file pass named this file.** `of_file` answers
            // that, and its answer is used here: a call site that discards it
            // with `let _` leaves the function dead — `-D dead-code` cannot see
            // a called-and-discarded one, and inverting its predicate then
            // changes nothing anywhere, measured. What it is for
            // is the other half of the sentence above: the finding is about the
            // file that holds the unpinned image, and the resolved pass writes
            // `docker-compose.yml` on every finding it makes.
            let by_name = of_file(&findings, named);
            // **Equality, not containment.** `finding.file == named` widened to
            // `contains` answers the same for every name this list holds, so
            // without this the clause is decided by nothing, measured. A
            // substring of a real file name is the shortest input that tells the
            // two apart, and it needs no fixture: it is this name with its first
            // character gone.
            let shortened = named.get(1..).unwrap_or_default();
            if !shortened.is_empty() && of_file(&findings, shortened) != 0 {
                wrong.push(format!(
                    "  `{shortened}` is not a file in this tree, and `of_file` counted \
                     findings for it — so it answers about names that merely hold the \
                     one asked for"
                ));
            }
            if by_name == 0 {
                wrong.push(format!(
                    "  `{named}` — no finding names this file, so a reader is sent to \
                     whichever file the resolved pass happens to label"
                ));
            }
            if found != 1 {
                wrong.push(format!(
                    "  `{named}` — the file pass reported {found} finding(s), and this file \
                     holds one unpinned image"
                ));
            }
        }
        verdict(&wrong, "in_the_compose_files' name list");
    }
}
