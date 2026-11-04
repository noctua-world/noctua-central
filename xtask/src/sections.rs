//! A `§` citation names a section that exists.
//!
//! The reference gate checks that a cited **path** exists and stops there, so
//! a citation to a section number nobody ever wrote passes it — and so does a
//! real miscitation, such as a manifest citing `§15.4` (the central log) for
//! the append-only rule, which lives in `§14.9`. Only reading finds that kind,
//! because `§15.4` exists; what this catches is a section number no document
//! has.
//!
//! # Why this can be mechanical at all
//!
//! Because the documents number their own headings, consistently, and have for
//! long enough that the shape is a convention rather than an accident:
//!
//! | Document | Top level | Sub-section |
//! |---|---|---|
//! | `NOCTUA.md` | `## §10 · Title` | `### 10.2 Title` |
//! | `NOCTUA-PLAN.md` | `## §P0 · Title` | `### P0.5 Title` |
//! | `reference/identity-and-authorization.md` | `## 4. Title` | — |
//!
//! # What it deliberately does not do
//!
//! It does not check that the section says what the citing sentence claims. No
//! gate can. What it catches is the whole class where a section was renumbered,
//! merged or never existed — which is every case this repository has actually
//! hit, because the citations are written from memory and the numbers move.

use std::collections::BTreeSet;
use std::path::Path;

use crate::gates::{Finding, walk};
use crate::relative_to;

/// This file, for the findings it files against its own counters.
///
/// Written once, so every call site names the same file, and because a call
/// naming it inline does not fit on a line.
const HERE: &str = "xtask/src/sections.rs";

/// The kinds of file whose `§` citations are read.
///
/// **Wider than the reference gate's, because `§` travels further than a path
/// does.** `md | rs | toml` alone leaves every citation in `policy/*.cedar`,
/// in the schema, in the migrations and in `docker-compose.yml` unread. Those
/// are the files where a section number gets renumbered under you, because
/// nobody rebuilds them for months.
pub(crate) const READ: [&str; 12] = [
    "md",
    "rs",
    "toml",
    "yml",
    "sql",
    "cedar",
    "cedarschema",
    // **And the four kinds a seven-name list leaves outside.** Measured:
    // under such a list, three planted miscitations in `justfile`,
    // `xtask/the-checkout-shape.sh` and `xtask/mutants.py` — files that carry
    // live `§` citations — leave the whole transcript byte-identical. The gate a
    // rename leans on would not read the file that says how to run the gate.
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
/// `Path::extension` answers `None` for both of these — a leading dot is not a
/// separator and `justfile` has nothing to separate — so no list of extensions
/// can ever reach them, however wide. Between them they carry three `§`
/// citations today.
pub(crate) const READ_BY_NAME: [&str; 2] = ["justfile", ".gitignore"];

/// The kinds this repository holds that this gate does not open, and why.
///
/// **A list of what is read is only a claim while what is left out is
/// unwritten.** The test below partitions the kinds actually on disk across
/// this list and [`READ`], in both directions, so a kind the tree *gains* is a
/// failure rather than a silence — which is the half a guard whose expectation
/// is a third copy of `READ`'s own strings cannot do.
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

/// The document a bare `§` refers to.
const DEFAULT: &str = "NOCTUA.md";

/// Every document whose sections may be cited, and where it lives relative to
/// the workspace root.
const DOCUMENTS: [&str; 3] = [
    "NOCTUA.md",
    "NOCTUA-PLAN.md",
    "reference/identity-and-authorization.md",
];

/// Whether this gate read a fraction of the lines a second count finds.
///
/// **The floor on the citations is zero**, so a filter that skips whole files
/// leaves `read` looking like a measurement while it measures a fraction of the
/// tree — measured without this, such a filter takes the run's
/// `section citation(s)` count to a small fraction of itself with
/// `check passed`, and every document these gates exist for is among the ones
/// skipped.
fn the_walk_is_short(root: &Path, judged: usize) -> Result<Vec<Finding>, String> {
    let expected = lines_read(root)?;
    // **Exact.** Every line this walk skips pays the counter and both sides
    // read the same six documents above the repository, so a half is slack
    // rather than a floor — `floor_on_the_rust_lines` makes the argument for the
    // Rust walks and its own comment condemns "half", which "would let a walk
    // reading 60 % of it pass". Measured under a half, on a corpus whose crate
    // sources were 44 % of its lines: one `if relative.starts_with("crates/nc-")`,
    // dropping every crate's source, leaves both static canaries reported, both
    // planted fixtures green, and `check passed`.
    // **`expected == 0` is not a pass, it is the one case worth hearing.** When
    // both counts are zero `judged == expected` already answers, so an
    // `expected == 0 ||` clause could only ever decide when the second count
    // finds *nothing* and the walk claims to have judged something — which is
    // precisely the state that says the second count is broken, and a clause
    // answering "not short" there would silence it.
    if judged == expected {
        return Ok(Vec::new());
    }
    Ok(vec![Finding {
        file: HERE.to_owned(),
        line: 1,
        what: format!(
            "the section walk judged {judged} line(s) and a second count of the same files \
             found {expected}"
        ),
        fix: "something inside the walk is filtering, and a `§` in the part it skipped \
              reads as checked. The number of citations cannot say this: its floor is zero"
            .to_owned(),
    }])
}

/// How many lines the files this gate reads hold, counted separately.
///
/// Its own walk, so a filter inside the gate's walk makes the two disagree —
/// the same floor `references.rs` carries, for the same measurement.
fn lines_read(root: &Path) -> Result<usize, String> {
    let mut found = 0_usize;
    walk(root, &mut |path| {
        if !reads(path) {
            return;
        }
        if let Ok(text) = std::fs::read_to_string(path) {
            found += text.lines().count();
        }
    })?;
    if let Some(workspace) = workspace_of(root) {
        // **All six, not three.** `DOCUMENTS` is the list of documents whose
        // *sections* may be cited; the walk also reads the workspace's own
        // `AGENTS.md`, `README.md` and `CLAUDE.md`, which do the citing. So a
        // floor over `DOCUMENTS` alone counts fewer lines than the walk — every
        // line of those three — and a gap like that forces a half rather than
        // an equality. Both sides count the same ground, so the comparison
        // below is exact.
        //
        // **And a walk rather than six names.** `counts.rs` one file over walks
        // the workspace for Markdown, under a doc block that argues the case in
        // as many words; a list of six leaves a seventh document under the
        // normative `reference/` — which `§0.4` puts above `NOCTUA.md` itself —
        // read by that gate and by nothing else, and a citation planted in one
        // leaves the count where it is.
        for document in crate::counts::the_workspace_documents(&workspace) {
            if let Ok(text) = std::fs::read_to_string(&document) {
                found += text.lines().count();
            }
        }
    }
    Ok(found)
}

/// Whether an `allow-reference:` reason on this line names this section.
///
/// Split out because the walk that calls it is at clippy's line limit, and
/// because it is the same question `references.rs` asks about a path — one
/// marker excusing every citation on a line is, measured, seven judgements for
/// one character, across two gates that share the marker.
fn names_the_section(line: &str, section: &str, comments: crate::references::Comments) -> bool {
    crate::references::reason_after(line, crate::references::ALLOW, comments)
        .is_some_and(|reason| crate::references::names(reason, &format!("§{section}")))
}

/// The fence a line opens or closes, as its character and its length.
///
/// **`CommonMark` has two fence characters, and this knows both.** Its clause
/// 4.5: *"A code fence is a sequence of at least three consecutive backtick
/// characters or tildes."* The paragraph above the first toggle records why the
/// toggle exists — a document that *shows* what a heading looks like mints
/// section numbers in its own example, and the gate then vouches for a section
/// no document has. Matching backticks only, the identical example written in a
/// `~~~` block mints the invented section again and a citation of it resolves
/// in silence. Nothing downstream can see that: a citation that resolves is
/// still read and still judged, so neither the citation count nor the
/// short-walk identity moves.
///
/// One rule for the three, because three copies of a rule drift apart and the
/// narrowest one decides.
fn the_fence_on(line: &str) -> Option<(char, usize, usize)> {
    let trimmed = line.trim_start();
    let opener = trimmed
        .chars()
        .next()
        .filter(|letter| *letter == '`' || *letter == '~')?;
    let run = trimmed
        .chars()
        .take_while(|letter| *letter == opener)
        .count();
    // **And `CommonMark` measures that indentation in columns, not in bytes.**
    // Its clause 2.2 advances a tab to the next four-column stop, so one byte of
    // tab is four columns — and a bound stated in columns while counting bytes
    // makes a tab-indented fence three columns *inside* the bound instead of
    // one column outside it. Measured against pandoc 3.7.0.2: with a
    // tab-indented ``` line inside a ```text block, pandoc keeps both that line
    // and the heading below it as content.
    //
    // **And the indentation is spaces and tabs, which is not what `trim_start`
    // removes.** It removes every Unicode `White_Space`, so a line beginning
    // U+00A0 — or U+2003, U+3000, U+000B, U+000C — would be one column of
    // indentation here and a fence. To `CommonMark` it is not a fence at all,
    // it is content: measured against pandoc 3.7.0.2, all five keep the line
    // *and* the heading below it inside the code block, and over this
    // workspace's own `NOCTUA.md` a planted U+00A0 fence leaves pandoc's header
    // count where it was while a `trim_start` reader loses every real heading
    // below it and reports every live citation of them as naming nothing.
    // Counting such a character as zero columns is too weak, measured: `at` is
    // then 0, which is still within the bound. The line has to stop being a
    // fence.
    let leading = &line[..line.len() - trimmed.len()];
    if leading.chars().any(|white| white != ' ' && white != '\t') {
        return None;
    }
    let at = leading.chars().fold(0_usize, |column, white| {
        if white == '\t' {
            column + 4 - column % 4
        } else {
            column + 1
        }
    });
    (run >= 3).then_some((opener, run, at))
}

/// Whether this line is the fence line, given what is open.
///
/// **`CommonMark` closes a fence with the character that opened it**, and with at
/// least as many. A toggle that knows both characters flips the state twice on
/// a `~~~` line *inside* a backtick block: the example heading printed after it
/// is minted as a real section, and the real heading below the block is then
/// read as fenced and lost.
///
/// Both halves are needed, and each is measured against pandoc.
/// Requiring the same **length** is too strong: four backticks legitimately
/// close a three-backtick block, so such a walk stays fenced for the rest of
/// the document and loses every heading below it — half of the defect this
/// closes. Tracking only the **character** is the mirror and too weak: pandoc
/// keeps a three-backtick line inside a four-backtick block as content. At
/// least as many, of the same character, is the rule that survives both.
fn the_fence_state(line: &str, fenced: &mut Option<(char, usize, usize)>) -> bool {
    match *fenced {
        Some((letter, run, indent)) => {
            // **And a fence indented past the opening one is content**, which
            // is the other half of `CommonMark`'s clause for the two fence
            // characters. A closing fence may be indented up to three spaces
            // further than the one that opened the block; at four it is part of
            // the content — measured with pandoc, where the four-space fence
            // *and* the heading below it stay inside the code block. Read as a
            // closer it does both halves of the damage at once: the example
            // heading is minted as a section, and the block's real closing fence
            // then opens a new one that never closes, so the heading below the
            // block is lost.
            //
            // The bound is **relative**, which is what keeps it from being too
            // strong. Measured, the absolute rule — refuse any fence at four or
            // more — breaks two real shapes: this file's own fixtures spell
            // their fences inside Rust string literals indented twenty columns,
            // and a fenced block opened at column four inside a numbered list
            // item is one pandoc renders as a fenced block.
            if the_fence_on(line)
                .is_some_and(|(shut, long, at)| shut == letter && long >= run && at <= indent + 3)
            {
                *fenced = None;
            }
            true
        }
        None => match the_fence_on(line) {
            Some(opened) => {
                *fenced = Some(opened);
                true
            }
            None => false,
        },
    }
}

/// Reads every `§` citation in the tree and checks each names a real section.
///
/// Returns the findings and how many citations were read, so a tree whose
/// documents moved is visible rather than green.
///
/// # Errors
///
/// Returns why the workspace documents could not be read. Their absence is an
/// error rather than an empty result: this repository sits inside the workspace
/// by `NOCTUA.md §3.5`, and a gate that passed loudest when it could not find
/// them would be worse than none.
pub(crate) fn every_section_cited_exists(root: &Path) -> Result<(Vec<Finding>, usize), String> {
    let mut judged = 0_usize;
    // **The citations that reached a verdict**, against the citations read. The
    // second counter is the last statement each judged citation reaches, so a
    // filter written between the two makes them disagree — which is the one
    // thing no position for a single counter can do.
    let mut reached = 0_usize;
    // **Found by its documents, not by counting directories up.** `root` may
    // arrive as `…/xtask/..`, and `Path::parent` is lexical — it would strip the
    // `..` and land inside `xtask`. Walking up until `NOCTUA.md` is beside a
    // `repos/` directory is the same thing `NOCTUA.md §3.6`'s guard does, and it
    // survives being called from anywhere.
    let workspace = workspace_of(root).ok_or_else(|| {
        format!(
            "no workspace holding {DEFAULT} was found above {}. This repository lives \
             inside one (§3.5), and every `§` in it points into that document",
            root.display()
        )
    })?;
    let workspace = workspace.as_path();

    let mut known: BTreeSet<String> = BTreeSet::new();
    for document in DOCUMENTS {
        let text = std::fs::read_to_string(workspace.join(document)).map_err(|error| {
            format!(
                "{document} could not be read: {error}. Every `§` in this repository \
                 points into it, and a gate that cannot open it would report a clean \
                 tree for the wrong reason"
            )
        })?;
        for section in headings(&text) {
            known.insert(format!("{document}#{section}"));
        }
    }
    if known.is_empty() {
        return Err("no numbered section was found in any workspace document".to_owned());
    }

    let mut findings = Vec::new();
    let mut read = 0_usize;
    let mut files = Vec::new();
    walk(root, &mut |path| {
        if reads(path) {
            files.push(path.to_path_buf());
        }
    })?;
    files.sort();
    // **The workspace documents cite themselves and each other**, so a walk of
    // the repository only — reading them to build the list of known sections
    // and never checking what they name — misses those: measured, a probe line
    // appended to `NOCTUA.md` is invisible to such a walk. `counts.rs` reaches
    // them for exactly the same reason; this is the other half.
    // **Every Markdown document in the workspace, walked.** `DOCUMENTS` is the
    // list of documents whose *sections* may be cited; it is not the list of
    // files that do the citing. Any list of names leaves one out: a document
    // under the normative `reference/` read by `counts.rs` and by nothing else
    // holds a citation — naming a section no document numbers — that leaves
    // the count unmoved. The walk beside it is the one that answers "which
    // documents are up there", so it is the one asked.
    files.extend(crate::counts::the_workspace_documents(workspace));

    for path in &files {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        // A workspace document is named by the workspace, not by this machine:
        // `relative_to` cannot strip a path above the repository.
        //
        // **And a file inside the repository keeps its repository-relative
        // name.** With the workspace prefix and no guard — the repository is
        // *inside* the workspace — one file comes out here as
        // `<workspace>/repos/noctua-central/crates/…` and from the reference gate
        // beside it as `crates/…`: two spellings for one file in one run, which
        // is a reader working out whether they are the same file. `counts.rs`
        // carries the same guard for the same measured reason.
        let inside = path.strip_prefix(root).is_ok();
        let relative = match path.strip_prefix(workspace) {
            Ok(above) if !inside => {
                format!("<workspace>/{}", above.to_string_lossy().replace('\\', "/"))
            }
            _ => relative_to(root, path),
        };
        // **`#` opens a heading in one of the kinds `READ` names, and a comment
        // in several of the others.** A heading rule for the workspace
        // documents, applied to every file, would skip a `§` in a column-zero
        // comment of a TOML or a YAML file — which is where nearly all of them
        // live: in this tree's TOML and YAML, a `§` outside a column-zero
        // comment is the exception. `§15.8` renumbered to a section that does
        // not exist, on `catalog/limits.toml`'s column-zero comment, prints the
        // usual `ok … section citation(s) name a real section` under such a
        // rule; the same text one space to the right is caught immediately, and
        // teaching the rule which files have headings raises the count by every
        // citation those comments carry.
        // **No `md`-only test here.** A test reading the file's extension into
        // `headings_start_with_a_hash` and then discarding it with `let _` would
        // be a dead computation: measured, `"md"` changed to `"rs"` and the gate
        // unmoved. The fence toggle below is about where a `§` sits rather than
        // about what kind of file holds it.
        // **A fence toggle.** Without it nothing shields a `§` in a shell
        // transcript or an example heading from being judged — and `counts.rs`
        // argues the opposite position for the same corpus: *a count inside a
        // code block is output or an example, and neither is a claim about this
        // tree.* The same is true of a citation. Measured without the toggle: a
        // section mark inside a fenced block is reported, which is a false
        // finding waiting for the first person to paste a transcript.
        let mut fenced: Option<(char, usize, usize)> = None;
        for (index, line) in text.lines().enumerate() {
            if the_fence_state(line, &mut fenced) {
                judged += 1;
                continue;
            }
            if fenced.is_some() {
                judged += 1;
                continue;
            }
            // **A heading is not a citation of itself.** `## §14 · Authorization`
            // holds a `§`, and it is the section's own number rather than a
            // reference to one — and only the workspace documents write a
            // heading that way.
            // **No heading skip, and none is needed.** A heading that names a
            // section *declares* it, so its own number resolves like any other.
            // A skip would put every `§` at the start of a `#` line outside the
            // gate: measured, a heading appended naming a section number that
            // does not exist leaves the count byte-identical, while the same
            // text one word to the right is caught immediately — and
            // `trim_start_matches('#')` strips every leading hash, so a `#`
            // comment inside a fenced block takes the same exit.
            //
            // Without the skip every such citation is judged, and a passing run
            // says none of them is wrong — so a skip would protect nothing and
            // hide a class.
            // **The reference gate's marker excuses this gate too**, and the
            // two say different things: a line excused for naming a file that
            // does not exist is also excused for every `§` on it. Nothing in the
            // tree relies on that today — no `allow-reference:` line carries a
            // section — and it is written down here so the next reader does not
            // have to discover it.
            //
            // **Counted before it is excused**, as its twin in `references.rs`
            // is. `read` is what this step prints as evidence it inspected
            // anything; with an excused citation subtracted from it, one marker
            // on a line carrying three citations moves the run's
            // `section citation(s)` count down by one — so silencing the gate on
            // a real miscitation *lowers* the number that proves the gate does
            // anything, while the hatch census goes *up*. The only floor under it
            // is zero.
            // **And the reason names the section.** This gate is reached by the
            // *same* marker as the path gate, so without that, one
            // `allow-reference: x` silences both on a line.
            // Measured: six dangling paths and one non-existent section, seven
            // judgements for one character, with both coverage numbers going up
            // rather than down because citations are counted before they are
            // excused.
            let marked =
                crate::references::excused(line, crate::references::Comments::of(&relative));
            let excused_here = |section: &str| {
                marked
                    && names_the_section(line, section, crate::references::Comments::of(&relative))
            };
            // **A bare `§` inside a workspace document resolves in either.**
            // The plan cites a bare number meaning the reference, and a bare
            // `P`-number meaning itself, in the same file and often the same
            // line — the
            // convention is that the plan's own sections are the `P`-numbered
            // ones and everything else is the reference. Encoding that as a
            // per-document rule would be a rule about two files; accepting a
            // section that exists in the containing document is what a reader
            // does, and it is the same latitude a bare `§` already has.
            let containing = DOCUMENTS
                .iter()
                .find(|document| relative.ends_with(*document))
                .copied();
            judged += 1;
            for (document, section) in cited(line) {
                read += 1;
                if resolves_here(&known, containing, line, &document, &section)
                    || excused_here(&section)
                {
                    reached += 1;
                    continue;
                }
                findings.push(missing(&relative, index, &document, &section));
                reached += 1;
            }
        }
    }
    // **A floor on the lines, because the floor on the citations is zero.** The
    // sibling gate carries the same one, for the same measurement: a filter
    // that skips whole files — `metadata(path).len() > 4096`, which reads like
    // an optimisation — takes the run's citation count to a small fraction of
    // itself with `check passed`, and every document these gates exist for is
    // among the ones skipped.
    findings.extend(the_walk_is_short(root, judged)?);
    crate::gates::everything_reached_a_verdict("citation", HERE, read, reached, &mut findings);
    Ok((findings, read))
}

/// Whether a citation names a section that exists, here or in its own document.
///
/// A `P`-numbered section inside the plan is one of its own, which is the
/// latitude a bare `§` already has: the plan cites a bare number meaning the
/// reference and a bare `P`-number meaning itself, in the same file and often
/// the same line.
fn resolves_here(
    known: &std::collections::BTreeSet<String>,
    containing: Option<&str>,
    line: &str,
    document: &str,
    section: &str,
) -> bool {
    known.contains(&format!("{document}#{section}"))
        || (document == DEFAULT
            && !line.contains(DEFAULT)
            && containing.is_some_and(|here| known.contains(&format!("{here}#{section}"))))
}

/// The finding for a `§` that names nothing.
fn missing(relative: &str, index: usize, document: &str, section: &str) -> Finding {
    Finding {
        file: relative.to_owned(),
        line: index + 1,
        what: format!("`{document} §{section}` names a section that does not exist"),
        fix: "a citation is how one file sends a reader to the rule it obeys, and a wrong \
              number sends them to a different rule that reads as though it were the right \
              one. Check the number, or mark the line `allow-reference: <reason>` naming \
              the section it excuses"
            .to_owned(),
    }
}

/// How many numbered list items sit directly under one heading.
///
/// A run of `1.`, `2.`, … at the start of a line, before the next heading. The
/// convention these documents use is to cite the *n*th of them as `§X.Y.n`, and
/// a gate that cannot represent that reports every such real citation as
/// naming nothing.
fn numbered_items_under(text: &str, heading: &str) -> usize {
    // **The heading is located by the same walk that reads the body.**
    // `text.find(heading)` is a plain substring search, blind to the fences the
    // rest of this function honours. When a document *shows* a heading inside a
    // fenced example above the real one, which is how these documents record a
    // convention and exactly what the fence toggle is for, such a scan begins
    // inside the code block: the example's numbers are counted as the real
    // section's items, and the block's closing fence is then read as an
    // *opening* one, so the real list is skipped entirely and the section is
    // credited with items nobody wrote.
    //
    // `text.rfind` is the cheap alternative and is wrong one shape along — an
    // example shown *below* the real heading moves the anchor past it — so the
    // anchor is found by carrying the fence state, like everything else here.
    let mut fenced: Option<(char, usize, usize)> = None;
    let mut at = None;
    let mut offset = 0;
    // **`candidate` rather than `line`**, because the loop header `headings`
    // uses is the text one of the meta-gate's mutation operators rewrites, and
    // a second occurrence of it anywhere in this file leaves that operator with
    // nothing to apply to — reported as `not applicable`, which is a clause
    // with no coverage. The gate says a *comment quoting* the clause is enough
    // to do it, which is why this note does not spell the header out: measured,
    // a note that does leaves the operator unapplied.
    for candidate in text.lines() {
        // **And `fenced.is_none()` is not asked here**, because it cannot be
        // false where it would be evaluated: `the_fence_state` returns `false`
        // on one path only — the arm where nothing is open and the line opens
        // nothing — and that path leaves `fenced` as `None`. It would read like the
        // clause keeping the anchor out of a fenced block while the first clause
        // and the state carried between lines do all of it, and no mutation of it
        // could be killed.
        if !the_fence_state(candidate, &mut fenced) && candidate.starts_with(heading) {
            at = Some(offset);
            break;
        }
        offset += candidate.len() + 1;
    }
    let Some(at) = at else {
        return 0;
    };
    let mut highest = 0_usize;
    fenced = None;
    for line in text[at + heading.len()..].lines() {
        // **A fence first**, because a `## ` inside one is example markdown and
        // would end the scan early, and a number inside one is example output.
        if the_fence_state(line, &mut fenced) {
            continue;
        }
        if fenced.is_some() {
            continue;
        }
        if line.starts_with("## ") || line.starts_with("### ") {
            break;
        }
        // **Emphasis and a quote marker stripped first.** These documents write
        // their items as `**1. Multi-realm.**`, so the number is not at the
        // start of the line and, unstripped, `§1.3.4` — which `NOCTUA.md`
        // cites — resolves to nothing; and `reference/identity-and-authorization.md`
        // numbers three WIT additions inside a blockquote, so `§2.3` would too.
        let line = line.trim_start_matches(['*', '_', ' ', '>']);
        // `str::split` always yields at least one piece, so a `let … else`
        // guard here would decide nothing — its `else` arm unreachable — and the
        // empty case is handled by the `!number.is_empty()` below, where it
        // belongs.
        let number = line.split('.').next().unwrap_or_default();
        // **A dot then a space**, which is what a list item is. `4.0` in
        // `CC BY-NC 4.0` — the tail of a hard-wrapped sentence, at the start of
        // its line — would credit the licence section with four items, and the
        // gate would vouch for three section numbers that do not exist. A
        // version string and a memory figure would do the same to two more sections.
        if !number.is_empty()
            && number.chars().all(|c| c.is_ascii_digit())
            && line.as_bytes().get(number.len()) == Some(&b'.')
            && line.as_bytes().get(number.len() + 1) == Some(&b' ')
            && let Ok(item) = number.parse::<usize>()
        {
            highest = highest.max(item);
        }
    }
    highest
}

/// What sits above this repository, and why it is or is not its workspace.
///
/// **The discriminant is kept, not computed and thrown away.** Ending
/// [`what_is_above`] in `.then_some(here)` makes a marker that was found and
/// rejected answer exactly like a marker that was never there — and four
/// `check` steps then print *"there is no workspace above this repository
/// (§3.5)"*, which sends a reader looking for a bare checkout when what they
/// have is a workspace with a misplaced marker.
///
/// The difference is not academic, and reproducing it takes **one file**: a
/// second copy of the marker inside a workspace's own `repos/`. The walk then
/// stops at that copy, this repository is not under `<workspace>/repos/repos/`,
/// and everything above it goes unread. Measured with that ending, on a
/// checkout with a marker one directory above it, which is the same shape:
///
/// ```text
/// warn  skipped: the section gate …
/// warn  skipped: … of them name a workspace document and could not be judged: …
/// warn  skipped: … workspace document(s) above this repository were not read: …
/// check passed
/// ```
///
/// The whole section gate, every document above the repository and every
/// citation into one, under a green verdict. The counts are elided on purpose:
/// they move as the tree grows, which is the lesson `references.rs` records
/// beside its own unjudgeable list. What does not move is that the reason each
/// of those lines gives is false, and that the probe every `AGENTS.md` carries
/// answers `inside-workspace:…` for the same tree — which it calls "not an
/// alarm, continue". The two guards a reader consults disagree, and the one
/// that is wrong is the one that prints a reason.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Above {
    /// No `.noctua-workspace` marker in this directory or in any above it —
    /// a bare checkout, which is what CI always has.
    NoMarker,
    /// The nearest marker is at this path, and this repository is not inside
    /// its `repos/`. Somebody else's workspace, and the honest answer to
    /// "which workspace is this repository's" is still "none".
    SomebodyElses(std::path::PathBuf),
    /// The nearest marker is at this path and this repository is inside its
    /// `repos/`, which is the layout `§3.5` requires.
    Ours(std::path::PathBuf),
}

/// Why the documents above this repository were not read, or `None` when they
/// were.
///
/// The clause a skipping step puts after its own name. Two shapes reach a
/// reader and they are not the same problem: one is a checkout with nothing
/// above it, which is what CI has and what nobody needs to fix, and the other
/// is a workspace whose marker is in the wrong place, which is a misconfigured
/// machine and takes one `mv` to repair. A step that reports the second as the
/// first costs the reader the whole diagnosis.
pub(crate) fn out_of_reach(above: &Above) -> Option<String> {
    match above {
        // Nothing was out of reach, so a step naming this would be announcing
        // a skip it did not take.
        Above::Ours(_) => None,
        Above::NoMarker => Some(format!(
            "there is no `{MARKER}` marker in any parent (§3.5)"
        )),
        Above::SomebodyElses(at) => Some(format!(
            "a workspace marker is at `{}`, but this repository is not inside its \
             `repos/` (§3.5), so its documents were not read",
            at.display()
        )),
    }
}

/// The workspace directory above `path`, if there is one.
///
/// A thin wrapper over [`what_is_above`], because "is there one" is what most
/// callers ask and a three-armed answer at each of them would be nine places
/// for the rule to drift. The two arms that are not a workspace collapse here,
/// which is the whole of what this loses — and the steps that print a reason
/// take [`what_is_above`] instead.
///
/// **No total of callers here.** A count of call sites in a paragraph is held by
/// nothing and goes stale the next time a step is added — a "nine of the ten
/// callers" stands beside three production sites that take the three-armed
/// answer — so it is not restated: `grep` answers it, and the proportion is
/// what the sentence is actually about.
pub(crate) fn workspace_of(path: &Path) -> Option<std::path::PathBuf> {
    match what_is_above(path) {
        Above::Ours(workspace) => Some(workspace),
        Above::NoMarker | Above::SomebodyElses(_) => None,
    }
}

/// Which of the three shapes sits above `path`.
pub(crate) fn what_is_above(path: &Path) -> Above {
    // **One answer, and the marker is what gives it.** A second detector that
    // looks for `NOCTUA.md` beside a `repos/`, while `references.rs` looks for
    // the `.noctua-workspace` marker file, disagrees with it, and the
    // disagreement is the dangerous direction — measured on a tree with the
    // documents and `repos/` present and no marker:
    //
    // ```text
    // ok  <section gate>            (workspace_of said yes)
    // ok  <written-count gate>      (workspace_of said yes)
    // ok  465 citation(s) resolve   (workspace_root said no, and accepted two
    //                                planted dangling paths in silence)
    // ```
    //
    // Two facts about one run, no warning anywhere. `NOCTUA.md §3.6`'s guard is
    // the marker — a document beside a directory is a shape any copy or
    // scratch tree can have. The marker is deliberate and cannot happen by
    // accident.
    //
    // **And it has to be *this* repository's workspace.** The walk climbs until
    // it finds a marker or reaches `/`, so a marker anywhere above a checkout —
    // `/tmp`, `/`, a developer's home — attaches the run to a workspace that
    // knows nothing about it, and the verdict changes with no warning. What
    // makes a workspace this repository's is the layout `§3.5` requires: the
    // repository sits inside that workspace's `repos/`. A marker further up
    // that does not have this tree under it is somebody else's, and the honest
    // answer is the same as no workspace at all.
    //
    // **And which of the two it is, kept rather than discarded.** The rejection
    // above is an answer of its own rather than a second spelling of "no
    // marker anywhere".
    // `Above`'s own documentation carries the measurement.
    //
    // A path that will not canonicalise answers `NoMarker`, because a
    // directory that is not there has nothing above it either.
    let Ok(start) = std::fs::canonicalize(path) else {
        return Above::NoMarker;
    };
    let mut here = start.clone();
    loop {
        if here.join(MARKER).is_file() {
            return if start.starts_with(here.join("repos")) {
                Above::Ours(here)
            } else {
                Above::SomebodyElses(here)
            };
        }
        if !here.pop() {
            return Above::NoMarker;
        }
    }
}

/// What names a workspace, and the only thing that does.
///
/// `NOCTUA.md §3.6`'s workspace-detection guard is this file, so a gate that
/// asked a different question could answer differently from the guard every
/// agent runs.
pub(crate) const MARKER: &str = ".noctua-workspace";

/// Every numbered heading in one document.
fn headings(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut fenced: Option<(char, usize, usize)> = None;
    for line in text.lines() {
        // **A fence first**, which `numbered_items_under` has too — one rule
        // written twice, and the two copies have to agree. Without it, a document
        // that *shows* what a heading looks like, which is how a convention gets
        // documented, mints the section numbers in its own example: measured
        // with a two-line heading example — one `##` and one `###`, both
        // carrying a section number no document has — inside a `text` fence at
        // the end of `NOCTUA.md`, and one citation of the invented subsection
        // written into a README. The run answers
        // `ok … section citation(s) name a real section` and `check passed`.
        // Delete only the two fenced lines and the citation is reported at once.
        // `sections.rs`'s own module header is a table of the heading shapes
        // these documents use, so this is one fence away from being written.
        if the_fence_state(line, &mut fenced) {
            continue;
        }
        if fenced.is_some() {
            continue;
        }
        let Some(rest) = line
            .strip_prefix("## ")
            .or_else(|| line.strip_prefix("### "))
        else {
            continue;
        };
        let rest = rest.strip_prefix('#').unwrap_or(rest).trim_start();
        let token = rest.split_whitespace().next().unwrap_or_default();
        // `## §10 · …`, `### 10.2 …`, `### P0.5 …`, and the reference's `## 4. …`.
        let token = token.strip_prefix('§').unwrap_or(token);
        let token = token.trim_end_matches('.');
        if token.is_empty() {
            continue;
        }
        if token
            .chars()
            .all(|character| character.is_ascii_digit() || character == '.' || character == 'P')
        {
            found.push(token.to_owned());
            // **A numbered item inside a section is a section a citation can
            // name.** These documents cite `§14.9.7` for the seventh invariant
            // of `### 14.9`; a `headings` that cannot represent that leaves every
            // such citation across the three workspace documents naming nothing,
            // and the repository's own files would have to write "§14.9's
            // seventh invariant" in prose to get past the gate. The items are
            // right there under the heading; counting them is what the reader
            // does.
            // **No `.max(1)`: a heading with no list has no items.** With it,
            // a heading with no numbered list under it gets `{token}.1` anyway,
            // and the gate vouches for a section number under every such
            // heading, one that exists in no document — measured across the
            // three it reads. Measured directly too: a citation of item one
            // under a heading that lists nothing resolves silently, and so does a
            // citation under a top-level number no heading in the document
            // carries, while item nine under the same heading is reported.
            //
            // The latitude buys nothing: without it every real citation still
            // resolves, which is what a passing run says.
            found.extend(
                (1..=numbered_items_under(text, line)).map(|item| format!("{token}.{item}")),
            );
        }
    }
    found
}

/// Every `§` citation on a line, as (document, section).
///
/// A citation may name its document — ``NOCTUA-PLAN.md §P0.3`` — or not, in
/// which case it is `NOCTUA.md`'s, which is how the documents themselves are
/// written.
fn cited(line: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let bytes: Vec<char> = line.chars().collect();
    for (index, character) in bytes.iter().enumerate() {
        if *character != '§' {
            continue;
        }
        let section: String = bytes[index + 1..]
            .iter()
            .take_while(|next| next.is_ascii_digit() || **next == '.' || **next == 'P')
            .collect();
        let section = section.trim_end_matches('.').to_owned();
        if section.is_empty() {
            continue;
        }
        // The document is whichever name appears closest before the `§` on this
        // line. Without that, `NOCTUA-PLAN.md §P0.3` would be looked up in
        // `NOCTUA.md`, and every plan citation would be a finding.
        let before: String = bytes[..index].iter().collect();
        let document = DOCUMENTS
            .iter()
            .filter_map(|name| before.rfind(name).map(|at| (at, *name)))
            .max_by_key(|(at, _)| *at)
            .map_or(DEFAULT, |(_, name)| name);
        found.push((document.to_owned(), section));
    }
    found
}

#[cfg(test)]
mod tests {
    /// The short-walk floor reports a walk that lost a citation.
    ///
    /// **Decided here.** `if judged == expected` widened to `if true` fails
    /// this test — and this is the floor whose own paragraph records a filter
    /// taking the citation count to a small fraction of itself with
    /// `check passed` printed over it. A second count that returns zero is a
    /// broken second count too, and `a_walk_that_read_half_the_lines_is_short`
    /// holds that state.
    #[test]
    fn the_short_walk_floor_reports_a_lost_citation() {
        let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        // The clean tree agrees with itself — the gate's own step reports `ok`
        // for it, so the silent direction is watched by every green run.
        let short = super::the_walk_is_short(root, 1).expect("the tree walks");
        assert!(
            !short.is_empty(),
            "the walk judged one citation where the tree holds hundreds, and the floor \
             said nothing"
        );
    }

    /// A marker above a checkout that does not hold it is somebody else's.
    ///
    /// **The walk climbs to `/`.** Accepting any `.noctua-workspace` above a
    /// checkout — `/tmp`, `/`, a developer's home — attaches the run to a
    /// workspace that knows nothing about it, and the verdict changes with no
    /// warning: the section gate and the written-count gate read that
    /// workspace's documents, and citations resolve against its files.
    ///
    /// What makes a workspace *this* repository's is the layout `§3.5`
    /// requires — the repository inside that workspace's `repos/`.
    #[test]
    fn a_workspace_that_does_not_hold_this_repository_is_not_its_workspace() {
        let base = std::env::temp_dir().join(format!("nc-ws-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        // A marker at the top, and the repository *outside* its `repos/`.
        std::fs::create_dir_all(base.join("repos/somebody-else")).expect("writable");
        std::fs::create_dir_all(base.join("elsewhere/noctua-central")).expect("writable");
        std::fs::write(base.join(super::MARKER), "").expect("writable");
        assert_eq!(
            super::workspace_of(&base.join("elsewhere/noctua-central")),
            None,
            "a checkout outside the workspace's `repos/` was attached to it anyway"
        );

        // And the layout `§3.5` actually asks for.
        let inside = base.join("repos/noctua-central");
        std::fs::create_dir_all(&inside).expect("writable");
        assert_eq!(
            super::workspace_of(&inside),
            Some(std::fs::canonicalize(&base).expect("readable")),
            "the workspace this repository is in was not found"
        );

        // **Not "and the real tree is in a workspace".** It is here and it is not
        // in a checkout, which is the whole reason the four workspace-reading
        // steps print `skipped:` in CI — an assertion either way would fail in
        // one of the two environments this repository runs in. What is asserted
        // instead is that when a workspace *is* found, this tree is inside its
        // `repos/`, which is the rule and holds in both.
        if let Some(found) = super::workspace_of(root()) {
            assert!(
                std::fs::canonicalize(root())
                    .expect("readable")
                    .starts_with(found.join("repos")),
                "the walk returned a workspace this repository is not inside"
            );
        }

        let _ = std::fs::remove_dir_all(&base);
    }

    /// A misplaced marker does not read as no marker at all.
    ///
    /// **The reproduction is one file.** Take a workspace laid out as `§3.1`
    /// describes — the marker at its root, this repository at
    /// `<workspace>/repos/noctua-central` — and copy the marker into
    /// `<workspace>/repos/`. The climb stops at the copy, this repository is
    /// not under `<workspace>/repos/repos/`, and [`super::workspace_of`]
    /// answers `None`, which is correct and is not what this test is about.
    ///
    /// What it is about is the sentence four `check` steps would print if that
    /// `None` were the whole answer: *"there is no workspace above this
    /// repository (§3.5)"*. There is one, it is two directories up, and one
    /// `mv` repairs it — while the message describes a
    /// bare checkout, which is the state nobody is expected to fix. The probe
    /// in every `AGENTS.md` answers `inside-workspace:…` for the same tree and
    /// calls it "not an alarm, continue", so a reader consulting both guards is
    /// told two different things.
    ///
    /// Both shapes are built here rather than asserted about in prose, and what
    /// is asserted is that the two reasons are different sentences and that
    /// each one names its own case.
    #[test]
    fn a_misplaced_marker_does_not_read_as_no_marker_at_all() {
        let base = std::env::temp_dir().join(format!("nc-above-marker-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);

        // A checkout with nothing above it, which is what CI always has.
        let alone = base.join("alone/noctua-central");
        std::fs::create_dir_all(&alone).expect("writable");
        assert_eq!(super::what_is_above(&alone), super::Above::NoMarker);

        // And the layout `§3.1` describes.
        let workspace = base.join("workspace");
        let inside = workspace.join("repos/noctua-central");
        std::fs::create_dir_all(&inside).expect("writable");
        std::fs::write(workspace.join(super::MARKER), "").expect("writable");
        let real = std::fs::canonicalize(&workspace).expect("readable");
        assert_eq!(
            super::what_is_above(&inside),
            super::Above::Ours(real),
            "the layout `§3.5` requires was not read as this repository's workspace"
        );
        assert_eq!(
            super::out_of_reach(&super::what_is_above(&inside)),
            None,
            "a step would have announced a skip with the workspace in place"
        );

        // The one file.
        let repos = workspace.join("repos");
        std::fs::write(repos.join(super::MARKER), "").expect("writable");
        let misplaced = super::what_is_above(&inside);
        assert_eq!(
            misplaced,
            super::Above::SomebodyElses(std::fs::canonicalize(&repos).expect("readable")),
            "the marker that was found and rejected was not reported as one"
        );
        // **And the answer itself is `None`.** A marker that does not have
        // this repository under its `repos/` is not this repository's
        // workspace, and every caller that asks only that gets `None`.
        assert_eq!(
            super::workspace_of(&inside),
            None,
            "a marker this repository is not under was accepted as its workspace"
        );

        let nothing = super::out_of_reach(&super::what_is_above(&alone))
            .expect("a checkout with nothing above it says why");
        let wrong_place = super::out_of_reach(&misplaced).expect("a misplaced marker says why");
        assert_ne!(
            nothing, wrong_place,
            "the two shapes give one sentence, which is the whole defect: a reader \
             cannot tell a bare checkout from a workspace with the marker in the \
             wrong place"
        );
        assert!(
            nothing.contains(super::MARKER) && !nothing.contains("repos"),
            "the bare checkout's reason does not say that no marker was found: {nothing}"
        );
        let named = std::fs::canonicalize(&repos)
            .expect("readable")
            .display()
            .to_string();
        assert!(
            wrong_place.contains(&named),
            "the misplaced marker's reason does not say where it is, which is the one \
             thing a reader needs to move it: {wrong_place}"
        );
        assert!(
            wrong_place.contains("repos/") && !wrong_place.contains("no `"),
            "the misplaced marker's reason still reads as a checkout with nothing \
             above it: {wrong_place}"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// A workspace is named by its marker, and by nothing else.
    ///
    /// **Two detectors would disagree.** One looking for `NOCTUA.md` beside a `repos/`
    /// and `references.rs` looking for the marker, measured on a tree with the
    /// documents and `repos/` present and no marker — the shape a copy or a
    /// scratch directory can have:
    ///
    /// ```text
    /// ok  <section gate>            (the first said yes)
    /// ok  465 citation(s) resolve   (the other said no, and accepted two
    ///                                planted dangling paths in silence)
    /// ```
    ///
    /// Two facts about one run, no warning anywhere. `NOCTUA.md §3.6`'s guard
    /// is the marker: a document beside a directory is a shape any copy can
    /// have, and the marker is deliberate.
    #[test]
    fn only_the_marker_names_a_workspace() {
        let scratch = std::env::temp_dir().join(format!("nc-marker-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        let inside = scratch.join("repos/probe/crates/one");
        std::fs::create_dir_all(&inside).expect("writable");

        // The shape that fools a second detector: the documents and `repos/`,
        // no marker.
        for document in super::DOCUMENTS {
            let at = scratch.join(document);
            if let Some(parent) = at.parent() {
                std::fs::create_dir_all(parent).expect("writable");
            }
            std::fs::write(at, "# nothing\n").expect("writable");
        }
        assert_eq!(
            super::workspace_of(&inside),
            None,
            "a tree shaped like a workspace was read as one, with no marker in it"
        );

        // And with the marker, from a directory well below it.
        std::fs::write(scratch.join(super::MARKER), "").expect("writable");
        let found = super::workspace_of(&inside).expect("the marker names the workspace");
        assert_eq!(
            std::fs::canonicalize(&found).expect("exists"),
            std::fs::canonicalize(&scratch).expect("exists")
        );
        let _ = std::fs::remove_dir_all(&scratch);
    }

    use super::{
        DEFAULT, NOT_READ, READ, READ_BY_NAME, cited, every_section_cited_exists, headings,
    };
    use std::path::Path;

    fn root() -> &'static Path {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
    }

    #[test]
    fn this_repository_cites_no_section_that_is_missing() {
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
        let (findings, read) = every_section_cited_exists(root()).expect("the documents are there");
        // **Except the canary**, which is the one file this gate is required to
        // report — `check` fails when it is *absent*, because a filter that hides
        // lines hides the canary too, and that is what a count cannot say.
        let genuine: Vec<&crate::gates::Finding> = findings
            .iter()
            .filter(|finding| !finding.file.ends_with("canaries/citations.md"))
            .collect();
        assert!(genuine.is_empty(), "{genuine:#?}");
        assert!(
            findings.len() > genuine.len(),
            "the canary was not reported, so the walk is not reaching it"
        );
        // The floor: a gate that read no citation would report a clean tree.
        assert!(read > 50, "only {read} section citation(s) were read");
    }

    #[test]
    fn a_heading_is_recognised_in_every_shape_the_documents_use() {
        let text = "## §10 · Repository\n### 10.2 Monorepo layout [DERIVED]\n\
                    ### P0.5 Progress marker\n## 4. Cedar schema\n## Not numbered\n";
        // **Each heading, and only the items it actually lists.** Not the
        // phantoms — `10.1`, `10.2.1`, `P0.5.1`, `4.1` for headings that list
        // nothing — on the argument that item one is how these documents cite
        // a section's first point. They do not: measured, `.max(1)` credits
        // every heading with an item it has not got, and the gate vouches for a
        // section number under every heading that lists nothing, while without
        // it every real citation still resolves, so the latitude holds no real
        // citation up. Among the phantoms is item one under a top-level number
        // that has no heading in the document at all.
        assert_eq!(headings(text), vec!["10", "10.2", "P0.5", "4"]);
    }

    /// A heading inside a fenced block is an example, not a section.
    ///
    /// **One rule written twice, and both copies carry it.**
    /// `numbered_items_under` carries a fence toggle — *"a `## ` inside one is
    /// example markdown"* — and so does `headings`. Without it, a document that
    /// *shows* what a heading looks like, which is how a convention gets
    /// documented, declares those section numbers and every citation of them
    /// resolves. Measured with a two-line heading example
    /// inside a `text` fence at the end of `NOCTUA.md` and one citation of the
    /// invented subsection written into a README:
    /// `ok … section citation(s) name a real section`, `check passed`.
    ///
    /// **And this test is mutation coverage for the toggle.** `sections.rs` is
    /// judged by `corpus:: limits:: sections::`, and this test reads a fenced
    /// heading, so the runner's operator over the toggle fails it. A rule the
    /// meta-gate cannot weaken is a rule with no reported coverage, which is
    /// the hole `xtask/mutants.py` exists to report.
    #[test]
    fn a_heading_inside_a_fence_is_an_example_and_not_a_section() {
        let text = "## §10 · A real section\n\n\
                    ```text\n\
                    ## §99 · The shape a heading takes\n\
                    ### 99.4 The shape a subsection takes\n\
                    ```\n\n\
                    ### 10.2 Another real one\n";
        let found = headings(text);
        assert!(
            found.contains(&"10".to_owned()) && found.contains(&"10.2".to_owned()),
            "the real headings were not read: {found:?}"
        );
        for shown in ["99", "99.4"] {
            assert!(
                !found.contains(&shown.to_owned()),
                "`{shown}` is inside a fence and was minted as a section, so every citation \
                 of it resolves against an example: {found:?}"
            );
        }
    }

    #[test]
    fn a_numbered_item_inside_a_section_is_a_section_a_citation_can_name() {
        // These documents cite `§14.9.7` for the seventh invariant of
        // `### 14.9`; a `headings` that cannot represent it leaves every such
        // citation across the three workspace documents naming nothing, and the
        // repository's own files would have to write "§14.9's seventh invariant"
        // in prose to get past the gate.
        let text = "### 14.9 The core invariants\n\n\
                    1. Deny by default.\n\
                    2. No permission without provenance.\n\
                    3. No manual grant without an expiry.\n\n\
                    ### 15.1 Errors\n\n1. One.\n";
        let found = headings(text);
        for named in ["14.9", "14.9.1", "14.9.3", "15.1", "15.1.1"] {
            assert!(found.contains(&named.to_owned()), "{named} in {found:?}");
        }
        // And not an item the section does not have: the count is the highest
        // number the list reaches, not an open range.
        assert!(!found.contains(&"14.9.4".to_owned()), "{found:?}");
        // The next section's items are its own, not the previous one's.
        assert!(!found.contains(&"15.1.2".to_owned()), "{found:?}");
    }

    /// A fence holds example markdown, and the item reader steps over it.
    ///
    /// **Decided here.** The rule's own comment says why it exists — a heading
    /// inside a fence would end the scan early, and a number inside one would
    /// be counted as an item. Turning the fence test off fails this test.
    ///
    /// Both directions here, because the one rule is wrong in two ways when it
    /// goes: the section loses its items, and it gains items nobody wrote.
    #[test]
    fn a_fence_holds_example_markdown_and_not_this_section() {
        // A heading inside a fence is not the end of this section: without the
        // rule the scan breaks there and the section has no items at all.
        let broken = "## 7 Naming\n\n\
                      FENCEtext\n\
                      ## 8 an example heading, inside a fence\n\
                      FENCE\n\n\
                      1. One.\n2. Two.\n3. Three.\n"
            .replace("FENCE", "```");
        assert_eq!(
            super::numbered_items_under(&broken, "## 7 Naming"),
            3,
            "a heading inside a fence is example markdown, and the items below it are this \
             section's"
        );

        // And a number inside a fence is example output, not an item: without
        // the rule the section gains a ninth item nobody wrote, and a citation
        // of it resolves against nothing.
        let invented = "## 7 Naming\n\n\
                        FENCEtext\n\
                        9. this line is example output\n\
                        FENCE\n\n\
                        1. One.\n2. Two.\n"
            .replace("FENCE", "```");
        assert_eq!(
            super::numbered_items_under(&invented, "## 7 Naming"),
            2,
            "a number inside a fence is example output, and vouching for it names a section \
             no document has"
        );

        // **And the heading itself is located past the fences.** Anchored with
        // `text.find(heading)`, blind to the fences the rest of this function
        // honours, a document that *shows* the heading inside an example above
        // the real one begins its scan inside the code block, takes the
        // example's numbers for the section's items, and then reads the block's
        // closing fence as an opening one and skips the real list.
        let shown_above = "# A document\n\n\
                           FENCEtext\n\
                           ## 7 Naming\n\
                           1. an example item\n\
                           2. another\n\
                           3. a third\n\
                           4. a fourth\n\
                           FENCE\n\n\
                           ## 7 Naming\n\n\
                           1. One.\n2. Two.\n"
            .replace("FENCE", "```");
        assert_eq!(
            super::numbered_items_under(&shown_above, "## 7 Naming"),
            2,
            "the scan began at the heading shown inside a fence, so a number in an example \
             became an item of the real section"
        );

        // The mirror, which `text.rfind` would get wrong: the same example
        // shown *below* the real heading must not move the anchor past it.
        let shown_below = "## 7 Naming\n\n\
                           1. One.\n2. Two.\n\n\
                           FENCEtext\n\
                           ## 7 Naming\n\
                           1. an example item\n\
                           FENCE\n"
            .replace("FENCE", "```");
        assert_eq!(
            super::numbered_items_under(&shown_below, "## 7 Naming"),
            2,
            "the scan began at the heading shown inside a fence below the real one"
        );
    }

    #[test]
    fn a_citation_takes_the_document_named_closest_before_it() {
        assert_eq!(
            cited("see NOCTUA-PLAN.md \u{a7}P0.3 for the order"),
            vec![("NOCTUA-PLAN.md".to_owned(), "P0.3".to_owned())]
        );
        // A bare one is NOCTUA.md's, which is how the documents write them.
        assert_eq!(
            cited("the rule is \u{a7}14.9"),
            vec![("NOCTUA.md".to_owned(), "14.9".to_owned())]
        );
        // And the trailing sentence punctuation is not part of the number.
        assert_eq!(
            cited("as \u{a7}15.8. And also \u{a7}16.4, which"),
            vec![
                ("NOCTUA.md".to_owned(), "15.8".to_owned()),
                ("NOCTUA.md".to_owned(), "16.4".to_owned()),
            ]
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
        // **Not a third copy of the list it is checking.** A guard that walks
        // the tree into a set and then iterates the same seven strings,
        // skipping any the tree does not hold, can catch a kind *removed* from
        // the claim list and never a kind the tree *gains*. Measured with such a
        // guard: three planted miscitations in files of a kind outside the seven
        // leave the transcript byte-identical to a clean run.
        //
        // So the expectation is the tree, in both directions: every kind on
        // disk is read or is written down with a reason, and every name in
        // either list is a kind this repository actually holds.
        let present = the_kinds_on_disk();
        for kind in &present {
            let read = READ.contains(&kind.as_str()) || READ_BY_NAME.contains(&kind.as_str());
            let written_off = NOT_READ.iter().any(|(named, _)| named == kind);
            assert!(
                read != written_off,
                "this repository holds `{kind}` files and the gate neither reads them nor \
                 says why it does not — add the kind to READ, or a row to NOT_READ with \
                 the reason a citation cannot be written there"
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
    fn nothing_the_gate_leaves_unread_carries_a_section_citation() {
        // **What holds `NOT_READ` to the tree.** Without this, a kind could be
        // moved out of `READ` into `NOT_READ` with a sentence beside it and the
        // partition above would still pass — the ratchet paid rather than read.
        // A `§` is unambiguous, so the claim "no citation can be written there"
        // is checkable rather than a promise: move `md` across and this fails
        // with the first file it finds.
        let mut looked = 0_usize;
        let mut unread = Vec::new();
        crate::gates::walk(root(), &mut |path| {
            let Some(kind) = super::kind_of(path) else {
                return;
            };
            if !NOT_READ.iter().any(|(named, _)| *named == kind) {
                return;
            }
            looked += 1;
            if let Ok(text) = std::fs::read_to_string(path)
                && text.contains('\u{a7}')
            {
                unread.push(path.display().to_string());
            }
        })
        .expect("the tree is readable");
        assert!(
            unread.is_empty(),
            "these files carry a `\u{a7}` and their kind is written off as unread: {unread:#?}"
        );
        assert!(
            looked >= NOT_READ.len(),
            "only {looked} file(s) of an unread kind were opened, and there are \
             {} kinds written off",
            NOT_READ.len()
        );
    }

    #[test]
    fn dropping_a_kind_from_read_is_caught_rather_than_quietly_shrinking_the_gate() {
        // The hole, tested rather than the rule — the shape `references.rs`
        // has too. Dropping `"md"` here, which takes every Markdown document's
        // citations out of the run, fails this test.
        //
        // **What names the kinds is `READ` itself.** A claim list beside it
        // would be a second copy of the same strings, and a guard anchoring it
        // a third; a kind the tree gains would be named by none of them.
        // `every_kind_on_disk_is_either_read_or_written_down_as_unread`
        // holds `READ` to the tree, so what this test holds is the other
        // half: that the walk really opens one of each.

        // And at the walk level, because a string check alone would pass a `READ`
        // that held the right names and a walk that never used it.
        let scratch = std::env::temp_dir().join(format!("nc-sections-read-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        let repository = scratch.join("repos").join("noctua-central");
        std::fs::create_dir_all(&repository).expect("writable");
        std::fs::write(scratch.join(super::MARKER), "").expect("writable");
        std::fs::write(scratch.join(DEFAULT), "### 14.9 The core invariants\n").expect("writable");
        std::fs::write(scratch.join("NOCTUA-PLAN.md"), "### P0.5 Progress\n").expect("writable");
        std::fs::create_dir_all(scratch.join("reference")).expect("writable");
        std::fs::write(
            scratch.join("reference/identity-and-authorization.md"),
            "## 9. Core invariants\n",
        )
        .expect("writable");

        // Built rather than written as a literal: this file is walked too.
        let citation = "// the rule is NOCTUA.md \u{a7}99.9\n";
        for kind in READ {
            std::fs::write(repository.join(format!("a.{kind}")), citation).expect("writable");
        }
        // **And the two with no extension**, which is why they are a list of
        // names rather than more entries in `READ`: a file called `justfile`
        // cannot be reached by an extension list, however wide it grows.
        for named in READ_BY_NAME {
            std::fs::write(repository.join(named), citation).expect("writable");
        }
        let (findings, read) = every_section_cited_exists(&repository).expect("readable");
        let expected = READ.len() + READ_BY_NAME.len();
        assert_eq!(read, expected, "one kind of file was not read at all");
        assert_eq!(findings.len(), expected, "{findings:#?}");

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// A heading shown inside a tilde fence mints no section, and neither does a
    /// citation of it resolve.
    ///
    /// **The toggle knows both of `CommonMark`'s fence characters.** The
    /// paragraph above the first toggle records the case it is for: a document
    /// that *shows* what a heading looks like mints section numbers in its own
    /// example, and a citation of the invented subsection then resolves with
    /// `check passed`. Closing the backtick spelling only, the identical example
    /// in a `~~~` block mints the invented section again — and nothing
    /// downstream can see it, because a citation that resolves is still read
    /// and still counted.
    #[test]
    fn a_heading_inside_a_tilde_fence_mints_no_section() {
        let scratch = std::env::temp_dir().join(format!("nc-tilde-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        let repository = scratch.join("repos").join("noctua-central");
        std::fs::create_dir_all(&repository).expect("writable");
        std::fs::write(scratch.join(super::MARKER), "").expect("writable");
        // Built rather than written, because this file is walked by the gate.
        let mark = "\u{a7}";
        std::fs::write(
            scratch.join("NOCTUA.md"),
            format!(
                "## {mark}14 \u{b7} Authorization\n### 14.9 The core invariants\n\n\
                 A heading is written like this:\n\n~~~text\n### 77.7 An invented \
                 subsection\n~~~\n"
            ),
        )
        .expect("writable");
        std::fs::write(scratch.join("NOCTUA-PLAN.md"), "### P0.5 Progress marker\n")
            .expect("writable");
        std::fs::create_dir_all(scratch.join("reference")).expect("writable");
        std::fs::write(
            scratch.join("reference/identity-and-authorization.md"),
            "## 9. Core invariants\n",
        )
        .expect("writable");
        std::fs::write(
            repository.join("a.rs"),
            format!("// the rule is NOCTUA.md {mark}77.7\n"),
        )
        .expect("writable");
        let (findings, _) = every_section_cited_exists(&repository).expect("readable");
        assert!(
            findings.iter().any(|finding| finding.what.contains("77.7")),
            "a section shown inside a `~~~` example was minted as real, and a citation \
             of it resolved in silence: {findings:#?}"
        );
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// A fence is closed by what opened it, and by at least as much of it.
    ///
    /// **Both characters, and not a toggle.** A toggle flips on a `~~~` inside a
    /// backtick block twice, so the example heading after it is minted and the
    /// real heading below the block is then read as fenced and lost — both
    /// halves visible in one list. The two obvious alternatives are each measured
    /// against pandoc and each is wrong: the same *length* refuses four
    /// backticks closing a three-backtick block, and the character alone
    /// accepts three inside four.
    #[test]
    fn a_fence_is_closed_by_the_character_that_opened_it() {
        let mark = "\u{a7}";
        let dot = "\u{b7}";
        // Built from lines rather than one literal: a line continuation in a
        // string is collapsed by rustfmt into a run of spaces, and an indented
        // `### ` is not a heading to the reader under test.
        for (what, lines, wanted, unwanted) in the_fences_this_reader_has_been_beaten_by(mark, dot)
        {
            let found = headings(&(lines.join("\n") + "\n"));
            assert!(
                found.iter().any(|number| number == wanted),
                "{what}: the real heading `{wanted}` below the block was not read — the \
                 walk stayed fenced: {found:?}"
            );
            assert!(
                !found.iter().any(|number| number == unwanted),
                "{what}: the heading shown inside the block was minted as a section, \
                 which is a gate satisfied by a string literal: {found:?}"
            );
        }
    }

    /// The fenced documents a fence reader gets wrong most easily.
    ///
    /// Its own function because the table with its indentation rows crosses
    /// clippy's hundred-line bound. The last two are the
    /// shapes that make an *absolute* indentation rule wrong, both measured
    /// against pandoc.
    /// <!-- allow-logging: `the_fences_this_reader_has_been_beaten_by` hands back fixture documents written as literals in this file, and nothing a caller chose -->
    fn the_fences_this_reader_has_been_beaten_by(
        mark: &str,
        dot: &str,
    ) -> Vec<(&'static str, Vec<String>, &'static str, &'static str)> {
        let shown = |open: &str, inside: &str, close: &str| {
            a_heading_shown_inside(mark, dot, open, inside, close)
        };
        Vec::from([
            (
                "a tab-indented fence is content, because a tab is four columns",
                shown("```text", "\t```", "```"),
                "10.2",
                "99",
            ),
            (
                "a fence led by a non-breaking space is not a fence at all",
                shown("```text", "\u{a0}```", "```"),
                "10.2",
                "99",
            ),
            // **The length half of clause 4.5, asked by this row.** A
            // two-backtick run is a code span — the `CommonMark` spelling used
            // when the code itself holds a backtick — and reading it as a fence
            // opens a block nothing closes, so every heading below it is lost.
            // It cannot be written through `shown`: inside an open ``` block the
            // `long >= run` rule refuses a shorter closer anyway, so the row has
            // to be the one where the two-run would be the *opening* fence.
            (
                "a two-character run is a code span, not a fence that opens",
                vec![
                    format!("## {mark}10 {dot} A real section"),
                    String::new(),
                    "``a ` b`` is a code span".to_owned(),
                    String::new(),
                    format!("## {mark}99 {dot} The shape a heading takes"),
                    String::new(),
                    "### 10.2 Another real one".to_owned(),
                ],
                "10.2",
                "nothing-is-invented-here",
            ),
            (
                "a tilde line inside a backtick block is content",
                shown("```text", "~~~", "```"),
                "10.2",
                "99",
            ),
            (
                "a longer fence closes a shorter one",
                shown("```text", "", "````"),
                "10.2",
                "99",
            ),
            // **The indentation three.** `CommonMark` bounds a closing fence's
            // indentation to three past the opening one; at four it is content.
            // The last two are the shapes that make the *absolute* rule wrong —
            // a fence legitimately opened at column four inside a list item,
            // which `AGENTS.md` ships and pandoc renders as a fenced block.
            (
                "a fence indented four spaces is content, not a closing fence",
                shown("```text", "    ```", "```"),
                "10.2",
                "99",
            ),
            (
                "a fence indented three spaces still closes",
                shown("```text", "", "   ```"),
                "10.2",
                "99",
            ),
            (
                "a block opened inside a list item closes at its own indentation",
                vec![
                    format!("## {mark}10 {dot} A real section"),
                    String::new(),
                    "1.  A step:".to_owned(),
                    String::new(),
                    "    ```toml".to_owned(),
                    format!("    ## {mark}99 {dot} The shape a heading takes"),
                    "    ```".to_owned(),
                    String::new(),
                    "### 10.2 Another real one".to_owned(),
                ],
                "10.2",
                "99",
            ),
            (
                "a shorter fence does not close a longer one",
                shown("````text", "```", "````"),
                "10.2",
                "99",
            ),
        ])
    }

    /// The document every fence row but one is.
    ///
    /// A real heading, a fenced example that *shows* what a heading looks like,
    /// and a real heading after the block — so each row differs only in the
    /// fence lines, which is what each of them is about. `inside` is the line
    /// between the opening fence and the example heading, and is empty where the
    /// row's subject is the closing fence rather than a line inside the block.
    ///
    /// Written once, because with a copy per row one fence rule can be tested
    /// against a narrower document than the other two.
    fn a_heading_shown_inside(
        mark: &str,
        dot: &str,
        open: &str,
        inside: &str,
        close: &str,
    ) -> Vec<String> {
        let mut document = vec![
            format!("## {mark}10 {dot} A real section"),
            String::new(),
            open.to_owned(),
        ];
        if !inside.is_empty() {
            document.push(inside.to_owned());
        }
        document.extend([
            format!("## {mark}99 {dot} The shape a heading takes"),
            close.to_owned(),
            String::new(),
            "### 10.2 Another real one".to_owned(),
        ]);
        document
    }

    #[test]
    fn a_section_that_does_not_exist_is_a_finding() {
        // Fired on purpose, in a scratch tree, because a gate nobody has watched
        // fire is a gate nobody knows is wired up. The shape is a section no
        // document has; a citation of a section that exists for a rule that
        // lives elsewhere — §15.4 for a rule in §14.9 — only a reader finds.
        let scratch = std::env::temp_dir().join(format!("nc-sections-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        let repository = scratch.join("repos").join("noctua-central");
        std::fs::create_dir_all(&repository).expect("writable");
        std::fs::write(scratch.join(super::MARKER), "").expect("writable");
        std::fs::write(
            scratch.join("NOCTUA.md"),
            "## §14 · Authorization\n### 14.9 The core invariants\n",
        )
        .expect("writable");
        std::fs::write(scratch.join("NOCTUA-PLAN.md"), "### P0.5 Progress marker\n")
            .expect("writable");
        std::fs::create_dir_all(scratch.join("reference")).expect("writable");
        std::fs::write(
            scratch.join("reference/identity-and-authorization.md"),
            "## 9. Core invariants\n",
        )
        .expect("writable");

        // Built rather than written as literals: this file is walked by the gate
        // too, so a `§` in a fixture string is a citation the gate reports.
        let absent = "// the rule is NOCTUA.md \u{a7}99.9\n";
        std::fs::write(repository.join("a.rs"), absent).expect("writable");
        let (findings, read) = every_section_cited_exists(&repository).expect("readable");
        // Two: the planted line, and the scratch `NOCTUA.md`'s own
        // `## §14 · Authorization`, which is a heading naming its own section and
        // is judged like any other line: there is no heading exemption.
        assert_eq!(read, 2);
        assert_eq!(findings.len(), 1, "{findings:#?}");
        assert!(findings[0].what.contains("99.9"), "{}", findings[0].what);

        // And one that exists is not a finding, or the assertion above would pass
        // against a gate that reported everything.
        let present = "// the rule is NOCTUA.md \u{a7}14.9\n";
        std::fs::write(repository.join("a.rs"), present).expect("writable");
        let (findings, _) = every_section_cited_exists(&repository).expect("readable");
        assert!(findings.is_empty(), "{findings:#?}");

        // A section of another document, looked up in that document.
        let both = "// the order is NOCTUA-PLAN.md \u{a7}P0.5, and the model is \
                    reference/identity-and-authorization.md \u{a7}9\n";
        std::fs::write(repository.join("a.rs"), both).expect("writable");
        let (findings, read) = every_section_cited_exists(&repository).expect("readable");
        assert!(findings.is_empty(), "{findings:#?}");
        // Plus the scratch `NOCTUA.md`'s own heading, judged like any other
        // line.
        assert_eq!(read, 3);

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// Excusing a citation does not lower the number that proves coverage.
    ///
    /// **Counted before it is excused, as its twin is.** `read` is what the
    /// step prints as evidence it inspected anything, and with an excused
    /// citation subtracted from it, one marker on a line carrying a real
    /// miscitation takes the run's `section citation(s)` count down by one, with
    /// the hatch census going *up* by one at the same time and nothing relating
    /// the two. The only floor under the number is zero.
    #[test]
    fn excusing_a_citation_does_not_lower_the_number_that_proves_coverage() {
        let scratch = std::env::temp_dir().join(format!("nc-sec-excused-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        let repository = scratch.join("repos").join("noctua-central");
        std::fs::create_dir_all(&repository).expect("writable");
        std::fs::write(scratch.join(super::MARKER), "").expect("writable");
        std::fs::write(scratch.join("NOCTUA.md"), "## §14 · Authorization\n").expect("writable");
        std::fs::write(scratch.join("NOCTUA-PLAN.md"), "### P0.5 Progress marker\n")
            .expect("writable");
        std::fs::create_dir_all(scratch.join("reference")).expect("writable");
        std::fs::write(
            scratch.join("reference/identity-and-authorization.md"),
            "## 9. Core invariants\n",
        )
        .expect("writable");

        let cited = "// the rule is NOCTUA.md \u{a7}99.9\n";
        std::fs::write(repository.join("a.rs"), cited).expect("writable");
        let (findings, read) = every_section_cited_exists(&repository).expect("readable");
        assert_eq!(findings.len(), 1, "the miscitation was not reported");
        // The planted line plus the fixture's own heading — see above.
        assert_eq!(read, 2);

        // The same line, silenced. The finding goes; the coverage number must
        // not, or using the hatch is invisible in the one number that says the
        // gate looked at anything.
        std::fs::write(
            repository.join("a.rs"),
            format!(
                "// the rule is NOCTUA.md \u{a7}99.9 // {} \u{a7}99.9 is being renumbered\n",
                crate::references::ALLOW
            ),
        )
        .expect("writable");
        let (findings, excused_read) = every_section_cited_exists(&repository).expect("readable");
        assert!(findings.is_empty(), "{findings:#?}");
        // **Not equal — at least.** The reason has to name the section it
        // excuses, and a section named in a reason is itself a citation on that
        // line, so the count goes *up* by one. What must never happen is that it
        // goes down, which is what this test is for: marking a line must not
        // subtract from the one number that says the gate looked at anything.
        assert!(
            excused_read >= read,
            "excusing a citation lowered the number that proves coverage: \
             {excused_read} against {read}"
        );

        // **And a reason that names a different section excuses nothing.**
        // Rewriting `names_the_section` to ask only whether a reason exists
        // fails this half, and would make every one of the tree's hatches a
        // blanket excuse for whatever else lands on its line. The test above
        // writes a reason that *does* name the section, so it passes either way
        // — the half that matters is this one.
        std::fs::write(
            repository.join("a.rs"),
            format!(
                "// the rule is NOCTUA.md \u{a7}99.9 // {} the build directory moves\n",
                crate::references::ALLOW
            ),
        )
        .expect("writable");
        let (findings, _) = every_section_cited_exists(&repository).expect("readable");
        assert_eq!(
            findings.len(),
            1,
            "a reason about something else excused the miscitation: {findings:#?}"
        );

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// A `#` at column zero hides a citation in one kind of file and no other.
    ///
    /// **The exclusion tested as a hole.** "A heading is not a citation of
    /// itself", a rule for the workspace documents, applied to every file
    /// `READ` names skips three kinds that spell a comment with `#`. That is
    /// where the catalog and the CI workflow keep their `§` numbers, nearly
    /// every one of them behind a column-zero `#`. Such a rule is green over
    /// all of them, and the count it prints reads exactly like a run that
    /// judged them.
    #[test]
    fn a_hash_hides_a_citation_in_markdown_and_nowhere_else() {
        let scratch = std::env::temp_dir().join(format!("nc-hash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        let repository = scratch.join("repos").join("noctua-central");
        std::fs::create_dir_all(&repository).expect("writable");
        std::fs::write(scratch.join(super::MARKER), "").expect("writable");
        std::fs::write(scratch.join("NOCTUA.md"), "## §14 · Authorization\n").expect("writable");
        std::fs::write(scratch.join("NOCTUA-PLAN.md"), "### P0.5 Progress marker\n")
            .expect("writable");
        std::fs::create_dir_all(scratch.join("reference")).expect("writable");
        std::fs::write(
            scratch.join("reference/identity-and-authorization.md"),
            "## 9. Core invariants\n",
        )
        .expect("writable");

        // The same citation, at column zero behind a `#`, in each kind that
        // spells a comment that way — and in Markdown, where it is a heading
        // that *cites* a section rather than naming its own, and is judged like
        // any other line.
        let cited = "# the rule is NOCTUA.md \u{a7}99.9\n";
        for (name, judged) in [
            ("a.toml", true),
            ("a.yml", true),
            ("a.md", true),
            ("a.rs", true),
        ] {
            std::fs::write(repository.join(name), cited).expect("writable");
            let (findings, read) = every_section_cited_exists(&repository).expect("readable");
            // **One from the fixture itself**, because there is no heading-skip
            // exemption: `## §14 · Authorization` in the scratch `NOCTUA.md` is a
            // heading that names its own section, and that citation resolves like
            // any other. Such an exemption protects nothing — without it every
            // heading's citation in the real tree is judged, and a passing run
            // says none of them is wrong — while putting every `§` at the start
            // of a `#` line outside the gate.
            assert_eq!(
                read,
                1 + usize::from(judged),
                "`{name}` read {read} citation(s) from a column-zero `#` line"
            );
            assert_eq!(
                findings.len(),
                usize::from(judged),
                "`{name}`: {findings:#?}"
            );
            std::fs::remove_file(repository.join(name)).expect("writable");
        }

        // And a Markdown heading that names its **own** number is judged like
        // any other line, and so is the same citation one line down, outside
        // the heading. A rule skipping every `#` line in every Markdown file
        // leaves `## The security answer §2.4 asks for` unjudged as well, and
        // every `§` in a `#` shell comment inside a fenced block.
        std::fs::write(
            repository.join("a.md"),
            "## \u{a7}99.9 · A heading naming itself\nthe rule is NOCTUA.md \u{a7}99.9\n",
        )
        .expect("writable");
        let (findings, read) = every_section_cited_exists(&repository).expect("readable");
        // **Three citations, and two of them findings.** The heading naming
        // itself is judged rather than skipped: the scratch Markdown file is not
        // one of the documents that declare sections, so a heading in it citing
        // a section names a section that does not exist and says so. That is the
        // whole point — a heading exemption makes every `§` at the start of a
        // `#` line invisible, and protects nothing: across the real tree, every
        // citation it would hide is judged and resolves.
        assert_eq!(read, 3, "{findings:#?}");
        assert_eq!(findings.len(), 2, "{findings:#?}");

        // And a heading that cites a section it is not is judged.
        std::fs::write(
            repository.join("a.md"),
            "## The security answer \u{a7}99.9 asks for\n",
        )
        .expect("writable");
        let (findings, read) = every_section_cited_exists(&repository).expect("readable");
        // Plus the scratch `NOCTUA.md`'s own heading, as above.
        assert_eq!(read, 2, "{findings:#?}");
        assert_eq!(findings.len(), 1, "{findings:#?}");

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// A walk that read a fraction of the lines is short, and an exact one is not.
    ///
    /// **The clause its own paragraph calls the whole of the floor, decided
    /// here.** Loosened to `judged * 2 >= expected`, it still passes any test
    /// that reaches it through the walk, because the walk hands it the true
    /// number. Half is what the paragraph condemns, so half is what this hands
    /// it.
    #[test]
    fn a_walk_that_read_half_the_lines_is_short() {
        let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        let expected = super::lines_read(root).expect("the tree is readable");
        assert!(
            expected > 100,
            "a second count of the files this gate reads found {expected} line(s), so \
             the comparison below is about nothing"
        );
        let seen = super::the_walk_is_short(root, expected).expect("the tree is readable");
        assert!(
            seen.is_empty(),
            "a walk that read every line was reported as short: {seen:?}"
        );

        // **A second count of nothing, beside a walk that claims something.**
        // This is the only state an `expected == 0` clause can ever decide: when
        // both counts are zero `judged == expected` already answers, so the
        // clause speaks only when the second count finds nothing while the walk
        // claims to have judged lines — which is exactly the state that says
        // the second count is broken, and answering "not short" there is wrong.
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
}
