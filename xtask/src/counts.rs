//! A count written in prose is a claim, and a claim needs a gate.
//!
//! `AGENTS.md` already states the rule: **"Record the command, not the value.
//! 'Run `cargo xtask check`', never 'there are 41 tests' — a written count rots
//! silently and an agent that trusts a stale one errs with confidence."**
//!
//! Unenforced, counts rot together: an audit of an unenforced tree finds every
//! count wrong at once — "five compile-fail fixtures" in a tree holding nine,
//! "the five invariants" in one holding six, "the journal's ten integration
//! tests" in one holding thirteen — and a count corrected in one file stays
//! standing in another, in the same change, which is the failure mode exactly.
//!
//! # It refuses the count rather than checking it
//!
//! Checking is the wrong shape: `§14.9`'s **seventh
//! invariant** is an ordinal rather than a count, and a gate that read it as one
//! would report a number nobody wrote. A gate that is wrong in that direction is
//! worse than no gate, because it teaches a reader to ignore it.
//!
//! So this enforces the rule as `AGENTS.md` writes it: **do not state the
//! count.** A number immediately before a noun this repository can count is a
//! finding, and the fix is the command that answers it. Nothing has to be kept
//! in step, because nothing is written down.
//!
//! `COUNTABLE` is small on purpose. "Five verbs, and the surface does not grow"
//! stays: that number is a *constraint* `§19.4` states, not an observation about
//! the tree, and it is load-bearing prose. What is here is the shape that rots —
//! a number describing how many files happen to be in a directory today.
//!
//! Written numbers as well as digits, because that is how every one of these was
//! written: "**five** compile-fail fixtures", not "5".

use std::path::Path;

use crate::gates::{Finding, walk};
use crate::relative_to;

/// The documents whose counts are checked.
///
/// A person is asked to trust these before they run anything, which is what
/// makes a stale number in one worse than a stale number in a comment.
///
/// **Source files are out of scope, and that is the scope's own exclusion.**
/// `NOT_READ` states the one hole inside Markdown; the larger hole is the file
/// type, and counts rot there too — an audit of the Rust sources finds eleven
/// rotted, six of them in `xtask/src/`, the tool that enforces the rule.
///
/// Widening to Rust sources is measured and costs more than it buys: twelve
/// findings, nearly all of them a gate's own test fixture stating a count about
/// a scratch tree, or this module's prose *about* counting. Each would need a
/// marker, and twelve markers added to make a gate fire is the exclusion list
/// growing to fit the gate rather than the gate fitting the tree.
///
/// So the rule is what it says: a number in a document a person is told to read
/// before running anything is a number a gate checks, and a number in a comment
/// is checked by the reader who is already reading the code. What closes the
/// difference is that the comment sits beside what it describes, and the
/// document does not.
const READ: [&str; 3] = ["AGENTS.md", "README.md", "CLAUDE.md"];

/// The workspace documents, which live above this repository and are read too.
///
/// A walk of the repository root alone leaves `NOCTUA.md` and `NOCTUA-PLAN.md`
/// — the Master Technical Reference and the staged plan, which an agent is told
/// to read before anything else — out of reach entirely. The workspace's own
/// `AGENTS.md`, `README.md` and `CLAUDE.md` are here for the same reason: a
/// stale count in the file that tells an agent how to work is the worst place
/// for one.
///
/// `sections.rs` finds the workspace the same way and treats its absence as an
/// error rather than an empty result, and so does this: a gate that passed
/// loudest when it could not open its documents would be worse than none.
///
/// **And the document that outranks every other one.**
/// `reference/identity-and-authorization.md` is first in `NOCTUA.md §0.4`'s
/// authority order — above `NOCTUA.md` itself — and an omission here carries no
/// reason, unlike every entry in [`NOT_READ`]: the walk that proves no Markdown
/// falls through the gap walks `root()`, which by construction cannot reach
/// above the repository. Measured with it unnamed, a planted `The monorepo ships
/// nine invariants under `policy/core/`.` — against seven on disk — leaves the
/// `… document(s) state no count that can rot, in … paragraph(s)` line
/// byte-identical and `check passed`, while the same sentence in `NOCTUA.md`
/// is refused. The step that holds `policy/schema.cedarschema` against §4
/// reads this file in the same run, so without it two gates in one `check`
/// disagree about what "the workspace documents" are.
pub(crate) const ABOVE: [&str; 6] = [
    "NOCTUA.md",
    "NOCTUA-PLAN.md",
    "AGENTS.md",
    "README.md",
    "CLAUDE.md",
    "reference/identity-and-authorization.md",
];

/// The Markdown above this repository this gate does not read, and why.
///
/// The same rule as [`NOT_READ`], for the workspace half: **an exclusion is a
/// claim and needs its own test**, and
/// `every_workspace_document_is_one_this_gate_reads` holds this one in both
/// directions — nothing here is unreachable, and nothing outside it is skipped.
const NOT_READ_ABOVE: [(&str, &str); 1] = [(
    "reference/original-pt-BR",
    "the Portuguese originals these documents were translated from. The workspace \
     is English-only and the translation beside each one is what outranks; keeping \
     the original is provenance, and a count in it is a count in a superseded \
     document nobody is told to read",
)];

/// Whether a workspace-relative path sits inside a named directory.
///
/// Segment by segment: a named directory covers what is inside it and does not
/// cover a file whose name merely begins with the same characters, which is a
/// different file sitting beside it.
fn under_the_directory(relative: &str, directory: &str) -> bool {
    relative
        .strip_prefix(directory)
        .is_some_and(|rest| rest.starts_with('/'))
}

/// Every Markdown document in the workspace above this repository.
///
/// **A walk rather than a list.** [`ABOVE`] is what this tree expects to find
/// there, and `every_workspace_document_is_one_this_gate_reads` holds the two
/// against each other in both directions — but what this gate *reads* is what is
/// on disk, so a document added to the workspace tomorrow is read the day it is
/// added rather than the day somebody remembers to name it. A list alone misses
/// whatever nobody names: measured with the document `NOCTUA.md §0.4` puts above
/// `NOCTUA.md` itself left out of it, a planted count there passes.
///
/// `repos/` holds other repositories, each with its own gate over its own
/// documents; a dotted directory is scratch rather than something a person is
/// told to read. Both are rules, so a directory added tomorrow falls on the
/// right side of them by itself. Anything else skipped is named in
/// [`NOT_READ_ABOVE`] with its reason.
pub(crate) fn the_workspace_documents(workspace: &Path) -> Vec<std::path::PathBuf> {
    fn under(at: &Path, workspace: &Path, found: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(at) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_str().unwrap_or("");
            if name.starts_with('.') || name == "repos" {
                continue;
            }
            if path.is_dir() {
                under(&path, workspace, found);
            } else if path.extension().is_some_and(|kind| kind == "md") {
                // **By segment, not by prefix**, which is the rule [`excluded`]
                // ten screens down carries too: a `starts_with` on the excluded
                // directory's name is also true of a *file* of that name with an
                // extension after it, sitting beside the directory and outside
                // the gate with nothing said — and a test of the hole asking the
                // same `starts_with` cannot see it.
                let relative = crate::relative_to(workspace, &path);
                if !NOT_READ_ABOVE
                    .iter()
                    .any(|(directory, _)| under_the_directory(&relative, directory))
                {
                    found.push(path);
                }
            }
        }
    }
    let mut found = Vec::new();
    under(workspace, workspace, &mut found);
    found.sort();
    found
}

/// The Markdown this gate deliberately does not read, and why.
///
/// `gates.rs` states the rule this pair exists to satisfy: **an exclusion list
/// is a claim and needs its own test.** Two tests hold it — one that every
/// Markdown file in the tree is either read or excluded by a prefix here, so a
/// document added tomorrow cannot fall through the gap unnoticed, and one that
/// puts a stated count inside the hole and proves the hole is where this says
/// it is.
const NOT_READ: [(&str, &str); 3] = [
    (
        // **The wiki texts, which are what a person reads about an entry.**
        // Each is held by the catalog compiler — placed, named for an entry,
        // translated, in a format, bounded — and compiled into the generated
        // crate, and none is a document about this tree: a number in one is
        // something a person is told about their own data, never a count of
        // registries or crates that can rot.
        "/catalog/wiki",
        "the wiki texts the catalog compiler holds and compiles in: what a person is \
         shown about an entry, never a document about this repository, so a number in \
         one counts nothing in this tree",
    ),
    (
        // **`/docs`, which is one directory, and not `docs` anywhere.** The
        // reason below is true of the generated directory at the root and of
        // nothing else, and a rule matching the *name* at any depth makes
        // `policy/docs/README.md` a hole with a reason written about a <!-- allow-reference: names `policy/docs/README.md`, the plant that proved this hole, which is deliberately not in the tree -->
        // different file. Measured under such a rule: that file, stating six
        // invariants against seven policies and three migrations against
        // twelve, leaves `14 document(s) state no count that can rot`
        // byte-identical and the suite passing; the same file at `policy/notes/README.md` is a finding. // allow-reference: `policy/notes/README.md` is where this measurement put a file, not a file this tree has
        // A leading `/` means the entry is a path from this repository's root;
        // anything else is a name, matched wherever it sits.
        "/docs",
        "generated from the catalog by `cargo xtask docs` and drift-checked against \
         disk, so a number in one is the tree counting itself rather than a person \
         claiming a count — and editing one by hand is already a build failure",
    ),
    (
        // **The one canary this gate can reach, not the directory.** Naming
        // `canaries` would also hide `README.md` — well over a hundred kilobytes
        // of prose a reader is told to read first, holding counts nothing could
        // check. The stated reason is true of the fixtures and false of that
        // file, and this walk reads Markdown, so `citations.md` is the only one
        // of the three it could ever read — and read, `README.md` is judged like
        // the prose it is, so a rotted count in one of its headings is a
        // finding.
        "citations.md",
        "a file whose whole purpose is to carry the defect a gate must report, so a \
         count in one is a fixture and not a claim. `check` asserts each gate found \
         its canary; a number about the canary would be a number about that fixture",
    ),
];

/// Markdown the release writes, which this gate does not read: the changelog.
///
/// **Beside [`NOT_READ`] rather than in it, because a hole there holds a file**
/// and the changelog is there only once a release has written it. Each of its
/// lines is a commit's subject, written whole from the history by
/// `cargo xtask release`, so a number in one is what a commit said of the tree
/// when it was made, and the tree a later release reads is not held to it.
#[cfg(test)]
const WRITTEN_BY_THE_RELEASE: [&str; 1] = [crate::release::CHANGELOG];

/// The nouns this repository can count, and where each is counted.
///
/// A noun is here only when the tree answers it exactly. "Gates" is not, because
/// what is a gate is a judgement; "compile-fail fixtures" is, because it is the
/// number of files in one directory.
/// Each is a plural noun, the directory that answers it, and the glob that
/// answers it there — a glob rather than an extension, so `crates`, which is
/// answered by directories and not by files, fits the same shape.
///
/// **The extension matters.** `policy/core/` holds a `README` and a `tests/`
/// directory beside its policy files, so counting every entry answers more
/// than there are policies — a gate that is wrong in that direction is worse
/// than no gate, because it teaches a reader to ignore it. `ls policy/core/*.cedar`
/// is the command, and the glob is why.
///
/// **The noun is written plural, and matched plural.** English spells the
/// indefinite article and the number one the same way, and `NOCTUA.md` has
/// "**one registry** deciding which floating list is open" — an article, about
/// another repository, reported as a count. A count of a set is written with a
/// plural noun; "one registry deciding" and "every compile-fail fixture
/// declares" are not counts of anything and cannot be read as one.
///
/// Spelled rather than derived, because `registry` pluralises to `registries`
/// and appending an `s` would have produced a needle that never matches — a
/// gate wired to nothing, which is the `§19.3` failure this whole suite exists
/// to catch.
///
/// **`components` is measured and left out, and the number is here so the next
/// reader does not measure it again.** It is the obvious candidate: the two
/// workspace documents above make counts about `noctua-design`, and such a count
/// — "`AILabel` is drawn inside forty-six other components" — goes stale as soon
/// as the other repository moves, which is precisely what this gate exists to
/// stop. Added as another noun it fires seven times, and six of those are
/// dated stage rows in `NOCTUA-PLAN.md`: "**complete** — 2026-07-30 … nine components declare it"
/// is a record of a day, not a claim about the tree, and it does not rot. Six
/// `allow-count:` markers bought to catch one live claim is the trade this
/// module's own header calls worse than no gate, because it teaches a reader to
/// scroll past the marker.
///
/// The structural half is the reason it stays measured rather than argued:
/// the directory that answers the noun is in **another repository**, reached
/// through the workspace and absent from a bare checkout, so the noun would
/// resolve to nothing here and the decoration test below would have to learn a
/// skip — and the reference gate refuses a paragraph naming that directory as a
/// path. A gate whose only way to be honest is a new way to go quiet is the
/// wrong gate. A live claim of that kind is written with no number instead.
const COUNTABLE: [(&str, &str, &str); 7] = [
    ("compile-fail fixtures", "crates/nc-log/fixtures", "*.rs"),
    ("invariants", "policy/core", "*.cedar"),
    // **One of the three examples in the header above**, and without this noun
    // the gate cannot see the defect it names.
    ("integration test files", "crates/nc-journal/tests", "*.rs"),
    ("migrations", "migrations", "*.sql"),
    // The reservation ledger is a top-level file that is not a registry, and
    // `i18n/` is a registry that is not a top-level file, so `ls catalog/*.toml`
    // answers this exactly — which is the bar the paragraph above sets.
    ("registries", "catalog", "*.toml"),
    // Answered by directories, which is why the field above is a glob.
    ("crates", "crates", "*/Cargo.toml"),
    // **A number about `policy/packs/` is a count like any other**, and
    // without the noun here a document telling a reader how many packs the
    // tree holds would carry a number nothing checks. A pack is a directory
    // with a README saying what it is for; that is the whole of `org/` and
    // `collective/` today, and it is what makes the empty ones the indirection
    // `§14.4` argues for rather than a missing directory. Answered by
    // directories, like `crates` above.
    ("packs", "policy/packs", "*/README.md"),
];

/// The marker that says a count is about something this tree does not hold.
///
/// It takes a reason, and the reason is never parsed — its job is to put the
/// exception in front of whoever reads the line next.
pub(crate) const ALLOW: &str = "allow-count:";

/// Punctuation that touches a word rather than ending a clause.
///
/// A lookback taking the last *alphanumeric run* ends at an empty word on a
/// hyphen or a closing quote against the noun: `six well-documented invariants`
/// and `six "live" invariants` are then both invisible. A bracket is
/// deliberately not here — it closes a parenthetical, which is a boundary.
///
/// **A comma is not here, and that is the point.** With it here, the scan walks
/// across a list separator: `The pack ships six policies, invariants and
/// schemas` is reported as six invariants. A comma ends the item before it, so
/// it is a boundary; a quote and a hyphen touch the word they are part of.
const TOUCHES_A_WORD: [char; 7] = [
    '-', '"', '\'', '\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}',
];

/// How many words may sit between a number and the noun it counts.
///
/// `six live invariants` and `nine new compile-fail fixtures` each put one
/// there, and reading only the adjacent word misses both.
///
/// **Two steps, and the number is measured rather than chosen.** Four steps
/// read `Six people wrote migrations`, `It took six days to write invariants`
/// and `Running three times the invariants would be slow` as counts, and
/// stopping each of those needs another word in the list below. Every widening
/// buys one real catch and several sentences of ordinary English reported as
/// claims — which this module's own header calls worse than
/// no gate. Two reaches the phrasing these documents actually use (`six live
/// invariants`, `six PostgreSQL migrations`) and stops before the verbs.
///
/// What it does not reach is stated rather than left to be found. **Two is a
/// count of steps, and one of the steps is the number** — so one adjective is
/// reached and two are not: `six pending schema migrations` and `six core Cedar
/// invariants` both get past, and the known-miss list below carries them by
/// name.
const ADJECTIVES: usize = 2;

/// Words the scan will not step over on its way back to a number.
///
/// A conjunction or a preposition means the number belongs to a different noun:
/// `the six tests and the invariants` counts tests, not invariants, and a scan
/// that walked past `and` would report six of both. The sentence boundary stops
/// it in the other direction.
const NOT_AN_ADJECTIVE: [&str; 45] = [
    "and",
    "of",
    "in",
    "for",
    "with",
    "plus",
    "than",
    "beside",
    "besides",
    "to",
    "before",
    "after",
    "by",
    "on",
    "from",
    "times",
    "days",
    "weeks",
    "months",
    "years",
    // **The singulars, which the plurals' own reason covers.** Without them,
    // `Every day six migrations run` is reported as six migrations: a
    // frequency, not a count of what is in the tree.
    "day",
    "week",
    "month",
    "year",
    "time",
    "every",
    // **And its synonym, which the same sentence is written with.** With
    // `every` here and not `each`, `The pruner runs every six hours` reads as an
    // interval and `It vacuums each seven days` — the same shape, the same rate
    // — as a claim of seven days.
    "each",
    // **A fraction of a number is not that number.** Without it, `Half a dozen
    // invariants` is reported as **12**.
    "half",
    // **A magnitude this reader cannot represent.** `written` tops out at a
    // hundred, so `Two thousand invariants` would read as **2** — a finding whose
    // own number would be wrong. A document claiming thousands of any of
    // `COUNTABLE`'s nouns is not a claim this gate would be believed about
    // anyway, which is the same argument `is_a_year` already makes.
    "thousand",
    "million",
    "billion",
    // **A hedge is not a count.** Without these, eleven of twenty-two fresh
    // ordinary sentences are reported, and every one of them says the number is
    // *approximate* or a *bound*:
    //
    // ```text
    // At most three migrations …            Roughly six or seven invariants …
    // At least five migrations …            Around twenty invariants …
    // About seven invariants …              Nearly nine crates …
    // Approximately six invariants …        Perhaps five migrations …
    // Each release adds roughly two …       Twice five invariants …
    // Nine of the ten invariants …
    // ```
    //
    // Catching `Nine out of ten migrations` and not `Nine of the ten
    // invariants` closes a spelling; this is the shape.
    // A number this gate should read is one a document *asserts* — "there are
    // six" — and none of these asserts anything a tree can contradict.
    "most",
    "least",
    "or",
    "about",
    "around",
    "roughly",
    "approximately",
    "nearly",
    "perhaps",
    "twice",
    "up",
    "over",
    "under",
    "some",
];

/// Reads every document and reports every count that disagrees with the tree.
///
/// Returns the findings and how many documents were read, so a tree whose
/// documents were renamed is visible rather than green.
///
/// # Errors
///
/// Returns why the documents could not be read.
pub(crate) fn every_written_count_matches_the_tree(
    root: &Path,
) -> Result<(Vec<Finding>, usize, usize, usize), String> {
    let files = documents(root)?;
    // **Counted by something that is not this walk.** `files.len()` is the same
    // vector the loop below reads, so one filter inside `documents()` moves
    // both: measured with such a floor, a line skipping `policy/core/README.md`
    // gives an `ok … document(s)` line one short, with a real rotten count in
    // the file it skips ("three invariants and two migrations", against ten
    // and six).
    // A floor derived from the thing it floors is a mirror. This one recurses
    // on its own and knows only the names.
    let expected = how_many_documents(root);
    let workspace = crate::sections::workspace_of(root);

    let mut findings = Vec::new();
    let mut read = 0_usize;
    let mut judged = 0_usize;
    for path in files {
        // **An unreadable document is an error, not a skip.** Every one of these
        // is named in a constant, so it not opening means the tree moved rather
        // than that there was nothing to read — and skipping would report the
        // clean tree this gate exists to disprove.
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("{} could not be read: {error}", path.display()))?;
        // **Named by the workspace, not by this machine.** `relative_to` cannot
        // strip a path that is *above* the repository, so through it a finding
        // in `NOCTUA.md` prints the absolute path of whoever's checkout it is —
        // machine-dependent output in a gate message, where every other finding
        // is repository-relative.
        //
        // **This repository's own documents keep their repository-relative
        // name.** The repository is *inside* the workspace, so with the
        // workspace prefix alone `README.md` comes out as
        // `<workspace>/repos/noctua-central/README.md` while every other finding
        // in the same run is `README.md`. Two spellings for one file in one
        // list of findings is a reader working out whether they are the same
        // file.
        let inside = path.strip_prefix(root).is_ok();
        let relative = match workspace
            .as_ref()
            .filter(|_| !inside)
            .and_then(|at| path.strip_prefix(at).ok())
        {
            Some(above) => format!("<workspace>/{}", above.to_string_lossy().replace('\\', "/")),
            None => relative_to(root, &path),
        };
        // **Documents opened, not counts found.** Counting the findings makes the
        // floor fire on the state this gate exists to reach: every count removed
        // is `read == 0`, which reads as "the walk found nothing" when it means
        // "there is nothing left to find". What has to be non-zero is the walk.
        read += 1;
        // **An unbalanced fence is a finding, not a guess.** A parser that drops
        // the *last* marker when the total is odd leaves a stray marker that
        // lands **first** flipping the role of every real fence after it:
        // measured, one ``` inserted at line 9 of `AGENTS.md` makes two stated
        // counts in ordinary prose invisible, with the document count unchanged
        // and the gate green.
        //
        // There is no reading that recovers the author's intent from an odd
        // number of fences, and every reading is one somebody can use to switch
        // the gate off for a file. So this says what it found and stops
        // guessing: an unclosed fence in a Markdown document is a defect in the
        // document, and it renders wrongly for a human reader too.
        if let Some(odd) = unbalanced_fence(&text) {
            findings.push(Finding {
                file: relative.clone(),
                line: odd,
                what: "this document has an odd number of ``` markers".to_owned(),
                fix: "a fence that is never closed makes every paragraph after it \
                      ambiguous — the renderer reads the rest of the file as code, and \
                      this gate cannot tell a stated count from a line of output. Close \
                      it, or write the marker as `` ``` `` if it is prose about a fence"
                    .to_owned(),
            });
        }
        // **And a document that yielded no paragraph at all, which is a document
        // read with nothing judged.** `unbalanced_fence` above reports an *odd*
        // number of markers; a document wrapped in a **pair** of them — one on
        // the first line, one on the last — has an even count, so it passes that,
        // and every line of it is read as code. Measured: a stated count
        // appended to the workspace's `CLAUDE.md` is reported, and without this
        // check the same file wrapped in one pair of markers leaves
        // `check passed` with the document still counted as read.
        //
        // A paragraph count collapsing to zero cannot say so: it is the *total*
        // across every document read, which no single wrapped document can
        // collapse.
        // Asked per document, it is exactly the right question, because a
        // document with no prose outside a fence is not a document anybody
        // wrote.
        //
        // **What this does not reach**, said plainly: an even number of *stray*
        // markers in the middle of a file, which inverts prose and code from
        // there to the next real fence without emptying the file. Nothing here
        // can tell that from a document whose fences are all real.
        if !text.trim().is_empty() && paragraphs(&text).is_empty() {
            findings.push(Finding {
                file: relative.clone(),
                line: 1,
                what: "this document yielded no paragraph, so nothing in it was judged".to_owned(),
                fix: "every line of it is inside a ``` fence — one pair wrapping the \
                      whole file reads as code to this gate and to a renderer alike, \
                      and a count written anywhere in it is invisible. Remove the \
                      stray pair, or write the markers as `` ``` `` if they are prose \
                      about a fence"
                    .to_owned(),
            });
        }
        // **Paragraphs, not lines.** These documents are hard-wrapped at about
        // eighty columns, so a count and its noun landing on different lines is
        // the common case rather than the exotic one — "the core holds six" /
        // "invariants that the gate reads" is invisible line by line. A
        // paragraph is what a sentence fits in; the line number reported is the
        // paragraph's first, which is where a reader looks.
        for (index, line) in paragraphs(&text) {
            // **Paragraphs judged, which is what this gate actually reads.**
            // `read` counts documents *opened*, and the floor compares it with
            // another document count — so with that alone, three lines here
            // skipping every paragraph past the tenth leave both numbers where
            // they are and a rotten count in a document goes unreported with
            // `check passed`. This file's own sibling says of another gate that
            // "what these gates judge is lines", and a floor on files is not one
            // on lines.
            //
            // The fence hole one screen up is answered *there*, per document,
            // and not here: a total across every document read does not
            // collapse when one of them is wrapped.
            // Counted where the verdict is — see below.
            for (noun, counted_in, glob) in COUNTABLE {
                let claimed = claim(&line, noun);
                if claimed.is_empty() {
                    continue;
                }
                // **The hatch, marked and reasoned**, like every other in this
                // tree. A count of something *outside* the tree does not rot the
                // same way — `§14.9` names seven invariants and will still name
                // seven tomorrow — and a gate with no way to say so is a gate
                // people route around by rephrasing.
                //
                // **And the reason names the noun.** The marker is tested
                // against the *paragraph*, because a count and its noun land on
                // different lines all the time — and unless the reason names the
                // noun, one marker on the first line of a three-line paragraph
                // would excuse every claim in it, of every noun. Measured
                // without the noun rule: one `<!-- allow-count: a probe -->`
                // suppresses two findings about two different nouns across three
                // physical lines.
                //
                // A paragraph is the right unit for *finding* a claim and the
                // wrong one for excusing it, so the excuse has to say which claim
                // it is about — the rule `allow-logging:` already follows, for
                // the same reason and after the same measurement.
                //
                // Markdown, where `#` is a heading and `#42` an issue
                // reference — neither of them a comment.
                // **Markdown's own comment, not Rust's.** Asked with `Slashes`,
                // the scanner treats a `//` as opening a comment — so any
                // `https://` earlier in the paragraph makes the marker "in a
                // comment" with no `<!-- -->` anywhere. Measured: the same
                // paragraph with and without a URL in front of the marker, one
                // reported and one not.
                if crate::references::marked_in(&line, ALLOW, crate::references::Comments::Markdown)
                    && reason_in(&line).is_some_and(|reason| crate::references::names(reason, noun))
                {
                    continue;
                }
                findings.push(Finding {
                    file: relative.clone(),
                    line: index,
                    what: format!(
                        "this states a count: {} {noun}",
                        claimed
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(" and ")
                    ),
                    fix: format!(
                        "`AGENTS.md` says to record the command rather than the value, \
                         because a written count rots silently and an agent that trusts \
                         a stale one errs with confidence — and every count in this tree \
                         was found wrong at once. Say `ls {counted_in}/{glob}` instead, \
                         or drop the number: `the {noun}` reads the same and cannot go \
                         stale"
                    ),
                });
            }
            // **The last statement a verdict reaches, not the first of the body.**
            // At the head of this loop, above the `COUNTABLE` rule — the
            // placement `xtask/canaries/README.md` records — `judged += 1` lets
            // one `continue` after it leave the number exact with a rotten count
            // in the part it skips.
            judged += 1;
        }
    }
    Ok((findings, read, expected, judged))
}

/// How many documents there are, counted by code that is not the walk.
///
/// **The floor for the walk below, and not that walk's own output.** Taken from
/// the walk, one filter inside `documents()` moves both numbers together, so a
/// document with a rotten count in it can be dropped and the run stays green.
/// This recursion shares nothing with it but the list of names.
fn how_many_documents(root: &Path) -> usize {
    fn under(at: &Path, root: &Path, found: &mut usize) {
        let Ok(entries) = std::fs::read_dir(at) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            // **The walk's own rule, not a list beside it.** `gates::walk` steps
            // over `target` only where cargo builds, so a list skipping these
            // names at any depth puts a document under a module directory of
            // that name inside the walk and outside this floor, and two readings
            // of one number then disagree on a tree with nothing wrong with it.
            if path.is_dir() && crate::gates::is_skipped(at, &name) {
                continue;
            }
            // The declared holes, by the same rule the walk uses, and against
            // the same subject: a path from the root, so `/docs` is the
            // generated directory and not every directory called `docs`.
            // Sharing the *list* is the point — it is the scope both are
            // counting — while the two recursions stay separate, which is what
            // makes this a floor rather than a mirror. Asked of files only, as
            // the walk asks it: `/docs` holds the generated pages and also a
            // hand-written `README.md` the walk reads.
            if !path.is_dir()
                && NOT_READ
                    .iter()
                    .any(|(named, _)| the_hole(named, &relative_to(root, &path)))
            {
                continue;
            }
            if path.is_dir() {
                under(&path, root, found);
            } else if READ.contains(&name.as_str()) {
                *found += 1;
            }
        }
    }
    let mut found = 0_usize;
    under(root, root, &mut found);
    // The workspace's own documents, when there is a workspace: the walk adds
    // them by name for the same reason.
    if crate::sections::workspace_of(root).is_some() {
        found += ABOVE.len();
    }
    found
}

/// Every document this gate reads, in a stable order.
///
/// # Errors
///
/// Returns why the workspace above this repository could not be found.
fn documents(root: &Path) -> Result<Vec<std::path::PathBuf>, String> {
    let mut files = Vec::new();
    walk(root, &mut |path| {
        if excluded(root, path) {
            return;
        }
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| READ.contains(&name))
        {
            files.push(path.to_path_buf());
        }
    })?;
    files.sort();

    // **The workspace's documents when there is a workspace, and the
    // repository's own either way.** An error when there is none skips the
    // whole step — and most of its documents live *in* this repository
    // (`AGENTS.md`, `README.md`, `CLAUDE.md`, `xtask/canaries/README.md`, the
    // `policy/**` READMEs) and are judgeable without one. Measured that way:
    // `This tree holds three migrations.` in the repository's own `CLAUDE.md`
    // is an error locally and invisible in CI.
    //
    // The reference gate has a workspace half and a repository half for
    // exactly this reason; this is that split.
    Ok(match crate::sections::workspace_of(root) {
        Some(workspace) => {
            files.extend(the_workspace_documents(&workspace));
            files
        }
        None => files,
    })
}

/// Whether `path` falls inside the hole `NOT_READ` declares.
fn excluded(root: &Path, path: &Path) -> bool {
    let relative = relative_to(root, path);
    // **By segment, not by prefix.** A name such as `canaries` sits in
    // `xtask/canaries/` and `crates/nc-canon/canaries/` — neither of which
    // *starts* with it, so a prefix match reads a file the list says it does
    // not, while a test of the list matching by segment passes. A declared hole
    // that is not a hole is worse than no hole: somebody writes a count into it
    // believing it is exempt.
    NOT_READ.iter().any(|(name, _)| the_hole(name, &relative))
}

/// Whether one `NOT_READ` entry covers `relative`, which is a path from the root.
///
/// A leading `/` makes the entry a place — that directory and what is under it,
/// and nothing else of the same name elsewhere. Without one it is a name, and a
/// name is matched wherever it sits, which is what `citations.md` needs: the
/// canary fixtures live in two directories and neither is the root.
///
/// A file `nc_catalog_compiler::HAND_WRITTEN_FILES` names is in no hole: it
/// sits in the generated directory and is prose a person wrote, so the reason
/// given for `/docs` is false of it.
fn the_hole(entry: &str, relative: &str) -> bool {
    if nc_catalog_compiler::HAND_WRITTEN_FILES.contains(&relative) {
        return false;
    }
    match entry.strip_prefix('/') {
        Some(rooted) => relative == rooted || relative.starts_with(&format!("{rooted}/")),
        None => relative == entry || relative.split('/').any(|segment| segment == entry),
    }
}

/// The line of the last fence marker, when a document holds an odd number of them.
///
/// Reported rather than worked around: see the call site.
fn unbalanced_fence(text: &str) -> Option<usize> {
    let markers: Vec<usize> = text
        .lines()
        .enumerate()
        .filter(|(_, line)| line.trim_start().starts_with("```"))
        .map(|(index, _)| index + 1)
        .collect();
    if markers.len().is_multiple_of(2) {
        None
    } else {
        markers.last().copied()
    }
}

/// How many paragraphs the documents hold, counted by code that is not the walk.
///
/// **The floor for the paragraph count, and it must not be that walk's own
/// output.** With `read` and `expected` both document counts, a walk that opens
/// every file and judges ten paragraphs of it satisfies both — measured, three
/// lines, and a stated count in the part it skips goes unreported with
/// `check passed`. This recursion shares nothing with `every_written_count…` but
/// the list of names, exactly as `how_many_documents` does above it.
///
/// A document that cannot be read counts nothing, which can only make the floor
/// stricter: the walk would have failed on it first.
pub(crate) fn how_many_paragraphs(root: &Path) -> usize {
    // **Its own counter, because calling `paragraphs` makes it a mirror.**
    // Reading the walk's `documents()` *and* the walk's `paragraphs()`, one line
    // inside the shared parser moves both numbers together: `found.truncate(70)`
    // leaves a fraction of the corpus's paragraphs counted on both sides,
    // `check passed`, and a rotten count in the part that vanished goes
    // unreported — most of the corpus gone with an "exact" floor that cannot
    // fire.
    //
    // This counts blank-line-separated blocks outside fenced code, which is what
    // a paragraph is, without asking the parser under test what it thinks.
    // **The workspace found the way `sections` finds it**, not the repository’s parent: the
    // repository sits under `repos/`, so joining one `..` names a directory that
    // holds no documents at all — measured, the floor then reads nothing,
    // `expected` comes back zero, and `count < expected` is false for every
    // count including the truncated one it is written to catch.
    // **The same documents the walk opens.** Joining `ABOVE` onto the workspace
    // and `READ` onto the root and stopping there leaves the `README.md` files
    // under `policy/` in the walk — `documents()` recurses — and outside the
    // floor: measured, every paragraph of those files free slack, under which
    // a filter skipping `policy/` is invisible.
    // `how_many_documents`, above, recurses and this does the same,
    // sharing with the walk only the rule for which names count.
    fn under(root: &Path, at: &Path, found: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(at) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            // **The walk's own rule, not a list beside it.** `gates::walk` steps
            // over `target` only where cargo builds, so a list skipping these
            // names at any depth puts a document under a module directory of
            // that name inside the walk and outside this floor, and two readings
            // of one number then disagree on a tree with nothing wrong with it.
            if path.is_dir() && crate::gates::is_skipped(at, &name) {
                continue;
            }
            if path.is_dir() {
                under(root, &path, found);
            } else if READ.contains(&name.as_str())
                && !crate::counts::excluded_for_tests(root, &path)
            {
                found.push(path);
            }
        }
    }
    let mut here = Vec::new();
    under(root, root, &mut here);
    crate::sections::workspace_of(root)
        .iter()
        .flat_map(|at| the_workspace_documents(at))
        .chain(here)
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .map(|text| blocks_in(&text))
        .sum()
}

/// The walk's own exclusion rule, so the floor enumerates the same documents.
///
/// A gate's floor must not be its walk — but it must be over the *same corpus*,
/// or the difference is slack rather than a measurement. Only the rule for which
/// files count is shared; the recursion and the paragraph counting are separate.
///
/// **One copy of the rule, so the two read the same text.** A second copy
/// matching `NOT_READ`'s directory names against the *absolute* path, while
/// [`excluded`] matches them against the path relative to the repository, makes
/// a checkout underneath a directory that happens to share a name with a
/// declared hole exclude documents the walk reads. Measured, two identical
/// scratch trees one directory name apart:
///
/// ```text
/// base …/docs/ws-<pid>/repos/central   ->  judged=15  paragraph_floor=10
/// base …/plain/ws-<pid>/repos/central  ->  judged=15  paragraph_floor=15
/// ```
///
/// So with a second copy `cargo xtask check` cannot pass from a checkout under
/// a directory called `docs` — a clone under a home directory's own documents
/// folder — and the message a developer gets blames the tree:
/// `paragraphs: 15 were judged and a second count of the same ground says 10`.
/// It fails closed, a false failure rather than a hole, and it still breaks
/// this file's own rule that two halves of one rule read the same text. This
/// calls [`excluded`] rather than reimplementing it.
pub(crate) fn excluded_for_tests(root: &Path, path: &Path) -> bool {
    excluded(root, path)
}

/// Blank-line-separated blocks of prose, counted without the walk's parser.
fn blocks_in(text: &str) -> usize {
    let mut found = 0_usize;
    let mut fenced = false;
    let mut inside = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            inside = false;
            continue;
        }
        if fenced {
            continue;
        }
        if line.trim().is_empty() {
            inside = false;
        } else if !inside {
            inside = true;
            found += 1;
        }
    }
    found
}

/// Each paragraph of a document, and the line it begins on.
///
/// A paragraph is a run of non-blank lines joined by spaces. A count and its
/// noun on either side of a hard wrap is one sentence, and reading lines split
/// it in two.
fn paragraphs(text: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    let mut current = String::new();
    let mut began = 1_usize;
    // **A fenced block is not prose.** These documents hold directory trees,
    // shell transcripts and SQL, and joining their lines into a paragraph
    // produces sentences nobody wrote: `… (Legs 2, 3)` immediately above
    // `├── crates/` reads as "3 crates", a finding against a tree diagram. A
    // count *inside* a code block is output or an example, and neither is a
    // claim about this tree.
    // **A fence that is never closed is not a fence.** A toggle that flips on
    // any ``` and never resets lets one stray marker turn this gate off for
    // everything below it in the file — the "exclusion list turns the gate off"
    // shape, in the parser rather than in a list. Counting them first means an
    // odd number leaves the last one as ordinary prose rather than as an opening.
    let markers = text
        .lines()
        .filter(|line| line.trim_start().starts_with("```"))
        .count();
    let closes = markers - markers % 2;
    let mut seen = 0_usize;
    let mut fenced = false;
    for (index, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            seen += 1;
            if seen > closes {
                // The odd one out: prose, not a fence.
                continue;
            }
            fenced = !fenced;
            if !current.is_empty() {
                found.push((began, core::mem::take(&mut current)));
            }
            continue;
        }
        if fenced {
            continue;
        }
        if line.trim().is_empty() {
            if !current.is_empty() {
                found.push((began, core::mem::take(&mut current)));
            }
            continue;
        }
        if current.is_empty() {
            began = index + 1;
        } else {
            current.push(' ');
        }
        current.push_str(line);
    }
    if !current.is_empty() {
        found.push((began, current));
    }
    found
}

/// The marker's reason, ending where its comment does.
///
/// **Not "everything after the marker".** A paragraph is joined into one line
/// here, so the text after `allow-count:` would run to the end of the
/// *paragraph* — the reason would contain every later sentence, and a rule
/// asking whether the reason names the claim would be satisfied by the claim
/// itself. The
/// comment ends at `-->`, and so does the reason.
fn reason_in(line: &str) -> Option<&str> {
    let after =
        crate::references::reason_after(line, ALLOW, crate::references::Comments::Markdown)?;
    Some(
        after
            .split("-->")
            .next()
            .unwrap_or(after)
            .split("*/")
            .next()
            .unwrap_or(after),
    )
}

/// The words that put a count in front of a noun, rather than a label.
///
/// See the paragraph in `claim` that uses it. Every entry is a construction that
/// can only be counting — a copula, a verb of holding or listing, or a
/// quantifier — so a word missing from here is a count this gate does not see.
/// A word wrongly *in* here would be a
/// false finding, and this file's own header calls a gate wrong in that
/// direction worse than no gate; that is why it is this list and not the other.
const COUNTS_THEM: [&str; 31] = [
    "are",
    "is",
    "was",
    "were",
    "be",
    "been",
    "has",
    "have",
    "had",
    "holds",
    "hold",
    "carries",
    "carry",
    "contains",
    "contain",
    "ships",
    "declares",
    "lists",
    "names",
    "adds",
    "all",
    "only",
    "just",
    "both",
    // **Seven more, each measured hiding a stale number.** Appended to
    // `README.md` without them, `The catalog defines 40 registries, and the pack
    // provides 99 invariants.` gives `ok … document(s) state no count that can
    // rot`; the same sentence with `holds` and `lists` reports both numbers. The
    // whitelist is the weak half of this rule and this does not change that —
    // it is a list of English verbs and English has more.
    //
    // **The other direction is the wrong one.** Treating a number that
    // *touches* its noun as a count needs no verb at all, and it reads as the
    // right shape: a label goes before its noun (`Stage 10`, `page 12`,
    // `§19.4`) and a count goes after the verb. But `Stage 10 migrations were
    // written for it` touches `migrations` too, and the corpus says so — four
    // cases, all of them a label followed by a countable noun. Closing the
    // class that way needs a list of every word that can *identify*, which
    // fails the way this module's header describes: a word it lacks
    // is then a false finding, and a gate wrong in that direction is worse than
    // no gate. A word missing from the list below is a miss, which is the
    // failure this rule is willing to have.
    "defines",
    "define",
    "provides",
    "includes",
    "offers",
    "comprises",
    "supports",
];

/// The number a line claims of `noun`, if it claims one.
fn claim(line: &str, noun: &str) -> Vec<usize> {
    // **Emphasis stripped first.** Taking the last *alphanumeric* run before
    // the noun, `**five**` ends in `*`, so the run is empty and `written("")` is
    // `None`: every bold, italic or backticked number is invisible — and
    // `**five** compile-fail fixtures` is the spelling this module's own doc
    // comment quotes twice. These documents use bold constantly.
    let lowered = line.to_lowercase().replace(['*', '_', '`'], " ");
    // **And the runs that blanking just made, because the needle has none.**
    // The emphasis above is replaced character for character, and `COUNTABLE`
    // holds multi-word nouns written with exactly one space — so emphasis
    // *inside* a noun phrase leaves two or three where the needle has one, and
    // uncollapsed, `five **compile-fail** fixtures` matches
    // `compile-fail fixtures` nowhere and produces **no claim at all**, which
    // is a count gate switched off by the ordinary way somebody writes a
    // sentence. The transformation that makes the *number* visible would make
    // the *noun* invisible: one side of the comparison normalised and the
    // other a raw literal.
    //
    // Every index taken below is into `lowered` itself, never back into
    // `line`, so collapsing runs here changes no offset's meaning.
    let lowered = lowered.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut found = Vec::new();
    // **Every occurrence, not the first.** `find` stops at the first mention,
    // so a sentence naming the noun once without a number would hide a count
    // later on the same line.
    let mut from = 0;
    while let Some(at) = lowered[from..].find(noun) {
        let here = from + at;
        from = here + noun.len();
        // **The noun, not a longer word that begins with it.** `crates.io` holds
        // `crates`, and `Seven crates on crates.io` would be reported twice — once
        // for the count and once for the registry's name, as `7 and 7`. A `.`
        // followed by a letter is a domain or a path, not the end of a word.
        let after = lowered[from..].chars();
        let mut after = after.clone().take(2);
        let ends_the_word = match (after.next(), after.next()) {
            (Some(c), _) if c.is_alphanumeric() || c == '_' => false,
            // A dot then a letter is a domain: `crates.io`.
            (Some('.'), Some(next)) if next.is_alphanumeric() => false,
            // **And a slash is a path.** `3 crates/nc-journal/tests/…` is a
            // criterion number in front of a directory, and it would be reported
            // as three crates — in a table of pointers whose whole job is to name
            // files.
            (Some('/'), _) => false,
            _ => true,
        };
        if !ends_the_word {
            continue;
        }
        let before = lowered[..here].trim_end();
        // **Up to `ADJECTIVES` steps back, not one.** `six live invariants`
        // and `nine new compile-fail fixtures` are invisible to one: the number
        // is not always adjacent to its noun, and an adjective between them is
        // ordinary English. The scan stops at a sentence boundary so it cannot
        // reach into the sentence before.
        // **Trimmed of punctuation, not stopped by it.** A lookback taking the
        // last *alphanumeric run* ends the walk at an empty word on
        // `six well-documented invariants` and `six "live" invariants`: a hyphen
        // or a closing quote touching the noun makes the count invisible. `*`,
        // `_` and backticks are stripped above; nothing else is.
        let mut rest = before.trim_end_matches(TOUCHES_A_WORD).trim_end();
        let mut counted = None;
        for _ in 0..ADJECTIVES {
            // **A hyphenated compound is one word.** `six on-disk invariants`
            // and `six well-documented invariants` would be two steps each — the
            // second half, then the first — so an ordinary compound adjective
            // used up the whole lookback.
            let Some(word) = rest
                .rsplit(|c: char| !c.is_alphanumeric() && c != '-')
                .next()
            else {
                break;
            };
            let preceding = rest[..rest.len() - word.len()].trim_end();
            // **Just the empty word.**
            // `word.is_empty() || punctuation && word.is_empty()` is
            // `word.is_empty()` — a punctuation half here decides nothing. The
            // sentence boundary is enforced below, where the walk steps.
            if word.is_empty() {
                break;
            }
            if let Some(number) = written(word) {
                counted = Some((number, preceding.to_owned(), word.to_owned()));
                break;
            }
            // Not a number: step over this word and try the one before it, but
            // never across a sentence boundary and never past a word that says
            // the number belongs to something else.
            if NOT_AN_ADJECTIVE.contains(&word) {
                break;
            }
            if preceding.is_empty() || preceding.ends_with(['.', ';', ':', ',', '(']) {
                break;
            }
            // The punctuation, then the space it left behind: `six "live"`
            // trims to `six `, and the empty run after that space would end the
            // walk one word short of the number.
            rest = preceding.trim_end_matches(TOUCHES_A_WORD).trim_end();
        }
        let Some((number, preceding, numeral)) = counted else {
            continue;
        };
        // **What is before the number decides whether it is a count.** `Stage 6
        // invariants` and `§19.4 invariants` would both be reported — a stage and a
        // section, one of them with the wrong number ("4", from `19.4`). A gate
        // that is wrong in the other direction teaches a reader to ignore it,
        // which is worse than no gate.
        //
        // **A full stop only hides a number when it is *inside* one.** A bare
        // `ends_with('.')` suppression, for `§19.4`, turns the gate off for
        // every count that opens a sentence after the first — including `Read \`ls
        // policy/core\`. Six invariants live there.`, which is the sentence this
        // gate's own fix text recommends followed by the stale number. What
        // distinguishes `19.4` is the digit before the dot.
        // **A separator with a digit on the far side is inside one number.**
        // `§19.4` is one case; `On 2026-08-25 migrations were re-verified` is
        // the same shape with a hyphen, and would be reported as twenty-five
        // migrations. A slash covers a date written the other way.
        let inside_a_number = preceding
            .chars()
            .next_back()
            .is_some_and(|last| matches!(last, '.' | '-' | '/' | ':'))
            && preceding[..preceding.len() - 1]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_digit());
        // **A four-digit number in this range is a year.** `In 2026 migrations
        // were re-verified` and `Since 2025 invariants have been enforced by a
        // gate` would be reported as 2026 migrations and 2025 invariants — the same
        // sentence this gate already suppresses when the date is written
        // `2026-08-25`, with the year alone. Nothing this reads counts in the
        // thousands: `COUNTABLE`'s nouns are compile-fail fixtures, invariants,
        // integration test files, migrations, registries, crates and packs, and
        // a document claiming two thousand of any of them is not a claim this
        // gate would be believed about anyway.
        // No number word reaches this range — `written` tops out at a hundred
        // and its compounds at ninety-nine — so this is a run of digits by
        // construction, and does not need to check that it is.
        let is_a_year = (1900..=2200).contains(&number);
        // **A number written in digits, with a word in front of it, names
        // something.** A list of label words — `stage`, `leg`, `chapter`,
        // `postgresql`, `round` — grows one entry at a time behind whichever
        // sentence somebody happens to write, and measured, 50% of fresh
        // ordinary English gets through such a list. Every miss has the same
        // shape and differs only in the word:
        //
        // ```text
        // Day 3 migrations      Issue 5 migrations    Python 3 …
        // Week 2 invariants     Task 3 invariants     Java 8 …
        // page 12 invariants    Level 2 invariants    Ubuntu 24 …
        // Group 4 migrations    Sprint 2 …            Column 3 …
        // Run 3 migrations      Schema 1 …            Priority 1 …
        // ```
        //
        // A list with `round` on it misses `run`, with `attempt` misses `task`,
        // with `node` misses `python`. The class such a list reaches for is *a
        // number that identifies rather than
        // counts*, and in this repository's prose those are written in digits
        // and preceded by their label, while a count is written as a word:
        // `ls policy/core` and there are **six** invariants. Measured across the
        // documents this gate reads: seven counts of a `COUNTABLE` noun, every
        // one of them spelled out, and no digit form at all.
        //
        // So digits after a word name something. What that gives up is a count
        // written `there are 3 migrations`, which nothing in these documents
        // does — and this gate's own header says a gate that is wrong in the
        // other direction teaches a reader to ignore it, which is worse than no
        // gate.
        let in_digits = numeral.chars().all(|c| c.is_ascii_digit());
        let opens_a_clause =
            preceding.is_empty() || preceding.ends_with(['.', ';', ':', ',', '(', '[', '—']);
        // **Or a word that counts them, which is the other half of the rule
        // above.** That paragraph gives up `there are 3 migrations` on the
        // grounds that "nothing in these documents does" it — a measurement of
        // today's prose, not a property, and the same green-because-of-how-the-
        // tree-happens-to-look reasoning this file exists to refuse. Measured, a
        // digit form of six of the nouns:
        //
        // ```text
        // There are 9 crates in this repository.        <- missed
        // The tree holds 12 migrations.                 <- missed
        // This repository ships 6 registries.           <- missed
        // There are nine crates in this repository.     <- caught
        // ```
        //
        // `AGENTS.md` states the rule this gate enforces and its own example of
        // a forbidden sentence is *"there are 41 tests"* — a digit, mid-clause,
        // which is exactly what falls through without this. And this module's header claims
        // "written numbers **as well as digits**".
        //
        // A whitelist, and deliberately on the other side from the label list
        // the paragraph above describes: that one has to name every word that
        // could *identify*, so a word it lacks is a false finding. This
        // one names the words that *count*, so a word it lacks is a miss —
        // today's behaviour — and never a wrong report. An identifying number
        // keeps its label (`Stage 10`, `page 12`, `Ubuntu 24`) and no label is
        // one of these.
        let counted = preceding
            .rsplit(|c: char| !c.is_alphanumeric())
            .find(|word| !word.is_empty())
            .is_some_and(|word| COUNTS_THEM.contains(&word.to_ascii_lowercase().as_str()));
        // **A multiplier a numeral precedes is a number this reader cannot
        // compose.** `two dozen migrations` would be reported as **12**, and this
        // module's header calls a gate that is wrong in this direction worse
        // than no gate — here the finding's own number would be wrong.
        //
        // The word *immediately* before, not `word_before_the_number`: that one
        // walks past a numeral on purpose — `one hundred` is one number written
        // in two words — which is exactly the word this needs to see.
        let composed = matches!(numeral.as_str(), "dozen" | "score" | "pair" | "couple")
            && preceding
                .rsplit(|c: char| !c.is_alphanumeric())
                .find(|word| !word.is_empty())
                .and_then(written)
                .is_some();
        let names_something_else = (in_digits && !opens_a_clause && !counted)
            || composed
            || preceding
                .rsplit(|c: char| !c.is_alphanumeric() && c != '§')
                .next()
                .is_some_and(|word| {
                    matches!(
                        word,
                        // **A subset, not a total.** `the first two invariants`
                        // and `the other two invariants` say nothing about how
                        // many there are, and both would be reported. These qualify
                        // a number written as a word, which the digit rule above
                        // does not reach.
                        "first" | "last" | "next" | "other" | "remaining" | "final"
                    | "another" | "further"
                    // **And these**, each measured reporting a subset as a
                    // total without them: `the previous two invariants`,
                    // `the following three migrations`.
                    | "previous" | "following" | "latter" | "former" | "earlier"
                    | "later" | "preceding" | "subsequent"
                    )
                })
            || preceding.ends_with('§')
            || inside_a_number
            || is_a_year
            // **The word immediately before the number.** `NOT_AN_ADJECTIVE`
            // stops the *walk*, so it only ever sees words between the number
            // and the noun — and each of these false fires has its stop word on
            // the other side:
            //
            // ```text
            // more than one hundred concurrent invariants checks -> 100 invariants
            // Nine out of ten migrations are additive            -> 10 migrations
            // Between two and five crates depend on the catalog  -> 5 crates
            // ```
            //
            // `than`, `of` and `and` each turn a number into a bound, a ratio or
            // a range, and none of the three is a count of what is in the tree.
            || stops_the_count(&preceding)
            // **And a clause that asserts nothing states no count.** Without it,
            // seven of forty ordinary sentences are reported, all one shape: the
            // number is in a supposition, a question, a prohibition or a
            // negation, and none of them says how many of anything are in this
            // tree.
            || asserts_nothing(&lowered, &preceding);
        if names_something_else {
            continue;
        }
        found.push(number);
    }
    found
}

/// Whether the clause holding the number asserts nothing.
///
/// **Seven of forty ordinary sentences, all one shape.** A number can sit in a
/// sentence that makes no claim about this tree at all, and without this every
/// one of these is reported as a stated count:
///
/// ```text
/// No two migrations share a number.          -> 2 migrations
/// Suppose three migrations fail at once.     -> 3 migrations
/// Why six invariants and not seven?          -> 6 invariants
/// It would be six invariants if the pack grew. -> 6 invariants
/// ```
///
/// A supposition, a question, a prohibition and a conditional. What they have
/// in common is mood: the sentence is not asserting that the tree holds that
/// many of anything, so there is nothing for this gate to check against a count
/// of files — and a finding a reader has to dismiss teaches them to dismiss the
/// next one, which this module's header calls worse than no gate.
///
/// Read from the number's own clause and never from the paragraph, so a
/// supposition in one sentence does not excuse a count in the next.
fn asserts_nothing(line: &str, preceding: &str) -> bool {
    /// Words that put what follows them out of the indicative. Each is matched
    /// whole: `if` is not `notify`, and `no` is not `noun`.
    const MOOD: [&str; 23] = [
        // Negation and prohibition.
        "no",
        "not",
        "nothing",
        "never",
        "neither",
        "none",
        "nobody",
        "cannot",
        // Supposition.
        "suppose",
        "supposing",
        "imagine",
        "assume",
        "assuming",
        "pretend",
        "consider",
        "considering",
        "if",
        "unless",
        "whether",
        // The conditional, and the question that opens with one.
        "would",
        "could",
        "might",
        "why",
    ];

    // **A definite reference is a claim whatever the mood.** `Do not edit the
    // six invariants by hand` states that there are six exactly as firmly as
    // `there are six invariants` does: the mood attaches to the verb and the
    // count sits in a noun phrase pointing at a set that exists. These
    // documents are written almost entirely in that register, so a mood rule
    // blind to it switches the gate off for most of what they say:
    //
    // ```text
    // Do not edit the six invariants by hand.          -> suppressed
    // None of the five migrations is destructive.      -> suppressed
    // Never run the five migrations by hand.           -> suppressed
    // Nothing outside the nine crates may log.         -> suppressed
    // ```
    //
    // Every one of those asserts a count. What the mood rule is for has no
    // article in front of the number — `No two migrations share a number`,
    // `Suppose three migrations fail`, `Why six invariants and not seven?` —
    // because there is no set being pointed at yet.
    let definite = preceding
        .split(|character: char| !character.is_alphanumeric())
        .rfind(|word| !word.is_empty())
        .is_some_and(|word| {
            matches!(
                word,
                "the" | "these" | "those" | "its" | "our" | "their" | "all" | "both"
            )
        });
    if definite {
        return false;
    }
    // The clause, not the sentence: `If two crates log, the scope holds both`
    // makes a claim on one side of the comma and a supposition on the other.
    let clause = preceding
        .rsplit(['.', ';', ':', ',', '(', '[', '—', '!', '?'])
        .next()
        .unwrap_or(preceding);
    if clause
        .split(|character: char| !character.is_alphanumeric())
        .any(|word| MOOD.contains(&word))
    {
        return true;
    }
    // And the question mark, which is at the far end of the sentence rather
    // than in front of the number: `Why six invariants and not seven?` opens
    // with a mood word, `Should six invariants be enough?` does not.
    let after = &line[preceding.len()..];
    after
        .find(['.', '!', '?'])
        .is_some_and(|at| after.as_bytes()[at] == b'?')
}

/// Whether what comes before the number means it is not a count of this tree.
///
/// **A preposition makes a ratio, and it also makes a passive sentence.** As
/// absolute stops, `of`, `by`, `from`, `on`, `with`, `to`, `in` and `for` buy a
/// zero false-fire rate on forty fresh ordinary sentences and pay for it here:
/// **eleven of twelve** stated counts written after a preposition are missed,
/// and that is how half of English states one.
///
/// ```text
/// missed  The wall is proven by eleven compile-fail fixtures.
/// missed  The core pack consists of six invariants.
/// missed  The platform is built from nine crates.
/// ```
///
/// What makes those different from `Nine out of ten migrations` and `more than
/// one hundred invariants` is not the preposition — it is what stands in front
/// of it. A number or a comparative there makes a bound or a ratio; a verb or a
/// noun makes an agent, and the count after it is a count.
fn stops_the_count(preceding: &str) -> bool {
    /// A preposition that can introduce the agent of a passive sentence, and so
    /// is only a stop when a number or a comparative precedes it.
    const AGENTIVE: [&str; 8] = ["of", "in", "for", "with", "to", "by", "on", "from"];
    /// Words that make the number after them a bound, a ratio or a multiple.
    const COMPARATIVE: [&str; 9] = [
        "more", "less", "fewer", "most", "least", "between", "twice", "half",
        // `Up to four migrations` is a ceiling, and `to` is the preposition it
        // is built from.
        "up",
    ];

    let word = word_before_the_number(preceding);
    if !NOT_AN_ADJECTIVE.contains(&word) {
        return false;
    }
    if !AGENTIVE.contains(&word) {
        return true;
    }
    // What stands in front of the preposition, past the determiners between
    // them and past `out` — `Nine out of ten` puts two words between the number
    // and the ratio it belongs to, and `Nine of the ten` puts one.
    //
    // The **raw** last word, not `word_before_the_number`: that one walks past a
    // numeral on purpose, and a numeral is exactly what this is looking for.
    let at = preceding.trim_end();
    let cut = at.rfind(word).unwrap_or(0);
    let mut before = at[..cut].trim_end();
    loop {
        let head = last_word(before);
        if head == "out" || head == "the" {
            before = before[..before.len() - head.len()].trim_end();
            continue;
        }
        // **`one` after `every` or `each` is a pronoun, not a quantity.**
        // `Every one of the six invariants has a test` states a total, and a
        // look-back reading `one of the six` as a ratio would suppress it — a
        // plain statement of a count, missed, and not on the known-miss list
        // either.
        if head == "one" {
            let outer = before[..before.len() - head.len()].trim_end();
            if matches!(last_word(outer), "every" | "each" | "Every" | "Each") {
                return false;
            }
        }
        return written(head).is_some() || COMPARATIVE.contains(&head);
    }
}

/// The last whole word of `text`, or the empty string.
fn last_word(text: &str) -> &str {
    text.rsplit(|character: char| !character.is_alphanumeric() && character != '-')
        .next()
        .unwrap_or_default()
}

/// The first word before a number that is not itself part of it.
///
/// **`one hundred` is one number written in two words**, so the word directly
/// before `hundred` is `one` — and checking that told the caller nothing. The
/// stop word is behind the whole of it: in *more than one hundred concurrent
/// invariants*, the word that decides is `than`, two back.
fn word_before_the_number(preceding: &str) -> &str {
    /// Words that carry no meaning between a stop word and the number it
    /// qualifies. `Nine of **the** ten invariants` puts one between them, and
    /// the whole phrase is a ratio either way.
    ///
    /// **`every` and `each` are not among them, and adding them costs a false
    /// fire.** `Every one of the six invariants has a test` does not need them —
    /// `stops_the_count` reads it correctly by looking behind the pronoun — so
    /// what they would change is a different shape entirely:
    ///
    /// ```text
    /// The pruner runs every six hours.   with them: claims 6 hours
    ///                                   without:   an interval, and no claim
    /// ```
    ///
    /// `every N <noun>` is a rate. Stepping past `every` hands the number to a
    /// verb — `runs` — which is no stop word, so an interval is read as a
    /// stated count and the gate goes looking for six of something.
    const CARRIES_NOTHING: [&str; 8] = ["the", "a", "an", "these", "those", "its", "our", "their"];

    let mut rest = preceding.trim_end();
    loop {
        let word = rest
            .rsplit(|c: char| !c.is_alphanumeric() && c != '-')
            .next()
            .unwrap_or_default();
        if word.is_empty() || (written(word).is_none() && !CARRIES_NOTHING.contains(&word)) {
            return word;
        }
        rest = rest[..rest.len() - word.len()].trim_end();
    }
}

/// A number written as a word or as digits.
///
/// Words as well as digits, because that is how every one of these was written —
/// "**five** compile-fail fixtures", not "5".
fn written(word: &str) -> Option<usize> {
    match word {
        "one" => Some(1),
        "two" => Some(2),
        "three" => Some(3),
        "four" => Some(4),
        "five" => Some(5),
        "six" => Some(6),
        "seven" => Some(7),
        "eight" => Some(8),
        "nine" => Some(9),
        "ten" => Some(10),
        "eleven" => Some(11),
        "twelve" | "dozen" => Some(12),
        // **Past twelve** — and thirteen is the *true* number this module's own
        // header cites ("the journal's ten integration tests", in a tree holding
        // thirteen), so a reader stopping at twelve cannot read the one value
        // the header calls out as right.
        "thirteen" => Some(13),
        "fourteen" => Some(14),
        "fifteen" => Some(15),
        "sixteen" => Some(16),
        "seventeen" => Some(17),
        "eighteen" => Some(18),
        "nineteen" => Some(19),
        "twenty" => Some(20),
        "thirty" => Some(30),
        "forty" => Some(40),
        "fifty" => Some(50),
        "sixty" => Some(60),
        "seventy" => Some(70),
        "eighty" => Some(80),
        "ninety" => Some(90),
        "hundred" => Some(100),
        // **Zero**, which is a claim that rots the moment one is written. `dozen`
        // sits with `twelve` above, because it is the same number written the
        // way a person writes it.
        "zero" | "none" => Some(0),
        // **A hyphenated compound of two number words is one number.** The
        // lookback reads a hyphenated compound as one word — deliberately, so
        // that `six well-documented invariants` is one step and not two — which
        // without this makes every number from twenty-one to ninety-nine
        // unreadable: `twenty-one invariants` and `thirty-five migrations` are
        // both invisible, and those are the numbers a growing tree reaches.
        //
        // Both halves have to be numbers, so `well-documented` and `on-disk`
        // still read as one ordinary word.
        compound => match compound.split_once('-') {
            Some((tens, units)) => match (written(tens), written(units)) {
                (Some(tens), Some(units)) if tens >= 20 && units < 10 => Some(tens + units),
                _ => None,
            },
            None => compound.parse().ok(),
        },
    }
}

/// The predicate, so `corpus.rs` can decide it case by case.
///
/// A door rather than a copy: the corpus is what `xtask/mutants.py` weakens the
/// gate against, and a corpus deciding a reimplementation would measure the
/// reimplementation.
#[cfg(test)]
pub(crate) fn claim_for_tests(line: &str, noun: &str) -> Vec<usize> {
    claim(line, noun)
}

#[cfg(test)]
mod tests {
    use super::{
        ABOVE, COUNTABLE, NOT_READ, NOT_READ_ABOVE, READ, claim, documents,
        every_written_count_matches_the_tree, unbalanced_fence, under_the_directory, written,
    };
    use std::path::Path;

    fn root() -> &'static Path {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
    }

    /// Write a scratch document, making the directory it sits in first.
    ///
    /// `ABOVE` holds a document under `reference/`, and a scratch workspace that
    /// writes its entries flat turns a test into `NotFound` rather than into a
    /// finding about what it tests.
    fn wrote(at: &Path, text: &str) {
        if let Some(parent) = at.parent() {
            std::fs::create_dir_all(parent).expect("writable");
        }
        std::fs::write(at, text).expect("writable");
    }

    #[test]
    fn this_repositorys_documents_say_what_is_there() {
        // **Skipped when there is no workspace above, and it says so.** This
        // test is *about* the workspace's documents, which live above this
        // repository (`§3.5`) — a checkout of this repository alone has none of
        // them, which is what CI has. A test that cannot be performed is not a
        // test that passed, so `check`'s test count carries a clause naming what
        // did not run: "and the ones that need the workspace above this
        // repository did not run".
        if crate::sections::workspace_of(root()).is_none() {
            println!("skipped: there is no workspace above this repository (§3.5)");
            return;
        }
        let (findings, read, _, _) =
            every_written_count_matches_the_tree(root()).expect("readable");
        assert!(findings.is_empty(), "{findings:#?}");
        // The floor: a tree whose documents lost their prose would produce no
        // findings and prove nothing.
        assert!(
            read > 0,
            "no countable claim was read, so this proved nothing"
        );
    }

    #[test]
    fn a_stray_fence_is_a_finding_rather_than_a_way_to_switch_the_gate_off() {
        // **A stray marker that lands *first* flips every real fence after it**
        // when a parser drops the last marker of an odd total, which reads the
        // file as if the stray one were an opening: measured, one ``` at line 9
        // of `AGENTS.md` makes two stated counts in ordinary prose invisible
        // with the document count unchanged and the gate green — the
        // "exclusion list turns the gate off" shape, in the parser rather than
        // in a list.
        let stray = "Prose above.\n\n```\n\nThe core holds six invariants.\n\n                     ```\ncode\n```\n\nAnd nine compile-fail fixtures.\n";
        assert_eq!(
            unbalanced_fence(stray),
            Some(9),
            "an odd number of markers was not reported"
        );
        // A balanced document says nothing, or every document would be a
        // finding and the rule would be useless.
        assert_eq!(unbalanced_fence("a\n\n```\ncode\n```\n\nb\n"), None);
        assert_eq!(unbalanced_fence("no fences here\n"), None);
        // Three is odd too, which is the case that reads as one pair plus a
        // stray whichever end it is at.
        assert!(unbalanced_fence("```\na\n```\n\n```\nb\n").is_some());
    }

    /// Fresh ordinary English, none of which states a count of this tree.
    ///
    /// **A list of label spellings reports half of fifty fresh sentences.**
    /// Every one has the same shape — a label word, a number in digits, a
    /// countable noun — and a hand-written list with `round` on it misses `run`,
    /// with `attempt` misses `task`, with `node` misses `python`. This module's
    /// own header says a gate that is wrong in this direction teaches a reader
    /// to ignore it, which is worse than no gate.
    #[test]
    fn a_label_in_front_of_a_number_is_not_a_count_however_it_is_spelled() {
        for (ordinary, noun) in [
            ("Day 3 migrations are additive", "migrations"),
            ("Week 2 invariants were reviewed", "invariants"),
            ("See page 12 invariants for the list", "invariants"),
            ("Issue 5 migrations closed this week", "migrations"),
            ("Task 3 invariants are assigned", "invariants"),
            ("Level 2 invariants apply to a collective", "invariants"),
            ("Group 4 migrations run last", "migrations"),
            ("Iteration 3 invariants were added", "invariants"),
            ("Sprint 2 migrations are queued", "migrations"),
            ("Python 3 invariants are transcribed", "invariants"),
            ("Java 8 migrations were ported", "migrations"),
            ("Ubuntu 24 crates build cleanly", "crates"),
            ("Run 3 migrations to catch up", "migrations"),
            ("Type 2 invariants are the strict ones", "invariants"),
            ("Class 3 migrations touch the schema", "migrations"),
            ("Option 2 invariants are the default", "invariants"),
            ("Schema 1 migrations create the tables", "migrations"),
            ("Priority 1 invariants block a release", "invariants"),
            ("Column 3 migrations were renumbered", "migrations"),
            // And the shapes a label list covers, which must stay covered.
            ("Stage 6 invariants are in the plan", "invariants"),
            ("Leg 3 migrations describe the schema", "migrations"),
            ("Chapter 3 invariants come first", "invariants"),
            ("PostgreSQL 18 migrations run in order", "migrations"),
            ("Round 2 invariants were reviewed", "invariants"),
            ("Version 2 migrations are numbered", "migrations"),
            ("Appendix 2 migrations describe the schema", "migrations"),
            (
                "Acceptance criterion 4 invariants are re-checked",
                "invariants",
            ),
            ("Milestone 4 invariants were agreed", "invariants"),
            ("Figure 2 migrations show the order", "migrations"),
            // Magnitudes and fractions, each of which produced a *wrong number*.
            ("Two thousand invariants would be absurd", "invariants"),
            ("Half a dozen invariants live there", "invariants"),
            ("There are two dozen migrations", "migrations"),
            // A frequency, not a count of what is in the tree.
            ("Every day six migrations run", "migrations"),
        ] {
            assert_eq!(
                claim(ordinary, noun),
                Vec::<usize>::new(),
                "this ordinary sentence was read as a stated count: {ordinary}"
            );
        }
    }

    /// A count stated after a preposition is still a count.
    ///
    /// **Eleven of twelve are missed by absolute stops**, and that is how half of
    /// English states one. `of`, `by`, `from`, `on`, `with`, `to`, `in` and
    /// `for` make a bound or a ratio — `Nine out of ten`, `more
    /// than one hundred` — and they also introduce the agent of a passive
    /// sentence, where the number after them is a plain count.
    ///
    /// What tells the two apart is not the preposition but what stands in front
    /// of it: a number or a comparative makes a ratio, a verb or a noun makes an
    /// agent.
    #[test]
    fn a_count_after_a_preposition_is_read_and_a_ratio_is_not() {
        for (stated, noun, expected) in [
            (
                "The wall is proven by eleven compile-fail fixtures.",
                "compile-fail fixtures",
                11,
            ),
            (
                "The journal is covered by thirteen integration test files.",
                "integration test files",
                13,
            ),
            ("The schema is made of six invariants.", "invariants", 6),
            ("This release ships with five migrations.", "migrations", 5),
            ("The platform is built from nine crates.", "crates", 9),
            ("The catalog is split into two registries.", "registries", 2),
            ("The core pack consists of six invariants.", "invariants", 6),
            (
                "Coverage comes from thirteen integration test files.",
                "integration test files",
                13,
            ),
            (
                "The database is brought up to date by five migrations.",
                "migrations",
                5,
            ),
            (
                "The wall rests on eleven compile-fail fixtures.",
                "compile-fail fixtures",
                11,
            ),
            ("Protection is provided by six invariants.", "invariants", 6),
            ("The build depends on nine crates.", "crates", 9),
            (
                "Each of the six invariants has a denial test.",
                "invariants",
                6,
            ),
            // **`one` is a pronoun here, not a quantity.** A walk stepping past
            // `the` to `one` finds `written("one")` succeeds and reads the phrase
            // as the ratio `one of the six` — a plain statement of a total,
            // missed, and recorded nowhere as a known miss.
            (
                "Every one of the six invariants has a test.",
                "invariants",
                6,
            ),
            (
                "Each one of the five migrations is additive.",
                "migrations",
                5,
            ),
            // **And the indicative, which the mood rule must not reach.** A
            // rule that suppresses a stated count is this gate switched off,
            // which is the failure `§19.3` is about — so every shape above is
            // paired with one that still has to fire.
            ("The pack holds seven invariants.", "invariants", 7),
            ("Six invariants have denial tests.", "invariants", 6),
            ("This tree has five migrations.", "migrations", 5),
            // **A definite reference is a claim whatever the mood.** These
            // documents are written almost entirely in the imperative and the
            // prohibitive, and a mood rule blind to that switches the gate off
            // for that whole register: `Never edit the six invariants`
            // suppressed and `Edit the six invariants` reported, one word apart.
            // The count sits in a noun phrase pointing at a set that
            // exists, and the mood attaches to the verb.
            ("Do not edit the six invariants by hand.", "invariants", 6),
            (
                "You cannot add to the six invariants without a test.",
                "invariants",
                6,
            ),
            (
                "There is no way to run the five migrations out of order.",
                "migrations",
                5,
            ),
            ("Never run the five migrations by hand.", "migrations", 5),
            (
                "No agent may edit the six invariants directly.",
                "invariants",
                6,
            ),
            ("Nothing outside the nine crates may log.", "crates", 9),
            (
                "It is not enough to run the eleven compile-fail fixtures.",
                "compile-fail fixtures",
                11,
            ),
            // **The number's own clause, not the sentence it is in.** A
            // supposition in front of the colon does not excuse the count
            // behind it, or one hypothetical opener switches the gate off for
            // everything a writer puts after it.
            (
                "Suppose the pack grew: the core holds seven invariants today.",
                "invariants",
                7,
            ),
            (
                "It would be more one day, but there are five migrations now.",
                "migrations",
                5,
            ),
        ] {
            assert_eq!(
                claim(stated, noun),
                vec![expected],
                "a stated count after a preposition was not read: {stated}"
            );
        }
    }

    /// And what those stops were made for, which must not fire.
    ///
    /// **Paired with the test above on purpose.** A rule that suppresses a
    /// stated count is this gate switched off, and the two halves are what keep
    /// each other honest: every shape here has a sentence there that still has
    /// to be read.
    #[test]
    fn a_ratio_a_bound_a_rate_and_a_supposition_are_not_counts() {
        for (ratio, noun) in [
            ("Nine out of ten migrations are additive.", "migrations"),
            // **A rate, not a count.** `every N <noun>` says how often, and
            // reading it as a claim sends the gate looking for six of
            // something. It is read as one if `every` and `each` sit in
            // `CARRIES_NOTHING`, for a phrase `stops_the_count` already handles
            // on its own.
            ("The pruner runs every six hours.", "hours"),
            ("It vacuums each seven days.", "days"),
            // **A clause that asserts nothing states no count.** Without it,
            // seven of forty ordinary sentences are reported, all this one
            // shape: the number sits in a supposition, a question, a prohibition
            // or a negation, and none of them says how many of anything this
            // tree holds.
            ("No two migrations share a number.", "migrations"),
            ("Nothing stops two crates from logging.", "crates"),
            ("Never write three migrations in one change.", "migrations"),
            ("Suppose three migrations fail at once.", "migrations"),
            ("Imagine four fixtures refusing to compile.", "fixtures"),
            ("Consider five policies with no denial test.", "policies"),
            ("If two crates log, the scope holds both.", "crates"),
            ("It would be six invariants if the pack grew.", "invariants"),
            // The question mark is at the far end of the sentence rather than
            // in front of the number, so it is read from the other side.
            ("Why six invariants and not seven?", "invariants"),
            ("Should six invariants be enough?", "invariants"),
            (
                "Nine of the ten invariants are Cedar policies.",
                "invariants",
            ),
            (
                "More than one hundred concurrent invariants are checked.",
                "invariants",
            ),
            ("Up to four migrations may be pending.", "migrations"),
            (
                "Between two and five crates depend on the catalog.",
                "crates",
            ),
            ("Twice five invariants would be too many.", "invariants"),
            (
                "Half of the six invariants mention the journal.",
                "invariants",
            ),
            (
                "At least five migrations run on a cold database.",
                "migrations",
            ),
        ] {
            assert_eq!(
                claim(ratio, noun),
                Vec::<usize>::new(),
                "a bound or a ratio was reported as a count: {ratio}"
            );
        }
    }

    /// And the counts a document really does state are still read.
    ///
    /// The other half of the test above: a rule that suppressed everything
    /// would pass it and be a gate wired to nothing.
    #[test]
    fn a_stated_count_is_still_read_after_the_label_rule() {
        for (stated, noun, expected) in [
            ("There are six migrations", "migrations", 6_usize),
            ("Six invariants live there", "invariants", 6),
            ("six pending migrations", "migrations", 6),
            ("six on-disk invariants", "invariants", 6),
            ("**five** compile-fail fixtures", "compile-fail fixtures", 5),
            ("three migrations run in order", "migrations", 3),
            // Digits opening a clause are still a count: nothing labels them.
            ("3 migrations run in order", "migrations", 3),
            ("The tree holds: 7 invariants", "invariants", 7),
        ] {
            assert_eq!(
                claim(stated, noun),
                vec![expected],
                "this stated count stopped being read: {stated}"
            );
        }
    }

    #[test]
    fn ordinary_english_is_not_a_stated_count() {
        // **The direction this module's own header calls worse than no gate.**
        // Fifteen sentences written into `AGENTS.md` and put through a gate
        // without these rules: **eight of eight** ordinary sentences reported as
        // stated counts, and four real ones missed. A gate that is wrong in the
        // reporting direction teaches a reader to ignore it, and then the
        // direction that matters is off too.
        //
        // Every sentence below is one of those, verbatim.
        for ordinary in [
            // A label in front of its own number — the `Stage 6` shape, with a
            // different keyword each time.
            "Version 2 migrations are numbered in order.",
            "Acceptance criterion 4 invariants are re-checked each release.",
            "Appendix 2 migrations describe the schema in full.",
            "Round 2 invariants were reviewed by a second reader.",
            "Chapter 3 invariants are the ones a reader meets first.",
            // A year. The dated form `2026-08-25` is suppressed separately; the
            // year alone, unsuppressed, reads as a count of two thousand and
            // twenty-six.
            "In 2026 migrations were re-verified on this machine.",
            "Since 2025 invariants have been enforced by a gate.",
            // A time of day, which is the same suppression as a version number
            // with a different separator.
            "At 09:00 migrations run against the staging database.",
        ] {
            let noun = if ordinary.contains("invariants") {
                "invariants"
            } else {
                "migrations"
            };
            assert_eq!(
                claim(ordinary, noun),
                Vec::<usize>::new(),
                "ordinary English was reported as a stated count: {ordinary}"
            );
        }

        // **And the other direction, or the fix above is just a wider hole.**
        // Two of the four misses are closed here; the other two are left, with
        // the reason, below.
        assert_eq!(
            claim(
                "There are twenty-one invariants in the pack today.",
                "invariants"
            ),
            vec![21],
            "a hyphenated number word was invisible"
        );
        assert_eq!(
            claim(
                "The repository ships thirty-five migrations right now.",
                "migrations"
            ),
            vec![35]
        );
        // A compound that is not two numbers stays one ordinary word, which is
        // what the lookback reads a hyphenated compound as and why it does.
        assert_eq!(
            claim("six well-documented invariants live there", "invariants"),
            vec![6]
        );
        assert_eq!(
            claim("the on-disk invariants are checked", "invariants"),
            Vec::<usize>::new()
        );

        // **And a number that is a bound, a ratio or a range is not a count.**
        // `NOT_AN_ADJECTIVE` stops the *walk*, so it only ever saw words between
        // the number and the noun; each of these has its stop word on the other
        // side, and each would be reported.
        for ordinary in [
            "PostgreSQL rejects more than one hundred concurrent invariants checks.",
            "Nine out of ten migrations are additive.",
            "Between two and five crates depend on the catalog.",
        ] {
            let noun = if ordinary.contains("invariants") {
                "invariants"
            } else if ordinary.contains("migrations") {
                "migrations"
            } else {
                "crates"
            };
            assert_eq!(
                claim(ordinary, noun),
                Vec::<usize>::new(),
                "a bound, a ratio or a range was reported as a count: {ordinary}"
            );
        }
    }

    #[test]
    fn a_hedged_number_is_not_a_stated_count() {
        // **A hedge is not a count either.** Without this, eleven of twenty-two
        // fresh ordinary sentences are reported, every one of them saying the
        // number is approximate or a bound. Catching `Nine out of ten
        // migrations` and not `Nine of the ten invariants` closes a spelling,
        // and this is the shape.
        for hedged in [
            "At most three migrations are ever pending.",
            "At least five migrations have to run first.",
            "Roughly six or seven invariants are in the pack.",
            "Nine of the ten invariants are Cedar policies.",
            "Around twenty invariants would be too many.",
            "About seven invariants is where this settled.",
            "Each release adds roughly two migrations.",
            "Approximately six invariants cover the core.",
            "Perhaps five migrations will be needed.",
            "Nearly nine crates depend on the catalog.",
            "Twice five invariants would be ten.",
            "Up to four migrations may be pending.",
            "Over six invariants would be a smell.",
            "Some six invariants live there.",
        ] {
            let noun = if hedged.contains("invariants") {
                "invariants"
            } else if hedged.contains("migrations") {
                "migrations"
            } else {
                "crates"
            };
            assert_eq!(
                claim(hedged, noun),
                Vec::<usize>::new(),
                "a hedged number was reported as a stated count: {hedged}"
            );
        }

        // **Nine misses left standing, on purpose, and this is the list.**
        // Reading any of them means widening the lookback or reading a number
        // that follows its noun, and measured, each widening buys one catch and
        // several false fires: `ADJECTIVES` is two words rather than four for
        // exactly that reason. A miss is a count nobody checked; a false fire
        // is a gate nobody reads, and then both directions are gone.
        //
        // Written down so the next reader knows they are known rather than
        // undiscovered, and so a widening that starts catching one is a
        // deliberate two-place edit rather than a surprise.
        for (missed, noun) in [
            // A bracket ends the lookback at an empty word.
            (
                "The core holds six (live) invariants at the moment.",
                "invariants",
            ),
            // The number follows the noun. Four shapes, one cause.
            ("Invariants: six.", "invariants"),
            ("The number of migrations is five.", "migrations"),
            ("The count of invariants is six.", "invariants"),
            ("Migrations number five in total.", "migrations"),
            // `of` stops the walk before the number is reached.
            ("We now have six of these invariants.", "invariants"),
            // **Two adjectives, not three.** `ADJECTIVES` is the number of
            // *steps* the walk takes, and one of them is the number itself — so
            // the boundary is one adjective reached and two missed. Every one of
            // these is a phrase somebody would write:
            //
            // ```text
            // six pending schema migrations    six brand new invariants
            // six small new invariants         six core Cedar invariants
            // ```
            ("six pending schema migrations", "migrations"),
            ("six brand new invariants", "invariants"),
            (
                "Six carefully written core invariants live there.",
                "invariants",
            ),
            // **The second item of a list**, which the stop-word rule costs.
            // `There are six migrations and seven invariants.` reports the six
            // and not the seven, because `and` suppresses — and without that
            // rule it reports both *and* eleven ordinary sentences besides. The
            // trade is measured: six fewer false fires for this one miss.
            (
                "There are six migrations and seven invariants.",
                "invariants",
            ),
            // One adjective with a comma before it: the comma ends the walk,
            // so this is missed for a different reason than the two above.
            (
                "We ship six well-tested, hand-written migrations.",
                "migrations",
            ),
        ] {
            assert_eq!(
                claim(missed, noun),
                Vec::<usize>::new(),
                "this is a known miss; if it starts being read, say so here: {missed}"
            );
        }
    }

    #[test]
    fn a_number_before_a_countable_noun_is_a_claim() {
        assert_eq!(
            claim("the five invariants, and tests/", "invariants"),
            vec![5]
        );
        assert_eq!(
            claim(
                "Five compile-fail fixtures each declare",
                "compile-fail fixtures"
            ),
            vec![5]
        );
        // **Bold, italic and backticked**, which is how every one of these was
        // written — and `**five**` ends in `*`, so a reader taking the last
        // alphanumeric run finds it empty and the number invisible.
        for emphasised in [
            "there are **five** compile-fail fixtures in this tree",
            "there are *five* compile-fail fixtures in this tree",
            "there are `five` compile-fail fixtures in this tree",
            "there are __five__ compile-fail fixtures in this tree",
            // **Emphasis inside the noun, not on the number.** Every row above
            // emphasises the count; the blanking above replaces each `*` with a
            // space, and `COUNTABLE`'s nouns are written with exactly one — so
            // uncollapsed, `**compile-fail** fixtures` leaves two spaces where
            // the needle has one and produces *no claim at all*. Bolding the
            // adjective is how anybody writes it, and it would switch the gate off.
            "there are five **compile-fail** fixtures in this tree",
            "there are five compile-fail `fixtures` in this tree",
            "there are five _compile-fail_ **fixtures** in this tree",
        ] {
            assert_eq!(
                claim(emphasised, "compile-fail fixtures"),
                vec![5],
                "{emphasised}"
            );
        }
        // **Every occurrence**, not the first: a sentence naming the noun once
        // without a number would hide a count later on the same line.
        assert_eq!(
            claim(
                "each invariant is a file, and there are six invariants",
                "invariants"
            ),
            vec![6]
        );
        // And a sentence with no number before the noun is not a claim.
        assert_eq!(
            claim("the invariants are in policy/core", "invariants"),
            [] as [usize; 0]
        );
        // **The singular is not a count**, however it is spelled: the article
        // and the number are the same word, and `one registry deciding which
        // floating list is open` — prose about another repository entirely —
        // would be reported as a claim that this tree holds one registry.
        for prose in [
            "every compile-fail fixture declares",
            "one compile-fail fixture is enough to prove it",
            "a second compile-fail fixture would say nothing new",
        ] {
            assert!(
                claim(prose, "compile-fail fixtures").is_empty(),
                "`{prose}` is not a count and was reported as one"
            );
        }
        assert_eq!(
            claim(
                "one registry deciding which floating list is open",
                "registries"
            ),
            [] as [usize; 0]
        );
    }

    #[test]
    fn a_count_that_opens_a_sentence_is_still_a_count() {
        // **A suppression that turns the gate off for most of the prose.** A
        // bare `preceding.ends_with('.')`, so `§19.4 invariants` is not read as
        // four, hides every count that begins a sentence — measured, most of the
        // sentences in the documents this reads. The first of these is the
        // sentence the gate's own fix text recommends, followed by the stale
        // number it is meant to replace.
        for prose in [
            "Read `ls policy/core`. Six invariants live there",
            "The pack is small, e.g. six invariants and a README",
            "Everything is checked. Nine compile-fail fixtures prove the wall",
            "The wall is proven. Ten integration test files cover the journal",
        ] {
            assert!(
                !claim(prose, "invariants").is_empty()
                    || !claim(prose, "compile-fail fixtures").is_empty()
                    || !claim(prose, "integration test files").is_empty(),
                "`{prose}` states a count and was not reported"
            );
        }
        // And the case the suppression was for still holds: the digit before the
        // dot is what makes `19.4` one number rather than two.
        assert_eq!(
            claim("see §19.4 invariants for the rule", "invariants"),
            [] as [usize; 0]
        );
        assert_eq!(
            claim("19.4 invariants is not a count", "invariants"),
            [] as [usize; 0]
        );
    }

    #[test]
    fn an_adjective_between_the_number_and_the_noun_does_not_hide_it() {
        // `six live invariants` and `nine new compile-fail fixtures` are both
        // silent to a scan reading the word touching the noun and nothing further.
        assert_eq!(
            claim("the core holds six live invariants", "invariants"),
            vec![6]
        );
        assert_eq!(
            claim(
                "the tree carries nine new compile-fail fixtures",
                "compile-fail fixtures"
            ),
            vec![9]
        );
        // And what two steps deliberately does not reach, stated rather than
        // left to be found. Four steps read `Six people wrote migrations` and
        // `It took six days to write invariants` as counts, and stopping each
        // needs another stop word.
        assert_eq!(
            claim(
                "there are three carefully written new invariants",
                "invariants"
            ),
            [] as [usize; 0]
        );
        // And it does not reach across a sentence to find one.
        assert_eq!(
            claim("There are six of them. Invariants live here", "invariants"),
            [] as [usize; 0]
        );
        assert_eq!(
            claim("we shipped six things, and invariants too", "invariants"),
            [] as [usize; 0]
        );
        // And a conjunction, which says the number belongs to a different noun.
        assert_eq!(
            claim("the six tests and the invariants", "invariants"),
            [] as [usize; 0]
        );
        assert_eq!(
            claim("six migrations for the invariants", "invariants"),
            [] as [usize; 0]
        );
        // **A unit, a verb or a product name between the number and the noun**,
        // each measured turning ordinary prose into a finding.
        for prose in [
            "it took six days to write invariants",
            "six weeks before migrations were added, the wall existed",
            "running three times the invariants would be slow",
            "PostgreSQL 18 migrations run in order",
        ] {
            assert!(
                claim(prose, "invariants").is_empty() && claim(prose, "migrations").is_empty(),
                "`{prose}` states no count and was reported"
            );
        }
    }

    #[test]
    fn punctuation_touching_the_noun_does_not_hide_the_count() {
        // A lookback taking the last *alphanumeric run* ends the walk at an
        // empty word on a hyphen or a closing quote against the noun.
        for (prose, noun) in [
            (
                "the core holds six well-documented invariants",
                "invariants",
            ),
            ("the core holds six \"live\" invariants", "invariants"),
            (
                "there are nine hand-written compile-fail fixtures",
                "compile-fail fixtures",
            ),
        ] {
            assert!(
                !claim(prose, noun).is_empty(),
                "`{prose}` states a count and was not reported"
            );
        }
    }

    /// The gate reaches a stated count wherever in a document it sits.
    ///
    /// **No counter can answer this, which is why it is a fixture.** A plant that
    /// skips every paragraph past the tenth *and increments the counter anyway*
    /// keeps every number honest — measured, the `… document(s) … in …
    /// paragraph(s)` line and `check passed`, byte-identical to baseline, with a
    /// rotten count in the part that was skipped. The exact paragraph floor
    /// catches a narrowed **walk** (`.take(10)` leaves a small fraction of the
    /// paragraphs judged) and cannot catch a narrowed **judgement**.
    ///
    /// What answers it is a defect at a position nothing can predict. The number
    /// of filler paragraphs comes from this process's own id, so a filter tuned
    /// to any fixed depth misses it.
    #[test]
    fn the_gate_reaches_a_stated_count_wherever_it_sits() {
        let workspace = std::env::temp_dir().join(format!("nc-depth-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&workspace);
        let scratch = workspace.join("repos/central");
        std::fs::create_dir_all(&scratch).expect("writable");
        std::fs::write(workspace.join(".noctua-workspace"), "").expect("writable");
        for name in ABOVE {
            wrote(&workspace.join(name), "# nothing here\n");
        }
        for name in READ {
            wrote(&scratch.join(name), "# nothing here\n");
        }
        // **Past the longest document this gate reads, not within a window of
        // sixty.** `pid % 60 + 5` would put the claim between paragraphs 5 and
        // 64 — a window written in the line itself — so a filter skipping every
        // paragraph past the eightieth is invisible to it while a rotten count in
        // `AGENTS.md` goes unreported with the paragraph number *up*.
        // **The real tree, not the scratch.** Measured against the scratch — which
        // is empty at this point — `longest` is zero and the plant lands in the
        // first hundred lines, which is the window this widening exists to
        // remove. The number that matters is the longest file the gate reads when
        // it runs for real.
        let longest = crate::gates::longest_file_read(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/.."
        )));
        let depth = longest + 1 + std::process::id() as usize % 100;
        // The relationship, asserted rather than assumed: `longest + 1 > longest`
        // is a tautology, and the number has to come from the tree a second time
        // for the comparison to mean anything.
        assert!(
            depth > longest && longest > 100,
            "the fixture is {depth} deep and the longest file this gate reads is \
             {longest}: a filter tuned above the fixture would be invisible to it"
        );
        let mut document = String::new();
        for at in 0..depth {
            use core::fmt::Write as _;
            let _ = writeln!(
                document,
                "Filler paragraph number {at}, which states nothing.\n"
            );
        }
        document.push_str("The core holds six invariants.\n");
        // **In every place the walk reads, not just the root.** A claim in the
        // scratch's own `README.md` alone leaves a filter naming a *directory* —
        // `if relative.starts_with("policy/") { judged += 1; continue; }`, with
        // the counter kept honest — invisible while a rotten count under
        // `policy/` goes unreported. A fixture that answers "did the walk
        // reach the line" has to be at every place the walk goes, or it answers
        // only for one of them.
        // **And above the repository.** The gate reads the documents here and
        // every one of `ABOVE`, whose own doc calls the workspace "the worst
        // place" for a stale count. With every planted claim inside the scratch
        // *repository*, one branch keyed on `inside` — the variable the walk
        // already has — skips every document of `ABOVE` with the paragraph
        // count going *up* and `check passed`, the stated count still sitting in
        // the workspace's `AGENTS.md`.
        //
        // The rule this test's own comment states holds across the two trees as
        // well as across directories.
        //
        // **And every directory the real tree keeps a read document in, rather
        // than a few written here.** A fixture naming the scratch root,
        // `policy/core/`, `policy/packs/family/` and two of `ABOVE` never plants
        // in `xtask/canaries/` — the directory of the file `AGENTS.md` names to
        // read first — so a walk narrowed to exactly that directory is invisible
        // to it. A fixture that names its own places answers for the places
        // somebody remembered; one that reads the tree answers for the directory
        // added tomorrow, on the day it is added.
        //
        // **Derived from `READ`, never from the hole.** Asking the production
        // exclusion which directories to plant in would make this fixture agree
        // with every widening of that exclusion, which is the derived
        // expectation this file refuses throughout. The places come
        // from the file *names* in `READ`, a list held against the tree in both
        // directions by `every_kind_on_disk_is_either_read_or_written_down_as_
        // unread`'s sibling here. So a directory added to `NOT_READ` still gets
        // a plant, the plant goes unreported, and this fails.
        let mut where_a_read_document_lives = std::collections::BTreeSet::new();
        crate::gates::walk(root(), &mut |path| {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if !READ.contains(&name) {
                return;
            }
            let relative = crate::relative_to(root(), path);
            where_a_read_document_lives.insert(
                relative
                    .rsplit_once('/')
                    .map_or(String::new(), |(at, _)| at.to_owned()),
            );
        })
        .expect("walkable");
        assert!(
            where_a_read_document_lives.len() >= 6,
            "{} directories hold a document this gate reads, which is not this tree: a \
             fixture that plants in fewer places than the tree has answers only for the \
             places it planted",
            where_a_read_document_lives.len()
        );
        let places = where_a_read_document_lives
            .iter()
            .map(|at| {
                if at.is_empty() {
                    scratch.join("README.md")
                } else {
                    scratch.join(at).join("README.md")
                }
            })
            .chain(ABOVE.iter().map(|name| workspace.join(name)))
            .collect::<Vec<_>>();
        for place in &places {
            if let Some(parent) = place.parent() {
                std::fs::create_dir_all(parent).expect("writable");
            }
            std::fs::write(place, &document).expect("writable");
        }

        let (findings, _, _, judged) =
            every_written_count_matches_the_tree(&scratch).expect("readable");
        for place in &places {
            // A document above the repository is named `<workspace>/…`, which is
            // what the walk prints and what a reader of a finding sees.
            let named = place.strip_prefix(&scratch).map_or_else(
                |_| {
                    format!(
                        "<workspace>/{}",
                        place
                            .strip_prefix(&workspace)
                            .expect("under the workspace")
                            .to_string_lossy()
                            .replace('\\', "/")
                    )
                },
                |inside| inside.to_string_lossy().replace('\\', "/"),
            );
            assert!(
                findings
                    .iter()
                    .any(|finding| finding.file == named && finding.what.contains("6 invariants")),
                "the gate did not reach a stated count {depth} paragraph(s) down in \
                 `{named}`, having judged {judged}: a filter between the counter and the \
                 judgement leaves every count where it was: {findings:#?}"
            );
        }
        let _ = std::fs::remove_dir_all(&workspace);
    }

    #[test]
    fn a_fenced_block_is_not_prose() {
        // These documents hold directory trees, shell transcripts and SQL.
        // Joining their lines produces sentences nobody wrote — `… (Legs 2, 3)`
        // above `├── crates/` reads as "3 crates", a finding against a tree
        // diagram — and a count *inside* a block is output or an example, never
        // a claim about this tree.
        let workspace = std::env::temp_dir().join(format!("nc-fenced-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&workspace);
        let scratch = workspace.join("repos/central");
        std::fs::create_dir_all(&scratch).expect("writable");
        std::fs::create_dir_all(workspace.join("repos")).expect("writable");
        // The marker is what names a workspace (`NOCTUA.md §3.6`), so a scratch
        // tree that stands in for one carries it — a document beside a `repos/`
        // is a shape any copy can have, so a detector asking only that would
        // disagree with one asking for the marker.
        std::fs::write(workspace.join(crate::sections::MARKER), "").expect("writable");
        for document in ABOVE {
            wrote(&workspace.join(document), "Read `ls policy/core`.\n");
        }
        std::fs::write(
            scratch.join("AGENTS.md"),
            "The layout:\n\n```\ntree/ (Legs 2, 3)\n├── crates/\n```\n\n             And the count six invariants, which is prose.\n",
        )
        .expect("writable");
        let (findings, _, _, _) = every_written_count_matches_the_tree(&scratch).expect("readable");
        assert_eq!(
            findings.len(),
            1,
            "the fenced block was read as prose, or the prose was not: {findings:#?}"
        );
        assert!(findings[0].what.contains("invariant"), "{findings:#?}");
        let _ = std::fs::remove_dir_all(&workspace);
    }

    #[test]
    fn a_subset_is_not_a_total() {
        // `the first two invariants` and `the other two invariants` say nothing
        // about how many there are, and both would be reported — the
        // wrong-direction failure this module says is worse than no gate.
        for prose in [
            "the first two invariants are enforced by a gate",
            "the other two invariants are about the log",
            "the last three migrations add constraints",
            "the remaining four invariants are policies",
            "another two invariants arrived with the pack",
            // And these, each measured reporting a subset without the rule.
            "the previous two invariants are about the log",
            "the following three migrations add constraints",
            "the latter two invariants are the pack's",
            "the earlier three migrations are applied",
        ] {
            assert!(
                claim(prose, "invariants").is_empty() && claim(prose, "migrations").is_empty(),
                "`{prose}` names a subset and was read as a total"
            );
        }
    }

    #[test]
    fn ordinary_prose_is_not_a_stated_count() {
        // Every one of these is measured being reported without it, and the module's own
        // header calls a gate wrong in this direction worse than no gate.
        for (prose, noun) in [
            (
                "the pack ships six policies, invariants and schemas",
                "invariants",
            ),
            (
                "Leg 3 migrations were applied during that stage",
                "migrations",
            ),
            (
                "Milestone 4 invariants were reviewed last week",
                "invariants",
            ),
            (
                "Postgres 18 migrations run in the order they are numbered",
                "migrations",
            ),
            ("PostgreSQL 18 migrations run in order", "migrations"),
            (
                "on 2026-08-25 migrations were re-verified on this machine",
                "migrations",
            ),
            ("six people wrote migrations for this tree", "migrations"),
            ("it took six days to write invariants", "invariants"),
            (
                "running three times the invariants would be slow",
                "invariants",
            ),
        ] {
            assert!(
                claim(prose, noun).is_empty(),
                "`{prose}` states no count and was reported"
            );
        }
    }

    #[test]
    fn a_stated_count_is_read_however_it_is_written() {
        // The other direction, on the same batch: each of these is a real
        // stated count a wider suppression misses.
        for (prose, noun, number) in [
            (
                "there are six PostgreSQL migrations in this repository",
                "migrations",
                6,
            ),
            (
                "the core holds six on-disk invariants today",
                "invariants",
                6,
            ),
            (
                "the core holds six well-documented invariants",
                "invariants",
                6,
            ),
            ("the tree holds a dozen invariants", "invariants", 12),
            ("there are zero invariants in this pack", "invariants", 0),
            ("the core holds sixty invariants", "invariants", 60),
        ] {
            assert_eq!(claim(prose, noun), vec![number], "{prose}");
        }
    }

    #[test]
    fn a_domain_or_a_path_is_not_the_noun() {
        // `Seven crates on crates.io` would be reported twice — once for the count
        // and once for the registry's name, as `7 and 7 crates`.
        assert_eq!(claim("seven crates on crates.io today", "crates"), vec![7]);
        assert_eq!(claim("published to crates.io", "crates"), [] as [usize; 0]);
        assert_eq!(
            claim("under migrations/0001.sql", "migrations"),
            [] as [usize; 0]
        );
        // A criterion number in front of a directory is not a count of crates,
        // which is the shape a table of pointers is written in.
        assert_eq!(
            claim("3 crates/nc-journal/tests/tampering.rs", "crates"),
            [] as [usize; 0]
        );
        assert_eq!(
            claim("see crates/nc-log/fixtures for the six fixtures", "crates"),
            [] as [usize; 0]
        );
    }

    #[test]
    fn a_stage_or_a_section_number_is_not_a_count() {
        // **The wrong-direction failure this module says is worse than no
        // gate.** `Stage 6 invariants` and `§19.4 invariants` would both be
        // reported as counts — one of them with the number "4", read out of `19.4`.
        for prose in [
            "Stage 6 invariants are checked before the gate runs",
            "see §19.4 invariants for the rule that governs this",
            "19.4 invariants is not a count",
            "invariant 6 invariants",
            "rule 3 invariants",
        ] {
            assert!(
                claim(prose, "invariants").is_empty(),
                "`{prose}` is not a count and was reported as one"
            );
        }
    }

    #[test]
    fn every_markdown_file_is_read_or_named_in_the_hole() {
        // `gates.rs` states the rule: **an exclusion list is a claim and needs
        // its own test.** `NOT_READ` claims that the only Markdown this gate
        // skips is generated; a document added tomorrow under a name `READ` does
        // not hold would otherwise fall through the gap in silence.
        let mut skipped = std::collections::BTreeSet::new();
        let mut walked = 0_usize;
        crate::gates::walk(root(), &mut |path| {
            if path.extension().is_none_or(|kind| kind != "md") {
                return;
            }
            walked += 1;
            let relative = crate::relative_to(root(), path);
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            // **The hole first, and the production rule rather than a second
            // copy of it.** Asking `READ.contains(name)` before the hole, and
            // matching the hole by segment, disagrees with the filter twice
            // over: a `README.md` inside the hole is called *read* while the
            // gate skips it, and a hole named by a directory means that
            // directory anywhere. Both together make `policy/docs/README.md` <!-- allow-reference: names `policy/docs/README.md`, the plant that proved this hole, which is deliberately not in the tree -->
            // a document nothing judges and nothing can notice — the walk
            // excludes it and the test declares it read.
            let hole = NOT_READ
                .iter()
                .find(|(entry, _)| super::the_hole(entry, &relative));
            if hole.is_some() {
                skipped.insert(relative);
                return;
            }
            if super::WRITTEN_BY_THE_RELEASE.contains(&relative.as_str()) {
                return;
            }
            assert!(
                READ.contains(&name),
                "{relative} is Markdown this gate neither reads nor declares it skips. \
                 Add its name to READ, or add its directory to NOT_READ with the reason"
            );
        })
        .expect("walkable");
        assert!(walked > 0, "no Markdown was walked, so this proved nothing");
        // **And no exclusion for nothing.** A prefix that excludes no file is a
        // hole with nothing in it, which reads as caution and is really rot.
        for (entry, _) in NOT_READ {
            assert!(
                skipped.iter().any(|path| super::the_hole(entry, path)),
                "NOT_READ excludes `{entry}` and nothing is there"
            );
        }

        // **And the hole is where it says it is.** `/docs` is one directory;
        // the same name elsewhere is not excused, and a document written there
        // is judged like any other. Measured with the name matched anywhere: `policy/docs/README.md` <!-- allow-reference: names `policy/docs/README.md`, the plant that proved this hole, which is deliberately not in the tree -->
        // stating six invariants against seven policies leaves `14 document(s)
        // state no count that can rot` byte-identical and the suite passing.
        assert!(
            super::the_hole("/docs", "docs/commands.md"),
            "the generated directory is not in its own hole"
        );
        assert!(
            !super::the_hole("/docs", "docs/README.md"),
            "`docs/README.md` is hand-written and the hole written about the generated \
             pages excuses it"
        );
        for elsewhere in ["policy/docs/README.md", "crates/nc-log/docs/README.md"] {
            assert!(
                !super::the_hole("/docs", elsewhere),
                "`{elsewhere}` is excused by a reason written about the generated \
                 directory at the root"
            );
        }
        // A name without the leading `/` still means that name anywhere, which
        // is what the canary fixtures need: they live in two directories and
        // neither is the root.
        for anywhere in [
            "xtask/canaries/citations.md",
            "crates/nc-canon/canaries/citations.md",
        ] {
            assert!(
                super::the_hole("citations.md", anywhere),
                "`{anywhere}` is a canary and was not excused"
            );
        }
    }

    #[test]
    fn every_workspace_document_is_one_this_gate_reads() {
        // **The other half of `every_markdown_file_is_read_or_named_in_the_hole`.**
        // That walk starts at `root()` and so cannot reach above the
        // repository, which is where many of this gate's documents live — so
        // without this `ABOVE` is a hand-written list checked against nothing in
        // either direction, and a document it leaves out —
        // `reference/identity-and-authorization.md`, first in `NOCTUA.md §0.4`'s
        // authority order, say — is one where a planted count passes.
        //
        // `repos/` holds other repositories, each with its own gate over its own
        // documents, and a dotted directory is scratch rather than something a
        // person is told to read. Both are rules rather than names, so a
        // directory added tomorrow falls on the right side of them by itself.
        // The production walk, not a second copy of it: a test that walks the way
        // it wishes the gate walked proves nothing about the gate.
        let documents_under = |at: &Path| -> Vec<String> {
            super::the_workspace_documents(at)
                .iter()
                .map(|path| crate::relative_to(at, path))
                .collect()
        };

        // The hole first, on a scratch workspace, because the rule has to hold
        // where there is no workspace above this repository at all — which is
        // every checkout, and is the shape this gate announces it cannot judge.
        let scratch = std::env::temp_dir().join(format!("nc-above-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join("reference")).expect("writable");
        for document in ABOVE {
            let at = scratch.join(document);
            if let Some(parent) = at.parent() {
                std::fs::create_dir_all(parent).expect("writable");
            }
            std::fs::write(at, "Read `ls policy/core`.\n").expect("writable");
        }
        let mut planted = documents_under(&scratch);
        planted.sort();
        assert_eq!(
            planted,
            {
                let mut named: Vec<String> = ABOVE.iter().map(|one| (*one).to_owned()).collect();
                named.sort();
                named
            },
            "the walk does not find exactly the documents `ABOVE` names, so neither \
             direction of the comparison below means anything"
        );
        std::fs::write(
            scratch.join("reference/a-new-document.md"),
            "Nine of them.\n",
        )
        .expect("writable");
        let with_one_more = documents_under(&scratch);
        assert!(
            with_one_more.contains(&"reference/a-new-document.md".to_owned()),
            "a document added to the workspace is invisible to this walk, which is \
             the failure the walk exists to make impossible"
        );
        let _ = std::fs::remove_dir_all(&scratch);

        // And the workspace this tree actually sits in, when there is one.
        let Some(workspace) = crate::sections::workspace_of(root()) else {
            // **It says so when it stands down.** The half it abandons is the
            // one that means anything — `ABOVE`'s names held against the real
            // workspace, and the two assertions that keep `NOT_READ_ABOVE`
            // honest — and `NOT_READ_ABOVE`'s own doc says this test "holds
            // this one in both directions". In a bare checkout, which is what
            // CI always has, it holds neither, and a silent stand-down leaves the
            // run no way to know. The tree's own remedy for a test that cannot
            // run here is to make it say so, which is what its two siblings in
            // this file do.
            println!("skipped: there is no workspace above this repository (§3.5)");
            return;
        };
        let mut on_disk = documents_under(&workspace);
        on_disk.sort();
        // **And no assertion that the disk holds only what `ABOVE` names**,
        // because it would be wrong twice over. A document `ABOVE` does not name
        // does not have "a count that has rotted in it pass this gate" — the
        // walk reads it, so a rotted count in it is reported, which is the whole
        // point of walking. And the workspace is shared with other
        // repositories: a contributing guide written beside them, or a brief
        // dropped in for an hour, would fail `noctua-central`'s gate with a
        // message telling the developer to go and edit `xtask`.
        //
        // What is checked is the direction that means something: a name here that
        // the disk does not hold is a name that has rotted.
        for document in ABOVE {
            assert!(
                on_disk.iter().any(|found| found == document),
                "`ABOVE` names `{document}` and the workspace does not hold it"
            );
        }
        // **And no exclusion for nothing**, the same way `NOT_READ` is held: a
        // directory excused here that holds no document reads as caution and is
        // really rot. The walk skips it, so the disk is what says it is there.
        for (directory, _) in NOT_READ_ABOVE {
            let at = workspace.join(directory);
            assert!(
                at.is_dir()
                    && std::fs::read_dir(&at)
                        .expect("readable")
                        .flatten()
                        .any(|entry| entry.path().extension().is_some_and(|kind| kind == "md")),
                "`NOT_READ_ABOVE` excuses `{directory}` and no document is there"
            );
            assert!(
                !on_disk
                    .iter()
                    .any(|found| under_the_directory(found, directory)),
                "`NOT_READ_ABOVE` excuses `{directory}` and the walk read it anyway"
            );
            // **And a file whose name merely begins with the directory's is
            // read.** With the exclusion and this assertion both asking
            // `starts_with`, the test would agree with the walk about a hole
            // wider than the sentence describing it.
            assert!(
                !under_the_directory(&format!("{directory}.md"), directory),
                "`{directory}.md` is a file beside `{directory}`, and the exclusion \
                 covers it — which is a hole nobody wrote down"
            );
        }
    }

    #[test]
    fn the_hole_is_where_the_list_says_it_is() {
        // The hole, fired on purpose: the same sentence is a finding in a file
        // this gate reads and is silent in one it declares it skips. Testing the
        // rule without testing the hole is how an exclusion list turns a gate
        // off unnoticed.
        let workspace = std::env::temp_dir().join(format!("nc-hole-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&workspace);
        let scratch = workspace.join("repos/central");
        std::fs::create_dir_all(scratch.join("docs")).expect("writable");
        std::fs::create_dir_all(workspace.join("repos")).expect("writable");
        // The marker is what names a workspace (`NOCTUA.md §3.6`), so a scratch
        // tree that stands in for one carries it — a document beside a `repos/`
        // is a shape any copy can have, so a detector asking only that would
        // disagree with one asking for the marker.
        std::fs::write(workspace.join(crate::sections::MARKER), "").expect("writable");
        for document in ABOVE {
            wrote(&workspace.join(document), "Read `ls policy/core`.\n");
        }
        let claim = "The core holds the six invariants.\n";

        std::fs::write(scratch.join("docs/errors.md"), claim).expect("writable");
        let (findings, _, _, _) = every_written_count_matches_the_tree(&scratch).expect("readable");
        assert!(
            findings.is_empty(),
            "the hole is not where NOT_READ says it is: {findings:#?}"
        );

        std::fs::write(scratch.join("AGENTS.md"), claim).expect("writable");
        let (findings, _, _, _) = every_written_count_matches_the_tree(&scratch).expect("readable");
        assert_eq!(
            findings.len(),
            1,
            "the same sentence outside the hole was not a finding: {findings:#?}"
        );

        let _ = std::fs::remove_dir_all(&workspace);
    }

    #[test]
    fn the_workspace_documents_are_among_them() {
        // **Skipped when there is no workspace above, and it says so.** This
        // test is *about* the workspace's documents, which live above this
        // repository (`§3.5`) — a checkout of this repository alone has none of
        // them, which is what CI has. A test that cannot be performed is not a
        // test that passed, so `check`'s test count carries a clause naming what
        // did not run: "and the ones that need the workspace above this
        // repository did not run".
        if crate::sections::workspace_of(root()).is_none() {
            println!("skipped: there is no workspace above this repository (§3.5)");
            return;
        }
        // A floor of `read > 0` is cleared by a tree that reaches only its own
        // `CLAUDE.md` — and reaching `NOCTUA.md` and `NOCTUA-PLAN.md` is the
        // whole of what this asks.
        let read = documents(root()).expect("readable");
        for document in ABOVE {
            assert!(
                read.iter().any(|path| path.ends_with(document)),
                "{document} was not read, and it is the document an agent reads first"
            );
        }
        for name in READ {
            assert!(
                read.iter()
                    .any(|path| path.file_name().is_some_and(|had| had == name)),
                "no {name} was read"
            );
        }
    }

    #[test]
    fn the_countable_nouns_are_the_ones_written_down() {
        // **The list's membership, not each entry's behaviour.** The test below
        // iterates `COUNTABLE`, so deleting entries just shortens its loop and
        // cannot fail it; this names every row, so a deletion fails here —
        // `integration test files`, the noun for one of the three examples this
        // module's own header cites, among them.
        //
        // The unit tests that call `claim` pass the noun in by hand, so they
        // exercise no membership at all. Written out, a removal is a two-place
        // edit a reader meets in the diff.
        // **The whole row, not the noun.** A noun's directory and glob are what
        // it is a count *of*, and freezing the name alone leaves them free: a
        // row repointed to a glob that answers the same number — `*/tests` for
        // `*/Cargo.toml`, in a tree where every crate has a `tests/` — moves no
        // count, so `ANSWERED_TODAY` cannot see it and neither could a frozen
        // name.
        // Three of the seven directories hold the same number of two different
        // things, which is why *matches something* and *answers this number*
        // are both too weak on their own.
        let mut rows: Vec<(&str, &str, &str)> = COUNTABLE.to_vec();
        rows.sort_unstable();
        assert_eq!(
            rows,
            [
                ("compile-fail fixtures", "crates/nc-log/fixtures", "*.rs"),
                ("crates", "crates", "*/Cargo.toml"),
                ("integration test files", "crates/nc-journal/tests", "*.rs"),
                ("invariants", "policy/core", "*.cedar"),
                ("migrations", "migrations", "*.sql"),
                ("packs", "policy/packs", "*/README.md"),
                ("registries", "catalog", "*.toml"),
            ],
            "COUNTABLE changed; if a noun was added, removed or repointed, say so here too"
        );
    }

    #[test]
    fn every_countable_noun_is_a_rule_and_not_a_decoration() {
        // `determinism.rs` guards its needle list this way, and so does this:
        // a row removed from `COUNTABLE` — `integration test files`, say — is a
        // loop one shorter here and a failure in the test above.
        for (noun, counted_in, glob) in COUNTABLE {
            assert_eq!(
                claim(&format!("there are six {noun} here"), noun),
                vec![6],
                "`{noun}` is in COUNTABLE and matches nothing, so it is a decoration"
            );
            assert!(
                !counted_in.is_empty() && glob.starts_with('*'),
                "`{noun}` names no command that answers it: `ls {counted_in}/{glob}`"
            );
            // **And the command it names answers something.** Neither field
            // reaches anything but the `fix` string, and the guard above only
            // asks that they are non-empty and glob-shaped — measured, the
            // directory replaced by a path that does not exist and
            // `check passed`. Every finding this gate prints would then send a
            // reader to `ls` a path that does not exist, and a noun whose
            // directory was renamed would be invisible.
            let directory = root().join(counted_in);
            assert!(
                directory.is_dir(),
                "`{noun}` is counted in `{counted_in}`, which is not a directory of this \
                 repository — every finding about `{noun}` names a path a reader cannot \
                 list"
            );
            // **The glob, not the text after its last dot.** Taking the suffix
            // and looking for it *anywhere beneath* the directory passes
            // `("invariants", "policy", "*.cedar")` and
            // `("crates", "crates", "*/Cargo.toml")` while
            // `ls policy/*.cedar` and `ls crates/*.rs` match nothing — every
            // finding about those nouns would hand a reader a command answering
            // zero, which is the defect the message below claims to prevent.
            // The glob is evaluated segment by segment at the depth it names.
            let matched = crate::gates::glob_count(&directory, glob);
            assert!(
                matched > 0,
                "`ls {counted_in}/{glob}` matches nothing, so the command this gate hands \
                 a reader answers zero and the noun is counted against nothing"
            );
            // **And the number it answers, written down.** *Matches something*
            // is satisfied by any directory with a file in it, so a noun could
            // be pointed at an unrelated one and the rule would still pass —
            // measured, two of the seven repointed and both pass that. A noun
            // read as *integration tests* with a glob that counts **files** is
            // the case: the journal's integration tests number over a hundred in
            // four files, so a document claiming either number is judged against
            // the other.
            // Each noun counts what it says it counts, and the answers are
            // pinned so a repoint moves one.
            let written = ANSWERED_TODAY
                .iter()
                .find(|(named, _)| *named == noun)
                .map(|(_, answer)| *answer);
            assert_eq!(
                Some(matched),
                written,
                "`ls {counted_in}/{glob}` answers {matched} for `{noun}`, and \
                 `ANSWERED_TODAY` says otherwise. A noun repointed at another directory \
                 keeps matching something; what it cannot keep is its number"
            );
        }
    }

    /// What each countable noun's own command answers today.
    ///
    /// A ratchet rather than a rule: the numbers move when the tree does, and
    /// moving one is the visible act. What it stops is a noun quietly pointed
    /// at a directory that has nothing to do with it, which *matches something*
    /// cannot see.
    const ANSWERED_TODAY: [(&str, usize); 7] = [
        ("compile-fail fixtures", 12),
        ("invariants", 7),
        ("integration test files", 4),
        ("migrations", 12),
        ("registries", 8),
        ("crates", 17),
        ("packs", 3),
    ];

    #[test]
    fn a_count_split_by_a_hard_wrap_is_still_a_count() {
        // **Paragraphs, not lines, held by a test.** Replacing `paragraphs()`
        // with a line splitter fails this test. These documents
        // are wrapped at about eighty columns, which makes a count and its noun
        // landing on two lines the ordinary case.
        let workspace = std::env::temp_dir().join(format!("nc-wrap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&workspace);
        let scratch = workspace.join("repos/central");
        std::fs::create_dir_all(&scratch).expect("writable");
        std::fs::create_dir_all(workspace.join("repos")).expect("writable");
        // The marker is what names a workspace (`NOCTUA.md §3.6`), so a scratch
        // tree that stands in for one carries it — a document beside a `repos/`
        // is a shape any copy can have, so a detector asking only that would
        // disagree with one asking for the marker.
        std::fs::write(workspace.join(crate::sections::MARKER), "").expect("writable");
        for document in ABOVE {
            wrote(&workspace.join(document), "Read `ls policy/core`.\n");
        }
        std::fs::write(
            scratch.join("AGENTS.md"),
            "The core holds six\ninvariants that the gate reads.\n",
        )
        .expect("writable");

        let (findings, _, _, _) = every_written_count_matches_the_tree(&scratch).expect("readable");
        assert_eq!(
            findings.len(),
            1,
            "a count split across a hard wrap was not read: {findings:#?}"
        );
        assert_eq!(
            findings[0].line, 1,
            "the paragraph's first line is reported"
        );
        let _ = std::fs::remove_dir_all(&workspace);
    }

    #[test]
    fn words_and_digits_are_both_numbers() {
        // **Past twelve** — thirteen is the value this module's own header
        // cites. Deleting `thirteen` through `fifty` fails these rows, where a
        // test stopping at twelve would stop at exactly the boundary that
        // matters.
        for (word, value) in [
            ("one", 1),
            ("nine", 9),
            ("twelve", 12),
            ("thirteen", 13),
            ("nineteen", 19),
            ("twenty", 20),
            ("fifty", 50),
            ("41", 41),
        ] {
            assert_eq!(written(word), Some(value), "{word}");
        }
        assert_eq!(written("many"), None);
        assert_eq!(written(""), None);
    }

    /// One marker excuses one claim, not a paragraph of them.
    ///
    /// **Tested against the paragraph alone, a marker would excuse every claim in
    /// it, of every noun.** The marker is tested against the joined paragraph — which
    /// is right for *finding* a
    /// claim, because a count and its noun land on different lines all the time,
    /// and wrong for excusing one. Measured: a three-line paragraph with the
    /// marker on line 1 and a second, different claim on line 3, and one
    /// `<!-- allow-count: a probe -->` silences both without the noun rule.
    #[test]
    fn an_excuse_names_the_claim_it_excuses() {
        // The same scratch workspace the test below builds: this gate reads
        // `NOCTUA.md` and `NOCTUA-PLAN.md` and treats their absence as an error.
        let workspace = std::env::temp_dir().join(format!("nc-excuse-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&workspace);
        let tree = workspace.join("repos/central");
        std::fs::create_dir_all(&tree).expect("writable");
        std::fs::write(workspace.join(crate::sections::MARKER), "").expect("writable");
        // **`ABOVE`, not a copy of it.** Written out, this list stops being the
        // gate's own and the two drift: a document added to `ABOVE` and not
        // here makes the count below say the walk has read one document too few.
        for document in ABOVE {
            wrote(&workspace.join(document), "Read `ls policy/core`.\n");
        }
        let tree = tree.as_path();
        std::fs::create_dir_all(tree.join("policy/core")).expect("writable");
        for name in ["a.cedar", "b.cedar", "c.cedar"] {
            std::fs::write(tree.join("policy/core").join(name), "// x\n").expect("writable");
        }
        std::fs::create_dir_all(tree.join("migrations")).expect("writable");
        for name in ["0001.sql", "0002.sql", "0003.sql", "0004.sql"] {
            std::fs::write(tree.join("migrations").join(name), "-- x\n").expect("writable");
        }
        std::fs::write(
            tree.join("AGENTS.md"),
            "The core holds the three invariants \
             <!-- allow-count: the invariants §14.9 fixes -->\n\
             and the tree carries four migrations that follow them.\n",
        )
        .expect("writable");
        let (findings, read, _, _) = every_written_count_matches_the_tree(tree).expect("readable");
        eprintln!(
            "READ {read} FINDINGS {findings:#?} TEXT {:?}",
            std::fs::read_to_string(tree.join("AGENTS.md"))
        );
        assert_eq!(
            findings.len(),
            1,
            "one marker about invariants excused a claim about migrations: {findings:#?}"
        );
        assert!(
            findings[0].what.contains("migration"),
            "the wrong claim survived: {findings:#?}"
        );
        // The floor: a rule that ignored the marker would report both, and this
        // test would pass for the wrong reason.
        std::fs::write(
            tree.join("AGENTS.md"),
            "The core holds the three invariants \
             <!-- allow-count: the invariants §14.9 fixes -->\n",
        )
        .expect("writable");
        let (findings, _, _, _) = every_written_count_matches_the_tree(tree).expect("readable");
        assert!(
            findings.is_empty(),
            "a marker that names its noun did not excuse it: {findings:#?}"
        );
        let _ = std::fs::remove_dir_all(&workspace);
    }

    #[test]
    fn a_stated_count_is_a_finding_whatever_the_number_is() {
        // Fired on purpose, because a gate nobody has watched fire is a gate
        // nobody knows is wired up.
        // **A workspace above it**, because this gate reads `NOCTUA.md` and
        // `NOCTUA-PLAN.md` and treats their absence as an error — so a scratch
        // tree floating in `/tmp` would prove the wrong thing.
        let workspace = std::env::temp_dir().join(format!("nc-counts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&workspace);
        let scratch = workspace.join("repos/central");
        std::fs::create_dir_all(&scratch).expect("writable");
        // The marker is what names a workspace (`NOCTUA.md §3.6`).
        std::fs::write(workspace.join(crate::sections::MARKER), "").expect("writable");
        // **`ABOVE`, not a copy of it.** Written out, this list stops being the
        // gate's own and the two drift: a document added to `ABOVE` and not
        // here makes the count below say the walk has read one document too few.
        for document in ABOVE {
            wrote(&workspace.join(document), "Read `ls policy/core`.\n");
        }
        std::fs::create_dir_all(scratch.join("policy/core")).expect("writable");
        for name in ["a.cedar", "b.cedar", "c.cedar"] {
            std::fs::write(scratch.join("policy/core").join(name), "// x\n").expect("writable");
        }
        std::fs::write(
            scratch.join("AGENTS.md"),
            "The core holds the five invariants, and tests/ beside them.\n",
        )
        .expect("writable");

        let (findings, read, _, _) =
            every_written_count_matches_the_tree(&scratch).expect("readable");
        assert_eq!(
            read,
            1 + ABOVE.len(),
            "the repository's document and the workspace's were read"
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.what.contains("5 invariant")),
            "a stated count was not a finding: {findings:#?}"
        );
        // **A correct count is a finding too**, which is the point: it is right
        // today and rots tomorrow, and nothing would say when.
        std::fs::write(
            scratch.join("AGENTS.md"),
            "The core holds the three invariants, and tests/ beside them.\n",
        )
        .expect("writable");
        let (findings, _, _, _) = every_written_count_matches_the_tree(&scratch).expect("readable");
        assert_eq!(
            findings.len(),
            1,
            "a correct count is still a count: {findings:#?}; the file holds {:?}",
            std::fs::read_to_string(scratch.join("AGENTS.md"))
        );

        // **The hatch, and the hole in it.** A marked line passes; a bare marker
        // with no reason does not, because `references::names` demands a reason
        // that names the noun — which is why this gate borrows it rather than
        // inventing its own.
        std::fs::write(
            scratch.join("AGENTS.md"),
            "The core holds the three invariants. <!-- allow-count: the invariants §14.9 fixes -->\n",
        )
        .expect("writable");
        let (findings, _, _, _) = every_written_count_matches_the_tree(&scratch).expect("readable");
        assert!(
            findings.is_empty(),
            "a marked count was reported: {findings:#?}"
        );

        std::fs::write(
            scratch.join("AGENTS.md"),
            "The core holds the three invariants. <!-- allow-count: -->\n",
        )
        .expect("writable");
        let (findings, _, _, _) = every_written_count_matches_the_tree(&scratch).expect("readable");
        assert_eq!(
            findings.len(),
            1,
            "a bare marker with no reason excused a count: {findings:#?}"
        );

        // And prose that names no number produces nothing.
        std::fs::write(
            scratch.join("AGENTS.md"),
            "The core holds the invariants, and tests/ beside them. See `ls policy/core`.\n",
        )
        .expect("writable");
        let (findings, _, _, _) = every_written_count_matches_the_tree(&scratch).expect("readable");
        assert!(findings.is_empty(), "{findings:#?}");

        let _ = std::fs::remove_dir_all(&workspace);
    }

    /// A count in digits is a count, and a number that names something is not.
    ///
    /// **Without `COUNTS_THEM`, every digit form of every countable noun falls
    /// through**, on the label rule's grounds that "nothing in these documents
    /// does" it — which is a measurement of today's prose rather than a
    /// property of the rule, and the documents this reads are the ones an agent
    /// is told to trust before running anything. `AGENTS.md` states this very
    /// rule and gives *"there are 41 tests"* as its example of the forbidden
    /// sentence; without it, that sentence is invisible to the gate enforcing
    /// it.
    ///
    /// A case per entry of `COUNTS_THEM`, because a whitelist with no case per
    /// member is a shape this repository refuses — and the same
    /// sentences with their labels back, because a rule that caught those would
    /// be worse than no rule.
    #[test]
    fn a_count_in_digits_is_a_count_and_a_label_is_not() {
        // **Written out, not iterated from the list.** Looping over
        // `COUNTS_THEM` builds the cases from the thing under test, so dropping
        // an entry drops its own case — measured: with a loop, removing `"both"`
        // leaves this test green. That is a shape this repository refuses
        // wherever it finds it.
        const COUNTING: [&str; 31] = [
            "are",
            "is",
            "was",
            "were",
            "be",
            "been",
            "has",
            "have",
            "had",
            "holds",
            "hold",
            "carries",
            "carry",
            "contains",
            "contain",
            "ships",
            "declares",
            "lists",
            "names",
            "adds",
            "all",
            "only",
            "just",
            "both",
            "defines",
            "define",
            "provides",
            "includes",
            "offers",
            "comprises",
            "supports",
        ];
        assert_eq!(
            COUNTING.len(),
            super::COUNTS_THEM.len(),
            "a word was added to or removed from `COUNTS_THEM` and this list did not \
             follow, so the case for it does not exist"
        );
        let mut missed = Vec::new();
        for word in COUNTING {
            assert!(
                super::COUNTS_THEM.contains(&word),
                "`{word}` is written here and is not in `COUNTS_THEM`"
            );
            let line = format!("This tree {word} 9 crates today.");
            if super::claim(&line, "crates").is_empty() {
                missed.push(line);
            }
        }
        assert!(
            missed.is_empty(),
            "a counting word in the list reads as a label: {missed:#?}"
        );

        // And six of the nouns, each in the phrasing a person actually writes.
        for (line, noun) in [
            ("There are 9 crates in this repository.", "crates"),
            ("The tree holds 12 migrations.", "migrations"),
            ("This repository ships 6 registries.", "registries"),
            ("There are 7 invariants.", "invariants"),
            (
                "The log has 11 compile-fail fixtures",
                "compile-fail fixtures",
            ),
            (
                "The journal has 13 integration test files.",
                "integration test files",
            ),
            // The shape of `AGENTS.md`'s own example of the sentence this gate
            // forbids — a digit, mid-clause — with the noun this gate knows.
            // Written unquoted, because a quoted sentence is an example rather
            // than a claim and this gate is right to exempt one; the case below
            // holds that half.
            (
                "There are 41 integration test files.",
                "integration test files",
            ),
        ] {
            assert!(
                !super::claim(line, noun).is_empty(),
                "`{line}` states a count of {noun} and the gate did not see it"
            );
        }

        // The other direction, which is the one that must not move: a number
        // that identifies keeps its label, and no label is a counting word.
        for (line, noun) in [
            ("Stage 10 migrations were written for it.", "migrations"),
            ("Tested on Ubuntu 24 with PostgreSQL 18.", "migrations"),
            ("See page 12 invariants for the rest.", "invariants"),
            ("Issue 5 migrations are tracked there.", "migrations"),
            ("Round 3 crates were reviewed.", "crates"),
            ("Python 3 registries are not a thing.", "registries"),
            // And a count *quoted as an example* is not a claim about this
            // tree. `AGENTS.md` writes the forbidden sentence in quotes in
            // order to forbid it, and a gate reporting that would be reporting
            // the rule for stating the rule.
            (
                "Run the gate, never \"there are 41 integration tests\".",
                "integration test files",
            ),
        ] {
            assert!(
                super::claim(line, noun).is_empty(),
                "`{line}` names something rather than counting it, and the gate \
                 reported it — a gate wrong in this direction is worse than no gate"
            );
        }
    }

    /// The independent paragraph counter counts paragraphs.
    ///
    /// **One half of an equality the whole rotting-count gate rests on.** With
    /// `blocks_in(&text) * 0` — one token — the other side goes to zero, and
    /// under a comparison accepting anything at or above what it expects the
    /// rule would collapse to `judged > 0`. `check.rs` calls that "a floor with
    /// everything in it" in as many words.
    ///
    /// The cases are the ones the walk it is compared against has to agree
    /// about: a fence is not prose, a run of blank lines is one break, and a
    /// document that is nothing but a fenced block has no paragraphs at all —
    /// which is the hole `unbalanced_fence` cannot see, because it only reports
    /// an *odd* number of markers.
    #[test]
    fn the_second_paragraph_counter_counts_paragraphs() {
        for (text, expected, why) in [
            ("", 0, "an empty document"),
            ("one\n", 1, "one line"),
            ("one\ntwo\n", 1, "a wrapped paragraph is one"),
            ("one\n\ntwo\n", 2, "a blank line separates two"),
            ("one\n\n\n\ntwo\n", 2, "a run of blank lines is one break"),
            ("```\none\n```\n", 0, "a fenced block is not prose"),
            (
                "a\n\n```\none\n\ntwo\n```\n\nb\n",
                2,
                "prose either side of a fence",
            ),
            ("```text\none\n```\n", 0, "an info string opens a fence too"),
            ("  ```\none\n  ```\n", 0, "an indented fence is a fence"),
            ("one\n   \ntwo\n", 2, "a line of spaces is blank"),
        ] {
            assert_eq!(
                super::blocks_in(text),
                expected,
                "{why}: `{text:?}` counted {} paragraph(s)",
                super::blocks_in(text)
            );
        }

        // And it agrees with the walk it is compared against, over the real
        // documents — which is the comparison the gate makes, asserted here so
        // the two cannot drift while `check` happens not to be run.
        //
        // **Against the documents this run can actually see, not a constant.**
        // `how_many_paragraphs` reads the documents in this repository *and*
        // those in the workspace above it, so the number for a developer with
        // the workspace checked out is several times the number for a clone of
        // this repository alone — which is what the checkout step gives CI, and
        // what `README.md` promises `check` passes on. A floor of `> 500`
        // calibrated in the first state makes `cargo xtask check` fail in the
        // second, on the only machine that matters:
        //
        // ```text
        //   ok    7 document(s) state no count that can rot, in 365 paragraph(s)
        //   error tests … the second counter read 365 paragraph(s)
        // ```
        //
        // The floor is the documents in front of it, counted a third way: at
        // least one block per document opened, which is true of every document
        // in this tree and false of a counter that stopped.
        let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        let counted = super::how_many_paragraphs(root);
        let documents = super::documents(root).expect("the documents walk").len()
            + if crate::sections::workspace_of(root).is_some() {
                super::ABOVE.len()
            } else {
                0
            };
        assert!(
            documents > 5,
            "{documents} document(s) were found, so this is not the tree this gate reads"
        );
        assert!(
            counted > documents * 10,
            "the second counter read {counted} paragraph(s) across {documents} \
             document(s), which is fewer than ten each — these are long documents, so \
             either it stopped early or it is being asked about the wrong tree"
        );
    }
}
