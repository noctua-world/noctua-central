//! Writes a version everywhere it appears.
//!
//! Deliberately conservative, and for a reason that is not about taste: a
//! version is reviewed before it becomes history. It verifies, rewrites,
//! relocks, and prints the commands that commit and tag it — it never runs one
//! of them.
//!
//! It also publishes nothing, and cannot: this repository has no remote and no
//! registry name (`NOCTUA.md §5.3`). "Release" here means *the version is
//! written consistently*, which is what `§5.3` calls publishable.
//!
//! # Why the list of files is written down
//!
//! [`CARRIES_THE_VERSION`] names every file that holds one, and a file in it
//! that does **not** hold the current version is an **error** rather than
//! something to skip. The alternative — search the tree and rewrite what matches
//! — is the shape that leaves one file behind on the day somebody adds it, and a
//! release where one file disagrees is worse than a release that refused: the
//! disagreement is discovered by whoever is debugging something else.

use std::path::Path;

use crate::ui;

/// Every file that names the repository's version.
///
/// One entry today. It grows in the change that adds a file naming a version —
/// never afterwards, because "afterwards" is a release that shipped with two.
const CARRIES_THE_VERSION: [&str; 1] = ["Cargo.toml"];

/// The line in a manifest that holds it.
///
/// **Kept for the tests and the prose, not for the writer.** The writer used to
/// be `replacen` over this one spelling, and `version_of` — the reader — accepts
/// arbitrary whitespace, so a manifest cargo accepts was readable and
/// unwritable. `written_as_the_reader_reads_it` asks the reader where the line
/// is; this is the canonical form the scanner and the fixtures are written in.
#[cfg(test)]
const KEY: &str = "version = \"";

/// Writes the lockfile's pristine bytes back after a release failed part way.
///
/// **It is the only thing that puts `Cargo.lock` back**, in a repository whose
/// own header says there is no git to undo it — and it was held by a count and a
/// string literal: one test asserted how many writing calls this module ships and
/// another that the failure message contains a sentence. Neither read
/// what was written, and the test named for the restore never enters this path:
/// it drives `rewrite_listed` twice by hand against a scratch tree that has no
/// `Cargo.lock` in it at all. Measured by a reviewer in wave 16, writing
/// `&locked[..0]` here left every one of the 627 tests green, and a failed
/// `cargo update` then truncated the lockfile to nothing — after which
/// `--locked` refuses all five verbs.
///
/// A named function so the bytes can be asked about directly, which is what
/// `the_lockfile_comes_back_byte_for_byte` does.
fn put_the_lockfile_back(lockfile: &Path, locked: &[u8]) -> std::io::Result<()> {
    std::fs::write(lockfile, locked)
}

/// Validates `version`, rewrites it everywhere, relocks, and prints what a human
/// runs next.
///
/// # Errors
///
/// Returns why the version is not one, why the gate refused, why a file that
/// should name a version does not, or why the lockfile could not be rebuilt.
pub(crate) fn run(root: &Path, version: &str, dry_run: bool) -> Result<(), String> {
    validate(version)?;
    ui::heading("release");

    // **The gate first, before anything is written.** A release that rewrote a
    // version and then discovered the tests were red would leave the tree in a
    // state nobody asked for, in a repository with no git to undo it (`§5.3`).
    if dry_run {
        ui::warn("--dry-run: the gate is not run and nothing is written");
    } else {
        ui::detail("running the gate before anything is written");
        crate::check::run(root, false)?;
    }

    let current = read(root, CARRIES_THE_VERSION[0])?;
    let from = version_in(&current).ok_or_else(|| {
        format!(
            "{} names no version, and it is the file the others are checked against",
            CARRIES_THE_VERSION[0]
        )
    })?;
    // A version this tool did not write and cannot order — a pre-release, or a
    // part that is not a number — is said as that, rather than as "not ahead".
    if validate(&from).is_err() {
        return Err(format!(
            "this tree is at `{from}`, which is not three numbers. `cargo xtask release` \
             writes three-number versions and orders them as numbers; ordering anything \
             else is semver's job, and semver is a dependency this repository has not \
             taken. Set the version by hand to the release you mean"
        ));
    }
    if !moves_forward(&from, version) {
        return Err(format!(
            "this tree is at {from} and {version} is not ahead of it. A release that \
             goes backwards, or that repeats a version, prints `git tag -s v{version}` \
             for a tag that already names something else"
        ));
    }

    // Read before anything is written, so the relock's failure has something to
    // put back — and read *before the manifest is rewritten*, which it was not.
    //
    // **A refusal, not a `None`.** This was `.ok()`, which reads "there is no
    // lockfile yet" and "the lockfile could not be read" as the same thing, and
    // the message below then asserted a restore that never happened. Making it a
    // refusal was right and left in the wrong place: it fired *after*
    // `rewrite_listed`, so `Cargo.lock could not be read … Nothing was written`
    // printed over a `Cargo.toml` already at the new version, in a repository
    // with no git — and re-running the same command answered "not ahead of it".
    let locked = std::fs::read(root.join("Cargo.lock")).map_err(|error| {
        format!(
            "Cargo.lock could not be read, and a release rewrites it: {error}. Nothing \
             was written"
        )
    })?;

    let rewritten = if dry_run {
        for name in CARRIES_THE_VERSION {
            ui::detail(&format!("{name}: {from} → {version}"));
        }
        Vec::new()
    } else {
        rewrite_listed(root, &CARRIES_THE_VERSION, version)?
    };

    if dry_run {
        ui::warn(&format!(
            "nothing was written; {version} is what it would be"
        ));
        return Ok(());
    }

    ui::ok(&format!(
        // **What was written, not what was listed.** This printed
        // `CARRIES_THE_VERSION.len()`, so a `rewrite_listed` replaced by
        // `Vec::new()` still reported `1 file(s) carry 0.2.0`, relocked and
        // printed the tag commands having written nothing.
        "{} file(s) carry {version}",
        rewritten.len()
    ));
    for name in &rewritten {
        ui::detail(name);
    }

    // The lockfile records the workspace crates' versions, so it is part of the
    // release rather than a side effect of the next build.
    //
    // **And a failure here puts the version back.** `relock` runs
    // `cargo update --workspace`, which is one registry away from the network,
    // and the window between the write above and this line left the tree with
    // `Cargo.toml` at the new version and `Cargo.lock` at the old one — in a
    // repository with no git to undo it (`§5.3`). Measured with a cargo that
    // fails only on `update`:
    //
    // ```text
    // ok    1 file(s) carry 0.3.0
    // error cargo update refused: …
    // version after: 0.3.0        lockfile says: 0.2.0
    // ```
    //
    // Worse, the obvious recovery was refused: re-running the same command
    // answered `this tree is at 0.3.0 and 0.3.0 is not ahead of it`, so the
    // operator had to hand-edit the manifest back before they could try again.
    // The `ok` line prints before the error, so the transcript reads as a
    // release that happened.
    // **And what an interruption between the two writes costs, before it can
    // happen.** Nothing here handles a signal, and there is a window: the
    // manifest is at the new version and `Cargo.lock` is at the old until
    // `cargo update` returns. Measured with Ctrl-C in that window —
    // 0.218 s wide on a warm cache, wider whenever cargo waits for its package
    // lock — the tree was left with `Cargo.toml` at 0.2.0 and `Cargo.lock` at
    // 0.1.0, and the next command a developer types is:
    //
    // ```text
    // $ cargo xtask check
    // error: cannot update the lock file … because --locked was passed to prevent this
    // ```
    //
    // `.cargo/config.toml` puts `--locked` in the alias, so all five verbs are
    // unreachable at once: the tool cannot be run to undo its own write, in a
    // tree with no git, and cargo's message mentions neither the release nor the
    // version. One line printed *before* the window is what makes that
    // recoverable by somebody who has not read this file.
    //
    // A check *afterwards* cannot help and is not written: in that state cargo
    // refuses to start `xtask` at all, so any guard inside it is unreachable —
    // which is why the recovery is a sentence here and in `AGENTS.md`, and not
    // a gate.
    ui::detail(
        "rewriting Cargo.lock. If this is interrupted, the manifest and the lockfile \
         disagree and every `cargo xtask` verb refuses — the alias passes `--locked`. \
         `cargo update --workspace` puts the lockfile back",
    );
    if let Err(problem) = relock(root) {
        // **The lockfile too, and it was outside the set.** `CARRIES_THE_VERSION`
        // is `["Cargo.toml"]`, so the restore put the manifest back and left
        // `Cargo.lock` at the new version — measured: `Cargo.toml` at 0.1.0 and
        // ten members at 0.2.0 in the lockfile, under a message saying nothing
        // was released. `cargo update` had already written it before failing.
        // The next unlocked cargo re-syncs it, but the gate itself runs
        // `clippy --locked` and `cargo deny --locked`, and an operator reading
        // the tree in between is reading a lie.
        let lockfile = root.join("Cargo.lock");
        let put_back = match (
            rewrite_listed(root, &CARRIES_THE_VERSION, &from),
            put_the_lockfile_back(&lockfile, &locked),
        ) {
            (Ok(_), Ok(())) => {
                format!(". Nothing was released and the version is back at {from}")
            }
            // **And it says which half failed — both halves.** The message
            // emitted "the version is back at {from}" unconditionally and then,
            // in the same sentence, "this tree names {version} in Cargo.toml":
            // an operator who read the first clause and stopped was told the
            // opposite of the truth. And this arm discarded the lockfile's own
            // result with `_`, so somebody told to fix the manifest fixed one of
            // two damaged files and met `clippy --locked` on the other.
            (Err(second), lock) => format!(
                ". And putting {from} back failed too: {second} — this tree names \
                 {version} in {}{}, and there is no git here to undo it",
                CARRIES_THE_VERSION.join(", "),
                if lock.is_ok() {
                    ", with Cargo.lock back as it was"
                } else {
                    ", and Cargo.lock could not be put back either"
                }
            ),
            (Ok(_), Err(second)) => format!(
                ". The version is back at {from} in {}, and Cargo.lock could not be put \
                 back: {second}. Run `cargo update --workspace` when the registry is \
                 reachable",
                CARRIES_THE_VERSION.join(", ")
            ),
        };
        return Err(format!("{problem}{put_back}"));
    }
    ui::ok("Cargo.lock rebuilt");

    // **Printed, never run.** The key needs a touch, and a tool that hung
    // waiting for one would look like a tool that had crashed.
    ui::gap();
    ui::detail("this repository has no git (§5.3), so there is nothing to commit or tag.");
    ui::detail("when it has one, the release ends with:");
    ui::detail(&format!("    git commit -S -am \"release: {version}\""));
    ui::detail(&format!("    git tag -s v{version} -m \"v{version}\""));
    ui::detail("    git push --follow-tags");
    Ok(())
}

fn read(root: &Path, name: &str) -> Result<String, String> {
    std::fs::read_to_string(root.join(name))
        .map_err(|error| format!("could not read {name}: {error}"))
}

/// Rebuilds the lockfile without touching the network beyond what resolution
/// needs.
fn relock(root: &Path) -> Result<(), String> {
    let program = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    #[expect(
        clippy::disallowed_methods,
        reason = "a tool this gate drives; its output is read, not printed"
    )]
    let output = std::process::Command::new(program)
        .args(["update", "--workspace"])
        .current_dir(root)
        .output()
        .map_err(|error| format!("could not run cargo update: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "cargo update refused: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

/// The version a manifest declares, if it declares one.
fn version_in(text: &str) -> Option<String> {
    version_of(text).map(|(_, version)| version)
}

/// The workspace's version, and which line of the file it is on.
///
/// **Under `[workspace.package]`, not the first `version = "` in the file.**
/// This took the first trimmed line with the key, whatever table it was in — so
/// a dependency pinned above the workspace's own table won, and a release
/// rewrote *that*:
///
/// ```text
/// [workspace.dependencies.house-crate]
/// version = "0.2.0"        <- rewritten
/// path = "crates/house"
///
/// [workspace.package]
/// version = "0.1.0"        <- untouched
/// ```
///
/// `1 file(s) carry 0.2.0`, the lockfile rebuilt, `git tag -s v0.2.0` printed,
/// a dependency requirement corrupted, and the release not made. The read-back
/// added last round could not see it, because it re-read through this same
/// function and agreed with itself. Today's manifest happens to put
/// `[workspace.package]` first; the second file added to `CARRIES_THE_VERSION`
/// is the one that bites.
///
/// **And the value is read as TOML writes it.** `version="0.1.0"` with no
/// spaces and `version = "0.1.0" # keep` are both manifests cargo parses, and
/// both answered `None` — which is `Cargo.toml names no version` about a file
/// that names one.
fn version_of(text: &str) -> Option<(usize, String)> {
    let mut table = String::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            name.clone_into(&mut table);
            continue;
        }
        // The workspace's table, a single-crate manifest's own, or the top
        // level of a file that is nothing but a version. What is skipped is a
        // *named* table that is neither — `[workspace.dependencies.x]` is where
        // the decoy lived.
        if !matches!(table.as_str(), "workspace.package" | "package" | "") {
            continue;
        }
        let Some(rest) = line.strip_prefix("version") else {
            continue;
        };
        let Some(value) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let value = value.trim_start();
        // **TOML has two string forms and this read one of them.** `version =
        // '0.1.0'` is a *literal* string, which cargo parses like any other —
        // measured, `cargo metadata` on a scratch manifest answers
        // `"version":"0.1.0"` — and this answered `None` for it. So the whole
        // gate went red on a tree cargo builds, with a message saying the
        // manifest *holds no version*, and `cargo xtask release` refused a
        // manifest naming the version it holds. That is the refusal-of-something-
        // legitimate direction, in the reader the writer was made to follow.
        //
        // This file already records the same class twice — `version="0.1.0"` and
        // `version  = "0.1.0"`, each *a manifest cargo accepts* — and each repair
        // widened only the spellings somebody had tried. TOML has exactly two
        // string forms, so this is the closed set rather than a third list.
        let (version, _) = ['"', '\''].into_iter().find_map(|quote| {
            value
                .strip_prefix(quote)
                .and_then(|rest| rest.split_once(quote))
        })?;
        return Some((index, version.to_owned()));
    }
    None
}

/// Rewrites the version in every listed file, and reads each one back.
///
/// Separated from [`run`] so the writing half can be driven against a scratch
/// tree without re-entering the gate. **`run` had no test at all**: the gate
/// before the write, the refusal on a listed file with no version, the refusal
/// when the files already disagree — all three could be deleted with the whole
/// suite green and `check passed`.
///
/// # Errors
///
/// Returns why a listed file names no version, why the files already disagree,
/// why one could not be written, or why a file does not name the new version
/// after being rewritten.
fn rewrite_listed<'a>(
    root: &Path,
    files: &[&'a str],
    version: &str,
) -> Result<Vec<&'a str>, String> {
    let current = read(root, files[0])?;
    let from = version_in(&current).ok_or_else(|| {
        format!(
            "{} names no version, and it is the file the others are checked against",
            files[0]
        )
    })?;

    // **What each file held before this touched it**, so a refusal can put it
    // back. `rewrite_listed` wrote and *then* validated, and returned `Err` with
    // no restore — so a comment naming the current version above
    // `[workspace.package]` took the rewrite, the read-back saw the real version
    // unchanged, and the run stopped with the comment already edited:
    //
    // ```text
    // $ cargo xtask release 0.2.0
    // check passed
    //   error Cargo.toml was rewritten and still names 0.1.0, not 0.2.0. … Fix the
    //         file and run this again
    //
    // 6: # The pinned floor for the design system is version = "0.2.0" today.
    // 13: version = "0.1.0"
    // ```
    //
    // The message says "fix the file" and never says the file has already been
    // changed, in a tree this module's own header says has no git to undo it
    // (`§5.3`). With more than one entry in `CARRIES_THE_VERSION` the same path
    // leaves file one rewritten when file two refuses. The relock-failure path
    // one screen down already restores both files; this path did not.
    let mut before: Vec<(&str, String)> = Vec::new();
    let restore = |before: &[(&str, String)]| {
        for (name, text) in before {
            let _ = std::fs::write(root.join(name), text);
        }
    };
    let mut rewritten = Vec::new();
    for name in files {
        let text = read(root, name)?;
        before.push((name, text.clone()));
        // Not "skip a file that does not have it". A file listed here and not
        // holding the version is either a list that has gone stale or a file
        // that lost its version, and both are things to look at now.
        if version_in(&text).is_none() {
            restore(&before);
            return Err(format!(
                "{name} is listed as carrying the version and holds none. Either it \
                 stopped carrying one — take it out of CARRIES_THE_VERSION — or the \
                 line was lost, and a release now would ship a tree that disagrees \
                 with itself"
            ));
        }
        // **Spelled as the reader spells it.** `version_of` was widened to accept
        // a version line written with no spaces, which cargo accepts, and `KEY`
        // was not — so `replacen` never matched and the release refused a
        // manifest it had just read, naming the version it holds. The reader and
        // the writer have to agree about what a version line looks like.
        let updated = written_as_the_reader_reads_it(&text, &from, version);
        if updated == text && from != version {
            restore(&before);
            return Err(format!(
                "{name} holds a version and not `{from}`, so the files already disagree"
            ));
        }
        // **And the file now says so, read back the way it was read before.**
        // `version_in` scans *trimmed lines* for the key, so it skips a comment;
        // `replacen` searches the *raw text* and rewrites the first occurrence
        // anywhere, comments included. One comment line above
        // `[workspace.package]` mentioning the current version was enough:
        //
        // ```text
        // $ cargo xtask release 0.2.0
        //   ok    1 file(s) carry 0.2.0
        //         git tag -s v0.2.0 -m "v0.2.0"
        // $ grep -n '^version' Cargo.toml
        // 13:version = "0.1.0"
        // ```
        //
        // Exit 0, the tag command printed, and the release did not happen —
        // which is the state this module's own header says it exists to
        // prevent, reached through the one file it lists.
        if updated != text {
            std::fs::write(root.join(name), &updated)
                .map_err(|error| format!("could not write {name}: {error}"))?;
            rewritten.push(*name);
        }
        let after = read(root, name)?;
        if version_in(&after).as_deref() != Some(version) {
            restore(&before);
            return Err(format!(
                "{name} was rewritten and still names {}, not {version}. The reader \
                 and the writer are one rule — the version is rewritten on the line \
                 `version_of` found it on — so this is a file whose version sits \
                 somewhere that reader does not look: a table other than \
                 `[workspace.package]` or `[package]`, or a spelling it cannot parse. \
                 Every file this run touched has been put back as it was, so fixing \
                 the file and running this again is all there is to do",
                version_in(&after).unwrap_or_else(|| "no version".to_owned())
            ));
        }
    }
    Ok(rewritten)
}

/// The ways TOML writes a version key, for the scanner that looks for files
/// carrying one.
///
/// **A writer's summary sat here.** *"Rewrites the version line, in whichever
/// spelling the file uses"* is a verb phrase about a function, and rustdoc
/// rendered it as the description of a `#[cfg(test)]` array of literals that
/// rewrites nothing — three paragraphs above this block's own sentence saying
/// the writer no longer reads the list. Neither doc-block census can see a
/// summary moved *inside* one block: the block count and the bare count are both
/// conserved, and the summary paragraph is one line, so the fused-summary rule
/// never looks past it. Found by eye in round forty-nine, which is what that
/// rule's own doc says is the only way.
///
/// **And the scanner knew one of them.** `every_file_listed_as_carrying_the_version…`
/// searched the tree for `format!("{KEY}{at}\"")` — a single spelling — while the
/// writer three hundred lines up iterated four. A probe file holding
/// `version="0.1.0"` was invisible to the scan and would have been rewritten by
/// the writer, which is the pairing "the file somebody adds tomorrow" is about:
///
/// ```text
/// version="0.1.0"    -> every_file_listed_as_carrying_the_version… ok
/// version = "0.1.0"  -> …are not in CARRIES_THE_VERSION: ["docs/probe.md"]
/// ```
///
/// **The writer no longer reads this.** It rewrote the version by matching one
/// of these four literals, which is narrower than `version_of` — the reader — so
/// `version  = "0.1.0"` was readable and unwritable, and the release refused a
/// manifest naming the version it had just read. `written_as_the_reader_reads_it`
/// asks `version_of` where the line is. What is left here is the *scanner*'s
/// list, which looks for files that carry a version and are not on the list, and
/// a scanner may be broad.
///
/// **And both quote characters, which it did not.** The scanner looked for a
/// double quote alone, so a *new* file naming its version `version = '0.1.0'`
/// was invisible to the pairing this list exists for — the same blindness the
/// reader carried, in the same rule seen from the other end.
#[cfg(test)]
const SPELLINGS: [&str; 8] = [
    "version = \"",
    "version=\"",
    "version =\"",
    "version= \"",
    "version = '",
    "version='",
    "version ='",
    "version= '",
];

/// The version rewritten on the line the *reader* found it on.
///
/// **The reader and the writer knew different languages.** `version_of` trims
/// whitespace without bound, so `version  = "0.1.0"` — two spaces, which cargo
/// accepts and `cargo fmt` never touches, because it does not format TOML — is
/// an ordinary version to it; `SPELLINGS` is four literals, none of them that
/// one. So the whole gate ran, and then:
///
/// ```text
/// check passed, with 4 warn line(s) above
///   error Cargo.toml holds a version and not `0.1.0`, so the files already disagree
/// ```
///
/// `Cargo.toml` held exactly `0.1.0` and was the only file on the list, so there
/// was nothing for it to disagree with. Asking `version_of` where the version is
/// and rewriting *that line* makes one rule out of two: whatever the reader can
/// read, the writer can write.
fn written_as_the_reader_reads_it(text: &str, from: &str, version: &str) -> String {
    let Some((line_number, found)) = version_of(text) else {
        return text.to_owned();
    };
    if found != from {
        return text.to_owned();
    }
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let Some(line) = lines.get_mut(line_number) else {
        return text.to_owned();
    };
    // **In the quotes the file uses.** `version_of` reads both TOML string
    // forms and this looked for a double-quoted one only, so a manifest cargo
    // accepts — `version = '0.1.0'` — was read, matched, and then left
    // untouched, and `cargo xtask release` answered *the files already
    // disagree* about a list with one file in it. The rewrite keeps the
    // spelling it found, which is what
    // `the_rewrite_does_not_change_the_spelling` asks of it.
    let Some((at, quote)) = ['"', '\'']
        .into_iter()
        .find_map(|quote| Some((line.find(&format!("{quote}{from}{quote}"))?, quote)))
    else {
        return text.to_owned();
    };
    let needle = format!("{quote}{from}{quote}");
    line.replace_range(at..at + needle.len(), &format!("{quote}{version}{quote}"));
    let mut out = lines.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Three numbers separated by dots, and nothing else.
///
/// Checked by hand rather than with a semver dependency: the repository takes
/// three-number versions only, and the error a parser would give
/// ("unexpected character at 5") is worse than the one below.
fn validate(version: &str) -> Result<(), String> {
    let parts: Vec<&str> = version.split('.').collect();
    // **Each part written the way it parses back.** `"01".parse::<u32>()` is
    // `Ok(1)`, so `01.02.03` validated — and cargo's semver parser refuses a
    // leading zero. The gate ran, passed, the manifest was written, and *then*
    // `cargo update` failed:
    //
    // ```text
    //   ok    1 file(s) carry 01.02.03
    //   error cargo update refused: invalid leading zero in major version number
    // ```
    //
    // After which nothing in the workspace builds — including `xtask` itself,
    // so the tool cannot be run again to undo its own write, in a repository
    // with no git to undo it either. The gate running before the write does not
    // help when the *writer* is what is wrong.
    let canonical = |part: &&str| {
        part.parse::<u32>()
            .is_ok_and(|number| number.to_string() == **part)
    };
    if parts.len() != 3 || !parts.iter().all(canonical) {
        return Err(format!(
            "`{version}` is not a version. Expected three numbers, such as 0.2.0 — \
             each written as cargo writes it, so `01` is not `1`"
        ));
    }
    Ok(())
}

/// Whether `to` is a version this repository may move to from `from`.
///
/// **Forward only.** A downgrade and a no-op both validated, both printed
/// `git tag -s v<version>`, and a real `release 0.1.0` on an 0.1.0 tree wrote
/// nothing, reported `1 file(s) carry 0.1.0`, relocked, and told the developer
/// to re-tag a version already shipped. A tag is the one thing in this list
/// that cannot be taken back once it is pushed.
fn moves_forward(from: &str, to: &str) -> bool {
    // **A part this cannot read is not silently dropped.** `filter_map` skipped
    // the part it could not parse and compared what was left, so `0.1.0-rc.1`
    // read as `[0, 1]` — a shorter list that orders against nothing sensibly.
    // This tool takes three numbers and says so; ordering a pre-release
    // correctly is semver's job and semver is a dependency `AGENTS.md` rule 3
    // asks about before it is taken. So a version this cannot order is refused
    // rather than guessed at, and [`run`] says which one it was.
    let numbers = |version: &str| -> Option<Vec<u32>> {
        let parts: Vec<Option<u32>> = version
            .split('.')
            .map(|part| part.parse::<u32>().ok())
            .collect();
        (parts.len() == 3).then_some(())?;
        parts.into_iter().collect()
    };
    match (numbers(from), numbers(to)) {
        (Some(from), Some(to)) => to > from,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{CARRIES_THE_VERSION, KEY, SPELLINGS, moves_forward, validate, version_in};
    use std::path::Path;

    fn root() -> &'static Path {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
    }

    /// Every file under `directory`, with its bytes.
    ///
    /// **A comparison of named files cannot see a file appear.**
    /// `a_dry_run_writes_nothing` read back the one entry in
    /// `CARRIES_THE_VERSION` and asserted it was unchanged, which is true of
    /// every tree a stray write lands in — the write is somewhere else. A
    /// listing is what has the other shape: it holds what the run was not
    /// supposed to touch as well as what it was.
    ///
    /// The bytes and not the length, because a rewrite that keeps the size is
    /// the same defect and a length cannot see it. `crate::gates::walk` is the
    /// walk every gate here uses, so build output is already out of the set.
    fn every_file_under(directory: &Path) -> Vec<(String, Vec<u8>)> {
        let mut seen = Vec::new();
        crate::gates::walk(directory, &mut |path| {
            seen.push((
                crate::relative_to(directory, path),
                std::fs::read(path).unwrap_or_default(),
            ));
        })
        .expect("the tree is readable");
        seen.sort();
        seen
    }

    /// What two listings disagree about, named one file per line.
    ///
    /// Empty is the assertion; the sentences exist so a failure says which file
    /// and what happened to it rather than printing five megabytes of bytes.
    fn changed_between(before: &[(String, Vec<u8>)], after: &[(String, Vec<u8>)]) -> Vec<String> {
        let mut said = Vec::new();
        for (name, bytes) in after {
            match before.iter().find(|(was, _)| was == name) {
                // Bound rather than pushed inline, each of them: the verdict
                // census in `check.rs` reads `.push(` and these are a test's own
                // labels, not sentences the gate can say.
                None => {
                    let appeared = format!("{name} appeared");
                    said.push(appeared);
                }
                Some((_, was)) if was != bytes => {
                    let rewritten = format!("{name} was rewritten");
                    said.push(rewritten);
                }
                Some(_) => {}
            }
        }
        for (name, _) in before {
            if !after.iter().any(|(now, _)| now == name) {
                let vanished = format!("{name} vanished");
                said.push(vanished);
            }
        }
        said.sort();
        said
    }

    /// One file rewritten, and the file then read back.
    ///
    /// **`run` had no test at all.** Everything its header promises — the gate
    /// before the write, the refusal on a listed file with no version, the
    /// refusal when the files already disagree, printing the git commands
    /// rather than running them — could be deleted with the whole gate green:
    /// 657 tests, `check passed`, exit 0. This drives the writing half against
    /// a scratch tree, which is what makes those branches real rather than
    /// documentation.
    ///
    /// `dry_run` is passed so the gate is not re-entered from inside itself.
    fn rewrite(directory: &Path, files: &[(&str, &str)], version: &str) -> Result<(), String> {
        for (name, text) in files {
            let path = directory.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("writable");
            }
            std::fs::write(path, text).expect("writable");
        }
        // **The names this call was given, not `CARRIES_THE_VERSION`.** This
        // called `rewrite_the_version`, which reads the shipped list — one entry
        // — so every case here that named two scratch files wrote both and then
        // rewrote a third, and every assertion about the first going back was
        // true because nothing had touched it. A refusal part-way through a list
        // is exactly the case the shipped list does not reach today.
        let named: Vec<&str> = files.iter().map(|(name, _)| *name).collect();
        super::rewrite_listed(directory, &named, version).map(|_| ())
    }

    /// `--dry-run` writes nothing, and something now checks that.
    ///
    /// **The entire write half of `run` was executed by no test.** Both calls in
    /// this module pass `dry_run = true` and return before any write, so
    /// deleting the `if dry_run { … } else { rewrite_listed(…) }` arm left
    /// `cargo test -p xtask --bin xtask release::` at `11 passed; 0 failed` —
    /// and then:
    ///
    /// ```text
    /// $ cargo xtask release 0.1.1 --dry-run
    ///   warn  --dry-run: the gate is not run and nothing is written
    ///   warn  nothing was written; 0.1.1 is what it would be
    /// $ sed -n 11p Cargo.toml
    /// version = "0.1.1"
    /// ```
    ///
    /// The line saying nothing was written printed while the file was written,
    /// in a repository with no git — and every later `cargo xtask` then refused
    /// with `cannot update the lock file … because --locked was passed`.
    ///
    /// Driven against a scratch tree rather than this one, and only in the
    /// `--dry-run` direction: the other direction runs the whole gate and then
    /// `cargo update`, which is a release, not a test.
    #[test]
    fn a_dry_run_writes_nothing() {
        let scratch = std::env::temp_dir().join(format!("nc-dry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).expect("writable");
        let manifest = "[workspace.package]\nversion = \"0.1.0\"\n";
        for (name, body) in [("Cargo.toml", manifest), ("Cargo.lock", "version = 4\n")] {
            std::fs::write(scratch.join(name), body).expect("writable");
        }

        // **The whole tree, not the two files this test wrote.** The assertion
        // below reads back `CARRIES_THE_VERSION` and finds it unchanged, which
        // a stray write elsewhere leaves true. Measured: a
        // `fs::copy(root.join("clippy.toml"), root.join("probe.txt"))` at the
        // top of `run`, before the gate, put a file into the repository root —
        // a tree with no git — and all thirteen tests in this module passed.
        let before = every_file_under(&scratch);

        super::run(&scratch, "0.2.0", true).expect("a dry run of a valid move");

        assert_eq!(
            changed_between(&before, &every_file_under(&scratch)),
            Vec::<String>::new(),
            "`--dry-run` printed that nothing was written and the tree is not the tree \
             it was given"
        );

        for name in CARRIES_THE_VERSION {
            assert_eq!(
                std::fs::read_to_string(scratch.join(name)).expect("readable"),
                manifest,
                "`--dry-run` said nothing was written and wrote {name}, in a tree with \
                 no git to put it back"
            );
        }
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// The write half of `run` is a call nothing had entered.
    ///
    /// **`a_dry_run_writes_nothing` drives the `true` arm, and every other test
    /// in this module passes `true` too.** So the one call in `run` that
    /// rewrites the listed files was reached by
    /// no test: replacing the write with an empty vector left
    /// `cargo test -p xtask --bin xtask release` at `12 passed; 0 failed` and
    /// `check passed`, and a real `cargo xtask release 0.2.0` then printed
    /// `1 file(s) carry 0.2.0`, relocked and printed the tag commands having
    /// written nothing, in a tree with no git to notice.
    ///
    /// `run` cannot be driven with `dry_run = false` from a test — it runs the
    /// whole gate first, on a scratch tree that is not a workspace — so what is
    /// held here is the call itself: the same arguments line 104 passes, over
    /// the same list, with the file required to change. The count `run` prints
    /// is `rewritten.len()` now, so a write that does nothing cannot report a
    /// file.
    #[test]
    fn the_write_half_writes() {
        let scratch = std::env::temp_dir().join(format!("nc-write-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).expect("writable");
        for name in CARRIES_THE_VERSION {
            std::fs::write(
                scratch.join(name),
                "[workspace.package]\nversion = \"0.1.0\"\n",
            )
            .expect("writable");
        }

        let rewritten = super::rewrite_listed(&scratch, &CARRIES_THE_VERSION, "0.2.0")
            .expect("every listed file carries the version it is moving from");

        assert_eq!(
            rewritten.len(),
            CARRIES_THE_VERSION.len(),
            "the write half reported {} file(s) for a list of {}, and `run` prints that \
             number as `N file(s) carry <version>`",
            rewritten.len(),
            CARRIES_THE_VERSION.len()
        );
        for name in CARRIES_THE_VERSION {
            assert_eq!(
                std::fs::read_to_string(scratch.join(name)).expect("readable"),
                "[workspace.package]\nversion = \"0.2.0\"\n",
                "{name} was listed as carrying the version and does not carry the new one"
            );
        }
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn a_version_written_into_a_comment_is_not_a_release() {
        // **Measured, with the real verb.** `version_in` reads the first
        // *trimmed line* beginning `version = "`, so it skips a comment;
        // `replacen` rewrote the first occurrence in the *raw text*, which a
        // comment can be. Exit 0, `1 file(s) carry 0.2.0`, the tag command
        // printed — and `[workspace.package]` still said 0.1.0.
        let scratch = std::env::temp_dir().join(format!("nc-release-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).expect("writable");

        let decoy = "# the line below is the one this rewrites: version = \"0.1.0\"\n\
                     [workspace.package]\nversion = \"0.1.0\"\n";
        // **The refusal became a release, and that is the repair working.** The
        // writer now rewrites the line `version_of` found rather than the first
        // occurrence in the raw text, so the comment is not a candidate at all —
        // where before it was rewritten and the real version left behind, and
        // the only thing standing between that and a bad release was a read-back
        // that refused *after* the write. One rule cannot disagree with itself.
        rewrite(&scratch, &[("Cargo.toml", decoy)], "0.2.0")
            .expect("the real version is on a line the reader found");
        let after = std::fs::read_to_string(scratch.join("Cargo.toml")).expect("readable");
        assert_eq!(
            after,
            "# the line below is the one this rewrites: version = \"0.1.0\"\n\
             [workspace.package]\nversion = \"0.2.0\"\n",
            "the comment is prose and the version is the version"
        );

        let good = "[workspace.package]\nversion = \"0.1.0\"\n";

        // **All three refusal paths, not one.** The repair went into the
        // read-back path; the two above it — a listed file holding no version,
        // and a file whose version is not the one being moved from — still
        // returned with the first file rewritten at the *new* version under a
        // message saying nothing was released, in a tree with no git.
        let holds_none = "[package]\nname = \"x\"\n";
        let refused = rewrite(
            &scratch,
            &[("first.toml", good), ("second.toml", holds_none)],
            "0.2.0",
        )
        .expect_err("a listed file that carries no version stops the release");
        assert!(refused.contains("holds none"), "{refused}");
        assert_eq!(
            std::fs::read_to_string(scratch.join("first.toml")).expect("readable"),
            good,
            "a file was left rewritten when a later one carried no version"
        );

        let disagrees = "[workspace.package]\nversion = \"9.9.9\"\n";
        let refused = rewrite(
            &scratch,
            &[("first.toml", good), ("second.toml", disagrees)],
            "0.2.0",
        )
        .expect_err("files that already disagree stop the release");
        assert!(refused.contains("already disagree"), "{refused}");
        assert_eq!(
            std::fs::read_to_string(scratch.join("first.toml")).expect("readable"),
            good,
            "a file was left rewritten when a later one already disagreed"
        );

        // **And the spelling the reader accepts, the writer writes.**
        // `version_of` was widened to read a version line with no spaces around
        // the `=` — which cargo accepts — and `KEY` was not, so the rewrite
        // matched nothing and the release refused a manifest it had just read,
        // naming the version it holds. Reachable today, with one entry listed.
        // Two spaces: what cargo accepts, what `cargo fmt` never touches
        // because it does not format TOML, and what `SPELLINGS`' four literals
        // did not hold — so the whole gate ran and the release then refused a
        // manifest holding exactly the version it had just read.
        let spaced = "[workspace.package]\nversion  = \"0.1.0\"\n";
        rewrite(&scratch, &[("Cargo.toml", spaced)], "0.2.0")
            .expect("whatever the reader can read, the writer can write");
        assert_eq!(
            std::fs::read_to_string(scratch.join("Cargo.toml")).expect("readable"),
            "[workspace.package]\nversion  = \"0.2.0\"\n",
            "the rewrite changed the spacing as well as the number"
        );

        let tight = "[workspace.package]\nversion=\"0.1.0\"\n";
        rewrite(&scratch, &[("Cargo.toml", tight)], "0.2.0")
            .expect("a manifest cargo accepts is one this can release");
        assert_eq!(
            std::fs::read_to_string(scratch.join("Cargo.toml")).expect("readable"),
            "[workspace.package]\nversion=\"0.2.0\"\n",
            "the rewrite changed the spelling as well as the number"
        );

        // **And the other of TOML's two string forms**, which `version_of`
        // reads and this refused to write: the needle was a double-quoted one
        // whatever the file used, so a manifest cargo accepts was read, matched
        // and left untouched — and `release` then answered *the files already
        // disagree* about a list with one file in it.
        let quoted = "[workspace.package]\nversion = '0.1.0'\n";
        rewrite(&scratch, &[("Cargo.toml", quoted)], "0.2.0")
            .expect("a manifest cargo accepts is one this can release");
        assert_eq!(
            std::fs::read_to_string(scratch.join("Cargo.toml")).expect("readable"),
            "[workspace.package]\nversion = '0.2.0'\n",
            "the rewrite changed the quotes as well as the number"
        );

        // The same, for a list of more than one: the first file is written, the
        // second refuses, and the first must go back.
        //
        // The decoy above is rewritable now, so the file that refuses is one
        // whose only `version` sits in a table the reader skips — a dependency's
        // pin, which is the shape `version_of`'s own comment names as the decoy
        // it was taught to walk past.
        let unwritable = "[workspace.dependencies.serde]\nversion = \"0.1.0\"\n";
        let refused = rewrite(
            &scratch,
            &[("first.toml", good), ("second.toml", unwritable)],
            "0.2.0",
        )
        .expect_err("the second file has no version this rewrites");
        assert!(refused.contains("holds none"), "{refused}");
        assert_eq!(
            std::fs::read_to_string(scratch.join("first.toml")).expect("readable"),
            good,
            "the first file was left rewritten when the second refused"
        );

        // And the ordinary file, so the refusal above is about the comment.
        let plain = "[workspace.package]\nversion = \"0.1.0\"\nedition = \"2024\"\n";
        rewrite(&scratch, &[("Cargo.toml", plain)], "0.2.0").expect("an ordinary manifest");
        let after = std::fs::read_to_string(scratch.join("Cargo.toml")).expect("readable");
        assert_eq!(version_in(&after).as_deref(), Some("0.2.0"));

        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn a_listed_file_that_lost_its_version_stops_the_release() {
        // The branch is unreachable while `CARRIES_THE_VERSION` has one entry —
        // `from` is read from the same file the loop re-reads — so it was
        // documentation rather than a gate. Driven here with a second file, it
        // is the rule the header describes: a list that has gone stale, or a
        // file that lost its line, is a tree that would ship disagreeing with
        // itself.
        let scratch = std::env::temp_dir().join(format!("nc-release-two-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).expect("writable");
        std::fs::write(
            scratch.join("Cargo.toml"),
            "[workspace.package]\nversion = \"0.1.0\"\n",
        )
        .expect("writable");
        std::fs::write(scratch.join("second.toml"), "edition = \"2024\"\n").expect("writable");

        let refused = super::rewrite_listed(&scratch, &["Cargo.toml", "second.toml"], "0.2.0")
            .expect_err("a listed file with no version stops the release");
        assert!(
            refused.contains("second.toml") && refused.contains("holds none"),
            "the refusal does not name the file that lost its version: {refused}"
        );

        // And a second file that holds a *different* version is the other half:
        // the tree already disagrees, and a release would paper over it. The
        // manifest goes back to where it started, because the call above left
        // it at the version this one is moving to.
        std::fs::write(
            scratch.join("Cargo.toml"),
            "[workspace.package]\nversion = \"0.1.0\"\n",
        )
        .expect("writable");
        std::fs::write(scratch.join("second.toml"), "version = \"9.9.9\"\n").expect("writable");
        let refused = super::rewrite_listed(&scratch, &["Cargo.toml", "second.toml"], "0.2.0")
            .expect_err("a tree that already disagrees stops the release");
        assert!(
            refused.contains("already disagree"),
            "the refusal does not say the files disagree: {refused}"
        );

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// A line with any leading visibility taken off it.
    ///
    /// **A re-export is an import, and this rule did not think so.**
    /// [`the_imports_of`] matched `starts_with("use ")` alone, so
    /// `pub(crate) use std::fs as disk;` was not an import at all — an ordinary
    /// line, `cargo fmt` and clippy clean — and every rule built on that census
    /// held against nothing: `disk::write(…)` is `std::fs::write` to rustc and
    /// matches none of the spellings below, which is exactly the alias the
    /// paragraph above says the census exists to refuse.
    ///
    /// `pub`, `pub(crate)`, `pub(super)` and `pub(in path)` are all of them.
    /// Whitespace is required after the visibility, so an identifier that merely
    /// begins with those three letters is left alone.
    fn without_a_visibility(line: &str) -> &str {
        let Some(rest) = line.strip_prefix("pub") else {
            return line;
        };
        let rest = if let Some(inside) = rest.strip_prefix('(') {
            match inside.find(')') {
                Some(at) => &inside[at + 1..],
                None => return line,
            }
        } else {
            rest
        };
        if rest.starts_with(char::is_whitespace) {
            rest.trim_start()
        } else {
            line
        }
    }

    /// Every `use` statement in `shipped`, one per line and joined.
    ///
    /// Split out of [`the_gate_runs_before_the_first_write`], which crossed
    /// clippy's line limit with it inline. The whole statement, because
    /// `use std::fs\n    as disk;` is the same import and rustfmt is free to
    /// wrap a long one; a bound on how far it will look, because an unterminated
    /// `use` is a file that does not compile and not this rule's business.
    fn the_imports_of(shipped: &str) -> String {
        let lines: Vec<&str> = shipped.lines().collect();
        let mut imports = String::new();
        for (at, line) in lines.iter().enumerate() {
            let opens = without_a_visibility(line.trim());
            if !opens.starts_with("use ") {
                continue;
            }
            let mut statement = opens.to_owned();
            let mut next = at + 1;
            while !statement.ends_with(';') && next < lines.len() && next - at < 20 {
                statement.push(' ');
                statement.push_str(lines[next].trim());
                next += 1;
            }
            imports.push_str(&statement.split_whitespace().collect::<Vec<_>>().join(" "));
            imports.push('\n');
        }
        imports
    }

    /// Every function of this workspace the shipped half of this module calls.
    ///
    /// **A write can be a call into this workspace, and every rule beside this
    /// reads `std::fs`.** Measured, two lines inside `run`:
    ///
    /// ```text
    /// if !dry_run { crate::docs::run(root)?; }
    /// validate(version)?;
    ///
    ///   386 passed, `cargo fmt --all --check` clean
    ///   cargo xtask release 0.1.1, with docs/errors.md stale
    ///     ->  docs / ok 6 page(s), 1 rewritten
    ///         running the gate before anything is written
    /// ```
    ///
    /// Six pages rewritten, one of them changed, *above* the line announcing
    /// that the gate had not yet run — in a tree with no git (`§5.3`). All five
    /// guards were silent, and each for the same reason: the imports rule reads
    /// `use … as`, `nothing_binds_a_write` reads `= std::fs::`,
    /// `puts_bytes_on_disk` is twenty literal spellings, and the `Command`
    /// census counts child processes. `crate::docs::run(root)` is none of
    /// those. The `!dry_run` guard is what put it on the one path no test
    /// enters — both behavioural tests pass `dry_run = true` — and without that
    /// guard `a_dry_run_writes_nothing` does fire, which is the half that held.
    ///
    /// So this is a name list, compared both directions the way
    /// `xtask/src/policy.rs`'s `CORE_POLICIES` is: a call into this workspace
    /// that is not written here fails, and a name written here that the module
    /// stopped calling fails too. The list is one entry, and it is the gate.
    const MAY_REACH: [&str; 1] = ["crate::check::run"];

    /// Every `use` statement the shipped half is allowed to carry.
    ///
    /// The other half of [`MAY_REACH`]: the call census reads paths beginning
    /// `crate::`, and `use crate::docs;` followed by `docs::run(root)` is the
    /// same call spelled through an import. The alias rule beside this refuses
    /// `use … as`; this refuses the import itself, so both spellings of a new
    /// reach into this workspace are a line added here.
    const MAY_IMPORT: [&str; 2] = ["use std::path::Path;", "use crate::ui;"];

    /// The three spellings of a path into this workspace, and what each one is.
    ///
    /// **`super::` is a reach `MAY_REACH` could not spell, and it is the same
    /// defect respelled.** The census read one needle, `crate::`, and this file
    /// is a child of the crate root — so `super::docs::run(root)?` on the
    /// `!dry_run` path *is* `crate::docs::run` to rustc, and it is the write the
    /// paragraph on [`MAY_REACH`] records verbatim. Measured, those two lines
    /// inside `run` spelled that way: `15 passed`, and `cargo xtask check`
    /// byte-identical to a clean run. The import half held — `use super::docs;`
    /// is an import [`MAY_IMPORT`] does not name — and the call half saw
    /// nothing at all.
    ///
    /// `check.rs` already carries this list for its own census, as
    /// `THIS_MODULE`, which is one rule written twice with the second copy a
    /// spelling short.
    ///
    /// Each needle is paired with what it means in front of a path here: this
    /// module is `crate::release`, so `super::` resolves to the crate root and
    /// `self::` to this module. Both are rewritten to the spelling `MAY_REACH`
    /// is written in, or the list would have to name every way of saying one
    /// function.
    ///
    /// **And a sibling member is a reach with no prefix at all.** The three
    /// above are the ways to say "somewhere in *this crate*", and `MAY_REACH`'s
    /// own doc says "every function of this workspace". `xtask` depends on
    /// `nc-catalog`, `nc-catalog-compiler` and `nc-policy`, and a call into one
    /// of those needs no import and wears none of these prefixes. Measured, one
    /// line inside `run`:
    ///
    /// ```text
    /// let _ = nc_catalog_compiler::camel("probe");
    ///   ->  16 passed, `cargo xtask check` byte-identical to a clean run
    /// ```
    ///
    /// No live write follows from it today — those three crates hand back values
    /// and the writes are in `build.rs` — so this is a census with a hole in it
    /// rather than a hole with a write behind it. The hole is the finding: the
    /// list says what this module reaches, and a whole class of reach could not
    /// be spelled.
    ///
    /// The names come from `cargo metadata`, through the one reader of the
    /// resolved graph this crate has, so a member added to the workspace is in
    /// this census the moment cargo builds it — a written-out list here would be
    /// the fourth hand copy of the member list in this repository. Cargo spells
    /// a package name with hyphens and rustc spells the crate with underscores,
    /// which is the one translation between them.
    ///
    /// Assembled, or the needles are the thing they look for.
    fn the_spellings_of_a_reach() -> Vec<(String, String)> {
        let mut spellings = vec![
            (["cra", "te::"].join(""), ["cra", "te::"].join("")),
            (["sup", "er::"].join(""), ["cra", "te::"].join("")),
            (["se", "lf::"].join(""), ["cra", "te::release::"].join("")),
        ];
        for member in crate::members::members(root()).expect("cargo lists this workspace") {
            let crate_name = format!("{}::", member.name.replace('-', "_"));
            spellings.push((crate_name.clone(), crate_name));
        }
        spellings
    }

    /// Every path into this workspace that the shipped half *calls*, in the one
    /// spelling [`MAY_REACH`] is written in.
    ///
    /// Read out of the blanked text, so a `crate::…` named in this module's
    /// prose — and there are several — is prose and not a call. A call is told
    /// from a citation by what follows the path, as
    /// [`the_fs_bindings_of`] tells a call from a binding: a call opens a
    /// parenthesis.
    fn the_workspace_calls_of(shipped: &str) -> Vec<String> {
        let spent = crate::gates::blanked(shipped);
        let mut called: Vec<String> = Vec::new();
        for (needle, instead) in the_spellings_of_a_reach() {
            let mut rest = spent.as_str();
            while let Some(at) = rest.find(&needle) {
                rest = &rest[at..];
                let path: String = rest
                    .chars()
                    .take_while(|character| {
                        character.is_alphanumeric() || *character == '_' || *character == ':'
                    })
                    .collect();
                rest = &rest[path.len()..];
                // What rustc resolves it to, so one function has one name here
                // however it was written.
                let resolved = match path.strip_prefix(&needle) {
                    Some(tail) => format!("{instead}{tail}"),
                    None => path,
                };
                if rest.starts_with('(') && !called.contains(&resolved) {
                    called.push(resolved);
                }
            }
        }
        called.sort();
        called
    }

    /// `shipped` with its `use` statements blanked, line for line.
    ///
    /// An import is a path into this workspace that opens no parenthesis, and
    /// so is a binding — which is the whole distinction
    /// [`the_workspace_bindings_of`] reads. The two are told apart the only way
    /// they can be: an import is a statement of its own, and this is the same
    /// line walk [`the_imports_of`] uses to collect them, spending the lines
    /// instead of keeping them.
    fn outside_the_imports(shipped: &str) -> String {
        let lines: Vec<&str> = shipped.lines().collect();
        let mut out = String::with_capacity(shipped.len());
        let mut at = 0_usize;
        while at < lines.len() {
            let opens = without_a_visibility(lines[at].trim());
            if !opens.starts_with("use ") {
                out.push_str(lines[at]);
                out.push('\n');
                at += 1;
                continue;
            }
            // A long import is wrapped by rustfmt, and every line of it is the
            // one statement.
            let began = at;
            let mut statement = opens.to_owned();
            while !statement.ends_with(';') && at + 1 < lines.len() && at - began < 20 {
                at += 1;
                statement.push(' ');
                statement.push_str(lines[at].trim());
            }
            for _ in began..=at {
                out.push('\n');
            }
            at += 1;
        }
        out
    }

    /// Every path into this workspace the shipped half *binds* rather than
    /// calls.
    ///
    /// **The half that was missing, and its own doc said it was there.**
    /// [`the_workspace_calls_of`] says it tells a call from a binding "as
    /// [`the_fs_bindings_of`] does" — and that sibling *reports* the bindings it
    /// finds, while this one dropped them on the floor at
    /// `if rest.starts_with('(')`. A function is a value in Rust, so the two
    /// lines
    ///
    /// ```text
    /// let go = crate::docs::run;
    /// go(root)?;
    /// ```
    ///
    /// are `crate::docs::run(root)` to rustc and match nothing `MAY_REACH` can
    /// refuse. Measured, that pair placed on the non-dry-run path: `16 passed`
    /// and `check passed`; the control — the same call written out — fails with
    /// the assertion naming `crate::docs::run`. `nothing_binds_a_write` records
    /// the identical defeat for `std::fs`, and `xtask/src/main.rs` a third for
    /// the print scan.
    ///
    /// Read outside the `use` statements, because an import is a path that
    /// opens no parenthesis too — and this module imports `crate::ui`, which
    /// [`MAY_IMPORT`] already decides.
    fn the_workspace_bindings_of(shipped: &str) -> Vec<String> {
        let spent = crate::gates::blanked(&outside_the_imports(shipped));
        let mut bound: Vec<String> = Vec::new();
        for (needle, instead) in the_spellings_of_a_reach() {
            let mut rest = spent.as_str();
            while let Some(at) = rest.find(&needle) {
                rest = &rest[at..];
                let path: String = rest
                    .chars()
                    .take_while(|character| {
                        character.is_alphanumeric() || *character == '_' || *character == ':'
                    })
                    .collect();
                rest = &rest[path.len()..];
                // A prefix with nothing after it names no function: `crate::` on
                // its own is how a path is written in a `mod` declaration, not a
                // value anybody can call later.
                if path.len() == needle.len() {
                    continue;
                }
                let resolved = match path.strip_prefix(&needle) {
                    Some(tail) => format!("{instead}{tail}"),
                    None => path,
                };
                if !rest.starts_with('(') && !bound.contains(&resolved) {
                    bound.push(resolved);
                }
            }
        }
        bound.sort();
        bound
    }

    /// The shipped half binds no function of this workspace to a name.
    ///
    /// The shape [`nothing_binds_a_write`] has, for the census beside it: a
    /// call is refused unless `MAY_REACH` names it, and a binding is refused
    /// outright, because the name it is bound to is a word the author chooses
    /// and no list of spellings can hold it.
    #[track_caller]
    fn nothing_binds_a_workspace_call(shipped: &str) {
        let bound = the_workspace_bindings_of(shipped);
        assert!(
            bound.is_empty(),
            "this module binds a function of this workspace to a name instead of calling \
             it, and the census beside this reads a call: `let go = crate::docs::run;` \
             makes `go(root)` a reach `MAY_REACH` cannot refuse. The bindings are: \
             {bound:?}"
        );
    }

    /// The shipped half reaches only the functions of this workspace it says it
    /// does, and imports only what it says it imports.
    ///
    /// Its own function because [`the_gate_runs_before_the_first_write`] is at
    /// clippy's line limit, which is why its neighbours are functions too.
    #[track_caller]
    fn nothing_else_in_this_workspace_is_reached(shipped: &str, imports: &str) {
        let called = the_workspace_calls_of(shipped);
        let mut written: Vec<String> = MAY_REACH.iter().map(|one| (*one).to_owned()).collect();
        written.sort();
        assert_eq!(
            called, written,
            "the shipped half of this module calls {called:?} inside this workspace and \
             `MAY_REACH` names {written:?}. A write can be a call — `crate::docs::run(root)` \
             rewrote six pages of this repository above the line announcing the gate had \
             not run, and every `std::fs` rule beside this was silent"
        );
        let carried: Vec<&str> = imports.lines().collect();
        assert_eq!(
            carried, MAY_IMPORT,
            "the shipped half of this module imports {carried:?} and `MAY_IMPORT` names \
             {MAY_IMPORT:?}. `use crate::docs;` then `docs::run(root)` is a reach into \
             this workspace the call census above cannot see, because the path it writes \
             does not begin with the crate"
        );
    }

    /// The two rules a list of `std::fs` spellings cannot state, together.
    ///
    /// One function because [`the_gate_runs_before_the_first_write`] is at
    /// clippy's line limit, which is why its neighbours are functions too.
    #[track_caller]
    fn nothing_renames_or_reaches_past_the_gate(shipped: &str, imports: &str) {
        // The rename written with `let`, which no import carries.
        nothing_binds_a_write(shipped);
        // And a write that is neither: a call into this workspace.
        nothing_else_in_this_workspace_is_reached(shipped, imports);
        // And the same rename on that census, which had only the call half.
        nothing_binds_a_workspace_call(shipped);
    }

    /// Every place the shipped half *binds* a `std::fs` function rather than
    /// calling it.
    ///
    /// **The rename the import rule cannot see.** `use std::fs as disk;` is
    /// refused above, and `let put = std::fs::write;` is the same rename written
    /// in a body: `put(root.join("probe.txt"), b"planted")` is `std::fs::write`
    /// to rustc and matches none of the spellings the census below holds.
    /// Measured, that pair placed after the `--dry-run` early return wrote a
    /// file into the root of this repository with `381 passed` and
    /// `check passed`; the control — the same call spelled `std::fs::write(` —
    /// fails the count at `left: 4 right: 3`. `xtask/src/main.rs` records the
    /// identical defeat for the print scan, in its own header:
    /// `let handle = std::io::stdout;` and then `handle()`.
    ///
    /// A call is told from a binding by what follows the path: a call opens a
    /// parenthesis and a binding does not. That distinction is what keeps the
    /// eleven ordinary `std::fs::read_to_string(…)` calls in this module out of
    /// it.
    ///
    /// **And `let` was only one of the ways to hand a function on**, which is
    /// what the two needles `= std::fs::` and `= fs::` could see. The paragraph
    /// above says a function is a value in Rust and then closed the assignment
    /// spelling alone; a value can be put in an array, in an `Option`, or passed
    /// straight to something that calls it, and none of those is an `=`.
    /// Measured, one line on the `!dry_run` path:
    ///
    /// ```text
    /// let _ = [std::fs::write].map(|put| put(root.join("probe.txt"), b"planted"));
    ///   ->  cargo fmt clean, clippy exit 0, 16 passed, `cargo xtask check`
    ///       byte-identical to a clean run — and a 7-byte `probe.txt` in the
    ///       root of this repository
    /// ```
    ///
    /// `Some(std::fs::write).map(…)` and `f(std::fs::write)` are the same shape.
    /// So the `= ` prefix is gone and what is left is the test the sibling
    /// [`the_workspace_calls_of`] already used: the path is found wherever it is
    /// written, and what follows it says which of the two it is. One needle
    /// rather than two, because `std::fs::write` and `fs::write` differ only in
    /// a qualifier that is read backwards off the same match — two needles would
    /// report the qualified spelling twice.
    ///
    /// Read out of the blanked text, for the reason [`the_workspace_calls_of`]
    /// gives: this module's own prose names `std::fs::write` a dozen times, and
    /// a rule that could not tell a paragraph from a program would be
    /// unsatisfiable here.
    fn the_fs_bindings_of(shipped: &str) -> Vec<String> {
        let spent = crate::gates::blanked(shipped);
        let is_path = |character: char| character.is_alphanumeric() || character == '_';
        let mut bound = Vec::new();
        for (at, _) in spent.match_indices("fs::") {
            // The qualifier in front of it, so `std::fs::write` is one match and
            // not two, and so a field or a variable called `…fs` is not one.
            let head_at = spent[..at]
                .rfind(|character: char| !(is_path(character) || character == ':'))
                .map_or(0, |before| before + 1);
            let head = &spent[head_at..at];
            if !(head.is_empty() || head.ends_with("::")) {
                continue;
            }
            let rest = &spent[at + "fs::".len()..];
            let path: String = rest
                .chars()
                .take_while(|character| is_path(*character) || *character == ':')
                .collect();
            if path.is_empty() || rest[path.len()..].starts_with('(') {
                continue;
            }
            // Bound rather than pushed inline, for the reason its neighbours
            // give: the verdict census reads `.push(`.
            let spelled = format!("{head}fs::{path}");
            bound.push(spelled);
        }
        bound
    }

    /// The shipped half binds no `std::fs` function to a name.
    ///
    /// **A function is a value in Rust, so the import rule cannot be the whole
    /// of it.** `let put = std::fs::write;` binds the name in a body, where no
    /// `use` statement carries it — measured, that binding plus
    /// `let _ = put(root.join("probe"), b"planted");` after the `--dry-run`
    /// early return gave `381 passed` and `check passed`, with a file written
    /// into the root of this repository. The control, the same call written
    /// out, fails the write count at `left: 4 right: 3`.
    ///
    /// Its own function because [`the_gate_runs_before_the_first_write`] is at
    /// clippy's line limit, which is why the import rule beside it is a
    /// function too.
    #[track_caller]
    fn nothing_binds_a_write(shipped: &str) {
        let bound = the_fs_bindings_of(shipped);
        assert!(
            bound.is_empty(),
            "this module binds a `std::fs` function to a name instead of calling it, and \
             every rule beside this is a list of spellings: `let put = std::fs::write;` \
             makes `put(…)` a write none of them can see. The bindings are: {bound:?}"
        );
    }

    /// A `std::fs` function bound to a name is read as a binding, and a call as
    /// a call.
    ///
    /// The floor under the refusal in [`the_gate_runs_before_the_first_write`]:
    /// a reader that saw a binding in every `std::fs::read_to_string(…)` would
    /// be unsatisfiable, and one that saw none would refuse nothing.
    #[test]
    fn a_std_fs_function_bound_to_a_name_is_read_as_a_binding() {
        for bound in [
            "let put = std::fs::write;\n",
            "let put = fs::write;\n",
            "let put: fn(&Path, &[u8]) = std::fs::write;\n",
            "let gone = std::fs::remove_dir_all;\n",
            "let make = std::fs::DirBuilder::new;\n",
            // **And the four spellings that are not an assignment.** A function
            // is a value, and a value goes into an array, into an `Option`, into
            // an argument and into a return position without an `=` anywhere
            // near it. The first of these wrote a file into the root of this
            // repository with every rule in this module green.
            "let _ = [std::fs::write].map(|put| put(root.join(\"probe.txt\"), b\"x\"));\n",
            "let _ = Some(std::fs::write).map(|put| put(root, b\"x\"));\n",
            "hand_it_on(std::fs::write);\n",
            "    std::fs::write\n",
        ] {
            assert_eq!(
                the_fs_bindings_of(bound).len(),
                1,
                "`{bound}` binds a `std::fs` function to a name and was read as a call"
            );
        }
        for called in [
            "let text = std::fs::read_to_string(&path).expect(\"readable\");\n",
            "let _ = std::fs::write(root.join(name), text);\n",
            "let file = std::fs::File::create(&path);\n",
            "let text = fs::read_to_string(&path);\n",
            // A mention in this module's own prose, which the widened needle
            // would otherwise report on every paragraph that names the rule.
            "// `let put = std::fs::write;` binds it rather than calling it\n",
            "let hint = \"std::fs::write\";\n",
            // And a name that merely ends in those letters.
            "let path = self.prefs::default();\n",
        ] {
            assert!(
                the_fs_bindings_of(called).is_empty(),
                "`{called}` calls a `std::fs` function and was read as a binding"
            );
        }
    }

    /// A call into this workspace is read as one however it is spelled.
    ///
    /// The floor under [`nothing_else_in_this_workspace_is_reached`]: the census
    /// read `crate::` alone, so the same call written `super::` was no call at
    /// all — and this file is a child of the crate root, which makes those two
    /// spellings the same function. Both directions, because a census that
    /// answered every path would refuse this module's own prose.
    #[test]
    fn a_reach_into_this_workspace_is_read_however_it_is_spelled() {
        // Assembled, or this test's own text is a call site.
        let root = ["cra", "te::"].join("");
        let up = ["sup", "er::"].join("");
        let here = ["se", "lf::"].join("");
        let sibling = ["nc_catalog", "_compiler::"].join("");
        for (why, code, resolved) in [
            (
                "the crate root, written out",
                format!("{root}docs::run(root)?;\n"),
                format!("{root}docs::run"),
            ),
            (
                "the parent module, which is the crate root",
                format!("{up}docs::run(root)?;\n"),
                format!("{root}docs::run"),
            ),
            (
                "this module, the long way",
                format!("{here}validate(version)?;\n"),
                format!("{root}release::validate"),
            ),
            (
                "guarded, on the path no test enters",
                format!("if !dry_run {{ {up}docs::run(root)?; }}\n"),
                format!("{root}docs::run"),
            ),
            // **A sibling member, which wears none of the three prefixes.**
            // `xtask` depends on three crates of this workspace and a call into
            // one needs no import at all, so `MAY_REACH` — whose own doc says
            // "every function of this workspace" — could not spell a whole class
            // of reach. Assembled, like its neighbours.
            (
                "a sibling member of this workspace",
                format!("let _ = {sibling}camel(\"probe\");\n"),
                format!("{sibling}camel"),
            ),
        ] {
            assert_eq!(
                the_workspace_calls_of(&code),
                vec![resolved.clone()],
                "`{code}` reaches `{resolved}` inside this workspace ({why}) and the \
                 census walks past it, so `MAY_REACH` names nothing it could refuse"
            );
        }
        for (why, code) in [
            (
                "a citation rather than a call",
                format!("// see {root}check::run\n"),
            ),
            (
                "an import, which the other half reads",
                format!("use {up}docs;\n"),
            ),
            (
                "a path that opens no parenthesis",
                format!("let it = {root}check::run;\n"),
            ),
            (
                "ordinary code",
                "let root = repository_root();\n".to_owned(),
            ),
            (
                "a sibling member named in prose rather than called",
                format!("// {sibling}camel is the name it writes\n"),
            ),
            (
                "an import of a sibling member, which opens no parenthesis",
                format!("use {sibling}model::Catalog;\n"),
            ),
        ] {
            assert!(
                the_workspace_calls_of(&code).is_empty(),
                "`{code}` is not a call into this workspace ({why}) and the census \
                 reports one, so the rule fires on this module's own prose"
            );
        }
    }

    /// A binding of a function of this workspace is read as one, and a call is
    /// not.
    ///
    /// The floor under [`nothing_binds_a_workspace_call`], in the shape
    /// [`a_reach_into_this_workspace_is_read_however_it_is_spelled`] has: a
    /// census that reported nothing would be satisfied by any binding, and one
    /// that reported every path would fire on this module's own imports. The
    /// third case of the first list is the one the negative case
    /// `a path that opens no parenthesis` is about — it is correctly no *call*,
    /// and it is exactly what this reads.
    #[test]
    fn a_binding_of_a_workspace_function_is_read_as_one() {
        // Assembled, or this test's own text is a binding site.
        let root = ["cra", "te::"].join("");
        let up = ["sup", "er::"].join("");
        let sibling = ["nc_catalog", "_compiler::"].join("");
        for (why, code, resolved) in [
            (
                "the `let` that hands a function on, which the call census drops",
                format!("let go = {root}docs::run;\n"),
                format!("{root}docs::run"),
            ),
            (
                "the same binding under the parent spelling",
                format!("let go = {up}docs::run;\n"),
                format!("{root}docs::run"),
            ),
            (
                "a function put in an array rather than assigned",
                format!("let _ = [{root}docs::run].map(|go| go(root));\n"),
                format!("{root}docs::run"),
            ),
            (
                "a sibling member handed to something that calls it",
                format!("let _ = Some({sibling}camel).map(|go| go(\"probe\"));\n"),
                format!("{sibling}camel"),
            ),
        ] {
            assert_eq!(
                the_workspace_bindings_of(&code),
                vec![resolved.clone()],
                "`{code}` binds `{resolved}` ({why}) and the census walks past it, so the \
                 name it is bound to is a reach `MAY_REACH` can never refuse"
            );
        }
        for (why, code) in [
            (
                "a call, which the other half reads",
                format!("{root}docs::run(root)?;\n"),
            ),
            (
                "an import, which `MAY_IMPORT` decides",
                format!("use {root}ui;\n"),
            ),
            (
                "an import the formatter wrapped across lines",
                format!("use {root}ui::{{\n    ok,\n}};\n"),
            ),
            (
                "a citation rather than code",
                format!("// see {root}check::run\n"),
            ),
            (
                "ordinary code",
                "let root = repository_root();\n".to_owned(),
            ),
        ] {
            assert!(
                the_workspace_bindings_of(&code).is_empty(),
                "`{code}` binds nothing ({why}) and the census reports one, so the rule \
                 fires on this module's own imports"
            );
        }
    }

    /// A visibility in front of an import does not hide it from the census.
    #[test]
    fn a_re_export_is_read_as_the_import_it_is() {
        for spelled in [
            "use std::fs as disk;",
            "pub use std::fs as disk;",
            "pub(crate) use std::fs as disk;",
            "pub(super) use std::fs as disk;",
            "pub(in crate::release) use std::fs as disk;",
            "    pub(crate) use std::fs as disk;",
        ] {
            let read = the_imports_of(&format!("{spelled}\n"));
            assert_eq!(
                read, "use std::fs as disk;\n",
                "`{spelled}` was not read as the import it is, so the alias rule in \
                 `the_gate_runs_before_the_first_write` holds against nothing"
            );
        }
        // And a name that merely opens with those three letters is not one.
        assert_eq!(the_imports_of("published = 1;\n"), "");
        assert_eq!(the_imports_of("pubuse std::fs;\n"), "");
    }

    /// The restore puts the lockfile's own bytes back, and not some of them.
    ///
    /// **What held it was a count and a string literal.** One test asserts how
    /// many `fs::write(` sites this module ships; another asserts the failure
    /// message contains a sentence. Neither reads what is written, and the test
    /// named for the restore never enters this path — it drives `rewrite_listed`
    /// twice by hand against a scratch tree with no `Cargo.lock` in it. So the
    /// restore could write anything: measured in wave 16 as `&locked[..0]`,
    /// which leaves all 627 tests green and truncates the lockfile to nothing
    /// after a failed `cargo update`, in a repository with no git.
    #[test]
    fn the_lockfile_comes_back_byte_for_byte() {
        // **A file, not a directory.** A scratch *tree* is a thing this
        // repository counts and guards — `BUILDS_A_SCRATCH` and its two
        // censuses — and this test needs one path, so it builds none.
        let lockfile =
            std::env::temp_dir().join(format!("nc-release-restore-{}.lock", std::process::id()));
        // Bytes a truncation, a trim and a re-encoding each change.
        let locked: Vec<u8> = b"# pristine\n[[package]]\nname = \"nc-log\"\n\n\t \r\n".to_vec();
        std::fs::write(&lockfile, b"whatever a failed update left").expect("writable");
        super::put_the_lockfile_back(&lockfile, &locked).expect("the restore writes");
        let back = std::fs::read(&lockfile).expect("readable");
        let _ = std::fs::remove_file(&lockfile);
        assert_eq!(
            back, locked,
            "the restore is the only thing that puts `Cargo.lock` back, and it put \
             back something else"
        );
    }

    /// Where the release verb calls the gate.
    fn the_gate_is_called(body: &str) -> usize {
        body.find("crate::check::run(root, false)")
            .expect("the gate is run from the release verb")
    }

    /// The warning prints before the window it warns about opens.
    ///
    /// The eighteen-line paragraph above that line says what it is for — "One
    /// line printed *before* the window is what makes that recoverable by
    /// somebody who has not read this file" — and the whole of its value is its
    /// position. Moving the call below the window leaves the sentence in place
    /// and the tree green, measured by a reviewer in wave 17: a rule whose
    /// value is where it sits, held by nothing about where it sits.
    fn the_warning_prints_before_the_window(body: &str) {
        let said = body
            .find(&["ui::detail(", "\n        \"rewriting Cargo.lock"].join(""))
            .expect("the release warns before it rewrites the lockfile");
        let window = body
            .find(&["relock(", "root)"].join(""))
            .expect("the release relocks");
        assert!(
            said < window,
            "the line warning that `Cargo.lock` is being rewritten prints after the \
             rewrite has already begun, so an interrupted release leaves a tree \
             nobody was told how to recover"
        );
    }

    /// The lockfile is read before the first write, which is the whole repair.
    ///
    /// The six-line comment above that statement records the defect as measured:
    /// the read "fired *after* `rewrite_listed`, so
    /// `Cargo.lock could not be read … Nothing was written` printed over a
    /// `Cargo.toml` already at the new version, in a repository with no git".
    /// Only `gate < write` was pinned, so moving the read below the write left
    /// every test green — measured by a reviewer in wave 16. Assembled for the
    /// same reason the needle beside it is: this scan reads the file it is in.
    fn the_lockfile_is_read_before(body: &str, write: usize) {
        let read = body
            .find(&["std::fs::read(root.join(\"Cargo", ".lock\"))"].join(""))
            .expect("the release reads the lockfile it rewrites");
        assert!(
            read < write,
            "`Cargo.lock` is read after the first write, so a release that cannot \
             read it prints `Nothing was written` over a manifest already at the \
             new version, in a repository with no git to undo it"
        );
    }

    /// The gate runs before anything is written.
    ///
    /// **A source assertion, and it says so.** Running the real thing means
    /// running the whole gate and then rewriting the manifest of the tree the
    /// test is running in, which is not something a test may do in a repository
    /// with no git. What can be checked is the order of the two calls in `run`,
    /// which is the whole of the rule — and deleting the gate call left every
    /// other test in this module green.
    ///
    /// What this does not prove: that `check::run` checks anything. That is
    /// `check`'s own suite. What it proves is that a release cannot write a
    /// version and *then* discover the tests are red, in a tree with no undo.
    #[test]
    fn the_gate_runs_before_the_first_write() {
        let source = std::fs::read_to_string(root().join("xtask/src/release.rs"))
            .expect("this file is readable");
        let body = source
            .split_once("pub(crate) fn run(")
            .expect("run is here")
            .1;
        let gate = the_gate_is_called(body);
        let write = body
            // **The write, not the restore.** Both call `rewrite_listed(root,`,
            // and the restore is 86 lines below — so this needle was satisfied
            // by the wrong one, and the write could be deleted with the gate
            // still "before" a call that only runs when a release has already
            // failed. The version being moved *to* is what tells them apart.
            // **Assembled, or this scanner is the thing it is looking for.**
            // The needle is a line of code in the file the scan reads, and it
            // sits below `run`, so a written-out needle matches itself and the
            // pin passes with the call deleted.
            .find(&["rewrite_listed(root, &CARRIES_THE", "_VERSION, version)"].join(""))
            .expect("the release writes through rewrite_listed");
        assert!(
            gate < write,
            "the gate is called after the first write, so a red tree would be \
             rewritten before anyone found out"
        );
        // **And the lockfile is read before the first write too.** The six-line
        // comment above that statement records the defect as measured and
        // repaired — the read "fired *after* `rewrite_listed`, so
        // `Cargo.lock could not be read … Nothing was written` printed over a
        // `Cargo.toml` already at the new version, in a repository with no
        // git". The order is the whole of that repair and only `gate < write`
        // was pinned: moving the read below the write left every test green,
        // measured by a reviewer in wave 16. Assembled for the same reason the
        // needle above is.
        the_lockfile_is_read_before(body, write);
        the_warning_prints_before_the_window(body);
        // **What this module *ships*, which is the code above the tests and
        // the code below them.** Both assertions here used to read only the
        // half above, cut at the last `mod tests {`: a `pub(crate) fn` appended
        // at the foot of the file spawning a process and writing a file passed
        // both, measured. The write count was the worse of the two, because it
        // added the tests' own writes to its expected total — so a write below
        // the split incremented both sides and cancelled.
        //
        // Four attempts at a boundary did not bind before this one, and the
        // sequence is recorded on [`crate::gates::the_test_modules`],
        // whose whole job this is: the braces are matched in a text where every
        // comment and every literal has been spent character for character, so
        // a `{` in a string and a `'{'` in a char literal are not braces and an
        // offset taken from it is an offset into this file.
        let tests = crate::gates::the_test_modules(&source)
            .expect("every module of tests in this file closes its braces");
        let mut ran = String::new();
        let mut shipped_from = 0;
        for region in &tests {
            ran.push_str(&source[shipped_from..region.start]);
            shipped_from = region.end;
        }
        ran.push_str(&source[shipped_from..]);
        // **And the cut took the tests and only the tests.** A boundary that
        // missed a module would leave that module's `Command`s and writes in
        // `ran` and fail loudly below; a boundary that took too much would hide
        // shipped code, and nothing below would notice. This file has two
        // modules of tests — `mod tests` and `mod relock_tests` — and the
        // `rfind` this replaced cut at the first, leaving the second's four
        // hundred lines counted as shipped and the code below *it* read by
        // nothing at all.
        assert!(
            !ran.contains(&["#[te", "st]"].join("")),
            "a module of tests was not cut out, so the counts below are about the \
             tests as well as about what this module ships"
        );
        assert!(
            tests
                .iter()
                .all(|region| source[region.clone()].contains(&["mod", " "].join(""))),
            "something that is not a module was cut out of the shipped code"
        );
        // And nothing writes before either of them: `run` reads, validates and
        // gates, and `rewrite_listed` is the only thing in this module that
        // calls `std::fs::write`.
        //
        // **The spellings this list names, which is not the same as every
        // spelling.** `ran.matches("std::fs::write(").count()` was a single
        // literal needle, and a file arrives on disk through `copy`, `rename`,
        // `remove_file`, `File::create(..).write_all(..)` and `OpenOptions`
        // too — none of which is that text. Measured: a `fs::copy` of
        // `clippy.toml` to `probe.txt` at the top of `run`, before the gate,
        // wrote into the root of this repository and left all thirteen tests in
        // this module green.
        //
        // **And the list that replaced it called itself "every spelling", which
        // it was not.** `File::create_new`, `File::options`, `write!` and
        // `writeln!` are each a file on disk and none was here — measured, two
        // of them writing into `temp_dir()` ahead of the gate with `13 passed`.
        // They are named below now; what the sentence claims is what the list
        // holds, so the next spelling that is missing is a line added here
        // rather than a promise this paragraph already made.
        //
        // Written without a `std::` prefix, so a `use std::fs;` spelling is the
        // same match, and written here rather than beside the code because
        // everything in this module is cut out of `ran` before it is read.
        //
        // **And a rename is what makes any of it blind.** A list of spellings
        // cannot resolve a name: `use std::fs as disk;` and then `disk::write(…)`
        // is `std::fs::write` to rustc and matches nothing here — measured, a
        // copy of `clippy.toml` written into the root of this repository ahead
        // of the gate with all thirteen tests in this module green. The list
        // below cannot be extended to cover it, because the alias is a word the
        // author chooses. So the shipped half of this module is held to
        // importing nothing under another name at all: the census then reads
        // the spellings rustc reads, and adding an alias is a failure here
        // rather than a hole opened silently.
        //
        // The whole `use` statement, joined, because `use std::fs\n    as disk;`
        // is the same import and rustfmt is free to wrap a long one.
        let imports = the_imports_of(&ran);
        assert!(
            !imports.contains(" as "),
            "this module imports something under another name, and every rule below is \
             a list of spellings: `use std::fs as disk;` makes `disk::write(…)` a write \
             none of them can see. The imports are:\n{imports}"
        );
        nothing_renames_or_reaches_past_the_gate(&ran, &imports);
        // The floor under that: a filter that read no `use` statement would
        // satisfy the rule with any alias in place. This module imports
        // `std::path::Path` and `crate::ui` and nothing else.
        assert!(
            imports.contains("use std::path::Path"),
            "the shipped half of this module reads as having no imports at all, so the \
             rule above holds against nothing: {imports}"
        );
        let puts_bytes_on_disk = [
            "fs::write(",
            "fs::copy(",
            "fs::rename(",
            "fs::remove_file(",
            "fs::remove_dir(",
            "fs::remove_dir_all(",
            "fs::create_dir(",
            "fs::create_dir_all(",
            "fs::hard_link(",
            "fs::soft_link(",
            "fs::set_permissions(",
            "fs::DirBuilder",
            "File::create(",
            "File::create_new(",
            "File::options(",
            "OpenOptions",
            "write_all(",
            "write!(",
            "writeln!(",
            "symlink(",
        ];
        let mut on_disk = 0;
        for needle in puts_bytes_on_disk {
            let found = ran.matches(needle).count();
            assert!(
                found == 0 || needle == "fs::write(",
                "`{needle}` puts bytes on disk, and this module writes through \
                 `rewrite_listed` and nothing else: {found} call site(s) outside it"
            );
            on_disk += found;
        }
        assert_eq!(
            on_disk,
            // One in `rewrite_listed`; one putting `Cargo.lock` back when the
            // relock fails; and one putting every file this run touched back
            // when the read-back finds the rewrite missed the version — both
            // restores are *after* the gate, and both exist because a refusal
            // that leaves a rewritten file behind is the state this module's
            // header says it prevents, in a tree with no git. The scratch-tree
            // writes in the tests are not counted at all now, because the tests
            // are not in `ran`.
            3,
            "something outside `rewrite_listed` writes a file"
        );
        // **And what it runs, not only what it writes.** This module's header
        // promises it prints the commit and the tag and never runs them, so a
        // version is reviewed before it becomes history. Nothing held
        // it: four lines putting `git tag -s` inside the existing `relock` left
        // `check passed` with every count unchanged. A new *function* was
        // caught, but only by the doc-coverage ratchet, which is not a rule
        // about git and does not fire when the call goes inside a function that
        // already exists.
        assert_eq!(
            ran.matches("Command").count(),
            1,
            "this module runs one child process — `cargo` for the relock. The commit \
             and the tag are printed, not run, so the version is reviewed before it \
             becomes history"
        );
        // Named through `$CARGO` with a `"cargo"` fallback, so the assertion is
        // about the binding rather than about a literal.
        assert!(
            ran.contains("Command::new(program)")
                && ran.contains("std::env::var(\"CARGO\")")
                && ran.contains(".args([\"update\", \"--workspace\"])"),
            "the one child process this module runs is no longer the cargo relock"
        );
    }

    /// And the verb refuses it, not only the helper.
    ///
    /// **`--dry-run` runs no gate and writes nothing**, which makes it the one
    /// way to drive `run` itself from a test. Without this, `moves_forward`
    /// could be correct and never called — which is what every other finding in
    /// this module has been.
    #[test]
    fn the_verb_refuses_a_release_that_does_not_move_forward() {
        // **Bracketed, because the two calls below drive the release verb
        // against the repository this test is running in.** Nothing here said
        // anything about that tree, so a write inside `run` landed in it and
        // this test reported a pass — measured, a file in the root of a
        // checkout with no git to take it back out. A listing either side is
        // the assertion that `--dry-run` means what it says here as well as on
        // the scratch above.
        let before = every_file_under(root());

        let at = version_in(
            &std::fs::read_to_string(root().join(CARRIES_THE_VERSION[0]))
                .expect("the manifest is readable"),
        )
        .expect("it names a version");
        let refused = super::run(root(), &at, true).expect_err("a repeat is not a release");
        assert!(
            refused.contains("not ahead of it"),
            "re-releasing the version this tree is at was accepted: {refused}"
        );
        // And a version that is ahead is accepted, so the refusal above is
        // about the direction and not about `--dry-run`.
        let ahead = format!(
            "{}.{}",
            at.rsplit_once('.').expect("three parts").0,
            at.rsplit('.')
                .next()
                .and_then(|last| last.parse::<u32>().ok())
                .expect("a number")
                + 1
        );
        super::run(root(), &ahead, true).expect("a version ahead of this one");

        assert_eq!(
            changed_between(&before, &every_file_under(root())),
            Vec::<String>::new(),
            "the release verb was driven twice against this repository and changed it. \
             There is no git here (§5.3), so whatever it did is what the tree is now"
        );
    }

    #[test]
    fn a_release_goes_forwards_only() {
        assert!(moves_forward("0.1.0", "0.2.0"));
        assert!(moves_forward("0.1.0", "0.1.1"));
        assert!(moves_forward("0.9.9", "1.0.0"));
        // A repeat prints `git tag` for a tag that already names something else.
        assert!(!moves_forward("0.1.0", "0.1.0"));
        assert!(!moves_forward("0.2.0", "0.1.0"));
        assert!(!moves_forward("1.0.0", "0.9.9"));
        // Compared as numbers, not as text: `0.10.0` is ahead of `0.9.0`.
        assert!(moves_forward("0.9.0", "0.10.0"));
        // **A version this tool cannot order is refused, not guessed at.**
        // `filter_map` dropped the part it could not read and compared what was
        // left, so `0.1.0-rc.1` read as `[0, 1]` — ordering against nothing
        // sensibly. Ordering a pre-release correctly is semver's job.
        assert!(!moves_forward("0.1.0-rc.1", "0.1.0"));
        assert!(!moves_forward("0.1.0", "0.1.1-rc.1"));
        assert!(!moves_forward("0.1.x", "0.2.0"));
        assert!(!moves_forward("0.1", "0.2.0"));
    }

    #[test]
    fn a_version_cargo_would_refuse_is_refused_here_first() {
        // `"01".parse::<u32>()` is `Ok(1)`, so this validated — and the write
        // then left a manifest nothing in the workspace could parse, `xtask`
        // included, in a repository with no git to undo it.
        for cargo_refuses in ["01.02.03", "0.01.0", "1.0.00", "+1.0.0", "1.0.0 "] {
            let error = validate(cargo_refuses).expect_err("cargo would refuse `{cargo_refuses}`");
            assert!(error.contains("three numbers"), "{cargo_refuses}: {error}");
        }
        assert!(validate("0.10.0").is_ok(), "a two-digit part is ordinary");
        assert!(validate("10.0.0").is_ok());
    }

    #[test]
    fn three_numbers_is_a_version() {
        assert!(validate("0.1.0").is_ok());
        assert!(validate("12.0.345").is_ok());
    }

    #[test]
    fn anything_else_is_not_a_version_and_the_message_says_what_is() {
        for attempt in ["0.1", "0.1.0.0", "v0.1.0", "0.1.0-rc1", "", "a.b.c"] {
            let error = validate(attempt).expect_err("should be rejected");
            assert!(
                error.contains("three numbers"),
                "the message has to say what a version looks like: {error}"
            );
        }
    }

    #[test]
    fn a_manifest_yields_its_version_and_a_file_without_one_yields_nothing() {
        assert_eq!(
            version_in("[workspace.package]\nversion = \"0.1.0\"\n").as_deref(),
            Some("0.1.0")
        );
        // The first one wins, which is the workspace's — a member's
        // `version.workspace = true` carries no literal to be confused with it.
        assert_eq!(
            version_in("version = \"0.1.0\"\nversion = \"9.9.9\"\n").as_deref(),
            Some("0.1.0")
        );
        assert_eq!(version_in("edition = \"2024\"\n"), None);
        assert_eq!(version_in("version.workspace = true\n"), None);

        // **A dependency pinned above the workspace's own table does not win.**
        // This took the first trimmed line carrying the key, whatever table it
        // was in, so a release rewrote *that* — corrupting a requirement, and
        // reporting `1 file(s) carry 0.2.0` for a release that did not happen.
        // The read-back added for the comment case could not see it, because it
        // re-read through this same function and agreed with itself.
        let decoy = "[workspace.dependencies.house-crate]\nversion = \"0.0.1\"\n\
                     path = \"crates/house\"\n\n[workspace.package]\nversion = \"0.1.0\"\n";
        assert_eq!(version_in(decoy).as_deref(), Some("0.1.0"));

        // **And the value is read as TOML writes it.** Both of these are
        // manifests cargo parses, and both answered `None` — which is
        // `Cargo.toml names no version` about a file that names one.
        assert_eq!(
            version_in("[workspace.package]\nversion=\"0.1.0\"\n").as_deref(),
            Some("0.1.0")
        );
        assert_eq!(
            version_in("[workspace.package]\nversion = \"0.1.0\" # keep this\n").as_deref(),
            Some("0.1.0")
        );

        // **And TOML's other string form**, which is the third time this rule has
        // been too narrow. `version = '0.1.0'` is a literal string; cargo parses
        // it like any other — `cargo metadata` on a scratch manifest answers
        // `"version":"0.1.0"` — and this answered `None`, so the whole gate went
        // red on a tree cargo builds and `release` refused a manifest naming the
        // version it holds. TOML has exactly two string forms, so these are all
        // of them rather than the next list somebody guesses.
        for said in [
            "[workspace.package]\nversion = '0.1.0'\n",
            "[workspace.package]\nversion='0.1.0'\n",
            "[workspace.package]\nversion = '0.1.0' # keep this\n",
        ] {
            assert_eq!(
                version_in(said).as_deref(),
                Some("0.1.0"),
                "a manifest cargo parses answered no version: {said}"
            );
        }
        // And a quote of one kind does not close a string opened with the other.
        assert_eq!(
            version_in("[workspace.package]\nversion = '0.1.0\"\n"),
            None
        );
    }

    #[test]
    fn every_file_listed_as_carrying_the_version_actually_carries_one() {
        // The list is what makes a release refuse rather than silently leave a
        // file behind, and a list naming a file that holds no version would make
        // every release fail for a reason nobody put there.
        assert!(!CARRIES_THE_VERSION.is_empty(), "nothing carries a version");
        // **The needle itself, written out here.** Every assertion below asks
        // `version_in`, which builds its needle from `KEY` — so `KEY` broken to
        // anything a manifest never holds made `version_in` answer `None`
        // everywhere, and the loop below found nothing to be wrong about because
        // its own filter had emptied. Measured: `cargo xtask check` green with
        // the release verb unable to find a version at all.
        //
        // TOML writes it exactly this way, and `version_of` is documented as
        // reading it the way TOML writes it.
        assert_eq!(
            KEY, "version = \"",
            "the key a manifest holds its version under is spelled another way here, and \
             everything below builds its needle from it"
        );
        for name in CARRIES_THE_VERSION {
            let text = std::fs::read_to_string(root().join(name))
                .unwrap_or_else(|error| panic!("{name} is listed and unreadable: {error}"));
            assert!(
                version_in(&text).is_some(),
                "{name} is listed as carrying the version and holds none"
            );
        }

        // **And nothing unlisted carries one**, which is the direction this
        // module's header is actually worried about: "the shape that leaves one
        // file behind on the day somebody adds it". The test above cannot see
        // that day — it only ever reads the files already named.
        //
        // The version this tree is at, looked for everywhere the walk goes.
        // `crates/nc-log/src/lib.rs` already has an `Origin { version }` field,
        // and the first service that fills it from a literal is the file that
        // would be left behind.
        let at = version_in(
            &std::fs::read_to_string(root().join(CARRIES_THE_VERSION[0])).expect("readable"),
        )
        .expect("the manifest names a version");
        // **And the spellings a *writer* would not use.** `SPELLINGS` is the
        // writer's list, and the writer rewrites TOML — but this half asks a
        // different question: which files *name* the version and are not listed
        // as carrying it. `crates/nc-log/src/lib.rs` already has an
        // `Origin { version: Literal }`, and the first service to fill it from a
        // literal writes `version: "0.1.0"`, which no TOML spelling matches.
        // That is the file the module header names as "the one left behind on
        // the day somebody adds it".
        //
        // Reported rather than rewritten: a Rust file naming the version is a
        // decision for a person — either it joins `CARRIES_THE_VERSION` and the
        // writer learns its spelling, or it stops naming the version.
        // **A version closes with the quote it opened with.** This appended a
        // double quote to every spelling, and half of `SPELLINGS` opens with a
        // single one — so four of the eight needles were `version = '0.1.0"`, a
        // string no TOML file can hold. Measured: that impossible text written
        // into a file is found and the real `version = '0.1.0'` is not, so a
        // second copy of the version in single quotes was invisible to this
        // rule while `version_of` reads both forms.
        let mut needles: Vec<String> = SPELLINGS
            .iter()
            .map(|spelling| {
                let closes = if spelling.ends_with('\'') { '\'' } else { '"' };
                format!("{spelling}{at}{closes}")
            })
            .collect();
        // Every spelling a *file* can name the version in, TOML, YAML, JSON and
        // Rust. `crates/nc-log/src/lib.rs` already carries an `Origin { version
        // }`, and the first service to fill it writes one of the last three —
        // measured, `const VERSION: &str = "0.1.0";` in `dev.rs` was invisible
        // to a list of TOML spellings, and `literal!("0.1.0")` is what the type
        // wall forces a service to write.
        for spelling in [
            "version: ",
            "version = ",
            "\"version\": ",
            "VERSION: &str = ",
            "VERSION: &'static str = ",
            "literal!(",
        ] {
            let named = format!("{spelling}\"{at}\"");
            needles.push(named);
        }
        assert!(
            needles.contains(&format!("{KEY}{at}\"")),
            "`KEY` is not one of the spellings the writer knows, so the two halves of \
             this rule are looking for different things"
        );
        // **And every needle is a string a file could hold.** Four of the eight
        // built from `SPELLINGS` were `version = '0.1.0"` — opened with one
        // quote and closed with the other — so half this scan looked for text
        // no TOML file can contain, and a second copy of the version in single
        // quotes was invisible to it while `version_of` reads that form.
        for spelling in SPELLINGS {
            let opens = spelling
                .chars()
                .next_back()
                .expect("a spelling ends in a quote");
            let built = needles
                .iter()
                .find(|needle| needle.starts_with(spelling))
                .unwrap_or_else(|| panic!("`{spelling}` built no needle"));
            assert!(
                built.ends_with(opens),
                "`{built}` opens with `{opens}` and closes with something else, so it is \
                 a string no file can hold and this half of the scan reads nothing"
            );
        }
        let mut carriers = Vec::new();
        // **A floor on the walk.** `carriers.is_empty()` is an assertion over an
        // empty collection, and `if true { return; }` as the first line of this
        // callback left it green — measured. The number read is the number that
        // says the walk happened at all.
        let mut read = 0_usize;
        crate::gates::walk(root(), &mut |path| {
            let relative = crate::relative_to(root(), path);
            if CARRIES_THE_VERSION.contains(&relative.as_str()) || relative == "Cargo.lock" {
                return;
            }
            // This file, which writes the needles out in order to look for them.
            if relative == "xtask/src/release.rs" {
                return;
            }
            let Ok(text) = std::fs::read_to_string(path) else {
                return;
            };
            // **After the judgement, not before it.** `read` is the floor that
            // makes `carriers.is_empty()` mean something, and it stood above the
            // decision — so one `if !relative.starts_with("crates/") { return; }`
            // under it left a real unlisted carrier unreported with the floor
            // satisfied. Measured, a probe file under `docs/` holding the
            // workspace version: `10 passed; 0 failed`.
            let names_it = needles.iter().any(|needle| text.contains(needle.as_str()));
            read += 1;
            if names_it {
                carriers.push(relative);
            }
        })
        .expect("the tree is walkable");
        assert!(
            carriers.is_empty(),
            "these name the workspace version and are not in CARRIES_THE_VERSION, so a \
             release would leave them behind: {carriers:?}"
        );
        assert!(
            read > 100,
            "the walk read {read} file(s), so `carriers.is_empty()` is true of a \
             collection nothing could have been put in"
        );
    }
}

#[cfg(test)]
mod relock_tests {
    /// A failed relock leaves the version where it was.
    ///
    /// **The window between the write and the lockfile.** `relock` runs
    /// `cargo update --workspace`, which is one registry away from the network,
    /// and a failure there left `Cargo.toml` at the new version and `Cargo.lock`
    /// at the old one — in a repository with no git to undo it (`§5.3`).
    /// Measured with a cargo that failed only on `update`:
    ///
    /// ```text
    /// ok    1 file(s) carry 0.3.0
    /// error cargo update refused: …
    /// version after: 0.3.0        lockfile says: 0.2.0
    /// ```
    ///
    /// And the obvious recovery was refused — `this tree is at 0.3.0 and 0.3.0
    /// is not ahead of it` — so the operator had to hand-edit the manifest back
    /// before they could try again.
    ///
    /// Driven over a scratch manifest through the write and the restore, because
    /// running the real verb means running the whole gate twice.
    #[test]
    fn a_failed_relock_puts_the_version_back() {
        let scratch = std::env::temp_dir().join(format!("nc-relock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).expect("writable");
        let manifest = scratch.join("Cargo.toml");
        std::fs::write(
            &manifest,
            "[workspace.package]\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("writable");

        let listed = ["Cargo.toml"];
        super::rewrite_listed(&scratch, &listed, "0.2.0").expect("the write lands");
        assert!(
            std::fs::read_to_string(&manifest)
                .expect("readable")
                .contains("version = \"0.2.0\""),
            "the write did not happen, so the restore below proves nothing"
        );

        // What `run` does when `relock` fails.
        super::rewrite_listed(&scratch, &listed, "0.1.0").expect("the restore lands");
        assert!(
            std::fs::read_to_string(&manifest)
                .expect("readable")
                .contains("version = \"0.1.0\""),
            "the version was not put back, and there is no git here to do it"
        );

        // And the source carries the restore, because the two calls above are a
        // model of `run` and not `run` itself.
        let source =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/release.rs"))
                .expect("this file is readable");
        // **Neither the first `#[cfg(test)]` nor the last.** This cut the file
        // at `rfind`, and the last occurrence here is the string literal on the
        // assertion's own line — so the half it called production was lines
        // 1 to 2280, four test modules included. `gates::without_test_modules`
        // blanks every test module and keeps everything else, and `check.rs`
        // records the two measurements that put it there.
        let body = crate::gates::without_test_modules(&source);
        assert!(
            body.contains("Nothing was released and the version is back at"),
            "`run` no longer restores the version when the relock fails"
        );

        let _ = std::fs::remove_dir_all(&scratch);
    }
}
