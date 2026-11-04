//! Compiles `catalog/` into the crate that carries it.
//!
//! **Production is separated from the write**, and that separation is the only
//! reason the drift check means anything. [`artifacts`] loads and emits entirely
//! in memory, touching nothing; [`run`] writes. A `check` that regenerated
//! before comparing would compare the output to itself and report "in sync"
//! immediately after a hand edit.
//!
//! **rustfmt runs as part of generation, not after it.** `cargo fmt --all`
//! reformats a generated crate that is a workspace member, so the emitter's
//! output and rustfmt's have to agree or the file is permanently both out of
//! sync and badly formatted, with no edit that fixes both.

use std::path::Path;
use std::process::{Command, Stdio}; // allow-nondeterminism: the one `process::Command` this stage runs is rustfmt, and the call site below carries the reason it may

use nc_catalog_compiler::Problem;

use crate::ui;

/// Loads the catalog and emits every file it produces, in memory.
///
/// Touches nothing on disk beyond reading.
pub(crate) fn artifacts(
    root: &Path,
    bootstrap: nc_catalog_compiler::ledger::Bootstrap,
) -> Result<nc_catalog_compiler::Compiled, String> {
    let mut compiled =
        nc_catalog_compiler::compile(root, bootstrap).map_err(|problems| report(&problems))?;
    let catalog = &compiled.catalog;

    ui::ok("compiled catalog/");
    ui::detail(&format!(
        "{} error code(s), {} notification(s), {} email(s), {} command(s), \
         {} log event(s), {} table(s), {} capability(ies), {} locale(s)",
        catalog.errors.len(),
        catalog.notifications.len(),
        catalog.emails.len(),
        catalog.commands.len(),
        catalog.events.len(),
        catalog.limits.tables.len(),
        catalog.capabilities.len(),
        catalog.locales.len(),
    ));

    for file in &mut compiled.files {
        if std::path::Path::new(&file.path)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("rs"))
        {
            file.contents = rustfmt(root, &file.contents)?.into_source();
        }
    }
    Ok(compiled)
}

/// Writes every generated file whose contents changed.
pub(crate) fn run(root: &Path, reserve: bool) -> Result<(), String> {
    use nc_catalog_compiler::ledger::Bootstrap;

    ui::heading("build");
    if reserve {
        // Loud, because it is the one command that accepts a ledger nothing
        // vouches for, and because "I ran it by habit" is how a bypass becomes
        // routine.
        ui::warn("--reserve: a ledger with no checksum will be accepted and rewritten");
        ui::detail(
            "every error code it then holds is reserved permanently. Run this once, on a \
             new catalog — never to get past a checksum that stopped matching",
        );
    }
    let compiled = artifacts(
        root,
        if reserve {
            Bootstrap::Allow
        } else {
            Bootstrap::Refuse
        },
    )?;
    // **What this build reserves, said on the line it reserves it.** A code
    // added to `catalog/errors.toml` is appended to the ledger by an ordinary
    // `cargo xtask build`, permanently, and the run said `15 generated file(s),
    // 4 rewritten` — an irreversible write announced as a file count, in a tree
    // with no history to restore from. A newcomer trying out a number learns
    // what happened one build later, when renumbering it is refused.
    //
    // **And the reading is inside the write, because it is a reading of the
    // before-state.** As `let reserved = newly_reserved(…)` on the line above
    // the call, the two statements can be swapped with the whole suite green,
    // and every build that burns a code then announces *"reserving nothing
    // new"* and names none of them — because the ledger on disk **is** the
    // ledger just written, so a check inspecting its own fresh output can only
    // ever find nothing. `run` is reached by no test at all, so nothing would
    // say so. Inside the write there is no statement to move.
    let (written, reserved) = write_what_changed(root, &compiled.files, &compiled.ledger)?;
    ui::ok(&what_a_run_wrote(compiled.files.len(), written));
    if let Some(reserved) = reserved {
        // **After the write, because it is a claim about a write.** Run before
        // the loop, a build that then fails — a generated file made read-only
        // is enough — prints `reserved NC-VAL-0002 — permanently` and reserves
        // nothing: measured, `grep -c NC-VAL-0002
        // catalog/errors.reserved.toml` comes back `0`. `AGENTS.md` states the
        // contract in as many words: "announces each code it reserves, **on the
        // line it reserves it**". A developer told a number was burned, in a
        // tree with no history to check it against, goes looking for the undo —
        // which begins by blanking the ledger's checksum.
        for code in &reserved {
            ui::ok(&format!("reserved {code} — permanently"));
            ui::detail(
                "a reserved code is never freed: every log line ever written with it \
                 still refers to it. If this was a number you were trying out, it is not \
                 too late yet — the fix on the refusal you will get for renumbering it \
                 says how",
            );
        }
        ui::detail(&what_the_ledger_write_was(&reserved));
    }
    Ok(())
}

/// What to say about a run that rewrote `catalog/errors.reserved.toml`.
///
/// **A count of ledger writes says `1 appended … which is permanent` for a
/// write that appends nothing.** The ledger holds a header as well as its
/// reservations — so with such a counter, editing a comment in `ledger.rs` and
/// rebuilding announces a permanent reservation, twice, with the set of codes
/// unchanged both times. Measured with such a counter, on a ledger of 28 codes:
///
/// ```text
/// cargo xtask build   ->  ok  15 generated file(s), 0 rewritten
///                             and 1 appended to catalog/errors.reserved.toml, which is permanent
/// grep -c '^"NC-' catalog/errors.reserved.toml   ->  28   (28 before, 28 after)
/// ```
///
/// A developer reading that has been told a number was burned. In a tree with
/// no history to check it against, the next thing they do is try to take it
/// back — and the refusal they get names a code this run never reserved.
///
/// Phrased from `newly_reserved`, which is the same list the lines above are
/// printed from, so the count and the names cannot disagree.
fn what_the_ledger_write_was(reserved: &[String]) -> String {
    if reserved.is_empty() {
        "and catalog/errors.reserved.toml was rewritten, reserving nothing new".to_owned()
    } else {
        format!(
            "and {} code(s) appended to catalog/errors.reserved.toml, which is permanent",
            reserved.len()
        )
    }
}

/// Every error code the ledger's `[reserved]` block names.
///
/// The ledger writes `"NC-AREA-NNNN" = "<key> — <description>"`, one per line.
/// Read as text rather than parsed, because what is wanted is the set of names
/// and a TOML round trip would be a second opinion about a file the compiler
/// already rendered.
///
/// **One clause, because a second could be decided by no input.** A quote half
/// beside it, as in `starts_with('"') && line.contains(" = ")`, cannot be
/// decided by anything, because the condition under which it is false is
/// exactly the condition under which the `strip_prefix('"')` below drops the
/// line — one rule written twice, two lines apart, with the second copy doing
/// the work. Measured on that two-clause filter against a corpus with no
/// quoted line lacking a ` = `, deleting either half leaves the suite green.
///
/// The clause here is the one that does decide: a quoted line with no ` = ` is
/// not a key and a build must not make it permanent. `next = 30` and
/// `# a note = with an equals in it` are not decided by it — they have no
/// leading quote, so `strip_prefix('"')` drops them whatever it says — so the
/// corpus below drives it with a line that does.
fn the_codes_reserved_in(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| line.contains(" = "))
        .filter_map(|line| line.strip_prefix('"'))
        .filter_map(|rest| rest.split_once('"'))
        .map(|(code, _)| code.to_owned())
        .collect()
}

/// The codes this build is about to add to the ledger, in the ledger's order.
///
/// Read from the two renderings rather than asked of the compiler: `reconcile`
/// returns the whole reservation set and not the difference, and what a reader
/// needs is what *this run* makes permanent.
fn newly_reserved(root: &Path, ledger: &nc_catalog_compiler::EmittedFile) -> Vec<String> {
    let before = std::fs::read_to_string(root.join(&ledger.path)).unwrap_or_default();
    // The ledger writes `"NC-AREA-NNNN" = "<key> — <description>"` under
    // `[reserved]`, one per line. Read as text rather than parsed, because what
    // is wanted is the set of names and a TOML round trip would be a second
    // opinion about a file the compiler already rendered.
    let already = the_codes_reserved_in(&before);
    the_codes_reserved_in(&ledger.contents)
        .into_iter()
        .filter(|code| !already.contains(code))
        .collect()
}

/// Renders the compiler's problems the way `NOCTUA.md §15.1` asks: located, and
/// ending with what to do.
fn report(problems: &[Problem]) -> String {
    for problem in problems {
        ui::failure(&problem.to_string());
        ui::detail(&problem.fix);
    }
    format!("the catalog has {} problem(s)", problems.len())
}

/// Pipes source through rustfmt.
///
/// `--edition` is explicit because the input arrives on stdin and has no
/// manifest for rustfmt to infer it from; without it a 2024 construct is
/// reformatted as 2015 or refused.
///
/// **And it runs *in the repository*.** rustfmt discovers its configuration by
/// walking up from its working directory, and a child left where it is
/// inherits whoever's shell started `xtask` — while `xtask` itself takes its
/// root from the manifest, deliberately, so the two disagree the moment
/// somebody runs the verb from anywhere else. `check`'s
/// `the_formatter_config_stops_here` asks whether `rustfmt.toml` is at the root,
/// which says nothing about where the walk starts. Measured with the child not
/// moved, same binary, same tree, a `rustfmt.toml` with `max_width = 60` one
/// directory above the workspace:
///
/// ```text
/// from the repository root  ->  ok  15 generated file(s), 0 rewritten
/// from /tmp                 ->  ok  15 generated file(s), 6 rewritten
/// ```
///
/// With `newline_style = "Windows"` instead, nine files are rewritten to CRLF
/// and `cargo fmt --all --check` passes — rustfmt's default is `Auto`, which
/// preserves what a file already has. None of the three determinism gates sees
/// either one: they compare two generations *in the same process*, in the same
/// directory.
fn rustfmt(root: &Path, source: &str) -> Result<checked::Formatted, String> {
    #[expect(clippy::disallowed_methods, reason = "a tool this gate drives; its output is read, not printed")]
    let mut child = Command::new("rustfmt") // allow-nondeterminism: this `Command::new` runs rustfmt, whose components rust-toolchain.toml names — the channel there is a moving one, so the version is not pinned — and which is configured at the root, which the line below is what makes true
        .args(["--edition", "2024", "--emit", "stdout", "--quiet"])
        .current_dir(root) // allow-nondeterminism: this `current_dir` is the fix — rustfmt walks up from where it stands, and standing at the root is what stops it leaving the repository
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            format!("could not run rustfmt: {error}. It is a component of the pinned toolchain, so `rustup show` installs it")
        })?;
    {
        use std::io::Write as _;
        let stdin = child.stdin.as_mut().ok_or("rustfmt took no stdin")?;
        stdin
            .write_all(source.as_bytes())
            .map_err(|error| format!("could not write to rustfmt: {error}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("rustfmt did not finish: {error}"))?;
    if !output.status.success() {
        // **Two sentences, because there are two failures.** rustfmt refuses
        // either because the emitted source will not parse, or because it is not
        // installed — and the second is not the emitter's fault at all. Written
        // as one, the message ends with the *parse* advice after a refusal that
        // has nothing to do with positions, and rustfmt's own stderr is
        // concatenated onto a sentence that then opens with a bare `.`:
        //
        // ```text
        // error rustfmt refused the emitted source: error: 'rustfmt' is not
        //       installed for the toolchain 'stable-x86_64-unknown-linux-gnu'
        //       . Its positions are offsets into a stream, not into a file — …
        // ```
        //
        // Measured with rustfmt off `PATH`, which is the state a CI job is in
        // when it installs the toolchain without `components: rustfmt, clippy`
        // — and this runs rustfmt from `PATH` on every generated file.
        // `AGENTS.md` requires a refusal to end with its fix, and a single
        // sentence gives one of these two none.
        // Two early returns rather than one `Err(if … {} else {})`, because a
        // sentence built inside a conditional expression is one
        // `WHAT_THE_GATE_CAN_SAY` cannot read: its reader takes the literal that
        // opens the call, and what opens that one is `if`.
        let said = String::from_utf8_lossy(&output.stderr);
        let said = said.trim();
        if said.contains("is not installed") || said.contains("No such file") {
            return Err(format!(
                "rustfmt is what turns the emitted source into the bytes that reach disk, \
                 and it is not here: {said}. `rust-toolchain.toml` asks for it as a \
                 component, and a runner that installed the toolchain another way has to \
                 ask for it too"
            ));
        }
        return Err(format!(
            "rustfmt refused the emitted source: {said}. Its positions are offsets into a \
             stream, not into a file — the name that produced this is what to look for in \
             the registry"
        ));
    }
    let formatted = String::from_utf8(output.stdout)
        .map_err(|error| format!("rustfmt returned invalid UTF-8: {error}"))?;
    // **The checked source is the only source there is.** Written as
    // `check_formatted(&formatted)?;` on its own line above an `Ok(formatted)`,
    // the check is discardable: `let _ = check_formatted(…)` compiles, changes
    // no verdict anywhere, and hands every caller rustfmt's output unread — and
    // unit tests of the guard cannot tell, because they do not reach its call
    // site.
    //
    // Handing the string back *through* the check is not a better-tested version
    // of the same shape; it is a different shape. There is no `formatted` to
    // return except the one the check returned, so a caller that drops the
    // verdict has nothing left to give back and does not compile.
    checked::Formatted::new(formatted)
}

/// Whether what is on disk is already these bytes.
///
/// **Named so it can be driven.** Written inline in the loop it guards, the
/// decision would be reachable by nothing: the `continue` replaced with a
/// `false` would leave every suite green, because what it costs is a
/// modification time rather than a wrong byte. That is a real cost — an
/// identical rewrite wakes a file watcher, and a watcher woken by its own
/// tool's writes is a loop that never settles — and it is the kind a test has to
/// be written for on purpose, because nothing else will notice.
///
/// A file that is not there yet, or that this process cannot read, is not
/// already written: both answer `false` and the write goes ahead, which is the
/// direction to be wrong in.
fn already_written(path: &Path, contents: &str) -> bool {
    std::fs::read_to_string(path).is_ok_and(|on_disk| on_disk == contents)
}

/// Write the generated files that differ, put the ledger in place, and say how
/// many of each.
///
/// **Lifted because nothing drives [`run`].** `run` has one caller and no test,
/// and `check` reaches only [`artifacts`] — so a write loop inside `run` would
/// be held by reading alone. Measured with the loop inside `run`, each alone,
/// with the whole suite and `cargo xtask check` green after every one: the
/// ledger counted among the generated files (`written += 1` beside its
/// branch); the announcement made to read a hand-tallied counter instead of
/// the list; and the list of files to write emptied outright, so a run would
/// report nothing rewritten and write nothing.
///
/// Computing the count from the list rather than tallying it, inside `run`,
/// leaves all three green, because the problem there is not which expression
/// the number comes from — it is that no test reaches the function at all.
/// What closes it is a seam a test can hold, which is this.
///
/// The two counts are returned rather than printed, because a number a function
/// prints is a number only a person can check.
fn write_what_changed(
    root: &Path,
    files: &[nc_catalog_compiler::EmittedFile],
    ledger: &nc_catalog_compiler::EmittedFile,
) -> Result<(usize, Option<Vec<String>>), String> {
    let mut written = 0_usize;
    for file in files {
        // See [`what_a_run_would_write`]: this decision is the one that says
        // whether the verb writes anything, and inverted it writes nothing —
        // and the counts `the_ledger_is_counted_apart_from_the_generated_files`
        // asserts then fail.
        if what_a_run_would_write(root, [(file.path.as_str(), file.contents.as_str())]).is_empty() {
            // An identical rewrite bumps the modification time, which is enough
            // to wake a file watcher and start a loop that never settles.
            continue;
        }
        let path = root.join(&file.path);
        make_room_for(&path)?;
        std::fs::write(&path, &file.contents).map_err(|error| {
            // **The clause only when it is true.** `written` counts files
            // actually rewritten, so when the first differing file is the one
            // that fails, an unconditional clause asserts an intermediate state
            // that does not exist — at the moment a developer is deciding
            // whether it is safe to stop.
            let part_way = if written > 0 {
                format!(
                    " — {written} generated file(s) were rewritten before it, so this \
                     tree is part-way through a build"
                )
            } else {
                " — nothing was rewritten before it, so the tree is as it was".to_owned()
            };
            format!(
                "could not write {}: {error}{part_way}. Run `cargo xtask build` again \
                 once that path is writable",
                file.path
            )
        })?;
        written += 1;
    }
    // **The ledger after them, and counted apart.** Chained onto the generated
    // files with a branch inside, one counter answers two questions — a run
    // that rewrites everything prints one more rewritten than there are files,
    // and a run that rewrites only the ledger prints `1 rewritten` with every
    // generated file byte-identical. Both measured. The second is the one that
    // matters: an append to the append-only ledger, permanent and the one
    // write here that cannot be undone, announced as a generated file
    // changing.
    if what_a_run_would_write(root, [(ledger.path.as_str(), ledger.contents.as_str())]).is_empty() {
        return Ok((written, None));
    }
    // **Read here, one statement above the write it is a reading of.** Which
    // codes this run makes permanent is a fact about the ledger *before* it is
    // replaced, and read by the caller, the read and the write would be two
    // statements a reordering can swap. Inside, there is nothing to reorder.
    let reserved = newly_reserved(root, ledger);
    let path = root.join(&ledger.path);
    make_room_for(&path)?;
    put_the_ledger_in_place(&path, &ledger.contents, &ledger.path)?;
    Ok((written, Some(reserved)))
}

/// The directory a file is about to be written into, made if it is not there.
fn make_room_for(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }
    Ok(())
}

/// Put the ledger in place by renaming a sibling onto it.
///
/// **Rename, not write, because a short ledger is worse than none.**
/// `fs::write` truncates and then writes, so an interrupted one leaves the file
/// cut off — measured under `ulimit -f`, the same partial write a full disk
/// produces: 4096 bytes of 5090, 22 reservations of 28, and *no error line at
/// all*, the process having taken the signal.
///
/// What follows from that is the hazard. The next run reports `does not
/// match its own checksum, so it was edited outside the compiler` — blaming a
/// person for the tool's own interrupted write — and offers `--reserve` as the
/// remedy. Following it verbatim, `reconcile` re-derives every truncated-away
/// line from `catalog/errors.toml` *as it reads now*, so a meaning change that
/// was refused before the truncation is ratified without a word. The build that
/// gets interrupted is normally the one run *because* the catalog was just
/// edited, so the in-flight edit is exactly what gets ratified.
///
/// A rename on the same filesystem is atomic: readers see the old ledger or the
/// new one, and a signal in the middle leaves the old.
///
/// **Named for this process.** A fixed `…toml.writing` is one path two builds
/// share, and nothing here takes a lock — cargo's own build lock is released
/// before the verb runs. Two builds render identical bytes today, so the overlap
/// has no visible effect until `catalog/errors.toml` changes between them; a
/// name of this process's own costs nothing and removes the question.
///
/// **And the temp file goes with the failure.** A partial write leaves a
/// truncated `errors.reserved.toml.writing.<pid>` beside the real ledger —
/// measured under `ulimit -f`, 4096 bytes of 5088 — and left there, it is never
/// reused: the next build makes a new name beside it. A syntactically
/// plausible half-ledger sitting next to the real one is the worst litter this
/// directory can hold.
///
/// The messages are about the ledger, too, and not the temp path — which is
/// absolute, in a run whose every other line is repository-relative, for a file
/// the reader has never seen and which no longer exists.
fn put_the_ledger_in_place(path: &Path, contents: &str, named: &str) -> Result<(), String> {
    let beside = path.with_extension(format!("toml.writing.{}", std::process::id())); // allow-nondeterminism: `process::id` names the temp file the ledger is renamed from, and reaches no output
    if let Err(error) = std::fs::write(&beside, contents) {
        let _ = std::fs::remove_file(&beside);
        return Err(format!(
            "could not write {named}: {error}. The ledger was not changed"
        ));
    }
    if let Err(error) = std::fs::rename(&beside, path) {
        let _ = std::fs::remove_file(&beside);
        return Err(format!(
            "could not put {named} in place: {error}. The ledger was not changed"
        ));
    }
    Ok(())
}

/// What a run wrote, as the sentence the console shows.
///
/// **Two numbers that are different things.** The sentence is
/// "N generated file(s), M rewritten": the first is how many the compiler
/// produced, the second how many reached disk. Substituting the second for the
/// first — so a run that rewrote nothing reports zero generated files, and a
/// full rewrite reports both the same — fails
/// `a_run_says_what_it_generated_and_what_it_rewrote`, and nothing a live run
/// prints would show it. A reader watching for drift reads the gap between
/// them.
fn what_a_run_wrote(files: usize, written: usize) -> String {
    format!("{files} generated file(s), {written} rewritten")
}

/// Which of `files` a run would write, by workspace-relative path.
///
/// **The polarity of the skip, which a test of the predicate cannot reach.**
/// `already_written` carries its own unit test, which passes whichever way the
/// `continue` it guards is turned; that is a second decision: inverted,
/// `cargo xtask build` writes **nothing at all** — strictly worse than a
/// predicate that answers wrong.
///
/// Lifted so a test can hand it a directory holding one of three files and read
/// back which are left to write.
fn what_a_run_would_write<'a>(
    root: &Path,
    files: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<String> {
    files
        .into_iter()
        .filter(|(path, contents)| !already_written(&root.join(path), contents))
        .map(|(path, _)| (*path).to_owned())
        .collect()
}

/// Source that has been through the header check, and no other source.
///
/// **A `?` on a line of its own is discardable.** As a statement above an
/// `Ok(formatted)`, `let _ = check_formatted(…)` compiles, changes no verdict
/// anywhere, and hands every caller rustfmt's output unread — on the guard
/// whose own paragraph records a stub `rustfmt` emptying every committed
/// generated file to zero bytes while `build` reports them rewritten. Unit
/// tests of the guard cannot tell, because they do not reach its call site.
///
/// Returning the string *through* the check closes the `let _` shape and
/// leaves `check_formatted(formatted.clone())` beside it. A private field in a
/// module of its own closes the question instead: nothing outside these few
/// lines can build a [`checked::Formatted`], so the only value [`rustfmt`] has
/// to give back is the one the check handed it, and a caller that drops the
/// verdict has nothing to return and does not compile.
mod checked {
    /// rustfmt output carrying the header the emitter writes.
    ///
    /// `Debug` because a test that asks for the error has to be able to say what
    /// came back instead when there is no error.
    #[derive(Debug)]
    pub(super) struct Formatted(String);

    impl Formatted {
        /// The header is the cheapest thing that cannot be there by accident.
        ///
        /// **Trusting a zero exit is not enough**, and this is measured rather
        /// than argued: a `rustfmt` earlier in `PATH` that exits zero and prints
        /// nothing — a stub, a wrapper, a shim, a `rustfmt` from a broken
        /// toolchain — trusted on its exit alone, makes `build` report every
        /// generated file rewritten while emptying each to zero bytes.
        pub(super) fn new(formatted: String) -> Result<Self, String> {
            if formatted.contains(nc_catalog_compiler::GENERATED) {
                return Ok(Self(formatted));
            }
            Err(format!(
                "rustfmt returned {} byte(s) without the generated header, so what came \
                 back is not the source that went in. Check what `rustfmt` resolves to on \
                 this machine: a wrapper that exits zero and prints nothing would empty \
                 every generated file while this reported success",
                formatted.len()
            ))
        }

        /// The source, once there is a `Formatted` to take it out of.
        pub(super) fn into_source(self) -> String {
            self.0
        }
    }
}

#[cfg(test)]
mod tests {
    /// The write loop counts the generated files and the ledger apart.
    ///
    /// **What [`super::run`] cannot drive.** With the loop inside `run` — one
    /// caller and no test, with `check` reaching only [`super::artifacts`] —
    /// three defects planted one at a time each leave the suite and
    /// `cargo xtask check` green: the ledger counted among the generated files,
    /// the announcement reading a hand-tallied counter, and the list of files
    /// to write emptied outright.
    ///
    /// Driven over a scratch root, because the seam is what makes it drivable at
    /// all — a structural repair inside `run` changes none of the three. That
    /// the ledger arrives by a *rename* rather than by a write over the file is
    /// [`the_ledger_is_placed_by_a_rename_and_not_a_write_over`] and
    /// [`the_ledger_is_written_beside_itself_first`]: the three are apart
    /// because they fail for different causes and a reader should be told which.
    #[test]
    fn the_ledger_is_counted_apart_from_the_generated_files() {
        use nc_catalog_compiler::EmittedFile;

        let held = crate::check::LeavesNothingBehind::new("build-counts-apart");
        let scratch = held.root.clone();
        let generated = |name: &str, body: &str| EmittedFile {
            path: format!("crates/nc-catalog/src/{name}"),
            contents: body.to_owned(),
        };
        // **The fixture holds reservations, not a comment.** A fixture of
        // `# one` makes every reading of *which codes this run makes permanent*
        // an empty list whatever the code does — a test baselined where it
        // cannot fail, one step from the seam that carries the reading.
        let ledger = EmittedFile {
            path: "catalog/errors.reserved.toml".to_owned(),
            contents: "[reserved]\n\"NC-CAT-0001\" = \"one — the first\"\n".to_owned(),
        };

        // Nothing on disk: two files and the ledger, counted apart.
        let files = vec![generated("a.rs", "//! a\n"), generated("b.rs", "//! b\n")];
        let said = super::write_what_changed(&scratch, &files, &ledger).expect("writable");
        assert_eq!(
            said,
            (2, Some(vec!["NC-CAT-0001".to_owned()])),
            "a first build wrote two generated files and the ledger, and reported {said:?}"
        );

        // Nothing differs now, so nothing is rewritten and the ledger is not
        // touched — the identical-rewrite skip, from the other side.
        let said = super::write_what_changed(&scratch, &files, &ledger).expect("writable");
        assert_eq!(
            said,
            (0, None),
            "a second build over identical bytes reported {said:?}"
        );

        // **The ledger alone.** A run that appends to the append-only ledger and
        // rewrites no generated file must say so, and a counter shared between
        // the two cannot.
        let moved = EmittedFile {
            contents: "[reserved]\n\"NC-CAT-0001\" = \"one — the first\"\n\"NC-CAT-0002\" = \"two — the second\"\n"
                .to_owned(),
            ..ledger.clone()
        };
        let said = super::write_what_changed(&scratch, &files, &moved).expect("writable");
        // **And the names are the ones this run made permanent.** Read by the
        // caller one statement apart from the write, the two statements could
        // be swapped with the whole suite green while every build that burns a
        // code announces *reserving nothing new*, because the ledger on disk is
        // the ledger just written. The reading is inside the write, and this is
        // what holds it: a reading taken after the write can only answer
        // `Some([])` here.
        assert_eq!(
            said,
            (0, Some(vec!["NC-CAT-0002".to_owned()])),
            "a build that appended to the ledger and rewrote nothing else reported {said:?}"
        );
        assert_eq!(
            std::fs::read_to_string(scratch.join(&moved.path)).expect("the ledger is there"),
            moved.contents,
            "the ledger on disk is not what the build put there"
        );

        // One generated file differing, the ledger steady.
        let files = vec![
            generated("a.rs", "//! a, again\n"),
            generated("b.rs", "//! b\n"),
        ];
        let said = super::write_what_changed(&scratch, &files, &moved).expect("writable");
        assert_eq!(
            said,
            (1, None),
            "a build that rewrote one generated file and no ledger reported {said:?}"
        );

        // **The part-way clause, driven.** `written > 0` is what tells a
        // developer whether the tree is mid-build at the moment they are
        // deciding whether it is safe to stop, and the readonly directory in the
        // test beside this one fails inside `put_the_ledger_in_place`, after the
        // loop has written nothing. A test with no *failing* generated-file
        // write cannot tell the condition from `written > files.len()`, never
        // true, under which every interrupted build says the tree is as it was
        // however many files it had already rewritten; nor can the two censuses
        // that pin the sentences, because they pin the literals and not the
        // branch.
        //
        // Driven here by making the *second* file unwritable, so the first is
        // rewritten before the failure.
        let two = vec![
            generated("c.rs", "//! c, new\n"),
            generated("d.rs", "//! d, new\n"),
        ];
        for file in &two {
            let at = scratch.join(&file.path);
            std::fs::create_dir_all(at.parent().expect("a parent")).expect("writable");
            std::fs::write(&at, "what was there\n").expect("writable");
        }
        let second = scratch.join(&two[1].path);
        let was = std::fs::metadata(&second).expect("it is there");
        let mut shut = was.permissions();
        shut.set_readonly(true);
        std::fs::set_permissions(&second, shut).expect("writable");
        let part_way = super::write_what_changed(&scratch, &two, &moved).expect_err("refused");
        std::fs::set_permissions(&second, was.permissions()).expect("writable");
        assert!(
            part_way.contains("1 generated file(s) were rewritten before it"),
            "a build that rewrote one file and then failed said: {part_way}. A reader \
             deciding whether it is safe to stop is told the tree is as it was"
        );
    }

    /// A sibling write that fails takes the sibling with it too.
    ///
    /// **The cleanup the hazard is measured on.** The test below reaches the
    /// *rename* failure — the sibling is written fine there — so it never takes
    /// the `fs::write` branch, and deleting `let _ = remove_file(&beside)` from
    /// that branch fails this test instead. That branch is exactly the one
    /// `put_the_ledger_in_place`'s paragraph measures under `ulimit -f`: the
    /// write fails **after** creating the file and leaves a
    /// truncated `errors.reserved.toml.writing.<pid>` — 4096 bytes of 5088 —
    /// beside the real ledger.
    ///
    /// The state is made here by putting the sibling there first, read-only:
    /// `fs::write` then refuses it with `EACCES` while the file exists, and
    /// removing it needs only the directory's permission, which the cleanup has.
    /// The sibling path carries this process's own id, and the test runs in the
    /// same process as the code it drives, so the two agree by construction.
    #[test]
    fn a_sibling_write_that_fails_takes_the_sibling_with_it() {
        use nc_catalog_compiler::EmittedFile;

        let kept = crate::check::LeavesNothingBehind::new("build-ledger-beside");
        let scratch = kept.root.clone();
        let ledger = EmittedFile {
            path: "catalog/errors.reserved.toml".to_owned(),
            contents: "[reserved]\n\"NC-CAT-0001\" = \"one — the first\"\n".to_owned(),
        };
        let at = scratch.join(&ledger.path);
        std::fs::create_dir_all(at.parent().expect("a parent")).expect("writable");
        let beside = at.with_extension(format!("toml.writing.{}", std::process::id())); // allow-nondeterminism: `process::id` names the temp file this drives the refusal of, and reaches no output
        std::fs::write(&beside, "half a ledger\n").expect("writable");
        let mut locked = std::fs::metadata(&beside)
            .expect("it is there")
            .permissions();
        locked.set_readonly(true);
        std::fs::set_permissions(&beside, locked).expect("writable");

        let refused = super::write_what_changed(&scratch, &[], &ledger)
            .expect_err("a sibling that cannot be written is a placement that cannot happen");
        assert!(
            refused.contains("catalog/errors.reserved.toml"),
            "the refusal does not name the ledger it could not place: {refused}"
        );
        assert!(
            !beside.exists(),
            "the sibling write failed and left `{}` behind — a syntactically plausible \
             half-ledger beside the real one, which is the worst litter this directory \
             can hold",
            beside.display()
        );
        assert!(
            !at.exists(),
            "the ledger was placed even though the write beside it failed"
        );
        drop(kept);
    }

    /// A placement that fails takes its half-written sibling with it.
    ///
    /// **"The temp file goes with the failure", driven.**
    /// [`the_ledger_is_written_beside_itself_first`] asserts nothing is left
    /// beside the ledger only after a *successful* run, where the rename
    /// consumes the sibling and the cleanup never runs — and in its failing case
    /// the sibling is never created, so the cleanup is a no-op there too. This
    /// reaches a state where a sibling exists **and** the placement fails, so
    /// deleting the cleanup on the rename's failure path fails it.
    ///
    /// That state is made here by putting a directory where the ledger goes:
    /// the sibling is written normally and `rename` refuses to replace a
    /// directory with a file, which is the only failure this file can produce
    /// on demand without permission games.
    ///
    /// **And what it cannot cover is said rather than implied.** The paragraph
    /// on `put_the_ledger_in_place` measures the hazard under `ulimit -f`,
    /// where the process takes `SIGXFSZ` — and a cleanup on an error-return path
    /// does not run when the process dies, nor on `^C`, nor under the OOM
    /// killer. For those a stale sibling has to be *found* rather than
    /// prevented, and it is:
    /// `nothing_unnamed_sits_in_the_catalog` refuses a name the catalog does not
    /// declare, and the kind walk allows only `toml` under `catalog/`.
    #[test]
    fn a_placement_that_fails_takes_its_sibling_with_it() {
        use nc_catalog_compiler::EmittedFile;

        let kept = crate::check::LeavesNothingBehind::new("build-ledger-sibling");
        let scratch = kept.root.clone();
        let ledger = EmittedFile {
            path: "catalog/errors.reserved.toml".to_owned(),
            contents: "[reserved]\n\"NC-CAT-0001\" = \"one — the first\"\n".to_owned(),
        };
        let at = scratch.join(&ledger.path);
        // A directory where the ledger goes. `rename` will not replace one with
        // a file, so the sibling is written and the placement then fails.
        std::fs::create_dir_all(at.join("in the way")).expect("writable");

        let refused = super::write_what_changed(&scratch, &[], &ledger)
            .expect_err("a placement onto a directory cannot succeed");
        assert!(
            refused.contains("catalog/errors.reserved.toml"),
            "the refusal does not name the ledger it could not place: {refused}"
        );
        let beside: Vec<String> = std::fs::read_dir(scratch.join("catalog")) // allow-nondeterminism: this `read_dir` lists a scratch directory to prove the failed placement left no temp file, and reaches no generated output
            .expect("the catalog directory is there")
            .filter_map(|entry| Some(entry.ok()?.file_name().to_string_lossy().into_owned()))
            .filter(|name| name.contains(".writing."))
            .collect();
        assert!(
            beside.is_empty(),
            "the placement failed and left {beside:?} beside the ledger — a \
             syntactically plausible half-ledger next to the real one is the worst \
             litter this directory can hold, and the next reader cannot tell which is \
             the record of what is burned"
        );
        assert!(
            at.is_dir(),
            "the failed placement changed what stands at the ledger's own path"
        );
        drop(kept);
    }

    /// The ledger is placed by a rename, not by a write over it.
    ///
    /// **A rename replaces a directory entry; a write mutates an inode.** That
    /// is the difference the sibling-and-rename exists for, and it is the one
    /// thing the neighbour below cannot read: it proves only that a *new* file
    /// is created beside the ledger, and `check`'s rule proves only that two
    /// byte sequences appear in the source. A placement moved onto a branch
    /// production never leaves — write-over when the ledger exists, rename when
    /// it does not — leaves both green.
    ///
    /// So the inode is what is asked. A second name is hard-linked onto the
    /// ledger before a build that changes it; after a rename that link still
    /// holds the **old** bytes, because the rename gave the ledger's path a new
    /// inode and left the old one alone. After `fs::write(path, …)`, after
    /// `fs::copy` + remove, and after the branch above, the link holds the new
    /// bytes, because all three truncate the inode the link names.
    ///
    /// It needs no permission games and it refuses nothing legitimate: every
    /// implementation that writes beside and renames passes, from any sibling
    /// path on the same filesystem.
    #[test]
    fn the_ledger_is_placed_by_a_rename_and_not_a_write_over() {
        use nc_catalog_compiler::EmittedFile;

        let kept = crate::check::LeavesNothingBehind::new("build-ledger-inode");
        let scratch = kept.root.clone();
        let ledger = EmittedFile {
            path: "catalog/errors.reserved.toml".to_owned(),
            contents: "[reserved]\n\"NC-CAT-0001\" = \"one — the first\"\n".to_owned(),
        };
        let at = scratch.join(&ledger.path);
        std::fs::create_dir_all(at.parent().expect("a parent")).expect("writable");
        std::fs::write(&at, &ledger.contents).expect("writable");
        // The second name for the ledger's current inode. Whatever the build
        // does to the *path*, this keeps naming the bytes that were there.
        let held = scratch.join("catalog/the-inode-that-was-there");
        std::fs::hard_link(&at, &held).expect("a hard link is writable");

        let appended = EmittedFile {
            contents: format!(
                "{}\"NC-CAT-0002\" = \"two — the second\"\n",
                ledger.contents
            ),
            ..ledger.clone()
        };
        let said = super::write_what_changed(&scratch, &[], &appended).expect("writable");
        assert_eq!(
            said,
            (0, Some(vec!["NC-CAT-0002".to_owned()])),
            "the build did not append to the ledger, so there is no placement to read"
        );
        assert_eq!(
            std::fs::read_to_string(&at).expect("the ledger is there"),
            appended.contents,
            "the ledger at its own path is not what the build put there"
        );
        assert_eq!(
            std::fs::read_to_string(&held).expect("the second name is there"),
            ledger.contents,
            "the inode the ledger had before this build now holds the bytes this build \
             wrote, so the ledger was not renamed into place — it was written over, and \
             an interrupted write leaves a short file where the record of which codes \
             are burned used to be"
        );
        drop(kept);
    }

    /// The ledger is written beside its own path first.
    ///
    /// **What this proves is that the ledger is written *beside* itself, and
    /// that is all it proves.** The rename is never reached here: a directory at
    /// mode 0555 refuses a *new* file with `EACCES` while a truncating write onto
    /// the ledger already in it succeeds, so what fails here is the
    /// `fs::write(&beside, …)` that `put_the_ledger_in_place` runs before the
    /// rename. A placement moved onto a branch
    /// production never leaves — write-over when the ledger exists, rename when
    /// it does not — leaves this green and leaves `check`'s text rule green too,
    /// because a `contains` cannot tell a live call from a dead branch.
    ///
    /// So the two halves are two tests, and
    /// [`the_ledger_is_placed_by_a_rename_and_not_a_write_over`] is the one
    /// that reads the placement.
    ///
    /// **A rename told apart from its absence, which counting temp files cannot
    /// do.** "The rename left nothing beside it" is satisfied *harder* by never
    /// making a temp file at all — the sequence replaced with
    /// `fs::write(path, …)` straight onto the ledger leaves a count of temp
    /// files green.
    ///
    /// What separates them is a directory that will not take a *new* file while
    /// the file already in it can still be truncated: creating
    /// `errors.reserved.toml.writing.<pid>` needs write permission on the
    /// directory, and rewriting `errors.reserved.toml` needs it only on the file.
    /// So the *sibling write* must fail and leave the ledger exactly as it was,
    /// and a write straight to the target would succeed and replace it — which
    /// is the interrupted-write shape, with the interruption supplied rather
    /// than waited for.
    #[test]
    fn the_ledger_is_written_beside_itself_first() {
        use nc_catalog_compiler::EmittedFile;

        let kept = crate::check::LeavesNothingBehind::new("build-ledger-rename");
        let scratch = kept.root.clone();
        let files = vec![EmittedFile {
            path: "crates/nc-catalog/src/a.rs".to_owned(),
            contents: "//! a\n".to_owned(),
        }];
        let ledger = EmittedFile {
            path: "catalog/errors.reserved.toml".to_owned(),
            contents: "# one\n# two\n".to_owned(),
        };
        super::write_what_changed(&scratch, &files, &ledger).expect("writable");

        // **The rename left nothing beside it.** A truncated
        // `…toml.writing.<pid>` next to the real ledger is the worst litter this
        // directory can hold.
        let beside: Vec<String> = std::fs::read_dir(scratch.join("catalog")) // allow-nondeterminism: this `read_dir` lists a scratch directory to prove the rename left no temp file, and reaches no generated output
            .expect("readable")
            .filter_map(|entry| Some(entry.ok()?.file_name().to_str()?.to_owned()))
            .filter(|name| name.contains(".writing."))
            .collect();
        assert!(
            beside.is_empty(),
            "the ledger was renamed into place and left {beside:?} behind"
        );

        // The directory takes no new file; the ledger in it is still writable.
        // **Through the door each system shuts a directory with.** A read-only
        // mark is a mode on Unix and an attribute Windows ignores on a
        // directory, so there it shut nothing and the build wrote its sibling
        // and renamed it as though nothing were in the way.
        nc_platform::testing::shut_its_owner_out(&scratch.join("catalog")).expect("shut");
        let refused = super::write_what_changed(
            &scratch,
            &files,
            &EmittedFile {
                contents: "# one\n# two\n# three\n".to_owned(),
                ..ledger.clone()
            },
        );
        nc_platform::testing::make_private(&scratch.join("catalog")).expect("opened again");
        assert!(
            refused.is_err(),
            "the ledger's directory would take no new file and the build reported \
             {refused:?} — so the ledger was not written beside and renamed, it was \
             written over, and an interrupted one leaves it truncated"
        );
        assert_eq!(
            std::fs::read_to_string(scratch.join(&ledger.path)).expect("the ledger is there"),
            ledger.contents,
            "the ledger changed on a run that failed. Whatever put it there was not a \
             rename from a sibling path: the record of which codes are burned is now \
             whatever a half-finished write left"
        );
    }

    /// A run says how many files it generated and how many it rewrote.
    ///
    /// Two numbers, and the sentence is the only place a reader sees the gap
    /// between them. Substituting one for the other is invisible to a live run;
    /// this asks for both, with values that cannot be confused.
    ///
    /// **Anchor an insertion on the doc block, never on the `fn`.** A test
    /// inserted between this block and the function it was written for makes
    /// rustdoc render it as the *other* test's summary and leaves this one with
    /// none — and a per-file ratchet bumped to match then pins the damaged
    /// shape. The doc-block rule beside it is what reads the shape.
    #[test]
    fn a_run_says_what_it_generated_and_what_it_rewrote() {
        assert_eq!(
            super::what_a_run_wrote(9, 2),
            "9 generated file(s), 2 rewritten",
            "the two counts are not both in the sentence, in their own places"
        );
        assert_eq!(
            super::what_a_run_wrote(9, 0),
            "9 generated file(s), 0 rewritten",
            "a run that rewrote nothing must still say how many it generated"
        );
    }

    /// A run writes the files that are not already on disk, and only those.
    ///
    /// **The decision, not the predicate.** `already_written` is named and
    /// unit-tested; the skip it guards is a second decision, and inverting that
    /// one condition, which makes `cargo xtask build` write nothing at all,
    /// fails this test.
    ///
    /// Both directions: skipping what must be written is a generated crate that
    /// stops tracking the registry, and writing what is already there bumps a
    /// modification time and wakes a watcher that never settles — which is what
    /// the `continue`'s own comment is about.
    #[test]
    fn a_run_writes_the_files_that_are_not_already_on_disk() {
        let held = crate::check::LeavesNothingBehind::new("build-would-write");
        let root = held.root.clone();
        std::fs::create_dir_all(root.join("crates/nc-catalog/src")).expect("writable");
        std::fs::write(root.join("crates/nc-catalog/src/same.rs"), "identical\n")
            .expect("writable");
        std::fs::write(
            root.join("crates/nc-catalog/src/stale.rs"),
            "what was there\n",
        )
        .expect("writable");
        assert_eq!(
            super::what_a_run_would_write(
                &root,
                [
                    ("crates/nc-catalog/src/same.rs", "identical\n"),
                    (
                        "crates/nc-catalog/src/stale.rs",
                        "what the registry says now\n"
                    ),
                    ("crates/nc-catalog/src/absent.rs", "never written\n"),
                ]
            ),
            [
                "crates/nc-catalog/src/stale.rs",
                "crates/nc-catalog/src/absent.rs"
            ],
            "a run would write the wrong set, and the generated crate is what every \
             other gate reads"
        );
        drop(held);
    }

    /// A reservation is a quoted code **with a value beside it**.
    ///
    /// **`next = 30` and `# a note = with an equals in it` decide nothing
    /// here.** Whatever the filter says, both are dropped by the
    /// `strip_prefix('"')` in `the_codes_reserved_in`, so a corpus of only
    /// those two rows cannot tell either half of a two-clause filter from its
    /// absence.
    ///
    /// The row below drives the filter: a quoted code on a line of its own is
    /// not a key, and read as one it makes that code permanent in a ledger
    /// `AGENTS.md` says is append-only.
    #[test]
    fn only_a_quoted_line_reserves_a_code() {
        let ledger = concat!(
            "[reserved]\n",
            "next = 30\n",
            "\"NC-CAT-0001\" = \"catalog.one — the first\"\n",
            "\"NC-CAT-0009\"\n",
            "\"NC-CAT-0002\" = \"catalog.two — the second\"\n",
            "# a note = with an equals in it\n",
        );
        assert_eq!(
            super::the_codes_reserved_in(ledger),
            vec!["NC-CAT-0001".to_owned(), "NC-CAT-0002".to_owned()],
            "a quoted code with no value beside it was read as a reservation, and a \
             build would make it permanent"
        );
    }

    /// What a build makes permanent is the codes the ledger did not already hold.
    ///
    /// **`AGENTS.md` states the contract as "announces each code it reserves, on
    /// the line it reserves it", and which codes those are is decided
    /// here.** Measured on 2026-09-08, the filter narrowed to
    /// `!already.contains(code) && !code.starts_with("NC")` — which is every code
    /// this catalog has: `cargo test -p xtask --bin xtask` green at 481, and the
    /// run would then say *"reserving nothing new"* while
    /// `catalog/errors.reserved.toml` grew, permanently, in a tree with no git.
    ///
    /// `what_the_ledger_write_was` — the sentence *chooser* — has a test; the
    /// list it chooses from needs one too.
    #[test]
    fn a_build_reserves_the_codes_the_ledger_did_not_hold() {
        let held = crate::check::LeavesNothingBehind::new("scratch");
        let scratch = held.root.clone();
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join("catalog")).expect("writable");
        let ledger = |codes: &[&str]| {
            use core::fmt::Write as _;
            let mut rows = String::new();
            for code in codes {
                let _ = writeln!(rows, "\"{code}\" = \"error.some.key — a description\"");
            }
            format!(
                "[meta]\nchecksum = \"x\"\ncount = {}\n\n[reserved]\n{rows}",
                codes.len()
            )
        };
        let path = "catalog/errors.reserved.toml";
        std::fs::write(scratch.join(path), ledger(&["NC-DB-0001", "NC-DB-0002"]))
            .expect("writable");

        // Two held, three written: exactly the third is new.
        let writing = nc_catalog_compiler::EmittedFile::new(
            path,
            ledger(&["NC-DB-0001", "NC-DB-0002", "NC-DB-0003"]),
        );
        assert_eq!(
            super::newly_reserved(&scratch, &writing),
            vec!["NC-DB-0003".to_owned()],
            "a code this build makes permanent was not announced"
        );

        // The same ledger rewritten: nothing is new, and saying otherwise would
        // put a code in front of a reader that was already theirs.
        let same =
            nc_catalog_compiler::EmittedFile::new(path, ledger(&["NC-DB-0001", "NC-DB-0002"]));
        assert_eq!(super::newly_reserved(&scratch, &same), [] as [String; 0]);

        // And a ledger that is not there yet: every code in it is new, which is
        // the first build of a catalog.
        let _ = std::fs::remove_file(scratch.join(path));
        assert_eq!(
            super::newly_reserved(&scratch, &writing).len(),
            3,
            "the first build announced fewer codes than it reserved"
        );
        drop(held);
    }
    use super::checked::Formatted;
    use super::what_the_ledger_write_was;

    #[test]
    fn what_is_already_on_disk_is_not_written_again() {
        let held = crate::check::LeavesNothingBehind::new("build-already-written");
        let path = held.root.join("generated.rs");
        assert!(
            !super::already_written(&path, "a"),
            "a file that is not there was read as already holding these bytes"
        );
        std::fs::write(&path, "a").expect("writable");
        assert!(
            super::already_written(&path, "a"),
            "a file holding exactly these bytes was read as needing a write"
        );
        assert!(
            !super::already_written(&path, "b"),
            "a file holding other bytes was read as already holding these"
        );
        drop(held);
    }

    #[test]
    fn source_that_came_back_with_its_header_is_accepted() {
        let good = format!(
            "//! {}\npub const X: u8 = 1;\n",
            nc_catalog_compiler::GENERATED
        );
        assert_eq!(
            Formatted::new(good.clone())
                .expect("source with its header is formatted source")
                .into_source(),
            good,
            "the source that went in is not the source that came back out"
        );
    }

    #[test]
    fn nothing_coming_back_is_refused_and_the_message_says_what_to_check() {
        // The exact answer a stub `rustfmt` gives: exit zero and no output,
        // which, trusted, would truncate every generated file to nothing while
        // `build` said "ok".
        let error =
            Formatted::new(String::new()).expect_err("an empty result is not formatted source");
        assert!(error.contains("rustfmt"), "{error}");
        assert!(error.contains("0 byte"), "{error}");
    }

    #[test]
    fn source_without_the_header_is_refused() {
        assert!(Formatted::new("pub const X: u8 = 1;\n".to_owned()).is_err());
    }

    /// A ledger write with no new code does not claim a permanent reservation.
    ///
    /// A counter incremented once per *write of the file*, with a header above
    /// the ledger's reservations, makes a comment edited in `ledger.rs` announce
    /// `1 appended … which is permanent` with the code set unchanged. Both
    /// sentences are asserted, because the failure is not that one is missing:
    /// it is that the wrong one is printed.
    #[test]
    fn a_ledger_rewrite_that_reserves_nothing_says_so() {
        // Written out in full, the way `each_audit_says_what_it_ran` pins the
        // sentence `ui::ok(audit.what_it_read())` prints: a sentence a helper
        // returns never reaches `WHAT_THE_GATE_CAN_SAY`, because the census
        // reads the literal at the call site and there is none. This test is
        // that list, for these two.
        assert_eq!(
            what_the_ledger_write_was(&[]),
            "and catalog/errors.reserved.toml was rewritten, reserving nothing new"
        );
        assert_eq!(
            what_the_ledger_write_was(&["NC-CAT-0001".to_owned()]),
            "and 1 code(s) appended to catalog/errors.reserved.toml, which is permanent"
        );
        assert_eq!(
            what_the_ledger_write_was(&["NC-CAT-0001".to_owned(), "NC-CAT-0002".to_owned()]),
            "and 2 code(s) appended to catalog/errors.reserved.toml, which is permanent"
        );
    }
}
