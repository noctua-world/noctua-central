//! Every path a document or a comment cites must exist.
//!
//! `NOCTUA.md §0.5` splits document maintenance into two halves and says which
//! one can be measured: *"a path must resolve, and a name must be something the
//! code declares"*. The other half — whether a sentence is still **true** — no
//! gate can ask, which is exactly why the half that can be checked should be.
//!
//! This one earned its place twice on the day it was written. The first version
//! of this repository's `AGENTS.md` and `README.md` both described a
//! `rust-toolchain.toml` that a failed `cd` had silently prevented from being
//! written, and both tabulated a generated documentation directory that did not
//! exist. Then a
//! reviewer found that the gate itself could not see the repository map — the
//! one place those claims actually lived — because it only read backticked runs
//! and the map is a fenced code block. Both halves are covered now, and the
//! second is why [`fenced`] exists.
//!
//! **What it deliberately does not check.** A backticked token with a brace, an
//! angle bracket, a star or a space in it is a pattern or a placeholder, not a
//! path — `catalog/i18n/{en,pt-BR}.toml` and `NC-<AREA>-<NNNN>` are both prose.
//! Reporting those would produce a gate people learn to ignore, which is worse
//! than no gate.

use std::collections::BTreeSet;
use std::path::Path;

use crate::gates::{Finding, walk};
use crate::relative_to;

/// This file, for the findings it files against its own counters.
///
/// Written once because it was written twice, and because a call naming it
/// inline no longer fits on a line.
const HERE: &str = "xtask/src/references.rs";

/// The same-line marker that excuses a citation, with a mandatory reason.
///
/// Same line only, and deliberately: rustfmt is free to move a comment that sits
/// above a statement, which would silently separate a marker from what it
/// excuses. `noctua-colors` lost time to exactly that.
pub(crate) const ALLOW: &str = "allow-reference:";

/// What opens a comment in the file being read.
///
/// **`#` is a comment in a manifest and not in Rust.** `marked` treated it as
/// one everywhere, so `r#"…allow-reference: written as data…"#` — a raw string
/// — excused its line, which is the defect the comment rule was written to
/// close, one character along. Measured through the citation gate: the count
/// went up by one, so the gate *read* the dangling path and then excused it.
/// `#[doc = "…"]`, `r#type`, a Markdown `## heading` and a `#42` issue
/// reference all did the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Comments {
    /// `<!--` and nothing else. What a Markdown document has.
    ///
    /// **`//` is not a comment opener in prose, and treating it as one turned a
    /// gate off.** `counts.rs` reads Markdown and asked with `Slashes`, so any
    /// `https://` earlier in a paragraph satisfied "the marker is in a comment"
    /// — no `<!-- -->` needed at all. Measured, in a document stating a count
    /// that was wrong:
    ///
    /// ```text
    /// … described at https://example.invalid/core …  ->  no finding
    /// … described at example.invalid/core …          ->  the finding
    /// ```
    ///
    /// One URL anywhere above the marker was the whole difference.
    Markdown,
    /// `//`, `/*` and `<!--`. What Rust and Markdown have.
    Slashes,
    /// Those, and `#` — a manifest, a workflow, a shell script, a justfile.
    AndHash,
    /// Those, and `--`. SQL, whose migrations this tree reads.
    AndDashes,
}

impl Comments {
    /// What a file of this name uses.
    pub(crate) fn of(file: &str) -> Self {
        match std::path::Path::new(file)
            .extension()
            .and_then(|extension| extension.to_str())
        {
            Some("rs") => Self::Slashes,
            // **Markdown has `<!-- -->` and not `//`.** Mapping it to `Slashes`
            // made any `https://` earlier on a line open a comment, so an
            // `allow-reference:` in plain prose silenced the section gate with no
            // `<!-- -->` anywhere: measured, one line in `README.md` citing a
            // section that does not exist passed with the URL and was reported
            // without it. `Comments::Markdown` was added for the counts gate and
            // the three other `.md` consumers were left behind — this is the one
            // place that decides for all of them.
            Some("md") => Self::Markdown,
            Some("sql") => Self::AndDashes,
            // TOML, YAML, shell, and a justfile, which has no extension at all.
            _ => Self::AndHash,
        }
    }
}

/// Whether a line carries `marker` in a comment, **and a reason after it**.
///
/// Shared, because every source gate needs the same shape and each one inventing
/// its own would mean each one forgetting the reason separately — which is how
/// `allow-reference:` came to accept a bare marker for months.
pub(crate) fn marked_in(line: &str, marker: &str, comments: Comments) -> bool {
    // **Every occurrence, not the first.** `split_once` took the first one, and
    // a line can hold the marker twice: as *data* inside a string literal, and
    // again as a real comment excusing the line. This gate's own fixtures are
    // exactly that shape —
    //
    // ```text
    // "Box::leak(x) // allow-logging:\n", // allow-logging: this gate's own fixture
    // ```
    //
    // — and the first occurrence, being inside a string, made the answer `false`
    // before the second was ever looked at. The line was excused by neither
    // marker. Reading them all cannot weaken the rule: a marker inside a string
    // is still not in a comment, and it is still refused.
    how_many_marked_in(line, marker, comments) > 0
}

/// How many hatches of `marker` the line carries.
///
/// **A census of hatches counts hatches, and this counted lines.** The loop in
/// `check.rs` added one per line per marker, so two `allow-reference:` comments
/// written on one line — which is ordinary, and which this file's own fixtures
/// do — moved the tree's hatch count by one instead of two. The number beside
/// each marker in `EXCUSED` exists so that adding a hatch is a two-place edit;
/// a count that saturates at one per line lets the second one in free.
///
/// What it still does not measure is a hatch's *reach*: one marker whose reason
/// names two citations excuses both, and is one hatch here. That is the right
/// count for "how many decisions a reviewer took" and the wrong one for "how
/// many subjects went unchecked", and only the first is what `EXCUSED` claims.
pub(crate) fn how_many_marked_in(line: &str, marker: &str, comments: Comments) -> usize {
    line.match_indices(marker)
        .filter(|(at, _)| in_a_comment(line, *at, marker.len(), comments))
        .count()
}

/// Whether the marker at `at` sits in a comment and carries a reason.
fn in_a_comment(line: &str, at: usize, width: usize, comments: Comments) -> bool {
    let before = &line[..at];
    let reason = &line[at + width..];
    // **In a comment, not anywhere on the line.** This read the raw line, so a
    // marker inside a *string literal* excused it — and a gate whose subject is
    // source code is full of lines that hold strings. Measured against the
    // logging gate, with a real leak at the call site:
    //
    // ```text
    // Box::leak(x); /* allow-logging: reason */              -> excused
    // let s = "allow-logging: nothing to see"; Box::leak(x); -> excused
    // ```
    //
    // The second is the one that matters: writing the marker *as data* turned
    // the gate off for the line. `wall.rs` writes its own needles with
    // `concat!` for exactly this reason — "this gate reads its own source and a
    // literal in a code position is a finding" — and the marker it reads was
    // not held to it.
    //
    // **Character literals too.** The scanner tracked `"` and not `'`, so a
    // line holding `'"'` opened a string that never closed and its trailing
    // `//` was never seen — a real comment, refused, on a line the gate's own
    // fix text tells the reader to mark.
    // **And raw strings, which the paragraph above named and did not handle.**
    // The scanner opened a string at the first `"` of `r#"a "b // marker"#` and
    // closed it at the second, so the `//` fell outside a string and the marker
    // — written as data, in exactly the shape this comment says was closed —
    // excused its line again. Measured against the mint scan, with a real
    // `from_crockford` on the line and `check passed`.
    //
    // A raw string ends at `"` followed by its own number of `#`, and nothing
    // inside it escapes anything.
    let mut in_a_string = false;
    let mut in_a_char = false;
    let mut escaped = false;
    let mut opened = false;
    let mut raw: Option<usize> = None;
    // **A block comment closes.** This tracked openers and nothing else: `/*`
    // and `<!--` set `opened` and broke out of the scan, so *any* comment
    // earlier on the line made a marker after it count as commented. Measured
    // on this repository's own documents, one hatch rewritten in place so that
    // an HTML comment closed between the prose and the marker — the marker then
    // sitting in rendered prose — with `189 line(s) carry a gate's escape
    // hatch`, its per-marker count and `2038 citation(s) resolve` all
    // byte-identical, and `431 passed`. `AGENTS.md`'s own invariant is *the
    // marker has to be in a comment*, and the same two characters defeat the
    // count gate's hatch too.
    //
    // The marker is not spelled out in this paragraph, because a marker written
    // in a comment *is* a hatch and the census counts it — which is the rule
    // this function is the reader for.
    //
    // The Rust half was worse and is the same code: breaking at `/*` meant the
    // scan never reached a marker written inside a *string literal* after one,
    // which is the defect two paragraphs above record as closed.
    let mut block: Option<&[char]> = None;
    let characters: Vec<char> = before.chars().collect();
    let mut skip_to = 0_usize;
    for (at, character) in characters.iter().enumerate() {
        if at < skip_to {
            continue;
        }
        if let Some(closes) = block {
            if characters[at..].starts_with(closes) {
                block = None;
                skip_to = at + closes.len();
            }
            continue;
        }
        if let Some(hashes) = raw {
            let closes: Vec<char> = std::iter::once('"')
                .chain(std::iter::repeat_n('#', hashes))
                .collect();
            if characters[at..].starts_with(closes.as_slice()) {
                raw = None;
                skip_to = at + closes.len();
            }
            continue;
        }
        // **The one reader, not a fifth copy of it.** This knew `r` and `br`
        // and not `cr`, which is the same gap `wall.rs` was measured walking
        // through in round 55: a C raw string takes the scan out of a string at
        // the bare quotes, and the `//` in the body then opens a comment.
        // `gates::raw_string_at` carries all three prefixes and the measurement
        // that put the third one there.
        if let Some((opener, hashes)) = crate::gates::raw_string_at(&characters, at) {
            raw = Some(hashes);
            skip_to = at + opener;
            continue;
        }
        if in_a_string || in_a_char {
            match character {
                '\\' if !escaped => escaped = true,
                '"' if !escaped && in_a_string => in_a_string = false,
                '\'' if !escaped && in_a_char => in_a_char = false,
                _ => escaped = false,
            }
            continue;
        }
        match character {
            '"' => {
                // A raw string's `#` characters sit between `r` and the quote,
                // and are not a comment opener — which is how `r#"…"#` excused
                // its line.
                in_a_string = true;
                escaped = false;
            }
            '\'' if closes_a_char(&characters[at..]) => {
                in_a_char = true;
                escaped = false;
            }
            // **Only where `#` is a comment.** It is one in a manifest, a
            // workflow, a shell script and a justfile, and it is not one in
            // Rust or Markdown — where `r#"…"#` is a raw string, `#[derive]`
            // an attribute, `r#type` an identifier, `## x` a heading and `#42`
            // an issue reference. Every one of those excused its line while
            // this was unconditional, which is the defect the comment rule was
            // written to close, one character along.
            //
            // No test on what follows it: TOML ends a line at `#` wherever it
            // sits, so `key=1#c` is a comment, and a rule that asked for a
            // space would be wrong about the file type it is for.
            '#' if comments == Comments::AndHash => opened = true,
            '-' if comments == Comments::AndDashes && characters.get(at + 1) == Some(&'-') => {
                opened = true;
            }
            '/' if comments != Comments::Markdown && characters.get(at + 1) == Some(&'/') => {
                opened = true;
            }
            // A block runs to its closer and the scan goes on past it, which is
            // what tells `/* note */ marker` from `/* marker */`.
            '/' if comments != Comments::Markdown && characters.get(at + 1) == Some(&'*') => {
                block = Some(&['*', '/']);
                skip_to = at + 2;
            }
            '<' if characters[at..].starts_with(&['<', '!', '-', '-']) => {
                block = Some(&['-', '-', '>']);
                skip_to = at + 4;
            }
            _ => {}
        }
        if opened {
            break;
        }
    }
    // A block still open where the marker sits is a marker inside it.
    if !opened && block.is_none() {
        return false;
    }
    let reason = reason
        .trim()
        .trim_end_matches("-->")
        .trim_end_matches("*/")
        .trim();
    !reason.is_empty()
}

/// Whether an apostrophe opens a character literal rather than a lifetime.
///
/// `'x'` and `'\''` are literals; `'static` and `'a` are lifetimes, and reading
/// one as a literal would swallow the rest of the line.
fn closes_a_char(from: &[char]) -> bool {
    let mut at = 1;
    if from.get(at) == Some(&'\\') {
        at += 1;
        while at < from.len() && at < 12 {
            if from[at] == '\'' {
                return true;
            }
            at += 1;
        }
        return false;
    }
    from.get(at + 1) == Some(&'\'')
}

/// The text after a marker on one line, ending where its comment does.
///
/// **`rsplit` alone is not this function, and six call sites used `rsplit`
/// alone.** A marker written in a *block* comment placed before the code —
/// `let x = /* allow-logging: … */ Snowflake::from_crockford(text);` — leaves
/// the excused code itself inside what `rsplit` hands back, so `names(reason,
/// needle)` matches because the *offending line* contains the needle. Measured:
/// that spelling passed while the identical words written after the code failed,
/// and `cargo fmt` preserves the block form. Position on the line was the entire
/// difference. Splitting at `*/` and at `-->` is what makes the reason the
/// comment's and not the line's, and every hatch in the tree reads through here
/// now.
pub(crate) fn reason_after<'a>(line: &'a str, marker: &str, comments: Comments) -> Option<&'a str> {
    // **The last occurrence *in a comment*, not the last anywhere.**
    // `marked_in` asks whether any occurrence is in a comment and this took the
    // text after the last one on the line, so a marker spelled in ordinary prose
    // — after a real one — supplied the reason. Measured on 2026-09-08 in a
    // paragraph of `policy/core/README.md` that already carried a real
    // count hatch, with a sentence appended naming that marker in prose and two
    // different nouns: `check passed`, `ok 14 document(s) state no count that
    // can rot`, and the hatch census unmoved — every number byte-identical,
    // because the prose spelling is correctly *not* counted as a hatch. So the
    // one instrument built to make hatches visible reported nothing while a gate
    // was switched off.
    //
    // The honest direction bites too: two properly written markers in one
    // paragraph, each naming its own noun, and only the last one worked.
    //
    // This is round twenty-nine's logging-hatch finding from the other side.
    // There a prose spelling *inflated* a census; here it steers an excuse and
    // leaves the census still — and writing this paragraph inflated the census
    // twice over before the marker names came out of it.
    let at = *line
        .match_indices(marker)
        .filter(|(at, _)| in_a_comment(line, *at, marker.len(), comments))
        .map(|(at, _)| at)
        .collect::<Vec<usize>>()
        .last()?;
    let after = &line[at + marker.len()..];
    // **No `unwrap_or` here: `split` always yields at least one piece.** Both
    // arms were unreachable, and the second was wrong if it had ever been
    // taken — it hands back the *uncut* text, undoing the cut above it. Two of
    // this file's mutation operators quoted spans that included them, so the
    // runner reported a kill for text that was part clause and part nothing.
    // Reported in round sixty-six.
    let cut = after.split("-->").next().unwrap_or_default();
    Some(cut.split("*/").next().unwrap_or_default())
}

/// Whether an excuse's reason names `subject` as a word rather than as a
/// substring of a longer one.
///
/// **Every naming rule in this tree was `reason.contains(subject)`**, and a
/// substring test on free text is not a naming rule. Measured, all three green:
///
/// ```text
/// allow-count: democrates migrations      excused a claim about `crates`
/// allow-nondeterminism: chronology …      excused `chrono`
/// allow-nondeterminism: … Utc::nowhere    excused `Utc::now`
/// nc:bounded: login_attempts is bounded…  excused the table `login`
/// ```
///
/// The last one is the expensive shape: `login_attempts` is not in the file, is
/// not in the catalog and does not exist, and `"login_attempts".contains("login")`
/// was the whole of the check. A boundary is a character that cannot be part of
/// an identifier, which is what makes `login` in `login_attempts` not a mention
/// of `login`.
pub(crate) fn names(reason: &str, subject: &str) -> bool {
    if subject.is_empty() {
        return false;
    }
    let boundary =
        |character: Option<char>| character.is_none_or(|c| !c.is_alphanumeric() && c != '_');
    // **The whole reason at every occurrence, because re-slicing loses the left
    // context.** This walked the reason by cutting off everything up to and
    // including each occurrence it rejected, and then asked the *cut* string
    // what stood before the next one — so the second occurrence onwards always
    // began at index 0 and `before` was always `None`, which is a boundary. Two
    // occurrences running, `<anything><subject><subject>`, and the first
    // rejection manufactured a boundary for the second: `democratescrates
    // migrations` names `crates`, measured through this function in round
    // sixty-five. The same rule in `crates/nc-catalog-compiler/src/lib.rs` was
    // written over `match_indices` from the start and never had this, which is
    // the argument for writing it that way here rather than for repairing the
    // walk: there is no cut string left to ask the wrong question of.
    //
    // `match_indices` yields the same non-overlapping occurrences the cut walk
    // did, indexed into the string the boundaries are actually read from.
    //
    // A subject that itself ends in punctuation — `"/dev/`, `literal!` —
    // brings its own right-hand boundary, and that does not vary per
    // occurrence, so it is asked once.
    let ends_itself = subject
        .chars()
        .next_back()
        .is_some_and(|c| !c.is_alphanumeric() && c != '_');
    reason.match_indices(subject).any(|(at, _)| {
        let before = reason[..at].chars().next_back();
        let after = reason[at + subject.len()..].chars().next();
        // **A path is not a mention.** `crates` is a noun the count gate reads
        // *and* a directory, so `allow-count: the layout is fixed by
        // crates/nc-log/fixtures` named the noun without being about the count —
        // measured, a claim of "four crates" against eight, excused. `claim`
        // already refuses a path on the claim side, for the same reason and
        // after the same measurement; this is the excuse side of it.
        // **Followed by a slash, not preceded by one.** Refusing both made a
        // reason that *cites where the code lives* fail — `allow-logging: the
        // reader is nc-ids/from_crockford and only reads a stored id` names
        // `from_crockford` about as plainly as a sentence can, and was refused.
        // A name with a slash *after* it is a directory (`crates/nc-log/…`); a
        // name with one before it is the last segment of a path, which is a way
        // of naming the thing.
        if after == Some('/') {
            return false;
        }
        boundary(before) && (ends_itself || boundary(after))
    })
}

/// Whether a line carries the marker **and a reason**.
///
/// The reason is what the doc comment on [`ALLOW`] has always promised and what
/// the gate's own fix text tells a reader to write. It was never checked: a bare
/// `allow-reference:` excused every citation on the line, so the escape hatch
/// cost nothing to use and left nothing behind for the next reader. An exemption
/// with no cost is how a gate stops applying to the code it exists to guard.
pub(crate) fn excused(line: &str, comments: Comments) -> bool {
    // Trailing comment syntax is not a reason. `-->` closes an HTML comment and
    // `*/` a block comment, and a line ending in either after a bare marker is
    // the empty case wearing punctuation. `marked_in` handles both.
    marked_in(line, ALLOW, comments)
}

/// The sentence that marks a file as generated.
///
/// Matched **in the first few lines**, and in full. An earlier version tested
/// `text.contains("do not edit.")` anywhere in the file, and a reviewer proved
/// what that costs: this very module contains that phrase in its own prose, so
/// the gate exempted itself, and a dangling citation planted here was reported
/// as clean. A whole-file substring is not an exemption rule, it is a phrase
/// nobody may write.
/// Imported rather than repeated: the emitter writes it, so the emitter owns it.
/// A second copy is a constant with two owners in a repository whose whole thesis
/// is that this is how a rule becomes a bug on a delay fuse (`§10.8`).
use nc_catalog_compiler::GENERATED;

/// How far into a file the generated header may be.
///
/// It is the first line of every file the compiler writes. A handful of lines of
/// slack costs nothing and survives a future emitter that puts a shebang or an
/// attribute above it.
const HEADER_LINES: usize = 6;

/// The generated marker is inside the header, and the header is short.
///
/// **A slack of six can be widened without anything noticing.** Measured, 6 →
/// 20: `cargo xtask check` stayed green, and the window that decides whether a
/// file is generated then reaches twenty lines into it — far enough to find the
/// sentence quoted in an ordinary module's prose and exempt a hand-written file
/// from the citation gate. The constant is the *whole* rule: there is no other
/// bound on where the marker may sit.
#[cfg(test)]
const HEADER_IS_SHORT: usize = 8;

/// Files whose citations are read.
///
/// **An exclusion list is a claim and needs its own test** — `gates::SKIP` has
/// two, and this had none. Dropping `"md"` took the gate from 93 citations to
/// 57 with `check passed`: 39% of its work, and precisely the half the module
/// header says it exists for, since a document that describes a file nobody
/// wrote is the failure this catches. The partition test beside `NOT_READ` is
/// what keeps it honest.
const READ: [&str; 12] = [
    "md",
    "rs",
    "toml",
    "yml",
    "sql",
    "cedar",
    "cedarschema",
    // **And the four kinds a seven-name list left outside.** Measured by a
    // reviewer on 2026-09-11: eleven of this repository's 170 files wore a kind
    // the list did not name, and the gate a rename leans on had never opened
    // the file that says how to run it (`justfile`), the one that measures what
    // a checkout skips (`xtask/the-checkout-shape.sh`) or the meta-gate
    // (`xtask/mutants.py`). Three planted dangling paths in those files left the
    // whole transcript byte-identical to a clean run.
    "py",
    "sh",
    "tsv",
    "example",
    // The kernel's world, whose header cites the section that owed its three
    // additions and the two files it is held equal to.
    "wit",
];

/// The files this gate opens that have no extension to be named by.
///
/// `Path::extension` answers `None` for both — a leading dot is not a separator
/// and `justfile` has nothing to separate — so no list of extensions can reach
/// them, however wide it grows.
const READ_BY_NAME: [&str; 2] = ["justfile", ".gitignore"];

/// The kinds this repository holds that this gate does not open, and why.
///
/// **A list of what is read is only a claim while what is left out is
/// unwritten.** The test below partitions the kinds actually on disk across
/// this list and [`READ`], in both directions, so a kind the tree *gains* is a
/// failure rather than a silence — which is the half the previous guard could
/// not do, because its expectation was a third copy of `READ`'s own strings.
#[cfg(test)]
const NOT_READ: [(&str, &str); 3] = [
    (
        "json",
        "the world the policy cases are decided against — data, frozen row by row in `policy::THE_WORLD`, and not prose",
    ),
    ("lock", "resolved versions, written by cargo"),
    (
        "proptest-regressions",
        "seeds the proptest runner writes when a case fails",
    ),
];

/// The kind a file is judged by: its extension, or its whole name when it has
/// none.
fn kind_of(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_owned)
        .or_else(|| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
        })
}

/// Whether this gate opens the file at `path`.
fn reads(path: &Path) -> bool {
    kind_of(path)
        .is_some_and(|kind| READ.contains(&kind.as_str()) || READ_BY_NAME.contains(&kind.as_str()))
}

/// Extensions that make a token a path rather than a name.
const EXTENSIONS: [&str; 15] = [
    "md",
    "rs",
    "toml",
    "yml",
    "yaml",
    "json",
    "sql",
    "cedar",
    "cedarschema",
    "lock",
    "example",
    "gitkeep",
    // **The two kinds this repository cites by bare name and could not see.**
    // 37 citations of `mutants.py`, seven of `status-lines.tsv` and one of
    // `denote-corpus.tsv` were not accepted — they were never read, so renaming
    // either file would have left every one of those sentences dangling with
    // `check passed`. Measured by a reviewer on 2026-09-11 with a pair of probe
    // citations on one line: the `.md` half was reported at once and the `.tsv`
    // half moved the citation count by one rather than two.
    //
    // `sh` is deliberately not here, and the test below is what holds that
    // decision: no `.sh` file in this tree is cited by bare name, while three
    // spans of prose — `gate.sh` and `secrets.env.sh` — name scripts that
    // belong to a scratch tree rather than to this repository.
    "py",
    "tsv",
    // The kernel's world, which the kernel's sources and the plugin build cite
    // by name.
    "wit",
];

/// Checks every citation in the repository.
///
/// Returns the findings and the number of citations read, so a run that read
/// nothing is visible rather than green.
pub(crate) fn run(root: &Path) -> Result<(Vec<Finding>, usize, usize), String> {
    let mut findings = Vec::new();
    let mut read = 0_usize;
    // **How many lines were actually judged**, which is what the number above
    // could not say. `read` counts citations and its only floor was zero, so a
    // filter that skipped whole files — `metadata(path).len() > 4096`, which
    // reads like an optimisation — took the run from `757 citation(s) resolve`
    // to `38` with `check passed`, and every large document in the tree, which
    // is every document these gates exist for, went unread.
    let mut judged = 0_usize;
    // **The citations that reached a verdict**, against the citations read: the
    // second is on every path out of the judgement, so a filter between the two
    // makes them disagree — the one thing no position for a single counter can
    // do.
    let mut reached = 0_usize;
    // **How many citations this could not judge**, which used to be nothing a
    // reader saw. Without a workspace above the repository the names of the
    // documents that live there resolve against nothing, so they are accepted —
    // and the step still printed `ok 463 citation(s) resolve`, silently
    // switching from checking to accepting. A step that stopped performing part
    // of its check has to say so.
    let unjudged = core::sync::atomic::AtomicUsize::new(0);
    let known = basenames(root)?;
    let mut files = Vec::new();
    walk(root, &mut |path| {
        if reads(path) {
            files.push(path.to_path_buf());
        }
    })?;
    files.sort();
    // **The workspace documents are deliberately not walked here**, and the
    // section gate beside this one *does* walk them — so the difference is worth
    // stating rather than leaving to be discovered.
    //
    // A `§` means the same thing wherever it is written. A *path* does not:
    // `NOCTUA.md` describes five repositories, and a path in it may belong to
    // any of them — resolved from a root this gate has never seen.
    // Measured, walking them: **428 findings**, almost all of them real paths in
    // a repository that is not this one, plus package names like
    // scoped package names that are not paths at all. A gate that loud is one
    // people route around.
    //
    // Checking them needs a gate that knows all five repositories, which is the
    // workspace's to write and not this repository's — `AGENTS.md` there is
    // where it would go.

    for path in files {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        // **And it has to actually be generated.** The header is a sentence
        // anybody can type, and `stale` only polices the two generated trees —
        // so a hand-written `policy/core/NOTES.md` opening with it cited three <!-- allow-reference: names `policy/core/NOTES.md` — a probe file, written and removed -->
        // files nobody wrote, with `check passed`. Deleting that one line and
        // nothing else produced all three findings. This module's own header
        // calls the fix "a phrase nobody may write"; the phrase was writable.
        let relative = relative_to(root, &path);
        if is_generated(&text) && in_a_generated_tree(&relative) {
            // **Paid for, because a skipped file is a verdict too.** The floor
            // below was `judged * 2 < expected` — a walk could drop half the
            // tree and pass — and it was written as a half because the two
            // numbers were over different ground: `lines_of` counts every line
            // and this loop skipped some without saying so. Counting them here
            // makes the two exactly equal on a correct run, so the comparison
            // can be equality and no rule has to be shared between the walk and
            // its floor. `ui.rs` makes the same argument one file over.
            judged += text.lines().count();
            continue;
        }
        let markdown = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension == "md");
        let directory = path.parent().unwrap_or(root);
        let mut in_fence = false;
        // Whether a code span is still open from the line above — see
        // [`backticked`]. A blank line ends a paragraph and a fence ends
        // everything, so both reset it: a span left open by a line that forgot
        // its closing backtick must not swallow the rest of the document.
        let mut in_a_span = false;
        for (index, line) in text.lines().enumerate() {
            if line.trim_start().starts_with("```") {
                in_fence = !in_fence;
                in_a_span = false;
                judged += 1;
                continue;
            }
            in_a_span = in_a_span && !ends_a_paragraph(line);
            let cited = if in_fence && markdown {
                fenced(line)
            } else {
                citations(line, &mut in_a_span)
            };
            // **A span crosses a hard wrap, and a hard wrap only happens in
            // prose.** The state carried across every line of every kind, and a
            // blank line was the only thing that closed it — so one stray
            // backtick in a data cell of `crates/nc-ids/tests/denote-corpus.tsv`,
            // a file with no blank line in it, left the last thirteen of its
            // sixty-three lines read by nothing. Measured by a reviewer on
            // 2026-09-13, with two identical dangling citations planted either
            // side of that row: exactly one was reported. A line that is not
            // prose cannot be the first half of a wrapped span, so it closes
            // one.
            if !carries_prose(line, markdown) {
                in_a_span = false;
            }
            // **Counted before it is excused.** `read` is the number this step
            // prints as evidence it inspected anything, and every excused
            // citation was subtracted from it — so marking one line took the
            // coverage number *down*, from 577 to 576, and the only floor under
            // it is zero. An excused citation is one this gate read and was
            // told not to judge; that is still a citation it read.
            read += cited.len();
            judged += 1;
            let marked = excused(line, Comments::of(&relative));
            for citation in cited {
                // The last statement each judged citation reaches is
                // `reached += 1`, and it is on every path out of this body, so
                // a filter written between the counter above and the judgement
                // below makes the two disagree.
                if resolves(root, directory, &citation, &known, &unjudged) {
                    reached += 1;
                    continue;
                }
                // **And the reason names the citation**, which on this rule it
                // did not have to. One `// allow-reference: x` silenced six
                // dangling paths and a non-existent section on one line — seven
                // judgements for one character — and, because citations are
                // counted before they are excused, the two numbers that prove
                // the gates did any work went *up* while they were switched off.
                // Four of the six markers in this tree already follow this rule.
                if marked
                    && reason_after(line, ALLOW, Comments::of(&relative))
                        .is_some_and(|reason| names(reason, &citation))
                {
                    reached += 1;
                    continue;
                }
                reached += 1;
                findings.push(Finding {
                    file: relative.clone(),
                    line: index + 1,
                    what: format!("`{citation}` names a path that does not exist"),
                    fix: format!(
                        "a document that describes a file nobody wrote reads as true to \
                         everyone who does not go and look — create it, correct the \
                         citation, or mark the line `{ALLOW} <reason>` if it names \
                         something deliberately absent"
                    ),
                });
            }
        }
    }
    crate::gates::everything_reached_a_verdict("citation", HERE, read, reached, &mut findings);
    findings.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    // **A floor on the lines, because the floor on the citations is zero.** A
    // filter that skips whole files leaves `read` looking like a measurement and
    // makes it one of a fraction of the tree — measured at 38 of 757, with every
    // document these gates exist for among the ones skipped. `judged` is the
    // number a filter cannot leave alone, and `lines_of` counts the same lines
    // with different code.
    findings.extend(the_walk_is_short(root, judged)?);
    Ok((
        findings,
        read,
        unjudged.load(core::sync::atomic::Ordering::Relaxed),
    ))
}

/// The walk read every line a second count of the same files found.
///
/// **Exact.** Every line this walk skips pays the counter, so the two numbers
/// are the same count of the same ground and a half is slack rather than a
/// floor — `floor_on_the_rust_lines` makes this argument about the Rust walks
/// and its own comment condemns "half", which "would let a walk reading 60 % of
/// it pass". Measured before: one `if relative.starts_with("crates/nc-")` took
/// this gate from 964 citations to 782 — every crate's source, 44 % of the
/// corpus — with both static canaries still reported and `check passed`.
///
/// **Lifted out of the walk so the comparison can be driven.** A reviewer
/// loosened it to `judged * 2 >= expected` on 2026-09-14 and the whole suite
/// stayed green: the only test that reached it did so through the walk, which
/// hands it the true number, and a sibling in `xtask/src/sections.rs` carried
/// the same undecided clause. A function is what a corpus can hand a fraction
/// to.
fn the_walk_is_short(root: &Path, judged: usize) -> Result<Vec<Finding>, String> {
    let expected = lines_of(root)?;
    // **`expected == 0` is not a pass, it is the one case worth hearing.** When
    // both counts are zero `judged == expected` already answers, so the clause
    // could only ever decide when the second count found *nothing* and the walk
    // claimed to have judged something — which is precisely the state that says
    // the second count is broken, and it was being silenced. A reviewer dropped
    // the clause in round sixty-four with every test green; writing the fixture
    // showed it was worse than undecided.
    if judged == expected {
        return Ok(Vec::new());
    }
    Ok(vec![Finding {
        file: HERE.to_owned(),
        line: 1,
        what: format!(
            "the citation walk judged {judged} line(s) and a second count of the same \
             files found {expected}"
        ),
        fix: "something inside the walk is filtering, and a citation in the part it \
              skipped reads as checked. The number of citations cannot say this: its \
              floor is zero, and a walk that reads one file reports a number that \
              looks like a measurement"
            .to_owned(),
    }])
}

/// How many lines the files this gate reads hold, counted separately.
///
/// Its own walk over the same ground, so a filter inside the *body* of the
/// gate's walk makes the two disagree — which is the defect it was written
/// after, a `starts_with("crates/nc-")` that took the gate from 964 citations
/// to 782 with everything green.
///
/// **And a filter inside [`reads`] does not, which this paragraph used to deny.**
/// Both walks ask `reads` which files they are about, deliberately: that
/// predicate is the definition of the gate's ground, and two copies of it would
/// drift. So narrowing `reads` moves both numbers together and this comparison
/// stays silent — measured in round sixty-five with a path filter inside it,
/// which took the run from 2,663 citations to 2,169 and left the floor saying
/// nothing at all. What refuses that is not here but in the gates' own
/// fixtures, `the_citation_gates_and_the_emitter_walk_reach_one_too` and
/// `in_every_real_directory`: they plant a dangling citation in each real
/// directory and require the gate to reach it, so a `reads` that stops covering
/// a directory fails by not finding what was put there. This one holds the
/// other half — the walk reading fewer lines of the files it does read — and a
/// doc that claims both is a doc a reader stops checking.
///
/// `census` is the same idea one gate along, and its doc says why a shared list
/// of names would be a mirror rather than a floor.
fn lines_of(root: &Path) -> Result<usize, String> {
    let mut found = 0_usize;
    crate::gates::walk(root, &mut |path| {
        if !reads(path) {
            return;
        }
        if let Ok(text) = std::fs::read_to_string(path) {
            found += text.lines().count();
        }
    })?;
    Ok(found)
}

/// Whether a path is inside a directory the compiler writes.
///
/// The other half of [`is_generated`]. Both are needed: the header alone is a
/// sentence a hand-written file can type, and the directory alone would exempt
/// a hand-written file that happens to live beside generated output.
fn in_a_generated_tree(relative: &str) -> bool {
    nc_catalog_compiler::GENERATED_DIRECTORIES
        .iter()
        .any(|directory| relative.starts_with(&format!("{directory}/")))
}

/// Whether a file is generated output, judged by its header rather than by its
/// prose.
fn is_generated(text: &str) -> bool {
    text.lines().take(HEADER_LINES).any(|line| {
        line.trim_start()
            // Markdown's comment opener as well as Rust's and TOML's: the
            // developer surfaces under `docs/` are generated too, and a header
            // they cannot express is a header they would have to do without.
            .trim_start_matches("<!--")
            .trim_start()
            .trim_start_matches(['/', '!', '#', '*'])
            .trim_start()
            .starts_with(GENERATED)
    })
}

/// Every path-shaped token on an ordinary line: backticked runs and Markdown
/// link targets.
fn citations(line: &str, open: &mut bool) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for token in backticked(line, open).into_iter().chain(link_targets(line)) {
        if looks_like_a_path(&token) {
            found.insert(token);
        }
    }
    found
}

/// Every path-shaped token inside a fenced block of a Markdown document.
///
/// This is where a repository map lives, which is where a document is most
/// likely to describe a tree that has moved — and the least likely place for
/// anyone to notice, because a map is read as a picture rather than as a list of
/// claims.
///
/// A fence also holds shell commands, so a line that starts with one is skipped
/// whole: `cargo xtask build` has no paths in it and `rm -rf target/` is not a
/// citation of `target/`. <!-- allow-reference: `target/` is build output, which CARGO_TARGET_DIR may put elsewhere -->
fn fenced(line: &str) -> BTreeSet<String> {
    let trimmed = line
        .trim_start_matches([' ', '│', '├', '└', '─', '#', '$'])
        .trim_start();
    // **A command is a word, and `docker-compose.yml` is not `docker`.** The
    // rule asked whether the line *starts with* one of these, so a map entry
    // whose filename begins with a command word was skipped whole — measured on
    // 2026-09-08, the extension changed on `AGENTS.md`'s repository-map line for
    // the compose file — a name no file has: `ok 2195 citation(s) resolve`, byte
    // for byte what a clean run prints, while the same token written second on
    // the neighbouring line is reported at once. `fenced` exists *for*
    // repository maps —
    // *"the least likely place for anyone to notice, because a map is read as a
    // picture"* — and the hatch meant to skip shell commands turned the gate off
    // for a map entry instead.
    //
    // `every_word_that_skips_a_fenced_line_begins_a_command_somewhere` tests
    // that each word begins *some* line; it never tested the hole. A command's
    // name is followed by a space or ends the line; a filename carries on.
    if COMMANDS.iter().any(|command| {
        trimmed
            .strip_prefix(command)
            .is_some_and(|rest| command.ends_with(' ') || rest.is_empty() || rest.starts_with(' '))
    }) {
        return BTreeSet::new();
    }
    let mut found = BTreeSet::new();
    for token in trimmed.split_whitespace() {
        // A tree line is `catalog/  ← what it is`; the arrow and everything
        // after it is prose about the entry, not another entry.
        if token.starts_with('←') || token.starts_with("<-") {
            break;
        }
        let token = token.trim_end_matches([',', ';', ':']);
        if looks_like_a_path(token) && (token.ends_with('/') || has_extension(token)) {
            found.insert(token.to_owned());
        }
    }
    found
}

/// Line openings that make a fenced line a command rather than a map.
const COMMANDS: [&str; 10] = [
    "cargo", "just", "git", "docker", "npm", "pnpm", "rustup", "sh ", "cd ", "export ",
];

/// Whether a line is prose, and so may wrap a code span onto the next line.
///
/// Every line of a Markdown document is; in a source file, a comment line is —
/// `///`, `//!`, `//`, `#` and `--`, which is the same list [`ends_a_paragraph`]
/// strips. A row of a table or a line of code is not: it ends where it ends.
fn carries_prose(line: &str, markdown: bool) -> bool {
    if markdown {
        return true;
    }
    let trimmed = line.trim_start();
    ["///", "//!", "//", "#", "--"]
        .iter()
        .any(|opener| trimmed.starts_with(opener))
}

/// Whether a line ends the paragraph a code span may cross.
///
/// **A blank line is not how a paragraph ends in source.** The reset was
/// `line.trim().is_empty()`, and in a Rust comment block a paragraph break is
/// written `    //` — four spaces and two slashes, which is not empty. So a span
/// left open by one odd backtick carried through the whole comment block, and
/// every citation below it was read one backtick out of step. Measured on
/// 2026-09-10, after the run rule closed the fence-marker case: **1530 lines**
/// were still entered with a span open and **six real path citations were read
/// by nothing** — one of them proved by renaming the cited file and watching
/// `ok 2289 citation(s) resolve` come back byte-identical, while the same token
/// on a clean line 869 lines later was reported at once.
///
/// A line whose whole content is its comment marker is that break. The marker
/// forms are this repository's own — `//`, `///`, `//!`, `#` and `--` — and
/// anything after one makes the line content again, so a wrapped span whose
/// continuation carries text still closes on it.
///
/// **This does not close the class.** Modelled over every file the gate reads it
/// takes the poisoned lines to 1241 and the unread citations to four; what is
/// left needs a reader that works in paragraphs rather than in lines, which is
/// the limit `backticked` records about itself.
fn ends_a_paragraph(line: &str) -> bool {
    let trimmed = line.trim();
    let bare = trimmed
        .strip_prefix("///")
        .or_else(|| trimmed.strip_prefix("//!"))
        .or_else(|| trimmed.strip_prefix("//"))
        .or_else(|| trimmed.strip_prefix("--"))
        .or_else(|| trimmed.strip_prefix('#'))
        .unwrap_or(trimmed);
    bare.trim().is_empty()
}

fn backticked(line: &str, open: &mut bool) -> Vec<String> {
    // **A code span may cross a hard wrap, and these documents wrap at eighty
    // columns.** The pairing was per line, so a span that opened on one line and
    // closed on the next left an odd backtick count — and on the continuation
    // line every pair was offset by one, which makes every *real* backticked
    // path after it invisible. Ninety-two lines outside a fence in the documents
    // this gate reads already have an odd count. Measured on 2026-09-08 by
    // renaming the path inside an existing wrapped span on `AGENTS.md`:
    // `ok 2195 citation(s) resolve`, byte-identical to a clean run — the
    // citation was not accepted, it was never read — while the same token on an
    // ordinary line is reported at once.
    //
    // So the state carries across lines, and the caller resets it at a blank
    // line and at a fence. What is still *not* judged is the wrapped span
    // itself: its text is split between two lines and neither half is a path.
    // That is a smaller hole than every span after it on the line, and it is the
    // one `counts.rs` avoids by reading paragraphs rather than lines — which
    // this gate cannot do, because a finding here names a line.
    // **A run of backticks is not a backtick, and one odd line switched the gate
    // off for its whole paragraph.** Pairing them one at a time makes any line
    // with an odd count open a span that is not there — and the commonest such
    // line in this tree is not a wrapped span at all: it is `"```"` inside a
    // Rust string, or Markdown's own escape for a literal backtick, `` ``` ``.
    // Measured on 2026-09-10 on the untouched tree: **4833 lines** were entered
    // with a span open and **29 path citations were never read** — including
    // nine poisoned by one line of `check.rs` that spells the fence marker
    // inside a diagnostic. A probe path written on such a line was not accepted,
    // it was never seen, while the same probe on the next clean line was
    // reported at once.
    //
    // So a *run* of two or more is literal — it wraps nothing and toggles
    // nothing — and only a single backtick opens or closes. Every genuine
    // hard-wrapped span in these documents still closes on the first backtick of
    // its continuation line, because that is a run of one.
    let mut found = Vec::new();
    // `Some` while a span that opened **on this line** is being read. A span
    // carried in from the line above stays `None`, so its tail is not offered as
    // a citation — the paragraph above says why that half is not judged, and
    // widening it here would be a second change hiding inside a repair.
    let mut since: Option<String> = None;
    let characters: Vec<char> = line.chars().collect();
    let mut at = 0;
    while at < characters.len() {
        if characters[at] != '`' {
            if let Some(text) = since.as_mut() {
                text.push(characters[at]);
            }
            at += 1;
            continue;
        }
        let mut run = 0;
        while characters.get(at + run) == Some(&'`') {
            run += 1;
        }
        at += run;
        if run != 1 {
            if let Some(text) = since.as_mut() {
                for _ in 0..run {
                    text.push('`');
                }
            }
            continue;
        }
        if *open {
            if let Some(text) = since.take() {
                found.push(text);
            }
            *open = false;
        } else {
            *open = true;
            since = Some(String::new());
        }
    }
    found
}

/// Every markdown link target on a line, in the order they are written.
///
/// **A loop, and nothing drove it as one.** This walks `](` to the closing
/// paren and continues past it, so a paragraph citing two files is two
/// citations. A reviewer tried to plant it as first-only in round sixty-seven
/// and the plant did not compile, which left the clause **unmeasured in both
/// directions** — and no line in this tree carries two links, so the live run
/// could not have noticed either. The citation gate reads every target this
/// returns; a walk that stopped at the first would leave the second file in a
/// pair unchecked, which is the shape `the_repository_cites_nothing_that_is_missing`
/// exists to refuse.
///
/// An unclosed `](` ends the walk rather than reaching past the line, because a
/// paren that never closes is not a link and the text after it is prose.
fn link_targets(line: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = line;
    while let Some(open) = rest.find("](") {
        rest = &rest[open + 2..];
        let Some(close) = rest.find(')') else { break };
        found.push(rest[..close].to_owned());
        rest = &rest[close + 1..];
    }
    found
}

fn has_extension(token: &str) -> bool {
    token
        .rsplit_once('.')
        .is_some_and(|(_, extension)| EXTENSIONS.contains(&extension))
}

/// Whether a token is a path this gate should resolve.
fn looks_like_a_path(token: &str) -> bool {
    if token.is_empty() || token.len() > 200 {
        return false;
    }
    // A pattern, a placeholder, a shell line or a URL. Each is prose about a
    // shape rather than a citation of one file.
    if token.starts_with("http")
        || token.contains(char::is_whitespace)
        || token.contains(['{', '}', '<', '>', '*', '?', '$', '|', '"', '\'', '…'])
        // Punctuation that happens to hold a slash: a bare comment marker, a
        // fraction, an arrow. A path has at least one letter or digit in it.
        || !token.chars().any(char::is_alphanumeric)
    {
        return false;
    }
    // An absolute path is a location on some machine — a mount point inside a
    // container, a system directory — and never a citation of a file in this
    // repository. `/var/lib/postgresql` is a real path and resolving it here
    // would mean resolving it against the developer's own root.
    if token.starts_with('/') {
        return false;
    }
    // A bare hostname — `github.com/noctua-world` — has slashes and is not a
    // path. A dot in the FIRST segment of something that HAS segments is what
    // separates it from `.github/…` and from `catalog/i18n/en.toml`, whose dots
    // are all further along. The `contains('/')` guard is load-bearing: without
    // it this would reject `AGENTS.md`, which is a filename and not a host.
    if token.contains('/')
        && token
            .split('/')
            .next()
            .is_some_and(|first| first.trim_start_matches('.').contains('.'))
    {
        return false;
    }
    // **A file with no extension at all is still a file.** `justfile` and
    // `.gitignore` are cited by bare name in this tree, and no extension list
    // can reach either however wide it grows — so both were read as ordinary
    // words and renaming one would have left those sentences behind with
    // `check passed`. `.gitignore` also walks straight into the rule below,
    // which refuses `.rs` for being the name of a kind and cannot tell the two
    // apart. `READ_BY_NAME` is the same two names this gate opens, and the
    // partition test holds it to every extensionless file on disk.
    if READ_BY_NAME.contains(&token) {
        return true;
    }
    // **A file type is not a file.** `.rs`, `.sql`, `.md` written in prose —
    // "every `.rs` file in the tree" — is a name for a kind, and it has an
    // extension and nothing else, so `has_extension` called it a citation and
    // the gate reported `` `.rs` names a path that does not exist``. Any
    // sentence naming a file type by its extension was a hard failure. A
    // citation has something *before* the dot, or a slash in it.
    if token.starts_with('.') && !token[1..].contains(['.', '/']) {
        return false;
    }
    // A trailing slash is how this repository writes a directory, and it is the
    // only reliable signal: `commands.dispatch` has a dot and is a function.
    //
    // **The third arm is a directory named without one**, and it is the arm this
    // predicate's tests never reached: every fixture in this file had an
    // extension. Measured with it removed — `policy/core`, `crates/nc-log`,
    // `catalog/i18n` and eighty-four more left the gate, the run printed
    // `1945 citation(s) resolve` against a baseline of 2032, both canaries still
    // fired, and `check passed`. A rename is exactly where a directory citation
    // bites, and the floor under this gate counts *lines*, so a filter that
    // drops citations rather than lines is invisible to it by construction.
    token.ends_with('/') || has_extension(token) || token.contains('/')
}

/// The documents that live above a repository, and cannot be checked without one.
///
/// `NOCTUA.md §3.1` puts these in the workspace, not in any repository, so a
/// checkout of this repository alone has none of them. Written out rather than
/// derived, because the point is that this list is *short and known*: anything
/// not on it is judged whether or not a workspace is there.
const UNJUDGEABLE_WITHOUT_THE_WORKSPACE: [&str; 9] = [
    "NOCTUA.md",
    "NOCTUA-PLAN.md",
    "reference",
    "AGENTS.md",
    "README.md",
    "CLAUDE.md",
    // The workspace's own directory of repositories, which this one sits in.
    "repos",
    // The two normative documents by their bare names, which is how prose in
    // this repository refers to them once `reference/` has been said once.
    "identity-and-authorization.md",
    "noctua-kernel.wit",
];

/// Whether a citation resolves.
///
/// Three roots, in the order a reader would try them: beside the citing file,
/// at the repository root, and at the **workspace** root. The third is not a
/// convenience — this repository legitimately cites `NOCTUA.md`,
/// `NOCTUA-PLAN.md` and `reference/`, which live one level up and are the
/// documents every agent here is sent to (`NOCTUA.md §3.1`). Without it the gate
/// would report the citations it most wants to keep honest.
fn resolves(
    root: &Path,
    directory: &Path,
    citation: &str,
    known: &BTreeSet<String>,
    unjudged: &core::sync::atomic::AtomicUsize,
) -> bool {
    // **One spelling for a path with repeated separators.** A doubled slash after
    // `repos` misses the `repos/<this repository>/` prefix that makes a citation
    // judgeable, and then matches `repos/`, which marks it unjudgeable — so one
    // extra character turned a checked path into an accepted one. Nobody writes
    // that on purpose, which is exactly why nothing would notice it being
    // written.
    let citation: String = {
        let mut collapsed = String::with_capacity(citation.len());
        let mut last_was_slash = false;
        for character in citation.chars() {
            if character == '/' && last_was_slash {
                continue;
            }
            last_was_slash = character == '/';
            collapsed.push(character);
        }
        collapsed
    };
    let citation = citation.as_str();
    let trimmed = citation.trim_end_matches('/');
    // An explicit `..` is a path into a sibling repository, which has its own
    // gate and which this one cannot assume is checked out.
    //
    // **Judged when it resolves, and counted when it cannot be.** This returned
    // before both — so a citation of that shape was accepted unconditionally,
    // in a workspace *and* in a bare checkout, and moved the unjudged count by
    // zero. Measured with a citation of that shape naming a file that does not
    // exist, planted in `crates/nc-prune/src/lib.rs`: a finding in neither
    // configuration and invisible to the number that exists to say what was not
    // looked at. The design system is depended on by exactly that path shape
    // (`Cargo.toml`'s `[workspace.dependencies]`), so it is the form a citation
    // into it naturally takes — and this comment does not spell one out,
    // because writing the example is writing the defect.
    if trimmed.starts_with("..") {
        if directory.join(trimmed).exists() || root.join(trimmed).exists() {
            return true;
        }
        // **Unjudgeable only when there is nothing to judge against.** Counting
        // it either way made a real dangling citation into an announced skip
        // *with a false reason*: measured with the workspace in place,
        // `skipped: 1 of them name a workspace document and could not be
        // judged: there is no workspace above this repository` — which is wrong
        // three ways over, since it is not a workspace document, it *was*
        // judged, and there is a workspace. With one above, a sibling
        // repository either is checked out or is a citation this gate can say
        // is missing.
        if workspace_root(root).is_some() {
            return false;
        }
        unjudged.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        return true;
    }
    if directory.join(trimmed).exists() || root.join(trimmed).exists() {
        return true;
    }
    if let Some(workspace) = workspace_root(root) {
        if workspace.join(trimmed).exists() {
            return true;
        }
    } else {
        // **No workspace above means these cannot be judged, not that they are
        // wrong.** A checkout action clones one repository into an empty
        // directory, and every citation naming a workspace document —
        // `NOCTUA.md`, `NOCTUA-PLAN.md`, `reference/identity-and-authorization.md`
        // — then resolved against nothing. Measured, this repository copied
        // alone: **dozens of dangling reference(s)**, none of them a defect in
        // the tree. The exact number moved as the tree grew — it was 52 when this
        // was written and 62 a day later, which is why it is not written down.
        //
        // A citation this gate cannot check is one it must not report, and the
        // set of unjudgeable names is small and known: they are the documents
        // `§3.1` puts above every repository. Everything else — every path into
        // this tree, which is what this gate is for — is judged exactly as
        // before.
        //
        // **A path into *this* repository is judgeable wherever it is
        // written.** `repos/` was on the unjudgeable list, so every citation
        // written workspace-relative — `repos/<this repository>/crates/…` —
        // went unjudged in a checkout, which is most of what this gate is for.
        // Measured: a citation of that shape naming a crate that does not exist
        // was an error in the workspace and accepted in CI, with the count
        // silently rising.
        if let Some(here) = std::fs::canonicalize(root).ok().and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        }) && let Some(inside) = trimmed.strip_prefix(&format!("repos/{here}/"))
        {
            return root.join(inside).exists();
        }
        if UNJUDGEABLE_WITHOUT_THE_WORKSPACE
            .iter()
            .any(|above| trimmed == *above || trimmed.starts_with(&format!("{above}/")))
        {
            unjudged.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            return true;
        }
    }
    // The repository's own name, which is how a repository map opens. Resolved
    // through the filesystem rather than from the path text: `root` is built
    // from the manifest directory and ends in `..`, so `file_name()` on it
    // answers `".."`.
    if std::fs::canonicalize(root)
        .ok()
        .and_then(|path| path.file_name().map(|name| name == trimmed))
        .unwrap_or(false)
    {
        return true;
    }
    // A bare name — a file or a directory — names the thing rather than the
    // location. Prose says `identity-and-authorization.md`, not the four
    // segments that reach it, and a repository map draws `i18n/` under the
    // parent it is nested below rather than repeating the path. Any file or
    // directory of that name answers it, which is deliberately looser than a
    // path: the failure this gate exists for is a rename, and a rename changes
    // the name too.
    !trimmed.contains('/') && known.contains(trimmed)
}

/// The workspace root, found the way every `AGENTS.md` finds it: by walking up
/// for the `.noctua-workspace` marker (`NOCTUA.md §3.6`).
fn workspace_root(root: &Path) -> Option<std::path::PathBuf> {
    // One detector for the whole tool: two of them disagreed, and a run then
    // held two different facts about whether it was inside a workspace.
    crate::sections::workspace_of(root)
}

/// Every file and directory name in this repository, and every file name in the
/// workspace's own documents.
fn basenames(root: &Path) -> Result<BTreeSet<String>, String> {
    let mut names = BTreeSet::new();
    walk(root, &mut |path| {
        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            names.insert(name.to_owned());
        }
        // Directories too, and by walking up rather than by visiting them: the
        // walk yields files, and a repository map cites the directories.
        let mut parent = path.parent();
        while let Some(directory) = parent {
            if directory == root {
                break;
            }
            if let Some(name) = directory.file_name().and_then(|name| name.to_str()) {
                names.insert(name.to_owned());
            }
            parent = directory.parent();
        }
    })?;
    if let Some(workspace) = workspace_root(root) {
        // The workspace's own files and `reference/`, which is where the two
        // normative documents live (`NOCTUA.md §3.1`). Not `repos/`: a sibling
        // repository is not this one's to vouch for.
        for directory in [workspace.clone(), workspace.join("reference")] {
            let Ok(entries) = std::fs::read_dir(&directory) else {
                continue;
            };
            for entry in entries.flatten() {
                if entry.path().is_file()
                    && let Some(name) = entry.file_name().to_str()
                {
                    names.insert(name.to_owned());
                }
            }
        }
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    /// A file type named by its extension is not a citation.
    ///
    /// **Any prose naming one was a hard failure.** "every `.rs` file in the
    /// tree" reported `` `.rs` names a path that does not exist``, because
    /// `has_extension` is true of a token that is nothing but an extension.
    /// `.github/workflows/ci.yml` is a real path that also starts with a dot,
    /// which is why the rule asks for something after it.
    #[test]
    fn a_file_type_is_not_a_file() {
        for kind in [".rs", ".sql", ".md", ".toml", ".cedar"] {
            assert!(
                !super::looks_like_a_path(kind),
                "`{kind}` is a file type, and naming one in prose is not a citation"
            );
        }
        // The floor: a dotfile path is still a path, or this rule would silence
        // the gate over `.github/` and `.cargo/`.
        for path in [
            ".github/workflows/ci.yml",
            ".cargo/config.toml",
            ".env.example",
            "crates/nc-ids/src/lib.rs",
            "AGENTS.md",
        ] {
            assert!(
                super::looks_like_a_path(path),
                "`{path}` is a citation and stopped being read as one"
            );
        }
    }

    /// A comment that closed before the marker excuses nothing.
    ///
    /// **The scanner opened comments and never closed them.** `/*` and `<!--`
    /// set its flag and stopped the scan, so any comment earlier on the line
    /// made a marker after it count as commented — and `AGENTS.md`'s own
    /// invariant for every hatch in this tree is *the marker has to be in a
    /// comment*. Measured, one of this repository's real hatches rewritten in
    /// place so the marker sat in rendered Markdown prose: the hatch census,
    /// the per-marker count and `2038 citation(s) resolve` were all
    /// byte-identical and `431 passed`.
    ///
    /// The Rust half is the same code and worse: stopping at `/*` meant the
    /// scan never reached a marker inside a string literal after one, which is
    /// exactly the shape [`a_marker_inside_a_raw_string_is_not_a_comment`]
    /// exists to refuse.
    #[test]
    fn a_marker_after_a_closed_comment_is_not_in_one() {
        for (line, comments) in [
            (
                "The pack is json, <!-- a note --> allow-reference: a reason with no citation in it",
                Comments::Markdown,
            ),
            (
                "The core holds six invariants. <!-- a --> allow-count: counted elsewhere",
                Comments::Markdown,
            ),
            (
                "let x = 1; /* a note */ allow-logging: reason",
                Comments::Slashes,
            ),
            (
                "let x = /* a note */ \"allow-logging: written as data\";",
                Comments::Slashes,
            ),
        ] {
            let marker = if line.contains("allow-reference:") {
                "allow-reference:"
            } else if line.contains("allow-count:") {
                "allow-count:"
            } else {
                "allow-logging:"
            };
            assert!(
                !super::marked_in(line, marker, comments),
                "a marker outside every comment on its line excused it: {line}"
            );
        }

        // And the floor: a marker genuinely inside a block comment still is.
        for (line, comments) in [
            (
                "The pack is json. <!-- allow-reference: a reason with no citation in it -->",
                Comments::Markdown,
            ),
            ("let x = 1; /* allow-logging: reason */", Comments::Slashes),
            (
                "let x = 1; /* a note */ /* allow-logging: reason */",
                Comments::Slashes,
            ),
        ] {
            let marker = if line.contains("allow-reference:") {
                "allow-reference:"
            } else {
                "allow-logging:"
            };
            assert!(
                super::marked_in(line, marker, comments),
                "a real hatch inside a comment was refused: {line}"
            );
        }
    }

    /// A marker written inside a raw string is data, not a comment.
    ///
    /// **The shape this function's own header says it closed, reopened by two
    /// characters.** The scanner opened a string at the first `"` of
    /// `r#"a "b // allow-logging: x"#` and closed it at the second, so the `//`
    /// landed outside a string and the marker excused its line — measured
    /// against the mint scan, with a real `from_crockford` on the line and
    /// `check passed`. Every gate that reads a marker went through here.
    #[test]
    fn a_marker_inside_a_raw_string_is_not_a_comment() {
        for data in [
            "let x = r#\"a \"b // allow-logging: from_crockford\"#;",
            "let x = r\"// allow-logging: from_crockford\";",
            "let x = br#\"a \"b // allow-logging: from_crockford\"#;",
            "let x = \"// allow-logging: from_crockford\";",
        ] {
            assert!(
                !super::marked_in(data, "allow-logging:", super::Comments::Slashes),
                "a marker written as data excused its line: {data}"
            );
        }
        // And a real comment on a line holding a raw string still is one — the
        // floor, which a scanner that gave up on `r#` would fail.
        for real in [
            "let x = r#\"a \"b\"#; // allow-logging: from_crockford is a fixture",
            "let x = r\"plain\"; // allow-logging: from_crockford is a fixture",
        ] {
            assert!(
                super::marked_in(real, "allow-logging:", super::Comments::Slashes),
                "a real marker was refused: {real}"
            );
        }
        // **A constant that merely names a marker is data, not a hatch.** This
        // is the shape the hatch census in `check.rs` read as a declaration for
        // as long as it scanned a text that keeps literal contents: four
        // fixtures in this tree already hold the characters `&str =` inside a
        // string, and the census was green only because none of their values
        // ended in a colon. Written here because this is where the question "is
        // this marker code or data" already lives.
        assert!(!super::marked_in(
            r#"const M: &str = "nc-probe:";"#,
            "nc-probe:",
            super::Comments::Slashes
        ));
    }

    use super::{Comments, HEADER_IS_SHORT, NOT_READ, READ, READ_BY_NAME, excused};
    use super::{citations, fenced, is_generated, looks_like_a_path, run};
    use std::path::Path;

    /// A counter no test reads: the unjudged tally is about a checkout with no
    /// workspace, and these run inside one.
    fn nothing() -> core::sync::atomic::AtomicUsize {
        core::sync::atomic::AtomicUsize::new(0)
    }

    fn root() -> &'static Path {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
    }

    #[test]
    fn every_word_that_skips_a_fenced_line_begins_a_command_somewhere() {
        // **`COMMANDS` decides which fenced lines are read at all**, and half of
        // it could be broken without anything noticing: a word removed means
        // that line's paths become citations (noise a reader has to excuse), and
        // a word *added* means every line starting with it stops being read
        // (silence). Neither direction had a test.
        //
        // Anchored to the tree: each word has to begin a fenced line somewhere
        // in the documents this gate reads, or it is a rule about nothing.
        let mut begins: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        // Every kind this gate reads, not only Markdown: `fenced` is applied to
        // the lines of any of them, and `git` begins a line in a `.rs` doc
        // comment before it begins one in a document.
        crate::gates::walk(root(), &mut |path| {
            if !path
                .extension()
                .and_then(|kind| kind.to_str())
                .is_some_and(|kind| READ.contains(&kind))
            {
                return;
            }
            let Ok(text) = std::fs::read_to_string(path) else {
                return;
            };
            for line in text.lines() {
                let trimmed = line
                    .trim_start_matches([' ', '│', '├', '└', '─', '#', '$'])
                    .trim_start();
                for command in super::COMMANDS {
                    if trimmed.starts_with(command) {
                        begins.insert(command);
                    }
                }
            }
        })
        .expect("the tree is readable");
        // `npm`, `pnpm` and `rustup` are here for the day a document names them;
        // the rest are words this repository's own documents already begin lines
        // with, and each of those is what makes the list a rule rather than a
        // wish.
        for command in ["cargo", "just", "git", "docker"] {
            assert!(
                begins.contains(command),
                "`{command}` is in `COMMANDS` and begins no fenced line in any document, \
                 so nothing would notice it being removed"
            );
        }
        assert!(
            begins.len() >= 4,
            "only {} of the words in `COMMANDS` begin a line anywhere: {begins:?}",
            begins.len()
        );
    }

    #[test]
    fn the_generated_marker_is_looked_for_only_in_a_header() {
        // Compared as values rather than asserted as a constant: clippy reads
        // `assert!(CONST <= CONST)` as a constant and refuses it, and the point
        // is the *bound*, not the literal.
        assert_eq!(
            super::HEADER_LINES.clamp(1, HEADER_IS_SHORT),
            super::HEADER_LINES,
            "the generated-file marker is looked for {} lines in, and a window that \
             reaches into a file's prose exempts a hand-written file from this gate the \
             first time it quotes the sentence",
            super::HEADER_LINES
        );
    }

    /// Every kind of file on disk, as the gate judges kinds.
    fn the_kinds_on_disk() -> std::collections::BTreeSet<String> {
        let mut present: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        crate::gates::walk(root(), &mut |path| {
            if let Some(kind) = super::kind_of(path) {
                present.insert(kind);
            }
        })
        .expect("the tree is readable");
        present
    }

    #[test]
    fn every_kind_on_disk_is_either_read_or_written_down_as_unread() {
        // **The previous guard was a third copy of the list it was checking.**
        // It walked the tree into a set and then iterated the same seven
        // strings, skipping any the tree did not hold — so it could catch a
        // kind *removed* from the claim list and never a kind the tree
        // *gained*, which is the direction the defect actually arrived from.
        // Measured by a reviewer on 2026-09-11.
        let present = the_kinds_on_disk();
        for kind in &present {
            let read = READ.contains(&kind.as_str()) || READ_BY_NAME.contains(&kind.as_str());
            let written_off = NOT_READ.iter().any(|(named, _)| named == kind);
            assert!(
                read != written_off,
                "this repository holds `{kind}` files and the gate neither reads them nor \
                 says why it does not — add the kind to READ, or a row to NOT_READ with \
                 the reason a path cannot be cited there"
            );
        }
        for kind in READ.iter().chain(READ_BY_NAME.iter()) {
            assert!(
                present.contains(*kind),
                "`{kind}` is named as read and this repository holds no such file, so \
                 nothing decides whether the walk still opens one"
            );
        }
        for (kind, why) in NOT_READ {
            assert!(
                present.contains(kind),
                "`{kind}` is written off as unread and this repository holds no such file"
            );
            assert!(
                !why.trim().is_empty(),
                "`{kind}` is written off with no reason"
            );
        }
        // **And every file with no extension at all is named.** `Path::extension`
        // answers `None` for `justfile` and for a leading-dot name, so a list of
        // extensions cannot reach one however wide it grows — and a name that is
        // in neither list is a file this gate cannot open and cannot see cited.
        let mut nameless: Vec<String> = Vec::new();
        crate::gates::walk(root(), &mut |path| {
            if path.extension().is_some() {
                return;
            }
            if let Some(name) = path.file_name().and_then(|name| name.to_str())
                && !READ_BY_NAME.contains(&name)
            {
                nameless.push(name.to_owned());
            }
        })
        .expect("the tree is readable");
        nameless.sort();
        nameless.dedup();
        assert!(
            nameless.is_empty(),
            "these files have no extension and are named by neither list: {nameless:#?}"
        );
    }

    #[test]
    fn nothing_the_gate_leaves_unread_names_a_path() {
        // **What holds `NOT_READ` to the tree**, the way `sections.rs` holds
        // its own with the `\u{a7}`: a kind could otherwise be moved out of
        // `READ` into `NOT_READ` with a sentence beside it, and the partition
        // above would still pass. A citation is a backticked token that looks
        // like a path, which is the gate's own rule rather than a second one.
        let mut looked = 0_usize;
        let mut naming = Vec::new();
        crate::gates::walk(root(), &mut |path| {
            let Some(kind) = super::kind_of(path) else {
                return;
            };
            if !NOT_READ.iter().any(|(named, _)| *named == kind) {
                return;
            }
            looked += 1;
            let Ok(text) = std::fs::read_to_string(path) else {
                return;
            };
            let mut open = false;
            for (line, text) in text.lines().enumerate() {
                for token in super::backticked(text, &mut open) {
                    if super::looks_like_a_path(&token) {
                        naming.push(format!("{}:{} `{token}`", path.display(), line + 1));
                    }
                }
            }
        })
        .expect("the tree is readable");
        assert!(
            naming.is_empty(),
            "these lines cite a path and their kind is written off as unread: {naming:#?}"
        );
        assert!(
            looked >= NOT_READ.len(),
            "only {looked} file(s) of an unread kind were opened, and there are {} kinds \
             written off",
            NOT_READ.len()
        );
    }

    #[test]
    fn every_file_this_tree_cites_by_bare_name_wears_a_kind_extensions_names() {
        // **`EXTENSIONS` decides whether a token *is* a path**, so a bare name
        // of a kind it does not list is not judged at all — not accepted,
        // never read. Measured by a reviewer on 2026-09-11: 37 citations of
        // `mutants.py`, seven of `status-lines.tsv` and one of
        // `denote-corpus.tsv`, all of them invisible, in a gate whose own
        // header says the failure it exists for is a rename.
        //
        // The anchor is the tree rather than a hand-written second list: if a
        // file on disk is named by a bare backticked token anywhere the gate
        // reads, the kind it wears has to be one `EXTENSIONS` knows.
        let mut named: std::collections::BTreeMap<String, String> =
            std::collections::BTreeMap::new();
        crate::gates::walk(root(), &mut |path| {
            let (Some(name), Some(kind)) = (
                path.file_name().and_then(|name| name.to_str()),
                super::kind_of(path),
            ) else {
                return;
            };
            named.insert(name.to_owned(), kind);
        })
        .expect("the tree is readable");

        let mut missing: Vec<String> = Vec::new();
        let mut cited = 0_usize;
        crate::gates::walk(root(), &mut |path| {
            if !super::reads(path) {
                return;
            }
            let Ok(text) = std::fs::read_to_string(path) else {
                return;
            };
            let mut open = false;
            for line in text.lines() {
                for token in super::backticked(line, &mut open) {
                    if token.contains('/') {
                        continue;
                    }
                    let Some(kind) = named.get(&token) else {
                        continue;
                    };
                    cited += 1;
                    // The gate's own predicate, not a second copy of the list
                    // behind it: what matters is whether this token would be
                    // judged, however the rule decides that.
                    if !super::looks_like_a_path(&token) {
                        missing.push(format!("`{token}` is a {kind} file this tree holds"));
                    }
                }
            }
        })
        .expect("the tree is readable");
        missing.sort();
        missing.dedup();
        assert!(
            missing.is_empty(),
            "these files are cited by bare name and their kind does not make a token a \
             path, so a rename would leave the sentences behind: {missing:#?}"
        );
        assert!(
            cited > 100,
            "only {cited} bare name(s) of files in this tree were read, which is too few \
             for the check above to have looked at anything"
        );
    }

    #[test]
    fn dropping_a_kind_from_read_is_caught_rather_than_quietly_halving_the_gate() {
        // `READ` had no hole test. Removing `"md"` left `cargo xtask check`
        // green while the gate stopped reading every Markdown document in the
        // repository — the documents `§0.5` says agents keep current, and the
        // ones most likely to name a file that no longer exists.
        //
        // **What names the kinds is `READ` itself now**, held to the tree by
        // `every_kind_on_disk_is_either_read_or_written_down_as_unread`; the
        // claim list beside it was a second copy of the same strings and the
        // guard meant to anchor it was a third. What is left for this test is
        // the half it always did best: that the walk really opens one of each.
        let scratch = std::env::temp_dir().join(format!("nc-read-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).expect("writable");
        // Built rather than written as a literal: this file is walked by the
        // gate too, and a backticked path in the fixture is a citation.
        let absent = "catalog/there-is-no-such-file.toml";
        for kind in READ {
            std::fs::write(
                scratch.join(format!("doc.{kind}")),
                format!("this names `{absent}`\n"),
            )
            .expect("writable");
        }
        // **And the two with no extension**, which is why they are a list of
        // names rather than more entries in `READ`.
        for named in READ_BY_NAME {
            std::fs::write(scratch.join(named), format!("this names `{absent}`\n"))
                .expect("writable");
        }
        let expected = READ.len() + READ_BY_NAME.len();
        let (findings, read, _) = run(&scratch).expect("readable");
        assert_eq!(
            findings.len(),
            expected,
            "one kind of document was not read at all: {findings:#?}"
        );
        assert_eq!(read, expected);
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// A directory named without a trailing slash and without an extension.
    ///
    /// **The arm of `looks_like_a_path` that nothing tested.** Removing
    /// `token.contains('/')` dropped eighty-seven citations from the run with
    /// `check passed` and both canaries still firing, because every fixture in
    /// this file has an extension and the floor under the gate counts lines
    /// rather than citations.
    #[test]
    fn a_directory_without_a_slash_or_an_extension_is_a_citation() {
        for named in ["policy/core", "crates/nc-log", "catalog/i18n", "xtask/src"] {
            assert!(
                super::looks_like_a_path(named),
                "`{named}` is a directory this tree cites and the shape rule does not \
                 see it, so a rename leaves it dangling with nothing said"
            );
        }
        // And the shapes that merely hold a slash are still not citations — a
        // host, and a location on some machine.
        //
        // **What this arm does cost, written down:** a prose word with a slash
        // in it reads as a directory here and is reported as dangling. `and/or` is the one English writes, and this very line is the demonstration — it needs the hatch. // allow-reference: names `and/or`, a prose word with a slash in it and not a directory in this tree No document in this tree writes one, the
        // gate says exactly which token it means, and a marker excuses it; the
        // alternative is to give up directory citations, which is the eighty-seven
        // this test exists for.
        for prose in ["github.com/noctua-world", "/var/lib/postgresql"] {
            assert!(
                !super::looks_like_a_path(prose),
                "`{prose}` was read as a citation of a file in this tree"
            );
        }
    }

    #[test]
    fn excusing_a_citation_does_not_lower_the_number_that_proves_coverage() {
        // **Marking one line took the coverage number down.** `read` is what
        // this step prints as evidence it inspected anything, and an excused
        // citation was subtracted from it: planting a dangling path and then
        // excusing it moved the run from `read 577 citation(s)` to
        // `ok 576 citation(s) resolve`. The only floor under that number is
        // zero, so using the hatch is invisible in exactly the number that
        // exists to prove the gate is doing something.
        let scratch = std::env::temp_dir().join(format!("nc-excused-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).expect("writable");

        let cited = "This names `nowhere-at-all.toml`, which is not here.\n"; // allow-reference: names `nowhere-at-all.toml` — the fixture this test plants and then excuses
        std::fs::write(scratch.join("README.md"), cited).expect("writable");
        let (findings, plain, _) = super::run(&scratch).expect("readable");
        assert_eq!(findings.len(), 1, "the dangling path was not reported");

        std::fs::write(
            scratch.join("README.md"),
            format!(
                "{} <!-- {} `nowhere-at-all.toml` is coming in a later stage -->\n", // allow-reference: names `nowhere-at-all.toml` — the fixture this test writes into a scratch tree
                cited.trim_end(),
                super::ALLOW
            ),
        )
        .expect("writable");
        let (findings, excused, _) = super::run(&scratch).expect("readable");
        assert!(
            findings.is_empty(),
            "the marker did not excuse it: {findings:#?}"
        );
        assert_eq!(
            excused, plain,
            "excusing a citation lowered the number that proves this gate read anything"
        );
        assert!(plain > 0, "nothing was read, so this proved nothing");

        // **And a reason that names something else excuses nothing.** The rule
        // that the reason must *name* the citation was decided by nothing:
        // deleting it left this file's 21 tests and the whole 427-test suite
        // green, and every one of the tree's hatches for this gate became a
        // blanket excuse for whatever else landed on its line — which `cargo
        // fmt` arranges. `counts.rs` has held this direction for its own marker
        // since the same defect was found there; the two gates that share the
        // marker did not.
        std::fs::write(
            scratch.join("README.md"),
            format!(
                "{} <!-- {} the build directory, which CARGO_TARGET_DIR may put elsewhere -->\n",
                cited.trim_end(),
                super::ALLOW
            ),
        )
        .expect("writable");
        let (findings, _, _) = super::run(&scratch).expect("readable");
        assert_eq!(
            findings.len(),
            1,
            "a reason about something else excused the citation: {findings:#?}"
        );

        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn the_escape_hatch_needs_a_reason_and_a_bare_marker_excuses_nothing() {
        // It excused everything. The marker's own doc comment says "with a
        // mandatory reason" and the gate's fix text tells a reader to write
        // `allow-reference: <reason>`, and neither was ever checked — so the
        // hatch cost nothing to use, which is how a gate quietly stops applying.
        // The fixtures carry no backticks on purpose: this file is itself walked
        // by the gate, and a fixture that looked like a citation would be one.
        assert!(excused(
            "cites nope.toml <!-- allow-reference: it never existed -->",
            Comments::Slashes
        ));
        assert!(excused(
            "// allow-reference: the trap was found here",
            Comments::Slashes
        ));
        assert!(!excused(
            "cites nope.toml <!-- allow-reference: -->",
            Comments::Slashes
        ));
        assert!(!excused("// allow-reference:", Comments::Slashes));
        assert!(!excused("/* allow-reference: */", Comments::Slashes));
        assert!(!excused("// allow-reference:    ", Comments::Slashes));
        // And a line with no marker at all is not excused, or the ones above
        // would pass against a gate that excused nothing and everything alike.
        assert!(!excused("cites nope.toml", Comments::Slashes));

        // **The marker as data excuses nothing.** This read the raw line, so
        // writing the marker inside a *string literal* turned the gate off for
        // that line — and every gate that borrows this one reads source files
        // full of strings. Measured against the logging gate, with a real leak
        // at the call site beside it:
        //
        // ```text
        // let s = "allow-logging: nothing to see"; Box::leak(x);   -> excused
        // ```
        //
        // `wall.rs` writes its own needles with `concat!` because "this gate
        // reads its own source and a literal in a code position is a finding";
        // the marker it reads was not held to the same rule.
        let quote = '"';
        assert!(
            !excused(
                &format!("let s = {quote}allow-reference: not a reason{quote};"),
                Comments::Slashes
            ),
            "a marker inside a string literal excused the line"
        );
        assert!(
            !excused(
                &format!("let s = {quote}x{quote}; more(); allow-reference: still not a comment"),
                Comments::Slashes
            ),
            "a marker in a code position excused the line"
        );
        // And the same line with the marker where it belongs is excused, so the
        // rule is about where the marker is and not about the string beside it.
        assert!(excused(
            &format!("let s = {quote}x{quote}; // allow-reference: this one is a comment"),
            Comments::Slashes
        ));
        // Every comment opener the files this reads actually use.
        assert!(excused(
            "# allow-reference: a TOML or YAML comment",
            Comments::AndHash
        ));
        assert!(excused(
            "/* allow-reference: a block comment */",
            Comments::Slashes
        ));
        assert!(excused(
            "<!-- allow-reference: a Markdown comment -->",
            Comments::Slashes
        ));

        // **A raw string is not a comment, and `#` is not one in Rust.** The
        // rule was `'#' => opened = true` everywhere, so `r#"…"#` excused its
        // line — the defect the comment rule was written to close, one
        // character along. Measured through the citation gate: the count went
        // *up* by one, so the gate read the dangling path and then excused it.
        for as_data in [
            "let s = r#\"allow-reference: written as data\"#;",
            "#[doc = \"allow-reference: written as data\"]",
            "let r#type = 1; allow-reference: still code",
        ] {
            assert!(
                !excused(as_data, Comments::Slashes),
                "`{as_data}` excused its line"
            );
        }
        // And in Markdown, where `#` is a heading and `#42` an issue reference.
        for prose in [
            "## the gate reads a path allow-reference: visible prose",
            "- see #42 and a path allow-reference: visible prose",
        ] {
            assert!(
                !excused(prose, Comments::Slashes),
                "`{prose}` excused its line"
            );
        }
        // Where `#` *is* a comment it still is, and only when it opens one.
        assert!(excused(
            "key = 1 # allow-reference: a manifest",
            Comments::AndHash
        ));
        // SQL's own opener, for the migrations this reads.
        assert!(excused(
            "-- allow-reference: a migration's comment",
            Comments::AndDashes
        ));

        // **A character literal is not the start of a string.** The scanner
        // tracked the double quote and not the single one, so a line holding a
        // quote *as a character* opened a string that never closed and the
        // trailing `//` was never seen — a real comment, refused, on a line the
        // fix text tells the reader to mark.
        let quoted_char = format!("let separator = '{quote}';");
        assert!(
            excused(
                &format!("{quoted_char} // allow-reference: it is deliberately absent"),
                Comments::Slashes
            ),
            "a real comment after a character literal was refused"
        );
        // And a lifetime is not a character literal, or it would swallow the
        // rest of the line.
        assert!(excused(
            "fn f<'a>(x: &'a str) {} // allow-reference: a lifetime is not a literal",
            Comments::Slashes
        ));
    }

    /// A reason names its subject, and four clauses decide what naming is.
    ///
    /// **All four were decided by nothing.** The mutation runner reached this
    /// file for the first time and every operator over `names` and
    /// `closes_a_char` survived — which is what a runner reporting `SURVIVED`
    /// is *for*, and it is worth saying plainly that these are the clauses that
    /// decide whether a gate is switched off for a line. `AGENTS.md` invariant
    /// 15 is written about exactly this hatch.
    #[test]
    fn a_reason_names_its_subject_at_a_boundary_and_not_anywhere() {
        // **A bare subject matches everywhere.** `"".find("")` is `Some(0)` and
        // both boundaries of an empty span are whatever surrounds it, so
        // without the guard an empty subject is named by every reason there is
        // — a hatch that excuses everything, which is what a hatch must never
        // be. No caller passes one today; this function is `pub(crate)` and the
        // next one might.
        assert!(
            !super::names("a reason about something else entirely", ""),
            "an empty subject is named by nothing, or the hatch excuses everything"
        );

        // **A boundary, not a substring.** Without it a reason that merely
        // holds the letters excuses the subject: `code` inside `transcoder`.
        assert!(
            !super::names("the transcoder handles it", "code"),
            "a subject inside a longer word is not a mention of it"
        );
        assert!(
            super::names("the code path is fixed", "code"),
            "and a subject standing on its own is one"
        );

        // **A subject that ends in punctuation brings its own right boundary.**
        assert!(
            super::names("Utc::now is called once at start-up", "Utc::now"),
            "a subject ending in punctuation is named where it stands"
        );

        // **A slash after the subject makes it a directory rather than a
        // mention**, and a slash before it does not: the last segment of a path
        // is a way of naming the thing. Both halves measured on real reasons —
        // the first excused a count of four crates against eight.
        assert!(
            !super::names("the layout is fixed by crates/nc-log/fixtures", "crates"),
            "a directory named in passing is not a reason about the noun"
        );
        assert!(
            super::names(
                "the reader is nc-ids/from_crockford and only reads a stored id",
                "from_crockford"
            ),
            "the last segment of a path is a way of naming the thing"
        );

        // **The left boundary, which none of the cases above asks about.** Each
        // of them is refused by what follows the subject or accepted by both
        // sides at once, so the clause reading what *precedes* it decided
        // nothing: forced to `None` — which is a boundary — the suite stayed
        // green. What it is for is this doc block's own headline measurement —
        // a count hatch whose reason read `democrates migrations`, excusing a
        // claim about `crates` — which had never been written down as an
        // assertion. The marker itself is not spelled here: a paragraph about a
        // hatch that writes one adds a hatch to the census built to make them
        // visible. Round sixty-five.
        assert!(
            !super::names("democrates migrations", "crates"),
            "a subject at the end of a longer word is not a mention of it"
        );

        // **And the reason is read whole at every occurrence.** The walk cut
        // off everything up to and including each occurrence it rejected and
        // then asked the *cut* string what stood before the next one, so every
        // occurrence after the first began at index 0 and was handed a boundary
        // that is not there. Two running — `<anything><subject><subject>` — and
        // the rejection of the first accepted the second. Proven against the
        // shipped function before it was rewritten over `match_indices`.
        assert!(
            !super::names("democratescrates migrations", "crates"),
            "a rejected occurrence does not manufacture a left boundary for the next"
        );

        // **A subject ending in punctuation, followed by more word.** The
        // subject that makes this real is a needle `determinism.rs` actually
        // carries — the opening quote and root of an absolute path, written out
        // here rather than imported, because a driver taken from the list it
        // drives disappears with the entry it was meant to hold. Without the
        // clause this reason is refused and a live hatch stops working; the
        // cases above never needed it, because a space follows every one of
        // their subjects.
        assert!(
            super::names(
                "the only \"/dev/urandom read sits behind a test-only door",
                "\"/dev/"
            ),
            "a subject that brings its own right-hand boundary is named where it stands"
        );

        // **And the other direction of that clause, which only another file's
        // suite refused.** Every case above is decided by the left boundary or
        // by the slash rule, so making `ends_itself` always true — which
        // degrades this to a left-boundary test — was invisible here. What it
        // re-opens is the expensive shape this function's own doc names:
        // `login_attempts` is not a mention of `login`. Round sixty-six.
        assert!(
            !super::names("the login_attempts table is written once", "login"),
            "a subject at the start of a longer word is not a mention of it, and only \
             the right-hand boundary refuses it"
        );
    }

    /// Every link on a line is a citation, not just the first.
    ///
    /// The walk had no test, and no document in this tree writes two links on
    /// one line — so neither the suite nor a live run would have noticed it
    /// stopping at the first. Round sixty-seven's reviewer named it
    /// **unmeasured, not cleared**; this measures it.
    ///
    /// **The fixtures are assembled rather than written.** A literal
    /// `](docs/…)` in this file is a citation, and the gate one screen up
    /// resolves it — correctly, and it did. Interpolating the target keeps the
    /// link out of the source while handing this walk the same characters.
    #[test]
    fn every_link_on_a_line_is_a_citation() {
        let first = ["docs", "first.md"].join("/");
        let second = ["docs", "second.md"].join("/");
        assert_eq!(
            super::link_targets(&format!("see [one]({first}) and [two]({second}) today")),
            [first.clone(), second.clone()],
            "a line citing two files yielded something other than both, in order"
        );
        // A paren that never closes is not a link, and the walk stops rather
        // than reading the rest of the line as a target.
        assert_eq!(
            super::link_targets(&format!("an unclosed [one]({first} and then prose")),
            Vec::<String>::new(),
            "an unclosed link was read as a target"
        );
        // The first closing paren ends the first target, so a second link with
        // nothing between them is still found.
        assert_eq!(
            super::link_targets(&format!("[a]({first})[b]({second})")),
            [first, second],
            "two links with nothing between them were not both found"
        );
        let got = super::link_targets("no links at all");
        assert!(got.is_empty(), "a line with no link yielded one: {got:?}");
    }

    /// A reason is the comment's, and it ends where the comment does.
    ///
    /// **Three clauses, and all three were decided by nothing.** `marked_in`'s
    /// half of the marker rule is driven by the test above; the half that reads
    /// the *reason* had no unit test at all, and `xtask/mutants.py` carries
    /// operators for `names` and `closes_a_char` in this file and none for
    /// [`super::reason_after`]. Each clause below is a defect this tree
    /// measured once and then left to a comment to remember. Round sixty-five.
    #[test]
    fn a_reason_is_the_comments_and_ends_where_the_comment_does() {
        let marker = super::ALLOW;

        // **The last occurrence *in a comment*, not the last anywhere.** A
        // marker spelled in ordinary prose after a real one supplied the
        // reason, and the hatch census did not move — the prose spelling is
        // correctly not counted as a hatch, so the one instrument built to make
        // hatches visible reported nothing while a gate was steered by a
        // sentence. Measured on 2026-09-08 in a paragraph of
        // `policy/core/README.md` that already carried a real hatch.
        let prose = format!(
            "<!-- {marker} the citation is deliberately absent --> and the paragraph \
             goes on to spell {marker} in prose"
        );
        let reason = super::reason_after(&prose, marker, super::Comments::Markdown)
            .expect("the marker is in a comment on this line");
        assert!(
            reason.contains("deliberately absent"),
            "the reason came from somewhere other than the comment: {reason:?}"
        );
        assert!(
            !reason.contains("in prose"),
            "a marker written in prose after a real one supplied the reason: {reason:?}"
        );

        // **The honest direction bites too**: two properly written markers in
        // one paragraph, each naming its own noun, and only the last one is the
        // reason for the line — which is why the rule is the last *in a
        // comment* rather than the first.
        let twice = format!("<!-- {marker} about crates --> <!-- {marker} about sections -->");
        let reason = super::reason_after(&twice, marker, super::Comments::Markdown)
            .expect("both markers are in comments");
        assert!(
            reason.contains("sections") && !reason.contains("crates"),
            "the last marker in a comment is the one that speaks: {reason:?}"
        );

        // **Cut at `*/`, or the excused code is inside its own excuse.** A
        // marker in a block comment placed before the code leaves the offending
        // line itself in what the reason hands back, so `names(reason, needle)`
        // matches because the *line* holds the needle. Measured: that spelling
        // passed while the identical words after the code failed, and
        // `cargo fmt` preserves the block form.
        let block =
            format!("let id = /* {marker} the id is already stored */ from_crockford(text);");
        let reason = super::reason_after(&block, marker, super::Comments::Slashes)
            .expect("the marker is in a block comment");
        assert!(
            !reason.contains("from_crockford"),
            "the excused code is inside its own excuse: {reason:?}"
        );

        // **And cut at `-->` for the same reason in a document.**
        let html = format!("<!-- {marker} the count is stated once --> 14 documents");
        let reason = super::reason_after(&html, marker, super::Comments::Markdown)
            .expect("the marker is in a comment");
        assert!(
            !reason.contains("14 documents"),
            "the text after the comment closed is inside the reason: {reason:?}"
        );

        // **And a line with no marker in a comment has no reason**, rather than
        // the text after a marker that happens to sit in code.
        assert!(
            super::reason_after(
                &format!("let excuse = \"{marker} not a comment\";"),
                marker,
                super::Comments::Slashes
            )
            .is_none(),
            "a marker inside a string literal supplied a reason"
        );
    }

    /// An escape closes a char literal within a few characters, or not at all.
    ///
    /// The bound is what tells `'\n'` from an apostrophe in prose: unbounded,
    /// the `'` in *it's* pairs with the next one anywhere on the line and a
    /// stretch of ordinary text is read as a char literal — which moves where
    /// the marker reader thinks a comment begins.
    #[test]
    fn an_escape_closes_a_char_within_a_few_characters() {
        let short: Vec<char> = "'\\n' rest".chars().collect();
        assert!(
            super::closes_a_char(&short),
            "an escaped newline is a char literal"
        );
        let long: Vec<char> = "'\\uuuuuuuuuuuuuuuuuu' rest".chars().collect();
        assert!(
            !super::closes_a_char(&long),
            "a quote eighteen characters away is not this literal's end"
        );
    }

    #[test]
    fn a_backticked_path_is_a_citation() {
        let found = citations("see `catalog/errors.toml` for the codes", &mut false);
        assert!(found.contains("catalog/errors.toml"), "{found:?}");
    }

    /// A run of two or more backticks wraps nothing and opens no span.
    ///
    /// The line that switched the gate off for the rest of its paragraph.
    /// Neither of these opens a span, so the citation *after* them on the same
    /// line is read, and so is every line below them.
    #[test]
    fn a_run_of_backticks_wraps_nothing_and_leaves_no_span_open() {
        for literal in [
            "\"this document has an odd number of ``` markers\"",
            "write the markers as `` ``` `` when they are prose",
        ] {
            let mut open = false;
            let found = citations(&format!("{literal} — see `catalog/errors.toml`"), &mut open);
            assert!(
                found.contains("catalog/errors.toml"),
                "the citation after `{literal}` was not read: {found:?}"
            );
            assert!(
                !open,
                "`{literal}` left a span open, so every line below it is read one \
                 backtick out of step"
            );
        }
    }

    /// A bare comment marker ends the paragraph a span could cross.
    ///
    /// **A paragraph break in source is not a blank line.** It is `    //`, and
    /// the reset asked `line.trim().is_empty()` — so one odd backtick carried a
    /// phantom span through a whole comment block, and six real path citations
    /// in this repository were read by nothing. One was proved by renaming the
    /// file it cites and watching the run come back byte-identical.
    ///
    /// The negative cases are the ones that make this a rule rather than a
    /// filter: a marker with anything after it is content, and an attribute
    /// line begins with `#` and is code.
    #[test]
    fn a_bare_comment_marker_ends_the_paragraph_a_span_could_cross() {
        for ends in ["", "    //", "//", "///", "//!", "  --", "#"] {
            assert!(
                super::ends_a_paragraph(ends),
                "`{ends}` is a paragraph break in this repository's own sources and \
                 this reads it as content, so a span opened above it runs on"
            );
        }
        for holds in [
            "    // still the same paragraph",
            "/// see `catalog/errors.toml`",
            "#[derive(Debug)]",
            "# A heading",
            "-- a SQL comment with words",
        ] {
            assert!(
                !super::ends_a_paragraph(holds),
                "`{holds}` carries content and this reads it as a break, which closes a \
                 span that is still open"
            );
        }
    }

    /// A span that opened on the line above still closes on this one.
    ///
    /// The reason the state crosses lines at all, which the rule above must not
    /// undo: a single backtick is still an opener, and the first single backtick
    /// of the continuation line is still its close. Without this, the repair
    /// would be the other kind of wrong — a rule that refuses something
    /// legitimate, green only because nothing legitimate had arrived yet.
    #[test]
    fn a_span_that_crosses_a_hard_wrap_still_closes_on_the_next_line() {
        let mut open = false;
        let found = citations("the file is `catalog/", &mut open);
        assert!(found.is_empty(), "half a span is not a citation: {found:?}");
        assert!(open, "a lone backtick no longer opens a span");
        let found = citations("errors.toml` and `AGENTS.md` names it", &mut open);
        assert!(!open, "the continuation line did not close the span");
        assert!(
            found.contains("AGENTS.md"),
            "the citation after the wrapped span was not read: {found:?}"
        );
    }

    /// Which lines may wrap a code span onto the next one, both ways.
    ///
    /// **The rule that closes a span had no corpus of its own.** It decides
    /// whether a backtick left open on one line is still open on the next, and
    /// the two tests above drive it only through `citations` on Markdown, where
    /// it answers `true` for every line and decides nothing. A rule reached only
    /// where it is constant is a rule no test distinguishes from `true`.
    ///
    /// Both directions, because both have been wrong here: a data row that is
    /// not prose left thirteen lines of a `.tsv` read by nothing, and a comment
    /// line that *is* prose is the whole reason the state crosses lines at all.
    #[test]
    fn only_a_prose_line_wraps_a_span_onto_the_next() {
        for line in [
            "/// the file is `catalog/",
            "//! the file is `catalog/",
            "// the file is `catalog/",
            "    // indented, and still a comment",
            "# a shell or TOML comment",
            "-- a SQL comment",
        ] {
            assert!(
                super::carries_prose(line, false),
                "`{line}` is a comment in a source file and does not carry prose"
            );
        }
        for line in [
            "nc-name\tnc-name\tthe row `holds",
            "let path = \"catalog/errors.toml\";",
            "    \"a\" => 1,",
            "",
        ] {
            assert!(
                !super::carries_prose(line, false),
                "`{line}` is not prose and was read as a line a span may cross"
            );
        }
        // And in Markdown every line is prose, which is what makes the two
        // tests above unable to say anything about the list.
        assert!(
            super::carries_prose("| a | b |", true),
            "a Markdown line is prose whatever it holds"
        );
    }

    /// A walk that read a fraction of the lines is short, and an exact one is not.
    ///
    /// **The clause its own paragraph calls the whole of the floor was decided
    /// by nothing.** Loosened to `judged * 2 >= expected` the suite stayed green
    /// — measured on 2026-09-14 — because the only test that reached it did so
    /// through the walk, which hands it the true number. Half is what the
    /// paragraph condemns, so half is what this hands it.
    #[test]
    fn a_walk_that_read_half_the_lines_is_short() {
        let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        let expected = super::lines_of(root).expect("the tree is readable");
        assert!(
            expected > 100,
            "a second count of the files this gate reads found {expected} line(s), so \
             the comparison below is about nothing"
        );
        let got = super::the_walk_is_short(root, expected).expect("the tree is readable");
        assert!(
            got.is_empty(),
            "a walk that read every line was reported as short: {got:?}"
        );

        // **A second count of nothing, beside a walk that claims something.**
        // This is the only state the `expected == 0` clause could ever have
        // decided, and it decided it the wrong way: when both counts are zero
        // `judged == expected` already answers, so the clause spoke only when
        // the second count found nothing while the walk claimed to have judged
        // lines — which is exactly the state that says the second count is
        // broken, and it was returning "not short". Round sixty-four.
        let held = crate::check::LeavesNothingBehind::new("short-walk-zero");
        let empty = held.root.join("nothing");
        std::fs::create_dir_all(&empty).expect("writable");
        assert!(
            !super::the_walk_is_short(&empty, 5)
                .expect("an empty tree is readable")
                .is_empty(),
            "a second count of zero beside a walk claiming five line(s) was reported as \
             agreement, which is the one thing this comparison exists to refuse"
        );
        drop(held);
        for fraction in [expected / 2, expected - 1, expected + 1, 0] {
            assert_eq!(
                super::the_walk_is_short(root, fraction)
                    .expect("the tree is readable")
                    .len(),
                1,
                "a walk that judged {fraction} of {expected} line(s) was not reported \
                 as short, and a filter that skips whole files leaves the count \
                 looking like a measurement"
            );
        }
    }

    #[test]
    fn a_markdown_link_target_is_a_citation() {
        let found = citations("Read [`AGENTS.md`](AGENTS.md).", &mut false);
        assert!(found.contains("AGENTS.md"), "{found:?}");
    }

    #[test]
    fn a_repository_map_inside_a_fence_is_read() {
        // The place the original defect lived, and the place a gate reading only
        // backticks cannot see.
        let found = fenced("├── catalog/                        HAND-WRITTEN — the registries");
        assert!(found.contains("catalog/"), "{found:?}");
        let found = fenced("├── rust-toolchain.toml             the toolchain, pinned here");
        assert!(found.contains("rust-toolchain.toml"), "{found:?}");
    }

    #[test]
    fn a_command_inside_a_fence_is_not_a_citation() {
        // A fence holds shell as often as it holds a map, and `rm -rf target/`
        // is not a claim that `target/` exists. // allow-reference: `target/` is build output, which CARGO_TARGET_DIR may put elsewhere
        for command in [
            "cargo xtask check",
            "just check",
            "git clone <repository-url> noctua-central",
            "docker compose down -v",
            "cd path/to/noctua-workspace/repos",
        ] {
            assert!(fenced(command).is_empty(), "`{command}` read as a citation");
        }
    }

    /// And the hole: a map entry whose filename opens with a command word.
    ///
    /// **Every other exclusion in this file has a hole test; this one had the
    /// rule's positive direction only.** The five lines above are commands, so
    /// the word-boundary repair could be reverted to `starts_with(command)`
    /// with the whole suite green — measured on 2026-09-11, and the only trace
    /// was `2340 → 2339 citation(s) resolve`, a number nothing floors. The line
    /// floor cannot see it either: a line whose citations are discarded still
    /// pays the counter.
    ///
    /// A repository map is what `fenced` exists for, and this is the entry it
    /// was turning the gate off for.
    #[test]
    fn a_map_entry_that_opens_with_a_command_word_is_still_a_citation() {
        for (line, cited) in [
            ("├── docker-compose.yml", "docker-compose.yml"),
            ("│   └── just.toml", "just.toml"),
            ("├── rustup.toml", "rustup.toml"),
            ("cargo-features.toml", "cargo-features.toml"),
            ("├── npmrc.json", "npmrc.json"),
            ("└── gitignore.toml", "gitignore.toml"),
        ] {
            let found = fenced(line);
            assert!(
                found.contains(cited),
                "`{line}` was skipped whole by the command hatch, which is the hatch \
                 turning the gate off for a map entry: {found:?}"
            );
        }
    }

    #[test]
    fn the_prose_after_a_tree_entry_is_not_another_entry() {
        let found = fenced("├── docs/    GENERATED ← every code and limit, from catalog/");
        assert!(found.contains("docs/"), "{found:?}");
        assert_eq!(
            found.len(),
            1,
            "the prose after the arrow was read: {found:?}"
        );
    }

    #[test]
    fn a_pattern_is_prose_and_not_a_citation() {
        for prose in [
            "catalog/i18n/{en,pt-BR}.toml",
            "NC-<AREA>-<NNNN>",
            "crates/*",
            "cargo xtask check",
            "https://example.invalid/x.toml",
            "commands.dispatch",
            "github.com/noctua-world",
            "docs.rs/noctua-design",
            "/var/lib/postgresql",
            "/victoria-logs-data",
        ] {
            assert!(!looks_like_a_path(prose), "`{prose}` should read as prose");
        }
    }

    #[test]
    fn a_filename_with_a_dot_is_still_a_path() {
        for path in [
            "AGENTS.md",
            ".github/workflows/ci.yml",
            "catalog/i18n/en.toml",
        ] {
            assert!(looks_like_a_path(path), "`{path}` should read as a path");
        }
    }

    #[test]
    fn the_generated_marker_is_a_header_and_not_a_phrase_anyone_may_write() {
        // The exemption a reviewer defeated: with a whole-file substring match,
        // this very module exempted itself, because its prose contains the
        // phrase. A file that merely mentions the sentinel is not generated.
        assert!(is_generated(
            "//! Generated by nc-catalog-compiler — do not edit.\n//! more\n"
        ));
        assert!(!is_generated(
            "//! A module about generated files.\n\
             //!\n//! It matches `Generated by nc-catalog-compiler — do not edit.`\n"
        ));
        let mentioned_late = format!(
            "{}//! Generated by nc-catalog-compiler — do not edit.\n",
            "//! prose\n".repeat(20)
        );
        assert!(!is_generated(&mentioned_late));
    }

    #[test]
    fn a_document_of_this_repository_may_cite_the_workspace_above_it() {
        let known = super::basenames(root()).expect("the tree is readable");
        for citation in [
            "NOCTUA.md",
            "NOCTUA-PLAN.md",
            "reference/",
            "identity-and-authorization.md",
        ] {
            assert!(
                super::resolves(root(), root(), citation, &known, &nothing()),
                "`{citation}` is the workspace's, and it should resolve"
            );
        }
    }

    #[test]
    fn a_path_that_is_really_absent_does_not_resolve() {
        let known = super::basenames(root()).expect("the tree is readable");
        assert!(!super::resolves(
            root(),
            root(),
            "catalog/there-is-no-such-file.toml",
            &known,
            &nothing()
        ));
        assert!(!super::resolves(
            root(),
            root(),
            "there-is-no-such-file.toml",
            &known,
            &nothing()
        ));
    }

    #[test]
    fn a_doubled_slash_does_not_make_a_path_unjudgeable() {
        // A doubled slash after `repos` missed the prefix that makes a citation
        // judgeable and then matched `repos/`, which marks it unjudgeable — so
        // one extra character turned a checked path into an accepted one. Nobody writes that on purpose, which is exactly why
        // nothing would have noticed it being written.
        //
        // **Driven against a tree with no workspace above it**, because that is
        // the only branch where a citation is ever marked unjudgeable. Written
        // against the real tree it passed with the fix deleted: inside a
        // workspace the path simply fails to resolve, and the OS collapses the
        // doubled slash before anything here sees it.
        let scratch = std::env::temp_dir().join(format!("nc-slash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        let repo = scratch.join("noctua-central");
        std::fs::create_dir_all(repo.join("src")).expect("writable");
        std::fs::write(repo.join("src/real.rs"), "fn main() {}\n").expect("writable");
        assert!(
            super::workspace_root(&repo).is_none(),
            "this test needs a tree with no workspace above it"
        );

        let known = super::basenames(&repo).expect("the tree is readable");
        for citation in [
            "repos//noctua-central/src/nope.rs",
            "repos/noctua-central//src/nope.rs",
            "repos/noctua-central/src/nope.rs",
        ] {
            assert!(
                !super::resolves(&repo, &repo, citation, &known, &nothing()),
                "`{citation}` names nothing and was accepted"
            );
        }
        for citation in [
            "repos//noctua-central/src/real.rs",
            "repos/noctua-central/src/real.rs",
        ] {
            assert!(
                super::resolves(&repo, &repo, citation, &known, &nothing()),
                "`{citation}` names a real file and was reported"
            );
        }
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn the_gate_reads_something() {
        // A citation gate that read nothing would pass forever, which is the
        // failure NOCTUA.md §19.3 opens with.
        let (_, read, _) = run(root()).expect("the tree is readable");
        assert!(
            read > 20,
            "only {read} citations were read, so this proves nothing"
        );
    }

    #[test]
    fn the_repository_cites_nothing_that_is_missing() {
        let (findings, _, _) = run(root()).expect("the tree is readable");
        // **Except the canary, which is the one file that must be reported.**
        // `check` takes it out of what it reports and fails when it is *not*
        // there — a filter that hides lines hides the canary too, which is the
        // thing a count could not say. Here the same file has to be excluded, or
        // this test asserts the canary is absent.
        let real: Vec<&crate::gates::Finding> = findings
            .iter()
            .filter(|finding| !finding.file.ends_with("xtask/canaries/citations.md"))
            .collect();
        assert!(real.is_empty(), "{real:#?}");
        assert!(
            findings.len() > real.len(),
            "the canary was not reported, so the walk is not reaching it"
        );
    }

    /// Every name of four words or more that a Markdown file of this
    /// repository cites in backticks is a word its code or its data holds.
    ///
    /// **Found by a reviewer: `AGENTS.md` cited a test by the name it had
    /// before it was renamed**, and nothing read the name against the tree —
    /// the gate above resolves paths, and the other half of `NOCTUA.md §0.5`
    /// that a command can measure, a name being something the code declares,
    /// had no gate at all. A name is looked for as a whole word, so the name a
    /// rename lengthened is missing however much of the new one it spells.
    /// What this does not judge: a name of three words or fewer, which prose
    /// writes as often as code does, a span a fence or a hard wrap holds, and a
    /// stale name a comment somewhere still repeats, which is a word of the
    /// tree like any other.
    #[test]
    fn every_long_name_a_document_cites_is_a_word_of_the_tree() {
        let long = |text: &str| {
            text.starts_with(|first: char| first.is_ascii_lowercase())
                && text.split('_').count() >= 4
                && text.split('_').all(|word| {
                    !word.is_empty()
                        && word
                            .bytes()
                            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
                })
        };
        let mut documents = Vec::new();
        let mut words = std::collections::BTreeSet::new();
        crate::gates::walk(root(), &mut |path| {
            let Ok(text) = std::fs::read_to_string(path) else {
                return;
            };
            if path.extension().is_some_and(|extension| extension == "md") {
                documents.push((path.to_path_buf(), text));
            } else {
                words.extend(
                    text.split(|character: char| {
                        !(character.is_ascii_alphanumeric() || character == '_')
                    })
                    .filter(|word| long(word))
                    .map(str::to_owned),
                );
            }
        })
        .expect("the tree is walked");
        let mut judged = 0_usize;
        let mut missing = Vec::new();
        for (path, text) in &documents {
            let (mut in_a_span, mut in_a_fence) = (false, false);
            for (index, line) in text.lines().enumerate() {
                if line.trim_start().starts_with("```") {
                    in_a_fence = !in_a_fence;
                    in_a_span = false;
                    continue;
                }
                in_a_span = in_a_span && !super::ends_a_paragraph(line);
                if in_a_fence {
                    continue;
                }
                for cited in super::backticked(line, &mut in_a_span) {
                    if !long(&cited) {
                        continue;
                    }
                    judged += 1;
                    if !words.contains(&cited) {
                        missing.push(format!(
                            "{}:{}: `{cited}`",
                            path.strip_prefix(root()).unwrap_or(path).display(),
                            index + 1
                        ));
                    }
                }
            }
        }
        assert!(
            missing.is_empty(),
            "a document cites a name nothing outside the documents holds, as a whole \
             word — renamed, misspelt or never written: {missing:#?}"
        );
        assert!(
            judged > 80,
            "only {judged} name(s) of four words or more were judged in {} document(s), \
             so the walk is not reaching the documents that cite them",
            documents.len()
        );
    }

    /// The repository map draws every crate, and every directory a crate,
    /// a plugin or `xtask` holds beside its code.
    ///
    /// **Found by a reviewer: the kernel's `wit/` was not drawn.** It holds the
    /// one file that has to stay byte for byte the workspace's reference, and
    /// the map named the kernel and nothing under it — while nothing read the
    /// map against the tree in that direction. The citation gate asks whether
    /// what a map draws exists; this asks whether what exists is drawn. A
    /// member's `src/` and `tests/` are its code and are not drawn one by one,
    /// and `plugins/` is drawn as one directory each, so a plugin is judged by
    /// what it holds beside its code rather than by being drawn itself.
    #[test]
    fn the_repository_map_draws_every_crate_and_what_each_holds_beside_its_code() {
        let agents = std::fs::read_to_string(root().join("AGENTS.md")).expect("AGENTS.md");
        let drawn = drawn_in_the_map(&agents);
        let directories = |under: &Path| -> Vec<String> {
            let mut found: Vec<String> = std::fs::read_dir(under)
                .unwrap_or_else(|error| panic!("`{}` is readable: {error}", under.display()))
                .flatten()
                .filter(|entry| entry.path().is_dir())
                .filter_map(|entry| entry.file_name().to_str().map(ToOwned::to_owned))
                .collect();
            found.sort();
            found
        };
        let beside_the_code = |member: &str| -> Vec<String> {
            directories(&root().join(member))
                .into_iter()
                .filter(|held| !["src", "tests", "target"].contains(&held.as_str()))
                .map(|held| format!("{member}/{held}/"))
                .collect()
        };
        let crates = directories(&root().join("crates"));
        let mut expected = vec!["plugins/".to_owned(), "xtask/".to_owned()];
        for name in &crates {
            expected.push(format!("crates/{name}/"));
            expected.extend(beside_the_code(&format!("crates/{name}")));
        }
        for name in directories(&root().join("plugins")) {
            expected.extend(beside_the_code(&format!("plugins/{name}")));
        }
        expected.extend(beside_the_code("xtask"));
        let missing: Vec<&String> = expected
            .iter()
            .filter(|path| !drawn.contains(*path))
            .collect();
        assert!(
            missing.is_empty(),
            "`AGENTS.md`'s repository map does not draw {missing:?}. Draw each under its \
             parent, relative, with what it is for; a file in it the reference gate should \
             resolve is named by its whole path in the prose below the map"
        );
        let crates_drawn = drawn
            .iter()
            .filter(|path| {
                path.starts_with("crates/") && path.ends_with('/') && path.matches('/').count() == 2
            })
            .count();
        assert_eq!(
            (crates_drawn, crates.len() > 1),
            (crates.len(), true),
            "the map draws {crates_drawn} crate(s) and `crates/` holds {}: a map read \
             against a tree it no longer describes, or a walk that found nothing",
            crates.len()
        );
    }

    /// Every path the repository map in `agents` draws, rebuilt from the tree.
    ///
    /// Each level of the tree is four characters of `│`, `├──`, `└──` or space,
    /// and an entry's path is its parents' paths joined; the first word after
    /// the glyphs is the entry, and the rest of the line is what it is for.
    fn drawn_in_the_map(agents: &str) -> std::collections::BTreeSet<String> {
        let opens = agents
            .find("## Repository map")
            .expect("AGENTS.md has a repository map");
        let fence = opens
            + agents[opens..]
                .find("```\n")
                .expect("the map is a fenced block")
            + 4;
        let closes = fence + agents[fence..].find("\n```").expect("the fence closes");
        let mut parents: Vec<String> = Vec::new();
        let mut drawn = std::collections::BTreeSet::new();
        for line in agents[fence..closes].lines() {
            let entry = line.trim_start_matches([' ', '│', '├', '└', '─']);
            let depth = (line.chars().count() - entry.chars().count()) / 4;
            let Some(name) = entry.split_whitespace().next() else {
                continue;
            };
            // Depth zero is the repository's own name, which opens the map.
            if depth == 0 {
                continue;
            }
            parents.truncate(depth - 1);
            let path = format!("{}{name}", parents.concat());
            if name.ends_with('/') {
                parents.push(name.to_owned());
            }
            drawn.insert(path);
        }
        drawn
    }
}
