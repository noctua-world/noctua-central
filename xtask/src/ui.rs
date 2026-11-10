//! Every line this tool prints goes through here.
//!
//! One place, so the shape of the output is a decision rather than an accident,
//! and so colour can be turned off in exactly one spot. `anstream` strips the
//! escapes when stdout is not a terminal.

#![expect(
    clippy::disallowed_methods,
    reason = "this module is the one output path — the rule the crate-level deny protects"
)]
use anstyle::{AnsiColor, Style};

const HEADING: Style = Style::new().bold();
const GOOD: Style = Style::new().fg_color(Some(anstyle::Color::Ansi(AnsiColor::Green)));
const BAD: Style = Style::new()
    .fg_color(Some(anstyle::Color::Ansi(AnsiColor::Red)))
    .bold();
const WARN: Style = Style::new().fg_color(Some(anstyle::Color::Ansi(AnsiColor::Yellow)));
/// A part this system does not run: neither good nor bad, and not a warning.
const ASIDE: Style = Style::new().fg_color(Some(anstyle::Color::Ansi(AnsiColor::Cyan)));
const DIM: Style = Style::new().dimmed();

/// The system this tool was built for, by the name a not-applicable line gives
/// it: `linux`, `macos` or `windows`.
///
/// A name to print and nothing more. Whether a step applies here is decided
/// once, by `check::RUNS_THE_STACK`, and never by comparing this string.
pub(crate) const SYSTEM: &str = std::env::consts::OS;

/// Opens a section. Nothing else in this tool prints at column zero, so a
/// heading is what a reader's eye finds when scrolling a long run.
pub(crate) fn heading(text: &str) {
    // **Recorded, because a step that never ran prints nothing at all.** Every
    // guard in this file counts what it *said* — warnings, failures, announced
    // skips — and none of them counts what *ran*: wrapping a step in
    // `if !offline { … }` with no `warn` beside it moves neither counter, and
    // with the counters alone `check --offline` comes out byte-identical to a
    // clean run with acceptance criterion 2's whole gate gone. Measured. A
    // heading is the one thing every step prints before it decides anything, so
    // it is what a census can hold.
    if let Ok(mut printed) = HEADINGS.lock() {
        printed.push(text.to_owned());
    }
    anstream::println!("{HEADING}{text}{HEADING:#}");
}

/// Every heading this run printed, in the order it printed them.
static HEADINGS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// The steps this run opened.
///
/// Held against a written list by `check`'s verdict, which is what makes a step
/// that quietly returned visible: it is the only account of what ran that does
/// not depend on the step having something to say.
pub(crate) fn headings() -> Vec<String> {
    HEADINGS
        .lock()
        .map(|printed| printed.clone())
        .unwrap_or_default()
}

/// Something that worked. The trailing spaces align the message column with
/// `failure` and `warn`, so a run reads as one column of prose.
pub(crate) fn ok(text: &str) {
    let _printing = while_printing();
    anstream::println!("  {GOOD}ok{GOOD:#}    {}", aligned(text));
}

/// Something that did not. Goes to stderr, so `| grep` on stdout cannot hide it.
///
/// **Counted, as [`warn`] is counted.** With a `warn` line through a counter the
/// verdict reads and an `error` line through nothing, the only record that a
/// step found something wrong is the `Vec` the step itself pushes to — and a
/// step wired to a `&mut Vec::new()` prints its error lines into that void and
/// the run ends `check passed`, exit 0. Measured on ten step wires at once in
/// `check::run`: `error … constraint(s) … and this repository's policies have
/// …` printed, then `check passed`. What closes it is that the two are
/// independent — the printing and the recording — so the verdict can ask whether
/// they agree. See `check::finish`.
pub(crate) fn failure(text: &str) {
    let _printing = while_printing();
    FAILURES.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    anstream::eprintln!("  {BAD}error{BAD:#} {}", aligned(text));
}

/// Something worth knowing that is not a failure.
pub(crate) fn warn(text: &str) {
    let _printing = while_printing();
    WARNINGS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    // **A skip announces itself, and is counted apart.** Criterion 1 is "no
    // failures and no warnings", and enforcing that as a *number* — three
    // allowed under `--offline`, none otherwise — turns CI red on its own
    // checkout: a bare checkout has no workspace above it and no sibling design
    // system, several steps correctly say so — some the workspace and one the
    // sibling — and the run fails on the allowance with every gate green. A
    // count cannot tell a step that said "I did not run" from a step that said
    // something is wrong; the word it starts with can.
    // **One prefix, not a prefix or either of two phrases found anywhere in the
    // line.** A substring test on free prose classifies by accident: a genuine
    // warning phrased *"the design system's tag could not be judged: the pin
    // was not compared"* is counted as an announced skip and printed by the
    // verdict under `not run:` — a real problem, in the list of things that
    // deliberately did not happen. The same sentence with "could not be read"
    // instead fails the run, which is the right answer to both.
    //
    // The two long-form messages carry the prefix as well, so what decides is
    // a word an author writes on purpose at the start of a line. That is still
    // self-declared — nothing can check a step's own account of itself — but it
    // cannot be arrived at by describing a defect.
    if text.starts_with("skipped:") {
        SKIPS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        // **And the verdict says which.** `check passed, with 3 warn line(s)
        // above` is a number a reader has to scroll to interpret, and the three
        // it stands for include the section gate reading every citation or none
        // — measured, with only that count, renaming one zero-content marker
        // file turns the largest gate in the suite off and the last line of the
        // run is a pass with a count. What was skipped belongs beside the
        // verdict, because that is the line a person reads.
        if let Ok(mut announced) = ANNOUNCED.lock() {
            announced.push(shortened(text));
        }
    }
    anstream::println!("  {WARN}warn{WARN:#}  {}", aligned(text));
}

/// A part of the gate that this system does not run, by name.
///
/// **Its own kind, not a `warn` and not a skip.** A skip is a step that could
/// have run here and did not, because the caller asked or because what it
/// reads is absent; this is a step that never runs on this system at all —
/// the ones that need the container stack, which only the Linux job brings up
/// (`NOCTUA.md §22.2`), and the suites that need what this system lacks,
/// which the Linux job runs. Printed as a `warn`, every macOS and Windows run
/// would end `with N warn line(s)` over steps nobody could have run there, and
/// the number a reader uses to see that something was skipped would mean two
/// things.
///
/// **Counted, and by what it says.** `check::finish` holds the parts this run
/// printed against the list written for the system, exactly: none on Linux,
/// where every step runs, and elsewhere every part that needs the stack, then
/// every suite the system lacks what it needs for — so a part cannot start
/// standing aside on Linux, or stop saying so elsewhere, with the verdict
/// unchanged.
pub(crate) fn not_applicable(what: &str) {
    let _printing = while_printing();
    if let Ok(mut said) = NOT_APPLICABLE.lock() {
        said.push(what.to_owned());
    }
    let text = format!("not applicable on {SYSTEM}: {what}, and the Linux job runs it");
    anstream::println!("  {ASIDE}n/a{ASIDE:#}   {}", aligned(&text));
}

/// What each not-applicable line said, in the order they were printed.
static NOT_APPLICABLE: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// The parts of the gate this run said do not apply on this system.
pub(crate) fn not_applicable_parts() -> Vec<String> {
    NOT_APPLICABLE
        .lock()
        .map(|said| said.clone())
        .unwrap_or_default()
}

/// Records that this run fetched the packages the lockfile names.
///
/// **What the run did, kept where the verdict reads it.** `--offline` promises
/// no network. The fetch keeps that by not running at all, and every other
/// cargo the gate starts by being told so (`check::cargo_command`). The fetch's
/// half is one call site and the function it calls: with the call handed the
/// wrong flag, a run under `--offline` fetches and a run without it never does.
/// On a machine that already holds every package, either run differs from one
/// that got it right only by the fetch's own `ok` line, there or missing, and
/// no step reads that line. With `!offline` at the call every xtask test stays
/// green; this record is what tells the runs apart, and `check::finish` holds
/// it against what the run was asked to do, the way it holds the parts that
/// stood aside.
pub(crate) fn fetched() {
    let _printing = while_printing();
    FETCHED.store(true, core::sync::atomic::Ordering::Relaxed);
}

/// Whether [`fetched`] was called.
static FETCHED: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Whether this run fetched the packages the lockfile names.
pub(crate) fn the_fetch_ran() -> bool {
    FETCHED.load(core::sync::atomic::Ordering::Relaxed)
}

/// How many of the warnings were a step announcing it did not run.
pub(crate) fn skips() -> usize {
    SKIPS.load(core::sync::atomic::Ordering::Relaxed)
}

/// How many announced skips say `reason` is why they did not run.
///
/// **What this exists for is an exact count.** The total above cannot be one:
/// a bare checkout has no workspace over it and no sibling design system, and
/// several steps legitimately announce that — some the workspace and one the
/// sibling — on top of whatever a flag skips, so the only rule the total
/// can carry is a lower bound — and a lower bound is paid by any extra skip.
/// Measured: with `--offline` one step longer and the constant unmoved, a `>=`
/// keeps every document that states the number agreeing with a stale one.
/// Counting the skips that name the flag separates the two populations, so
/// the flag's own number can be an equality.
pub(crate) fn skips_because(reason: &str) -> usize {
    announced()
        .iter()
        .filter(|said| said.starts_with(reason))
        .count()
}

/// Announced skips, counted apart from warnings that are not one.
static SKIPS: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

/// What each of them said, in a few words, for the verdict to name.
static ANNOUNCED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// The skips this run announced, shortest form first said.
pub(crate) fn announced() -> Vec<String> {
    ANNOUNCED
        .lock()
        .map(|announced| announced.clone())
        .unwrap_or_default()
}

/// Enough of a message to say which step it was.
///
/// **Not the first clause.** Cut at its first comma, every `--offline` skip
/// would read `not run: --offline` — the same words for the database, the
/// integration tests and the advisory fetch. What tells them apart is further
/// along, so this cuts by length at a word boundary and says
/// so with an ellipsis.
fn shortened(text: &str) -> String {
    const ENOUGH: usize = 76;
    let text = text.strip_prefix("skipped:").unwrap_or(text).trim();
    if text.chars().count() <= ENOUGH {
        return text.to_owned();
    }
    let cut: String = text.chars().take(ENOUGH).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{}…", cut.trim_end_matches([',', '.', ';']))
}

/// The message column is `MESSAGE` characters in, including on a second line.
const MESSAGE: usize = 8;

/// Text with every line after the first indented to the message column.
///
/// **A line break inside a message breaks both of this module's rules at once.**
/// Unindented, the continuation starts at column zero, where the file says only
/// headings live, so a reader scrolling for section boundaries meets a wrapped
/// error instead — and the "one column of prose" the alignment exists for stops
/// after the first line. It is not hypothetical: `database.rs` embeds one, on
/// the path a developer hits when the stack is down, and `release`'s `relock`
/// passes cargo's multi-line stderr straight through.
///
/// ```text
///   error nothing is listening for PostgreSQL at 127.0.0.1:<this checkout's port>. …
/// Run `cargo xtask dev` to bring the stack up, or …
/// ```
///
/// Written here rather than at each call site, because "every line this tool
/// prints goes through here" is this module's whole premise.
fn aligned(text: &str) -> String {
    let mut lines = text.lines();
    let first = lines.next().unwrap_or_default().to_owned();
    lines.fold(first, |mut whole, line| {
        whole.push('\n');
        // An empty continuation gets no indent: eight spaces and nothing else
        // is a line of trailing whitespace, which is not what the column is
        // for.
        if !line.trim().is_empty() {
            for _ in 0..MESSAGE {
                whole.push(' ');
            }
            whole.push_str(line);
        }
        whole
    })
}

/// How many `warn` lines this run has printed.
///
/// **A run can announce a skip and still end `check passed`.** Measured in the
/// state a fresh checkout of this repository alone produces — no workspace above
/// the repository, which no step in `ci.yml` creates — every one of these is
/// printed and the last line is `check passed`:
///
/// ```text
/// warn  skipped: <n> workspace document(s) above this repository were not read:
/// warn  skipped: the design system is a sibling repository and there is none …
/// warn  skipped: the Cedar schema's transcription reads the workspace documents …
/// warn  skipped: the section gate reads the workspace documents above this repository
/// warn  skipped: <n> of them name a workspace document and could not be judged:
/// ```
///
/// Every one of them carries the prefix, because the rule below is a prefix
/// test and nothing else. Matching two phrases anywhere in a line as well
/// classifies a real warning as a skip for containing one of them.
///
/// **The count is not stated here.** The transcript above is what
/// `just checkout-shape` prints, so it is a measurement rather than a memory,
/// and the recipe asserts the count as an equality — a number written into
/// documents rots in all of them at once, so the only place it is written is
/// that recipe's `expected=`.
///
/// **The numbers *inside* the lines are `<n>` for the same reason.** The
/// recipe's equality counts announcement *lines*; nothing holds a figure written
/// inside one, so none is written — a count of workspace documents or of
/// unjudged citations pasted here goes stale the day the workspace gains a
/// document or a citation, as `references.rs` records for the second of them.
///
/// **And what the recipe asserts is this machine's count, not CI's.** The two
/// image pins are announced too where their tags are not in the local store,
/// which on a runner is both of them — so a runner makes two more, and the
/// transcript above is a checkout on a store like this one.
///
/// Citations naming a workspace document go unchecked there, in the file whose
/// whole purpose is that CI and a developer run one gate (`NOCTUA.md §19.4`).
/// The skips are honest and each says why; a verdict that reads the same as a
/// run that checked everything is not, so the verdict carries the number.
static WARNINGS: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

/// How many `error` lines this run has printed.
///
/// Separate from the failures a step *records*, which is the point: the verdict
/// is computed from the recorded list and the reader sees the printed lines, and
/// without this count nothing holds the two together.
static FAILURES: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

/// Held by the test that reads [`warnings`] or [`skips`] as a difference, and by
/// every printer in this module while a test build runs.
///
/// **Both counters are process-wide statics and `cargo test` runs tests in
/// parallel**, so "the number this test added" is only that number while no
/// other test is printing. `the_verdict_counts_the_skips_it_announced`, asserting
/// on such a difference unheld, would pass only because libtest starts tests in
/// name order and `check::` sorts before `release::`, whose two tests each print
/// two non-skip warnings through `release::run(…, dry_run)` — the one that says
/// the gate was not run, and the one that says nothing was written. Measured by
/// running the two by name with `--test-threads=2`: `left: 3, right: 1` — those
/// two, plus the one the reading test prints itself — two hundred times in two
/// hundred attempts.
///
/// **Taken by the printers, not by agreement among tests.** A lock every
/// printing test has to remember to take is a rule the next test breaks; this
/// one is taken inside `ok`, `failure`, `warn` and `detail` themselves, so a
/// test holding it has the statics to itself whatever else is running. It is
/// reentrant because the reading test prints while holding it.
///
/// `#[cfg(test)]` only — `check` is single-threaded and a real run pays nothing.
#[cfg(test)]
static PRINTING: std::sync::Mutex<()> = std::sync::Mutex::new(());

// How many guards this thread holds, so the outer one is the only real lock.
// `std::sync::ReentrantLock` is unstable on the pinned toolchain, and this is
// the whole of what is needed from it.
#[cfg(test)]
thread_local! {
    static DEPTH: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

/// A hold on [`PRINTING`], reentrant within one thread.
///
/// The guard is carried and never read: dropping it is the whole point, and
/// `Drop` on this type is what puts the depth back.
#[cfg(test)]
#[derive(Debug)]
pub(crate) struct Printing(
    #[expect(dead_code, reason = "held to be dropped")] Option<std::sync::MutexGuard<'static, ()>>,
);

#[cfg(test)]
impl Drop for Printing {
    fn drop(&mut self) {
        DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

/// The guard a counter-reading test takes, and every printer with it.
///
/// Poisoning is ignored: a panicking test has already failed, and refusing the
/// lock afterwards would turn one failure into every failure.
#[cfg(test)]
pub(crate) fn printing() -> Printing {
    let held = DEPTH.with(|depth| {
        let was = depth.get();
        depth.set(was + 1);
        was
    });
    Printing(if held == 0 {
        Some(
            PRINTING
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    } else {
        None
    })
}

/// See [`PRINTING`].
#[cfg(test)]
fn while_printing() -> Printing {
    printing()
}

/// Nothing at all, outside a test build.
#[cfg(not(test))]
#[derive(Debug)]
pub(crate) struct Printing;

/// See the test-build [`Printing`]. A real run is single-threaded and pays
/// nothing for this; it is a distinct type rather than `()` so the call sites
/// read the same in both builds and `let_unit_value` has nothing to say.
#[cfg(not(test))]
const fn while_printing() -> Printing {
    Printing
}

/// The number of `warn` lines printed so far.
pub(crate) fn warnings() -> usize {
    WARNINGS.load(core::sync::atomic::Ordering::Relaxed)
}

/// The number of `error` lines printed so far.
pub(crate) fn failures() -> usize {
    FAILURES.load(core::sync::atomic::Ordering::Relaxed)
}

/// Forgets every count, so a test can assert an exact one.
///
/// **Both directions of a rule about a count need the count.** These are
/// process-wide statics and `cargo test` runs in parallel, so a test can
/// otherwise only assert a *difference* — which says what printing one more line
/// does and can say nothing about the case where nothing was printed, and that
/// case is half of what `check::finish`'s rule means. Called while holding
/// [`printing`], which every printer in this module also takes: that is what
/// makes "nothing else has printed since" true rather than hoped for.
#[cfg(test)]
pub(crate) fn forget_counts() {
    FAILURES.store(0, core::sync::atomic::Ordering::Relaxed);
    WARNINGS.store(0, core::sync::atomic::Ordering::Relaxed);
    SKIPS.store(0, core::sync::atomic::Ordering::Relaxed);
    if let Ok(mut announced) = ANNOUNCED.lock() {
        announced.clear();
    }
    if let Ok(mut said) = NOT_APPLICABLE.lock() {
        said.clear();
    }
    FETCHED.store(false, core::sync::atomic::Ordering::Relaxed);
}

/// The second line: a count, a hint, or the command that fixes what was printed.
///
/// **This is where most multi-line text in the tool actually goes.** Every
/// gate's fix line is printed here, and one of them wraps: the workspace-lints
/// diagnostic ends with the two lines a member manifest is missing, written one
/// per line as they are written in the file. Without the message column, the
/// second of them would land at column zero, on the first error a newcomer
/// sees. The three status printers know the message column, and so does this
/// one.
pub(crate) fn detail(text: &str) {
    let _printing = while_printing();
    anstream::println!("        {DIM}{}{DIM:#}", aligned(text));
}

/// Separates two sections. Called by the caller rather than by `heading`, so a
/// run that prints one section prints no leading blank line.
pub(crate) fn gap() {
    anstream::println!();
}

/// Every line of source under `directory` that a run of spaces got into.
///
/// **The shape a lost line-continuation makes.** A diagnostic here is written
/// across several source lines with `\` at each break, and Rust then strips the
/// break *and the next line's indentation*. Drop one backslash and the string
/// keeps both: the sentence comes out as
/// `a case above did not decide              as written`, on one very long
/// source line that rustfmt cannot break and therefore leaves alone. A lost `\`
/// in `policy.rs`'s withheld-count message or `build.rs`'s reservation notice
/// gives exactly that shape.
///
/// Three rules keep this free of an exclusion list rather than needing one: a
/// line holding a `\n` escape is a fixture laying out embedded text, every test
/// module is a fixture too, and a comment line is prose about the code rather
/// than prose the tool prints. What is left is what a developer reads on their
/// terminal.
///
/// **Three, the comment rule among them.** The comment rule is not a small
/// exemption — most of the prose in these files is comments, and each comment
/// line is excused from the rule. A count that includes what a rule skips is a
/// count that cannot say how much the rule reached; the identity beside it
/// needs every line paid for, so a comment line is paid to `skipped`, and the
/// exemption is named here instead of being invisible in both places.
#[cfg(test)]
fn runs_of_spaces_in(directory: &std::path::Path) -> (Vec<String>, usize, usize, usize, usize) {
    let mut found = Vec::new();
    let mut read = 0_usize;
    // Lines actually judged, which is not the same as files opened.
    let mut judged = 0_usize;
    // **And the lines the exemption takes away.** With the split a
    // length-preserving blanking, a `judged` counting every line makes the floor
    // beside it read `N of N` and assert `2N > N` — a floor that cannot fail,
    // over a gate that can be switched off entirely. What the floor exists
    // to notice is an exemption covering most of the tree, so that is what is
    // counted: the lines this scanner is handed blank because a test module
    // stands there.
    let mut exempted = 0_usize;
    // The other exemption: a comment line, or one holding a `\n` escape.
    let mut skipped = 0_usize;
    // **Every depth, not the top of the directory.** With `read_dir` and no
    // recursion, here and in the `whole` it is floored against, a module written
    // as a directory of its own, rather than as a single file beside its
    // siblings, sits outside the scanner *and* outside the count that exists to
    // notice a scanner reading part of the tree. Measured with such a walk and
    // one byte-identical file in both shapes: flat, the test fails; nested, it
    // passes — and `crates/nc-kernel/src/host/` is a module directory of that
    // shape, every file of which such a walk would leave unread.
    // `walk` is what the other gates use and already skips build output.
    let mut files = Vec::new();
    let _ = crate::gates::walk(directory, &mut |path| {
        if path.extension().is_some_and(|kind| kind == "rs") {
            files.push(path.to_path_buf());
        }
    });
    files.sort();
    for path in files {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        read += 1;
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        // **Not a split at one `#[cfg(test)]`.** Several modules put a
        // `#[cfg(test)]` helper high up — `gates.rs` within its first sixty
        // lines — and splitting at the first occurrence exempts everything below
        // it: measured, under half the tree's lines read, with the same plant
        // failing above the split and passing below it. Splitting at the last,
        // `rfind`, reads every test module but the last as source — a second
        // test module in `wall.rs` turns a deliberate alignment run inside a
        // logging fixture into a finding, in text that has not changed. And
        // `#[cfg(test)]` is an attribute, not a module marker — several files put
        // one on a `const` near the top — so splitting below the first
        // *attribute* is wrong too: measured, under half the lines judged, with
        // `gates.rs` splitting at line 60 because of a `#[cfg(test)] const`, and
        // a real lost-`\` defect planted below that line invisible while the
        // byte-identical shape in a file that splits later is caught.
        //
        // **Every test module, not everything below the first one.** A file's
        // source is what is not inside a test module, and files interleave —
        // the catalog compiler's messages, the canon crate's root and this
        // crate's members module each hold shipped text below their first test
        // module. Measured: a `\` dropped from a shipped remedy in the first and
        // an eighteen-space run inside a shipped message in the second are both
        // invisible to a split, while the byte-identical shape planted above
        // another file's split fires at once.
        //
        // `crate::gates::the_test_modules` already computes the ranges, and
        // answers `None` for a file whose braces it cannot close — which is a
        // file this must not be given a wrong answer about, so it falls back to
        // a plain split there rather than judging a fixture as source.
        let ranges = crate::gates::the_test_modules(&text);
        let kept: String = match &ranges {
            Some(ranges) => {
                let mut kept = String::with_capacity(text.len());
                let mut from = 0_usize;
                for range in ranges {
                    kept.push_str(&text[from..range.start]);
                    // The lines a module spans are replaced by blank ones, so a
                    // finding's line number is still the file's own.
                    exempted += text[range.clone()].lines().count();
                    kept.extend(
                        text[range.clone()]
                            .chars()
                            .map(|character| if character == '\n' { '\n' } else { ' ' }),
                    );
                    from = range.end;
                }
                kept.push_str(&text[from..]);
                kept
            }
            None => text
                .match_indices("#[cfg(test)]")
                .find(|(at, _)| {
                    text[at + "#[cfg(test)]".len()..]
                        .trim_start()
                        .starts_with("mod ")
                })
                .map_or(text.clone(), |(at, _)| text[..at].to_owned()),
        };
        let above = kept.as_str();
        // **Counted per line at the verdict, not in bulk above the loop.** A
        // single `judged += above.lines().count()` outside every branch lets
        // one `if index > 0 { continue; }` inside leave `read`, `judged` and
        // `whole` exactly where they are — measured, a real lost-`\` diagnostic
        // planted in `dev.rs` goes unreported with `check passed` and one line
        // judged per file.
        for (index, line) in above.lines().enumerate() {
            // **The one way out of this loop, and it is a named function.**
            // *Any class at all* of line can be exempted without moving any of
            // the four floors below: a filter written here as
            // `judged += 1; continue;` pays the identity on its way past and
            // moves nothing else. Measured with the class `line.starts_with(' ')`
            // — which is every diagnostic written inside a function, so nearly
            // the whole tree — a real lost-`\` defect in `nc-log`'s `ship.rs` on
            // disk and no fixture in that shape: `check passed`.
            //
            // Two things answer it. `judged` is paid by the *scan*, at the
            // last statement a verdict reaches, so a `continue` inserted above
            // it leaves `judged + skipped` short of `whole`; and the classes
            // this passes over are [`passed_over`], one function with them all
            // in it, so a fourth is an edit to a named list rather than a line
            // in a loop. Neither makes a filter that pays every counter
            // impossible — a number can always be written by hand — which is
            // why `the_scanner_finds_a_run_of_spaces_wherever_it_sits` plants
            // the defect in the shape it actually takes, indented inside a
            // function, and that fixture is what closes this particular class.
            if passed_over(line) {
                // **An exemption, so the floor counts it.** This rule has three
                // — a test module, a comment line, and a line holding a `\n`
                // escape — and with only the first paying `exempted`, the floor
                // on that count sees the test modules alone while a large part
                // of the exemption is invisible to it. Measured: the count well
                // short of the actual, and widening the comment test to any line
                // *containing* `//` — the exact misreading this tree records
                // three times — turns the gate off over that ground with every
                // number at its baseline.
                skipped += 1;
                continue;
            }
            // A run of six or more, whatever its length — `windows(8)` alone
            // matches a run of exactly six and slides straight past a longer
            // one, which is the shape of both real cases.
            let bytes = line.as_bytes();
            let mut holds_a_run = false;
            let mut at = 0_usize;
            while at < bytes.len() {
                if bytes[at] != b' ' {
                    at += 1;
                    continue;
                }
                let from = at;
                while at < bytes.len() && bytes[at] == b' ' {
                    at += 1;
                }
                let before = from.checked_sub(1).map(|index| bytes[index]);
                let after = bytes.get(at).copied();
                if at - from >= 6
                    && before
                        .is_some_and(|byte| byte.is_ascii_alphanumeric() || b",.;:".contains(&byte))
                    && after.is_some_and(|byte| byte.is_ascii_alphabetic())
                {
                    holds_a_run = true;
                    break;
                }
            }
            if holds_a_run {
                found.push(format!("{name}:{}", index + 1));
            }
            // **The last statement a verdict reaches, not the first of the
            // body.** Written above the scan, it moves for every line the loop
            // *begins*, so a `continue` inserted between it and the judgement
            // leaves the identity below satisfied over ground nothing has read.
            // This repository's own canary notes state the rule, and two gates
            // in `limits.rs` follow it.
            judged += 1;
        }
    }
    (found, read, judged, exempted, skipped)
}

/// The classes of source line this gate passes over, and all of them.
///
/// **One function, because a class added to a loop is invisible and a class
/// added here is an edit somebody reads.** The four floors over this gate cannot
/// see a filter that increments the counter as it leaves, so what they are held
/// against is a list with a name rather than a shape in the middle of a walk.
///
/// A comment line, read as one pair of slashes at the *start* of the trimmed
/// line: a rule that read any line *containing* two would pass over a diagnostic
/// naming a URL scheme, which is the misreading this tree records three times.
/// And a line holding a `\n` escape, where the break is deliberate and the
/// spacing after it is the sentence's own.
///
/// **What that last class costs, measured rather than assumed.** It exempts the
/// *line*, and the justification is about a *run*, so a lost-`\` run anywhere on
/// a line that also holds a `\n` is invisible: **hundreds of this tree's source
/// lines are passed over by this clause alone**, and a real defect planted in
/// one of them — `crates/nc-log/src/ship.rs`, a run of spaces put mid-sentence
/// on a line ending `\n"` — leaves `cargo xtask check` at `check passed`,
/// exit 0, with **zero** warn lines, while the identical defect one line
/// higher, on a line with no escape, is named at once.
///
/// **Narrowing it to the run after the escape was measured and is wrong**, which
/// is why the clause stands as it is: spacing inside a `\n`-carrying string is
/// the sentence's own wherever it sits, not only where the escape put it. Six
/// shipped lines are that shape — `main.rs`'s help table aligns its verbs with
/// runs of spaces, `emit.rs` indents the Rust it emits, `check.rs` writes a
/// five-verb block — and every one of them is correct. A rule that reports them
/// is the refusal-of-something-legitimate direction this tree calls the worse
/// failure, so what is written here is the limit of what this scanner can decide
/// and the number it costs, rather than a cleverer rule that is wrong six times.
///
/// The class is pinned by `the_scanner_passes_over_a_line_that_carries_a_break`
/// rather than by nobody: a fourth class added here, or this one widened to any
/// escape, is a failing test.
#[cfg(test)]
fn passed_over(line: &str) -> bool {
    line.trim_start().starts_with("//") || line.contains("\\n")
}

#[cfg(test)]
mod tests {
    /// A line carrying a break is passed over, and this is what that costs.
    ///
    /// **The hatch's own test.** Its three classes are a test module, a comment
    /// line and a line holding a `\n` escape, and the four fixtures beside this
    /// plant none of the third — so this is the test that fails if the clause is
    /// widened to any escape, or to any line at all. Measured: a real lost-`\`
    /// run planted in `crates/nc-log/src/ship.rs` on a line that also holds a
    /// `\n` leaves `cargo xtask check` at `check passed`, exit 0, with **zero**
    /// warn lines; the identical run one line higher, on a line with no escape,
    /// is named at once. **Hundreds** of this tree's source lines are passed
    /// over by that clause alone.
    ///
    /// So the class is written down here rather than left to a reader: what it
    /// exempts, what it must not exempt, and the fact that the exemption is the
    /// *line*. `passed_over`'s own doc carries why narrowing it to the run after
    /// the escape is wrong — six shipped lines align a help table or indent
    /// emitted Rust, and reporting them is the direction this tree calls worse.
    #[test]
    fn the_scanner_passes_over_a_line_that_carries_a_break() {
        // The class, in the shape it takes on disk: a message wrapped by hand,
        // with the break written as an escape.
        assert!(super::passed_over(
            "        \"nc-log: the store did not confirm the line\\n\","
        ));
        // A comment, which is the second class.
        assert!(super::passed_over("    // a note about six       spaces"));
        assert!(super::passed_over("/// a doc line"));
        // And what it must *not* pass over: an ordinary diagnostic line with no
        // escape and no comment, which is the whole subject of the scan.
        assert!(!super::passed_over(
            "        \"nc-log: the store did not confirm the line\","
        ));
        assert!(!super::passed_over("    ui::failure(\"a sentence\");"));
        // An escape that is not a line break is not a break: widening the clause
        // to any backslash would pass over most of this tree.
        assert!(!super::passed_over("    let quoted = \"a \\\"word\\\"\";"));
        assert!(!super::passed_over("    let tabbed = \"a\\tword\";"));
    }

    /// The scanner finds a run of spaces wherever in a file it sits.
    ///
    /// **That it finds one at all.** Deleting the rule — `holds_a_run = true`
    /// changed to `false` — passes a gate that asserts only `found.is_empty()`,
    /// and fails this test. And a counter that is a bulk increment
    /// above the loop moves no number for a filter inside it either: a real
    /// lost-`\` diagnostic planted in `dev.rs` goes unreported with
    /// `check passed` and one line judged per file.
    ///
    /// Planted past the longest file the suite reads, for the reason
    /// `xtask/canaries/README.md` gives: a window a filter's author can read out
    /// of the test is not an unpredictable position.
    #[test]
    fn the_scanner_finds_a_run_of_spaces_wherever_it_sits() {
        use core::fmt::Write as _;
        let longest = crate::gates::longest_file_read(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/.."
        )));
        let depth = longest + 1 + std::process::id() as usize % 100;
        assert!(
            depth > longest && longest > 100,
            "the fixture is at line {depth} and the longest file this suite reads is \
             {longest}: a filter tuned above the fixture would be invisible to it"
        );
        let scratch = std::env::temp_dir().join(format!("nc-runs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).expect("writable");
        let mut text = String::new();
        for at in 1..depth {
            let _ = writeln!(text, "// filler line {at}");
        }
        // The real shape: a line break inside a string literal whose `\` was
        // eaten, leaving the next source line's indentation inside the sentence.
        text.push_str(
            "const SAID: &str = \"a sentence that lost its               \
                       continuation\";\n",
        );
        // **And the same defect in a sentence that holds two slashes.** The rule
        // that passes over a comment reads one — `trim_start().starts_with`. Read
        // as any line *containing* two slashes, which is the misreading this tree
        // records three times over, a diagnostic naming a URL scheme is passed
        // over: measured, a run of spaces planted in `nc-log`'s
        // `is not an http:// endpoint` passes such a rule with nothing said.
        text.push_str(
            "const URL: &str = \"this is not an http:// endpoint, so the store               \
                       is elsewhere\";\n",
        );
        // **A run after a comma, and a run of exactly six.** The predicate reads
        // a run of six or more that follows a letter, a digit or one of `,.;:` —
        // and both fixtures above plant fifteen spaces after a letter, so with
        // them alone the punctuation alternative and everything between six and
        // fourteen are decided by nothing. A comma is where this codebase wraps
        // a sentence most often, and six is where the rule says it begins.
        text.push_str(
            "const AFTER: &str = \"a sentence that wrapped after a comma,      \
                       and lost the mark\";\n",
        );
        // **And the same defect indented, which is where real ones are.** A
        // diagnostic is written inside a function, so its source line begins
        // with spaces — and all three fixtures above start at column zero.
        // Exempting exactly that class, `line.starts_with(' ')` with the counter
        // paid on the way out, moves none of the four numbers below: the gate
        // then reads nothing but column zero, and with a real lost-`\` in
        // `nc-log`'s `ship.rs` on disk this fixture is what fails. The numbers
        // cannot see a filter that pays every counter it passes; a fixture in
        // the shape the defect actually takes can.
        text.push_str("fn said() {\n");
        text.push_str(
            "    let _ = \"a sentence indented inside a function that lost its               \
                       continuation\";\n",
        );
        text.push_str("}\n");
        std::fs::write(scratch.join("planted.rs"), &text).expect("writable");

        let (found, files, judged, _, _) = super::runs_of_spaces_in(&scratch);
        assert_eq!(files, 1);
        assert!(
            found.iter().any(|said| said.contains(&depth.to_string())),
            "the scanner did not reach a run of spaces on line {depth} of {judged} \
             judged: a filter between the counter and the judgement leaves every count \
             where it was: {found:?}"
        );
        // **And the one whose sentence holds two slashes.** The rule that passes
        // over a comment reads one; read as any line *containing* two slashes it
        // passes over a diagnostic naming a URL scheme, and this fixture is two
        // lines below the one above.
        let slashed = depth + 1;
        // The third fixture, two lines below the second.
        let after_a_comma = depth + 2;
        assert!(
            found.iter().any(|said| said.contains(&slashed.to_string())),
            "a diagnostic whose own words hold `//` is not a comment, and the run of \
             spaces on line {slashed} was passed over: {found:?}"
        );
        assert!(
            found
                .iter()
                .any(|said| said.contains(&after_a_comma.to_string())),
            "a run of exactly six spaces after a comma is where this codebase wraps most \
             often, and line {after_a_comma} was passed over: {found:?}"
        );
        // The fourth fixture: `fn said() {` is one line below the third, and the
        // indented sentence is one below that.
        let indented = depth + 4;
        assert!(
            found
                .iter()
                .any(|said| said.contains(&indented.to_string())),
            "a diagnostic written inside a function begins with spaces, which is where \
             every real one of these has been, and the run on line {indented} was passed \
             over: {found:?}"
        );
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// No diagnostic in this tool carries a run of spaces.
    ///
    /// **A doc block that merges with its neighbour documents the wrong item.**
    /// Two doc comments standing with no blank line and no item between them
    /// make rustdoc attach the whole of both to whichever item comes next —
    /// leaving this test undocumented and putting a paragraph about a walk it
    /// does not do on another test's page; three blocks glued into one cut two
    /// sentences in half at the join and leave two items rendering somebody
    /// else's documentation.
    ///
    /// **And a ratchet paid to the merged state records the defect.** A
    /// `DOCUMENTED` row written while blocks are merged — one block short and one
    /// item undocumented — turns the suite red when the documentation is written
    /// back. A count that moves with the defect records it; the shape the test's
    /// own message names is exactly this one, and the answer is to read the file
    /// rather than the number.
    ///
    /// **Two messages, one slip.** A `\` lost at a line break of `policy.rs`'s
    /// withheld-count message or `build.rs`'s reservation notice leaves Rust
    /// keeping the break's indentation inside the sentence:
    ///
    /// ```text
    /// the denial-test count below is not reported: a case above did not decide
    ///               as written, and this count is only worth what those cases are worth
    /// ```
    ///
    /// The source line is far past `max_width` and rustfmt leaves it, because it
    /// cannot break a string literal, and clippy does not read inside one.
    ///
    /// Walks `xtask/src`, every crate's `src` and every plugin's `src`, and
    /// counts the files it read — a walk that reached nothing would otherwise
    /// report a clean tree.
    ///
    /// **It looks for a run on one source line, and that is the whole shape.** A
    /// `\` genuinely dropped from a Rust continuation leaves a *newline* plus
    /// the next line's indentation inside the string, and `aligned` re-indents
    /// every continuation to the message column — so that spelling of the slip
    /// renders correctly and is not a finding. What produces the real cases is
    /// an editing tool eating the backslash and joining the lines, which leaves
    /// the run on one line, past `max_width`, where rustfmt cannot break a
    /// string literal and therefore leaves it alone.
    #[test]
    fn no_diagnostic_carries_a_run_of_spaces() {
        /// How many source files this gate reads.
        ///
        /// Written down because the two counts it compares are two walks of one
        /// directory list, so a filter on that list moves both and neither
        /// notices.
        const THE_SOURCE_FILES: usize = 149;
        /// The lines this gate actually reads: the tree, less both exemptions.
        ///
        /// **A pin, not a ratchet.** It stands at the ground exactly, so it
        /// moves in *both* directions and every move is paid in the edit that
        /// made it: one line of judged Rust added raises it by one, one deleted
        /// lowers it by one, and a class of lines newly exempted — the thing the
        /// two proportion floors above cannot notice — lowers it by however many
        /// that class holds.
        ///
        /// **The measurement itself, with nothing under it.** Standing even four
        /// lines below the real ground makes an exemption of up to four lines
        /// free — and a seventeen-line block deleted while repairing something
        /// else hits the floor for a reason that has nothing to do with spacing,
        /// which reads as noise and invites lowering the number to make it go
        /// away. That is why the two ambiguous directions are the ones the
        /// failure spells out: source added is ordinary work and the number goes
        /// up with it, but source *deleted* and a class *exempted* look
        /// identical from here, and only one of them is something to accept.
        const THE_GROUND_THIS_READS: usize = 84_698;

        let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        let mut directories = vec![root.join("xtask/src")];
        // **Every plugin's `src` as well as every crate's.** A plugin's
        // diagnostics are written the way a crate's are, and a walk of
        // `crates/` alone misses them: measured, a lost `\` joining two lines of
        // `plugins/reference/src/behaviour.rs` leaves such a walk and the whole
        // gate green, while the same plant in `nc-kernel` fails it.
        // **Every tree of members, as `MEMBER_TREES` names them**, and one not
        // on disk skipped rather than a panic: a tree whose first member has
        // not arrived has no source to read.
        for tree in crate::members::trees_of_members() {
            let Ok(entries) = std::fs::read_dir(root.join(tree)) else {
                continue;
            };
            let mut members: Vec<std::path::PathBuf> = entries
                .flatten()
                .map(|entry| entry.path().join("src"))
                .filter(|path| path.is_dir())
                .collect();
            members.sort();
            directories.append(&mut members);
        }

        let mut found = Vec::new();
        let mut read = 0_usize;
        let mut judged = 0_usize;
        let mut exempted = 0_usize;
        let mut skipped = 0_usize;
        for directory in &directories {
            let (theirs, files, lines, blanked, passed_over) = super::runs_of_spaces_in(directory);
            found.extend(theirs);
            read += files;
            judged += lines;
            exempted += blanked;
            skipped += passed_over;
        }
        // **Lines, not files.** Counting files opened, a split that exempts
        // half of every one of them leaves a floor exactly where it is —
        // measured, every file read and under half of their lines judged, with
        // nothing saying so. The comparison is against the same tree, counted a
        // second time.
        //
        // **And the second count is not independent of the walk.** Both halves
        // call `crate::gates::walk`, so a defect in the walk moves both numbers
        // together and the floor cannot see it. What the comparison does catch
        // is the thing it is for: a *split* that exempts most of every file,
        // because only one side applies one. Said plainly rather than
        // overstated: a floor that claims more independence than it has is read
        // as covering a case it does not.
        // **Recursing, because the scanner does.** One directory deep, a nested
        // module directory would be outside the walk and outside the number that
        // exists to say the walk is short. A floor over less ground than the
        // thing it floors is slack, not a measurement.
        let whole: usize = directories
            .iter()
            .map(|directory| {
                let mut lines = 0_usize;
                let _ = super::super::gates::walk(directory, &mut |path| {
                    if path.extension().is_some_and(|kind| kind == "rs")
                        && let Ok(text) = std::fs::read_to_string(path)
                    {
                        lines += text.lines().count();
                    }
                });
                lines
            })
            .sum();
        // **The number, not a floor far below it.** `judged` and `whole` are two
        // walks of one `directories` value, so a filter on that list moves both
        // — and a floor such as `read > 20` leaves enough slack to drop several
        // files: measured, excluding `nc-log` takes its files out of `whole` and
        // under such a floor every assertion here still passes, with a real
        // lost-`\` defect in one of them. The list is the input to both halves,
        // which is a wider version of the thing the paragraph below concedes
        // about `walk`.
        assert_eq!(
            read, THE_SOURCE_FILES,
            "this gate read {read} source file(s) and the tree holds \
             {THE_SOURCE_FILES}. Adding a crate moves this number; a filter on the \
             directory list moves it too, and that is what it is here for"
        );
        // **The exemption, not the count of lines handed over.** With the split a
        // length-preserving blanking, every line is handed over — `N of N` — so
        // `judged * 2 > whole` reads `2N > N` and cannot fail: every file with a
        // test module blanked from end to end, with a real lost-`\` defect on
        // disk, then gives `check passed` with every test green.
        //
        // What the floor is written to notice is an exemption that covers most of
        // the tree, so that is what it counts. `judged` is still compared,
        // because a walk that reads nothing must not pass either.
        // **Each exemption on its own, because one number over three of them is
        // not a floor.** Counting only the test modules, while the comment rule
        // skips a large share more, leaves much of what the gate passes over
        // invisible to the number written to notice a blanket. Summed they are
        // most of the tree, which is the honest figure and is what a repository
        // whose test modules are most of its source looks like; what must not
        // happen is either one *becoming* the blanket.
        // **And half is not the number this argues for.** The paragraph above
        // concedes that the honest summed figure is most of the tree and that
        // "what must not happen is either one *becoming* the blanket" — and a
        // `* 2` fires the day the test modules pass half: measured, it fires at
        // 50.05% with a gate that has lost nothing at all. A
        // threshold nobody argued for, tripped by the repository doing what it
        // is asked to do, teaches the next author to lower a floor rather than
        // to look at one.
        //
        // Three quarters is what the sentence means by a blanket, and it is not
        // what stops one on its own: `THE_GROUND_THIS_READS` below is absolute,
        // and it is a **pin rather than a floor** — it stands at the ground
        // exactly, so adding or deleting one judged line of Rust anywhere in
        // this tree turns this test red and the number moves in the same edit.
        // A `>=` would be a floor: paid by nobody when source is *added*, so the
        // margin grows with ordinary work and only a deletion is ever charged —
        // measured, such margins reach 137 and 261 lines, every one of them free
        // exemption. An exact pin moves with ordinary work, the way every other
        // frozen census in this repository moves — paid, in the edit that moved
        // it, by somebody who looked. A margin would be paid by nobody.
        //
        // It stands at the ground itself rather than below it — the number is
        // in the constant and is not restated here, because a number restated
        // in prose is a second number for one quantity, and the prose is the
        // one that goes wrong.
        assert!(
            exempted * 4 < whole * 3,
            "{exempted} of {whole} line(s) were exempted as tests: an exemption over most \
             of every file leaves the file count exactly where it was, which is how this \
             gate went quiet over half the tree once already"
        );
        assert!(
            skipped * 2 < whole,
            "{skipped} of {whole} line(s) were passed over as comments or as lines holding \
             an escape: that rule reads a comment, and a rule that read any line *holding* \
             two slashes would pass over a diagnostic with a `//` inside its own words"
        );
        // **A fall here is read before it is paid.** The ordinary cause is the
        // one this paragraph names second: more *comment* lines. A comment is
        // skipped, so prose added on purpose takes lines out of what is judged
        // with no exemption list touched and both floors above green.
        //
        // **And the sum, which neither of the two above can see.** One permits
        // just under three quarters and the other just under half, so between
        // them every line could be exempt with both green — and they already
        // carry most of the tree between them. What is pinned here is the
        // ground this gate actually reads: widening an exemption by a class —
        // the wrapped-literal continuations, say, which are thousands of lines
        // and exactly where a lost `\` lives — moves it, and so does every other
        // edit to the judged tree.
        // Why it moved belongs to the commit that moved it.
        assert!(
            whole - exempted - skipped == THE_GROUND_THIS_READS,
            "this gate judged {} line(s) and read {THE_GROUND_THIS_READS} before: an \
             exemption widened by a class leaves both of the floors above it green, because \
             each is written against half the tree and neither can see the other. This is \
             a pin and it moves both ways, so write the new number in the edit that moved \
             it — but only after reading which way it went: judged Rust added or deleted \
             on purpose is ordinary and the number follows it, while a class of lines \
             newly exempted looks exactly like a deletion from here and is the thing this \
             exists to stop",
            whole - exempted - skipped
        );
        // **`judged` is what the scan reached, and the passed-over lines are
        // counted beside it.** Paid inside the exemption branch as well,
        // `judged == whole` holds for any filter that increments it on the way
        // past — enough to exempt every indented line in the tree with all four
        // numbers at their baseline, measured. Paid at the last statement of the
        // loop instead, a `continue` anywhere above it takes the sum below
        // `whole` unless whoever wrote it also pays `skipped`, and a class broad
        // enough to matter then fails the two floors above.
        // **Through the comparison every census in this tree shares.** An
        // `assert_eq!` written inline against the real tree can be handed no
        // disagreeing pair, and widened to `judged + skipped <= whole` it would
        // leave the whole suite green — which is exactly the weakening that
        // stops noticing the `continue` this paragraph is about.
        // `the_two_counts_disagree` has a fixture that drives under, equal and
        // over, so the weakening is caught there once for every place that asks.
        assert!(
            !crate::check::the_two_counts_disagree(judged + skipped, whole),
            "the scanner is handed every line, with the ones inside a test module blanked \
             rather than cut, and every line it does not scan is passed over for a written \
             reason — so {judged} scanned plus {skipped} passed over is how many lines \
             exist, and a difference means the walk stopped somewhere"
        );
        assert!(
            found.is_empty(),
            "a line break inside a string literal lost its `\\`, so the indentation of \
             the next source line is inside the sentence: {found:?}"
        );
    }

    /// A skip is counted for the reason it *begins* with.
    ///
    /// **`skips_because` is what makes `--offline`'s allowance an equality**,
    /// and this decides how it matches. Measured, `starts_with` widened to
    /// `contains` classifies every line of a clean run as before, so a fixture
    /// is what tells the two apart, and this one fails — after which any skip
    /// whose sentence merely *mentions* the flag would count toward the flag's
    /// own number, and `finish`'s equality would stop separating the two
    /// populations it exists to separate. `warn`'s own paragraph makes the same
    /// argument one function up: a substring test on free prose classifies by
    /// accident.
    ///
    /// Driven through `warn`, because what is being checked is what `warn`
    /// records — and `shortened` strips the `skipped:` prefix before it stores
    /// the sentence, so the reason has to be first in what is left.
    #[test]
    fn a_warning_is_a_skip_only_when_it_begins_with_one() {
        // **The rule acceptance criterion 1 rests on.**
        // `check::finish` computes `unexplained = warnings() - skips()` and
        // fails the run when it is non-zero, and the verdict's `not run:` list
        // is what a reader is pointed at to tell a full run from one that
        // skipped a step. Which warnings are skips is this prefix — and
        // `warn`'s own paragraph says why a substring test files a genuine
        // warning under `not run:`.
        //
        // Measured with `starts_with("skipped:")` widened to
        // `contains("skipped")`: a clean `cargo xtask check --offline` prints
        // the same nine warn lines and the same nine `not run:` lines, so the
        // run itself cannot tell the two rules apart, and this test is what
        // fails. The sibling rule, `skips_because`, has a test of its own.
        //
        // The hold, for the reason the test below gives: these are process-wide
        // counters read as a difference.
        let _printing = super::printing();
        let before = (super::warnings(), super::skips());
        // A genuine warning that describes a defect and happens to use the word:
        // this is the population a substring rule swallows.
        super::warn("the pin was not compared, so the check was skipped for this image");
        assert_eq!(
            (super::warnings(), super::skips()),
            (before.0 + 1, before.1),
            "a warning describing a defect was filed as a step that did not run, so \
             `finish` subtracts it away and the run passes with it on the terminal"
        );
        // And the announcement itself, which is what the prefix is for.
        super::warn("skipped: this is a test of the prefix rule");
        assert_eq!(
            (super::warnings(), super::skips()),
            (before.0 + 2, before.1 + 1),
            "a step announcing that it did not run was not counted as one"
        );
        // **And the word with its colon, somewhere other than the start.** The
        // fixture above writes "skipped" without one, so with it alone, widening
        // the rule from `starts_with` to `contains` — which is what turns a
        // sentence about a defect into an announced skip — leaves
        // `check --offline` byte-identical to a clean run and this test green.
        // This is the fixture that decides it, as the sibling rule's does in the
        // test below.
        super::warn("the design system's tag was skipped: the pin was not compared");
        assert_eq!(
            (super::warnings(), super::skips()),
            (before.0 + 3, before.1 + 1),
            "a sentence holding the word mid-line was filed as a step that did not run, \
             so `finish` subtracts it away and the run passes with it on the terminal"
        );
        // And the colon, which is the whole of what makes the word a
        // declaration rather than a subject: dropped from the rule, a sentence
        // *about* skipped steps becomes one.
        super::warn("skipped steps are listed below, and this line is a defect");
        assert_eq!(
            (super::warnings(), super::skips()),
            (before.0 + 4, before.1 + 1),
            "the word without its colon is a subject, not an announcement"
        );
    }

    #[test]
    fn a_skip_is_counted_for_the_reason_it_begins_with() {
        // **The hold, because this reads a process-wide counter as a
        // difference**, as its sibling in `check.rs` does.
        // Measured without it, with one competing test printing
        // `ui::warn("skipped: --offline, …")` at `--test-threads=4`:
        // `left: 9, right: 1`.
        let _printing = super::printing();
        let before = super::skips_because("--offline");
        super::warn("skipped: --offline, and this is a test of the counter");
        assert_eq!(
            super::skips_because("--offline"),
            before + 1,
            "a skip that names the flag first was not counted for it"
        );
        // The other half: a skip that mentions the flag while announcing
        // something else is a different population, and `contains` cannot tell
        // them apart.
        super::warn(
            "skipped: no database answered, so nothing ran — this is not what --offline \
             skips",
        );
        assert_eq!(
            super::skips_because("--offline"),
            before + 1,
            "a skip that only mentions the flag was counted as one the flag asked for, \
             so the equality in `finish` stops meaning what it says"
        );
        // Left behind, the skip naming the flag is one more than any later
        // test asks `finish` for, and it is refused for that.
        super::forget_counts();
    }

    /// A not-applicable line is recorded by what it says, and is neither a
    /// warning nor a skip.
    #[test]
    fn a_part_not_applicable_is_counted_apart() {
        let _printing = super::printing();
        let (warnings, skips) = (super::warnings(), super::skips());
        let before = super::not_applicable_parts().len();
        super::not_applicable("a probe part, which this system does not run");
        assert_eq!(
            (super::warnings(), super::skips()),
            (warnings, skips),
            "a part that does not apply here was counted as a warning or a skip"
        );
        let said = super::not_applicable_parts();
        assert_eq!(said.len(), before + 1, "the line was not recorded");
        assert_eq!(
            said.last().map(String::as_str),
            Some("a probe part, which this system does not run")
        );
        // **Forgotten before the hold is let go.** Left behind, the probe is a
        // part this system was never written down for, and `check::finish`
        // refuses every later run for it, before it reaches the rule a test
        // beside it is asking about.
        super::forget_counts();
    }

    #[test]
    fn a_message_with_a_line_break_keeps_the_message_column() {
        // **Both of this module's rules, broken by one `\n`.** Unindented, the
        // continuation starts at column zero, where the file says only headings
        // live, and the "one column of prose" the alignment exists for stops
        // after the first line. `database.rs` embeds one on the path a developer
        // hits when the stack is down.
        let aligned = super::aligned("first line\nsecond line\nthird");
        let lines: Vec<&str> = aligned.lines().collect();
        assert_eq!(lines[0], "first line", "the first line moved");
        for line in &lines[1..] {
            assert!(
                line.starts_with("        "),
                "a continuation starts at column {}: {line:?}",
                line.len() - line.trim_start().len()
            );
            assert!(
                !line.trim_start().is_empty(),
                "the indent was added to nothing"
            );
        }
        // The column is the one `ok`, `warn` and `error` put their text at, so
        // the whole run reads as one column.
        assert_eq!(super::MESSAGE, "  error ".len());
        assert_eq!(super::MESSAGE, "  ok    ".len());
        assert_eq!(super::MESSAGE, "  warn  ".len());
        assert_eq!(super::MESSAGE, "  n/a   ".len());

        // An ordinary message is untouched, because a function that rewrote
        // every line would be worse than the defect.
        assert_eq!(super::aligned("one line"), "one line");
        assert_eq!(super::aligned(""), "");
        // An empty continuation is not eight spaces of nothing.
        assert_eq!(super::aligned("a\n\nb"), "a\n\n        b");
        // `str::lines` yields one line per terminator, so three newlines are
        // two empty lines after an empty first — and none of them is indented.
        assert_eq!(super::aligned("\n\n\n"), "\n\n");
        // `\r\n` is handled by `str::lines`, which strips the `\r` — so the
        // continuation does not carry one into the middle of a line.
        assert_eq!(super::aligned("a\r\nb"), "a\n        b");
    }

    /// The file with its string contents and its comments spent.
    ///
    /// Char count is preserved, so every offset taken from it means the same
    /// place — the coordinate rule whose breach leaves nearly every body wrong.
    ///
    /// **[`crate::gates::blanked`], because a block comment is a comment.**
    /// Composing `without_string_contents` — which hands a `/* … */` back
    /// verbatim, by design, because its callers are supposed to strip comments
    /// first — with a per-line cut that knows only `//` lets a `}` inside a
    /// block comment close a function body here. Measured, two lines added
    /// inside an existing test in `database.rs`:
    ///
    /// ```text
    /// /* } */
    /// assert!(crate::ui::warnings() < usize::MAX, "a probe");
    /// ```
    ///
    /// — without the comment the rule fires
    /// (`1 test(s) read a process-wide counter as a difference without taking
    /// the hold`), with it the test passes and `check passed`, every number
    /// identical to a clean run. The floor beside it does not see it either:
    /// the truncated body still ends at a `}` — the comment's — so
    /// `bodies == closed` holds.
    ///
    /// `blanked` spends both, character for character, which is also the only
    /// stripper that keeps the char count this doc claims: the raw-string
    /// closer `"#` comes back from `without_string_contents` as one character.
    fn readable(text: &str) -> String {
        crate::gates::blanked(text)
    }

    /// Where the next indented `fn` declaration's name begins, at or after
    /// `from`.
    ///
    /// **A needle of `fn ` alone** sees a declaration written that way at four
    /// spaces and misses the rest of `xtask/src`'s: `async fn`, `const fn`,
    /// `pub(crate) fn` at four spaces and `fn` at eight. Measured, the
    /// identical probe reading a process-wide counter twice with no hold —
    /// written `#[test] fn` it is named, written `#[tokio::test] async fn` it is
    /// invisible, and `database.rs` has `async fn` helpers at that indent. The
    /// `bodies` floor cannot see the gap either: a declaration the walk never
    /// finds produces no body on either side of `bodies == closed`.
    ///
    /// **And five more spellings leave a gap.** A list holding `fn`,
    /// `async fn`, `const fn`, `pub fn` and `pub(crate) fn` at four spaces
    /// cannot express an indentation at all, so `fn` at eight stays outside
    /// it. A rule that misses a declaration attributes its body to the one
    /// before it, which is the direction that switches the rule off.
    ///
    /// **The size of that gap, over the population this walk actually reads.**
    /// A count over the whole workspace is a bound on a different population:
    /// the caller walks `xtask/src` and nothing else, and `pub const fn`, a
    /// spelling a five-spelling list misses, is declared nowhere in this
    /// directory. Quoting one population's number about another is the shape
    /// the four questions name.
    /// What holds the reach is not a number in this paragraph but
    /// [`the_declarations_a_line_scan_finds`], a second and simpler reader of
    /// the same files, compared against the walk on every run.
    ///
    /// So this walks the grammar instead of listing its spellings: a line with
    /// any indentation at all, an optional visibility however Rust writes one,
    /// then `default`, `const`, `async`, `unsafe` and `extern "…"` in any
    /// order, then `fn`. A spelling cannot be missing from a rule that names no
    /// spellings, and the visibility is stripped by the one function
    /// `crate::gates` owns rather than by a sixth copy of that rule.
    fn a_method_opens_at(code: &str, from: usize) -> Option<usize> {
        let mut at = from;
        while let Some(newline) = code[at..].find('\n') {
            let line = at + newline + 1;
            at = line;
            let rest = code.get(line..)?;
            let body = rest.trim_start_matches(' ');
            // Indented, which is what makes it a method rather than a free
            // function: the rule this serves is about test and helper bodies
            // inside a module, and a file's top-level items are read by the
            // census one gate along.
            if core::ptr::eq(body, rest) {
                continue;
            }
            let mut head = crate::gates::without_a_visibility(body);
            loop {
                let shorter = ["default ", "const ", "async ", "unsafe "]
                    .into_iter()
                    .find_map(|modifier| head.strip_prefix(modifier))
                    .or_else(|| {
                        head.strip_prefix("extern \"")
                            .and_then(|rest| rest.split_once("\" "))
                            .map(|(_, rest)| rest)
                    });
                match shorter {
                    Some(rest) => head = rest,
                    None => break,
                }
            }
            if let Some(named) = head.strip_prefix("fn ") {
                return Some(code.len() - named.len());
            }
        }
        None
    }

    /// How many indented declaration heads a plain line scan finds, over the
    /// files the rule reads.
    ///
    /// Written as a second, simpler reader so the walk is compared against
    /// something rather than against a number with slack in it. It is a line
    /// scan and nothing more: no brace matching, no bodies, no offsets — which
    /// is what makes it a check on the walk instead of a copy of it.
    ///
    /// It reads the same directory the rule reads. A count over the whole tree
    /// would be a bound on a different population, and this directory's figure
    /// is its own.
    fn the_declarations_a_line_scan_finds() -> usize {
        let mut found = 0_usize;
        let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src"));
        let mut reading = vec![root.to_path_buf()];
        while let Some(here) = reading.pop() {
            let Ok(entries) = std::fs::read_dir(&here) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    reading.push(path);
                    continue;
                }
                if path.extension().is_none_or(|kind| kind != "rs") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                for line in crate::gates::blanked(&text).lines() {
                    let body = line.trim_start_matches(' ');
                    if core::ptr::eq(body, line) {
                        continue;
                    }
                    let mut head = crate::gates::without_a_visibility(body);
                    loop {
                        let shorter = ["default ", "const ", "async ", "unsafe "]
                            .into_iter()
                            .find_map(|modifier| head.strip_prefix(modifier));
                        match shorter {
                            Some(rest) => head = rest,
                            None => break,
                        }
                    }
                    if head.strip_prefix("fn ").is_some_and(|rest| {
                        rest.starts_with(|c: char| c.is_alphabetic() || c == '_')
                    }) {
                        found += 1;
                    }
                }
            }
        }
        found
    }

    /// A method head is read however Rust writes it.
    ///
    /// A list with one use and no membership test holds what nothing decides:
    /// five spellings at one indentation leave every other indented declaration
    /// outside them. Each row below is a shape counted in this tree, written out
    /// here rather than derived from it: a fixture taken from the thing it
    /// measures disappears with the entry it was meant to hold.
    ///
    /// The refusals matter as much: a rule that reads `effning` or a trait's
    /// declaration as a body start attributes the wrong text to the wrong
    /// item, which is the same defect the other way round.
    #[test]
    fn a_method_head_is_read_however_rust_writes_it() {
        for (how, line) in [
            ("a plain method", "    fn probe() {"),
            ("eight spaces in", "        fn probe() {"),
            ("twelve spaces in", "            fn probe() {"),
            ("an async test", "    async fn probe() {"),
            ("a const fn", "    const fn probe() {"),
            ("a public const fn", "    pub const fn probe() {"),
            ("a public async fn", "    pub async fn probe() {"),
            ("a crate-visible one", "    pub(crate) fn probe() {"),
            (
                "a crate-visible async one",
                "    pub(crate) async fn probe() {",
            ),
            ("a super-visible one", "        pub(super) fn probe() {"),
            (
                "a super-visible const fn",
                "        pub(super) const fn probe() {",
            ),
            (
                "one visible in a path",
                "    pub(in crate::ui) fn probe() {",
            ),
            ("an unsafe one", "    unsafe fn probe() {"),
            ("a public unsafe one", "    pub unsafe fn probe() {"),
            ("an extern one", "    pub extern \"C\" fn probe() {"),
        ] {
            let code = format!("mod holding {{\n{line}\n    }}\n}}\n");
            let at = a_method_opens_at(&code, 0)
                .unwrap_or_else(|| panic!("`{how}` is a method head this walk cannot see"));
            assert!(
                code[at..].starts_with("probe("),
                "`{how}` was read as opening at {:?} rather than at its name",
                &code[at..at + 8.min(code.len() - at)]
            );
        }
        for (how, line) in [
            ("a free function", "fn probe() {"),
            ("a word ending in fn", "    let effn = 1;"),
            ("a trait's own declaration", "    trait Fnlike {"),
            ("a field named for one", "    the_fn: usize,"),
        ] {
            let code = format!("mod holding {{\n{line}\n    }}\n}}\n");
            assert!(
                a_method_opens_at(&code, 0).is_none(),
                "`{how}` was read as a method head, so a body starts at text that \
                 declares none and the text below it is attributed to it"
            );
        }
    }

    /// Nothing a function body may hold closes it early, and every offset means
    /// the same place.
    ///
    /// **A block comment is a comment, and [`readable`] spends it.** Composing
    /// `without_string_contents` — which hands `/* … */` back verbatim, by
    /// design, because its callers are meant to strip comments first — with a
    /// per-line cut that knows only `//` would let a `}` inside a block comment
    /// end a body here. Measured with that composition, two lines added to an
    /// existing test in `database.rs`:
    ///
    /// ```text
    /// /* } */
    /// assert!(crate::ui::warnings() < usize::MAX, "a probe");
    /// ```
    ///
    /// — without them the rule fires, with them the test passes and
    /// `check passed`, every number identical to a clean run.
    /// `the_extraction_still_works` is no help: the truncated body still ends
    /// at a `}`, so `bodies == closed`.
    ///
    /// And the coordinate rule this file's own doc claims: a stripper returning
    /// the raw-string closer `"#` as one character makes an offset taken from it
    /// point two places left in the original.
    #[test]
    fn nothing_a_body_holds_closes_it_early() {
        for (how, source) in [
            (
                "a block comment",
                "fn f() {\n    /* } */\n    let x = 1;\n}\n",
            ),
            ("a line comment", "fn f() {\n    // }\n    let x = 1;\n}\n"),
            (
                "a literal",
                "fn f() {\n    let s = \"}\";\n    let x = 1;\n}\n",
            ),
            (
                "a literal spanning lines",
                "fn f() {\n    let s = \"one\n}\";\n    let x = 1;\n}\n",
            ),
            (
                "a raw literal",
                "fn f() {\n    let s = r#\"}\"#;\n    let x = 1;\n}\n",
            ),
        ] {
            let spent = readable(source);
            assert_eq!(
                spent.matches('}').count(),
                1,
                "`{how}` left a brace in the readable copy, so a body ends at it and the \
                 rest of the function is read as somebody else's"
            );
            assert_eq!(
                spent.chars().count(),
                source.chars().count(),
                "`{how}` changed the char count, and every offset this scan takes from \
                 the readable copy is used to slice the original"
            );
            assert_eq!(
                spent.lines().count(),
                source.lines().count(),
                "`{how}` changed the line count"
            );
        }
    }

    /// The extraction the rule above is entirely built out of, floored.
    ///
    /// **The bodies are floored, not only the files.** `unheld` is built
    /// from function bodies, so an extraction returning nothing at all leaves
    /// `unheld.is_empty()` true with every file read — the vacuous assertion
    /// `NOCTUA.md §19.5` names, one layer below the floor written for it. The
    /// extraction is also what can break silently: measured, misaligned offsets
    /// leave nearly every body it produces wrong.
    ///
    /// So both numbers: how many bodies came out, and how many ended at a `}`. A
    /// body that does not is a body the brace walk lost.
    fn the_extraction_still_works(bodies: usize, closed: usize, read: usize) {
        // **A second count of the same ground, not a floor with slack in it.** A
        // floor of `bodies > 300` alone, under a walk finding several times
        // that, lets hundreds of declarations stop being read with nothing said
        // — and narrowing the walk to twelve spaces of indentation drops some of
        // this directory's declarations with every fixture and such a floor
        // green, measured.
        //
        // So the reach is compared against a count taken a different way: a
        // plain line scan for a declaration head over the same files. It is
        // not the same code as the walk — that is the point — and it is
        // deliberately generous, counting any line whose first word after the
        // modifiers is `fn`, so the walk is held to finding *at least* what a
        // reader sees.
        let declared = crate::ui::tests::the_declarations_a_line_scan_finds();
        assert!(
            bodies >= declared,
            "the walk extracted {bodies} function bodies from {read} file(s) and a plain \
             line scan of the same files finds {declared} declarations — a declaration \
             the walk never reaches has its body attributed to the one before it, and \
             every finding this rule can make is read out of a body"
        );
        assert!(
            bodies > 300,
            "{bodies} function bodies were extracted from {read} file(s), which is not \
             this crate — and every finding this rule can make is read out of one"
        );
        assert_eq!(
            bodies,
            closed,
            "{} of {bodies} extracted bodies do not end at their function's closing \
             brace, so the brace walk has drifted and each of those bodies is some other \
             function's text",
            bodies - closed
        );
    }

    /// And every printer goes through it.
    ///
    /// **A source assertion, and it says so.** What these three functions do is
    /// write to stdout and stderr, and capturing those from inside the process
    /// that owns them is a bigger apparatus than the rule deserves. Taking
    /// `aligned` off any one call site leaves the test above green, because that
    /// one drives the function and not the printers.
    #[test]
    fn every_printer_aligns_what_it_is_given() {
        let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ui.rs"))
            .expect("this file is readable");
        // **The test module, not the first `#[cfg(test)]`.** This module has
        // a `#[cfg(test)]` hold above the printers, so splitting on the
        // attribute cuts the file off before `detail` — and a printer this rule
        // cannot find is a printer it cannot judge, silently, unless the
        // assertion is a `find` that panics rather than `contains` over the
        // whole file.
        let body = source
            .split_once("\nmod tests {")
            .expect("the test module is below")
            .0;
        for printer in [
            "fn ok(",
            "fn failure(",
            "fn warn(",
            "fn detail(",
            "fn not_applicable(",
        ] {
            let from = body.find(printer).unwrap_or_else(|| panic!("{printer}"));
            let ends = body[from..].find("\n}").unwrap_or(body.len() - from);
            assert!(
                body[from..from + ends].contains("aligned(text)")
                    || body[from..from + ends].contains("aligned(&text)"),
                "`{printer}` prints its text without aligning it, so a message with a \
                 line break in it starts its second line at column zero — where this \
                 module says only headings live"
            );
            // **And every printer takes the hold.** The two counters this module
            // keeps are process-wide statics, and a test that reads one as a
            // difference is only reading its own difference while nothing else
            // prints. The hold makes that true; a printer that does not take it
            // is a hole in it, and the hole is invisible — the reading test goes
            // red later, in another module, for a reason nobody put there.
            assert!(
                body[from..from + ends].contains("while_printing()"),
                "`{printer}` prints without taking the hold, so a test reading \
                 `warnings()` or `skips()` as a difference can be interrupted by it. \
                 `the_verdict_counts_the_skips_it_announced` passed for a year only \
                 because libtest happened to start it before `release::`"
            );
        }

        // **And every test that reads one of the counters takes it.** The rule
        // above holds the writers and this holds the readers: a reader such as
        // `a_skip_is_counted_for_the_reason_it_begins_with` without the hold
        // fails at `left: 9, right: 1` with one competing test printing a skip.
        // **Every file, not three named ones.** The printers' half of this rule
        // is a walk, and so is the readers'. A hand list of `ui.rs`, `check.rs`
        // and `release.rs` says nothing about what decides the set: measured
        // with such a list, a `#[test]` in `gates.rs` reading `ui::warnings()`
        // as a difference with no hold leaves every test green and
        // `check passed`, with the flake this rule exists to end reintroduced in
        // a fourth file.
        let mut unheld = Vec::new();
        let mut bodies = 0_usize;
        let mut closed = 0_usize;
        let mut read = 0_usize;
        let directory = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src"));
        let mut sources: Vec<std::path::PathBuf> = Vec::new();
        crate::gates::walk(directory, &mut |path| {
            if path.extension().is_some_and(|kind| kind == "rs") {
                sources.push(path.to_path_buf());
            }
        })
        .expect("the source directory walks");
        sources.sort();
        for path in sources {
            let named = crate::relative_to(directory, &path);
            let text: &str = &std::fs::read_to_string(&path).expect("a source file is readable");
            read += 1;
            // **The body, found by counting braces.** Taking "everything up to
            // the first `\n    }\n`" reads past the end of a function whose own
            // body holds a nested block at that indentation, and then attributes
            // one test's counter read to another — measured, it names
            // `the_documents_agree_about_what_offline_skips`, whose body holds
            // no counter at all.
            // **Every offset comes from one string.** `without_string_contents`
            // replaces each *char* inside a literal with one *byte* of space —
            // char-count-preserving, not byte-count-preserving, and `gates.rs`
            // says so about its own use. Taking `opens` and `brace` from `text`
            // and slicing `code` with them drifts every file — `check.rs` by
            // hundreds of characters — and **nearly every body extracted that way
            // does not end at its function's closing brace**, some of them empty.
            // Measured: the identical probe — a `#[test]` reading
            // `warnings() - skips()` with no hold — is then reported in
            // `database.rs` and invisible in `gates.rs`.
            //
            // So the scan runs on `code` throughout. A declaration and a name
            // are ASCII, and `code` keeps every character outside a literal
            // verbatim, so nothing is lost by never looking at `text` again.
            let code = readable(text);
            // See [`a_method_opens_at`]: with a needle of `fn ` alone the probe
            // written `#[tokio::test] async fn` is invisible, and a list of
            // five spellings cannot express an indentation.
            let mut from = 0_usize;
            while let Some(opens) = a_method_opens_at(&code, from) {
                from = opens;
                let Some(name) = code[opens..].split('(').next() else {
                    continue;
                };
                let Some(brace) = code[opens..].find('{') else {
                    break;
                };
                // **Braces counted over source with its strings blanked and its
                // comments cut.** Counting them raw reads a `{` inside a string
                // literal as a block — these files are full of `{findings:#?}`
                // and of prose holding a lone brace — so one function's body
                // swallows the rest of its module and this rule attributes a
                // counter read to whichever function comes first: hundreds of
                // lines of "body" for a forty-line test. Bounding at the next
                // `\n    fn ` instead has the opposite failure, because the next
                // item may be `pub(crate) fn` or carry an attribute. `gates.rs`
                // already blanks string contents for the same reason one file
                // over.
                let mut depth = 0_i32;
                let mut ends = opens + brace;
                for (offset, character) in code[opens + brace..].char_indices() {
                    match character {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                ends = opens + brace + offset;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                let body = &code[opens + brace..ends];
                bodies += 1;
                if !body.is_empty() && code[ends..].starts_with('}') {
                    closed += 1;
                }
                let reads = READS_A_COUNTER.iter().any(|needle| body.contains(needle));
                if reads && !body.contains("printing()") {
                    let found = format!("{named}:{name}");
                    unheld.push(found);
                }
            }
        }
        assert!(
            read > 15,
            "{read} source file(s) were read, so this is not walking this crate"
        );
        the_extraction_still_works(bodies, closed, read);
        assert!(
            unheld.is_empty(),
            "{} test(s) read a process-wide count or record without taking the hold: \
             {}. Another test printing or forgetting between this one's reads makes what \
             it reads something else",
            unheld.len(),
            unheld.join(", ")
        );
    }

    /// What a body holds when it reads a process-wide count or record, for
    /// [`every_printer_aligns_what_it_is_given`] to hold the readers of.
    ///
    /// **Every record a test reads, the fetch's among them.** A reader of one
    /// left off this list is a test the hold rule cannot see, and what it reads
    /// is whatever another test left there, which can be the very refusal it
    /// asserts.
    const READS_A_COUNTER: [&str; 5] = [
        "warnings()",
        "skips()",
        "skips_because(",
        "not_applicable_parts()",
        "the_fetch_ran()",
    ];
}
